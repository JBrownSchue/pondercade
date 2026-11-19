use crate::theme::*;
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, Widget,
    pos2, vec2,
};

/// Rounded pill button with a hover animation.
pub struct Pill {
    text: String,
    color: Option<Color32>,
    selected: bool,
}

impl Pill {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), color: None, selected: false }
    }
    pub fn primary(text: impl Into<String>, color: Color32) -> Self {
        Self { text: text.into(), color: Some(color), selected: false }
    }
    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }
}

impl Widget for Pill {
    fn ui(self, ui: &mut Ui) -> Response {
        let font = FontId::proportional(14.5);
        let galley = ui.painter().layout_no_wrap(self.text, font, TEXT);
        let size = galley.size() + egui::vec2(30.0, 16.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        let (base, hi) = match (self.color, self.selected) {
            (Some(c), _) => (c, lerp(c, Color32::WHITE, 0.18)),
            (None, true) => (ACCENT, lerp(ACCENT, Color32::WHITE, 0.18)),
            (None, false) => (CARD, CARD_HI),
        };
        let r = rect.expand(h * 1.5);
        if self.color.is_some() {
            ui.painter().rect_filled(r.translate(egui::vec2(0.0, 3.0)), 10.0, alpha(base, 0.25));
        }
        ui.painter().rect_filled(r, 10.0, lerp(base, hi, h));
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, TEXT);
        resp.on_hover_cursor(CursorIcon::PointingHand)
    }
}

/// Segmented selector. Returns true when the selection changed.
pub fn segmented(ui: &mut Ui, value: &mut usize, options: &[&str]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (i, o) in options.iter().enumerate() {
            if ui.add(Pill::new(*o).selected(*value == i)).clicked() && *value != i {
                *value = i;
                changed = true;
            }
        }
    });
    changed
}

/// Small stat card: label on top, big value below.
pub fn chip(ui: &mut Ui, label: &str, value: impl ToString, color: Color32) {
    let vg = ui.painter().layout_no_wrap(value.to_string(), FontId::proportional(20.0), color);
    let lg = ui.painter().layout_no_wrap(label.to_uppercase(), FontId::proportional(10.5), MUTED);
    let w = vg.size().x.max(lg.size().x).max(56.0) + 26.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 54.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 12.0, CARD);
    p.galley(egui::pos2(rect.center().x - lg.size().x / 2.0, rect.top() + 9.0), lg, MUTED);
    p.galley(egui::pos2(rect.center().x - vg.size().x / 2.0, rect.top() + 23.0), vg, color);
}

/// Vector icons, painted directly (no emoji font needed - keeps the binary small).
#[derive(Clone, Copy)]
pub enum Icon {
    Home,
    Stats,
    Tiles,
    Snake,
    Mine,
    Cards,
    Ttt,
    Blocks,
    Chess,
    Play,
}

