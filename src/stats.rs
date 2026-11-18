use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
pub enum Outcome {
    Win,
    Loss,
    Draw,
    /// Score-only game (no win/lose concept).
    Score,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct GameStats {
    pub played: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
    pub best_score: u64,
    pub total_score: u64,
    pub streak: u32,
    pub best_streak: u32,
    pub seconds: f64,
    /// Best (lowest) completion times, keyed by variant label (e.g. difficulty).
    pub best_times: BTreeMap<String, f64>,
    /// Recent scores, newest last (for the sparkline).
    pub history: Vec<u64>,
}

impl GameStats {
    pub fn record(&mut self, outcome: Outcome, score: u64) {
        self.played += 1;
        self.total_score += score;
        self.best_score = self.best_score.max(score);
        self.history.push(score);
        if self.history.len() > 30 {
            self.history.remove(0);
        }
        match outcome {
            Outcome::Win => {
                self.wins += 1;
                self.streak += 1;
                self.best_streak = self.best_streak.max(self.streak);
            }
            Outcome::Loss => {
                self.losses += 1;
                self.streak = 0;
            }
            Outcome::Draw => self.draws += 1,
            Outcome::Score => {}
        }
    }

    /// Records a completion time; returns true if it is a new best.
    pub fn record_time(&mut self, variant: &str, secs: f64) -> bool {
        let best = self.best_times.entry(variant.to_owned()).or_insert(f64::MAX);
        if secs < *best {
            *best = secs;
            true
        } else {
            false
        }
    }

    pub fn win_rate(&self) -> Option<f32> {
        let decided = self.wins + self.losses + self.draws;
        (decided > 0).then(|| self.wins as f32 / decided as f32)
    }

    pub fn avg_score(&self) -> u64 {
        if self.played == 0 { 0 } else { self.total_score / self.played as u64 }
    }
}

pub fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
