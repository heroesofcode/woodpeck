use super::{BuilderParams, InvalidParams};
use crate::{puzzle::Puzzle, stats::cycle_order};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
#[derive(Debug, Error)]
#[error("Puzzle query failed: {0}")]
pub struct QueryError(pub String);
pub trait PuzzleQuery {
    fn query(
        &self,
        minimum: u16,
        maximum: u16,
        theme: Option<&str>,
    ) -> Result<Vec<Puzzle>, QueryError>;
}
#[derive(Debug, Error)]
pub enum GenerationError {
    #[error(transparent)]
    InvalidParams(#[from] InvalidParams),
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error(
        "Only {available} suitable puzzles are available; {requested} requested. Import more puzzles or change the constraints."
    )]
    Insufficient { available: usize, requested: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedSet {
    pub params: BuilderParams,
    pub puzzles: Vec<Puzzle>,
    pub original_band: (u16, u16),
    pub used_band: (u16, u16),
    pub widening_steps: u16,
}
impl GeneratedSet {
    pub fn widening_note(&self) -> Option<String> {
        (self.widening_steps > 0).then(|| {
            format!(
                "Rating band widened from {}–{} to {}–{} to find enough puzzles.",
                self.original_band.0, self.original_band.1, self.used_band.0, self.used_band.1
            )
        })
    }
}
/// Query each expanded band; never return a partial set or silently relax a theme cap.
pub fn generate(
    query: &impl PuzzleQuery,
    params: &BuilderParams,
    user_rating: u16,
    seed: u64,
) -> Result<GeneratedSet, GenerationError> {
    params.validate(user_rating)?;
    let original_band = params.band();
    let mut used_band = original_band;
    let mut steps = 0;
    loop {
        let mut distinct_ids = BTreeSet::new();
        let mut candidates: Vec<_> = query
            .query(used_band.0, used_band.1, params.focus_theme.as_deref())?
            .into_iter()
            .filter(|p| {
                (used_band.0..=used_band.1).contains(&p.rating)
                    && params
                        .focus_theme
                        .as_ref()
                        .is_none_or(|t| p.themes.contains(t))
                    && distinct_ids.insert(p.id.clone())
                    && p.validate().is_ok()
            })
            .collect();
        // Normalize query ordering before seeded selection (SQLite and fixture sources agree).
        candidates.sort_by(|a, b| a.id.cmp(&b.id));
        let mut best = Vec::new();
        for retry in 0..32 {
            let chosen = stratified(&candidates, params, seed.wrapping_add(retry), used_band.0);
            if chosen.len() > best.len() {
                best = chosen;
            }
            if best.len() == params.size as usize {
                return Ok(GeneratedSet {
                    params: params.clone(),
                    puzzles: best,
                    original_band,
                    used_band,
                    widening_steps: steps,
                });
            }
        }
        if used_band == (400, 3000) {
            return Err(GenerationError::Insufficient {
                available: best.len(),
                requested: params.size,
            });
        }
        used_band = (
            used_band.0.saturating_sub(50).max(400),
            used_band.1.saturating_add(50).min(3000),
        );
        steps += 1;
    }
}
fn stratified(
    candidates: &[Puzzle],
    params: &BuilderParams,
    seed: u64,
    minimum: u16,
) -> Vec<Puzzle> {
    let mut buckets: BTreeMap<u16, Vec<&Puzzle>> = BTreeMap::new();
    for p in candidates {
        buckets
            .entry((p.rating - minimum) / 100)
            .or_default()
            .push(p);
    }
    let mut buckets: Vec<_> = buckets
        .into_values()
        .enumerate()
        .map(|(i, b)| cycle_order(&b, true, seed.wrapping_add(i as u64), 1).into_iter())
        .collect();
    let mut selected = Vec::new();
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    let cap = params.size * 3 / 10;
    loop {
        let mut advanced = false;
        for bucket in &mut buckets {
            for p in bucket.by_ref() {
                advanced = true;
                let themes: BTreeSet<_> = p.themes.iter().map(String::as_str).collect();
                if params.is_general()
                    && themes
                        .iter()
                        .any(|t| counts.get(t).copied().unwrap_or(0) >= cap)
                {
                    continue;
                }
                for t in themes {
                    *counts.entry(t).or_default() += 1;
                }
                selected.push(p.clone());
                break;
            }
            if selected.len() == params.size as usize {
                return selected;
            }
        }
        if !advanced {
            return selected;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Query(Vec<Puzzle>);
    impl PuzzleQuery for Query {
        fn query(&self, _: u16, _: u16, _: Option<&str>) -> Result<Vec<Puzzle>, QueryError> {
            Ok(self.0.clone())
        }
    }
    fn pool() -> Query {
        Query(
            (0..200)
                .map(|i| {
                    let mut p = crate::puzzle::fixture();
                    p.id = format!("{i:03}");
                    p.rating = 900 + (i % 4) * 50;
                    p.themes = vec![format!("motif{}", i % 5)];
                    p
                })
                .collect(),
        )
    }
    fn params() -> BuilderParams {
        BuilderParams {
            size: 20,
            ..Default::default()
        }
    }
    #[test]
    fn exact_distinct_band_stratification_cap_and_stability() {
        let q = pool();
        let g = generate(&q, &params(), 1200, 42).unwrap();
        assert_eq!(g.puzzles.len(), 20);
        assert_eq!(
            g.puzzles
                .iter()
                .map(|p| &p.id)
                .collect::<BTreeSet<_>>()
                .len(),
            20
        );
        assert_eq!(g.widening_steps, 0);
        let mut counts = BTreeMap::new();
        for p in &g.puzzles {
            assert!((900..=1100).contains(&p.rating));
            for t in &p.themes {
                *counts.entry(t).or_insert(0) += 1;
            }
        }
        assert!(counts.values().all(|n| *n <= 6));
        assert_eq!(g, generate(&q, &params(), 1200, 42).unwrap());
        let lower = g.puzzles.iter().filter(|p| p.rating < 1000).count();
        assert_eq!(lower, 10);
    }
    #[test]
    fn filters_theme_and_duplicate_ids_even_for_bad_query() {
        let mut q = pool();
        for p in &mut q.0 {
            if p.rating < 1000 {
                p.themes = vec!["fork".into()];
            }
        }
        q.0.extend(q.0.clone());
        let p = BuilderParams {
            focus_theme: Some("fork".into()),
            ..params()
        };
        let g = generate(&q, &p, 1200, 1).unwrap();
        assert!(g.puzzles.iter().all(|p| p.themes.contains(&"fork".into())));
        assert_eq!(g.puzzles.len(), 20);
    }
    #[test]
    fn widens_in_fifty_point_steps_and_reports() {
        let mut q = pool();
        for p in &mut q.0 {
            p.rating = 1150;
        }
        let g = generate(&q, &params(), 1200, 4).unwrap();
        assert_eq!(g.used_band, (850, 1150));
        assert_eq!(g.widening_steps, 1);
        assert!(g.widening_note().unwrap().contains("widened"));
    }
    #[test]
    fn never_relaxes_cap_and_reports_shortfall() {
        let mut q = pool();
        for p in &mut q.0 {
            p.themes = vec!["fork".into()];
        }
        let error = generate(&q, &params(), 1200, 1).unwrap_err();
        assert!(matches!(
            error,
            GenerationError::Insufficient {
                available: 6,
                requested: 20
            }
        ));
    }
    #[test]
    fn propagates_query_error() {
        struct Bad;
        impl PuzzleQuery for Bad {
            fn query(&self, _: u16, _: u16, _: Option<&str>) -> Result<Vec<Puzzle>, QueryError> {
                Err(QueryError("offline".into()))
            }
        }
        assert!(matches!(
            generate(&Bad, &params(), 1200, 1),
            Err(GenerationError::Query(_))
        ));
    }
    #[test]
    fn rejects_invalid_puzzles() {
        let mut q = pool();
        for p in &mut q.0 {
            p.fen.clear();
        }
        assert!(matches!(
            generate(&q, &params(), 1200, 1),
            Err(GenerationError::Insufficient { available: 0, .. })
        ));
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    struct Local(Vec<Puzzle>);
    impl PuzzleQuery for Local {
        fn query(&self, _: u16, _: u16, _: Option<&str>) -> Result<Vec<Puzzle>, QueryError> {
            Ok(self.0.clone())
        }
    }
    #[test]
    fn generated_rows_are_owned_immutable_snapshots_and_do_not_widen_unnecessarily() {
        let mut source = Local(
            (0..20)
                .map(|i| {
                    let mut p = crate::puzzle::fixture();
                    p.id = format!("{i}");
                    p.themes = vec!["fork".into()];
                    p
                })
                .collect(),
        );
        let params = BuilderParams {
            size: 20,
            focus_theme: Some("fork".into()),
            ..Default::default()
        };
        let generated = generate(&source, &params, 1200, 1).unwrap();
        let original = generated.puzzles.clone();
        source.0[0].fen.clear();
        source.0[0].rating = 3000;
        assert_eq!(generated.puzzles, original);
        assert_eq!(generated.widening_note(), None);
    }
}
