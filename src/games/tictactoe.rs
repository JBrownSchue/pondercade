use crate::stats::{GameStats, Outcome};
use crate::theme::*;
use crate::util::Rng;
use crate::widgets::*;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use std::f32::consts::TAU;

const COLOR: Color32 = CYAN;
const X_COL: Color32 = ACCENT;
const O_COL: Color32 = CYAN;
const MODES: [&str; 4] = ["Easy", "Normal", "Impossible", "2 Players"];
const LINES: [[usize; 3]; 8] = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6]];

pub struct TicTacToe {
    b: [u8; 9],
    turn: u8,
    placed: [f64; 9],
    mode: usize,
    /// Who starts the next round alternates.
    x_starts: bool,
    result: Option<(u8, Option<[usize; 3]>, f64)>,
    ai_at: Option<f64>,
    session: (u32, u32, u32),
    rng: Rng,
}

fn winner(b: &[u8; 9]) -> Option<(u8, Option<[usize; 3]>)> {
    for l in LINES {
        if b[l[0]] != 0 && b[l[0]] == b[l[1]] && b[l[1]] == b[l[2]] {
            return Some((b[l[0]], Some(l)));
        }
    }
    b.iter().all(|&c| c != 0).then_some((0, None))
}

/// Negamax: score from the perspective of `player` to move.
fn negamax(b: &mut [u8; 9], player: u8, depth: i32) -> i32 {
    if let Some((w, _)) = winner(b) {
        return if w == 0 { 0 } else if w == player { 10 - depth } else { depth - 10 };
    }
    let mut best = i32::MIN;
    for i in 0..9 {
        if b[i] == 0 {
            b[i] = player;
            best = best.max(-negamax(b, 3 - player, depth + 1));
            b[i] = 0;
        }
    }
    best
}

impl TicTacToe {
    pub fn new() -> Self {
        Self {
            b: [0; 9],
            turn: 1,
            placed: [0.0; 9],
            mode: 1,
            x_starts: true,
            result: None,
            ai_at: None,
            session: (0, 0, 0),
            rng: Rng::new(),
        }
    }

    fn vs_ai(&self) -> bool {
        self.mode < 3
    }

    fn new_round(&mut self, now: f64) {
        self.b = [0; 9];
        self.result = None;
        self.turn = if self.x_starts { 1 } else { 2 };
        self.x_starts = !self.x_starts;
        self.ai_at = (self.vs_ai() && self.turn == 2).then_some(now + 0.45);
    }

    fn play(&mut self, i: usize, now: f64, stats: &mut GameStats) {
        self.b[i] = self.turn;
        self.placed[i] = now;
        self.turn = 3 - self.turn;
        if let Some((w, line)) = winner(&self.b) {
            self.result = Some((w, line, now));
            self.ai_at = None;
            match w {
                1 => self.session.0 += 1,
                2 => self.session.2 += 1,
                _ => self.session.1 += 1,
            }
            if self.vs_ai() {
                let o = match w {
                    1 => Outcome::Win,
                    2 => Outcome::Loss,
                    _ => Outcome::Draw,
                };
                stats.record(o, 0);
            }
        } else if self.vs_ai() && self.turn == 2 {
            self.ai_at = Some(now + 0.4);
        }
    }

    fn ai_move(&mut self) -> usize {
        let empty: Vec<usize> = (0..9).filter(|&i| self.b[i] == 0).collect();
        let smart = match self.mode {
            0 => self.rng.chance(0.15),
            1 => self.rng.chance(0.65),
            _ => true,
        };
        if !smart {
            return empty[self.rng.below(empty.len())];
        }
        let mut best = (i32::MIN, Vec::new());
        for &i in &empty {
            self.b[i] = 2;
            let s = -negamax(&mut self.b, 1, 1);
            self.b[i] = 0;
            if s > best.0 {
                best = (s, vec![i]);
            } else if s == best.0 {
                best.1.push(i);
            }
        }
        best.1[self.rng.below(best.1.len())]
    }
}

