//! Short training drills that sit beside Woodpecker sets.
//!
//! Logic here stays free of egui so a later web UI can reuse the same rules.
mod vision;

pub use vision::{
    DrillMode, Highlight, Outcome as VisionOutcome, Round as VisionRound, Session as VisionSession,
    Status as VisionStatus, targets_for,
};

/// Catalog entry shown on the Games hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    pub available: bool,
}

/// Games shipped or reserved for upcoming PRs.
pub const CATALOG: &[GameInfo] = &[
    GameInfo {
        id: "board-scan",
        title: "Board scan",
        blurb: "Tap every hanging or undefended piece before the timer runs out.",
        available: true,
    },
    GameInfo {
        id: "safe-move",
        title: "Safe move",
        blurb: "Play any solid move. The streak ends only when you blunder.",
        available: false,
    },
    GameInfo {
        id: "blind-line",
        title: "Blind line",
        blurb: "Watch a few moves, keep them in your head, then find the next one.",
        available: false,
    },
    GameInfo {
        id: "human-move",
        title: "Human move",
        blurb: "Choose the move a player at your rating would actually make.",
        available: false,
    },
];