pub fn paint_icon(p: &Painter, c: Pos2, s: f32, icon: Icon, color: Color32) {
    let st = |w: f32| Stroke::new(s * w, color);
    match icon {
        Icon::Home => {
            let roof = vec![c + vec2(0.0, -s * 0.48), c + vec2(s * 0.5, -s * 0.02), c + vec2(-s * 0.5, -s * 0.02)];
            p.add(Shape::convex_polygon(roof, color, Stroke::NONE));
            p.rect_filled(Rect::from_min_max(c + vec2(-s * 0.34, -s * 0.06), c + vec2(s * 0.34, s * 0.42)), s * 0.06, color);
            p.rect_filled(Rect::from_min_max(c + vec2(-s * 0.09, s * 0.12), c + vec2(s * 0.09, s * 0.42)), s * 0.03, BG);
        }
        Icon::Stats => {
            for (i, h) in [0.45f32, 0.8, 0.6].into_iter().enumerate() {
                let x = c.x - s * 0.36 + i as f32 * s * 0.28;
                let r = Rect::from_min_max(pos2(x, c.y + s * 0.42 - s * h), pos2(x + s * 0.18, c.y + s * 0.42));
                p.rect_filled(r, s * 0.05, color);
            }
        }
        Icon::Tiles => {
            for (dx, dy, a) in [(-1.0, -1.0, 1.0), (1.0, -1.0, 0.55), (-1.0, 1.0, 0.55), (1.0, 1.0, 1.0)] {
                let r = Rect::from_center_size(c + vec2(dx, dy) * s * 0.23, Vec2::splat(s * 0.38));
                p.rect_filled(r, s * 0.08, alpha(color, a));
            }
        }
        Icon::Snake => {
            let pts: Vec<Pos2> = (0..=24)
                .map(|k| {
                    let t = k as f32 / 24.0;
                    c + vec2(-s * 0.42 + t * s * 0.84, (t * std::f32::consts::TAU).sin() * s * 0.22)
                })
                .collect();
            let head = *pts.last().unwrap();
            p.add(Shape::line(pts, st(0.16)));
            p.circle_filled(head, s * 0.13, color);
        }
        Icon::Mine => {
            for k in 0..4 {
                let d = egui::Vec2::angled(k as f32 * std::f32::consts::FRAC_PI_4) * s * 0.46;
                p.line_segment([c - d, c + d], st(0.09));
            }
            p.circle_filled(c, s * 0.28, color);
            p.circle_filled(c + vec2(-s * 0.09, -s * 0.09), s * 0.07, alpha(Color32::WHITE, 0.7));
        }
        Icon::Cards => {
            let back = Rect::from_center_size(c + vec2(-s * 0.12, -s * 0.06), vec2(s * 0.5, s * 0.68));
            p.rect_stroke(back, s * 0.08, st(0.08), StrokeKind::Inside);
            let front = Rect::from_center_size(c + vec2(s * 0.14, s * 0.08), vec2(s * 0.5, s * 0.68));
            p.rect_filled(front, s * 0.08, color);
            p.circle_filled(front.center(), s * 0.09, BG);
        }
        Icon::Ttt => {
            let a = c + vec2(-s * 0.2, 0.0);
            let d = s * 0.17;
            p.line_segment([a - vec2(d, d), a + vec2(d, d)], st(0.12));
            p.line_segment([a + vec2(-d, d), a + vec2(d, -d)], st(0.12));
            p.circle_stroke(c + vec2(s * 0.24, 0.0), s * 0.17, st(0.11));
        }
        Icon::Blocks => {
            // Four cells in a zig-zag, like a falling piece.
            let u = s * 0.3;
            for (i, (x, y)) in [(-1.0, -0.5), (0.0, -0.5), (0.0, 0.5), (1.0, 0.5)].into_iter().enumerate() {
                let r = Rect::from_center_size(c + vec2(x * u, y * u), Vec2::splat(u * 0.86));
                p.rect_filled(r, s * 0.05, alpha(color, if i % 3 == 0 { 0.6 } else { 1.0 }));
            }
        }
        Icon::Chess => {
            // A pawn silhouette.
            p.circle_filled(c + vec2(0.0, -s * 0.22), s * 0.15, color);
            let body = vec![c + vec2(-s * 0.08, -s * 0.06), c + vec2(s * 0.08, -s * 0.06), c + vec2(s * 0.2, s * 0.26), c + vec2(-s * 0.2, s * 0.26)];
            p.add(Shape::convex_polygon(body, color, Stroke::NONE));
            p.rect_filled(Rect::from_min_max(c + vec2(-s * 0.3, s * 0.24), c + vec2(s * 0.3, s * 0.42)), s * 0.05, color);
        }
        Icon::Play => {
            let pts = vec![c + vec2(-s * 0.3, -s * 0.38), c + vec2(s * 0.4, 0.0), c + vec2(-s * 0.3, s * 0.38)];
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
    }
}

/// Standard game header: icon badge + title + subtitle.
pub fn title(ui: &mut Ui, icon: Icon, name: &str, subtitle: &str, color: Color32) -> f32 {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
        ui.painter().rect_filled(rect, 12.0, alpha(color, 0.18));
        paint_icon(ui.painter(), rect.center(), 26.0, icon, color);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(egui::RichText::new(name).size(24.0).strong().color(TEXT));
            ui.label(egui::RichText::new(subtitle).size(13.0).color(MUTED));
        });
    })
    .response
    .rect
    .width()
}

