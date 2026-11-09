//! Book import and conservative, legal-move-only reading models, separate from puzzle libraries.
pub mod document;
pub mod eval;
pub mod import;
pub mod model;
pub mod notation;
pub mod resolve;

pub mod cnn;
#[cfg(not(target_arch = "wasm32"))]
pub mod cover;
pub mod coverage;
pub mod diagram;
pub mod game_sides;

pub(crate) mod reflow;
