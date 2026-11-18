use crate::stats::{GameStats, Outcome, fmt_time};
use crate::theme::*;
use crate::util::Rng;
use crate::widgets::*;
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use std::f32::consts::{PI, TAU};

const COLOR: Color32 = PINK;
const DIFFS: [(&str, usize, usize); 3] = [("4×4", 4, 4), ("6×4", 6, 4), ("6×6", 6, 6)];
const PALETTE: [Color32; 6] = [RED, AMBER, GREEN, CYAN, ACCENT, PINK];
const SHAPES: usize = 6;

#[derive(Clone, Copy)]
struct Card {
    sym: (usize, usize), // (shape, color)
    up: bool,
    matched: bool,
}

pub struct Memory {
    diff: usize,
    cols: usize,
    rows: usize,
    cards: Vec<Card>,
    first: Option<usize>,
    second: Option<usize>,
    hide_at: Option<f64>,
    moves: u32,
    start: Option<f64>,
    end: Option<f64>,
    round: u64,
    new_best: bool,
    score: u64,
    rng: Rng,
}

impl Memory {
    pub fn new() -> Self {
        let mut m = Self {
            diff: 0,
            cols: 4,
            rows: 4,
            cards: Vec::new(),
            first: None,
            second: None,
            hide_at: None,
            moves: 0,
            start: None,
            end: None,
            round: 0,
            new_best: false,
            score: 0,
            rng: Rng::new(),
        };
        m.reset();
        m
    }

    fn reset(&mut self) {
        let (_, c, r) = DIFFS[self.diff];
        self.cols = c;
        self.rows = r;
        let pairs = c * r / 2;
        let mut syms: Vec<(usize, usize)> = (0..SHAPES).flat_map(|s| (0..PALETTE.len()).map(move |p| (s, p))).collect();
        self.rng.shuffle(&mut syms);
        let mut cards: Vec<Card> =
            syms[..pairs].iter().flat_map(|&s| [Card { sym: s, up: false, matched: false }; 2]).collect();
        self.rng.shuffle(&mut cards);
        self.cards = cards;
        self.first = None;
        self.second = None;
        self.hide_at = None;
        self.moves = 0;
        self.start = None;
        self.end = None;
        self.new_best = false;
        self.score = 0;
        self.round += 1;
    }

    fn pairs(&self) -> usize {
        self.cards.len() / 2
    }

    fn found(&self) -> usize {
        self.cards.iter().filter(|c| c.matched).count() / 2
    }

    fn hide_pending(&mut self) {
        for i in [self.first.take(), self.second.take()].into_iter().flatten() {
            self.cards[i].up = false;
        }
        self.hide_at = None;
    }

    fn click(&mut self, i: usize, now: f64) {
        if self.hide_at.is_some() {
            self.hide_pending();
        }
        if self.cards[i].up || self.cards[i].matched {
            return;
        }
        self.start.get_or_insert(now);
        self.cards[i].up = true;
        match self.first {
            None => self.first = Some(i),
            Some(f) => {
                self.moves += 1;
                if self.cards[f].sym == self.cards[i].sym {
                    self.cards[f].matched = true;
                    self.cards[i].matched = true;
                    self.first = None;
                } else {
                    self.second = Some(i);
                    self.hide_at = Some(now + 0.85);
                }
            }
        }
    }

    fn elapsed(&self, now: f64) -> f64 {
        match (self.start, self.end) {
            (Some(s), Some(e)) => e - s,
            (Some(s), None) => now - s,
            _ => 0.0,
        }
    }
}