/// Game header: title on the left, `right` (chips + buttons, laid out right-to-left) on the right.
/// If both don't fit on one line (judged from last frame's measured widths), the right-hand
/// part moves to its own row below the title so nothing overlaps in small windows.
pub fn header(ui: &mut Ui, icon: Icon, name: &str, subtitle: &str, color: Color32, right: impl FnOnce(&mut Ui)) {
    let id = ui.scope_id().with("header_widths");
    let (tw, rw): (f32, f32) = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let one_row = tw + rw + 24.0 <= ui.available_width();
    let rtl = egui::Layout::right_to_left(egui::Align::Center);
    let (mut new_tw, mut new_rw) = (0.0, 0.0);
    if one_row {
        ui.horizontal(|ui| {
            new_tw = title(ui, icon, name, subtitle, color);
            ui.with_layout(rtl, |ui| {
                right(ui);
                new_rw = ui.min_rect().width();
            });
        });
    } else {
        new_tw = title(ui, icon, name, subtitle, color);
        ui.horizontal(|ui| {
            ui.with_layout(rtl.with_main_wrap(true), |ui| {
                right(ui);
                new_rw = ui.min_rect().width();
            });
        });
    }
    ui.data_mut(|d| d.insert_temp(id, (new_tw, new_rw)));
}

/// Dim a board and show a centered message. `t` is the fade-in progress (0..1).
pub fn overlay(ui: &Ui, rect: Rect, t: f32, headline: &str, sub: &str, color: Color32) {
    let p = ui.painter();
    p.rect_filled(rect, 14.0, alpha(BG, 0.78 * t));
    let y = rect.center().y - 22.0 + (1.0 - t) * 14.0;
    p.text(egui::pos2(rect.center().x, y), Align2::CENTER_CENTER, headline, FontId::proportional(40.0), alpha(color, t));
    p.text(egui::pos2(rect.center().x, y + 40.0), Align2::CENTER_CENTER, sub, FontId::proportional(16.0), alpha(TEXT, t * 0.85));
}

/// A tiny bar sparkline of recent scores.
pub fn sparkline(ui: &mut Ui, values: &[u64], size: Vec2, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    if values.is_empty() {
        p.text(rect.center(), Align2::CENTER_CENTER, "no games yet", FontId::proportional(11.0), MUTED);
        return;
    }
    let max = values.iter().copied().max().unwrap_or(1).max(1) as f32;
    let n = values.len().max(12) as f32;
    let bw = rect.width() / n;
    for (i, v) in values.iter().enumerate() {
        let h = ((*v as f32 / max) * rect.height()).max(2.0);
        let x = rect.left() + i as f32 * bw;
        let r = Rect::from_min_max(egui::pos2(x + 1.0, rect.bottom() - h), egui::pos2(x + bw - 1.0, rect.bottom()));
        let last = i + 1 == values.len();
        p.rect_filled(r, 2.0, if last { color } else { alpha(color, 0.45) });
    }
}

/// Board area with the given aspect ratio that fits the remaining space,
/// leaving a margin so board backgrounds don't touch the controls above.
pub fn board_rect(ui: &mut Ui, aspect: f32, max_w: f32, sense: Sense) -> (Rect, Response) {
    const PAD: f32 = 18.0;
    let avail = ui.available_size();
    let mut w = (avail.x - 2.0 * PAD).min(max_w);
    let mut h = w / aspect;
    if h > avail.y - 2.0 * PAD {
        h = (avail.y - 2.0 * PAD).max(120.0);
        w = h * aspect;
    }
    let (outer, _) = ui.allocate_exact_size(egui::vec2(avail.x, h + 2.0 * PAD), Sense::hover());
    let rect = Rect::from_center_size(outer.center(), egui::vec2(w, h));
    let resp = ui.interact(rect, ui.scope_id().with("board"), sense);
    (rect, resp)
}
