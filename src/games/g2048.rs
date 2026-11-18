use crate::stats::{GameStats, Outcome};
use crate::theme::*;
use crate::util::Rng;
use crate::widgets::*;
use eframe::egui::{self, Align2, Color32, FontId, Key, Painter, Rect, Sense, Ui, pos2, vec2};

const N: usize = 4;
const SLIDE: f64 = 0.10;
const POP: f64 = 0.15;
const COLOR: Color32 = AMBER;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Moved,
    New,
    Merged,
}

#[derive(Clone, Copy)]
struct Tile {
    v: u32,
    from: (usize, usize),
    kind: Kind,
}

pub struct G2048 {
    grid: [[Option<Tile>; N]; N],
    /// Tiles that slide into a merge target and vanish: (value, from, to).
    ghosts: Vec<(u32, (usize, usize), (usize, usize))>,
    anim_start: f64,
    score: u64,
    moves: u32,
    won: bool,
    keep_going: bool,
    over: bool,
    recorded: bool,
    rng: Rng,
    drag: egui::Vec2,
    gain: Option<(u64, f64)>,
}

impl G2048 {
    pub fn new() -> Self {
        let mut g = Self {
            grid: [[None; N]; N],
            ghosts: Vec::new(),
            anim_start: -10.0,
            score: 0,
            moves: 0,
            won: false,
            keep_going: false,
            over: false,
            recorded: false,
            rng: Rng::new(),
            drag: egui::Vec2::ZERO,
            gain: None,
        };
        g.reset();
        g
    }

    fn reset(&mut self) {
        self.grid = [[None; N]; N];
        self.ghosts.clear();
        self.score = 0;
        self.moves = 0;
        self.won = false;
        self.keep_going = false;
        self.over = false;
        self.recorded = false;
        self.gain = None;
        self.spawn();
        self.spawn();
    }

    fn spawn(&mut self) {
        let empty: Vec<(usize, usize)> =
            (0..N * N).map(|i| (i / N, i % N)).filter(|&(r, c)| self.grid[r][c].is_none()).collect();
        if empty.is_empty() {
            return;
        }
        let (r, c) = empty[self.rng.below(empty.len())];
        let v = if self.rng.chance(0.1) { 4 } else { 2 };
        self.grid[r][c] = Some(Tile { v, from: (r, c), kind: Kind::New });
    }

    fn max_tile(&self) -> u32 {
        self.grid.iter().flatten().flatten().map(|t| t.v).max().unwrap_or(0)
    }

    fn can_move(&self) -> bool {
        for r in 0..N {
            for c in 0..N {
                let Some(t) = self.grid[r][c] else { return true };
                if c + 1 < N && self.grid[r][c + 1].is_some_and(|o| o.v == t.v) {
                    return true;
                }
                if r + 1 < N && self.grid[r + 1][c].is_some_and(|o| o.v == t.v) {
                    return true;
                }
            }
        }
        false
    }

    /// dir: 0 = left, 1 = right, 2 = up, 3 = down
    fn shift(&mut self, dir: usize, now: f64) {
        let mut next = [[None; N]; N];
        let mut ghosts = Vec::new();
        let mut moved = false;
        let mut gained = 0u64;
        for line in 0..N {
            let cells: [(usize, usize); N] = std::array::from_fn(|k| match dir {
                0 => (line, k),
                1 => (line, N - 1 - k),
                2 => (k, line),
                _ => (N - 1 - k, line),
            });
            let mut out: Vec<(u32, Vec<(usize, usize)>)> = Vec::new();
            for &(r, c) in &cells {
                if let Some(t) = self.grid[r][c] {
                    if let Some(last) = out.last_mut() {
                        if last.0 == t.v && last.1.len() == 1 {
                            last.0 *= 2;
                            last.1.push((r, c));
                            gained += last.0 as u64;
                            continue;
                        }
                    }
                    out.push((t.v, vec![(r, c)]));
                }
            }
            for (k, (v, src)) in out.into_iter().enumerate() {
                let to = cells[k];
                if src.len() == 1 {
                    moved |= src[0] != to;
                    next[to.0][to.1] = Some(Tile { v, from: src[0], kind: Kind::Moved });
                } else {
                    moved = true;
                    ghosts.extend(src.iter().map(|&s| (v / 2, s, to)));
                    next[to.0][to.1] = Some(Tile { v, from: to, kind: Kind::Merged });
                }
            }
        }
        if !moved {
            return;
        }
        self.grid = next;
        self.ghosts = ghosts;
        self.score += gained;
        self.moves += 1;
        self.anim_start = now;
        if gained > 0 {
            self.gain = Some((gained, now));
        }
        self.spawn();
        if !self.won && self.max_tile() >= 2048 {
            self.won = true;
        }
        if !self.can_move() {
            self.over = true;
        }
    }

