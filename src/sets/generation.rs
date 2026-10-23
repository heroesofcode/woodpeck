use super::{BuilderParams, InvalidParams, ThemeFilter};
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
        themes: ThemeFilter<'_>,
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
}
/// Query exactly the chosen inclusive rating range. Never widen it, return a partial set,
/// or silently relax a theme cap; a shortfall is reported as [`GenerationError::Insufficient`].
pub fn generate(
    query: &impl PuzzleQuery,
    params: &BuilderParams,
    seed: u64,
) -> Result<GeneratedSet, GenerationError> {
    params.validate()?;
    let (minimum, maximum) = params.range();
    let mut distinct_ids = BTreeSet::new();
    let mut candidates: Vec<_> = query
        .query(minimum, maximum, params.theme_filter())?
        .into_iter()
        .filter(|p| {
            params.rating_in_range(p.rating)
                && params.theme_filter().matches(&p.themes)
                && distinct_ids.insert(p.id.clone())
                && p.validate().is_ok()
        })
        .collect();
    // Normalize query ordering before seeded selection (SQLite and fixture sources agree).
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    let mut best = Vec::new();
    for retry in 0..32 {
        let chosen = stratified(&candidates, params, seed.wrapping_add(retry), minimum);
        if chosen.len() > best.len() {
            best = chosen;
        }
        if best.len() == params.size as usize {
            return Ok(GeneratedSet {
                params: params.clone(),
                puzzles: best,
            });
        }
    }
    Err(GenerationError::Insufficient {
        available: best.len(),
        requested: params.size,
    })
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
        fn query(&self, _: u16, _: u16, _: ThemeFilter<'_>) -> Result<Vec<Puzzle>, QueryError> {
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
        let g = generate(&q, &params(), 42).unwrap();
        assert_eq!(g.puzzles.len(), 20);
        assert_eq!(
            g.puzzles
                .iter()
                .map(|p| &p.id)
                .collect::<BTreeSet<_>>()
                .len(),
            20
        );
        let mut counts = BTreeMap::new();
        for p in &g.puzzles {
            assert!((900..=1100).contains(&p.rating));
            for t in &p.themes {
                *counts.entry(t).or_insert(0) += 1;
            }
        }
        assert!(counts.values().all(|n| *n <= 6));
        assert_eq!(g, generate(&q, &params(), 42).unwrap());
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
            include_themes: vec!["fork".into()],
            ..params()
        };
        let g = generate(&q, &p, 1).unwrap();
        assert!(g.puzzles.iter().all(|p| p.themes.contains(&"fork".into())));
        assert_eq!(g.puzzles.len(), 20);
    }
    #[test]
    fn never_widens_past_the_chosen_range() {
        let mut q = pool();
        for p in &mut q.0 {
            p.rating = 1150;
        }
        // Every puzzle is one point outside 900–1100 (and far inside 400–3200).
        assert!(matches!(
            generate(&q, &params(), 4).unwrap_err(),
            GenerationError::Insufficient {
                available: 0,
                requested: 20
            }
        ));
        let p = BuilderParams {
            rating_max: 1150,
            ..params()
        };
        assert!(
            generate(&q, &p, 4)
                .unwrap()
                .puzzles
                .iter()
                .all(|p| p.rating == 1150)
        );
    }
    #[test]
    fn range_bounds_are_inclusive() {
        let mut q = pool();
        for (i, p) in q.0.iter_mut().enumerate() {
            p.rating = [899, 900, 1100, 1101][i % 4];
            p.themes = vec![format!("motif{}", i % 20)];
        }
        let g = generate(
            &q,
            &BuilderParams {
                size: 100,
                ..params()
            },
            3,
        )
        .unwrap();
        let ratings: BTreeSet<_> = g.puzzles.iter().map(|p| p.rating).collect();
        assert_eq!(ratings, BTreeSet::from([900, 1100]));
        // 100 in-range puzzles exist, so one more is a shortfall, never a widened band.
        assert!(matches!(
            generate(
                &q,
                &BuilderParams {
                    size: 101,
                    ..params()
                },
                3
            )
            .unwrap_err(),
            GenerationError::Insufficient {
                available: 100,
                requested: 101
            }
        ));
    }
    #[test]
    fn never_relaxes_cap_and_reports_shortfall() {
        let mut q = pool();
        for p in &mut q.0 {
            p.themes = vec!["fork".into()];
        }
        let error = generate(&q, &params(), 1).unwrap_err();
        assert!(matches!(
            error,
            GenerationError::Insufficient {
                available: 6,
                requested: 20
            }
        ));
    }
    fn tagged(themes: &[&[&str]]) -> Query {
        Query(
            (0..120)
                .map(|i| {
                    let mut p = crate::puzzle::fixture();
                    p.id = format!("{i:03}");
                    p.rating = 950 + (i % 3) * 50;
                    p.themes = themes[i as usize % themes.len()]
                        .iter()
                        .map(|t| (*t).to_string())
                        .collect();
                    p
                })
                .collect(),
        )
    }
    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|t| (*t).to_string()).collect()
    }
    #[test]
    fn multi_theme_any_all_and_exclude_filters() {
        // 30 each: fork, pin, fork+pin, mate.
        let q = tagged(&[&["fork"], &["pin"], &["fork", "pin"], &["mate"]]);
        let with = |include: &[&str], exclude: &[&str], mode, size| BuilderParams {
            include_themes: strings(include),
            exclude_themes: strings(exclude),
            theme_match: mode,
            size,
            preset_id: None,
            ..params()
        };
        let any = generate(
            &q,
            &with(&["fork", "pin"], &[], super::super::ThemeMatch::Any, 90),
            3,
        )
        .unwrap();
        assert!(
            any.puzzles
                .iter()
                .all(|p| p.themes.iter().any(|t| t == "fork" || t == "pin"))
        );
        let all = generate(
            &q,
            &with(&["fork", "pin"], &[], super::super::ThemeMatch::All, 30),
            3,
        )
        .unwrap();
        assert!(
            all.puzzles
                .iter()
                .all(|p| p.themes == strings(&["fork", "pin"]))
        );
        assert!(matches!(
            generate(
                &q,
                &with(&["fork", "pin"], &[], super::super::ThemeMatch::All, 31),
                3
            ),
            Err(GenerationError::Insufficient {
                available: 30,
                requested: 31
            })
        ));
        let excluded = generate(
            &q,
            &with(&[], &["pin"], super::super::ThemeMatch::Any, 60),
            3,
        )
        .unwrap();
        assert!(
            excluded
                .puzzles
                .iter()
                .all(|p| !p.themes.contains(&"pin".into()))
        );
        let both = generate(
            &q,
            &with(&["fork"], &["pin"], super::super::ThemeMatch::Any, 30),
            3,
        )
        .unwrap();
        assert!(both.puzzles.iter().all(|p| p.themes == strings(&["fork"])));
        // Never a partial set: excluding everything reports the shortfall.
        assert!(matches!(
            generate(
                &q,
                &with(
                    &[],
                    &["fork", "mate", "pin"],
                    super::super::ThemeMatch::Any,
                    20
                ),
                3
            ),
            Err(GenerationError::Insufficient {
                available: 0,
                requested: 20
            })
        ));
        // Overlapping include/exclude is rejected up front rather than silently emptied.
        assert!(matches!(
            generate(
                &q,
                &with(&["fork"], &["fork"], super::super::ThemeMatch::Any, 20),
                3
            ),
            Err(GenerationError::InvalidParams(_))
        ));
    }
    #[test]
    fn propagates_query_error() {
        struct Bad;
        impl PuzzleQuery for Bad {
            fn query(&self, _: u16, _: u16, _: ThemeFilter<'_>) -> Result<Vec<Puzzle>, QueryError> {
                Err(QueryError("offline".into()))
            }
        }
        assert!(matches!(
            generate(&Bad, &params(), 1),
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
            generate(&q, &params(), 1),
            Err(GenerationError::Insufficient { available: 0, .. })
        ));
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    struct Local(Vec<Puzzle>);
    impl PuzzleQuery for Local {
        fn query(&self, _: u16, _: u16, _: ThemeFilter<'_>) -> Result<Vec<Puzzle>, QueryError> {
            Ok(self.0.clone())
        }
    }
    #[test]
    fn generated_rows_are_owned_immutable_snapshots() {
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
            include_themes: vec!["fork".into()],
            ..Default::default()
        };
        let generated = generate(&source, &params, 1).unwrap();
        let original = generated.puzzles.clone();
        source.0[0].fen.clear();
        source.0[0].rating = 3000;
        assert_eq!(generated.puzzles, original);
    }
}
