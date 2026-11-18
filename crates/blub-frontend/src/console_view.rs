//! Console view: the emulated picture on a drawn TV above a drawing of the
//! I Can Play Piano itself, its keyboard and buttons clickable. A
//! presentation feature only: it turns pointer input into the same key and
//! button presses the PC keyboard makes and never touches the emulation.
//!
//! Everything is drawn with vector shapes (Blub's own artwork, no
//! photographs or logos): a blue case with a white panel of buttons and
//! the 36 keys, F to E, the black keys raised. A key lights up while it is
//! held from any source (PC keys, MIDI, the mouse). The mouse plays a key
//! when pressed on it, lets go when released, and slides from key to key
//! while held. The proportions are Blub's (no measured drawing).

use crate::piano::{Input, is_black};
use blub_piano::{Button, KEYS};
use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};

/// The drawing's size in its own units (`U`), and where things are in it.
const W: f32 = 120.0;
const H: f32 = 122.0;
/// The picture on the TV (4:3).
const PICTURE: Rect = Rect { min: pos2(22.0, 5.0), max: pos2(98.0, 62.0) };
/// The piano's case, its button panel and its keys.
const CASE: Rect = Rect { min: pos2(1.0, 73.0), max: pos2(119.0, 121.0) };
const PANEL: Rect = Rect { min: pos2(5.0, 76.0), max: pos2(115.0, 88.0) };
const KEYBED: Rect = Rect { min: pos2(4.0, 91.0), max: pos2(116.0, 118.0) };
/// White keys on the keyboard (F to E, three octaves).
const WHITE_KEYS: usize = KEYS * 7 / 12;

const BLUE: Color32 = Color32::from_rgb(52, 120, 214);
const BLUE_DARK: Color32 = Color32::from_rgb(28, 70, 140);
const PANEL_WHITE: Color32 = Color32::from_rgb(234, 240, 248);
/// A held key's or button's light.
const LIT: Color32 = Color32::from_rgb(90, 190, 255);

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
}

/// The I Can Play Piano's drawing.
pub fn layout() -> Layout {
    Layout {
        aspect: W / H,
        screen: Rect::from_min_max(pos2(PICTURE.min.x / W, PICTURE.min.y / H), pos2(PICTURE.max.x / W, PICTURE.max.y / H)),
        screen_aspect: blub_piano::SCREEN_W as f32 / blub_piano::SCREEN_H as f32,
    }
}

/// What the view shows on its controls.
pub struct Lit<'a> {
    pub pressed: &'a dyn Fn(Input) -> bool,
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

/// The buttons on the panel, left to right, with their labels.
const BUTTONS: [(Button, &str); 15] = [
    (Button::Display1, "1"),
    (Button::Display2, "2"),
    (Button::Display3, "3"),
    (Button::Display4, "4"),
    (Button::ChangeInstrument, "Instrument"),
    (Button::CycleHands, "Hands"),
    (Button::Metronome, "Metronome"),
    (Button::Pause, "Pause"),
    (Button::TempoDown, "Tempo −"),
    (Button::TempoDefault, "Tempo ="),
    (Button::TempoUp, "Tempo +"),
    // (Drawn as arrows; the label sizes them.)
    (Button::ScrollUp, "^"),
    (Button::ScrollDown, "v"),
    (Button::Home, "Home"),
    (Button::Enter, "Enter"),
];

/// Gap between buttons, and the space a label's character takes
/// (drawing units).
const GAP: f32 = 1.2;
const CHAR: f32 = 1.45;

/// Each button's place on the panel (drawing units): as wide as its label,
/// all of them filling the panel.
fn button_rects() -> Vec<(Button, &'static str, Rect)> {
    let want: Vec<f32> = BUTTONS.iter().map(|(_, l)| 4.0 + CHAR * l.chars().count() as f32).collect();
    let room = PANEL.width() - GAP * (BUTTONS.len() + 1) as f32;
    let scale = room / want.iter().sum::<f32>();
    let mut x = PANEL.min.x + GAP;
    let h = PANEL.height() - 2.0 * GAP;
    BUTTONS
        .iter()
        .zip(want)
        .map(|(&(b, l), w)| {
            let r = Rect::from_min_size(pos2(x, PANEL.min.y + GAP), vec2(w * scale, h));
            x += w * scale + GAP;
            (b, l, r)
        })
        .collect()
}

