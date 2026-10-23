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
