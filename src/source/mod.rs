//! Source formats and quality filtering. Network/import job orchestration is added separately.
mod api;
mod client;
mod csv;
mod import;
mod pgn;
pub use api::{parse_batch, parse_puzzle};
pub use client::*;
pub use csv::{CsvPuzzles, ImportProgress, passes_quality};
pub use import::*;
pub use pgn::export_pgn;
use thiserror::Error;
pub const ATTRIBUTION: &str = "Puzzles from the Lichess puzzle database (CC0)";
#[derive(Debug, Error)]
pub enum SourceError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Csv(#[from] ::csv::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Operation cancelled.")]
    Cancelled,
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Invalid puzzle data: {0}")]
    Invalid(String),
}
