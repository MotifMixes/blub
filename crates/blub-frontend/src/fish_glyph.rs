// Blub's fish: the tropical fish emoji from Noto Emoji (SIL Open Font License,
// shipped with egui), drawn in Blub blue. Shared with blub-desktop's
// build.rs, which `include!`s this file to make the .exe icon.

/// Blub blue.
#[allow(dead_code)]
pub const FISH_BLUE: [u8; 3] = [0x3D, 0x8F, 0xF0];

/// RGBA pixels of the fish, `size` pixels square, on a transparent
/// background.
pub fn fish_glyph_rgba(size: u32) -> Vec<u8> {
    use ab_glyph::{Font, FontRef, point};
    let font = FontRef::try_from_slice(epaint_default_fonts::NOTO_EMOJI_REGULAR).expect("the emoji font");
    let id = font.glyph_id('\u{1F420}');
    let unit = font.outline_glyph(id.with_scale(100.0)).expect("a fish in the emoji font").px_bounds();
    let side = size as f32;
    let margin = (side * 0.04).max(1.0);
    let scale = 100.0 * (side - 2.0 * margin) / unit.width().max(unit.height());
    let glyph = font.outline_glyph(id.with_scale_and_position(scale, point(0.0, 0.0))).expect("fish outline");
    let b = glyph.px_bounds();
    let (ox, oy) = (((side - b.width()) / 2.0).round() as i32, ((side - b.height()) / 2.0).round() as i32);
    let n = size as usize;
    let mut cover = vec![0f32; n * n];
    glyph.draw(|x, y, c| {
        let (x, y) = (x as i32 + ox, y as i32 + oy);
        if (0..size as i32).contains(&x) && (0..size as i32).contains(&y) {
            cover[y as usize * n + x as usize] = c;
        }
    });
    let mut out = Vec::with_capacity(n * n * 4);
    for c in cover {
        let [r, g, b] = FISH_BLUE;
        out.extend_from_slice(&[r, g, b, (c.clamp(0.0, 1.0) * 255.0).round() as u8]);
    }
    out
}
