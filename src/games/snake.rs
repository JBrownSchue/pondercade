use crate::stats::{GameStats, Outcome};
use crate::theme::*;
use crate::util::Rng;
use crate::widgets::*;
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use std::collections::VecDeque;

const W: i32 = 24;
const H: i32 = 16;
const COLOR: Color32 = GREEN;
const SPEEDS: [&str; 3] = ["Relaxed", "Normal", "Turbo"];

#[derive(PartialEq, Clone, Copy)]
enum State {
    Ready,
    Running,
    Paused,
    Over,
}

pub struct Snake {
    body: VecDeque<(i32, i32)>,
    dir: (i32, i32),
    queue: VecDeque<(i32, i32)>,
    food: (i32, i32),
    bonus: Option<((i32, i32), f64)>,
    acc: f64,
    last: Option<f64>,
    state: State,
    score: u64,
    eaten: u32,
    speed: usize,
    rng: Rng,
    died_at: f64,
}

impl Snake {
    pub fn new() -> Self {
        let mut s = Self {
            body: VecDeque::new(),
            dir: (1, 0),
            queue: VecDeque::new(),
            food: (0, 0),
            bonus: None,
            acc: 0.0,
            last: None,
            state: State::Ready,
            score: 0,
            eaten: 0,
            speed: 1,
            rng: Rng::new(),
            died_at: 0.0,
        };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.body = (0..4).map(|i| (6 - i, H / 2)).collect();
        self.dir = (1, 0);
        self.queue.clear();
        self.bonus = None;
        self.acc = 0.0;
        self.state = State::Ready;
        self.score = 0;
        self.eaten = 0;
        self.food = self.free_cell();
    }

    fn free_cell(&mut self) -> (i32, i32) {
        loop {
            let c = (self.rng.below(W as usize) as i32, self.rng.below(H as usize) as i32);
            if !self.body.contains(&c) && self.bonus.is_none_or(|(b, _)| b != c) && c != self.food {
                return c;
            }
        }
    }

    fn interval(&self) -> f64 {
        let base = [0.15, 0.11, 0.075][self.speed];
        (base * 0.985f64.powi(self.eaten as i32)).max(0.045)
    }

    fn step(&mut self, now: f64) {
        if let Some(d) = self.queue.pop_front() {
            self.dir = d;
        }
        let (hx, hy) = self.body[0];
        let head = (hx + self.dir.0, hy + self.dir.1);
        let grows = head == self.food || self.bonus.is_some_and(|(b, _)| b == head);
        let hit_self = self.body.iter().take(self.body.len() - usize::from(!grows)).any(|&s| s == head);
        if head.0 < 0 || head.1 < 0 || head.0 >= W || head.1 >= H || hit_self {
            self.state = State::Over;
            self.died_at = now;
            return;
        }
        self.body.push_front(head);
        let mult = self.speed as u64 + 1;
        if head == self.food {
            self.eaten += 1;
            self.score += 10 * mult;
            self.food = self.free_cell();
            if self.eaten % 5 == 0 && self.bonus.is_none() {
                let c = self.free_cell();
                self.bonus = Some((c, now + 6.0));
            }
        } else if self.bonus.is_some_and(|(b, _)| b == head) {
            self.score += 50 * mult;
            self.bonus = None;
        } else {
            self.body.pop_back();
        }
    }

    fn handle_input(&mut self, ui: &Ui) {
        let (turns, space, pause) = ui.input(|i| {
            let mut t = Vec::new();
            for e in &i.events {
                if let egui::Event::Key { key, pressed: true, repeat: false, .. } = e {
                    let d = match key {
                        Key::ArrowUp | Key::W => (0, -1),
                        Key::ArrowDown | Key::S => (0, 1),
                        Key::ArrowLeft | Key::A => (-1, 0),
                        Key::ArrowRight | Key::D => (1, 0),
                        _ => continue,
                    };
                    t.push(d);
                }
            }
            (t, i.key_pressed(Key::Space) || i.key_pressed(Key::Enter), i.key_pressed(Key::P) || i.key_pressed(Key::Escape))
        });
        match self.state {
            State::Ready if space || !turns.is_empty() => self.state = State::Running,
            State::Running if pause || space => self.state = State::Paused,
            State::Paused if pause || space || !turns.is_empty() => self.state = State::Running,
            State::Over if space => self.reset(),
            _ => {}
        }
        if self.state == State::Running {
            for d in turns {
                let last = *self.queue.back().unwrap_or(&self.dir);
                if d != last && d != (-last.0, -last.1) && self.queue.len() < 3 {
                    self.queue.push_back(d);
                }
            }
        }
    }
}

