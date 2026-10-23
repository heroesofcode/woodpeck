//! Semantic joins. Git's text merge machinery is never used for progress.
use super::{
    Result, SyncError,
    format::{Field, Review, State},
};
use std::collections::BTreeMap;

fn join_fields(left: &mut BTreeMap<String, Field>, right: &BTreeMap<String, Field>) {
    for (key, value) in right {
        left.entry(key.clone())
            .and_modify(|old| {
                if (value.updated_at, value.value.to_string())
                    > (old.updated_at, old.value.to_string())
                {
                    *old = value.clone();
                }
            })
            .or_insert_with(|| value.clone());
    }
}
pub fn review(left: &Review, right: &Review) -> Review {
    Review {
        uid: left.uid.clone().min(right.uid.clone()),
        set_uids: left.set_uids.union(&right.set_uids).cloned().collect(),
        added_at: left.added_at.max(right.added_at),
        resolved_at: left.resolved_at.max(right.resolved_at),
    }
}
/// A commutative, idempotent union. Corrupt immutable identities stop the merge.
pub fn merge(left: &State, right: &State) -> Result<State> {
    left.validate()?;
    right.validate()?;
    let mut result = left.clone();
    join_fields(&mut result.settings, &right.settings);
    for (uid, remote) in &right.sets {
        if let Some(local) = result.sets.get_mut(uid) {
            if local.created_at != remote.created_at
                || local.seed != remote.seed
                || local.puzzles != remote.puzzles
            {
                return Err(SyncError::Invalid(
                    "Set identity has inconsistent frozen data.".into(),
                ));
            }
            join_fields(&mut local.fields, &remote.fields);
            for (id, cycle) in &remote.cycles {
                if let Some(old) = local.cycles.get_mut(id) {
                    if old.number != cycle.number || old.order != cycle.order {
                        return Err(SyncError::Invalid(
                            "Cycle identity has inconsistent order.".into(),
                        ));
                    }
                    old.started_at = old.started_at.min(cycle.started_at);
                    old.completed_at = old.completed_at.max(cycle.completed_at);
                } else {
                    local.cycles.insert(id.clone(), cycle.clone());
                }
            }
        } else {
            result.sets.insert(uid.clone(), remote.clone());
        }
    }
    for (uid, attempts) in &right.attempts {
        let local = result.attempts.entry(uid.clone()).or_default();
        let mut union: BTreeMap<_, _> = local.iter().map(|a| (a.uid.clone(), a.clone())).collect();
        for attempt in attempts {
            if union.get(&attempt.uid).is_some_and(|old| old != attempt) {
                return Err(SyncError::Invalid(
                    "Attempt identity has inconsistent data.".into(),
                ));
            }
            union.insert(attempt.uid.clone(), attempt.clone());
        }
        *local = union.into_values().collect();
    }
    for attempts in result.attempts.values_mut() {
        attempts.sort_by(|a, b| (&a.created_at, &a.uid).cmp(&(&b.created_at, &b.uid)));
    }
    for (id, remote) in &right.review {
        result
            .review
            .entry(id.clone())
            .and_modify(|old| *old = review(old, remote))
            .or_insert_with(|| remote.clone());
    }
    // Concurrent partial cycles can jointly complete a cycle. Duplicate answers remain in
    // the log but count as one position, preserving both the history and cycle statistics.
    for (uid, set) in &mut result.sets {
        let attempts = result.attempts.entry(uid.clone()).or_default();
        for cycle in set.cycles.values_mut() {
            let answered: std::collections::BTreeSet<_> = attempts
                .iter()
                .filter(|a| a.cycle_uid.as_ref() == Some(&cycle.uid))
                .map(|a| &a.puzzle_id)
                .collect();
            if answered.len() == cycle.order.len() {
                cycle.completed_at = cycle.completed_at.max(
                    attempts
                        .iter()
                        .filter(|a| a.cycle_uid.as_ref() == Some(&cycle.uid))
                        .map(|a| a.created_at)
                        .max(),
                );
            }
        }
    }
    result.validate()?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn field(value: i64, updated_at: i64) -> Field {
        Field {
            value: json!(value),
            updated_at,
        }
    }
    #[test]
    fn preferences_are_per_field_with_deterministic_ties() {
        let mut a = State::default();
        a.settings.insert("user_rating".into(), field(1200, 10));
        let mut b = a.clone();
        b.settings.insert("user_rating".into(), field(1300, 10));
        b.settings.insert(
            "auto_advance".into(),
            Field {
                value: json!(true),
                updated_at: 11,
            },
        );
        let merged = merge(&a, &b).unwrap();
        assert_eq!(merged, merge(&b, &a).unwrap());
        assert_eq!(merged, merge(&merged, &merged).unwrap());
        assert_eq!(merged, merge(&merged, &a).unwrap());
        assert_eq!(merged.settings["user_rating"].value, json!(1300));
        assert_eq!(merged.settings["auto_advance"].value, json!(true));
    }
    #[test]
    fn resolution_requires_review_solve_after_latest_miss() {
        let queued = Review {
            uid: "a".repeat(32),
            set_uids: ["b".repeat(32)].into(),
            added_at: 10,
            resolved_at: None,
        };
        for (resolved_at, added_at, resolved) in [
            (20, 10, true),
            (20, 20, false),
            (20, 21, false),
            (5, 10, false),
        ] {
            let other = Review {
                added_at,
                resolved_at: Some(resolved_at),
                ..queued.clone()
            };
            let joined = review(&queued, &other);
            assert_eq!(joined.resolved(), resolved);
            assert_eq!(joined, review(&other, &queued));
            assert_eq!(joined, review(&joined, &joined));
            // A stale pending record cannot undo a later review solve.
            assert_eq!(joined, review(&joined, &queued));
        }
    }
}

