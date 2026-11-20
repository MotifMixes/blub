//! wgpu presentation: the console's picture (AI-upscaled, exact pixels,
//! sharp-bilinear, an LCD grid look or Catmull-Rom smoothed)
//! plus the egui user interface on top. Display only: nothing here changes
//! what is emulated.

use crate::upscale::{AiSettings, Network, Upscaler};
use anyhow::{Context, Result};
use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::window::Window;

const SHADER: &str = r#"
struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    return o;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}

fn texel(p: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(tex));
    return textureLoad(tex, clamp(p, vec2<i32>(0), size - vec2<i32>(1)), 0);
}

fn catmull_rom(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return vec4<f32>(
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    );
}

@fragment
fn fs_smooth(in: Out) -> @location(0) vec4<f32> {
    let p = in.uv * vec2<f32>(textureDimensions(tex)) - 0.5;
    let base = floor(p);
    let f = p - base;
    let wx = catmull_rom(f.x);
    let wy = catmull_rom(f.y);
    let b = vec2<i32>(base);
    var acc = vec4<f32>(0.0);
    for (var j = 0; j < 4; j++) {
        var row = vec4<f32>(0.0);
        for (var i = 0; i < 4; i++) {
            row += texel(b + vec2<i32>(i - 1, j - 1)) * wx[i];
        }
        acc += row * wy[j];
    }
    return clamp(acc, vec4<f32>(0.0), vec4<f32>(1.0));
}

// Bilinear between the four texels around texel-space point `t` (texel
// centres at integer + 0.5).
fn bilinear(t: vec2<f32>) -> vec4<f32> {
    let q = t - 0.5;
    let b = vec2<i32>(floor(q));
    let f = q - floor(q);
    let top = mix(texel(b), texel(b + vec2<i32>(1, 0)), f.x);
    let bottom = mix(texel(b + vec2<i32>(0, 1)), texel(b + vec2<i32>(1, 1)), f.x);
    return mix(top, bottom, f.y);
}