impl super::Game for Snake {
    fn id(&self) -> &'static str {
        "snake"
    }
    fn name(&self) -> &'static str {
        "Snake"
    }
    fn icon(&self) -> Icon {
        Icon::Snake
    }
    fn blurb(&self) -> &'static str {
        "Eat, grow, don't bite yourself. Golden gems give bonus points."
    }
    fn color(&self) -> Color32 {
        COLOR
    }
    fn running(&self) -> bool {
        self.state == State::Running
    }

    #[cfg(debug_assertions)]
    fn demo(&mut self, now: f64) {
        for _ in 0..6 {
            self.body.push_back(*self.body.back().unwrap());
        }
        self.bonus = Some(((14, 4), now + 5.0));
        self.state = State::Running;
    }

    fn ui(&mut self, ui: &mut Ui, stats: &mut GameStats) {
        let now = ui.input(|i| i.time);
        let dt = self.last.map_or(0.0, |l| (now - l).min(0.1));
        self.last = Some(now);

        header(ui, self.icon(), "Snake", "Arrows / WASD to steer · Space to pause", COLOR, |ui| {
            if ui.add(Pill::primary("New game", COLOR)).clicked() {
                self.reset();
            }
            chip(ui, "Best", stats.best_score.max(self.score), AMBER);
            chip(ui, "Score", self.score, TEXT);
            chip(ui, "Length", self.body.len(), COLOR);
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Speed").color(MUTED));
            let mut sp = self.speed;
            if segmented(ui, &mut sp, &SPEEDS) {
                self.speed = sp;
                self.reset();
            }
        });

        let was_over = self.state == State::Over;
        self.handle_input(ui);
        if self.state == State::Running {
            if self.bonus.is_some_and(|(_, exp)| now > exp) {
                self.bonus = None;
            }
            self.acc += dt;
            while self.acc >= self.interval() && self.state == State::Running {
                self.acc -= self.interval();
                self.step(now);
            }
            ui.ctx().request_repaint();
        }
        if !was_over && self.state == State::Over {
            stats.record(Outcome::Score, self.score);
        }

        let (rect, _) = board_rect(ui, W as f32 / H as f32, 960.0, Sense::hover());
        let p = ui.painter().clone();
        let cs = rect.width() / W as f32;
        let cell = |x: i32, y: i32| Rect::from_min_size(pos2(rect.left() + x as f32 * cs, rect.top() + y as f32 * cs), vec2(cs, cs));
        let center = |c: (i32, i32)| cell(c.0, c.1).center();

        p.rect_filled(rect.expand(8.0), 18.0, PANEL);
        for y in 0..H {
            for x in 0..W {
                if (x + y) % 2 == 0 {
                    p.rect_filled(cell(x, y), 0.0, alpha(CARD, 0.45));
                }
            }
        }

        // Food (pulsing)
        let pulse = ((now * 5.0).sin() * 0.5 + 0.5) as f32;
        let fc = center(self.food);
        p.circle_filled(fc, cs * (0.55 + 0.12 * pulse), alpha(RED, 0.18));
        p.circle_filled(fc, cs * 0.34, RED);
        p.circle_filled(fc + vec2(-cs * 0.1, -cs * 0.1), cs * 0.09, alpha(Color32::WHITE, 0.6));
        // Bonus gem
        if let Some((b, exp)) = self.bonus {
            let left = ((exp - now) / 6.0).clamp(0.0, 1.0) as f32;
            let c = center(b);
            let r = cs * (0.38 + 0.06 * pulse);
            p.circle_filled(c, cs * 0.7, alpha(AMBER, 0.15 * left + 0.05));
            let pts = vec![c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)];
            p.add(Shape::convex_polygon(pts, alpha(AMBER, 0.4 + 0.6 * left), Stroke::NONE));
            if self.state == State::Running {
                ui.ctx().request_repaint();
            }
        }

        // Snake body: connectors first, then segments.
        let n = self.body.len();
        let col = |i: usize| lerp(Color32::from_rgb(110, 240, 190), Color32::from_rgb(20, 120, 140), i as f32 / n.max(2) as f32);
        let thick = cs * 0.78;
        for i in 0..n.saturating_sub(1) {
            let (a, b) = (center(self.body[i]), center(self.body[i + 1]));
            let r = Rect::from_two_pos(a, b).expand(thick / 2.0);
            p.rect_filled(r, thick * 0.3, col(i));
        }
        for (i, &s) in self.body.iter().enumerate() {
            p.rect_filled(Rect::from_center_size(center(s), vec2(thick, thick)), thick * 0.35, col(i));
        }
        // Head with eyes
        let hc = center(self.body[0]);
        let dead = self.state == State::Over;
        if dead {
            let f = (1.0 - ((now - self.died_at) / 0.6)).clamp(0.0, 1.0) as f32;
            p.rect_filled(Rect::from_center_size(hc, vec2(thick, thick)), thick * 0.35, lerp(col(0), RED, 0.4 + 0.6 * f));
        }
        let d = vec2(self.dir.0 as f32, self.dir.1 as f32);
        let perp = vec2(-d.y, d.x);
        for side in [-1.0, 1.0] {
            let e: Pos2 = hc + d * cs * 0.14 + perp * side * cs * 0.18;
            p.circle_filled(e, cs * 0.11, Color32::WHITE);
            p.circle_filled(e + d * cs * 0.03, cs * 0.055, BG);
        }

        let (head, sub, color) = match self.state {
            State::Ready => ("Ready?", "Press Space or an arrow key to start".to_owned(), COLOR),
            State::Paused => ("Paused", "Press Space to resume".to_owned(), CYAN),
            State::Over => ("Game over", format!("Score {}  ·  length {}  ·  Space to retry", self.score, n), RED),
            State::Running => ("", String::new(), COLOR),
        };
        let t = ui.ctx().animate_bool_with_time(ui.scope_id().with("snake_ov"), self.state != State::Running, 0.25);
        if t > 0.0 && self.state != State::Running {
            overlay(ui, rect.expand(8.0), t, head, &sub, color);
        }
    }
}
