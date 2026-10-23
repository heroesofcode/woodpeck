//! Domain, persistence, and native interface for the woodpeck chess trainer.
// Public domain methods describe errors in prose; callers may intentionally ignore pure results.
#![allow(clippy::missing_errors_doc, clippy::must_use_candidate)]
// Millisecond totals and bounded UI coordinates fit comfortably in their display precision.
#![allow(clippy::cast_precision_loss)]
// Independent persisted switches and timer pause gates are intentionally separate booleans.
#![allow(clippy::struct_excessive_bools)]
// UI geometry is bounded before pixel conversion.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
pub mod sync;
// Color conversion rounds and clamps floating-point channels to the byte range.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub mod theme;
pub mod timer;
