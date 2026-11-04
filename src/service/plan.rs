//! Job steps shared by the desktop worker thread and the browser executor.
use super::{JobOutput, Result, SharedStore, lock};
use crate::{
    puzzle::Puzzle,
    source::{
        CancelFlag, ImportProgress, LibrarySize, STARTER_BYTES, STARTER_VERSION, SourceError,
        import_stream,
    },
};
use std::io::Cursor;

/// The anonymous API takes one theme and one coarse difficulty per request, so batches walk
/// every included theme for each difficulty level overlapping the rating range. The exact
/// range, exclusions and match-all are then enforced locally on every returned puzzle.
pub(super) fn fallback_request(
    params: &crate::sets::BuilderParams,
    batch: u32,
) -> (Option<&str>, &'static str) {
    let k = batch as usize - 1;
    let include = &params.include_themes;
    let levels = crate::source::difficulties(params.rating_min, params.rating_max);
    let themes = include.len().max(1);
    let theme = (!include.is_empty()).then(|| include[k % themes].as_str());
    let level = levels
        .get((k / themes) % levels.len().max(1))
        .copied()
        .unwrap_or_else(|| {
            crate::source::difficulty(u16::midpoint(params.rating_min, params.rating_max))
        });
    (theme, level)
}

/// Idempotent first-run import of the bundled starter puzzles. Batches are saved as they are
/// parsed so no lock is held while decompressing.
pub(super) fn starter(
    store: &SharedStore,
    force: bool,
    cancel: &CancelFlag,
    progress: impl FnMut(ImportProgress),
) -> Result<JobOutput> {
    if !force && lock(store)?.setting("starter_version")?.as_deref() == Some(STARTER_VERSION) {
        return Ok(JobOutput::Imported(ImportProgress::default()));
    }
    let save = |batch: &[_]| {
        lock(store)
            .map_err(|e| SourceError::Invalid(e.to_string()))?
            .upsert_puzzles(batch)?;
        Ok(())
    };
    let result = import_stream(Cursor::new(STARTER_BYTES), cancel, progress, save)?;
    cancel.check()?;
    let store = lock(store)?;
    store.set_setting("starter_version", STARTER_VERSION)?;
    store.record_library_import(LibrarySize::Starter, chrono::Utc::now().timestamp())?;
    Ok(JobOutput::Imported(result))
}
/// Keeps only legal puzzles from an API batch and stores them; returns how many were kept.
pub(super) fn save_valid(store: &SharedStore, puzzles: Vec<Puzzle>) -> Result<usize> {
    let puzzles: Vec<_> = puzzles
        .into_iter()
        .filter(|p| p.validate().is_ok())
        .collect();
    lock(store)?.upsert_puzzles(&puzzles)?;
    Ok(puzzles.len())
}
pub(super) fn save_single(store: &SharedStore, puzzle: Puzzle, daily: bool) -> Result<JobOutput> {
    puzzle
        .validate()
        .map_err(|e| SourceError::Invalid(e.to_string()))?;
    lock(store)?.upsert_puzzles(std::slice::from_ref(&puzzle))?;
    Ok(if daily {
        JobOutput::Daily(puzzle)
    } else {
        JobOutput::Puzzle(puzzle)
    })
}
/// The exact rating range, theme filter and legality enforced on every fallback batch.
pub(super) fn in_band(params: &crate::sets::BuilderParams, puzzles: Vec<Puzzle>) -> Vec<Puzzle> {
    puzzles
        .into_iter()
        .filter(|p| {
            params.rating_in_range(p.rating)
                && params.theme_filter().matches(&p.themes)
                && p.validate().is_ok()
        })
        .collect()
}
