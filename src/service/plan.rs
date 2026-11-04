//! Job planning shared by the desktop worker thread and the browser executor.

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
