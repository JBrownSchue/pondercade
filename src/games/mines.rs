use crate::stats::{GameStats, Outcome, fmt_time};
use crate::theme::*;
use crate::util::Rng;
use crate::widgets::*;
use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use std::collections::VecDeque;

const COLOR: Color32 = RED;
const DIFFS: [(&str, usize, usize, usize); 3] = [("Easy", 9, 9, 10), ("Medium", 16, 16, 40), ("Hard", 30, 16, 99)];
const NUM_COLORS: [Color32; 8] = [CYAN, GREEN, RED, ACCENT, ORANGE, PINK, AMBER, TEXT];

#[derive(Clone, Copy, Default)]
struct Cell {
    mine: bool,
    open: bool,
    flag: bool,
    n: u8,
    /// Time at which the cell's reveal animation starts.
    t: f64,
}

#[derive(PartialEq, Clone, Copy)]
enum State {
    Playing,
    Won,
    Lost,
}

pub struct Mines {
    diff: usize,
    w: usize,
    h: usize,
    mines: usize,
    cells: Vec<Cell>,
    generated: bool,
    state: State,
    start: Option<f64>,
    end: f64,
    exploded: Option<usize>,
    anim_until: f64,
    new_best: bool,
    rng: Rng,
}

impl Mines {
    pub fn new() -> Self {
        let mut m = Self {
            diff: 0,
            w: 0,
            h: 0,
            mines: 0,
            cells: Vec::new(),
            generated: false,
            state: State::Playing,
            start: None,
            end: 0.0,
            exploded: None,
            anim_until: 0.0,
            new_best: false,
            rng: Rng::new(),
        };
        m.reset();
        m
    }

    fn reset(&mut self) {
        let (_, w, h, m) = DIFFS[self.diff];
        self.w = w;
        self.h = h;
        self.mines = m;
        self.cells = vec![Cell::default(); w * h];
        self.generated = false;
        self.state = State::Playing;
        self.start = None;
        self.exploded = None;
        self.new_best = false;
    }

    fn neighbors(&self, i: usize) -> impl Iterator<Item = usize> + use<> {
        let (w, h) = (self.w as i32, self.h as i32);
        let (x, y) = ((i % self.w) as i32, (i / self.w) as i32);
        (-1..=1)
            .flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
            .filter(move |&(nx, ny)| (nx, ny) != (x, y) && nx >= 0 && ny >= 0 && nx < w && ny < h)
            .map(move |(nx, ny)| (ny * w + nx) as usize)
    }

    fn generate(&mut self, safe: usize) {
        let forbidden: Vec<usize> = self.neighbors(safe).chain(std::iter::once(safe)).collect();
        let mut pool: Vec<usize> = (0..self.cells.len()).filter(|i| !forbidden.contains(i)).collect();
        self.rng.shuffle(&mut pool);
        for &i in pool.iter().take(self.mines) {
            self.cells[i].mine = true;
        }
        for i in 0..self.cells.len() {
            let n = self.neighbors(i).filter(|&j| self.cells[j].mine).count() as u8;
            self.cells[i].n = n;
        }
        self.generated = true;
    }

    fn reveal(&mut self, i: usize, now: f64) {
        if !self.generated {
            self.generate(i);
            self.start = Some(now);
        }
        if self.cells[i].open || self.cells[i].flag {
            return;
        }
        if self.cells[i].mine {
            self.state = State::Lost;
            self.end = now;
            self.exploded = Some(i);
            let (ex, ey) = ((i % self.w) as f64, (i / self.w) as f64);
            for (j, c) in self.cells.iter_mut().enumerate() {
                if c.mine {
                    c.open = true;
                    let d = (((j % self.w) as f64 - ex).powi(2) + ((j / self.w) as f64 - ey).powi(2)).sqrt();
                    c.t = now + d * 0.03;
                    self.anim_until = self.anim_until.max(c.t + 0.2);
                }
            }
            return;
        }
        // BFS flood fill with a ripple delay based on depth.
        let mut q = VecDeque::from([(i, 0u32)]);
        self.cells[i].open = true;
        self.cells[i].t = now;
        while let Some((c, depth)) = q.pop_front() {
            if self.cells[c].n != 0 {
                continue;
            }
            for j in self.neighbors(c).collect::<Vec<_>>() {
                let cell = &mut self.cells[j];
                if !cell.open && !cell.flag && !cell.mine {
                    cell.open = true;
                    cell.t = now + (depth + 1) as f64 * 0.025;
                    self.anim_until = self.anim_until.max(cell.t + 0.2);
                    q.push_back((j, depth + 1));
                }
            }
        }
        self.anim_until = self.anim_until.max(now + 0.2);
    }