// Sharp bilinear: each texel is a solid square; only the one output pixel
// straddling a texel edge is blended, so any scale looks crisp and even.
fn sharp(p: vec2<f32>, px_per_texel: vec2<f32>) -> vec4<f32> {
    let cell = floor(p);
    let f = p - cell;
    // Distance into the cell, pushed towards the centre except within half
    // an output pixel of the edge.
    let g = clamp((f - 0.5) * px_per_texel + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
    return bilinear(cell + g);
}

@fragment
fn fs_sharp(in: Out) -> @location(0) vec4<f32> {
    let p = in.uv * vec2<f32>(textureDimensions(tex));
    let s = 1.0 / max(fwidth(p), vec2<f32>(1e-4));
    return sharp(p, s);
}

// Sharp, plus a faint dark gap between pixels like the real LCD's grid;
// the gap fades out below 3x scale where it would only shimmer.
@fragment
fn fs_lcd(in: Out) -> @location(0) vec4<f32> {
    let p = in.uv * vec2<f32>(textureDimensions(tex));
    let s = 1.0 / max(fwidth(p), vec2<f32>(1e-4));
    let c = sharp(p, s);
    let f = p - floor(p);
    // Distance to the nearest cell edge, in output pixels.
    let d = min(f, 1.0 - f) * s;
    let scale = min(s.x, s.y);
    let gap = 0.08 * scale;
    let line = smoothstep(gap - 0.5, gap + 0.5, min(d.x, d.y));
    let strength = 0.22 * clamp((scale - 2.0) / 2.0, 0.0, 1.0);
    return vec4<f32>(c.rgb * (1.0 - strength * (1.0 - line)), 1.0);
}

// Catmull-Rom kernel.
fn cr(x: f32) -> f32 {
    let a = abs(x);
    if (a < 1.0) {
        return (1.5 * a - 2.5) * a * a + 1.0;
    }
    if (a < 2.0) {
        return ((-0.5 * a + 2.5) * a - 4.0) * a + 2.0;
    }
    return 0.0;
}

// Fits an upscaled picture to the window: Catmull-Rom when enlarging, the
// same kernel widened to the footprint when shrinking (no aliasing).
@fragment
fn fs_resample(in: Out) -> @location(0) vec4<f32> {
    let p = in.uv * vec2<f32>(textureDimensions(tex));
    let fw = fwidth(p);
    let r = clamp(max(fw.x, fw.y), 1.0, 4.0);
    let c = p - 0.5;
    let lo = vec2<i32>(ceil(c - 2.0 * r));
    let hi = vec2<i32>(floor(c + 2.0 * r));
    var acc = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var j = lo.y; j <= hi.y; j++) {
        let wy = cr((c.y - f32(j)) / r);
        for (var i = lo.x; i <= hi.x; i++) {
            let w = wy * cr((c.x - f32(i)) / r);
            acc += texel(vec2<i32>(i, j)).rgb * w;
            wsum += w;
        }
    }
    return vec4<f32>(clamp(acc / wsum, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
"#;

/// How the console's picture is scaled (display only).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum Display {
    /// Neural-network upscaling (see `upscale`), fills the space.
    #[default]
    Ai,
    /// Exact pixels, whole-number scale.
    Pixel,
    /// Square pixels with antialiased edges, fills the space.
    Sharp,
    /// Sharp plus a faint pixel grid.
    Lcd,
    /// Catmull-Rom bicubic, fills the space.
    Smooth,
}

/// Where the picture goes (x, y, w, h in physical pixels) and how it is drawn.
pub type LcdView = ((f32, f32, f32, f32), Display, AiSettings);

pub struct Gpu {
    pub window: Arc<Window>,
    /// Kept so a new surface can be made for a new window (Android takes
    /// the window away while the app is in the background).
    instance: wgpu::Instance,
    /// For setting up more windows' surfaces (`tool_surface`).
    adapter: wgpu::Adapter,
    /// None while detached (`detach`, `reattach`).
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    resample: wgpu::RenderPipeline,
    /// None when the GPU cannot render to Rgba16Float (AI falls back to Sharp).
    upscaler: Option<Upscaler>,
    /// The picture changed since the last upscale.
    frame_dirty: bool,
    last_chain: Vec<Network>,
    /// The console's picture: its texture, size and bind group.
    picture: Option<(wgpu::Texture, (u32, u32), wgpu::BindGroup)>,
    /// The picture last uploaded: the app redraws more often than the
    /// console draws, and a new upload re-runs the AI upscale.
    pixels: Vec<u32>,
    /// A recording's frame drawn off screen (`capture`): its texture and
    /// the buffer it is read back through.
    capture: Option<(wgpu::Texture, wgpu::Buffer, (u32, u32))>,
    pixel: wgpu::RenderPipeline,
    sharp: wgpu::RenderPipeline,
    lcd: wgpu::RenderPipeline,
    smooth: wgpu::RenderPipeline,
    pub egui: egui_wgpu::Renderer,
}

/// What `Gpu::connect` sets up: instance, surface, adapter, device, queue
/// and the surface's configuration.
type Connection =
    (wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter, wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration);

impl Gpu {
    /// For F3's details: the adapter, its graphics API and the size drawn to.
    pub fn summary(&self) -> (String, (u32, u32)) {
        let i = self.adapter.get_info();
        (format!("{} ({:?}, {:?})", i.name, i.backend, self.config.present_mode), (self.config.width, self.config.height))
    }

    /// A GPU set up to show pictures in `window`: the platform's usual API
    /// first (or the one `WGPU_BACKEND` names: vulkan, gl, dx12, metal),
    /// then OpenGL, then on Windows the software DirectX 12 renderer (WARP,
    /// part of Windows 10 and 11) for graphics chips that offer neither
    /// DirectX 12 nor a recent enough OpenGL. Some Linux setups (virtual machines, WSL, old drivers)
    /// offer a Vulkan device that says it can present to the window and
    /// then fails when the window is set up; that is tried and caught here.
    ///
    /// A graphics driver that crashes the process while the card is tried
    /// cannot be caught (a player's PC closed Blub at random starts, the
    /// window black, the log ending before any GPU line; drawing in software
    /// always started). So on Windows the tries are steps - every backend
    /// (which loads Vulkan and any overlay layers installed with it), DX12 on
    /// the card, OpenGL on the card, software - and `guard` (a file in the
    /// data folder) holds the Blub version and the step to start from.
    /// Before a step, the file says to start after it: a crash in that step
    /// is skipped at the next start. A step that works is written back, so
    /// later starts begin there. An update starts from the first step again.
    fn connect(window: &Arc<Window>, guard: Option<&std::path::Path>) -> Result<Connection> {
        // WSL's Vulkan driver cannot keep a window going (it fails a few
        // frames in), so OpenGL comes first there.
        let wsl = cfg!(target_os = "linux")
            && std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .is_ok_and(|r| r.to_lowercase().contains("microsoft"));
        let usual = if wsl { wgpu::Backends::GL } else { wgpu::Backends::all() };
        let first = wgpu::Backends::from_env().unwrap_or(usual);
        let mut tried = Vec::new();
        let mut last = anyhow::anyhow!("no GPU backend available");
        let software = cfg!(windows).then_some((wgpu::Backends::DX12, true));
        let guard = guard.filter(|_| software.is_some());
        let version = env!("CARGO_PKG_VERSION");
        let start = guard
            .and_then(|g| std::fs::read_to_string(g).ok())
            .and_then(|t| {
                let (v, step) = t.trim().split_once(' ')?;
                (v == version).then(|| step.parse::<usize>().ok()).flatten()
            })
            .unwrap_or(0);
        // BLUB_SOFTWARE_GPU: straight to the software renderer (a GPU
        // driver that draws wrongly or crashes).
        let hardware = if std::env::var_os("BLUB_SOFTWARE_GPU").is_some() && software.is_some() {
            // Said in the log: software drawing is slow in a big window (a
            // player's "slowdown" that went away in a small window was this,
            // the variable set long before as the guide's workaround).
            tracing::warn!(
                "BLUB_SOFTWARE_GPU is set: drawing in software, which slows the picture in a large window \
                 (remove the variable to use the graphics card)"
            );
            vec![]
        } else if software.is_some() {
            vec![(first, false), (wgpu::Backends::DX12, false), (wgpu::Backends::GL, false)]
        } else {
            vec![(first, false), (wgpu::Backends::GL, false)]
        };
        let steps: Vec<_> = hardware.into_iter().chain(software).collect();
        let guard = guard.filter(|_| steps.len() > 1);
        let start = start.min(steps.len() - 1);
        if start > 0 {
            tracing::warn!(
                "an earlier start ended while setting up the graphics card (its driver crashed?): starting \
                 from GPU {:?} (delete {} to try every way again; an update does too)",
                steps[start],
                guard.map_or_else(Default::default, |g| g.display().to_string())
            );
        }
        let note = |step: usize| {
            if let Some(g) = guard {
                let _ = std::fs::write(g, format!("{version} {step}"));
            }
        };
        for (i, &(backends, software)) in steps.iter().enumerate().skip(start) {
            if tried.contains(&(backends, software)) {
                continue;
            }
            tried.push((backends, software));
            note(i + 1);
            // Said before each try: a driver that crashes the process here
            // leaves this as the log's last line.
            tracing::info!("setting up the window with GPU ({backends:?}, software: {software})");
            match Self::connect_with(window, backends, software) {
                Ok(c) => {
                    match guard {
                        Some(g) if i == 0 => drop(std::fs::remove_file(g)),
                        _ => note(i),
                    }
                    if software {
                        tracing::warn!("no GPU can draw the window: drawing in software (WARP)");
                    }
                    return Ok(c);
                }
                Err(e) => {
                    tracing::warn!("GPU ({backends:?}, software: {software}) cannot show the window: {e:#}");
                    last = e;
                }
            }
        }
        if let Some(g) = guard {
            let _ = std::fs::remove_file(g);
        }
        Err(last.context(format!("no GPU can draw to the window (tried {tried:?})")))
    }

    /// `software`: the backend's software renderer instead of a GPU.
    fn connect_with(window: &Arc<Window>, backends: wgpu::Backends, software: bool) -> Result<Connection> {
        // (OpenGL through EGL needs the display the window is on.)
        let mut desc = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone()));
        desc.backends = backends;
        let instance = wgpu::Instance::new(desc);
        let surface = instance.create_surface(window.clone()).context("creating surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            force_fallback_adapter: software,
            ..Default::default()
        }))
        .context("no compatible GPU adapter")?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("blub"),
            // Lowest common denominator so the same renderer runs on GLES
            // devices (Android) as well as Vulkan/Metal/DX12.
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .context("creating GPU device")?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .context("surface not supported by adapter")?;
        // egui blends in gamma space and wants a non-sRGB target; the LCD
        // texture is non-sRGB too, so the emulated colours pass through as is.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = f;
        }
        // Set up the window and show one blank frame, catching what fails.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        surface.configure(&device, &config);
        if let wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) =
            surface.get_current_texture()
        {
            let view = frame.texture.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("first frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            }));
            queue.submit([encoder.finish()]);
            queue.present(frame);
        }
        surface.configure(&device, &config);
        if let Some(e) = pollster::block_on(scope.pop()) {
            anyhow::bail!("{:?} window setup failed: {e}", adapter.get_info().backend);
        }
        Ok((instance, surface, adapter, device, queue, config))
    }

    /// `guard`: see `connect` (None: no crash fallback).
    pub fn new(window: Arc<Window>, guard: Option<&std::path::Path>) -> Result<Self> {
        let (instance, surface, adapter, device, queue, config) = Self::connect(&window, guard)?;
        let info = adapter.get_info();
        tracing::info!(target: "blub::perf", adapter = %info.name, backend = ?info.backend, driver = %info.driver_info,
            present = ?config.present_mode, "graphics");

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lcd"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lcd"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lcd"),
            bind_group_layouts: &[Some(&bgl)],
            ..Default::default()
        });
        let make = |fragment: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("lcd"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(config.format.into())],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let (pixel, sharp, lcd, smooth) = (make("fs"), make("fs_sharp"), make("fs_lcd"), make("fs_smooth"));
        let resample = make("fs_resample");
        let upscaler = Upscaler::supported(&adapter).then(Upscaler::default);
        if upscaler.is_none() {
            tracing::warn!(target: "blub::video", "GPU cannot render Rgba16Float: AI upscaling unavailable");
        }
        let egui = egui_wgpu::Renderer::new(&device, config.format, egui_wgpu::RendererOptions::default());
        Ok(Self {
            window,
            instance,
            adapter,
            surface: Some(surface),
            device,
            queue,
            config,
            picture: None,
            pixels: Vec::new(),
            capture: None,
            bgl,
            sampler,
            resample,
            upscaler,
            frame_dirty: true,
            last_chain: Vec::new(),
            pixel,
            sharp,
            lcd,
            smooth,
            egui,
        })
    }

    /// Lets go of the window's surface (Android: the app went to the
    /// background and its window is going away). The device and every
    /// texture stay, so nothing has to be uploaded again.
    pub fn detach(&mut self) {
        self.surface = None;
    }

    /// Shows pictures in `window` again (Android: the app came back with a
    /// new window), on the same device.
    pub fn reattach(&mut self, window: Arc<Window>) -> Result<()> {
        let surface = self.instance.create_surface(window.clone()).context("creating surface")?;
        let size = window.inner_size();
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        surface.configure(&self.device, &self.config);
        self.surface = Some(surface);
        self.window = window;
        Ok(())
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            // Never past what the GPU allows: Windows has reported a
            // nonsense size (a negative height read as 65471) for a window
            // sized from outside, and the surface would refuse it (a panic).
            let most = self.device.limits().max_texture_dimension_2d;
            self.config.width = size.width.min(most);
            self.config.height = size.height.min(most);
            if let Some(surface) = &self.surface {
                surface.configure(&self.device, &self.config);
            }
        }
    }

    /// Where a picture of `size` goes inside `area` (x, y, w, h in physical
    /// pixels): as large as fits with its shape kept; at a whole-number
    /// scale for `Display::Pixel` (when at least 1x fits).
    pub fn picture_rect(area: (f32, f32, f32, f32), size: (usize, usize), display: Display) -> (f32, f32, f32, f32) {
        let (ax, ay, aw, ah) = area;
        let (tw, th) = (size.0 as f32, size.1 as f32);
        let mut scale = (aw / tw).min(ah / th).max(0.01);
        // A pixel of tolerance: fractional display scaling can leave the
        // area a fraction short of a whole multiple.
        if display == Display::Pixel && scale >= 1.0 {
            scale = ((aw + 1.0) / tw).min((ah + 1.0) / th).floor().max(1.0);
        }
        let (w, h) = (tw * scale, th * scale);
        (ax + (aw - w) / 2.0, ay + (ah - h) / 2.0, w, h)
    }

    /// The console's picture (0x00RRGGBB), `size` pixels.
    pub fn upload(&mut self, pixels: &[u32], size: (usize, usize)) {
        let (w, h) = (size.0 as u32, size.1 as u32);
        if self.picture.as_ref().is_none_or(|t| t.1 != (w, h)) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("picture"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("picture"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.picture = Some((texture, (w, h), group));
            self.pixels.clear();
        }
        if self.pixels == pixels {
            return;
        }
        self.pixels.clear();
        self.pixels.extend_from_slice(pixels);
        let rgba: Vec<u8> = pixels.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 0xFF]).collect();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.picture.as_ref().unwrap().0,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        // (For the AI upscale.)
        self.frame_dirty = true;
    }

    /// Draws the picture into `lcd` (None: no game screen) and egui on top.
    pub fn render(
        &mut self,
        lcd: Option<LcdView>,
        paint_jobs: &[egui::ClippedPrimitive],
        textures: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> Result<()> {
        for (id, deltas) in &textures.set {
            for delta in deltas {
                self.egui.update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        use wgpu::CurrentSurfaceTexture as Cst;
        // Detached (Android, in the background): nothing to show on.
        let Some(surface) = &self.surface else {
            return Ok(());
        };
        // Suboptimal: this frame is still shown, and the surface is set up
        // again once it is presented (wgpu refuses while the frame is held:
        // a validation error that ended Blub on some OpenGL drivers).
        let (output, suboptimal) = match surface.get_current_texture() {
            Cst::Success(t) => (t, false),
            Cst::Suboptimal(t) => (t, true),
            Cst::Outdated => {
                surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Cst::Timeout | Cst::Occluded => return Ok(()),
            Cst::Lost => anyhow::bail!("GPU surface lost"),
            Cst::Validation => anyhow::bail!("GPU validation error acquiring surface texture"),
        };
        let view = output.texture.create_view(&Default::default());
        let screen =
            egui_wgpu::ScreenDescriptor { size_in_pixels: [self.config.width, self.config.height], pixels_per_point };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let extra = self.egui.update_buffers(&self.device, &self.queue, &mut encoder, paint_jobs, &screen);
        // AI upscale, only when the picture or the chain changed. The
        // networks work at any size: sized by the picture's longer side.
        let mut ai_bind_group = None;
        let input = self.picture.as_ref().map(|t| (t.0.create_view(&Default::default()), t.1));
        if let (Some(((_, _, w, _), Display::Ai, ai)), Some(up), Some((input, (tw, th)))) = (lcd, &mut self.upscaler, &input) {
            let side = *tw.max(th);
            let chain = ai.chain(side, ai.resolution.target(side, w));
            let view = if self.frame_dirty || chain != self.last_chain || up.output().is_none() {
                self.frame_dirty = false;
                self.last_chain.clone_from(&chain);
                up.run(&self.device, &mut encoder, input, (*tw, *th), &chain, 1)
            } else {
                up.output().unwrap()
            };
            ai_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("upscaled"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            }));
        }
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.025, b: 0.03, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            if let (Some(((x, y, w, h), display, _)), Some(picture)) = (lcd, &self.picture) {
                let (fw, fh) = (self.config.width as f32, self.config.height as f32);
                let (x0, y0) = (x.max(0.0), y.max(0.0));
                let (w, h) = (w.min(fw - x0), h.min(fh - y0));
                if w >= 1.0 && h >= 1.0 {
                    pass.set_viewport(x0, y0, w, h, 0.0, 1.0);
                    match (&ai_bind_group, display) {
                        (Some(group), Display::Ai) => {
                            pass.set_pipeline(&self.resample);
                            pass.set_bind_group(0, group, &[]);
                        }
                        _ => {
                            pass.set_pipeline(match display {
                                Display::Pixel => &self.pixel,
                                Display::Ai | Display::Sharp => &self.sharp,
                                Display::Lcd => &self.lcd,
                                Display::Smooth => &self.smooth,
                            });
                            pass.set_bind_group(0, &picture.2, &[]);
                        }
                    }
                    pass.draw(0..3, 0..1);
                    pass.set_viewport(0.0, 0.0, fw, fh, 0.0, 1.0);
                }
            }
            self.egui.render(&mut pass, paint_jobs, &screen);
        }
        self.queue.submit(extra.into_iter().chain([encoder.finish()]));
        self.queue.present(output);
        if suboptimal {
            surface.configure(&self.device, &self.config);
        }
        for id in &textures.free {
            self.egui.free_texture(id);
        }
        Ok(())
    }
}

