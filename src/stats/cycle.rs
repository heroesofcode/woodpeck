use crate::puzzle::Outcome;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CycleError {
    #[error("Cycle must contain distinct puzzles and have a number and target from 1 to 10.")]
    InvalidConfiguration,
    #[error("This puzzle is absent or was already attempted in this cycle.")]
    InvalidAttempt,
    #[error("Finish the current cycle first.")]
    Incomplete,
    #[error("The target cycle count has been reached.")]
    TargetReached,
    #[error("At most ten cycles are allowed.")]
    MaximumReached,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleProgress {
    pub number: u8,
    pub target: u8,
    order: Vec<String>,
    outcomes: BTreeMap<String, Outcome>,
}
impl CycleProgress {
    pub fn new(number: u8, target: u8, order: Vec<String>) -> Result<Self, CycleError> {
        if !(1..=10).contains(&target)
            || number == 0
            || number > target
            || order.is_empty()
            || order.iter().collect::<BTreeSet<_>>().len() != order.len()
        {
            return Err(CycleError::InvalidConfiguration);
        }
        Ok(Self {
            number,
            target,
            order,
            outcomes: BTreeMap::new(),
        })
    }
    pub fn order(&self) -> &[String] {
        &self.order
    }
    pub fn next_puzzle(&self) -> Option<&str> {
        self.order
            .iter()
            .find(|id| !self.outcomes.contains_key(*id))
            .map(String::as_str)
    }
    pub fn record(&mut self, id: &str, outcome: Outcome) -> Result<(), CycleError> {
        if !self.order.iter().any(|p| p == id) || self.outcomes.contains_key(id) {
            return Err(CycleError::InvalidAttempt);
        }
        self.outcomes.insert(id.to_owned(), outcome);
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.outcomes.len() == self.order.len()
    }
    pub fn set_completed(&self) -> bool {
        self.complete() && self.number >= self.target
    }
    pub fn extend(&mut self) -> Result<(), CycleError> {
        if !self.complete() {
            return Err(CycleError::Incomplete);
        }
        if self.target >= 10 {
            return Err(CycleError::MaximumReached);
        }
        self.target += 1;
        Ok(())
    }
    pub fn next_cycle(&self) -> Result<Self, CycleError> {
        if !self.complete() {
            return Err(CycleError::Incomplete);
        }
        if self.number == 10 {
            return Err(CycleError::MaximumReached);
        }
        if self.number >= self.target {
            return Err(CycleError::TargetReached);
        }
        Self::new(self.number + 1, self.target, self.order.clone())
    }
}
/// Version-independent deterministic Fisher-Yates shuffle (`SplitMix64`).
pub fn cycle_order<T: Clone>(fixed: &[T], shuffle: bool, seed: u64, cycle: u8) -> Vec<T> {
    let mut result = fixed.to_vec();
    if !shuffle {
        return result;
    }
    let mut state = seed.wrapping_add(u64::from(cycle));
    for i in (1..result.len()).rev() {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        let j = usize::try_from(z % u64::try_from(i + 1).unwrap_or(u64::MAX)).unwrap_or(0);
        result.swap(i, j);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_same_puzzles_extend_to_ten() {
        let mut c = CycleProgress::new(1, 1, vec!["a".into(), "b".into()]).unwrap();
        assert_eq!(c.next_cycle(), Err(CycleError::Incomplete));
        c.record("a", Outcome::Skipped).unwrap();
        let json = serde_json::to_string(&c).unwrap();
        let mut c: CycleProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(c.next_puzzle(), Some("b"));
        assert!(c.record("a", Outcome::Correct).is_err());
        c.record("b", Outcome::Invalid).unwrap();
        assert!(c.set_completed());
        assert_eq!(c.next_cycle(), Err(CycleError::TargetReached));
        for n in 2..=10 {
            c.extend().unwrap();
            let next = c.next_cycle().unwrap();
            assert_eq!(next.number, n);
            assert_eq!(next.order(), c.order());
            c = next;
            c.record("a", Outcome::Correct).unwrap();
            c.record("b", Outcome::Correct).unwrap();
        }
        assert_eq!(c.extend(), Err(CycleError::MaximumReached));
        assert_eq!(c.next_cycle(), Err(CycleError::MaximumReached));
    }
    #[test]
    fn validates_configuration() {
        for (n, t, ids) in [
            (0, 1, vec!["a".into()]),
            (1, 11, vec!["a".into()]),
            (2, 1, vec!["a".into()]),
            (1, 1, vec![]),
            (1, 1, vec!["a".into(), "a".into()]),
        ] {
            assert!(CycleProgress::new(n, t, ids).is_err());
        }
    }
    #[test]
    fn shuffle_stable_preserves_membership() {
        let ids: Vec<_> = (0..100).collect();
        assert_eq!(cycle_order(&ids, false, 1, 1), ids);
        let order = cycle_order(&ids, true, 42, 1);
        assert_eq!(order, cycle_order(&ids, true, 42, 1));
        assert_ne!(order, cycle_order(&ids, true, 42, 2));
        let mut sorted = order;
        sorted.sort_unstable();
        assert_eq!(sorted, ids);
    }
}