/// The button's own colour (the display buttons each have one).
fn button_color(b: Button) -> Color32 {
    match b {
        Button::Display1 => Color32::from_rgb(250, 200, 40),
        Button::Display2 => Color32::from_rgb(235, 80, 70),
        Button::Display3 => Color32::from_rgb(90, 190, 90),
        Button::Display4 => Color32::from_rgb(160, 110, 220),
        Button::Enter => Color32::from_rgb(250, 140, 40),
        Button::Home => Color32::from_rgb(70, 160, 240),
        _ => Color32::from_rgb(196, 206, 222),
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

/// The input under `p` (screen points), black keys first (they lie over
/// the white ones).
fn hit(map: &Map, p: Pos2) -> Option<Input> {
    let black = (0..KEYS).filter(|&k| is_black(k)).find(|&k| map.r(key_rect(k)).contains(p));
    let white = || (0..KEYS).filter(|&k| !is_black(k)).find(|&k| map.r(key_rect(k)).contains(p));
    if let Some(k) = black.or_else(white) {
        return Some(Input::Key(k as u8));
    }
    button_rects().into_iter().find(|(_, _, r)| map.r(*r).contains(p)).map(|(b, _, _)| Input::Button(b))
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
            // The whole drawing scaled down so its picture is a whole
            // multiple of the console's.
            let ppp = ui.ctx().pixels_per_point();
            let screen = place(shell, layout.screen);
            let n = (screen.width() * ppp / blub_piano::SCREEN_W as f32).floor();
            if n >= 1.0 {
                let fit = n * blub_piano::SCREEN_W as f32 / ppp;
                shell = Rect::from_center_size(shell.center(), shell.size() * (fit / screen.width()));
            }
        }
        let screen = place(shell, layout.screen);
        let map = Map { shell };
        ui.painter().set(under, egui::Shape::Vec(paint(&map, lit, ui.ctx())));
        // The case is one control: whatever is under the pointer while it
        // is held down on it is played (sliding from key to key).
        let case = map.r(CASE);
        let resp = ui.interact(case, egui::Id::new("piano console view"), egui::Sense::click_and_drag());
        let pos = ui.ctx().pointer_latest_pos();
        let now = if resp.is_pointer_button_down_on() { pos.and_then(|p| hit(&map, p)) } else { None };
        if resp.hovered()
            && let Some(i) = pos.and_then(|p| hit(&map, p))
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
    (paint(&Map { shell }, lit, ctx), screen)
}

/// A recording's size for the console view `height` pixels tall (even
/// sides, as video encoders want).
pub fn capture_size(layout: &Layout, height: u32) -> (u32, u32) {
    (((height as f32 * layout.aspect) as u32) & !1, height & !1)
}

/// The font size for `s` at most `size`, smaller if it would be wider than
/// `width`.
fn fit_text(ctx: &egui::Context, s: &str, size: f32, width: f32) -> f32 {
    let w = ctx.fonts_mut(|f| f.layout_no_wrap(s.to_string(), egui::FontId::proportional(size.max(1.0)), Color32::WHITE).size().x);
    if w > width && w > 0.0 { size * width / w } else { size }
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
    // The TV: a dark set on a short stand, the picture inside it.
    let tv = map.r(PICTURE.expand(4.0));
    out.push(egui::Shape::rect_filled(map.r(Rect::from_center_size(pos2(60.0, 67.5), vec2(26.0, 3.0))), 1.0 * s, Color32::from_gray(40)));
    out.push(egui::Shape::rect_filled(tv.translate(vec2(0.0, 1.0 * s)), 4.0 * s, Color32::from_black_alpha(70)));
    out.push(egui::Shape::rect_filled(tv, 4.0 * s, Color32::from_gray(28)));
    out.push(egui::Shape::rect_stroke(tv, 4.0 * s, Stroke::new(0.6 * s, Color32::from_gray(70)), egui::StrokeKind::Inside));
    out.push(egui::Shape::rect_filled(map.r(PICTURE), 0.5 * s, Color32::BLACK));
    // The case.
    let case = map.r(CASE);
    out.push(egui::Shape::rect_filled(case.translate(vec2(0.0, 1.2 * s)), 6.0 * s, Color32::from_black_alpha(80)));
    out.push(egui::Shape::rect_filled(case, 6.0 * s, BLUE_DARK));
    out.push(egui::Shape::rect_filled(case.shrink(0.8 * s), 5.5 * s, BLUE));
    out.push(egui::Shape::ellipse_filled(map.p(pos2(60.0, 75.0)), vec2(44.0, 1.6) * s, Color32::from_white_alpha(40)));
    // The button panel.
    out.push(egui::Shape::rect_filled(map.r(PANEL), 2.5 * s, PANEL_WHITE));
    for (b, label, r) in button_rects() {
        let i = Input::Button(b);
        let on = (lit.pressed)(i);
        let r = map.r(r);
        let col = button_color(b);
        out.push(egui::Shape::rect_filled(r.translate(vec2(0.0, 0.5 * s)), r.height() * 0.4, mix(col, Color32::BLACK, 0.35)));
        let face = if on { r.translate(vec2(0.0, 0.4 * s)) } else { r };
        out.push(egui::Shape::rect_filled(face, r.height() * 0.4, if on { LIT } else { col }));
        let ink = if matches!(b, Button::Display1 | Button::Display3) || !on && col.r() > 180 && col.g() > 180 { Color32::from_gray(30) } else { Color32::WHITE };
        if matches!(b, Button::ScrollUp | Button::ScrollDown) {
            let (c, d) = (face.center(), face.width().min(face.height()) * 0.28);
            let tip = if b == Button::ScrollUp { -d } else { d };
            let tri = vec![c + vec2(0.0, tip), c + vec2(-d, -tip * 0.8), c + vec2(d, -tip * 0.8)];
            out.push(egui::Shape::convex_polygon(tri, ink, Stroke::NONE));
        } else {
            let size = fit_text(ctx, label, r.height() * 0.42, face.width() * 0.86);
            out.push(text(ctx, face.center(), egui::Align2::CENTER_CENTER, label, size, ink));
        }
        if lit.show_caps
            && let Some(cap) = (lit.cap)(i)
        {
            keycap(ctx, &mut out, pos2(face.center().x, face.max.y + 1.4 * s), &cap, 2.0 * s, on);
        }
    }
    // The keys: the white ones, then the black ones over them.
    out.push(egui::Shape::rect_filled(map.r(KEYBED.expand(0.8)), 1.2 * s, Color32::from_rgb(20, 40, 80)));
    for black in [false, true] {
        for k in (0..KEYS).filter(|&k| is_black(k) == black) {
            let i = Input::Key(k as u8);
            let on = (lit.pressed)(i);
            let r = map.r(key_rect(k));
            if black {
                out.push(egui::Shape::rect_filled(r.translate(vec2(0.3 * s, 0.6 * s)), 0.6 * s, Color32::from_black_alpha(120)));
                let fill = if on { mix(LIT, Color32::BLACK, 0.25) } else { Color32::from_gray(26) };
                out.push(egui::Shape::rect_filled(r, 0.6 * s, fill));
                if !on {
                    // A glint along the raised key's top.
                    let top = Rect::from_min_size(r.min + vec2(r.width() * 0.2, 0.4 * s), vec2(r.width() * 0.6, r.height() * 0.55));
                    out.push(egui::Shape::rect_filled(top, 0.4 * s, Color32::from_gray(52)));
                }
            } else {
                let r = r.shrink2(vec2(0.18 * s, 0.0));
                let fill = if on { LIT } else { Color32::from_gray(248) };
                out.push(egui::Shape::rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: (0.8 * s) as u8, se: (0.8 * s) as u8 }, fill));
                out.push(egui::Shape::rect_filled(Rect::from_min_size(r.left_bottom() - vec2(0.0, 1.0 * s), vec2(r.width(), 1.0 * s)), 0.5 * s, Color32::from_black_alpha(26)));
            }
            if lit.show_caps
                && let Some(cap) = (lit.cap)(i)
            {
                let at = pos2(r.center().x, r.max.y - if black { 2.0 } else { 3.2 } * s);
                keycap(ctx, &mut out, at, &cap, if black { 1.7 } else { 2.0 } * s, on);
            }
        }
    }
    out
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_tile_the_keybed() {
        assert_eq!(WHITE_KEYS, 21);
        let last = key_rect(KEYS - 1);
        assert!((last.max.x - KEYBED.max.x).abs() < 0.01, "the top E ends the keybed");
        // Black keys lie between white ones and above the keybed's middle.
        for k in (0..KEYS).filter(|&k| is_black(k)) {
            let r = key_rect(k);
            assert!(r.max.y < KEYBED.max.y && r.min.x > KEYBED.min.x && r.max.x < KEYBED.max.x, "{k}");
        }
    }

    #[test]
    fn the_pointer_finds_keys_and_buttons() {
        let map = Map { shell: Rect::from_min_size(Pos2::ZERO, vec2(W * 2.0, H * 2.0)) };
        // Low on a white key: that key; high between F and G: F#.
        let f = key_rect(0);
        assert_eq!(hit(&map, map.p(pos2(f.center().x, f.max.y - 1.0))), Some(Input::Key(0)));
        let fs = key_rect(1);
        assert_eq!(hit(&map, map.p(fs.center())), Some(Input::Key(1)));
        let (b, _, r) = button_rects()[14];
        assert_eq!((b, hit(&map, map.p(r.center()))), (Button::Enter, Some(Input::Button(Button::Enter))));
        assert!(r.max.x <= PANEL.max.x + 0.01, "the buttons fit the panel");
        assert_eq!(hit(&map, map.p(PICTURE.center())), None, "the picture is not a control");
        let l = layout();
        assert!((l.screen.width() * l.aspect / l.screen.height() - 4.0 / 3.0).abs() < 0.01, "the picture keeps its shape");
    }
}