    fn finish(&mut self, stats: &mut GameStats) {
        if !self.recorded && self.moves > 0 {
            stats.record(if self.won { Outcome::Win } else { Outcome::Loss }, self.score);
            self.recorded = true;
        }
    }
}

fn tile_colors(v: u32) -> (Color32, Color32) {
    let c = |r, g, b| Color32::from_rgb(r, g, b);
    let bg = match v {
        2 => c(46, 52, 92),
        4 => c(56, 66, 120),
        8 => c(79, 70, 229),
        16 => c(109, 40, 217),
        32 => c(147, 51, 234),
        64 => c(192, 38, 211),
        128 => c(219, 39, 119),
        256 => c(225, 29, 72),
        512 => c(234, 88, 12),
        1024 => c(245, 158, 11),
        2048 => c(234, 179, 8),
        _ => c(16, 185, 129),
    };
    let fg = if v <= 4 { c(200, 205, 235) } else { Color32::WHITE };
    (bg, fg)
}

fn draw_tile(p: &Painter, rect: Rect, v: u32, scale: f32) {
    if scale <= 0.01 {
        return;
    }
    let r = Rect::from_center_size(rect.center(), rect.size() * scale);
    let (bg, fg) = tile_colors(v);
    if v >= 128 {
        let glow = ((v as f32).log2() - 6.0).min(6.0) / 6.0;
        p.rect_filled(r.expand(5.0 * scale), 14.0, alpha(bg, 0.18 + 0.2 * glow));
    }
    p.rect_filled(r, 10.0 * scale, bg);
    let digits = v.to_string().len();
    let fs = r.width() * [0.46, 0.46, 0.40, 0.33, 0.27, 0.22][digits.min(5)];
    p.text(r.center(), Align2::CENTER_CENTER, v, FontId::proportional(fs), fg);
}

