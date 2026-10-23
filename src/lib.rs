//! UI-independent domain and persistence for the woodpeck chess trainer.
pub mod app;
pub mod cli;
pub mod layout;
pub mod motivation;
pub mod puzzle;
pub mod review;
pub mod service;
pub mod sets;
pub mod settings;
pub mod sound;
pub mod source;
pub mod stats;
pub mod store;
// Color conversion rounds and clamps floating-point channels to the byte range.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub mod theme;
pub mod timer;
