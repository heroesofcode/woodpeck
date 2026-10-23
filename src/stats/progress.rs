use super::{Accuracy, Attempt, AttemptKind, CycleStats, Delta, compare};
use crate::puzzle::Puzzle;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CycleSummary {
    pub number: u8,
    pub stats: CycleStats,
    pub complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissedPosition {
    pub puzzle_id: String,
    pub position: usize,
    pub misses: u32,
    pub accuracy: Accuracy,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressReport {
    pub puzzles_attempted: u32,
    pub accuracy: Accuracy,
    pub average_ms: f64,
    pub best_cycle: Option<CycleSummary>,
    pub latest_vs_first: Option<Delta>,
    pub by_theme: BTreeMap<String, Accuracy>,
    pub most_missed: Vec<MissedPosition>,
}
/// Progress describes cycle training; Review changes lifetime accuracy independently.
pub fn progress(
    puzzles: &[Puzzle],
    attempts: &[Attempt],
    cycles: &[CycleSummary],
) -> ProgressReport {
    let cycle_attempts: Vec<_> = attempts
        .iter()
        .filter(|a| a.kind == AttemptKind::Cycle)
        .collect();
    let mut accuracy = Accuracy::default();
    let mut time_ms = 0_u64;
    let mut attempted = 0_u32;
    let mut by_puzzle: BTreeMap<&str, (Accuracy, u32)> = BTreeMap::new();
    for a in cycle_attempts {
        accuracy.record(a.outcome);
        attempted += 1;
        time_ms = time_ms.saturating_add(a.time_ms.max(1));
        let entry = by_puzzle.entry(&a.puzzle_id).or_default();
        entry.0.record(a.outcome);
        if a.outcome.is_miss() {
            entry.1 += 1;
        }
    }
    let mut by_theme: BTreeMap<String, Accuracy> = BTreeMap::new();
    let mut most_missed = Vec::new();
    for (index, puzzle) in puzzles.iter().enumerate() {
        let (a, misses) = by_puzzle
            .get(puzzle.id.as_str())
            .copied()
            .unwrap_or_default();
        for theme in puzzle.themes.iter().collect::<BTreeSet<_>>() {
            let performance = by_theme.entry(theme.clone()).or_default();
            performance.correct += a.correct;
            performance.attempts += a.attempts;
        }
        if misses > 0 {
            most_missed.push(MissedPosition {
                puzzle_id: puzzle.id.clone(),
                position: index + 1,
                misses,
                accuracy: a,
            });
        }
    }
    most_missed.sort_by(|a, b| b.misses.cmp(&a.misses).then(a.position.cmp(&b.position)));
    let best_cycle = cycles
        .iter()
        .filter(|c| c.complete)
        .min_by_key(|c| (c.stats.time_ms, c.number))
        .copied();
    let first = cycles.iter().find(|c| c.number == 1 && c.complete);
    let latest = cycles
        .iter()
        .filter(|c| c.complete)
        .max_by_key(|c| c.number);
    let latest_vs_first = first
        .zip(latest)
        .filter(|(a, b)| a.number != b.number)
        .map(|(a, b)| {
            compare(
                b.stats,
                a.stats,
                u32::try_from(puzzles.len()).unwrap_or(u32::MAX),
            )
        });
    ProgressReport {
        puzzles_attempted: attempted,
        accuracy,
        average_ms: if attempted == 0 {
            0.0
        } else {
            time_ms as f64 / f64::from(attempted)
        },
        best_cycle,
        latest_vs_first,
        by_theme,
        most_missed,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzle::{Outcome, fixture};
    fn a(id: &str, outcome: Outcome) -> Attempt {
        Attempt {
            puzzle_id: id.into(),
            set_id: 1,
            cycle_id: Some(1),
            kind: AttemptKind::Cycle,
            outcome,
            time_ms: 1000,
            moves_played: vec![],
            created_at: 1,
        }
    }
    #[test]
    fn progress_filters_review_and_uses_completed_cycles_for_comparison() {
        let p = fixture();
        let mut q = p.clone();
        q.id = "second".into();
        q.themes = vec!["pin".into(), "pin".into()];
        let mut review = a(&p.id, Outcome::Correct);
        review.kind = AttemptKind::Review;
        let attempts = vec![
            a(&p.id, Outcome::Incorrect),
            a(&p.id, Outcome::Skipped),
            a(&q.id, Outcome::Correct),
            review,
        ];
        let cycles = [
            CycleSummary {
                number: 1,
                stats: CycleStats {
                    correct: 1,
                    incorrect: 1,
                    time_ms: 6000,
                    ..Default::default()
                },
                complete: true,
            },
            CycleSummary {
                number: 2,
                stats: CycleStats {
                    correct: 2,
                    time_ms: 3000,
                    ..Default::default()
                },
                complete: true,
            },
            CycleSummary {
                number: 3,
                stats: CycleStats {
                    correct: 1,
                    time_ms: 1000,
                    ..Default::default()
                },
                complete: false,
            },
        ];
        let report = progress(&[p.clone(), q], &attempts, &cycles);
        assert_eq!(report.puzzles_attempted, 3);
        assert_eq!(
            report.accuracy,
            Accuracy {
                correct: 1,
                attempts: 3
            }
        );
        assert_eq!(report.average_ms, 1000.0);
        assert_eq!(report.best_cycle.unwrap().number, 2);
        let delta = report.latest_vs_first.unwrap();
        assert_eq!(delta.time_reduction_percent, Some(50.0));
        assert_eq!(delta.accuracy_points, 50.0);
        assert_eq!(
            report.by_theme["pin"],
            Accuracy {
                correct: 1,
                attempts: 1
            }
        );
        assert_eq!(report.most_missed[0].puzzle_id, p.id);
        assert_eq!(report.most_missed[0].misses, 2);
        assert_eq!(report.most_missed[0].position, 1);
    }
    #[test]
    fn empty_progress_has_no_comparison_or_best() {
        let report = progress(&[], &[], &[]);
        assert_eq!(report.average_ms, 0.0);
        assert_eq!(report.best_cycle, None);
        assert_eq!(report.latest_vs_first, None);
        assert_eq!(report.by_theme, BTreeMap::new());
    }
}
