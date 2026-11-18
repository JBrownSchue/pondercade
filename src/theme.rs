use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, Theme, Visuals};

pub const BG: Color32 = Color32::from_rgb(13, 15, 28);
pub const PANEL: Color32 = Color32::from_rgb(19, 22, 40);
pub const CARD: Color32 = Color32::from_rgb(28, 32, 58);
pub const CARD_HI: Color32 = Color32::from_rgb(40, 46, 80);
pub const LINE: Color32 = Color32::from_rgb(48, 54, 92);
pub const TEXT: Color32 = Color32::from_rgb(232, 234, 246);
pub const MUTED: Color32 = Color32::from_rgb(139, 144, 175);
pub const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
pub const CYAN: Color32 = Color32::from_rgb(34, 211, 238);
pub const GREEN: Color32 = Color32::from_rgb(52, 211, 153);
pub const RED: Color32 = Color32::from_rgb(248, 113, 113);
pub const AMBER: Color32 = Color32::from_rgb(251, 191, 36);
pub const PINK: Color32 = Color32::from_rgb(244, 114, 182);
pub const ORANGE: Color32 = Color32::from_rgb(251, 146, 60);

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()), f(a.a(), b.a()))
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(Theme::Dark);
    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = PANEL;
    v.faint_bg_color = CARD;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.window_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    for (w, fill) in [
        (&mut v.widgets.inactive, CARD),
        (&mut v.widgets.hovered, CARD_HI),
        (&mut v.widgets.active, ACCENT),
        (&mut v.widgets.open, CARD_HI),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = 8.into();
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1.0, TEXT);
    }
    ctx.set_visuals_of(Theme::Dark, v);
    ctx.style_mut_of(Theme::Dark, |s| {
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.button_padding = egui::vec2(14.0, 8.0);
        s.text_styles = [
            (TextStyle::Heading, FontId::new(26.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace)),
        ]
        .into();
    });
}
