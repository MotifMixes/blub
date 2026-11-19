//! The consoles' icons: each device drawn plain with egui shapes (original
//! art, no logos, wordmarks or pictures from firmware), all in one style:
//! the device seen from the front on a soft tile, a highlight along its top,
//! its own colours. One place for the library's cards, its console chips
//! and anything else that shows a console (`paint`, `color`, `name`).
use crate::library::Hardware;

type C = egui::Color32;

/// A console's own colour (its chip in the library).
pub fn color(hw: Hardware) -> C {
    match hw {
        Hardware::Piano => C::from_rgb(60, 140, 230),
    }
}

/// A console's short name (its chip).
pub fn name(hw: Hardware) -> &'static str {
    match hw {
        Hardware::Piano => "Piano",
    }
}

fn mix(a: C, b: C, t: f32) -> C {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    C::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn button(p: &egui::Painter, c: egui::Pos2, r: f32, col: C) {
    p.circle_filled(c + egui::vec2(0.0, r * 0.15), r, mix(col, C::BLACK, 0.4));
    p.circle_filled(c, r, col);
    p.circle_filled(c - egui::vec2(r * 0.3, r * 0.3), r * 0.35, C::from_white_alpha(70));
}

/// The tile behind the device: a soft panel that reads on dark and light
/// backgrounds alike.
fn tile(p: &egui::Painter, rect: egui::Rect, radius: f32, hw: Hardware) {
    let tint = color(hw);
    p.rect_filled(rect, radius, mix(C::from_rgb(34, 38, 44), tint, 0.18));
    let glow = egui::Rect::from_center_size(rect.center() - egui::vec2(0.0, rect.height() * 0.08), rect.size() * 0.8);
    p.rect_filled(glow, glow.width() * 0.5, mix(C::from_rgb(40, 44, 50), tint, 0.28).gamma_multiply(0.6));
}

/// Draws `hw`'s icon filling `rect` (square best), on its tile.
pub fn paint(p: &egui::Painter, rect: egui::Rect, radius: f32, hw: Hardware) {
    tile(p, rect, radius, hw);
    paint_device(p, rect, hw);
}

/// The device alone (no tile), for chips.
pub fn paint_device(p: &egui::Painter, rect: egui::Rect, hw: Hardware) {
    let u = rect.width().min(rect.height()) / 100.0;
    let c = rect.center();
    let v = |x: f32, y: f32| egui::vec2(x * u, y * u);
    let at = |x: f32, y: f32| c + v(x, y);
    let rr = |x: f32, y: f32, w: f32, h: f32| egui::Rect::from_center_size(at(x, y), v(w, h));
    match hw {
        // I Can Play Piano: a wide glossy black keyboard seen from the
        // front, a silver-and-purple speaker at each end of its top, a pale
        // panel of coloured buttons between them, and below it the keys:
        // white keys with colour-coded pads, black keys raised between them.
        Hardware::Piano => {
            let case = rr(0.0, 2.0, 96.0, 52.0);
            p.rect_filled(case.translate(v(0.0, 3.0)), 10.0 * u, C::from_black_alpha(45));
            p.rect_filled(case, 10.0 * u, C::from_gray(60));
            p.rect_filled(case.shrink(1.4 * u), 9.0 * u, C::from_rgb(20, 20, 24));
            p.add(egui::Shape::ellipse_filled(at(0.0, -21.0), v(34.0, 1.6), C::from_white_alpha(30)));
            for x in [-38.0, 38.0] {
                p.rect_filled(rr(x, -13.0, 15.0, 14.0), 3.0 * u, C::from_gray(196));
                p.add(egui::Shape::ellipse_filled(at(x, -13.0), v(6.0, 5.6), C::from_rgb(92, 70, 205)));
            }
            // The button panel.
            p.rect_filled(rr(0.0, -13.0, 58.0, 13.0), 3.0 * u, C::from_rgb(228, 230, 244));
            for (i, col) in [C::from_rgb(40, 175, 170), C::from_rgb(245, 205, 40), C::from_rgb(245, 150, 40), C::from_rgb(232, 76, 48), C::from_rgb(70, 110, 215)].into_iter().enumerate() {
                button(p, at(-22.0 + i as f32 * 9.5, -13.0), 2.8 * u, col);
            }
            button(p, at(23.0, -13.0), 3.6 * u, C::from_rgb(250, 205, 30));
            // The keys: 12 white keys (F to C), black keys over them.
            let keys = rr(0.0, 10.0, 84.0, 26.0);
            p.rect_filled(keys.expand(1.0 * u), 2.5 * u, C::from_gray(8));
            let w = keys.width() / 12.0;
            let pads = [Some(C::from_rgb(110, 200, 70)), Some(C::from_rgb(245, 215, 40)), Some(C::from_rgb(214, 170, 140)), Some(C::from_rgb(220, 120, 210)), None, Some(C::from_rgb(70, 170, 240)), Some(C::from_rgb(250, 160, 30))];
            for i in 0..12 {
                let r = egui::Rect::from_min_size(keys.min + egui::vec2(i as f32 * w + 0.4 * u, 0.0), egui::vec2(w - 0.8 * u, keys.height()));
                p.rect_filled(r, 1.2 * u, C::from_gray(246));
                if let Some(col) = pads[i % 7] {
                    p.rect_filled(egui::Rect::from_center_size(egui::pos2(r.center().x, r.max.y - 5.0 * u), egui::vec2(w * 0.6, w * 0.6)), 0.6 * u, col);
                }
            }
            // Black keys after the white keys F, G, A, C and D (the white
            // keys run F to C).
            for i in [0, 1, 2, 4, 5, 7, 8, 9] {
                let x = keys.min.x + (i + 1) as f32 * w;
                let r = egui::Rect::from_center_size(egui::pos2(x, keys.min.y + keys.height() * 0.3), egui::vec2(w * 0.6, keys.height() * 0.6));
                p.rect_filled(r, 1.0 * u, C::from_gray(30));
            }
        }
    }
}

/// The All tab's console filter (a chip clicked), kept in egui's memory.
pub fn filter_id() -> egui::Id {
    egui::Id::new("blub-console-filter")
}

/// A small chip: the console's device and name, in its colour.
pub fn chip(p: &egui::Painter, at: egui::Pos2, hw: Hardware) -> egui::Rect {
    let col = color(hw);
    let g = p.layout_no_wrap(name(hw).to_string(), egui::FontId::proportional(11.0), C::WHITE);
    let icon = 18.0;
    let r = egui::Rect::from_min_size(at, egui::vec2(icon + g.size().x + 12.0, g.size().y.max(14.0) + 6.0));
    p.rect_filled(r, 6, mix(C::from_rgb(20, 22, 26), col, 0.22).gamma_multiply(0.94));
    p.rect_stroke(r, 6, egui::Stroke::new(1.0, col), egui::StrokeKind::Inside);
    paint_device(p, egui::Rect::from_center_size(egui::pos2(r.min.x + 3.0 + icon / 2.0, r.center().y), egui::vec2(icon, icon)), hw);
    p.galley(egui::pos2(r.min.x + icon + 7.0, r.center().y - g.size().y / 2.0), g, C::WHITE);
    r
}
