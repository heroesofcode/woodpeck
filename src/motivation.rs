//! Local calendar days are injected; XP and streak changes commit with each attempt.
use crate::puzzle::Outcome;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Motivation {
    pub xp: u64,
    pub current_streak: u32,
    pub best_streak: u32,
    pub last_day: Option<i64>,
    pub recent_days: Vec<i64>,
}
impl Motivation {
    pub fn record(&mut self, day: i64, outcome: Outcome, cycle_complete: bool) {
        self.xp = self
            .xp
            .saturating_add(if outcome == Outcome::Correct { 10 } else { 0 })
            .saturating_add(if cycle_complete { 50 } else { 0 });
        if self.last_day.is_none_or(|last| day > last) {
            self.current_streak = if self.last_day == day.checked_sub(1) {
                self.current_streak.saturating_add(1)
            } else {
                1
            };
            self.best_streak = self.best_streak.max(self.current_streak);
            self.last_day = Some(day);
            self.recent_days.push(day);
            self.recent_days.retain(|d| *d >= day.saturating_sub(6));
        }
    }
    pub fn streak_on(&self, day: i64) -> u32 {
        if self
            .last_day
            .is_some_and(|last| last >= day.saturating_sub(1))
        {
            self.current_streak
        } else {
            0
        }
    }
    /// Level L starts at 100 × (L − 1)² XP, evaluated with integer arithmetic.
    pub fn level(&self) -> u64 {
        (self.xp / 100).isqrt() + 1
    }
    /// Fraction of the current level earned, including saturated lifetime XP.
    pub fn level_progress(&self) -> f32 {
        let level = u128::from(self.level());
        let lower = 100 * (level - 1).pow(2);
        let upper = 100 * level.pow(2);
        (u128::from(self.xp) - lower) as f32 / (upper - lower) as f32
    }
    pub fn rank(&self) -> &'static str {
        match self.level() {
            1..=4 => "Pawn",
            5..=9 => "Knight",
            10..=19 => "Bishop",
            20..=34 => "Rook",
            35..=49 => "Queen",
            _ => "Grandmaster",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn days_streak_reset_backdated_attempts_xp_levels_and_ranks() {
        let mut m = Motivation::default();
        m.record(10, Outcome::Incorrect, false);
        m.record(10, Outcome::Correct, false);
        assert_eq!(m.current_streak, 1);
        m.record(11, Outcome::Skipped, false);
        assert_eq!((m.current_streak, m.best_streak), (2, 2));
        assert_eq!(m.streak_on(13), 0);
        m.record(14, Outcome::Correct, true);
        m.record(9, Outcome::Correct, false);
        assert_eq!(
            (m.current_streak, m.best_streak, m.last_day, m.xp),
            (1, 2, Some(14), 80)
        );
        for (xp, level, rank) in [
            (0, 1, "Pawn"),
            (1600, 5, "Knight"),
            (8100, 10, "Bishop"),
            (36100, 20, "Rook"),
            (115_600, 35, "Queen"),
            (240_100, 50, "Grandmaster"),
        ] {
            m.xp = xp;
            assert_eq!(m.level(), level);
            assert_eq!(m.rank(), rank);
        }
    }
    #[test]
    fn level_progress_handles_thresholds_and_saturated_xp() {
        for (xp, expected) in [(0, 0.0), (50, 0.5), (100, 0.0), (250, 0.5), (400, 0.0)] {
            let m = Motivation {
                xp,
                ..Default::default()
            };
            assert!((m.level_progress() - expected).abs() < f32::EPSILON);
        }
        let mut m = Motivation {
            xp: u64::MAX,
            ..Default::default()
        };
        m.record(1, Outcome::Correct, true);
        assert_eq!(m.xp, u64::MAX);
        assert!(m.level_progress().is_finite());
        assert!((0.0..=1.0).contains(&m.level_progress()));
    }
}
