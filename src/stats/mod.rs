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
            stats.time_ms = stats.time_ms.saturating_add(a.time_ms.max(1));
        }
        stats
    }
    pub fn attempted(self) -> u32 {
        self.correct + self.incorrect + self.skipped + self.invalid
    }
    /// Unavailable puzzles do not reduce accuracy. Unattempted positions do until complete.
    pub fn accuracy(self, set_size: u32) -> f64 {
        percent(self.correct, set_size.saturating_sub(self.invalid))
    }
    pub fn average_ms(self, set_size: u32) -> f64 {
        if set_size == 0 {
            0.0
        } else {
            self.time_ms as f64 / f64::from(set_size)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delta {
    pub time_reduction_percent: Option<f64>,
    pub accuracy_points: f64,
}
pub fn compare(current: CycleStats, baseline: CycleStats, size: u32) -> Delta {
    Delta {
        time_reduction_percent: (baseline.time_ms != 0)
            .then(|| 100.0 * (1.0 - current.time_ms as f64 / baseline.time_ms as f64)),
        accuracy_points: current.accuracy(size) - baseline.accuracy(size),
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
        let s = CycleStats::from_attempts(&rows);
        assert_eq!(s.attempted(), 4);
        assert_eq!(s.time_ms, 4000);
        assert!((s.accuracy(4) - 100.0 / 3.0).abs() < 1e-9);
        assert_eq!(s.average_ms(4), 1000.0);
        assert_eq!(
            lifetime(&rows),
            Accuracy {
                correct: 2,
                attempts: 4
            }
        );
    }
    #[test]
    fn deltas_and_zero_denominators() {
        let first = CycleStats {
            correct: 13,
            time_ms: 171_000,
            ..Default::default()
        };
        let last = CycleStats {
            correct: 17,
            time_ms: 114_000,
            ..Default::default()
        };
        let d = compare(last, first, 20);
        assert!((d.time_reduction_percent.unwrap() - 100.0 / 3.0).abs() < 1e-9);
        assert_eq!(d.accuracy_points, 20.0);
        assert_eq!(
            compare(first, CycleStats::default(), 0).time_reduction_percent,
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