impl super::Game for G2048 {
    fn id(&self) -> &'static str {
        "2048"
    }
    fn name(&self) -> &'static str {
        "2048"
    }
    fn icon(&self) -> Icon {
        Icon::Tiles
    }
    fn blurb(&self) -> &'static str {
        "Slide and merge numbered tiles. Reach the 2048 tile — then keep going."
    }
    fn color(&self) -> Color32 {
        COLOR
    }
    fn running(&self) -> bool {
        self.moves > 0 && !self.over
    }

    #[cfg(debug_assertions)]
    fn demo(&mut self, now: f64) {
        for _ in 0..120 {
            let d = [0, 2, 0, 2, 1, 2, 0, 3][self.rng.below(8)];
            self.shift(d, now - 1.0);
            if self.over {
                self.reset();
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, stats: &mut GameStats) {
        let now = ui.input(|i| i.time);
        header(ui, self.icon(), "2048", "Arrow keys / WASD, or drag with the mouse", COLOR, |ui| {
            if ui.add(Pill::primary("New game", COLOR)).clicked() {
                self.finish(stats);
                self.reset();
            }
            chip(ui, "Best", stats.best_score.max(self.score), AMBER);
            chip(ui, "Score", self.score, TEXT);
            chip(ui, "Moves", self.moves, MUTED);
            let mt = self.max_tile();
            chip(ui, "Max tile", mt, if mt < 8 { TEXT } else { lerp(tile_colors(mt).0, Color32::WHITE, 0.25) });
        });
        ui.add_space(8.0);

        let blocked = self.over || (self.won && !self.keep_going);
        if !blocked {
            let dir = ui.input(|i| {
                if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::A) {
                    Some(0)
                } else if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::D) {
                    Some(1)
                } else if i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::W) {
                    Some(2)
                } else if i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::S) {
                    Some(3)
                } else {
                    None
                }
            });
            if let Some(d) = dir {
                self.shift(d, now);
            }
        }

        let (rect, resp) = board_rect(ui, 1.0, 540.0, Sense::drag());
        if resp.dragged() {
            self.drag += resp.drag_delta();
        }
        if resp.drag_stopped() {
            let d = self.drag;
            self.drag = egui::Vec2::ZERO;
            if !blocked && d.length() > 30.0 {
                let dir = if d.x.abs() > d.y.abs() { if d.x < 0.0 { 0 } else { 1 } } else if d.y < 0.0 { 2 } else { 3 };
                self.shift(dir, now);
            }
        }
        if self.over {
            self.finish(stats);
        }

        let p = ui.painter().clone();
        p.rect_filled(rect.expand(2.0), 18.0, PANEL);
        let gap = rect.width() * 0.028;
        let cell = (rect.width() - gap * (N as f32 + 1.0)) / N as f32;
        let at = |r: f32, c: f32| {
            Rect::from_min_size(
                pos2(rect.left() + gap + c * (cell + gap), rect.top() + gap + r * (cell + gap)),
                vec2(cell, cell),
            )
        };
        for r in 0..N {
            for c in 0..N {
                p.rect_filled(at(r as f32, c as f32), 10.0, CARD);
            }
        }

        let el = now - self.anim_start;
        let t = (el / SLIDE).clamp(0.0, 1.0) as f32;
        let ease = 1.0 - (1.0 - t).powi(3);
        let pop = ((el - SLIDE) / POP).clamp(0.0, 1.0) as f32;
        let lerp_pos = |a: (usize, usize), b: (usize, usize)| {
            let r = a.0 as f32 + (b.0 as f32 - a.0 as f32) * ease;
            let c = a.1 as f32 + (b.1 as f32 - a.1 as f32) * ease;
            at(r, c)
        };
        if t < 1.0 {
            for &(v, from, to) in &self.ghosts {
                draw_tile(&p, lerp_pos(from, to), v, 1.0);
            }
        }
        for r in 0..N {
            for c in 0..N {
                let Some(tile) = self.grid[r][c] else { continue };
                match tile.kind {
                    Kind::Moved => draw_tile(&p, lerp_pos(tile.from, (r, c)), tile.v, 1.0),
                    Kind::New if t >= 1.0 => {
                        let s = 1.0 - (1.0 - pop).powi(2);
                        draw_tile(&p, at(r as f32, c as f32), tile.v, 0.3 + 0.7 * s)
                    }
                    Kind::Merged if t >= 1.0 => {
                        let s = 1.0 + 0.14 * (pop * std::f32::consts::PI).sin();
                        draw_tile(&p, at(r as f32, c as f32), tile.v, s)
                    }
                    _ => {}
                }
            }
        }

        // Floating "+points" indicator.
        if let Some((g, t0)) = self.gain {
            let k = ((now - t0) / 0.7) as f32;
            if k < 1.0 {
                let pos = rect.center_top() + vec2(0.0, -6.0 - 24.0 * k);
                p.text(pos, Align2::CENTER_BOTTOM, format!("+{g}"), FontId::proportional(20.0), alpha(AMBER, 1.0 - k));
                ui.ctx().request_repaint();
            }
        }
        if el < SLIDE + POP {
            ui.ctx().request_repaint();
        }

        let show_win = self.won && !self.keep_going && !self.over;
        let fade_win = ui.ctx().animate_bool_with_time(ui.scope_id().with("win"), show_win, 0.35);
        let fade_over = ui.ctx().animate_bool_with_time(ui.scope_id().with("over"), self.over, 0.35);
        if fade_over > 0.0 {
            let sub = format!("Score {}  ·  highest tile {}", self.score, self.max_tile());
            overlay(ui, rect, fade_over, if self.won { "Well played!" } else { "No moves left" }, &sub, if self.won { GREEN } else { RED });
            if self.over {
                let br = Rect::from_center_size(rect.center() + vec2(0.0, 70.0), vec2(140.0, 40.0));
                if ui.put(br, Pill::primary("Try again", COLOR)).clicked() {
                    self.reset();
                }
            }
        } else if fade_win > 0.0 {
            overlay(ui, rect, fade_win, "2048!", "You did it. Keep going for a higher score?", AMBER);
            if show_win {
                let br = Rect::from_center_size(rect.center() + vec2(0.0, 70.0), vec2(140.0, 40.0));
                if ui.put(br, Pill::primary("Keep going", COLOR)).clicked() {
                    self.keep_going = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(g: &G2048, r: usize) -> [u32; N] {
        std::array::from_fn(|c| g.grid[r][c].map_or(0, |t| t.v))
    }

    #[test]
    fn merges_once_per_move() {
        let mut g = G2048::new();
        g.grid = [[None; N]; N];
        for (c, v) in [2, 2, 2, 2].into_iter().enumerate() {
            g.grid[0][c] = Some(Tile { v, from: (0, c), kind: Kind::Moved });
        }
        for (c, v) in [2, 2, 4, 0].into_iter().enumerate() {
            g.grid[1][c] = (v > 0).then_some(Tile { v, from: (1, c), kind: Kind::Moved });
        }
        g.shift(0, 0.0);
        assert_eq!(&row(&g, 0)[..2], &[4, 4]);
        assert_eq!(&row(&g, 1)[..2], &[4, 4]);
        assert_eq!(g.score, 12);
    }
}
