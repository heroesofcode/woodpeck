//! Cycle invariants and statistics shared by training, review, and progress.
mod cycle;
mod progress;
use crate::puzzle::Outcome;
pub use cycle::{CycleError, CycleProgress, cycle_order};
pub use progress::{CycleSummary, MissedPosition, ProgressReport, progress};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptKind {
    Cycle,
    Review,
    Demo,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub puzzle_id: String,
    pub set_id: i64,
    pub cycle_id: Option<i64>,
    pub kind: AttemptKind,
    pub outcome: Outcome,
    pub time_ms: u64,
    pub moves_played: Vec<String>,
    pub created_at: i64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Accuracy {
    pub correct: u32,
    pub attempts: u32,
}
impl Accuracy {
    pub fn record(&mut self, outcome: Outcome) {
        if outcome != Outcome::Invalid {
            self.attempts += 1;
            if outcome == Outcome::Correct {
                self.correct += 1;
            }
        }
    }
    pub fn percent(self) -> f64 {
        percent(self.correct, self.attempts)
    }
}
pub fn percent(numerator: u32, denominator: u32) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        100.0 * f64::from(numerator) / f64::from(denominator)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CycleStats {
    pub correct: u32,
    pub incorrect: u32,
    pub skipped: u32,
    pub invalid: u32,
    /// Total time on answered puzzles, excluding unavailable attempts.
    pub time_ms: u64,
}
impl CycleStats {
    pub fn from_attempts<'a>(attempts: impl IntoIterator<Item = &'a Attempt>) -> Self {
        let mut stats = Self::default();
        for a in attempts {
            if a.kind != AttemptKind::Cycle {
                continue;
            }
            match a.outcome {
                Outcome::Correct => stats.correct += 1,
                Outcome::Incorrect => stats.incorrect += 1,
                Outcome::Skipped => stats.skipped += 1,
                Outcome::Invalid => stats.invalid += 1,
            }
            if a.outcome != Outcome::Invalid {
                stats.time_ms = stats.time_ms.saturating_add(a.time_ms.max(1));
            }
        }
        stats
    }
    pub fn attempted(self) -> u32 {
        self.correct + self.incorrect + self.skipped + self.invalid
    }
    /// Answered puzzles include skips, but exclude unavailable positions.
    pub fn answered(self) -> u32 {
        self.correct + self.incorrect + self.skipped
    }
    /// Accuracy among answered puzzles; skips count as misses.
    pub fn accuracy(self) -> f64 {
        percent(self.correct, self.answered())
    }
    pub fn average_ms(self) -> f64 {
        if self.answered() == 0 {
            0.0
        } else {
            self.time_ms as f64 / f64::from(self.answered())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delta {
    pub average_time_reduction_percent: Option<f64>,
    pub accuracy_points: f64,
}
pub fn compare(current: CycleStats, baseline: CycleStats) -> Delta {
    Delta {
        average_time_reduction_percent: (baseline.average_ms() > 0.0 && current.answered() > 0)
            .then(|| 100.0 * (1.0 - current.average_ms() / baseline.average_ms())),
        accuracy_points: current.accuracy() - baseline.accuracy(),
    }
}
/// Demo attempts never contribute to lifetime performance.
pub fn lifetime<'a>(attempts: impl IntoIterator<Item = &'a Attempt>) -> Accuracy {
    let mut accuracy = Accuracy::default();
    for a in attempts {
        if a.kind != AttemptKind::Demo {
            accuracy.record(a.outcome);
        }
    }
    accuracy
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn attempt(outcome: Outcome) -> Attempt {
        Attempt {
            puzzle_id: "p".into(),
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
    fn skips_miss_invalid_unavailable_and_review_excluded() {
        let mut rows = vec![
            attempt(Outcome::Correct),
            attempt(Outcome::Skipped),
            attempt(Outcome::Incorrect),
            attempt(Outcome::Invalid),
        ];
        let mut review = attempt(Outcome::Correct);
        review.kind = AttemptKind::Review;
        rows.push(review);
        let mut demo = attempt(Outcome::Correct);
        demo.kind = AttemptKind::Demo;
        rows.push(demo);
        rows[1].time_ms = 2000;
        rows[2].time_ms = 3000;
        rows[3].time_ms = 90_000;
        let s = CycleStats::from_attempts(&rows);
        assert_eq!(s.attempted(), 4);
        assert_eq!(s.answered(), 3);
        assert_eq!(s.invalid, 1);
        assert_eq!(s.time_ms, 6000);
        assert!((s.accuracy() - 100.0 / 3.0).abs() < 1e-9);
        assert_eq!(s.average_ms(), 2000.0);
        assert_eq!(
            lifetime(&rows),
            Accuracy {
                correct: 2,
                attempts: 4
            }
        );
    }
    #[test]
    fn skips_count_as_answered_misses() {
        let stats = CycleStats {
            correct: 7,
            incorrect: 2,
            skipped: 1,
            invalid: 5,
            time_ms: 25_000,
        };
        assert_eq!(stats.answered(), 10);
        assert_eq!(stats.accuracy(), 70.0);
        assert_eq!(stats.average_ms(), 2500.0);
    }
    #[test]
    fn unavailable_only_and_empty_cycles_have_zero_performance() {
        for stats in [
            CycleStats::default(),
            CycleStats::from_attempts(&[attempt(Outcome::Invalid)]),
        ] {
            assert_eq!(stats.answered(), 0);
            assert_eq!(stats.time_ms, 0);
            assert_eq!(stats.accuracy(), 0.0);
            assert_eq!(stats.average_ms(), 0.0);
        }
    }
    #[test]
    fn partial_and_completed_cycles_compare_average_time() {
        let completed = CycleStats {
            correct: 135,
            incorrect: 15,
            time_ms: 150_000,
            ..Default::default()
        };
        let mut partial = CycleStats {
            correct: 10,
            time_ms: 10_000,
            ..Default::default()
        };
        let delta = compare(partial, completed);
        assert_eq!(delta.accuracy_points, 10.0);
        assert_eq!(delta.average_time_reduction_percent, Some(0.0));
        partial.time_ms = 5000;
        assert_eq!(
            compare(partial, completed).average_time_reduction_percent,
            Some(50.0)
        );
    }
    #[test]
    fn deltas_and_zero_denominators() {
        let first = CycleStats {
            correct: 13,
            incorrect: 7,
            time_ms: 171_000,
            ..Default::default()
        };
        let last = CycleStats {
            correct: 17,
            skipped: 3,
            time_ms: 114_000,
            ..Default::default()
        };
        let d = compare(last, first);
        assert!((d.average_time_reduction_percent.unwrap() - 100.0 / 3.0).abs() < 1e-9);
        assert_eq!(d.accuracy_points, 20.0);
        assert_eq!(
            compare(first, CycleStats::default()).average_time_reduction_percent,
            None
        );
        assert_eq!(
            compare(CycleStats::default(), first).average_time_reduction_percent,
            None
        );
        assert_eq!(Accuracy::default().percent(), 0.0);
    }
    #[test]
    fn demos_do_not_change_lifetime() {
        let mut a = attempt(Outcome::Correct);
        a.kind = AttemptKind::Demo;
        assert_eq!(lifetime([&a]), Accuracy::default());
    }
}
