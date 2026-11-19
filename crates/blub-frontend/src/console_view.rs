//! Console view: the emulated picture on a drawn TV above a drawing of the
//! console itself, the I Can Play Piano (its keyboard and buttons
//! clickable) or the I Can Play Guitar (its fret buttons, strings and
//! buttons; see `paint_guitar`). A
//! presentation feature only: it turns pointer input into the same key and
//! button presses the PC keyboard makes and never touches the emulation.
//!
//! Everything is drawn with vector shapes (Blub's own artwork, no
//! photographs or logos), laid out after photos of the real console: a
//! glossy black case with a silver-and-purple speaker at each end, a pale
//! control panel of shaped buttons between them (Home, Enter and the
//! up/down rocker; the four mode buttons; the hands, metronome and pause
//! buttons; tempo up, normal and down; the instrument button), and the 36
//! keys, F to E, colour-coded as the toy's are: a coloured pad on each
//! white key but C, a coloured stripe on each black key but C#, and a mint
//! lip over the middle octave. A key lights up while it is held from any
//! source (PC keys, MIDI, the mouse). The mouse plays a key when pressed
//! on it, lets go when released, and slides from key to key while held.
//! The proportions are Blub's (no measured drawing), but for one: the TV
//! is sized so the keyboard the games draw across the top of the picture
//! (song screens) lies right above the drawn keys, key over key.

use crate::piano::{Input, is_black};
use blub_piano::{Button, GuitarButton, KEYS};
use egui::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};

/// The drawing's size in its own units (`U`), and where things are in it.
const W: f32 = 130.0;
const H: f32 = 155.0;
/// The keyboard the games draw on song screens: its first and last pixel
/// across the picture (F to E, 21 white keys, as on the console).
const GAME_KEYS: (f32, f32) = (9.0, 311.0);
/// Where the piano (all of it below, in its own units) is in the drawing.
const PIANO_AT: Vec2 = vec2(5.0, 33.0);
/// Drawing units per picture pixel: the game's keyboard as wide as the
/// keybed.
const PIXEL: f32 = (KEYBED.max.x - KEYBED.min.x) / (GAME_KEYS.1 - GAME_KEYS.0);
/// The picture on the TV (4:3), the game's keyboard right over the keybed.
const PICTURE: Rect = Rect {
    min: pos2(PIANO_AT.x + KEYBED.min.x - GAME_KEYS.0 * PIXEL, 5.0),
    max: pos2(PIANO_AT.x + KEYBED.min.x + (blub_piano::SCREEN_W as f32 - GAME_KEYS.0) * PIXEL, 5.0 + blub_piano::SCREEN_H as f32 * PIXEL),
};
/// The piano's case, its control panel, speakers and keys (piano units:
/// see PIANO_AT).
const CASE: Rect = Rect { min: pos2(1.0, 70.0), max: pos2(119.0, 121.0) };
const PANEL: Rect = Rect { min: pos2(22.0, 72.0), max: pos2(98.0, 88.5) };
const SPEAKERS: [Rect; 2] = [
    Rect { min: pos2(3.0, 71.5), max: pos2(20.5, 88.5) },
    Rect { min: pos2(99.5, 71.5), max: pos2(117.0, 88.5) },
];
const KEYBED: Rect = Rect { min: pos2(4.0, 91.0), max: pos2(116.0, 118.0) };
/// White keys on the keyboard (F to E, three octaves).
const WHITE_KEYS: usize = KEYS * 7 / 12;

const CASE_BLACK: Color32 = Color32::from_rgb(20, 20, 24);
const PANEL_PALE: Color32 = Color32::from_rgb(228, 230, 244);
const MINT: Color32 = Color32::from_rgb(122, 214, 190);
const INK: Color32 = Color32::from_rgb(60, 60, 90);
/// A held key's or button's light.
const LIT: Color32 = Color32::from_rgb(90, 190, 255);

/// The colour code on the keys, by note from F (None: C and C#, plain).
const NOTE_COLOURS: [Option<Color32>; 12] = [
    Some(Color32::from_rgb(110, 200, 70)),  // F: green
    Some(Color32::from_rgb(150, 70, 190)),  // F#: purple
    Some(Color32::from_rgb(245, 215, 40)),  // G: yellow
    Some(Color32::from_rgb(50, 100, 220)),  // G#: blue
    Some(Color32::from_rgb(214, 170, 140)), // A: tan
    Some(Color32::from_rgb(40, 150, 80)),   // A#: green
    Some(Color32::from_rgb(220, 120, 210)), // B: pink
    None,                                   // C
    None,                                   // C#
    Some(Color32::from_rgb(70, 170, 240)),  // D: blue
    Some(Color32::from_rgb(210, 50, 40)),   // D#: red
    Some(Color32::from_rgb(250, 160, 30)),  // E: orange
];
/// What the pointer did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Press(Input, bool),
}

/// The drawing's shape and where the picture goes in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Width / height of the drawing.
    pub aspect: f32,
    /// The picture, in the unit square (0..1 across, 0..1 down).
    pub screen: Rect,
    /// Width / height of the emulated picture.
    pub screen_aspect: f32,
    /// The guitar's drawing, not the piano's.
    pub guitar: bool,
}

/// The drawing of the console a game runs on.
pub fn layout(guitar: bool) -> Layout {
    if guitar { guitar_layout() } else { piano_layout() }
}

/// The I Can Play Piano's drawing.
pub fn piano_layout() -> Layout {
    Layout {
        aspect: W / H,
        screen: Rect::from_min_max(pos2(PICTURE.min.x / W, PICTURE.min.y / H), pos2(PICTURE.max.x / W, PICTURE.max.y / H)),
        screen_aspect: blub_piano::SCREEN_W as f32 / blub_piano::SCREEN_H as f32,
        guitar: false,
    }
}

/// What the view shows on its controls.
pub struct Lit<'a> {
    pub pressed: &'a dyn Fn(Input) -> bool,
    /// The hands' lights on the panel (left, right): lit red while that
    /// hand plays.
    pub hands: (bool, bool),
    /// The keyboard key for an input, as its keycap shows it.
    pub cap: &'a dyn Fn(Input) -> Option<String>,
    pub show_caps: bool,
    /// The keys for an input, for its tooltip.
    pub keys: &'a dyn Fn(Input) -> String,
    /// The picture is drawn at whole multiples only (the Pixel display):
    /// the drawing is then sized so the picture is one.
    pub whole_pixels: bool,
}

#[derive(Default)]
pub struct ConsoleView {
    /// The input the pointer holds now.
    held: Option<Input>,
}

