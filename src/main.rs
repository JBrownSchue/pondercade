#![windows_subsystem = "windows"]

mod games;
#[cfg(debug_assertions)]
mod shots;
mod stats;
mod theme;
mod util;
mod widgets;

use eframe::egui::{self, Align2, CentralPanel, CursorIcon, FontId, Frame, Panel, RichText, ScrollArea, Sense};
use games::Game;
use stats::{GameStats, fmt_time};
use std::collections::BTreeMap;
use theme::*;
use widgets::{Icon, Pill, chip, paint_icon, sparkline};

const STATS_KEY: &str = "arcade_stats_v1";
const APP_NAME: &str = "Pondercade";
/// Name used before the rename; its saved stats are carried over on first start.
const OLD_APP_NAME: &str = "Pixel Arcade";

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Home,
    Game(usize),
    Stats,
}

struct App {
    screen: Screen,
    games: Vec<Box<dyn Game>>,
    stats: BTreeMap<String, GameStats>,
    confirm_reset: bool,
    last_time: Option<f64>,
    #[cfg(debug_assertions)]
    shooter: Option<shots::Shooter>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let stats = cc.storage.and_then(|s| eframe::get_value(s, STATS_KEY)).unwrap_or_default();
        Self {
            screen: Screen::Home,
            games: games::all(),
            stats,
            confirm_reset: false,
            last_time: None,
            #[cfg(debug_assertions)]
            shooter: shots::Shooter::from_env(),
        }
    }

    fn totals(&self) -> (u32, u32, f64) {
        self.stats.values().fold((0, 0, 0.0), |(p, w, s), g| (p + g.played, w + g.wins, s + g.seconds))
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, compact: bool) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, 10.0, ACCENT);
            p.rect_filled(r.shrink(9.0).translate(egui::vec2(-3.0, -3.0)), 3.0, alpha(TEXT, 0.95));
            p.rect_filled(r.shrink(9.0).translate(egui::vec2(3.0, 3.0)), 3.0, alpha(CYAN, 0.85));
            if compact {
                return;
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("PONDERCADE").size(15.0).strong().color(TEXT));
                ui.label(RichText::new("while you wait").size(11.0).color(MUTED));
            });
        });
        ui.add_space(22.0);

        let list: Vec<_> = self.games.iter().map(|g| (g.icon(), g.name(), g.color())).collect();
        let mut nav = |ui: &mut egui::Ui, icon: Icon, label: &str, target: Screen, color| {
            if nav_item(ui, icon, label, self.screen == target, color, compact) {
                self.screen = target;
                self.confirm_reset = false;
            }
        };
        ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            if !compact {
                section(ui, "MENU");
            }
            nav(ui, Icon::Home, "Home", Screen::Home, ACCENT);
            nav(ui, Icon::Stats, "Statistics", Screen::Stats, CYAN);
            ui.add_space(14.0);
            if !compact {
                section(ui, "GAMES");
            }
            for (i, (icon, name, color)) in list.into_iter().enumerate() {
                nav(ui, icon, name, Screen::Game(i), color);
            }
        });

        // Footer only when there is room for it below the menu.
        if !compact && ui.available_height() > 36.0 {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                let (p, w, s) = self.totals();
                ui.label(RichText::new(format!("{p} games  ·  {w} wins  ·  {}", fmt_time(s))).size(11.5).color(MUTED));
            });
        }
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Welcome back").size(30.0).strong().color(TEXT));
        ui.label(RichText::new("Something to do while your AI is pondering. Every round is tracked.").color(MUTED));
        ui.add_space(8.0);
        let (p, w, s) = self.totals();
        ui.horizontal_wrapped(|ui| {
            chip(ui, "Games played", p, TEXT);
            chip(ui, "Wins", w, GREEN);
            chip(ui, "Time played", fmt_time(s), CYAN);
            let fav = self
                .games
                .iter()
                .max_by(|a, b| {
                    let sa = self.stats.get(a.id()).map_or(0.0, |s| s.seconds);
                    let sb = self.stats.get(b.id()).map_or(0.0, |s| s.seconds);
                    sa.total_cmp(&sb)
                })
                .filter(|g| self.stats.get(g.id()).is_some_and(|s| s.seconds > 0.0))
                .map_or("-", |g| g.name());
            chip(ui, "Favourite", fav, AMBER);
        });
        ui.add_space(14.0);

        let gap = 16.0;
        let avail = ui.available_width();
        let cols = if avail > 900.0 { 3 } else if avail > 600.0 { 2 } else { 1 };
        let cw = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        let mut open = None;
        ScrollArea::vertical().show(ui, |ui| {
            for row in (0..self.games.len()).collect::<Vec<_>>().chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for &i in row {
                        let g = &self.games[i];
                        let st = self.stats.get(g.id()).cloned().unwrap_or_default();
                        if game_card(ui, g.as_ref(), &st, cw) {
                            open = Some(i);
                        }
                    }
                });
                ui.add_space(gap - 10.0);
            }
        });
        if let Some(i) = open {
            self.screen = Screen::Game(i);
        }
    }

    fn stats_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Statistics").size(30.0).strong().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.confirm_reset {
                    if ui.add(Pill::new("Cancel")).clicked() {
                        self.confirm_reset = false;
                    }
                    if ui.add(Pill::primary("Yes, erase everything", RED)).clicked() {
                        self.stats.clear();
                        self.confirm_reset = false;
                    }
                } else if ui.add(Pill::new("Reset stats")).clicked() {
                    self.confirm_reset = true;
                }
            });
        });
        let (p, w, s) = self.totals();
        ui.horizontal_wrapped(|ui| {
            chip(ui, "Games played", p, TEXT);
            chip(ui, "Wins", w, GREEN);
            chip(ui, "Time played", fmt_time(s), CYAN);
        });
        ui.add_space(6.0);
        ScrollArea::vertical().show(ui, |ui| {
            for g in &self.games {
                let st = self.stats.get(g.id()).cloned().unwrap_or_default();
                Frame::default().fill(PANEL).corner_radius(14.0).inner_margin(16.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        widgets::title(ui, g.icon(), g.name(), &format!("{} rounds played", st.played), g.color());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if st.best_score > 0 {
                                sparkline(ui, &st.history, egui::vec2(160.0, 40.0), g.color());
                                ui.label(RichText::new("recent
scores").size(11.0).color(MUTED));
                            }
                        });
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        chip(ui, "Played", st.played, TEXT);
                        if let Some(r) = st.win_rate() {
                            chip(ui, "Win rate", format!("{:.0}%", r * 100.0), GREEN);
                            chip(ui, "W / L / D", format!("{} / {} / {}", st.wins, st.losses, st.draws), TEXT);
                            chip(ui, "Best streak", st.best_streak, AMBER);
                        }
                        if st.best_score > 0 {
                            chip(ui, "Best score", st.best_score, g.color());
                            chip(ui, "Avg score", st.avg_score(), TEXT);
                        }
                        for (k, t) in &st.best_times {
                            chip(ui, &format!("Best {k}"), fmt_time(*t), CYAN);
                        }
                        chip(ui, "Time", fmt_time(st.seconds), MUTED);
                    });
                });
                ui.add_space(4.0);
            }
        });
    }
}

fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(10.5).color(alpha(MUTED, 0.7)).strong());
}

fn nav_item(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool, color: egui::Color32, compact: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), Sense::click());
    let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || selected, 0.15);
    let p = ui.painter();
    if h > 0.0 {
        let fill = if selected { alpha(color, 0.16) } else { alpha(CARD_HI, h * 0.8) };
        p.rect_filled(rect, 10.0, fill);
    }
    if selected {
        let bar = egui::Rect::from_min_size(rect.left_top() + egui::vec2(0.0, 10.0), egui::vec2(3.0, 20.0));
        p.rect_filled(bar, 2.0, color);
    }
    let ic = if selected { color } else { lerp(MUTED, TEXT, h) };
    if compact {
        paint_icon(p, rect.center(), 18.0, icon, ic);
        ui.add_space(-4.0);
        return resp.on_hover_text(label).on_hover_cursor(CursorIcon::PointingHand).clicked();
    }
    paint_icon(p, rect.left_center() + egui::vec2(24.0, 0.0), 18.0, icon, ic);
    p.text(
        rect.left_center() + egui::vec2(46.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(15.0),
        if selected { TEXT } else { lerp(MUTED, TEXT, h) },
    );
    ui.add_space(-4.0);
    resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

fn game_card(ui: &mut egui::Ui, g: &dyn Game, st: &GameStats, w: f32) -> bool {
    // Leave room for the icon blob on the right; grow the card if the blurb wraps a lot.
    let blurb = ui.painter().layout(g.blurb().to_owned(), FontId::proportional(13.0), MUTED, (w - 120.0).max(80.0));
    let height = (44.0 + blurb.size().y + 64.0).max(160.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, height), Sense::click());
    let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.15);
    let r = rect.translate(egui::vec2(0.0, -3.0 * h));
    ui.painter().rect_filled(r.translate(egui::vec2(0.0, 6.0)), 18.0, alpha(g.color(), 0.10 * h));
    ui.painter().rect_filled(r, 18.0, lerp(PANEL, CARD, h));
    // decorative glow blob, clipped to the card
    let p = ui.painter().with_clip_rect(r.shrink(1.0));
    let p = &p;
    p.circle_filled(r.right_top() + egui::vec2(-36.0, 36.0), 54.0 + 6.0 * h, alpha(g.color(), 0.10 + 0.06 * h));
    p.circle_filled(r.right_top() + egui::vec2(-36.0, 36.0), 26.0, alpha(g.color(), 0.18));
    paint_icon(p, r.right_top() + egui::vec2(-36.0, 36.0), 28.0, g.icon(), g.color());

    let left = r.left() + 20.0;
    p.text(egui::pos2(left, r.top() + 26.0), Align2::LEFT_CENTER, g.name(), FontId::proportional(21.0), TEXT);
    p.galley(egui::pos2(left, r.top() + 44.0), blurb, MUTED);

    let line = if st.played == 0 {
        "Not played yet".to_owned()
    } else if st.best_score > 0 {
        format!("{} played  ·  best {}", st.played, st.best_score)
    } else {
        format!("{} played  ·  {} wins", st.played, st.wins)
    };
    p.text(egui::pos2(left, r.bottom() - 26.0), Align2::LEFT_CENTER, line, FontId::proportional(12.5), lerp(MUTED, TEXT, h * 0.5));
    let play = egui::Rect::from_center_size(r.right_bottom() + egui::vec2(-48.0, -26.0), egui::vec2(64.0, 28.0));
    p.rect_filled(play, 14.0, lerp(alpha(g.color(), 0.2), g.color(), h));
    let fg = lerp(g.color(), BG, h);
    p.text(play.center() - egui::vec2(7.0, 0.0), Align2::CENTER_CENTER, "Play", FontId::proportional(13.0), fg);
    paint_icon(p, play.center() + egui::vec2(18.0, 0.0), 10.0, Icon::Play, fg);
    resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(debug_assertions)]
        if let Some(sh) = &mut self.shooter {
            let n = self.games.len() + 2;
            let (switch, demo) = sh.tick(ui.ctx(), n);
            if let Some(i) = switch {
                self.screen = match i {
                    0 => Screen::Home,
                    1 => Screen::Stats,
                    g => Screen::Game(g - 2),
                };
                if i == 0 {
                    // Fake stats so the dashboard has something to show (never saved).
                    for (k, g) in self.games.iter().enumerate() {
                        let st = self.stats.entry(g.id().to_owned()).or_default();
                        for j in 0..(6 + k * 3) as u64 {
                            let o = if (j + k as u64) % 3 == 0 { stats::Outcome::Loss } else { stats::Outcome::Win };
                            st.record(o, if k == 4 { 0 } else { 200 + (j * 337 + k as u64 * 91) % 1800 });
                        }
                        st.seconds = 300.0 + k as f64 * 420.0;
                    }
                }
            }
            if demo {
                if let Screen::Game(i) = self.screen {
                    self.games[i].demo(ui.input(|i| i.time));
                }
            }
        }
        // Play-time tracking (idle gaps are capped so AFK time isn't counted).
        let now = ui.input(|i| i.time);
        let dt = self.last_time.map_or(0.0, |t| (now - t).clamp(0.0, 3.0));
        self.last_time = Some(now);
        if let Screen::Game(i) = self.screen {
            if self.games[i].running() {
                self.stats.entry(self.games[i].id().to_owned()).or_default().seconds += dt;
            }
        }

        // Narrow windows get an icon-only sidebar.
        let compact = ui.available_width() < 820.0;
        Panel::left("nav")
            .resizable(false)
            .exact_size(if compact { 64.0 } else { 220.0 })
            .frame(Frame::default().fill(PANEL).inner_margin(if compact { 8.0 } else { 16.0 }))
            .show(ui, |ui| self.sidebar(ui, compact));

        CentralPanel::default().frame(Frame::default().fill(BG).inner_margin(if compact { 16.0 } else { 28.0 })).show(ui, |ui| match self.screen {
            Screen::Home => self.home(ui),
            Screen::Stats => self.stats_page(ui),
            Screen::Game(i) => {
                let g = &mut self.games[i];
                let st = self.stats.entry(g.id().to_owned()).or_default();
                g.ui(ui, st);
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // eframe restores saved window geometry even with `persist_window: false`, so drop any
        // geometry left over from older versions (possibly oversized by the multi-monitor bug).
        storage.remove_string("window");
        #[cfg(debug_assertions)]
        if self.shooter.is_some() {
            return;
        }
        eframe::set_value(storage, STATS_KEY, &self.stats);
    }
}

/// Copy stats saved under the pre-rename app name, if the new location has none yet.
fn migrate_old_storage() {
    let (Some(new), Some(old)) = (eframe::storage_dir(APP_NAME), eframe::storage_dir(OLD_APP_NAME)) else {
        return;
    };
    let (new_file, old_file) = (new.join("app.ron"), old.join("app.ron"));
    if !new_file.exists() && old_file.exists() && std::fs::create_dir_all(&new).is_ok() {
        let _ = std::fs::copy(old_file, new_file);
    }
}

/// Window/taskbar icon drawn in code (no image asset): the two overlapping squares of the logo.
fn app_icon() -> egui::IconData {
    const N: u32 = 64;
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    let rounded = |x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
        let dx = (x0 + r - x).max(x - (x1 - r)).max(0.0);
        let dy = (y0 + r - y).max(y - (y1 - r)).max(0.0);
        x >= x0 && x <= x1 && y >= y0 && y <= y1 && dx * dx + dy * dy <= r * r
    };
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let px = if rounded(fx, fy, 22.0, 22.0, 50.0, 50.0, 6.0) {
                [CYAN.r(), CYAN.g(), CYAN.b(), 255]
            } else if rounded(fx, fy, 14.0, 14.0, 42.0, 42.0, 6.0) {
                [TEXT.r(), TEXT.g(), TEXT.b(), 255]
            } else if rounded(fx, fy, 2.0, 2.0, 62.0, 62.0, 16.0) {
                [ACCENT.r(), ACCENT.g(), ACCENT.b(), 255]
            } else {
                [0, 0, 0, 0]
            };
            rgba.extend_from_slice(&px);
        }
    }
    egui::IconData { rgba, width: N, height: N }
}

fn initial_size() -> [f32; 2] {
    // Debug builds: `PONDER_SIZE=WxH` overrides the start size (used to test small layouts).
    #[cfg(debug_assertions)]
    if let Some((w, h)) = std::env::var("PONDER_SIZE").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        return [w, h];
    }
    [1100.0, 760.0]
}

fn main() -> eframe::Result {
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_icon(app_icon())
            // No `with_min_inner_size`: egui converts it to *physical* pixels for the starting
            // monitor, and when the window is dragged to a monitor with a different scale factor
            // Windows clamps the DPI-rescaled window up to that stale minimum. If the bigger
            // window then crosses back to the other monitor the cycle repeats and the window
            // grows without bound. The layout handles small windows instead.
            .with_inner_size(initial_size()),
        // Don't restore the saved window geometry: a window that was enlarged by the bug above
        // (or left on a disconnected monitor) would otherwise come back the same way.
        persist_window: false,
        centered: true,
        ..Default::default()
    };
    // Debug screenshot runs use their own storage so they never touch the real stats file.
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("PONDER_SHOTS") {
        options.persistence_path = Some(std::path::PathBuf::from(dir).join("storage.ron"));
    }
    migrate_old_storage();
    eframe::run_native(APP_NAME, options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