    fn chord(&mut self, i: usize, now: f64) {
        let c = self.cells[i];
        if !c.open || c.n == 0 {
            return;
        }
        let flags = self.neighbors(i).filter(|&j| self.cells[j].flag).count();
        if flags == c.n as usize {
            for j in self.neighbors(i).collect::<Vec<_>>() {
                if self.state == State::Playing {
                    self.reveal(j, now);
                }
            }
        }
    }

    fn flags(&self) -> usize {
        self.cells.iter().filter(|c| c.flag).count()
    }

    fn cleared(&self) -> usize {
        self.cells.iter().filter(|c| c.open && !c.mine).count()
    }

    fn elapsed(&self, now: f64) -> f64 {
        match (self.start, self.state) {
            (None, _) => 0.0,
            (Some(s), State::Playing) => now - s,
            (Some(s), _) => self.end - s,
        }
    }
}

fn draw_flag(p: &Painter, c: Pos2, s: f32) {
    let pole_x = c.x - s * 0.12;
    p.line_segment([pos2(pole_x, c.y - s * 0.3), pos2(pole_x, c.y + s * 0.3)], Stroke::new(s * 0.07, TEXT));
    p.rect_filled(Rect::from_center_size(pos2(c.x, c.y + s * 0.3), vec2(s * 0.4, s * 0.07)), 1.0, TEXT);
    let pts = vec![pos2(pole_x, c.y - s * 0.32), pos2(c.x + s * 0.28, c.y - s * 0.16), pos2(pole_x, c.y)];
    p.add(Shape::convex_polygon(pts, RED, Stroke::NONE));
}

fn draw_mine(p: &Painter, c: Pos2, s: f32) {
    let r = s * 0.22;
    for k in 0..4 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let d = vec2(a.cos(), a.sin()) * r * 1.5;
        p.line_segment([c - d, c + d], Stroke::new(s * 0.06, Color32::from_rgb(20, 20, 30)));
    }
    p.circle_filled(c, r, Color32::from_rgb(20, 20, 30));
    p.circle_filled(c + vec2(-r * 0.35, -r * 0.35), r * 0.28, alpha(Color32::WHITE, 0.7));
}