/// A white key's or black key's place on the keybed (drawing units).
fn key_rect(key: usize) -> Rect {
    let w = KEYBED.width() / WHITE_KEYS as f32;
    // White keys before this one.
    let whites = (0..key).filter(|&k| !is_black(k)).count() as f32;
    if is_black(key) {
        let x = KEYBED.min.x + whites * w;
        Rect::from_center_size(pos2(x, KEYBED.min.y + KEYBED.height() * 0.31), vec2(w * 0.62, KEYBED.height() * 0.62))
    } else {
        Rect::from_min_size(pos2(KEYBED.min.x + whites * w, KEYBED.min.y), vec2(w, KEYBED.height()))
    }
}

/// The buttons on the panel, as on the console: each one's place
/// (drawing units, its centre and size).
fn button_rects() -> Vec<(Button, Rect)> {
    let at = |x: f32, y: f32, w: f32, h: f32| Rect::from_center_size(pos2(x, y), vec2(w, h));
    vec![
        (Button::Home, at(30.5, 80.5, 6.0, 3.8)),
        (Button::Enter, at(37.0, 76.0, 3.6, 3.6)),
        (Button::ScrollUp, at(37.0, 81.6, 3.8, 3.0)),
        (Button::ScrollDown, at(37.0, 84.6, 3.8, 3.0)),
        (Button::Display1, at(42.5, 77.5, 4.0, 4.0)),
        (Button::Display2, at(47.5, 77.5, 4.0, 4.0)),
        (Button::Display3, at(42.5, 82.5, 4.0, 4.0)),
        (Button::Display4, at(47.5, 82.5, 4.0, 4.0)),
        (Button::CycleHands, at(57.0, 85.6, 3.0, 3.0)),
        (Button::Metronome, at(65.0, 80.0, 4.2, 5.2)),
        (Button::Pause, at(72.0, 80.0, 5.8, 3.8)),
        (Button::TempoUp, at(80.5, 76.4, 4.2, 2.8)),
        (Button::TempoDefault, at(80.5, 80.0, 4.2, 3.2)),
        (Button::TempoDown, at(80.5, 83.6, 4.2, 2.8)),
        (Button::ChangeInstrument, at(87.5, 84.5, 3.0, 3.0)),
    ]
}

/// The button's own colour.
fn button_color(b: Button) -> Color32 {
    match b {
        Button::Home | Button::Pause => Color32::from_rgb(40, 175, 170),
        Button::Enter | Button::CycleHands | Button::ChangeInstrument => Color32::from_rgb(118, 116, 104),
        Button::ScrollUp | Button::ScrollDown => Color32::from_rgb(80, 130, 225),
        Button::Display1 | Button::Metronome | Button::TempoDown => Color32::from_rgb(245, 205, 40),
        Button::Display2 | Button::TempoDefault => Color32::from_rgb(245, 150, 40),
        Button::Display3 | Button::TempoUp => Color32::from_rgb(232, 76, 48),
        Button::Display4 => Color32::from_rgb(40, 170, 160),
    }
}
/// Maps drawing units into `shell` (screen points).
struct Map {
    shell: Rect,
}

impl Map {
    fn s(&self) -> f32 {
        self.shell.width() / W
    }
    fn p(&self, p: Pos2) -> Pos2 {
        self.shell.min + p.to_vec2() * self.s()
    }
    fn r(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.p(r.min), self.p(r.max))
    }
}

/// The map for the piano's own units (PIANO_AT).
fn piano_map(map: &Map) -> Map {
    Map { shell: map.shell.translate(PIANO_AT * map.s()) }
}

/// The input under `p` (screen points), black keys first (they lie over
/// the white ones).
fn hit(map: &Map, p: Pos2) -> Option<Input> {
    let map = &piano_map(map);
    let black = (0..KEYS).filter(|&k| is_black(k)).find(|&k| map.r(key_rect(k)).contains(p));
    let white = || (0..KEYS).filter(|&k| !is_black(k)).find(|&k| map.r(key_rect(k)).contains(p));
    if let Some(k) = black.or_else(white) {
        return Some(Input::Key(k as u8));
    }
    button_rects().into_iter().find(|(_, r)| map.r(*r).contains(p)).map(|(b, _)| Input::Button(b))
}

/// Where the drawing goes in `area`: as large as fits, its shape kept,
/// centred, with a small margin.
pub fn shell_rect(layout: &Layout, area: Rect) -> Rect {
    let margin = (area.width().min(area.height()) * 0.02).min(12.0);
    let a = area.shrink(margin);
    let w = a.width().min(a.height() * layout.aspect).max(1.0);
    Rect::from_center_size(a.center(), vec2(w, w / layout.aspect))
}

/// A unit-square rectangle placed in `shell`.
pub fn place(shell: Rect, r: Rect) -> Rect {
    Rect::from_min_max(
        pos2(shell.left() + r.left() * shell.width(), shell.top() + r.top() * shell.height()),
        pos2(shell.left() + r.right() * shell.width(), shell.top() + r.bottom() * shell.height()),
    )
}

impl ConsoleView {
    /// Lets go of everything the pointer held (events to apply).
    pub fn release_all(&mut self) -> Vec<Event> {
        self.held.take().map(|i| Event::Press(i, false)).into_iter().collect()
    }

    /// Draws the piano in `area` and handles the pointer; returns where the
    /// picture is and what the pointer did. `under` is a slot painted
    /// before the rest of the UI, so menus and messages stay on top.
    pub fn show(&mut self, ui: &egui::Ui, under: egui::layers::ShapeIdx, layout: &Layout, area: Rect, lit: &Lit) -> (Rect, Vec<Event>) {
        let mut shell = shell_rect(layout, area);
        if lit.whole_pixels {
            // The whole drawing scaled so its picture is a whole multiple of
            // the console's: the next multiple up when the drawing still fits
            // the area at it (eating into the margin), else the one below.
            let ppp = ui.ctx().pixels_per_point();
            let screen = place(shell, layout.screen);
            let times = screen.width() * ppp / blub_piano::SCREEN_W as f32;
            let size_at = |n: f32| shell.size() * (n * blub_piano::SCREEN_W as f32 / ppp / screen.width());
            let up = times.ceil();
            let n = if size_at(up).x <= area.width() && size_at(up).y <= area.height() { up } else { times.floor() };
            if n >= 1.0 {
                shell = Rect::from_center_size(shell.center(), size_at(n));
            }
        }
        let screen = place(shell, layout.screen);
        let map = Map { shell };
        ui.painter().set(under, egui::Shape::Vec(paint_any(layout, &map, lit, ui.ctx())));
        // The case is one control: whatever is under the pointer while it
        // is held down on it is played (sliding from key to key).
        let case = if layout.guitar { map.r(G_GUITAR) } else { piano_map(&map).r(CASE) };
        let hit = |p: Pos2| if layout.guitar { guitar_hit(&map, p) } else { hit(&map, p) };
        let resp = ui.interact(case, egui::Id::new("piano console view"), egui::Sense::click_and_drag());
        let pos = ui.ctx().pointer_latest_pos();
        let now = if resp.is_pointer_button_down_on() { pos.and_then(hit) } else { None };
        if resp.hovered()
            && let Some(i) = pos.and_then(hit)
        {
            let keys = (lit.keys)(i);
            let tip = if keys.is_empty() { i.label() } else { format!("{}  ({keys})", i.label()) };
            resp.on_hover_text(tip);
        }
        let mut events = Vec::new();
        if now != self.held {
            if let Some(old) = self.held {
                events.push(Event::Press(old, false));
            }
            if let Some(new) = now {
                events.push(Event::Press(new, true));
            }
            self.held = now;
        }
        (screen, events)
    }
}