fn poly(p: &Painter, c: Pos2, r: f32, n: usize, rot: f32, color: Color32) {
    let pts = (0..n).map(|k| c + egui::Vec2::angled(rot + k as f32 * TAU / n as f32) * r).collect();
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

fn draw_symbol(p: &Painter, c: Pos2, r: f32, shape: usize, color: Color32) {
    match shape {
        0 => {
            p.circle_filled(c, r * 0.85, color);
        }
        1 => {
            p.rect_filled(Rect::from_center_size(c, vec2(r * 1.5, r * 1.5)), r * 0.25, color);
        }
        2 => poly(p, c + vec2(0.0, r * 0.15), r * 1.05, 3, -PI / 2.0, color),
        3 => poly(p, c, r, 4, -PI / 2.0, color),
        4 => {
            // star: 5 convex kites
            for k in 0..5 {
                let a = -PI / 2.0 + k as f32 * TAU / 5.0;
                let pts = vec![
                    c,
                    c + egui::Vec2::angled(a - TAU / 10.0) * r * 0.45,
                    c + egui::Vec2::angled(a) * r * 1.05,
                    c + egui::Vec2::angled(a + TAU / 10.0) * r * 0.45,
                ];
                p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
            }
        }
        _ => poly(p, c, r * 0.95, 6, 0.0, color),
    }
}

impl super::Game for Memory {
    fn id(&self) -> &'static str {
        "memory"
    }
    fn name(&self) -> &'static str {
        "Memory"
    }
    fn icon(&self) -> Icon {
        Icon::Cards
    }
    fn blurb(&self) -> &'static str {
        "Flip cards and find all matching pairs in as few moves as possible."
    }
    fn color(&self) -> Color32 {
        COLOR
    }
    fn running(&self) -> bool {
        self.start.is_some() && self.end.is_none()
    }

    #[cfg(debug_assertions)]
    fn demo(&mut self, now: f64) {
        let a = 0;
        let b = (1..self.cards.len()).find(|&j| self.cards[j].sym == self.cards[a].sym).unwrap();
        self.click(a, now);
        self.click(b, now);
        let c = (1..self.cards.len()).find(|&j| j != b).unwrap();
        let d = (1..self.cards.len()).find(|&j| j != b && j != c && self.cards[j].sym != self.cards[c].sym).unwrap();
        self.click(c, now);
        self.click(d, now);
        self.hide_at = Some(now + 100.0);
        self.start = Some(now - 17.0);
    }

    fn ui(&mut self, ui: &mut Ui, stats: &mut GameStats) {
        let now = ui.input(|i| i.time);
        let label = DIFFS[self.diff].0;
        header(ui, self.icon(), "Memory", "Click two cards to flip them · match every pair", COLOR, |ui| {
            if ui.add(Pill::primary("New game", COLOR)).clicked() {
                self.reset();
            }
            let best = stats.best_times.get(label).map_or("-".to_owned(), |t| fmt_time(*t));
            chip(ui, &format!("Best {label}"), best, AMBER);
            chip(ui, "Time", fmt_time(self.elapsed(now)), TEXT);
            chip(ui, "Moves", self.moves, TEXT);
            chip(ui, "Pairs", format!("{}/{}", self.found(), self.pairs()), COLOR);
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Board").color(MUTED));
            let names: Vec<&str> = DIFFS.iter().map(|d| d.0).collect();
            if segmented(ui, &mut self.diff, &names) {
                self.reset();
            }
        });

        if self.hide_at.is_some_and(|t| now >= t) {
            self.hide_pending();
        }

        let aspect = self.cols as f32 / self.rows as f32 * 0.8;
        let (rect, resp) = board_rect(ui, aspect, self.cols as f32 * 120.0, Sense::click());
        let cw = rect.width() / self.cols as f32;
        let ch = rect.height() / self.rows as f32;
        let (cols, n) = (self.cols, self.cards.len());
        let card_rect = move |i: usize| {
            Rect::from_min_size(pos2(rect.left() + (i % cols) as f32 * cw, rect.top() + (i / cols) as f32 * ch), vec2(cw, ch))
                .shrink(cw.min(ch) * 0.06)
        };
        let hover = resp.hover_pos().and_then(|p| (0..n).find(|&i| card_rect(i).contains(p)));
        if self.end.is_none() && resp.clicked() {
            if let Some(i) = resp.interact_pointer_pos().and_then(|p| (0..n).find(|&i| card_rect(i).contains(p))) {
                self.click(i, now);
            }
        }
        if self.end.is_none() && self.found() == self.pairs() {
            self.end = Some(now);
            let secs = self.elapsed(now);
            let pairs = self.pairs() as i64;
            let waste = self.moves as i64 - pairs;
            self.score = (pairs * 100 - waste * 10 - secs as i64).max(10) as u64;
            stats.record(Outcome::Win, self.score);
            self.new_best = stats.record_time(label, secs);
        }

        let p = ui.painter().clone();
        let mut animating = self.hide_at.is_some() || self.running();
        for (i, c) in self.cards.iter().enumerate() {
            let id = ui.scope_id().with(("card", self.round, i));
            let t = ui.ctx().animate_bool_with_time(id, c.up || c.matched, 0.28);
            animating |= t > 0.0 && t < 1.0;
            let base = card_rect(i);
            let hov = hover == Some(i) && !c.up && !c.matched && self.end.is_none();
            let lift = ui.ctx().animate_bool_with_time(id.with("h"), hov, 0.12);
            let base = base.translate(vec2(0.0, -3.0 * lift));
            let sx = (1.0 - 2.0 * t).abs().max(0.02);
            let r = Rect::from_center_size(base.center(), vec2(base.width() * sx, base.height()));
            let rad = base.width().min(base.height()) * 0.12;
            p.rect_filled(r.translate(vec2(0.0, 4.0)), rad, alpha(Color32::BLACK, 0.25));
            if t < 0.5 {
                // back face
                p.rect_filled(r, rad, lerp(Color32::from_rgb(60, 44, 140), Color32::from_rgb(84, 62, 190), lift));
                p.rect_filled(r.shrink(base.width() * 0.07 * sx.max(0.3)), rad * 0.7, alpha(Color32::WHITE, 0.05));
                let s = base.width().min(base.height()) * 0.18;
                poly(&p, r.center(), s * sx.max(0.01), 4, -PI / 2.0, alpha(Color32::WHITE, 0.18));
            } else {
                let (shape, col) = c.sym;
                let color = PALETTE[col];
                let bg = if c.matched { lerp(CARD_HI, color, 0.15) } else { Color32::from_rgb(236, 238, 250) };
                p.rect_filled(r, rad, bg);
                let s = base.width().min(base.height()) * 0.28;
                let sym_col = if c.matched { color } else { lerp(color, Color32::BLACK, 0.1) };
                // squash the symbol horizontally with the flip
                let pr = ui.painter().clone().with_clip_rect(r);
                draw_symbol(&pr, r.center(), s * sx.max(0.05), shape, sym_col);
            }
        }
        if animating {
            ui.ctx().request_repaint();
        }

        if let Some(end) = self.end {
            let k = ((now - end) / 0.5).clamp(0.0, 1.0) as f32;
            if k < 1.0 {
                ui.ctx().request_repaint();
            }
            let sub = format!(
                "{} moves · {} · score {}{}",
                self.moves,
                fmt_time(self.elapsed(now)),
                self.score,
                if self.new_best { " · new best time!" } else { "" }
            );
            overlay(ui, rect.expand(8.0), k, "All pairs found!", &sub, GREEN);
        }
    }
}