impl Gpu {
    /// Draws the picture (at `lcd`: x, y, w, h) and `paint_jobs` (the
    /// console view, tessellated at 1 point per pixel) into a `size` frame
    /// off screen and reads it back (0x00RRGGBB), for a recording of the
    /// console view. Plain drawing (an AI display drawn Sharp).
    /// Call after `render`, which has brought egui's textures up to date.
    pub fn capture(&mut self, size: (u32, u32), lcd: (f32, f32, f32, f32), display: Display, paint_jobs: &[egui::ClippedPrimitive]) -> Option<Vec<u32>> {
        let (w, h) = size;
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        if self.capture.as_ref().is_none_or(|c| c.2 != size) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("capture"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("capture readback"),
                size: (row * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.capture = Some((texture, buffer, size));
        }
        let (texture, buffer, _) = self.capture.as_ref().unwrap();
        let view = texture.create_view(&Default::default());
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [w, h], pixels_per_point: 1.0 };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let extra = self.egui.update_buffers(&self.device, &self.queue, &mut encoder, paint_jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("capture"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.025, b: 0.03, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            let (x, y, lw, lh) = lcd;
            let (x0, y0) = (x.max(0.0), y.max(0.0));
            let (lw, lh) = (lw.min(w as f32 - x0), lh.min(h as f32 - y0));
            if let Some(picture) = self.picture.as_ref().filter(|_| lw >= 1.0 && lh >= 1.0) {
                pass.set_viewport(x0, y0, lw, lh, 0.0, 1.0);
                pass.set_pipeline(match display {
                    Display::Pixel => &self.pixel,
                    Display::Lcd => &self.lcd,
                    Display::Smooth => &self.smooth,
                    _ => &self.sharp,
                });
                pass.set_bind_group(0, &picture.2, &[]);
                pass.draw(0..3, 0..1);
                pass.set_viewport(0.0, 0.0, w as f32, h as f32, 0.0, 1.0);
            }
            self.egui.render(&mut pass, paint_jobs, &screen);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(extra.into_iter().chain([encoder.finish()]));
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        rx.recv().ok()?.ok()?;
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let out = {
            let data = slice.get_mapped_range().ok()?;
            let mut out = Vec::with_capacity((w * h) as usize);
            for line in data.chunks_exact(row as usize).take(h as usize) {
                out.extend(line[..(w * 4) as usize].chunks_exact(4).map(|p| {
                    let (r, g, b) = if bgra { (p[2], p[1], p[0]) } else { (p[0], p[1], p[2]) };
                    (r as u32) << 16 | (g as u32) << 8 | b as u32
                }));
            }
            out
        };
        buffer.unmap();
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    /// wgpu only reports shader errors when the window opens; check here.
    #[test]
    fn lcd_shaders_parse_and_validate() {
        let module = naga::front::wgsl::parse_str(super::SHADER)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(super::SHADER)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .expect("shader validates");
        for entry in ["fs", "fs_sharp", "fs_lcd", "fs_smooth", "fs_resample"] {
            assert!(module.entry_points.iter().any(|e| e.name == entry), "{entry}");
        }
    }
}
