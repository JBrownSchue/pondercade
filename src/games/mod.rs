use crate::stats::GameStats;
use crate::widgets::Icon;
use eframe::egui::{Color32, Ui};

pub mod chess;
pub mod g2048;
pub mod memory;
pub mod mines;
pub mod ponderfall;
pub mod snake;
pub mod tictactoe;

pub trait Game {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn icon(&self) -> Icon;
    fn blurb(&self) -> &'static str;
    fn color(&self) -> Color32;
    /// Whether a round is actively in progress (used for play-time tracking).
    fn running(&self) -> bool;
    fn ui(&mut self, ui: &mut Ui, stats: &mut GameStats);
    /// Debug-only: put the game into an interesting mid-play state for screenshots.
    #[cfg(debug_assertions)]
    fn demo(&mut self, _now: f64) {}
}

pub fn all() -> Vec<Box<dyn Game>> {
    vec![
        Box::new(g2048::G2048::new()),
        Box::new(snake::Snake::new()),
        Box::new(mines::Mines::new()),
        Box::new(memory::Memory::new()),
        Box::new(tictactoe::TicTacToe::new()),
        Box::new(ponderfall::Ponderfall::new()),
        Box::new(chess::Chess::new()),
    ]
}
