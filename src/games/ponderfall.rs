use crate::stats::{GameStats, Outcome};
use crate::theme::*;
use crate::util::Rng;
use crate::widgets::*;
use eframe::egui::{self, Align2, Color32, FontId, Key, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use std::collections::VecDeque;

const W: i32 = 10;
/// Total rows; the top `HIDDEN` rows are above the visible playfield (pieces spawn there).
const H: i32 = 22;
const HIDDEN: i32 = 2;
const VIS: i32 = H - HIDDEN;
/// Width (in cells) of the hold / next panels on either side of the playfield.
const SIDE: f32 = 5.5;
const COLOR: Color32 = ACCENT;
const BLUE: Color32 = Color32::from_rgb(96, 140, 255);
const KIND_COLORS: [Color32; 7] = [CYAN, AMBER, PINK, GREEN, RED, BLUE, ORANGE];
/// Auto-shift: delay before a held left/right key repeats, then the repeat interval.
const DAS: f64 = 0.16;
const ARR: f64 = 0.045;
const LOCK_DELAY: f64 = 0.5;
const MAX_LOCK_RESETS: u32 = 15;

/// The seven four-cell pieces, as cells inside an n×n rotation box (rotation 0).
const SHAPES: [(i32, [(i32, i32); 4]); 7] = [
    (4, [(0, 1), (1, 1), (2, 1), (3, 1)]), // I
    (2, [(0, 0), (1, 0), (0, 1), (1, 1)]), // O
    (3, [(1, 0), (0, 1), (1, 1), (2, 1)]), // T
    (3, [(1, 0), (2, 0), (0, 1), (1, 1)]), // S
    (3, [(0, 0), (1, 0), (1, 1), (2, 1)]), // Z
    (3, [(0, 0), (0, 1), (1, 1), (2, 1)]), // J
    (3, [(2, 0), (0, 1), (1, 1), (2, 1)]), // L
];

fn cells(kind: usize, rot: usize) -> [(i32, i32); 4] {
    let (n, mut c) = SHAPES[kind];
    for _ in 0..rot % 4 {
        c = c.map(|(x, y)| (n - 1 - y, x));
    }
    c
}

#[derive(Clone, Copy)]
struct Piece {
    kind: usize,
    rot: usize,
    x: i32,
    y: i32,
}

impl Piece {
    fn spawn(kind: usize) -> Self {
        let n = SHAPES[kind].0;
        Self { kind, rot: 0, x: (W - n) / 2, y: 1 }
    }
    fn blocks(&self) -> impl Iterator<Item = (i32, i32)> {
        let (x, y) = (self.x, self.y);
        cells(self.kind, self.rot).into_iter().map(move |(cx, cy)| (x + cx, y + cy))
    }
}

#[derive(PartialEq, Clone, Copy)]
enum State {
    Ready,
    Running,
    Paused,
    Over,
}

pub struct Ponderfall {
    grid: [[u8; W as usize]; H as usize],
    cur: Piece,
    hold: Option<usize>,
    hold_used: bool,
    queue: VecDeque<usize>,
    state: State,
    score: u64,
    lines: u32,
    acc: f64,
    lock_timer: f64,
    lock_resets: u32,
    /// Held horizontal direction and when it next auto-repeats.
    das: Option<(i32, f64)>,
    last: Option<f64>,
    /// Rows (visible index) that were just cleared, and when.
    flash: (Vec<i32>, f64),
    /// Points from the last clear, for the floating "+N" label.
    gain: Option<(u64, &'static str, f64)>,
    rng: Rng,
}

impl Ponderfall {
    pub fn new() -> Self {
        let mut g = Self {
            grid: [[0; W as usize]; H as usize],
            cur: Piece::spawn(0),
            hold: None,
            hold_used: false,
            queue: VecDeque::new(),
            state: State::Ready,
            score: 0,
            lines: 0,
            acc: 0.0,
            lock_timer: 0.0,
            lock_resets: 0,
            das: None,
            last: None,
            flash: (Vec::new(), -10.0),
            gain: None,
            rng: Rng::new(),
        };
        g.reset();
        g
    }

    fn reset(&mut self) {
        self.grid = [[0; W as usize]; H as usize];
        self.queue.clear();
        self.hold = None;
        self.state = State::Ready;
        self.score = 0;
        self.lines = 0;
        self.gain = None;
        self.flash.0.clear();
        self.next_piece();
    }

    fn level(&self) -> u32 {
        1 + self.lines / 10
    }

    /// Seconds per row at the current level.
    fn gravity(&self) -> f64 {
        let l = (self.level() - 1) as f64;
        (0.8 - l * 0.007).powf(l).max(0.02)
    }

    fn refill(&mut self) {
        while self.queue.len() < 7 {
            let mut bag: Vec<usize> = (0..7).collect();
            self.rng.shuffle(&mut bag);
            self.queue.extend(bag);
        }
    }

    fn next_piece(&mut self) {
        self.refill();
        let k = self.queue.pop_front().unwrap();
        self.start_piece(k);
    }

    fn start_piece(&mut self, kind: usize) {
        self.cur = Piece::spawn(kind);
        self.acc = 0.0;
        self.lock_timer = 0.0;
        self.lock_resets = 0;
        if !self.fits(&self.cur) {
            self.state = State::Over;
        }
    }

    fn fits(&self, p: &Piece) -> bool {
        p.blocks().all(|(x, y)| (0..W).contains(&x) && (0..H).contains(&y) && self.grid[y as usize][x as usize] == 0)
    }

    fn grounded(&self) -> bool {
        !self.fits(&Piece { y: self.cur.y + 1, ..self.cur })
    }

    /// Successful moves while resting on the stack restart the lock delay (a limited number of times).
    fn moved(&mut self) {
        if self.grounded() && self.lock_resets < MAX_LOCK_RESETS {
            self.lock_timer = 0.0;
            self.lock_resets += 1;
        }
    }

    fn shift(&mut self, dx: i32) -> bool {
        let p = Piece { x: self.cur.x + dx, ..self.cur };
        if self.fits(&p) {
            self.cur = p;
            self.moved();
            true
        } else {
            false
        }
    }

    fn rotate(&mut self, dir: i32) {
        if self.cur.kind == 1 {
            return;
        }
        let rot = (self.cur.rot as i32 + dir).rem_euclid(4) as usize;
        let kicks: &[(i32, i32)] = if self.cur.kind == 0 {
            &[(0, 0), (-1, 0), (1, 0), (-2, 0), (2, 0), (0, -1), (0, -2)]
        } else {
            &[(0, 0), (-1, 0), (1, 0), (0, -1), (-1, -1), (1, -1)]
        };
        for &(dx, dy) in kicks {
            let p = Piece { rot, x: self.cur.x + dx, y: self.cur.y + dy, ..self.cur };
            if self.fits(&p) {
                self.cur = p;
                self.moved();
                return;
            }
        }
    }

    fn ghost_y(&self) -> i32 {
        let mut p = self.cur;
        while self.fits(&Piece { y: p.y + 1, ..p }) {
            p.y += 1;
        }
        p.y
    }

    fn hard_drop(&mut self, now: f64) {
        let y = self.ghost_y();
        self.score += 2 * (y - self.cur.y) as u64;
        self.cur.y = y;
        self.lock(now);
    }

    fn hold_piece(&mut self) {
        if self.hold_used {
            return;
        }
        let k = self.cur.kind;
        match self.hold.replace(k) {
            Some(h) => self.start_piece(h),
            None => self.next_piece(),
        }
        self.hold_used = true;
    }

    fn lock(&mut self, now: f64) {
        let cur = self.cur;
        for (x, y) in cur.blocks() {
            self.grid[y as usize][x as usize] = cur.kind as u8 + 1;
        }
        // Locked entirely above the visible field: the stack has topped out.
        if cur.blocks().all(|(_, y)| y < HIDDEN) {
            self.state = State::Over;
            return;
        }
        let full: Vec<i32> = (0..H).filter(|&y| self.grid[y as usize].iter().all(|&c| c != 0)).collect();
        if !full.is_empty() {
            let mut rows: Vec<[u8; W as usize]> = self.grid.iter().enumerate().filter(|(y, _)| !full.contains(&(*y as i32))).map(|(_, r)| *r).collect();
            while rows.len() < H as usize {
                rows.insert(0, [0; W as usize]);
            }
            self.grid.copy_from_slice(&rows);
            let n = full.len();
            let pts = [0, 100, 300, 500, 800][n] * self.level() as u64;
            self.score += pts;
            self.lines += n as u32;
            self.gain = Some((pts, ["", "Single", "Double", "Triple", "Quad!"][n], now));
            self.flash = (full.into_iter().map(|y| y - HIDDEN).collect(), now);
        }
        self.hold_used = false;
        self.next_piece();
    }

    fn handle_input(&mut self, ui: &Ui, now: f64) {
        let mut presses = Vec::new();
        let (left, right, down) = ui.input(|i| {
            for e in &i.events {
                if let egui::Event::Key { key, pressed: true, repeat: false, .. } = e {
                    presses.push(*key);
                }
            }
            (
                i.key_down(Key::ArrowLeft) || i.key_down(Key::A),
                i.key_down(Key::ArrowRight) || i.key_down(Key::D),
                i.key_down(Key::ArrowDown) || i.key_down(Key::S),
            )
        });
        let pressed = |ks: &[Key]| presses.iter().any(|k| ks.contains(k));
        let pause = pressed(&[Key::P, Key::Escape]);
        let start = pressed(&[Key::Space, Key::Enter]);
        match self.state {
            State::Ready if start => {
                self.state = State::Running;
                return;
            }
            State::Running if pause => {
                self.state = State::Paused;
                return;
            }
            State::Paused if pause || start => {
                self.state = State::Running;
                return;
            }
            State::Over if start => {
                self.reset();
                return;
            }
            State::Running => {}
            _ => return,
        }

        // Left / right with auto-shift: the most recent press wins.
        for k in &presses {
            let d = match k {
                Key::ArrowLeft | Key::A => -1,
                Key::ArrowRight | Key::D => 1,
                _ => continue,
            };
            self.shift(d);
            self.das = Some((d, now + DAS));
        }
        if let Some((d, mut next)) = self.das {
            if (d < 0 && !left) || (d > 0 && !right) {
                self.das = None;
            } else {
                while now >= next {
                    if !self.shift(d) {
                        next = now;
                        break;
                    }
                    next += ARR;
                }
                self.das = Some((d, next.max(now)));
            }
        }
        if pressed(&[Key::ArrowUp, Key::W, Key::X]) {
            self.rotate(1);
        }
        if pressed(&[Key::Z, Key::Q]) {
            self.rotate(-1);
        }
        if pressed(&[Key::C, Key::H]) {
            self.hold_piece();
        }
        if pressed(&[Key::Space]) {
            self.hard_drop(now);
            return;
        }

        // Gravity (soft drop = fast gravity, 1 point per row).
        let dt = self.last.map_or(0.0, |l| (now - l).min(0.1));
        let interval = if down { self.gravity().min(0.04) } else { self.gravity() };
        self.acc += dt;
        while self.acc >= interval && self.state == State::Running {
            self.acc -= interval;
            if self.grounded() {
                self.acc = 0.0;
                break;
            }
            self.cur.y += 1;
            if down {
                self.score += 1;
            }
        }
        if self.grounded() {
            self.lock_timer += dt;
            if self.lock_timer >= LOCK_DELAY || self.lock_resets >= MAX_LOCK_RESETS {
                self.lock(now);
            }
        } else {
            self.lock_timer = 0.0;
        }
    }
}

fn block(p: &egui::Painter, r: Rect, color: Color32) {
    let r = r.shrink(r.width() * 0.05);
    let rad = r.width() * 0.18;
    p.rect_filled(r, rad, color);
    let shine = Rect::from_min_max(r.min + vec2(r.width() * 0.14, r.height() * 0.12), pos2(r.max.x - r.width() * 0.14, r.min.y + r.height() * 0.3));
    p.rect_filled(shine, rad * 0.6, alpha(Color32::WHITE, 0.22));
    let shade = Rect::from_min_max(pos2(r.min.x, r.max.y - r.height() * 0.16), r.max);
    p.rect_filled(shade, egui::CornerRadius { nw: 0, ne: 0, sw: rad as u8, se: rad as u8 }, alpha(Color32::BLACK, 0.18));
}

/// Draws a piece's rotation-0 shape centered in `area`.
fn mini_piece(p: &egui::Painter, area: Rect, kind: usize, cs: f32, color: Color32) {
    let c = cells(kind, 0);
    let (minx, maxx) = (c.iter().map(|b| b.0).min().unwrap(), c.iter().map(|b| b.0).max().unwrap());
    let (miny, maxy) = (c.iter().map(|b| b.1).min().unwrap(), c.iter().map(|b| b.1).max().unwrap());
    let size = vec2((maxx - minx + 1) as f32, (maxy - miny + 1) as f32) * cs;
    let origin = area.center() - size / 2.0;
    for (x, y) in c {
        let r = Rect::from_min_size(origin + vec2((x - minx) as f32 * cs, (y - miny) as f32 * cs), vec2(cs, cs));
        block(p, r, color);
    }
}

impl super::Game for Ponderfall {
    fn id(&self) -> &'static str {
        "ponderfall"
    }
    fn name(&self) -> &'static str {
        "Ponderfall"
    }
    fn icon(&self) -> Icon {
        Icon::Blocks
    }
    fn blurb(&self) -> &'static str {
        "Stack falling pieces and clear full lines. It speeds up every 10 lines."
    }
    fn color(&self) -> Color32 {
        COLOR
    }
    fn running(&self) -> bool {
        self.state == State::Running
    }

    #[cfg(debug_assertions)]
    fn demo(&mut self, _now: f64) {
        for y in (H - 6)..H {
            for x in 0..W {
                if (x * 7 + y * 3) % 10 != 0 && !(y < H - 3 && x > 6) {
                    self.grid[y as usize][x as usize] = ((x + y) % 7 + 1) as u8;
                }
            }
        }
        self.hold = Some(2);
        self.cur = Piece { y: 9, ..Piece::spawn(0) };
        self.score = 4280;
        self.lines = 23;
        self.state = State::Running;
    }

    fn ui(&mut self, ui: &mut Ui, stats: &mut GameStats) {
        let now = ui.input(|i| i.time);

        header(ui, self.icon(), "Ponderfall", "Arrows / WASD move · Up rotates · Space drops · C holds · P pauses", COLOR, |ui| {
            if ui.add(Pill::primary("New game", COLOR)).clicked() {
                self.reset();
            }
            chip(ui, "Best", stats.best_score.max(self.score), AMBER);
            chip(ui, "Score", self.score, TEXT);
            chip(ui, "Lines", self.lines, CYAN);
            chip(ui, "Level", self.level(), COLOR);
        });

        let was_over = self.state == State::Over;
        self.handle_input(ui, now);
        self.last = Some(now);
        if self.state == State::Running {
            ui.ctx().request_repaint();
        }
        if !was_over && self.state == State::Over {
            stats.record(Outcome::Score, self.score);
        }

        let total_w = W as f32 + 2.0 * SIDE;
        let (rect, _) = board_rect(ui, total_w / VIS as f32, 760.0, Sense::hover());
        let cs = rect.height() / VIS as f32;
        let field = Rect::from_min_size(rect.min + vec2(SIDE * cs, 0.0), vec2(W as f32 * cs, VIS as f32 * cs));
        let cell = |x: i32, y: i32| Rect::from_min_size(pos2(field.left() + x as f32 * cs, field.top() + (y - HIDDEN) as f32 * cs), vec2(cs, cs));
        let p = ui.painter().clone();

        // Playfield
        p.rect_filled(field.expand(8.0), 16.0, PANEL);
        for y in HIDDEN..H {
            for x in 0..W {
                p.rect_filled(cell(x, y).shrink(cs * 0.06), cs * 0.15, alpha(CARD, 0.5));
            }
        }
        let fp = p.with_clip_rect(field.expand(1.0));
        for y in 0..H {
            for x in 0..W {
                let v = self.grid[y as usize][x as usize];
                if v != 0 {
                    let col = if self.state == State::Over { lerp(KIND_COLORS[v as usize - 1], MUTED, 0.6) } else { KIND_COLORS[v as usize - 1] };
                    block(&fp, cell(x, y), col);
                }
            }
        }
        if self.state != State::Over {
            let col = KIND_COLORS[self.cur.kind];
            let gy = self.ghost_y();
            for (x, y) in (Piece { y: gy, ..self.cur }).blocks() {
                let r = cell(x, y).shrink(cs * 0.08);
                fp.rect_filled(r, cs * 0.16, alpha(col, 0.10));
                fp.rect_stroke(r, cs * 0.16, Stroke::new(1.5, alpha(col, 0.55)), StrokeKind::Inside);
            }
            // Pieces about to lock fade slightly to signal it.
            let fade = if self.grounded() { (self.lock_timer / LOCK_DELAY) as f32 * 0.35 } else { 0.0 };
            for (x, y) in self.cur.blocks() {
                block(&fp, cell(x, y), lerp(col, Color32::WHITE, fade));
            }
        }
        // Line-clear flash
        let k = ((now - self.flash.1) / 0.35) as f32;
        if k < 1.0 {
            for &row in &self.flash.0 {
                let r = Rect::from_min_size(pos2(field.left(), field.top() + row as f32 * cs), vec2(field.width(), cs));
                p.rect_filled(r.expand2(vec2(6.0 * (1.0 - k), 0.0)), cs * 0.2, alpha(Color32::WHITE, 0.55 * (1.0 - k)));
            }
            ui.ctx().request_repaint();
        }
        if let Some((pts, label, t0)) = self.gain {
            let k = ((now - t0) / 1.0) as f32;
            if k < 1.0 {
                let y = field.center().y - 30.0 - k * 40.0;
                p.text(pos2(field.center().x, y), Align2::CENTER_CENTER, label, FontId::proportional(cs * 1.1), alpha(AMBER, 1.0 - k));
                p.text(pos2(field.center().x, y + cs * 1.1), Align2::CENTER_CENTER, format!("+{pts}"), FontId::proportional(cs * 0.8), alpha(TEXT, 1.0 - k));
                ui.ctx().request_repaint();
            }
        }

        // Side panels: hold (left) and next (right).
        let label = |pos: egui::Pos2, text: &str| {
            p.text(pos, Align2::CENTER_TOP, text, FontId::proportional((cs * 0.55).clamp(10.0, 14.0)), MUTED);
        };
        let pad = cs * 0.5;
        let hold_box = Rect::from_min_size(pos2(rect.left(), rect.top()), vec2((SIDE - 0.6) * cs, 4.0 * cs)).translate(vec2(-pad * 0.2, 0.0));
        p.rect_filled(hold_box, 14.0, PANEL);
        label(hold_box.center_top() + vec2(0.0, cs * 0.3), "HOLD");
        if let Some(h) = self.hold {
            let col = if self.hold_used { lerp(KIND_COLORS[h], MUTED, 0.6) } else { KIND_COLORS[h] };
            mini_piece(&p, hold_box.shrink(cs * 0.3).translate(vec2(0.0, cs * 0.4)), h, cs * 0.75, col);
        }
        let next_box = Rect::from_min_size(pos2(field.right() + pad + 8.0, rect.top()), vec2((SIDE - 0.6) * cs, 10.5 * cs));
        p.rect_filled(next_box, 14.0, PANEL);
        label(next_box.center_top() + vec2(0.0, cs * 0.3), "NEXT");
        for (i, &k) in self.queue.iter().take(3).enumerate() {
            let s = if i == 0 { 0.75 } else { 0.6 };
            let area = Rect::from_center_size(pos2(next_box.center().x, next_box.top() + cs * (2.6 + 2.8 * i as f32)), vec2(next_box.width(), cs * 2.4));
            mini_piece(&p, area, k, cs * s, if i == 0 { KIND_COLORS[k] } else { alpha(KIND_COLORS[k], 0.75) });
        }

        let (head, sub, color) = match self.state {
            State::Ready => ("Ready?", "Press Space to start".to_owned(), COLOR),
            State::Paused => ("Paused", "Press P or Space to resume".to_owned(), CYAN),
            State::Over => ("Game over", format!("Score {}  ·  {} lines  ·  Space to retry", self.score, self.lines), RED),
            State::Running => ("", String::new(), COLOR),
        };
        let t = ui.ctx().animate_bool_with_time(ui.scope_id().with("pf_ov"), self.state != State::Running, 0.25);
        if t > 0.0 && self.state != State::Running {
            overlay(ui, field.expand(8.0), t, head, &sub, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_cycle_and_stay_in_box() {
        for k in 0..7 {
            let n = SHAPES[k].0;
            let mut a = cells(k, 0);
            let mut b = cells(k, 4);
            a.sort();
            b.sort();
            assert_eq!(a, b);
            for r in 0..4 {
                assert!(cells(k, r).iter().all(|&(x, y)| (0..n).contains(&x) && (0..n).contains(&y)));
            }
        }
    }

    #[test]
    fn clearing_lines_scores_and_collapses() {
        let mut g = Ponderfall::new();
        g.state = State::Running;
        // Fill the bottom two rows except a 1-wide gap at x = 9, then drop a vertical I into it.
        for y in [H - 1, H - 2] {
            for x in 0..9 {
                g.grid[y as usize][x as usize] = 2;
            }
        }
        g.grid[(H - 3) as usize][0] = 3;
        g.cur = Piece { kind: 0, rot: 1, x: 7, y: 0 }; // vertical I occupies column x + 2
        g.hard_drop(0.0);
        assert_eq!(g.lines, 2);
        assert!(g.score >= 300);
        // The leftover block from row H-3 has fallen two rows, and the I's top two cells sit above it.
        assert_eq!(g.grid[(H - 1) as usize][0], 3);
        assert_eq!(g.grid[(H - 1) as usize][9], 1);
        assert_eq!(g.grid[(H - 2) as usize][9], 1);
        assert!(g.grid[(H - 3) as usize].iter().all(|&c| c == 0));
    }

    #[test]
    fn topping_out_ends_the_game() {
        let mut g = Ponderfall::new();
        for y in 0..H {
            for x in 0..W {
                if x != 0 {
                    g.grid[y as usize][x as usize] = 1;
                }
            }
        }
        g.next_piece();
        assert!(g.state == State::Over);
    }
}
