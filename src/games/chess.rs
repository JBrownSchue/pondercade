use crate::stats::{GameStats, Outcome};
use crate::theme::*;
use crate::util::Rng;
use crate::widgets::*;
use eframe::egui::{self, Align2, Color32, CursorIcon, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const COLOR: Color32 = Color32::from_rgb(110, 160, 255);
const MODES: [&str; 4] = ["Easy", "Normal", "Hard", "2 Players"];
const SIDES: [&str; 2] = ["White", "Black"];
const LIGHT_SQ: Color32 = Color32::from_rgb(96, 104, 160);
const DARK_SQ: Color32 = Color32::from_rgb(54, 60, 104);
const W_BODY: Color32 = Color32::from_rgb(246, 245, 250);
const W_LINE: Color32 = Color32::from_rgb(22, 24, 42);
const B_BODY: Color32 = Color32::from_rgb(26, 28, 48);
const B_LINE: Color32 = Color32::from_rgb(200, 204, 236);

// ---------------------------------------------------------------------------------------------
// Rules engine. Squares are 0..64 with a1 = 0, h1 = 7, a8 = 56. Pieces are signed: + white, - black.
// ---------------------------------------------------------------------------------------------

const P: i8 = 1;
const N: i8 = 2;
const B: i8 = 3;
const R: i8 = 4;
const Q: i8 = 5;
const K: i8 = 6;
const VAL: [i32; 7] = [0, 100, 320, 330, 500, 900, 0];
const KNIGHT: [(i32, i32); 8] = [(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)];
/// First four orthogonal, last four diagonal.
const DIRS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
const MATE: i32 = 100_000;
const INF: i32 = 2 * MATE;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mv {
    from: u8,
    to: u8,
    /// Promotion piece type (N..=Q), or 0.
    promo: i8,
}

impl Mv {
    fn new(from: usize, to: usize) -> Self {
        Self { from: from as u8, to: to as u8, promo: 0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    b: [i8; 64],
    /// 1 = white to move, -1 = black.
    side: i8,
    /// Castling rights: 1 = white king side, 2 = white queen side, 4 / 8 = black.
    castle: u8,
    /// En-passant target square, or -1.
    ep: i8,
    /// Half-moves since the last capture or pawn move (50-move rule).
    half: u16,
}

fn sq_at(f: i32, r: i32) -> Option<usize> {
    ((0..8).contains(&f) && (0..8).contains(&r)).then_some((r * 8 + f) as usize)
}

impl Pos {
    fn start() -> Self {
        let back = [R, N, B, Q, K, B, N, R];
        let mut b = [0; 64];
        for f in 0..8 {
            b[f] = back[f];
            b[8 + f] = P;
            b[48 + f] = -P;
            b[56 + f] = -back[f];
        }
        Self { b, side: 1, castle: 15, ep: -1, half: 0 }
    }

    fn king(&self, side: i8) -> usize {
        self.b.iter().position(|&p| p == K * side).unwrap_or(0)
    }

    /// Is `sq` attacked by any piece of colour `by`?
    fn attacked(&self, sq: usize, by: i8) -> bool {
        let (f, r) = ((sq % 8) as i32, (sq / 8) as i32);
        let at = |f: i32, r: i32| sq_at(f, r).map(|s| self.b[s]);
        if [-1, 1].iter().any(|df| at(f + df, r - by as i32) == Some(P * by)) {
            return true;
        }
        if KNIGHT.iter().any(|&(df, dr)| at(f + df, r + dr) == Some(N * by)) {
            return true;
        }
        if DIRS.iter().any(|&(df, dr)| at(f + df, r + dr) == Some(K * by)) {
            return true;
        }
        for (i, &(df, dr)) in DIRS.iter().enumerate() {
            let slider = if i < 4 { R } else { B };
            let (mut x, mut y) = (f + df, r + dr);
            while let Some(p) = at(x, y) {
                if p != 0 {
                    if p == slider * by || p == Q * by {
                        return true;
                    }
                    break;
                }
                x += df;
                y += dr;
            }
        }
        false
    }

    fn in_check(&self) -> bool {
        self.attacked(self.king(self.side), -self.side)
    }

    fn is_capture(&self, m: Mv) -> bool {
        self.b[m.to as usize] != 0 || (self.b[m.from as usize].abs() == P && m.to as i8 == self.ep)
    }

    /// Moves that obey piece movement but may leave the own king in check.
    fn pseudo(&self, out: &mut Vec<Mv>) {
        let s = self.side;
        for sq in 0..64 {
            let p = self.b[sq] * s;
            if p <= 0 {
                continue;
            }
            let (f, r) = ((sq % 8) as i32, (sq / 8) as i32);
            match p {
                P => {
                    let r1 = r + s as i32;
                    let push = |to: usize, out: &mut Vec<Mv>| {
                        if r1 == 0 || r1 == 7 {
                            out.extend([Q, N, R, B].map(|promo| Mv { from: sq as u8, to: to as u8, promo }));
                        } else {
                            out.push(Mv::new(sq, to));
                        }
                    };
                    if let Some(to) = sq_at(f, r1) {
                        if self.b[to] == 0 {
                            push(to, out);
                            let start = if s == 1 { 1 } else { 6 };
                            if r == start {
                                let to2 = sq_at(f, r + 2 * s as i32).unwrap();
                                if self.b[to2] == 0 {
                                    out.push(Mv::new(sq, to2));
                                }
                            }
                        }
                    }
                    for df in [-1, 1] {
                        if let Some(to) = sq_at(f + df, r1) {
                            if self.b[to] * s < 0 || to as i8 == self.ep {
                                push(to, out);
                            }
                        }
                    }
                }
                N | K => {
                    for (df, dr) in if p == N { KNIGHT } else { DIRS } {
                        if let Some(to) = sq_at(f + df, r + dr) {
                            if self.b[to] * s <= 0 {
                                out.push(Mv::new(sq, to));
                            }
                        }
                    }
                }
                _ => {
                    let dirs = match p {
                        B => &DIRS[4..],
                        R => &DIRS[..4],
                        _ => &DIRS[..],
                    };
                    for &(df, dr) in dirs {
                        let (mut x, mut y) = (f + df, r + dr);
                        while let Some(to) = sq_at(x, y) {
                            let t = self.b[to] * s;
                            if t > 0 {
                                break;
                            }
                            out.push(Mv::new(sq, to));
                            if t < 0 {
                                break;
                            }
                            x += df;
                            y += dr;
                        }
                    }
                }
            }
        }
        // Castling: king and rook unmoved, squares between empty, king not passing through check.
        let (base, ks, qs) = if s == 1 { (0, 1, 2) } else { (56, 4, 8) };
        if self.b[base + 4] == K * s && self.castle & (ks | qs) != 0 && !self.attacked(base + 4, -s) {
            if self.castle & ks != 0
                && self.b[base + 5] == 0
                && self.b[base + 6] == 0
                && self.b[base + 7] == R * s
                && !self.attacked(base + 5, -s)
                && !self.attacked(base + 6, -s)
            {
                out.push(Mv::new(base + 4, base + 6));
            }
            if self.castle & qs != 0
                && self.b[base + 1] == 0
                && self.b[base + 2] == 0
                && self.b[base + 3] == 0
                && self.b[base] == R * s
                && !self.attacked(base + 3, -s)
                && !self.attacked(base + 2, -s)
            {
                out.push(Mv::new(base + 4, base + 2));
            }
        }
    }

    fn make(&self, m: Mv) -> Pos {
        let mut n = *self;
        let s = self.side;
        let (from, to) = (m.from as usize, m.to as usize);
        let p = n.b[from];
        let kind = p.abs();
        n.half = if kind == P || n.b[to] != 0 { 0 } else { self.half.saturating_add(1) };
        if kind == P && to as i8 == self.ep && n.b[to] == 0 {
            n.b[(to as i32 - 8 * s as i32) as usize] = 0;
        }
        n.b[to] = if m.promo != 0 { m.promo * s } else { p };
        n.b[from] = 0;
        if kind == K && to.abs_diff(from) == 2 {
            let (rf, rt) = if to > from { (from + 3, from + 1) } else { (from - 4, from - 1) };
            n.b[rt] = n.b[rf];
            n.b[rf] = 0;
        }
        n.ep = if kind == P && to.abs_diff(from) == 16 { ((from + to) / 2) as i8 } else { -1 };
        for sq in [from, to] {
            n.castle &= match sq {
                0 => !2,
                7 => !1,
                4 => !3,
                56 => !8,
                63 => !4,
                60 => !12,
                _ => !0,
            };
        }
        n.side = -s;
        n
    }

    fn legal(&self) -> Vec<Mv> {
        let mut v = Vec::with_capacity(48);
        self.pseudo(&mut v);
        let s = self.side;
        v.retain(|&m| {
            let n = self.make(m);
            !n.attacked(n.king(s), -s)
        });
        v
    }

    /// Same position for the repetition rule (ignores the move counters).
    fn same(&self, o: &Pos) -> bool {
        self.b == o.b && self.side == o.side && self.castle == o.castle && self.ep == o.ep
    }

    fn insufficient(&self) -> bool {
        let mut minors = 0;
        for &p in &self.b {
            match p.abs() {
                P | R | Q => return false,
                N | B => minors += 1,
                _ => {}
            }
        }
        minors <= 1
    }

    /// Static evaluation from the side to move's point of view.
    fn eval(&self) -> i32 {
        let mut non_pawn = [0; 2];
        for &p in &self.b {
            if (N..=Q).contains(&p.abs()) {
                non_pawn[usize::from(p < 0)] += VAL[p.abs() as usize];
            }
        }
        let endgame = non_pawn[0] <= 1300 && non_pawn[1] <= 1300;
        let mut sc = 0;
        for (sq, &p) in self.b.iter().enumerate() {
            if p == 0 {
                continue;
            }
            let (f, r) = (sq % 8, sq / 8);
            let i = if p > 0 { (7 - r) * 8 + f } else { r * 8 + f };
            let k = p.abs() as usize;
            let t = match p.abs() {
                P => PST_P[i],
                N => PST_N[i],
                B => PST_B[i],
                R => PST_R[i],
                Q => PST_Q[i],
                _ if endgame => PST_KE[i],
                _ => PST_KM[i],
            };
            let v = VAL[k] + t as i32;
            sc += if p > 0 { v } else { -v };
        }
        sc * self.side as i32
    }
}

// Piece-square tables (from white's view, rank 8 first).
#[rustfmt::skip]
const PST_P: [i16; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,  50, 50, 50, 50, 50, 50, 50, 50,  10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,   0,  0,  0, 20, 20,  0,  0,  0,   5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,   0,  0,  0,  0,  0,  0,  0,  0];
#[rustfmt::skip]
const PST_N: [i16; 64] = [
   -50,-40,-30,-30,-30,-30,-40,-50, -40,-20,  0,  0,  0,  0,-20,-40, -30,  0, 10, 15, 15, 10,  0,-30,
   -30,  5, 15, 20, 20, 15,  5,-30, -30,  0, 15, 20, 20, 15,  0,-30, -30,  5, 10, 15, 15, 10,  5,-30,
   -40,-20,  0,  5,  5,  0,-20,-40, -50,-40,-30,-30,-30,-30,-40,-50];
#[rustfmt::skip]
const PST_B: [i16; 64] = [
   -20,-10,-10,-10,-10,-10,-10,-20, -10,  0,  0,  0,  0,  0,  0,-10, -10,  0,  5, 10, 10,  5,  0,-10,
   -10,  5,  5, 10, 10,  5,  5,-10, -10,  0, 10, 10, 10, 10,  0,-10, -10, 10, 10, 10, 10, 10, 10,-10,
   -10,  5,  0,  0,  0,  0,  5,-10, -20,-10,-10,-10,-10,-10,-10,-20];
#[rustfmt::skip]
const PST_R: [i16; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,   5, 10, 10, 10, 10, 10, 10,  5,  -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,  -5,  0,  0,  0,  0,  0,  0, -5,  -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,   0,  0,  0,  5,  5,  0,  0,  0];
#[rustfmt::skip]
const PST_Q: [i16; 64] = [
   -20,-10,-10, -5, -5,-10,-10,-20, -10,  0,  0,  0,  0,  0,  0,-10, -10,  0,  5,  5,  5,  5,  0,-10,
    -5,  0,  5,  5,  5,  5,  0, -5,   0,  0,  5,  5,  5,  5,  0, -5, -10,  5,  5,  5,  5,  5,  0,-10,
   -10,  0,  5,  0,  0,  0,  0,-10, -20,-10,-10, -5, -5,-10,-10,-20];
#[rustfmt::skip]
const PST_KM: [i16; 64] = [
   -30,-40,-40,-50,-50,-40,-40,-30, -30,-40,-40,-50,-50,-40,-40,-30, -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30, -20,-30,-30,-40,-40,-30,-30,-20, -10,-20,-20,-20,-20,-20,-20,-10,
    20, 20,  0,  0,  0,  0, 20, 20,  20, 30, 10,  0,  0, 10, 30, 20];
#[rustfmt::skip]
const PST_KE: [i16; 64] = [
   -50,-40,-30,-20,-20,-30,-40,-50, -30,-20,-10,  0,  0,-10,-20,-30, -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30, -30,-10, 30, 40, 40, 30,-10,-30, -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-30,  0,  0,  0,  0,-30,-30, -50,-30,-30,-30,-30,-30,-30,-50];

// ---------------------------------------------------------------------------------------------
// Search: alpha-beta negamax with capture quiescence and MVV-LVA move ordering.
// ---------------------------------------------------------------------------------------------

fn order(pos: &Pos, moves: &mut [Mv], first: Option<Mv>) {
    moves.sort_by_cached_key(|&m| {
        let mut s = 0;
        if Some(m) == first {
            s += 1_000_000;
        }
        if pos.is_capture(m) {
            let victim = pos.b[m.to as usize].abs().max(P);
            s += 10_000 + 10 * VAL[victim as usize] - VAL[pos.b[m.from as usize].abs() as usize] / 10;
        }
        if m.promo != 0 {
            s += 9_000 + VAL[m.promo as usize];
        }
        -s
    });
}

struct Search {
    deadline: Option<Instant>,
    nodes: u64,
    stopped: bool,
}

impl Search {
    fn tick(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes & 1023 == 0 && self.deadline.is_some_and(|d| Instant::now() >= d) {
            self.stopped = true;
        }
        self.stopped
    }

    fn negamax(&mut self, pos: &Pos, depth: i32, mut alpha: i32, beta: i32, ply: i32) -> i32 {
        if depth <= 0 {
            return self.quiesce(pos, alpha, beta, 0);
        }
        if self.tick() {
            return 0;
        }
        let mut moves = pos.legal();
        if moves.is_empty() {
            return if pos.in_check() { -MATE + ply } else { 0 };
        }
        if pos.half >= 100 {
            return 0;
        }
        order(pos, &mut moves, None);
        let mut best = -INF;
        for m in moves {
            let s = -self.negamax(&pos.make(m), depth - 1, -beta, -alpha, ply + 1);
            if self.stopped {
                return 0;
            }
            best = best.max(s);
            alpha = alpha.max(s);
            if alpha >= beta {
                break;
            }
        }
        best
    }

    fn quiesce(&mut self, pos: &Pos, mut alpha: i32, beta: i32, qd: i32) -> i32 {
        if self.tick() {
            return 0;
        }
        let stand = pos.eval();
        if stand >= beta || qd >= 8 {
            return stand;
        }
        alpha = alpha.max(stand);
        let mut moves = Vec::with_capacity(16);
        pos.pseudo(&mut moves);
        moves.retain(|&m| pos.is_capture(m) || m.promo == Q);
        order(pos, &mut moves, None);
        for m in moves {
            let n = pos.make(m);
            if n.attacked(n.king(pos.side), -pos.side) {
                continue;
            }
            let s = -self.quiesce(&n, -beta, -alpha, qd + 1);
            if self.stopped {
                return 0;
            }
            if s >= beta {
                return s;
            }
            alpha = alpha.max(s);
        }
        alpha
    }
}

/// Picks the AI's move. `level`: 0 = easy, 1 = normal, 2 = hard.
fn best_move(pos: &Pos, level: usize, rng: &mut Rng) -> Option<Mv> {
    let mut moves = pos.legal();
    if moves.is_empty() {
        return None;
    }
    if level < 2 {
        // Score every root move with a full window, then add noise so weaker levels blunder sometimes.
        if level == 0 && rng.chance(0.15) {
            return Some(moves[rng.below(moves.len())]);
        }
        let (depth, noise) = if level == 0 { (1, 160) } else { (3, 25) };
        let mut sr = Search { deadline: None, nodes: 0, stopped: false };
        let mut best = (i32::MIN, moves[0]);
        for m in moves {
            let s = -sr.negamax(&pos.make(m), depth - 1, -INF, INF, 1);
            let s = if s.abs() > MATE - 100 { s } else { s + rng.below(2 * noise + 1) as i32 - noise as i32 };
            if s > best.0 {
                best = (s, m);
            }
        }
        return Some(best.1);
    }
    // Hard: iterative deepening with a time budget.
    let mut sr = Search { deadline: Some(Instant::now() + Duration::from_millis(1500)), nodes: 0, stopped: false };
    order(pos, &mut moves, None);
    let mut best = moves[0];
    for depth in 1..=8 {
        let mut alpha = -INF;
        let mut cur = None;
        for &m in &moves {
            let s = -sr.negamax(&pos.make(m), depth - 1, -INF, -alpha, 1);
            if sr.stopped {
                break;
            }
            if s > alpha {
                alpha = s;
                cur = Some(m);
            }
        }
        // The previous best is searched first, so a partial iteration's best is still trustworthy.
        if let Some(m) = cur {
            best = m;
        }
        if sr.stopped || alpha >= MATE - 100 {
            break;
        }
        order(pos, &mut moves, Some(best));
    }
    Some(best)
}

// ---------------------------------------------------------------------------------------------
// Game UI
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum End {
    /// Checkmate; the side that won.
    Mate(i8),
    Stalemate,
    Fifty,
    Repetition,
    Material,
}

pub struct Chess {
    pos: Pos,
    /// Every earlier position with the move played from it (for undo, repetition and highlights).
    history: Vec<(Pos, Mv)>,
    /// When the last move was played (slide animation).
    moved_at: f64,
    sel: Option<usize>,
    /// Pawn move waiting for the promotion piece to be picked.
    promo: Option<(usize, usize)>,
    mode: usize,
    play_as: usize,
    thinking: Option<(Receiver<Option<Mv>>, f64)>,
    end: Option<(End, f64)>,
    session: (u32, u32, u32),
    clock: f64,
    last_t: Option<f64>,
}

impl Chess {
    pub fn new() -> Self {
        Self {
            pos: Pos::start(),
            history: Vec::new(),
            moved_at: -10.0,
            sel: None,
            promo: None,
            mode: 1,
            play_as: 0,
            thinking: None,
            end: None,
            session: (0, 0, 0),
            clock: 0.0,
            last_t: None,
        }
    }

    fn vs_ai(&self) -> bool {
        self.mode < 3
    }

    fn human(&self) -> i8 {
        if self.play_as == 0 { 1 } else { -1 }
    }

    fn new_game(&mut self) {
        self.pos = Pos::start();
        self.history.clear();
        self.sel = None;
        self.promo = None;
        self.thinking = None;
        self.end = None;
        self.clock = 0.0;
    }

    fn check_end(&self) -> Option<End> {
        if self.pos.legal().is_empty() {
            return Some(if self.pos.in_check() { End::Mate(-self.pos.side) } else { End::Stalemate });
        }
        if self.pos.half >= 100 {
            return Some(End::Fifty);
        }
        if self.history.iter().filter(|(p, _)| p.same(&self.pos)).count() >= 2 {
            return Some(End::Repetition);
        }
        self.pos.insufficient().then_some(End::Material)
    }

    fn play(&mut self, m: Mv, now: f64, stats: &mut GameStats) {
        self.history.push((self.pos, m));
        self.pos = self.pos.make(m);
        self.moved_at = now;
        self.sel = None;
        self.promo = None;
        if let Some(e) = self.check_end() {
            self.end = Some((e, now));
            let winner = if let End::Mate(w) = e { w } else { 0 };
            if self.vs_ai() {
                let human = self.human();
                match winner {
                    0 => self.session.1 += 1,
                    w if w == human => self.session.0 += 1,
                    _ => self.session.2 += 1,
                }
                let o = match winner {
                    0 => Outcome::Draw,
                    w if w == human => Outcome::Win,
                    _ => Outcome::Loss,
                };
                stats.record(o, 0);
                if o == Outcome::Win {
                    stats.record_time(MODES[self.mode], self.clock);
                }
            } else {
                match winner {
                    1 => self.session.0 += 1,
                    -1 => self.session.2 += 1,
                    _ => self.session.1 += 1,
                }
            }
        }
    }

    fn undo(&mut self) {
        self.thinking = None;
        self.sel = None;
        self.promo = None;
        if let Some((p, _)) = self.history.pop() {
            self.pos = p;
        }
        // Against the AI, go back to the player's own turn.
        while self.vs_ai() && self.pos.side != self.human() {
            let Some((p, _)) = self.history.pop() else { break };
            self.pos = p;
        }
        self.moved_at = -10.0;
    }

    fn start_ai(&mut self, now: f64) {
        let (pos, level) = (self.pos, self.mode);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut rng = Rng::new();
            let _ = tx.send(best_move(&pos, level, &mut rng));
        });
        self.thinking = Some((rx, now));
    }
}

// --- piece drawing --------------------------------------------------------------------------

enum Part {
    Poly(Vec<Pos2>),
    Circle(Pos2, f32),
}

/// Draws a piece as a filled vector silhouette with an outline (no font glyphs needed).
fn draw_piece(p: &Painter, c: Pos2, s: f32, kind: i8, white: bool, fade: f32) {
    let (body, line) = if white { (W_BODY, W_LINE) } else { (B_BODY, B_LINE) };
    let (body, line) = (alpha(body, fade), alpha(line, fade));
    let pt = |x: f32, y: f32| c + vec2(x * s, y * s);
    let poly = |pts: &[(f32, f32)]| Part::Poly(pts.iter().map(|&(x, y)| pt(x, y)).collect());
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    let circle = |x: f32, y: f32, r: f32| Part::Circle(pt(x, y), r * s);

    let mut parts = vec![poly(&[(-0.26, 0.25), (0.26, 0.25), (0.31, 0.37), (-0.31, 0.37)])];
    match kind {
        P => {
            parts.push(poly(&[(-0.19, 0.26), (-0.08, 0.0), (0.08, 0.0), (0.19, 0.26)]));
            parts.push(rect(-0.14, -0.05, 0.14, 0.02));
            parts.push(circle(0.0, -0.16, 0.12));
        }
        N => {
            parts.push(poly(&[(-0.16, 0.26), (-0.14, 0.02), (-0.06, -0.30), (0.06, -0.34), (0.20, -0.10), (0.22, 0.26)]));
            parts.push(poly(&[(-0.02, -0.33), (0.08, -0.20), (-0.20, -0.02), (-0.30, -0.12)]));
            parts.push(poly(&[(0.0, -0.30), (0.07, -0.43), (0.12, -0.29)]));
        }
        B => {
            parts.push(poly(&[(-0.16, 0.26), (-0.07, 0.02), (0.07, 0.02), (0.16, 0.26)]));
            parts.push(rect(-0.14, -0.03, 0.14, 0.04));
            let mitre: Vec<Pos2> = (0..24).map(|k| {
                let a = k as f32 / 24.0 * std::f32::consts::TAU;
                pt(0.12 * a.cos(), -0.17 + 0.17 * a.sin())
            }).collect();
            parts.push(Part::Poly(mitre));
            parts.push(circle(0.0, -0.37, 0.05));
        }
        R => {
            parts.push(poly(&[(-0.16, -0.14), (0.16, -0.14), (0.18, 0.26), (-0.18, 0.26)]));
            parts.push(rect(-0.22, 0.17, 0.22, 0.26));
            parts.push(poly(&[(-0.24, -0.24), (0.24, -0.24), (0.20, -0.10), (-0.20, -0.10)]));
            for (x0, x1) in [(-0.24, -0.13), (-0.055, 0.055), (0.13, 0.24)] {
                parts.push(rect(x0, -0.36, x1, -0.22));
            }
        }
        Q => {
            parts.push(poly(&[(-0.20, 0.26), (-0.09, -0.06), (0.09, -0.06), (0.20, 0.26)]));
            parts.push(poly(&[(-0.25, -0.24), (0.25, -0.24), (0.12, -0.02), (-0.12, -0.02)]));
            for (x, y) in [(-0.25, -0.27), (-0.125, -0.29), (0.0, -0.32), (0.125, -0.29), (0.25, -0.27)] {
                parts.push(circle(x, y, 0.055));
            }
        }
        _ => {
            parts.push(poly(&[(-0.20, 0.26), (-0.09, -0.06), (0.09, -0.06), (0.20, 0.26)]));
            parts.push(poly(&[(-0.23, -0.22), (0.23, -0.22), (0.12, -0.02), (-0.12, -0.02)]));
            parts.push(rect(-0.035, -0.44, 0.035, -0.20));
            parts.push(rect(-0.11, -0.38, 0.11, -0.31));
        }
    }

    // Outline pass (each part slightly enlarged), then the body on top so inner seams disappear.
    let ow = (s * 0.04).max(1.2);
    for part in &parts {
        match part {
            Part::Poly(pts) => p.add(Shape::convex_polygon(pts.clone(), line, Stroke::new(ow * 2.0, line))),
            Part::Circle(c, r) => p.add(Shape::circle_filled(*c, r + ow, line)),
        };
    }
    for part in parts {
        match part {
            Part::Poly(pts) => p.add(Shape::convex_polygon(pts, body, Stroke::NONE)),
            Part::Circle(c, r) => p.add(Shape::circle_filled(c, r, body)),
        };
    }
    match kind {
        N => {
            p.circle_filled(pt(-0.05, -0.22), s * 0.028, line);
        }
        B => {
            p.line_segment([pt(-0.02, -0.24), pt(0.07, -0.12)], Stroke::new(ow, line));
        }
        _ => {}
    }
}

impl super::Game for Chess {
    fn id(&self) -> &'static str {
        "chess"
    }
    fn name(&self) -> &'static str {
        "Chess"
    }
    fn icon(&self) -> Icon {
        Icon::Chess
    }
    fn blurb(&self) -> &'static str {
        "The classic. Take on an AI at three levels, or play a friend on one screen."
    }
    fn color(&self) -> Color32 {
        COLOR
    }
    fn running(&self) -> bool {
        self.end.is_none() && !self.history.is_empty()
    }

    #[cfg(debug_assertions)]
    fn demo(&mut self, now: f64) {
        let mut st = GameStats::default();
        self.mode = 3;
        // A quiet Italian-game opening, ending with a knight selected.
        for (f, t) in [(12, 28), (52, 36), (6, 21), (57, 42), (5, 26), (61, 34), (10, 18)] {
            self.play(Mv::new(f, t), now - 1.0, &mut st);
        }
        self.sel = Some(62);
    }

    fn ui(&mut self, ui: &mut Ui, stats: &mut GameStats) {
        let now = ui.input(|i| i.time);
        let dt = self.last_t.map_or(0.0, |t| (now - t).clamp(0.0, 3.0));
        self.last_t = Some(now);
        if self.running() {
            self.clock += dt;
        }
        let ai = self.vs_ai();
        let human = self.human();

        let sub = if ai { "Click a piece, then a highlighted square · AI levels: Easy, Normal, Hard" } else { "Pass the mouse — White and Black take turns" };
        header(ui, self.icon(), "Chess", sub, COLOR, |ui| {
            if ui.add(Pill::primary("New game", COLOR)).clicked() {
                self.new_game();
            }
            if self.end.is_none() && !self.history.is_empty() && ui.add(Pill::new("Undo")).clicked() {
                self.undo();
            }
            let (w, d, l) = self.session;
            chip(ui, if ai { "AI" } else { "Black" }, l, MUTED);
            chip(ui, "Draws", d, MUTED);
            chip(ui, if ai { "You" } else { "White" }, w, COLOR);
            if ai {
                chip(ui, "Streak", stats.streak, AMBER);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Opponent").color(MUTED));
            if segmented(ui, &mut self.mode, &MODES) {
                self.session = (0, 0, 0);
                self.new_game();
            }
            if self.vs_ai() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Play as").color(MUTED));
                if segmented(ui, &mut self.play_as, &SIDES) {
                    self.new_game();
                }
            }
            ui.add_space(8.0);
            let to_move = if self.pos.side == 1 { "White" } else { "Black" };
            let (status, col) = match self.end {
                Some(_) => (String::new(), MUTED),
                None if self.thinking.is_some() => ("AI is thinking…".to_owned(), AMBER),
                None if self.pos.in_check() => (if ai && self.pos.side == human { "Check! Your move".to_owned() } else { format!("Check! {to_move} to move") }, RED),
                None if ai && self.pos.side == human => ("Your move".to_owned(), GREEN),
                None => (format!("{to_move} to move"), TEXT),
            };
            ui.label(egui::RichText::new(status).color(col).strong());
        });

        // AI turn: think on a background thread so the window stays responsive.
        if self.vs_ai() && self.end.is_none() && self.pos.side != self.human() {
            match &self.thinking {
                None => self.start_ai(now),
                Some((rx, started)) => {
                    // Give the player's own move a moment to land before answering.
                    if now - started >= 0.45 {
                        if let Ok(res) = rx.try_recv() {
                            self.thinking = None;
                            if let Some(m) = res {
                                self.play(m, now, stats);
                            }
                        }
                    }
                }
            }
            ui.ctx().request_repaint();
        }

        let (rect, resp) = board_rect(ui, 1.0, 680.0, Sense::click());
        let cs = rect.width() / 8.0;
        let flipped = self.vs_ai() && human == -1;
        let sq_rect = |sq: usize| {
            let (f, r) = (sq % 8, sq / 8);
            let (col, row) = if flipped { (7 - f, r) } else { (f, 7 - r) };
            Rect::from_min_size(pos2(rect.left() + col as f32 * cs, rect.top() + row as f32 * cs), vec2(cs, cs))
        };
        let sq_at_pos = |p: Pos2| -> Option<usize> {
            if !rect.contains(p) {
                return None;
            }
            let col = (((p.x - rect.left()) / cs) as usize).min(7);
            let row = (((p.y - rect.top()) / cs) as usize).min(7);
            Some(if flipped { row * 8 + (7 - col) } else { (7 - row) * 8 + col })
        };

        let human_turn = self.end.is_none() && self.thinking.is_none() && (!self.vs_ai() || self.pos.side == human);
        let legal = if human_turn { self.pos.legal() } else { Vec::new() };
        let targets: Vec<usize> = self.sel.map_or(Vec::new(), |s| legal.iter().filter(|m| m.from as usize == s).map(|m| m.to as usize).collect());
        let promo_opts = |to: usize| -> Vec<(i8, Rect)> {
            // Options stack from the target square towards the middle of the board.
            let r0 = sq_rect(to);
            let step = if r0.top() <= rect.top() + 1.0 { cs } else { -cs };
            [Q, N, R, B].iter().enumerate().map(|(i, &k)| (k, r0.translate(vec2(0.0, step * i as f32)))).collect()
        };
        let hover = resp.hover_pos().and_then(sq_at_pos);

        if resp.clicked() {
            let click = resp.interact_pointer_pos().and_then(sq_at_pos);
            if self.end.is_some() {
                if now - self.end.unwrap().1 > 0.6 {
                    self.new_game();
                }
            } else if let Some((from, to)) = self.promo {
                let pos = resp.interact_pointer_pos();
                let pick = promo_opts(to).into_iter().find(|(_, r)| pos.is_some_and(|p| r.contains(p)));
                self.promo = None;
                if let Some((k, _)) = pick {
                    self.play(Mv { from: from as u8, to: to as u8, promo: k }, now, stats);
                }
            } else if let (true, Some(sq)) = (human_turn, click) {
                if let Some(from) = self.sel.filter(|_| targets.contains(&sq)) {
                    let is_promo = legal.iter().any(|m| m.from as usize == from && m.to as usize == sq && m.promo != 0);
                    if is_promo {
                        self.promo = Some((from, sq));
                    } else {
                        self.play(Mv::new(from, sq), now, stats);
                    }
                } else if self.pos.b[sq] * self.pos.side > 0 && self.sel != Some(sq) {
                    self.sel = Some(sq);
                } else {
                    self.sel = None;
                }
            }
        }
        let clickable = human_turn
            && hover.is_some_and(|h| (self.pos.b[h] * self.pos.side > 0 && legal.iter().any(|m| m.from as usize == h)) || targets.contains(&h));
        if clickable || self.promo.is_some() || self.end.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        // --- board ---
        let p = ui.painter().clone();
        p.rect_filled(rect.expand(12.0), 18.0, PANEL);
        let board = p.with_clip_rect(rect);
        let last = self.history.last().map(|(_, m)| *m);
        for sq in 0..64 {
            let (f, r) = (sq % 8, sq / 8);
            let light = (f + r) % 2 == 1;
            let rr = sq_rect(sq);
            board.rect_filled(rr, 0.0, if light { LIGHT_SQ } else { DARK_SQ });
            if last.is_some_and(|m| m.from as usize == sq || m.to as usize == sq) {
                board.rect_filled(rr, 0.0, alpha(AMBER, 0.24));
            }
            if self.sel == Some(sq) {
                board.rect_filled(rr, 0.0, alpha(COLOR, 0.45));
            } else if human_turn && hover == Some(sq) && clickable {
                board.rect_filled(rr, 0.0, alpha(Color32::WHITE, 0.08));
            }
            // Coordinates along the bottom and left edges.
            let (col, row) = if flipped { (7 - f, r) } else { (f, 7 - r) };
            let label_col = alpha(if light { DARK_SQ } else { LIGHT_SQ }, 0.95);
            let font = FontId::proportional((cs * 0.17).clamp(9.0, 13.0));
            if row == 7 {
                board.text(rr.right_bottom() + vec2(-3.0, -2.0), Align2::RIGHT_BOTTOM, ((b'a' + f as u8) as char).to_string(), font.clone(), label_col);
            }
            if col == 0 {
                board.text(rr.left_top() + vec2(3.0, 2.0), Align2::LEFT_TOP, (r + 1).to_string(), font, label_col);
            }
        }
        // King in check glows red.
        if self.pos.in_check() || matches!(self.end, Some((End::Mate(_), _))) {
            let kc = sq_rect(self.pos.king(self.pos.side)).center();
            for (rad, a) in [(0.62, 0.15), (0.48, 0.25), (0.34, 0.35)] {
                board.circle_filled(kc, cs * rad, alpha(RED, a));
            }
        }

        // Pieces; the last moved piece slides in.
        let k = ((now - self.moved_at) / 0.18).clamp(0.0, 1.0) as f32;
        let k = 1.0 - (1.0 - k) * (1.0 - k);
        for sq in 0..64 {
            let pc = self.pos.b[sq];
            if pc == 0 {
                continue;
            }
            let mut c = sq_rect(sq).center();
            if let Some(m) = last.filter(|m| m.to as usize == sq && k < 1.0) {
                c = sq_rect(m.from as usize).center().lerp(c, k);
            }
            draw_piece(&board, c, cs, pc.abs(), pc > 0, 1.0);
        }
        if k < 1.0 {
            ui.ctx().request_repaint();
        }
        // Move hints: dots on empty squares, rings around capturable pieces.
        for &t in &targets {
            let c = sq_rect(t).center();
            let capture = self.pos.b[t] != 0 || (self.sel.is_some_and(|s| self.pos.b[s].abs() == P) && t as i8 == self.pos.ep);
            if capture {
                board.circle_stroke(c, cs * 0.44, Stroke::new(cs * 0.07, alpha(Color32::BLACK, 0.3)));
            } else {
                board.circle_filled(c, cs * 0.14, alpha(Color32::BLACK, if hover == Some(t) { 0.45 } else { 0.28 }));
            }
        }

        // Promotion picker
        if let Some((_, to)) = self.promo {
            p.rect_filled(rect, 0.0, alpha(BG, 0.55));
            for (kind, r) in promo_opts(to) {
                let h = resp.hover_pos().is_some_and(|hp| r.contains(hp));
                p.rect_filled(r.shrink(2.0), cs * 0.15, if h { CARD_HI } else { CARD });
                draw_piece(&p, r.center(), cs * if h { 0.95 } else { 0.85 }, kind, self.pos.side > 0, 1.0);
            }
        }

        if let Some((e, t0)) = self.end {
            let f = ((now - t0 - 0.5) / 0.3).clamp(0.0, 1.0) as f32;
            if f < 1.0 {
                ui.ctx().request_repaint();
            }
            let (head, color) = match e {
                End::Mate(w) if ai && w == human => ("Checkmate — you win!".to_owned(), GREEN),
                End::Mate(_) if ai => ("Checkmate — AI wins".to_owned(), RED),
                End::Mate(w) => (format!("Checkmate — {} wins!", if w == 1 { "White" } else { "Black" }), COLOR),
                End::Stalemate => ("Stalemate".to_owned(), MUTED),
                End::Fifty => ("Draw (50-move rule)".to_owned(), MUTED),
                End::Repetition => ("Draw by repetition".to_owned(), MUTED),
                End::Material => ("Draw (insufficient material)".to_owned(), MUTED),
            };
            if f > 0.0 {
                overlay(ui, rect.expand(12.0), f * 0.9, &head, "Click the board for a new game", color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_fen(fen: &str) -> Pos {
        let mut parts = fen.split_whitespace();
        let mut b = [0i8; 64];
        for (i, row) in parts.next().unwrap().split('/').enumerate() {
            let r = 7 - i;
            let mut f = 0;
            for ch in row.chars() {
                if let Some(d) = ch.to_digit(10) {
                    f += d as usize;
                    continue;
                }
                let k = match ch.to_ascii_lowercase() {
                    'p' => P,
                    'n' => N,
                    'b' => B,
                    'r' => R,
                    'q' => Q,
                    _ => K,
                };
                b[r * 8 + f] = if ch.is_ascii_uppercase() { k } else { -k };
                f += 1;
            }
        }
        let side = if parts.next() == Some("w") { 1 } else { -1 };
        let mut castle = 0;
        for ch in parts.next().unwrap_or("-").chars() {
            castle |= match ch {
                'K' => 1,
                'Q' => 2,
                'k' => 4,
                'q' => 8,
                _ => 0,
            };
        }
        let ep = parts.next().filter(|s| *s != "-").map_or(-1, |s| {
            let s = s.as_bytes();
            ((s[1] - b'1') * 8 + (s[0] - b'a')) as i8
        });
        Pos { b, side, castle, ep, half: 0 }
    }

    fn perft(p: &Pos, d: u32) -> u64 {
        if d == 0 {
            return 1;
        }
        p.legal().iter().map(|&m| perft(&p.make(m), d - 1)).sum()
    }

    #[test]
    fn perft_start() {
        let p = Pos::start();
        assert_eq!(perft(&p, 1), 20);
        assert_eq!(perft(&p, 2), 400);
        assert_eq!(perft(&p, 3), 8902);
    }

    /// "Kiwipete": castling, en passant, promotions and pins all in one position.
    #[test]
    fn perft_kiwipete() {
        let p = from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -");
        assert_eq!(perft(&p, 1), 48);
        assert_eq!(perft(&p, 2), 2039);
        assert_eq!(perft(&p, 3), 97862);
    }

    #[test]
    fn perft_endgame_en_passant_pins() {
        let p = from_fen("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -");
        assert_eq!(perft(&p, 1), 14);
        assert_eq!(perft(&p, 2), 191);
        assert_eq!(perft(&p, 3), 2812);
        assert_eq!(perft(&p, 4), 43238);
    }

    #[test]
    fn perft_promotions() {
        let p = from_fen("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1");
        assert_eq!(perft(&p, 1), 6);
        assert_eq!(perft(&p, 2), 264);
        assert_eq!(perft(&p, 3), 9467);
    }

    #[test]
    fn ai_finds_mate_in_one_at_every_level() {
        // Back-rank mate: Ra8#.
        let p = from_fen("6k1/5ppp/8/8/8/8/8/R5K1 w - -");
        for level in 0..3 {
            for _ in 0..5 {
                let mut rng = Rng::new();
                let m = best_move(&p, level, &mut rng).unwrap();
                if level > 0 {
                    assert_eq!((m.from, m.to), (0, 56), "level {level}");
                }
            }
        }
    }

    #[test]
    fn hard_ai_takes_a_hanging_queen() {
        let p = from_fen("4k3/8/8/3q4/8/8/3R4/4K3 w - -");
        let m = best_move(&p, 2, &mut Rng::new()).unwrap();
        assert_eq!((m.from, m.to), (11, 35));
    }

    #[test]
    fn game_end_detection() {
        let mut c = Chess::new();
        c.mode = 3;
        let mut st = GameStats::default();
        // Fool's mate: 1. f3 e5 2. g4 Qh4#
        for (f, t) in [(13, 21), (52, 36), (14, 30), (59, 31)] {
            c.play(Mv::new(f, t), 0.0, &mut st);
        }
        assert!(matches!(c.end, Some((End::Mate(-1), _))));

        let mut c = Chess::new();
        c.mode = 3;
        // Knights shuffling back and forth: threefold repetition.
        for _ in 0..2 {
            for (f, t) in [(6, 21), (62, 45), (21, 6), (45, 62)] {
                c.play(Mv::new(f, t), 0.0, &mut st);
            }
        }
        assert!(matches!(c.end, Some((End::Repetition, _))));

        assert!(from_fen("8/8/4k3/8/8/2B5/4K3/8 w - -").insufficient());
        assert!(!from_fen("8/8/4k3/8/8/2R5/4K3/8 w - -").insufficient());
    }
}