impl super::Game for Mines {
    fn id(&self) -> &'static str {
        "mines"
    }
    fn name(&self) -> &'static str {
        "Minesweeper"
    }
    fn icon(&self) -> Icon {
        Icon::Mine
    }
    fn blurb(&self) -> &'static str {
        "Clear the field using number clues. First click is always safe."
    }
    fn color(&self) -> Color32 {
        COLOR
    }
    fn running(&self) -> bool {
        self.start.is_some() && self.state == State::Playing
    }

    #[cfg(debug_assertions)]
    fn demo(&mut self, now: f64) {
        self.diff = 1;
        self.reset();
        let c = self.w * self.h / 2 + self.w / 2;
        self.reveal(c, now - 1.0);
        let mut flagged = 0;
        for i in 0..self.cells.len() {
            if self.cells[i].mine && flagged < 6 && self.neighbors(i).any(|j| self.cells[j].open) {
                self.cells[i].flag = true;
                flagged += 1;
            }
        }
        self.start = Some(now - 42.0);
    }

    fn ui(&mut self, ui: &mut Ui, stats: &mut GameStats) {
        let now = ui.input(|i| i.time);
        let label = DIFFS[self.diff].0;
        header(ui, self.icon(), "Minesweeper", "Left click: reveal · Right click: flag · Click a number: chord", COLOR, |ui| {
            if ui.add(Pill::primary("New game", COLOR)).clicked() {
                self.reset();
            }
            let best = stats.best_times.get(label).map_or("-".to_owned(), |t| fmt_time(*t));
            chip(ui, &format!("Best {label}"), best, AMBER);
            chip(ui, "Time", fmt_time(self.elapsed(now)), TEXT);
            chip(ui, "Mines left", self.mines as i64 - self.flags() as i64, COLOR);
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Difficulty").color(MUTED));
            let names: Vec<&str> = DIFFS.iter().map(|d| d.0).collect();
            if segmented(ui, &mut self.diff, &names) {
                self.reset();
            }
        });

        let (rect, resp) = board_rect(ui, self.w as f32 / self.h as f32, self.w as f32 * 42.0, Sense::click());
        let cs = rect.width() / self.w as f32;
        let (w, h) = (self.w, self.h);
        let idx_at = move |pos: Pos2| -> Option<usize> {
            if !rect.contains(pos) {
                return None;
            }
            let x = ((pos.x - rect.left()) / cs) as usize;
            let y = ((pos.y - rect.top()) / cs) as usize;
            (x < w && y < h).then(|| y * w + x)
        };
        let hover = resp.hover_pos().and_then(idx_at);

        if self.state == State::Playing {
            if let Some(i) = resp.interact_pointer_pos().and_then(idx_at) {
                if resp.clicked() {
                    if self.cells[i].open { self.chord(i, now) } else { self.reveal(i, now) }
                } else if resp.secondary_clicked() && !self.cells[i].open && self.generated {
                    self.cells[i].flag ^= true;
                } else if resp.middle_clicked() {
                    self.chord(i, now);
                }
            }
            if self.state == State::Playing && self.generated && self.cleared() == self.w * self.h - self.mines {
                self.state = State::Won;
                self.end = now;
                for c in self.cells.iter_mut().filter(|c| c.mine) {
                    c.flag = true;
                }
                let score = (self.cleared() * (self.diff + 1)) as u64;
                stats.record(Outcome::Win, score);
                self.new_best = stats.record_time(label, self.end - self.start.unwrap_or(now));
            }
            if self.state == State::Lost {
                stats.record(Outcome::Loss, (self.cleared() * (self.diff + 1)) as u64);
            }
        }
        if self.running() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
        if now < self.anim_until {
            ui.ctx().request_repaint();
        }

        let p = ui.painter().clone();
        p.rect_filled(rect.expand(8.0), 16.0, PANEL);
        let pad = (cs * 0.06).max(1.0);
        let font = FontId::proportional(cs * 0.55);
        for (i, c) in self.cells.iter().enumerate() {
            let r = Rect::from_min_size(pos2(rect.left() + (i % self.w) as f32 * cs, rect.top() + (i / self.w) as f32 * cs), vec2(cs, cs))
                .shrink(pad);
            let k = if c.open { ((now - c.t) / 0.15).clamp(0.0, 1.0) as f32 } else { 0.0 };
            if k < 1.0 {
                // hidden, raised tile
                let hov = hover == Some(i) && self.state == State::Playing && !c.open;
                let top = if hov { Color32::from_rgb(64, 72, 124) } else { Color32::from_rgb(48, 55, 98) };
                let s = 1.0 - k;
                let rr = Rect::from_center_size(r.center(), r.size() * (0.6 + 0.4 * s));
                p.rect_filled(rr, cs * 0.18, alpha(Color32::from_rgb(30, 34, 64), s));
                p.rect_filled(Rect::from_min_max(rr.min, rr.max - vec2(0.0, cs * 0.07)), cs * 0.18, alpha(top, s));
                if c.flag {
                    draw_flag(&p, r.center(), cs);
                }
            }
            if c.open && k > 0.0 {
                let is_boom = self.exploded == Some(i);
                let bg = if is_boom { RED } else if c.mine { alpha(RED, 0.25) } else { CARD };
                p.rect_filled(r, cs * 0.15, alpha(bg, k));
                if c.mine {
                    draw_mine(&p, r.center(), cs * (0.6 + 0.4 * k));
                } else if c.n > 0 {
                    p.text(r.center(), Align2::CENTER_CENTER, c.n, font.clone(), alpha(NUM_COLORS[c.n as usize - 1], k));
                }
            }
            if self.state == State::Lost && c.flag && !c.mine {
                let s = cs * 0.25;
                let st = Stroke::new(cs * 0.08, RED);
                p.line_segment([r.center() - vec2(s, s), r.center() + vec2(s, s)], st);
                p.line_segment([r.center() + vec2(-s, s), r.center() + vec2(s, -s)], st);
            }
        }

        let t = ui.ctx().animate_bool_with_time(ui.scope_id().with("mines_ov"), self.state != State::Playing, 0.5);
        if t > 0.0 && self.state != State::Playing {
            let el = fmt_time(self.end - self.start.unwrap_or(self.end));
            match self.state {
                State::Won => {
                    let sub = if self.new_best { format!("New best time for {label}: {el}!") } else { format!("Cleared {label} in {el}") };
                    overlay(ui, rect.expand(8.0), t, "Field cleared!", &sub, GREEN);
                }
                _ => overlay(ui, rect.expand(8.0), t * 0.7, "Boom!", "Click New game to try again", RED),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_click_is_safe_and_counts_match() {
        for _ in 0..50 {
            let mut m = Mines::new();
            m.diff = 2;
            m.reset();
            m.reveal(0, 0.0);
            assert!(m.state == State::Playing);
            assert_eq!(m.cells.iter().filter(|c| c.mine).count(), 99);
            assert!(m.cells[0].n == 0 && m.cleared() > 1);
        }
    }
}