/// The drawing for a recording of the console view, `size` pixels (at one
/// point per pixel), and where the picture goes in it.
pub fn capture_shapes(layout: &Layout, size: (u32, u32), lit: &Lit, ctx: &egui::Context) -> (Vec<egui::Shape>, Rect) {
    let shell = Rect::from_min_size(Pos2::ZERO, vec2(size.0 as f32, size.1 as f32));
    let screen = place(shell, layout.screen);
    (paint_any(layout, &Map { shell }, lit, ctx), screen)
}

fn paint_any(layout: &Layout, map: &Map, lit: &Lit, ctx: &egui::Context) -> Vec<egui::Shape> {
    if layout.guitar { paint_guitar(map, lit, ctx) } else { paint(map, lit, ctx) }
}

/// A recording's size for the console view `height` pixels tall (even
/// sides, as video encoders want).
pub fn capture_size(layout: &Layout, height: u32) -> (u32, u32) {
    (((height as f32 * layout.aspect) as u32) & !1, height & !1)
}

fn text(ctx: &egui::Context, pos: Pos2, anchor: egui::Align2, s: &str, size: f32, color: Color32) -> egui::Shape {
    ctx.fonts_mut(|f| egui::Shape::text(f, pos, anchor, s, egui::FontId::proportional(size.max(1.0)), color))
}

/// A small keycap with `label` centred at `c` (lit while `on`).
fn keycap(ctx: &egui::Context, out: &mut Vec<egui::Shape>, c: Pos2, label: &str, size: f32, on: bool) {
    let galley = ctx.fonts_mut(|f| f.layout_no_wrap(label.to_string(), egui::FontId::proportional(size.max(1.0)), Color32::WHITE));
    let r = Rect::from_center_size(c, galley.size() + vec2(size * 0.5, size * 0.15));
    out.push(egui::Shape::rect_filled(r, size * 0.25, if on { Color32::from_rgb(30, 110, 200) } else { Color32::from_black_alpha(150) }));
    out.push(egui::Shape::galley(r.center() - galley.size() / 2.0, galley, Color32::WHITE));
}

