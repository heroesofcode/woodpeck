//! Review membership is independent of set membership and cycle statistics.
use crate::{
    puzzle::{Outcome, Puzzle},
    stats::{Accuracy, Attempt, AttemptKind},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Cleared,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewItem {
    pub puzzle_id: String,
    pub set_id: i64,
    pub added_at: i64,
    pub last_missed_at: i64,
    pub status: ReviewStatus,
    pub cleared_at: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewChange {
    Unchanged,
    Save(ReviewItem),
}
/// The single transition rule used by both the in-memory queue and SQLite transaction.
pub fn transition(existing: Option<&ReviewItem>, attempt: &Attempt) -> ReviewChange {
    if attempt.kind == AttemptKind::Cycle && attempt.outcome.is_miss() {
        let added_at = existing
            .filter(|i| i.status == ReviewStatus::Pending)
            .map_or(attempt.created_at, |i| i.added_at);
        return ReviewChange::Save(ReviewItem {
            puzzle_id: attempt.puzzle_id.clone(),
            set_id: attempt.set_id,
            added_at,
            last_missed_at: existing.map_or(attempt.created_at, |i| {
                i.last_missed_at.max(attempt.created_at)
            }),
            status: ReviewStatus::Pending,
            cleared_at: None,
        });
    }
    if attempt.kind == AttemptKind::Review
        && attempt.outcome == Outcome::Correct
        && let Some(item) = existing.filter(|i| i.status == ReviewStatus::Pending)
    {
        return ReviewChange::Save(ReviewItem {
            status: ReviewStatus::Cleared,
            cleared_at: Some(attempt.created_at),
            ..item.clone()
        });
    }
    ReviewChange::Unchanged
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewSort {
    MostRecent,
    LowestAccuracy,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewCard {
    pub item: ReviewItem,
    pub puzzle: Puzzle,
    pub set_name: String,
    pub position: u32,
    pub accuracy: Accuracy,
}
impl ReviewCard {
    pub fn theme_chips(&self) -> (&[String], usize) {
        let shown = self.puzzle.themes.len().min(2);
        (
            &self.puzzle.themes[..shown],
            self.puzzle.themes.len() - shown,
        )
    }
}
pub fn filter_and_sort(
    mut cards: Vec<ReviewCard>,
    theme: Option<&str>,
    sort: ReviewSort,
) -> Vec<ReviewCard> {
    cards.retain(|c| {
        c.item.status == ReviewStatus::Pending
            && theme.is_none_or(|t| c.puzzle.themes.iter().any(|p| p == t))
    });
    cards.sort_by(|a, b| {
        let recent = b
            .item
            .last_missed_at
            .cmp(&a.item.last_missed_at)
            .then_with(|| a.item.set_id.cmp(&b.item.set_id))
            .then_with(|| a.item.puzzle_id.cmp(&b.item.puzzle_id));
        match sort {
            ReviewSort::MostRecent => recent,
            ReviewSort::LowestAccuracy => a
                .accuracy
                .percent()
                .total_cmp(&b.accuracy.percent())
                .then(recent),
        }
    });
    cards
}
#[derive(Clone, Debug, Default)]
pub struct ReviewQueue {
    items: BTreeMap<(i64, String), ReviewItem>,
    lifetime: BTreeMap<String, Accuracy>,
}
impl ReviewQueue {
    pub fn record(&mut self, attempt: &Attempt) -> ReviewChange {
        if attempt.kind != AttemptKind::Demo {
            self.lifetime
                .entry(attempt.puzzle_id.clone())
                .or_default()
                .record(attempt.outcome);
        }
        let key = (attempt.set_id, attempt.puzzle_id.clone());
        let change = transition(self.items.get(&key), attempt);
        if let ReviewChange::Save(item) = &change {
            self.items.insert(key, item.clone());
        }
        change
    }
    pub fn get(&self, set_id: i64, puzzle_id: &str) -> Option<&ReviewItem> {
        self.items.get(&(set_id, puzzle_id.to_owned()))
    }
    pub fn pending_count(&self) -> usize {
        self.items
            .values()
            .filter(|i| i.status == ReviewStatus::Pending)
            .count()
    }
    pub fn accuracy(&self, puzzle_id: &str) -> Accuracy {
        self.lifetime.get(puzzle_id).copied().unwrap_or_default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn attempt(kind: AttemptKind, outcome: Outcome, at: i64) -> Attempt {
        Attempt {
            puzzle_id: "p".into(),
            set_id: 1,
            cycle_id: (kind == AttemptKind::Cycle).then_some(1),
            kind,
            outcome,
            time_ms: 10,
            moves_played: vec![],
            created_at: at,
        }
    }
    #[test]
    fn wrong_and_skip_enter_immediately() {
        for outcome in [Outcome::Incorrect, Outcome::Skipped] {
            let mut q = ReviewQueue::default();
            q.record(&attempt(AttemptKind::Cycle, outcome, 1));
            assert_eq!(q.pending_count(), 1);
        }
    }
    #[test]
    fn duplicate_miss_bumps_time_and_retains_first_added() {
        let mut q = ReviewQueue::default();
        q.record(&attempt(AttemptKind::Cycle, Outcome::Incorrect, 1));
        q.record(&attempt(AttemptKind::Cycle, Outcome::Skipped, 5));
        assert_eq!(q.pending_count(), 1);
        let i = q.get(1, "p").unwrap();
        assert_eq!((i.added_at, i.last_missed_at), (1, 5));
    }
    #[test]
    fn only_correct_review_clears_then_later_miss_reenqueues() {
        let mut q = ReviewQueue::default();
        q.record(&attempt(AttemptKind::Cycle, Outcome::Incorrect, 1));
        for (kind, outcome) in [
            (AttemptKind::Review, Outcome::Incorrect),
            (AttemptKind::Review, Outcome::Skipped),
            (AttemptKind::Review, Outcome::Invalid),
            (AttemptKind::Cycle, Outcome::Correct),
            (AttemptKind::Demo, Outcome::Correct),
        ] {
            q.record(&attempt(kind, outcome, 2));
            assert_eq!(q.pending_count(), 1);
        }
        assert_eq!(q.get(1, "p").unwrap().last_missed_at, 1);
        q.record(&attempt(AttemptKind::Review, Outcome::Correct, 3));
        assert_eq!(q.pending_count(), 0);
        assert_eq!(q.get(1, "p").unwrap().cleared_at, Some(3));
        q.record(&attempt(AttemptKind::Cycle, Outcome::Skipped, 4));
        assert_eq!(q.pending_count(), 1);
        assert_eq!(q.get(1, "p").unwrap().cleared_at, None);
        assert_eq!(q.get(1, "p").unwrap().added_at, 4);
        assert_eq!(
            q.accuracy("p"),
            Accuracy {
                correct: 2,
                attempts: 6
            }
        );
    }
    #[test]
    fn backdated_miss_does_not_move_a_pending_item_back_in_time() {
        let mut q = ReviewQueue::default();
        q.record(&attempt(AttemptKind::Cycle, Outcome::Incorrect, 10));
        q.record(&attempt(AttemptKind::Cycle, Outcome::Skipped, 5));
        let item = q.get(1, "p").unwrap();
        assert_eq!((item.added_at, item.last_missed_at), (10, 10));
        assert_eq!(q.pending_count(), 1);
    }
    #[test]
    fn key_is_puzzle_plus_set_and_demo_invalid_do_not_enqueue() {
        let mut q = ReviewQueue::default();
        q.record(&attempt(AttemptKind::Demo, Outcome::Skipped, 1));
        q.record(&attempt(AttemptKind::Cycle, Outcome::Invalid, 1));
        assert_eq!(q.pending_count(), 0);
        let mut a = attempt(AttemptKind::Cycle, Outcome::Incorrect, 2);
        q.record(&a);
        a.set_id = 2;
        q.record(&a);
        assert_eq!(q.pending_count(), 2);
    }
    #[test]
    fn theme_filter_and_both_sorts_and_chips() {
        let p = crate::puzzle::fixture();
        let a = attempt(AttemptKind::Cycle, Outcome::Incorrect, 1);
        let ReviewChange::Save(item) = transition(None, &a) else {
            panic!()
        };
        let mut first = ReviewCard {
            item,
            puzzle: p,
            set_name: "Test".into(),
            position: 9,
            accuracy: Accuracy {
                correct: 0,
                attempts: 1,
            },
        };
        first.puzzle.themes.extend(["mate".into(), "short".into()]);
        assert_eq!(first.theme_chips().1, 2);
        let mut second = first.clone();
        second.item.last_missed_at = 2;
        second.item.puzzle_id = "q".into();
        second.puzzle.themes = vec!["pin".into()];
        second.accuracy = Accuracy {
            correct: 3,
            attempts: 5,
        };
        let cards = vec![first.clone(), second];
        assert_eq!(
            filter_and_sort(cards.clone(), Some("fork"), ReviewSort::MostRecent),
            vec![first.clone()]
        );
        assert_eq!(
            filter_and_sort(cards.clone(), Some("mateIn1"), ReviewSort::MostRecent),
            Vec::new()
        );
        assert_eq!(
            filter_and_sort(cards.clone(), None, ReviewSort::MostRecent)[0]
                .item
                .puzzle_id,
            "q"
        );
        assert_eq!(
            filter_and_sort(cards, None, ReviewSort::LowestAccuracy)[0],
            first
        );
    }
}
