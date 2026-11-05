//! Short training drills that sit beside Woodpecker sets.
//!
//! Logic here stays free of egui so a later web UI can reuse the same rules.
mod blind_line;
mod human_move;
mod safe_move;
mod vision;

pub use blind_line::{
    Phase as BlindPhase, Round as BlindRound, Session as BlindSession, TickEvent as BlindTick,
};
pub use human_move::{Round as HumanRound, Session as HumanSession, Status as HumanStatus};
pub use safe_move::{
    Round as SafeRound, Session as SafeSession, Status as SafeStatus, is_blunder, safe_moves,
};
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
        available: true,
    },
    GameInfo {
        id: "blind-line",
        title: "Blind line",
        blurb: "Watch a few moves, keep them in your head, then find the next one.",
        available: true,
    },
    GameInfo {
        id: "human-move",
        title: "Human move",
        blurb: "Choose the move a player at your rating would actually make.",
        available: true,
    },
];