fn paint(map: &Map, lit: &Lit, ctx: &egui::Context) -> Vec<egui::Shape> {
    let s = map.s();
    let mut out = Vec::new();
    // The TV: a dark set on a short stand, the picture inside it. The
    // picture is drawn before these shapes, so the set is a frame round it
    // (a band), never painted over it.
    let tv = map.r(PICTURE.expand(4.0));
    let frame = |out: &mut Vec<egui::Shape>, r: Rect, col: Color32| {
        out.push(egui::Shape::rect_stroke(r.shrink(2.0 * s), 2.0 * s, Stroke::new(4.0 * s, col), egui::StrokeKind::Middle));
    };
    out.push(egui::Shape::rect_filled(map.r(Rect::from_center_size(pos2(W / 2.0, PICTURE.max.y + 5.6), vec2(26.0, 3.0))), 1.0 * s, Color32::from_gray(40)));
    out.push(egui::Shape::rect_filled(Rect::from_min_max(pos2(tv.min.x + 2.0 * s, tv.max.y), pos2(tv.max.x - 2.0 * s, tv.max.y + 1.0 * s)), 1.0 * s, Color32::from_black_alpha(70)));
    frame(&mut out, tv, Color32::from_gray(28));
    out.push(egui::Shape::rect_stroke(tv, 4.0 * s, Stroke::new(0.6 * s, Color32::from_gray(70)), egui::StrokeKind::Inside));
    // The piano, in its own units.
    let map = &piano_map(map);
    // The case: glossy black, a sheen along its top.
    let case = map.r(CASE);
    out.push(egui::Shape::rect_filled(case.translate(vec2(0.0, 1.2 * s)), 6.0 * s, Color32::from_black_alpha(80)));
    out.push(egui::Shape::rect_filled(case, 6.0 * s, CASE_BLACK));
    out.push(egui::Shape::rect_stroke(case, 6.0 * s, Stroke::new(0.4 * s, Color32::from_gray(60)), egui::StrokeKind::Inside));
    out.push(egui::Shape::ellipse_filled(map.p(pos2(60.0, 71.0)), vec2(36.0, 0.7) * s, Color32::from_white_alpha(28)));
    // The speakers: silver housings, purple grilles.
    for (n, r) in SPEAKERS.iter().enumerate() {
        let r = map.r(*r);
        out.push(egui::Shape::rect_filled(r, 3.0 * s, Color32::from_gray(150)));
        out.push(egui::Shape::rect_filled(r.shrink(0.5 * s), 2.6 * s, Color32::from_gray(196)));
        let c = r.center() + vec2(if n == 0 { -0.4 } else { 0.4 } * s, 0.0);
        let rad = vec2(r.width() * 0.4, r.height() * 0.42);
        out.push(egui::Shape::ellipse_filled(c, rad + Vec2::splat(0.4 * s), Color32::from_rgb(40, 30, 110)));
        out.push(egui::Shape::ellipse_filled(c, rad, Color32::from_rgb(92, 70, 205)));
        for k in 1..5 {
            let y = c.y - rad.y + rad.y * 2.0 * k as f32 / 5.0;
            let half = rad.x * (1.0 - ((y - c.y) / rad.y).powi(2)).max(0.0).sqrt() * 0.86;
            out.push(egui::Shape::line_segment([pos2(c.x - half, y), pos2(c.x + half, y)], Stroke::new(0.35 * s, Color32::from_rgb(58, 42, 150))));
        }
        out.push(egui::Shape::ellipse_filled(c - vec2(rad.x * 0.3, rad.y * 0.45), vec2(rad.x * 0.35, rad.y * 0.18), Color32::from_white_alpha(40)));
    }
    // The control panel: a pale decal with a warm swoosh along its top.
    out.push(egui::Shape::rect_filled(map.r(PANEL), 3.0 * s, PANEL_PALE));
    let mut swoosh = egui::Mesh::default();
    let steps = 16;
    for k in 0..=steps {
        let t = k as f32 / steps as f32;
        let x = PANEL.min.x + 1.5 + t * PANEL.width() * 0.62;
        let top = map.p(pos2(x, PANEL.min.y + 0.6));
        let bottom = map.p(pos2(x, PANEL.min.y + 0.6 + 2.0 * (1.0 - t * 0.85)));
        let col = mix(Color32::from_rgb(232, 70, 40), Color32::from_rgb(150, 90, 200), t);
        let col = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (255.0 * (1.0 - t * 0.8)) as u8);
        swoosh.colored_vertex(top, col);
        swoosh.colored_vertex(bottom, col);
        if k > 0 {
            let v = 2 * k as u32;
            swoosh.add_triangle(v - 2, v - 1, v);
            swoosh.add_triangle(v - 1, v + 1, v);
        }
    }
    out.push(egui::Shape::mesh(swoosh));
    out.push(text(ctx, map.p(pos2(57.0, 75.0)), egui::Align2::CENTER_CENTER, "Blub", 2.6 * s, Color32::from_rgb(232, 120, 30)));
    // The power light, and the labels printed on the panel.
    out.push(egui::Shape::circle_filled(map.p(pos2(25.2, 79.5)), 1.5 * s, Color32::from_rgb(120, 20, 10)));
    out.push(egui::Shape::circle_filled(map.p(pos2(25.2, 79.5)), 1.1 * s, Color32::from_rgb(236, 70, 40)));
    for (x, y, t) in [
        (25.2, 82.6, "ON/OFF"),
        (37.0, 78.5, "ENTER"),
        (45.0, 86.4, "MODES"),
        (54.0, 84.6, "L"),
        (60.0, 84.6, "R"),
        (65.0, 84.4, "METRONOME"),
        (76.4, 74.4, "TEMPO"),
    ] {
        out.push(text(ctx, map.p(pos2(x, y)), egui::Align2::CENTER_CENTER, t, 1.1 * s, INK));
    }
    // The hands printed over the hands button, the metronome's light and
    // the volume knob (not emulated).
    for (x, left, on) in [(54.0, true, lit.hands.0), (60.0, false, lit.hands.1)] {
        glove(&mut out, map, pos2(x, 80.4), left, on);
    }
    out.push(egui::Shape::circle_filled(map.p(pos2(85.0, 78.0)), 0.9 * s, Color32::from_rgb(40, 170, 160)));
    let knob = map.p(pos2(93.0, 80.5));
    out.push(egui::Shape::circle_filled(knob + vec2(0.0, 0.5 * s), 3.1 * s, Color32::from_rgb(170, 130, 10)));
    out.push(egui::Shape::circle_filled(knob, 3.0 * s, Color32::from_rgb(250, 205, 30)));
    out.push(egui::Shape::rect_filled(Rect::from_center_size(knob, vec2(5.4, 1.5) * s), 0.6 * s, Color32::from_rgb(232, 180, 10)));
    // The buttons.
    for (b, r) in button_rects() {
        let i = Input::Button(b);
        let on = (lit.pressed)(i);
        let r = map.r(r);
        let col = button_color(b);
        let face = if on { r.translate(vec2(0.0, 0.35 * s)) } else { r };
        let fill = if on { mix(col, LIT, 0.55) } else { col };
        let edge = mix(col, Color32::BLACK, 0.4);
        match b {
            Button::Enter | Button::CycleHands | Button::ChangeInstrument => {
                out.push(egui::Shape::circle_filled(r.center() + vec2(0.0, 0.4 * s), r.width() * 0.5, edge));
                out.push(egui::Shape::circle_filled(face.center(), r.width() * 0.5, fill));
                out.push(egui::Shape::circle_filled(face.center() - vec2(0.3, 0.4) * s, r.width() * 0.22, Color32::from_white_alpha(40)));
            }
            Button::Metronome => {
                let pyramid = |r: Rect| vec![pos2(r.center().x - r.width() * 0.16, r.min.y), pos2(r.center().x + r.width() * 0.16, r.min.y), r.right_bottom(), r.left_bottom()];
                out.push(egui::Shape::convex_polygon(pyramid(r.translate(vec2(0.0, 0.4 * s))), edge, Stroke::NONE));
                out.push(egui::Shape::convex_polygon(pyramid(face), fill, Stroke::NONE));
                let slot = Rect::from_center_size(face.center() + vec2(0.0, 0.4 * s), vec2(0.5 * s, face.height() * 0.5));
                out.push(egui::Shape::rect_filled(slot, 0.2 * s, edge));
            }
            _ => {
                let c = (r.width().min(r.height()) * 0.5) as u8;
                let corner = match b {
                    Button::ScrollUp => egui::CornerRadius { nw: c, ne: c, sw: 0, se: 0 },
                    Button::ScrollDown => egui::CornerRadius { nw: 0, ne: 0, sw: c, se: c },
                    Button::Home | Button::Pause => egui::CornerRadius::same(c),
                    _ => egui::CornerRadius::same((0.9 * s) as u8),
                };
                out.push(egui::Shape::rect_filled(r.translate(vec2(0.0, 0.4 * s)), corner, edge));
                out.push(egui::Shape::rect_filled(face, corner, fill));
                out.push(egui::Shape::rect_filled(
                    Rect::from_min_size(face.min + vec2(face.width() * 0.2, 0.3 * s), vec2(face.width() * 0.6, face.height() * 0.22)),
                    face.height() * 0.1,
                    Color32::from_white_alpha(45),
                ));
                button_mark(&mut out, ctx, b, face, s);
            }
        }
        if lit.show_caps
            && let Some(cap) = (lit.cap)(i)
        {
            // Beside the rocker and the tempo buttons (clear of their
            // arrows), else along the button's foot.
            let at = match b {
                Button::ScrollUp | Button::ScrollDown => face.left_center() - vec2(1.4 * s, 0.0),
                Button::TempoUp | Button::TempoDefault | Button::TempoDown => face.right_center() + vec2(1.4 * s, 0.0),
                _ => face.center_bottom(),
            };
            keycap(ctx, &mut out, at, &cap, 1.4 * s, on);
        }
    }
    // The keys: the white ones, then the black ones over them, and the
    // mint lip around the middle octave.
    out.push(egui::Shape::rect_filled(map.r(KEYBED.expand(0.8)), 1.2 * s, Color32::from_gray(8)));
    for black in [false, true] {
        for k in (0..KEYS).filter(|&k| is_black(k) == black) {
            let i = Input::Key(k as u8);
            let on = (lit.pressed)(i);
            let r = map.r(key_rect(k));
            let code = NOTE_COLOURS[k % 12];
            if black {
                out.push(egui::Shape::rect_filled(r.translate(vec2(0.3 * s, 0.6 * s)), 0.6 * s, Color32::from_black_alpha(120)));
                // A plain key (C#) lights dark grey; a striped one in its colour.
                let fill = match (on, code) {
                    (true, Some(c)) => mix(c, Color32::BLACK, 0.2),
                    (true, None) => Color32::from_gray(80),
                    (false, _) => Color32::from_gray(22),
                };
                out.push(egui::Shape::rect_filled(r, 0.6 * s, fill));
                if !on {
                    // A glint along the raised key's side, and its stripe.
                    let glint = Rect::from_min_size(r.min + vec2(r.width() * 0.12, 0.4 * s), vec2(r.width() * 0.16, r.height() * 0.8));
                    out.push(egui::Shape::rect_filled(glint, 0.3 * s, Color32::from_gray(58)));
                    if let Some(c) = code {
                        let stripe = Rect::from_center_size(pos2(r.center().x, r.min.y + r.height() * 0.42), vec2(r.width() * 0.32, r.height() * 0.42));
                        out.push(egui::Shape::rect_filled(stripe, 0.2 * s, c));
                    }
                }
            } else {
                let r = r.shrink2(vec2(0.18 * s, 0.0));
                let fill = if on { code.map_or(LIT, |c| mix(c, Color32::WHITE, 0.35)) } else { Color32::from_gray(246) };
                out.push(egui::Shape::rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: (0.8 * s) as u8, se: (0.8 * s) as u8 }, fill));
                out.push(egui::Shape::rect_filled(Rect::from_min_size(r.left_bottom() - vec2(0.0, 1.0 * s), vec2(r.width(), 1.0 * s)), 0.5 * s, Color32::from_black_alpha(26)));
                if let Some(c) = code {
                    let side = r.width() * 0.66;
                    let pad = Rect::from_center_size(pos2(r.center().x, r.max.y - 7.6 * s), vec2(side, side));
                    out.push(egui::Shape::rect_filled(pad, 0.3 * s, if on { mix(c, Color32::BLACK, 0.15) } else { c }));
                }
            }
            if lit.show_caps
                && let Some(cap) = (lit.cap)(i)
            {
                let at = pos2(r.center().x, r.max.y - if black { 2.0 } else { 3.2 } * s);
                keycap(ctx, &mut out, at, &cap, if black { 1.7 } else { 2.0 } * s, on);
            }
        }
    }
    let (left, right) = (key_rect(12).min.x + 0.3, key_rect(23).max.x - 0.3);
    let lip = (1.2 * s) as u8;
    out.push(egui::Shape::rect_filled(
        map.r(Rect::from_min_max(pos2(left, KEYBED.min.y - 2.2), pos2(right, KEYBED.min.y - 0.2))),
        egui::CornerRadius { nw: lip, ne: lip, sw: 0, se: 0 },
        MINT,
    ));
    out.push(egui::Shape::rect_filled(map.r(Rect::from_min_max(pos2(left, KEYBED.max.y + 0.4), pos2(right, KEYBED.max.y + 1.4))), 0.4 * s, MINT));
    out
}