fn arc(center: Pos2, r: f32, from: f32, sweep: f32) -> Vec<Pos2> {
    let n = ((sweep.abs() / TAU) * 48.0).ceil().max(2.0) as usize;
    (0..=n).map(|k| center + egui::Vec2::angled(from + sweep * k as f32 / n as f32) * r).collect()
}

impl super::Game for TicTacToe {
    fn id(&self) -> &'static str {
        "tictactoe"
    }
    fn name(&self) -> &'static str {
        "Tic-Tac-Toe"
    }
    fn icon(&self) -> Icon {
        Icon::Ttt
    }
    fn blurb(&self) -> &'static str {
        "Three in a row against an AI with three skill levels, or a friend."
    }
    fn color(&self) -> Color32 {
        COLOR
    }
    fn running(&self) -> bool {
        self.result.is_none() && self.b.iter().any(|&c| c != 0)
    }

    #[cfg(debug_assertions)]
    fn demo(&mut self, now: f64) {
        let mut st = GameStats::default();
        self.mode = 3;
        for i in [4, 0, 2, 6, 3] {
            self.play(i, now - 1.0, &mut st);
        }
        self.placed[3] = now;
    }

    fn ui(&mut self, ui: &mut Ui, stats: &mut GameStats) {
        let now = ui.input(|i| i.time);
        let ai = self.vs_ai();
        let sub = if ai { "You are X · the AI plays O · starter alternates" } else { "Pass the mouse — X and O take turns" };
        header(ui, self.icon(), "Tic-Tac-Toe", sub, COLOR, |ui| {
            if ui.add(Pill::primary("New round", COLOR)).clicked() {
                self.new_round(now);
            }
            let (x, d, o) = self.session;
            chip(ui, if ai { "AI" } else { "O wins" }, o, O_COL);
            chip(ui, "Draws", d, MUTED);
            chip(ui, if ai { "You" } else { "X wins" }, x, X_COL);
            if ai {
                chip(ui, "Streak", stats.streak, AMBER);
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Opponent").color(MUTED));
            if segmented(ui, &mut self.mode, &MODES) {
                self.session = (0, 0, 0);
                self.x_starts = true;
                self.new_round(now);
            }
        });

        if let Some(t) = self.ai_at {
            if now >= t && self.result.is_none() {
                self.ai_at = None;
                let m = self.ai_move();
                self.play(m, now, stats);
            } else {
                ui.ctx().request_repaint();
            }
        }

        let (rect, resp) = board_rect(ui, 1.0, 460.0, Sense::click());
        let cs = rect.width() / 3.0;
        let cell = |i: usize| Rect::from_min_size(pos2(rect.left() + (i % 3) as f32 * cs, rect.top() + (i / 3) as f32 * cs), vec2(cs, cs));
        let idx = |p: Pos2| (0..9).find(|&i| cell(i).contains(p));
        let human_turn = self.result.is_none() && !(ai && self.turn == 2);
        let hover = resp.hover_pos().and_then(idx).filter(|&i| human_turn && self.b[i] == 0);

        if resp.clicked() {
            if self.result.is_some() {
                self.new_round(now);
            } else if let Some(i) = hover {
                self.play(i, now, stats);
            }
        }

        let p = ui.painter().clone();
        p.rect_filled(rect.expand(14.0), 22.0, PANEL);
        for i in 0..9 {
            let r = cell(i).shrink(cs * 0.05);
            let h = ui.ctx().animate_bool_with_time(ui.scope_id().with(("ttt_h", i)), hover == Some(i), 0.12);
            p.rect_filled(r, cs * 0.12, lerp(CARD, CARD_HI, h));
            if h > 0.0 && self.b[i] == 0 {
                // ghost preview of the current player's mark
                let col = alpha(if self.turn == 1 { X_COL } else { O_COL }, 0.25 * h);
                let c = r.center();
                let s = cs * 0.22;
                if self.turn == 1 {
                    p.line_segment([c - vec2(s, s), c + vec2(s, s)], Stroke::new(cs * 0.08, col));
                    p.line_segment([c + vec2(-s, s), c + vec2(s, -s)], Stroke::new(cs * 0.08, col));
                } else {
                    p.circle_stroke(c, s, Stroke::new(cs * 0.08, col));
                }
            }
        }

        let mut animating = false;
        for i in 0..9 {
            if self.b[i] == 0 {
                continue;
            }
            let k = ((now - self.placed[i]) / 0.28).clamp(0.0, 1.0) as f32;
            animating |= k < 1.0;
            let c = cell(i).center();
            let s = cs * 0.24;
            let w = cs * 0.09;
            if self.b[i] == 1 {
                let st = Stroke::new(w, X_COL);
                let k1 = (k * 2.0).min(1.0);
                let k2 = (k * 2.0 - 1.0).max(0.0);
                let a = c - vec2(s, s);
                p.line_segment([a, a + vec2(2.0 * s, 2.0 * s) * k1], st);
                p.circle_filled(a, w / 2.0, X_COL);
                p.circle_filled(a + vec2(2.0 * s, 2.0 * s) * k1, w / 2.0, X_COL);
                if k2 > 0.0 {
                    let b = c + vec2(s, -s);
                    p.line_segment([b, b + vec2(-2.0 * s, 2.0 * s) * k2], st);
                    p.circle_filled(b, w / 2.0, X_COL);
                    p.circle_filled(b + vec2(-2.0 * s, 2.0 * s) * k2, w / 2.0, X_COL);
                }
            } else {
                p.add(Shape::line(arc(c, s, -TAU / 4.0, TAU * k), Stroke::new(w, O_COL)));
            }
        }

        if let Some((w, line, t0)) = self.result {
            let k = ((now - t0 - 0.2) / 0.3).clamp(0.0, 1.0) as f32;
            animating |= k < 1.0;
            if let Some(l) = line {
                let (a, b) = (cell(l[0]).center(), cell(l[2]).center());
                let dir = (b - a).normalized();
                let (a, b) = (a - dir * cs * 0.3, b + dir * cs * 0.3);
                p.line_segment([a, a + (b - a) * k], Stroke::new(cs * 0.06, alpha(AMBER, 0.9)));
            }
            let (head, color) = match (w, ai) {
                (0, _) => ("Draw", MUTED),
                (1, true) => ("You win!", GREEN),
                (2, true) => ("AI wins", RED),
                (1, false) => ("X wins!", X_COL),
                _ => ("O wins!", O_COL),
            };
            let f = ((now - t0 - 0.6) / 0.3).clamp(0.0, 1.0) as f32;
            animating |= f < 1.0;
            if f > 0.0 {
                overlay(ui, rect.expand(14.0), f * 0.9, head, "Click the board for the next round", color);
            }
        }
        if animating {
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The "Impossible" AI must never lose, whatever X plays.
    #[test]
    fn impossible_ai_never_loses() {
        fn explore(t: &mut TicTacToe, st: &mut GameStats) {
            if t.result.is_some() {
                assert_ne!(t.result.unwrap().0, 1, "AI lost: {:?}", t.b);
                return;
            }
            if t.turn == 2 {
                let m = t.ai_move();
                t.play(m, 0.0, st);
                explore(t, st);
                return;
            }
            for i in 0..9 {
                if t.b[i] == 0 {
                    let (b, turn) = (t.b, t.turn);
                    t.play(i, 0.0, st);
                    explore(t, st);
                    t.b = b;
                    t.turn = turn;
                    t.result = None;
                }
            }
        }
        let mut st = GameStats::default();
        for x_first in [true, false] {
            let mut t = TicTacToe::new();
            t.mode = 2;
            t.x_starts = x_first;
            t.new_round(0.0);
            explore(&mut t, &mut st);
        }
        assert_eq!(st.wins, 0);
    }
}
