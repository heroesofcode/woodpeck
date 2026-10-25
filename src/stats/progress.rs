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
    let stats = CycleStats::from_attempts(attempts);
    let cycle_attempts: Vec<_> = attempts
        .iter()
        .filter(|a| a.kind == AttemptKind::Cycle)
        .collect();
    let mut accuracy = Accuracy::default();
    let mut by_puzzle: BTreeMap<&str, (Accuracy, u32)> = BTreeMap::new();
    for a in cycle_attempts {
        accuracy.record(a.outcome);
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
    let latest = cycles.iter().max_by_key(|c| c.number);
    let latest_vs_first = first
        .zip(latest)
        .filter(|(a, b)| a.number != b.number && b.stats.answered() > 0)
        .map(|(a, b)| compare(b.stats, a.stats));
    ProgressReport {
        puzzles_attempted: stats.attempted(),
        accuracy,
        average_ms: stats.average_ms(),
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
    fn progress_excludes_review_and_invalid_time_and_compares_latest_cycle() {
        let p = fixture();
        let mut q = p.clone();
        q.id = "second".into();
        q.themes = vec!["pin".into(), "pin".into()];
        let mut review = a(&p.id, Outcome::Correct);
        review.kind = AttemptKind::Review;
        let mut invalid = a(&p.id, Outcome::Invalid);
        invalid.time_ms = 90_000;
        let attempts = vec![
            a(&p.id, Outcome::Incorrect),
            a(&p.id, Outcome::Skipped),
            a(&q.id, Outcome::Correct),
            review,
            invalid,
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
        let puzzles = [p.clone(), q];
        let report = progress(&puzzles, &attempts, &cycles);
        assert_eq!(report.puzzles_attempted, 4);
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
        assert!((delta.average_time_reduction_percent.unwrap() - 200.0 / 3.0).abs() < 1e-9);
        assert_eq!(delta.accuracy_points, 50.0);
        let completed_report = progress(&puzzles, &attempts, &cycles[..2]);
        assert_eq!(
            completed_report
                .latest_vs_first
                .unwrap()
                .average_time_reduction_percent,
            Some(50.0)
        );
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
    fn unanswered_latest_cycle_has_no_comparison() {
        let cycles = [
            CycleSummary {
                number: 1,
                stats: CycleStats {
                    correct: 2,
                    time_ms: 2000,
                    ..Default::default()
                },
                complete: true,
            },
            CycleSummary {
                number: 2,
                stats: CycleStats {
                    invalid: 1,
                    ..Default::default()
                },
                complete: false,
            },
        ];
        assert_eq!(progress(&[], &[], &cycles).latest_vs_first, None);
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