/// A blue glove on the panel (one of the hands), its thumb inwards: lit
/// red while that hand plays.
fn glove(out: &mut Vec<egui::Shape>, map: &Map, c: Pos2, left: bool, on: bool) {
    let s = map.s();
    let thumb = if left { 1.0 } else { -1.0 };
    let (edge, fill) = if on {
        (Color32::from_rgb(150, 20, 20), Color32::from_rgb(240, 50, 40))
    } else {
        (Color32::from_rgb(35, 60, 140), Color32::from_rgb(70, 110, 215))
    };
    if on {
        out.push(egui::Shape::circle_filled(map.p(c), 3.6 * s, Color32::from_rgba_unmultiplied(255, 60, 40, 50)));
    }
    for (col, grow) in [(edge, 0.35), (fill, 0.0)] {
        let mut blob = |x: f32, y: f32, rx: f32, ry: f32| {
            out.push(egui::Shape::ellipse_filled(map.p(c + vec2(x, y)), vec2(rx + grow, ry + grow) * s, col));
        };
        blob(0.0, 0.7, 1.9, 1.7);
        for k in 0..4 {
            blob(-1.3 + 0.87 * k as f32, -1.1, 0.42, 1.0);
        }
        blob(1.9 * thumb, 0.3, 0.5, 0.75);
    }
}

/// The symbol printed on a button: the house on Home, arrows on the
/// rocker and the tempo buttons, the mode numbers, the pause bars.
fn button_mark(out: &mut Vec<egui::Shape>, ctx: &egui::Context, b: Button, face: Rect, s: f32) {
    let c = face.center();
    let white = Color32::WHITE;
    // Clockwise on screen, as egui fills convex polygons.
    let arrow = |up: bool| {
        let d = 0.85 * s;
        let points = if up {
            vec![c + vec2(0.0, -d), c + vec2(d * 1.1, d * 0.6), c + vec2(-d * 1.1, d * 0.6)]
        } else {
            vec![c + vec2(0.0, d), c + vec2(-d * 1.1, -d * 0.6), c + vec2(d * 1.1, -d * 0.6)]
        };
        egui::Shape::convex_polygon(points, white, Stroke::NONE)
    };
    match b {
        Button::Home => {
            out.push(egui::Shape::convex_polygon(vec![c + vec2(0.0, -1.3) * s, c + vec2(1.4, -0.1) * s, c + vec2(-1.4, -0.1) * s], white, Stroke::NONE));
            out.push(egui::Shape::rect_filled(Rect::from_min_max(c + vec2(-0.95, -0.2) * s, c + vec2(0.95, 1.2) * s), 0.1 * s, white));
            out.push(egui::Shape::rect_filled(Rect::from_min_max(c + vec2(-0.3, 0.3) * s, c + vec2(0.3, 1.2) * s), 0.0, button_color(b)));
        }
        Button::ScrollUp | Button::TempoUp => out.push(arrow(true)),
        Button::ScrollDown | Button::TempoDown => out.push(arrow(false)),
        Button::Pause => {
            for dx in [-0.55, 0.55] {
                out.push(egui::Shape::rect_filled(Rect::from_center_size(c + vec2(dx, 0.0) * s, vec2(0.5, 1.8) * s), 0.15 * s, white));
            }
        }
        Button::Display1 | Button::Display2 | Button::Display3 | Button::Display4 => {
            let n = match b {
                Button::Display1 => "1",
                Button::Display2 => "2",
                Button::Display3 => "3",
                _ => "4",
            };
            out.push(text(ctx, c, egui::Align2::CENTER_CENTER, n, face.height() * 0.72, Color32::from_gray(30)));
        }
        _ => {}
    }
}
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

// ----- The I Can Play Guitar -----
//
// Blub's own drawing of the guitar (no photographs or logos), after the
// cartridges' tutorial picture of it: a red electric guitar lying across
// under the TV, its neck to the left with the coloured fret buttons (all
// twelve frets on the outer strings, the first five on the four between),
// a white pickguard on the body with two round blue buttons (Home, Pause)
// and the arrow pad round Enter, and the six strings over the body, where
// they are plucked. The low E lies on top, as on a guitar held to play.

