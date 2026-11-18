//! Blub's window and taskbar icon: the fish from the emoji font egui
//! ships, in Blub blue (see fish_glyph.rs, shared with the .exe icon).

#[path = "fish_glyph.rs"]
mod fish_glyph;
#[allow(unused_imports)]
pub use fish_glyph::{FISH_BLUE, fish_glyph_rgba};

fn icon(size: u32) -> Option<winit::window::Icon> {
    winit::window::Icon::from_rgba(fish_glyph_rgba(size), size, size).ok()
}

/// Title-bar icon (Windows scales it down to 16-32 px).
pub fn window_icon() -> Option<winit::window::Icon> {
    icon(64)
}

/// Taskbar / Alt-Tab icon (Windows' "big" icon).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn taskbar_icon() -> Option<winit::window::Icon> {
    icon(256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_fills_a_square_with_transparent_corners() {
        for size in [16, 64, 256] {
            let rgba = fish_glyph_rgba(size);
            assert_eq!(rgba.len(), (size * size * 4) as usize);
            assert_eq!(rgba[3], 0, "corner is transparent");
            let solid = rgba.chunks_exact(4).filter(|p| p[3] >= 128).count();
            assert!(solid > (size * size / 10) as usize, "a fish at {size}: {solid} pixels");
            assert!(rgba.chunks_exact(4).all(|p| p[..3] == FISH_BLUE), "blue throughout");
        }
        assert!(window_icon().is_some() && taskbar_icon().is_some());
    }
}