#[cfg(test)]
mod progress_tests {
    use super::*;
    use crate::sync::format::{Attempt, Cycle, Set};
    use crate::{puzzle::Outcome, stats::AttemptKind};
    fn state() -> State {
        let uid = "a".repeat(32);
        let mut puzzles = Vec::new();
        for i in 0..20 {
            let mut puzzle = crate::puzzle::fixture();
            puzzle.id = format!("p{i:02}");
            puzzles.push(puzzle);
        }
        let fields = serde_json::to_value(crate::sets::BuilderParams {
            size: 20,
            ..Default::default()
        })
        .unwrap()
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                Field {
                    value: value.clone(),
                    updated_at: 10,
                },
            )
        })
        .collect();
        let cycle = Cycle {
            uid: format!("{uid}-1"),
            number: 1,
            started_at: 1,
            completed_at: None,
            order: puzzles.iter().map(|p| p.id.clone()).collect(),
        };
        let set = Set {
            uid: uid.clone(),
            fields,
            created_at: 1,
            seed: 42,
            puzzles,
            cycles: [(cycle.uid.clone(), cycle)].into(),
        };
        State {
            sets: [(uid.clone(), set)].into(),
            attempts: [(uid, Vec::new())].into(),
            ..State::default()
        }
    }
    fn answer(index: u8, timestamp: i64) -> Attempt {
        Attempt {
            uid: format!("{index:032x}"),
            puzzle_id: format!("p{index:02}"),
            cycle_uid: Some(format!("{}-1", "a".repeat(32))),
            kind: AttemptKind::Cycle,
            outcome: Outcome::Correct,
            time_ms: 1000,
            moves_played: vec![],
            created_at: timestamp,
            local_day: None,
        }
    }
    #[test]
    fn partial_cycles_complete_together_and_merge_is_associative() {
        let uid = "a".repeat(32);
        let mut a = state();
        let mut b = state();
        let mut c = state();
        a.attempts
            .insert(uid.clone(), (0..7).map(|i| answer(i, 2)).collect());
        b.attempts
            .insert(uid.clone(), (7..14).map(|i| answer(i, 3)).collect());
        c.attempts
            .insert(uid.clone(), (14..20).map(|i| answer(i, 4)).collect());
        a.sets
            .get_mut(&uid)
            .unwrap()
            .fields
            .get_mut("name")
            .unwrap()
            .value = serde_json::json!("Renamed");
        a.sets
            .get_mut(&uid)
            .unwrap()
            .fields
            .get_mut("name")
            .unwrap()
            .updated_at = 20;
        b.sets
            .get_mut(&uid)
            .unwrap()
            .fields
            .get_mut("target_cycles")
            .unwrap()
            .value = serde_json::json!(7);
        b.sets
            .get_mut(&uid)
            .unwrap()
            .fields
            .get_mut("target_cycles")
            .unwrap()
            .updated_at = 30;
        let ab = merge(&a, &b).unwrap();
        assert_eq!(ab, merge(&b, &a).unwrap());
        let abc = merge(&ab, &c).unwrap();
        assert_eq!(abc, merge(&a, &merge(&b, &c).unwrap()).unwrap());
        assert_eq!(abc, merge(&abc, &abc).unwrap());
        assert_eq!(abc.attempts[&uid].len(), 20);
        assert_eq!(
            abc.sets[&uid].cycles.values().next().unwrap().completed_at,
            Some(4)
        );
        assert_eq!(abc.sets[&uid].fields["name"].value, "Renamed");
        assert_eq!(abc.sets[&uid].fields["target_cycles"].value, 7);
        assert_eq!(abc, State::from_files(&abc.files().unwrap()).unwrap());
    }
    #[test]
    fn duplicate_attempts_are_idempotent_but_corrupt_uid_collisions_fail() {
        let uid = "a".repeat(32);
        let mut a = state();
        a.attempts.get_mut(&uid).unwrap().push(answer(0, 2));
        let mut b = a.clone();
        assert_eq!(merge(&a, &b).unwrap().attempts[&uid].len(), 1);
        b.attempts.get_mut(&uid).unwrap()[0].outcome = Outcome::Skipped;
        assert!(merge(&a, &b).is_err());
        let mut b = a.clone();
        b.sets
            .get_mut(&uid)
            .unwrap()
            .cycles
            .values_mut()
            .next()
            .unwrap()
            .order
            .reverse();
        assert!(merge(&a, &b).is_err());
        let mut b = a.clone();
        b.sets.get_mut(&uid).unwrap().seed = 100;
        assert!(merge(&a, &b).is_err());
    }
}