/// The guitar drawing's height (its width is W, as the piano's).
const GH: f32 = 138.0;
/// The picture on the TV (4:3).
const G_PICTURE: Rect = Rect { min: pos2(21.0, 4.0), max: pos2(109.0, 70.0) };
/// The guitar, all of it (the pointer's area).
const G_GUITAR: Rect = Rect { min: pos2(0.5, 76.0), max: pos2(129.0, 134.0) };
/// The neck (the fret buttons' board) and the frets' span along it.
const G_NECK: Rect = Rect { min: pos2(6.0, 89.0), max: pos2(81.0, 118.0) };
const G_FRETS: (f32, f32) = (8.0, 80.0);
/// The strings over the body: plucked between the pickup and the bridge.
const G_PICKUP: Rect = Rect { min: pos2(105.0, 89.0), max: pos2(108.5, 118.0) };
const G_BRIDGE: f32 = 124.0;

/// String `string`'s line across the neck and body (low E, string 5, on
/// top).
fn string_y(string: usize) -> f32 {
    G_NECK.min.y + 3.0 + (blub_piano::STRINGS - 1 - string) as f32 * 4.6
}

/// Fret `fret`'s button on string `string` (drawing units).
fn fret_rect(string: usize, fret: usize) -> Rect {
    let w = (G_FRETS.1 - G_FRETS.0) / blub_piano::FRETS as f32;
    Rect::from_center_size(pos2(G_FRETS.0 + (fret as f32 - 0.5) * w, string_y(string)), vec2(w * 0.72, 3.3))
}

/// Where string `string` is plucked.
fn string_rect(string: usize) -> Rect {
    Rect::from_min_max(pos2(G_PICKUP.max.x + 0.2, string_y(string) - 2.3), pos2(G_BRIDGE - 1.0, string_y(string) + 2.3))
}

/// The guitar's buttons: each one's place (centre and size).
fn guitar_button_rects() -> Vec<(GuitarButton, Rect)> {
    let at = |x: f32, y: f32, w: f32, h: f32| Rect::from_center_size(pos2(x, y), vec2(w, h));
    let (x, y, arm) = (97.5, 103.5, 4.4);
    vec![
        (GuitarButton::Home, at(87.0, 98.0, 5.2, 5.2)),
        (GuitarButton::Pause, at(87.0, 109.0, 5.2, 5.2)),
        (GuitarButton::Up, at(x, y - arm, 3.4, 3.6)),
        (GuitarButton::Down, at(x, y + arm, 3.4, 3.6)),
        (GuitarButton::Left, at(x - arm, y, 3.6, 3.4)),
        (GuitarButton::Right, at(x + arm, y, 3.6, 3.4)),
        (GuitarButton::Enter, at(x, y, 4.0, 4.0)),
    ]
}

/// The colour of each fret's buttons (fret 1 to 12), as the cartridges'
/// neck pictures colour them.
const FRET_COLOURS: [Color32; 12] = [
    Color32::from_rgb(170, 220, 70),
    Color32::from_rgb(200, 90, 200),
    Color32::from_rgb(240, 225, 140),
    Color32::from_rgb(80, 130, 220),
    Color32::from_rgb(220, 180, 150),
    Color32::from_rgb(60, 190, 120),
    Color32::from_rgb(235, 150, 220),
    Color32::from_rgb(235, 230, 205),
    Color32::from_rgb(140, 140, 140),
    Color32::from_rgb(110, 210, 240),
    Color32::from_rgb(235, 80, 90),
    Color32::from_rgb(245, 170, 70),
];

/// The I Can Play Guitar's drawing.
pub fn guitar_layout() -> Layout {
    Layout {
        aspect: W / GH,
        screen: Rect::from_min_max(pos2(G_PICTURE.min.x / W, G_PICTURE.min.y / GH), pos2(G_PICTURE.max.x / W, G_PICTURE.max.y / GH)),
        screen_aspect: blub_piano::SCREEN_W as f32 / blub_piano::SCREEN_H as f32,
        guitar: true,
    }
}

/// The guitar's input under `p` (screen points).
fn guitar_hit(map: &Map, p: Pos2) -> Option<Input> {
    for string in 0..blub_piano::STRINGS {
        for fret in 1..=blub_piano::FRETS {
            if blub_piano::has_fret(string, fret) && map.r(fret_rect(string, fret).expand(0.6)).contains(p) {
                return Some(Input::Fret(string as u8, fret as u8));
            }
        }
        if map.r(string_rect(string)).contains(p) {
            return Some(Input::String(string as u8));
        }
    }
    if map.r(G_PICKUP).contains(p) {
        return Some(Input::StrumAll);
    }
    guitar_button_rects().into_iter().find(|(_, r)| map.r(*r).contains(p)).map(|(b, _)| Input::Guitar(b))
}

fn paint_guitar(map: &Map, lit: &Lit, ctx: &egui::Context) -> Vec<egui::Shape> {
    let s = map.s();
    let mut out = Vec::new();
    // The TV, as the piano's: a frame round the picture on a short stand.
    let tv = map.r(G_PICTURE.expand(4.0));
    out.push(egui::Shape::rect_filled(map.r(Rect::from_center_size(pos2(W / 2.0, G_PICTURE.max.y + 5.6), vec2(26.0, 3.0))), 1.0 * s, Color32::from_gray(40)));
    out.push(egui::Shape::rect_stroke(tv.shrink(2.0 * s), 2.0 * s, Stroke::new(4.0 * s, Color32::from_gray(28)), egui::StrokeKind::Middle));
    out.push(egui::Shape::rect_stroke(tv, 4.0 * s, Stroke::new(0.6 * s, Color32::from_gray(70)), egui::StrokeKind::Inside));
    // The body: two round bouts and the waist between, a shadow under it.
    let red = Color32::from_rgb(214, 44, 40);
    let dark = mix(red, Color32::BLACK, 0.45);
    let bouts = [(pos2(100.0, 92.0), 12.5), (pos2(108.0, 112.0), 18.0)];
    for (c, r) in bouts {
        out.push(egui::Shape::circle_filled(map.p(c + vec2(0.0, 1.2)), r * s, Color32::from_black_alpha(80)));
    }
    for (c, r) in bouts {
        out.push(egui::Shape::circle_filled(map.p(c), r * s, dark));
    }
    out.push(egui::Shape::rect_filled(map.r(Rect::from_min_max(pos2(80.0, 88.0), pos2(112.0, 118.0))), 4.0 * s, dark));
    for (c, r) in bouts {
        out.push(egui::Shape::circle_filled(map.p(c - vec2(0.4, 0.6)), (r - 1.0) * s, red));
    }
    out.push(egui::Shape::rect_filled(map.r(Rect::from_min_max(pos2(81.0, 88.6), pos2(112.0, 117.2))), 3.6 * s, red));
    out.push(egui::Shape::ellipse_filled(map.p(pos2(104.0, 80.0)), vec2(8.0, 1.0) * s, Color32::from_white_alpha(40)));
    // The pickguard, white, under the buttons and the strings' ends.
    out.push(egui::Shape::rect_filled(map.r(Rect::from_min_max(pos2(82.0, 90.0), pos2(G_BRIDGE + 1.0, 117.0))), 5.0 * s, Color32::from_gray(244)));
    // The bridge, and the whammy bar from it.
    out.push(egui::Shape::rect_filled(map.r(Rect::from_min_max(pos2(G_BRIDGE - 0.6, 89.5), pos2(G_BRIDGE + 1.6, 117.5))), 0.6 * s, Color32::from_gray(170)));
    out.push(egui::Shape::line_segment([map.p(pos2(G_BRIDGE, 118.0)), map.p(pos2(112.0, 128.0))], Stroke::new(1.0 * s, Color32::from_gray(225))));
    // The neck: the headstock, the board and its fret wires.
    out.push(egui::Shape::rect_filled(map.r(Rect::from_min_max(pos2(0.8, G_NECK.min.y - 2.5), pos2(G_NECK.min.x + 0.5, G_NECK.max.y + 2.5))), 2.0 * s, Color32::from_rgb(40, 40, 46)));
    for k in 0..3 {
        for y in [G_NECK.min.y - 1.2, G_NECK.max.y + 1.2] {
            out.push(egui::Shape::circle_filled(map.p(pos2(2.0 + k as f32 * 1.6, y)), 0.6 * s, Color32::from_gray(190)));
        }
    }
    out.push(egui::Shape::rect_filled(map.r(G_NECK).translate(vec2(0.0, 0.8 * s)), 1.0 * s, Color32::from_black_alpha(80)));
    out.push(egui::Shape::rect_filled(map.r(G_NECK), 1.0 * s, Color32::from_rgb(250, 242, 215)));
    let fret_w = (G_FRETS.1 - G_FRETS.0) / blub_piano::FRETS as f32;
    for f in 0..=blub_piano::FRETS {
        let x = G_FRETS.0 + f as f32 * fret_w;
        out.push(egui::Shape::line_segment([map.p(pos2(x, G_NECK.min.y)), map.p(pos2(x, G_NECK.max.y))], Stroke::new(0.4 * s, Color32::from_gray(150))));
    }
    // The strings, along the neck and over the body (lit while plucked).
    for string in 0..blub_piano::STRINGS {
        let y = string_y(string);
        let width = (0.25 + 0.07 * string as f32) * s;
        let on = (lit.pressed)(Input::String(string as u8)) || (lit.pressed)(Input::StrumAll);
        let col = if on { LIT } else { Color32::from_gray(150) };
        out.push(egui::Shape::line_segment([map.p(pos2(G_NECK.min.x, y)), map.p(pos2(G_NECK.max.x, y))], Stroke::new(width, Color32::from_gray(160))));
        out.push(egui::Shape::line_segment([map.p(pos2(G_PICKUP.min.x, y)), map.p(pos2(G_BRIDGE, y))], Stroke::new(width + if on { 0.4 * s } else { 0.0 }, col)));
    }
    // The pickup: plucks all six.
    let all = (lit.pressed)(Input::StrumAll);
    let pickup = map.r(G_PICKUP);
    out.push(egui::Shape::rect_filled(pickup, 0.8 * s, if all { mix(Color32::from_rgb(190, 225, 245), LIT, 0.5) } else { Color32::from_rgb(190, 225, 245) }));
    out.push(egui::Shape::rect_stroke(pickup, 0.8 * s, Stroke::new(0.3 * s, Color32::from_rgb(80, 120, 150)), egui::StrokeKind::Inside));
    // The fret buttons.
    for string in 0..blub_piano::STRINGS {
        for fret in 1..=blub_piano::FRETS {
            if !blub_piano::has_fret(string, fret) {
                continue;
            }
            let i = Input::Fret(string as u8, fret as u8);
            let on = (lit.pressed)(i);
            let r = map.r(fret_rect(string, fret));
            let col = FRET_COLOURS[fret - 1];
            out.push(egui::Shape::rect_filled(r.translate(vec2(0.0, 0.35 * s)), r.height() * 0.5, mix(col, Color32::BLACK, 0.35)));
            let face = if on { r.translate(vec2(0.0, 0.3 * s)) } else { r };
            out.push(egui::Shape::rect_filled(face, r.height() * 0.5, if on { mix(col, LIT, 0.6) } else { col }));
            out.push(egui::Shape::rect_filled(
                Rect::from_min_size(face.min + vec2(face.width() * 0.25, 0.3 * s), vec2(face.width() * 0.5, face.height() * 0.25)),
                face.height() * 0.12,
                Color32::from_white_alpha(70),
            ));
            if lit.show_caps
                && let Some(cap) = (lit.cap)(i)
            {
                keycap(ctx, &mut out, face.center(), &cap, 1.5 * s, on);
            }
        }
    }
    // The buttons: Home and Pause round and blue, the arrows round Enter.
    for (b, r) in guitar_button_rects() {
        let i = Input::Guitar(b);
        let on = (lit.pressed)(i);
        let r = map.r(r);
        let face = if on { r.translate(vec2(0.0, 0.3 * s)) } else { r };
        let c = face.center();
        let white = Color32::WHITE;
        match b {
            GuitarButton::Home | GuitarButton::Pause | GuitarButton::Enter => {
                let col = if b == GuitarButton::Enter { Color32::from_rgb(50, 90, 200) } else { Color32::from_rgb(70, 140, 230) };
                out.push(egui::Shape::circle_filled(r.center() + vec2(0.0, 0.4 * s), r.width() * 0.5, mix(col, Color32::BLACK, 0.4)));
                out.push(egui::Shape::circle_filled(c, r.width() * 0.5, if on { mix(col, LIT, 0.55) } else { col }));
                out.push(egui::Shape::circle_filled(c - vec2(0.4, 0.5) * s, r.width() * 0.18, Color32::from_white_alpha(50)));
                if b == GuitarButton::Home {
                    out.push(egui::Shape::convex_polygon(vec![c + vec2(0.0, -1.2) * s, c + vec2(1.3, -0.1) * s, c + vec2(-1.3, -0.1) * s], white, Stroke::NONE));
                    out.push(egui::Shape::rect_filled(Rect::from_min_max(c + vec2(-0.85, -0.2) * s, c + vec2(0.85, 1.1) * s), 0.1 * s, white));
                } else if b == GuitarButton::Pause {
                    for dx in [-0.5, 0.5] {
                        out.push(egui::Shape::rect_filled(Rect::from_center_size(c + vec2(dx, 0.0) * s, vec2(0.45, 1.7) * s), 0.15 * s, white));
                    }
                }
            }
            _ => {
                // An arrow, yellow up and down, purple left and right (as
                // the cartridges draw the pad), pointing out.
                let (col, dir) = match b {
                    GuitarButton::Up => (Color32::from_rgb(240, 190, 40), vec2(0.0, -1.0)),
                    GuitarButton::Down => (Color32::from_rgb(240, 190, 40), vec2(0.0, 1.0)),
                    GuitarButton::Left => (Color32::from_rgb(150, 60, 190), vec2(-1.0, 0.0)),
                    _ => (Color32::from_rgb(150, 60, 190), vec2(1.0, 0.0)),
                };
                let side = vec2(-dir.y, dir.x);
                let tri = |c: Pos2, k: f32| {
                    let mut p = vec![c + dir * 1.7 * k * s, c - dir * 1.3 * k * s + side * 1.7 * k * s, c - dir * 1.3 * k * s - side * 1.7 * k * s];
                    // Clockwise on screen, as egui fills convex polygons.
                    if (p[1] - p[0]).x * (p[2] - p[0]).y - (p[1] - p[0]).y * (p[2] - p[0]).x < 0.0 {
                        p.swap(1, 2);
                    }
                    p
                };
                out.push(egui::Shape::convex_polygon(tri(r.center() + vec2(0.0, 0.4 * s), 1.0), mix(col, Color32::BLACK, 0.4), Stroke::NONE));
                out.push(egui::Shape::convex_polygon(tri(c, 1.0), if on { mix(col, LIT, 0.55) } else { col }, Stroke::NONE));
            }
        }
        if lit.show_caps
            && let Some(cap) = (lit.cap)(i)
        {
            let at = match b {
                GuitarButton::Home | GuitarButton::Pause => face.left_center() - vec2(2.2 * s, 0.0),
                GuitarButton::Up => face.center_top() - vec2(0.0, 1.2 * s),
                GuitarButton::Down => face.center_bottom() + vec2(0.0, 1.2 * s),
                _ => face.center_bottom() + vec2(0.0, 0.2 * s),
            };
            if !matches!(b, GuitarButton::Left | GuitarButton::Right | GuitarButton::Enter) {
                keycap(ctx, &mut out, at, &cap, 1.4 * s, on);
            }
        }
    }
    // The strings' keys, beside the bridge; the pickup's (all strings)
    // under it.
    if lit.show_caps {
        for string in 0..blub_piano::STRINGS {
            let i = Input::String(string as u8);
            if let Some(cap) = (lit.cap)(i) {
                keycap(ctx, &mut out, map.p(pos2(G_BRIDGE + 3.6, string_y(string))), &cap, 1.4 * s, (lit.pressed)(i));
            }
        }
        if let Some(cap) = (lit.cap)(Input::StrumAll) {
            keycap(ctx, &mut out, map.p(pos2(G_PICKUP.center().x, G_PICKUP.max.y + 2.2)), &cap, 1.4 * s, all);
        }
    }
    for (x, y, t) in [(87.0, 101.6, "HOME"), (87.0, 112.6, "PAUSE")] {
        out.push(text(ctx, map.p(pos2(x, y)), egui::Align2::CENTER_CENTER, t, 1.1 * s, INK));
    }
    out.push(text(ctx, map.p(pos2(100.0, 115.0)), egui::Align2::CENTER_CENTER, "Blub", 2.2 * s, Color32::from_rgb(232, 120, 30)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_tile_the_keybed() {
        assert_eq!(WHITE_KEYS, 21);
        let last = key_rect(KEYS - 1);
        assert!((last.max.x - KEYBED.max.x).abs() < 0.01, "the top E ends the keybed");
        // The game's keyboard lies right over the keybed.
        let px = PICTURE.width() / blub_piano::SCREEN_W as f32;
        assert!((PICTURE.min.x + GAME_KEYS.0 * px - (PIANO_AT.x + KEYBED.min.x)).abs() < 0.01);
        assert!((PICTURE.min.x + GAME_KEYS.1 * px - (PIANO_AT.x + KEYBED.max.x)).abs() < 0.01);
        assert!(PICTURE.max.y + 4.0 < PIANO_AT.y + CASE.min.y && PIANO_AT.y + CASE.max.y < H, "the TV, then the piano");
        // Black keys lie between white ones and above the keybed's middle.
        for k in (0..KEYS).filter(|&k| is_black(k)) {
            let r = key_rect(k);
            assert!(r.max.y < KEYBED.max.y && r.min.x > KEYBED.min.x && r.max.x < KEYBED.max.x, "{k}");
        }
    }

    #[test]
    fn the_pointer_finds_keys_and_buttons() {
        let map = Map { shell: Rect::from_min_size(Pos2::ZERO, vec2(W * 2.0, H * 2.0)) };
        let pm = piano_map(&map);
        // Low on a white key: that key; high between F and G: F#.
        let f = key_rect(0);
        assert_eq!(hit(&map, pm.p(pos2(f.center().x, f.max.y - 1.0))), Some(Input::Key(0)));
        let fs = key_rect(1);
        assert_eq!(hit(&map, pm.p(fs.center())), Some(Input::Key(1)));
        for (b, r) in button_rects() {
            assert_eq!(hit(&map, pm.p(r.center())), Some(Input::Button(b)), "{b:?}");
            assert!(PANEL.contains_rect(r), "{b:?} lies on the panel");
        }
        assert_eq!(hit(&map, map.p(PICTURE.center())), None, "the picture is not a control");
        for l in [layout(false), layout(true)] {
            assert!((l.screen.width() * l.aspect / l.screen.height() - 4.0 / 3.0).abs() < 0.01, "the picture keeps its shape");
        }
    }

    #[test]
    fn the_pointer_finds_the_guitar_s_controls() {
        let map = Map { shell: Rect::from_min_size(Pos2::ZERO, vec2(W * 2.0, GH * 2.0)) };
        let mut frets = 0;
        for string in 0..blub_piano::STRINGS {
            for fret in 1..=blub_piano::FRETS {
                if blub_piano::has_fret(string, fret) {
                    frets += 1;
                    let r = fret_rect(string, fret);
                    assert!(G_NECK.contains_rect(r));
                    assert_eq!(guitar_hit(&map, map.p(r.center())), Some(Input::Fret(string as u8, fret as u8)));
                }
            }
            assert_eq!(guitar_hit(&map, map.p(string_rect(string).center())), Some(Input::String(string as u8)));
        }
        assert_eq!(frets, blub_piano::FRET_BUTTONS);
        assert_eq!(guitar_hit(&map, map.p(G_PICKUP.center())), Some(Input::StrumAll));
        for (b, r) in guitar_button_rects() {
            assert_eq!(guitar_hit(&map, map.p(r.center())), Some(Input::Guitar(b)), "{b:?}");
            assert!(G_GUITAR.contains_rect(r));
        }
        assert_eq!(guitar_hit(&map, map.p(G_PICTURE.center())), None, "the picture is not a control");
        assert!(G_PICTURE.max.y + 4.0 < G_GUITAR.min.y && G_GUITAR.max.y < GH, "the TV, then the guitar");
    }
}
