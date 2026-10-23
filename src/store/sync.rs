//! Transactional bridge from portable UIDs to machine-local SQLite row IDs.
use super::{Result, Store, StoreError};
use crate::sync::{
    format::{Attempt, Cycle, Field, Review, Set, State},
    merge,
};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use std::collections::BTreeMap;

impl Store {
    pub fn sync_generation(&self) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT generation FROM sync_changes WHERE id=1",
            [],
            |r| r.get(0),
        )?)
    }
    /// A consistent snapshot. Local configuration and the puzzle library are excluded.
    pub fn export_sync(&self) -> Result<State> {
        self.export_sync_snapshot().map(|(state, _)| state)
    }
    pub fn export_sync_snapshot(&self) -> Result<(State, i64)> {
        let tx = self.connection.unchecked_transaction()?;
        let state = self.export_sync_rows()?;
        let generation = self.sync_generation()?;
        tx.commit()?;
        Ok((state, generation))
    }
    fn export_sync_rows(&self) -> Result<State> {
        let mut state = State {
            settings: self.export_preferences()?,
            ..State::default()
        };
        let mut ids = BTreeMap::new();
        for stored in self.sets()? {
            let (uid, name_at, target_at): (String, i64, i64) = self.connection.query_row(
                "SELECT uid,name_updated_at,target_cycles_updated_at FROM sets WHERE id=?1",
                [stored.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let values: BTreeMap<String, Value> =
                serde_json::from_value(serde_json::to_value(&stored.params)?)?;
            let fields = values
                .into_iter()
                .map(|(key, value)| -> Result<_> {
                    let updated_at = match key.as_str() {
                        "name" => name_at,
                        "target_cycles" => target_at,
                        _ => self
                            .connection
                            .query_row(
                                "SELECT updated_at FROM sync_fields WHERE key=?1",
                                [format!("set/{uid}/{key}")],
                                |r| r.get(0),
                            )
                            .optional()?
                            .unwrap_or(stored.created_at.saturating_mul(1000)),
                    };
                    Ok((key, Field { value, updated_at }))
                })
                .collect::<Result<_>>()?;
            let mut cycles = BTreeMap::new();
            let mut cycle_ids = BTreeMap::new();
            for cycle in self.cycles(stored.id)? {
                let uid: String = self.connection.query_row(
                    "SELECT uid FROM cycles WHERE id=?1",
                    [cycle.id],
                    |r| r.get(0),
                )?;
                cycle_ids.insert(cycle.id, uid.clone());
                cycles.insert(
                    uid.clone(),
                    Cycle {
                        uid,
                        number: cycle.number,
                        started_at: cycle.started_at,
                        completed_at: cycle.completed_at,
                        order: cycle.order,
                    },
                );
            }
            let mut stmt = self.connection.prepare(
                "SELECT uid,local_day FROM attempts WHERE set_id=?1 ORDER BY created_at,uid",
            )?;
            let identities = stmt
                .query_map([stored.id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let attempts = self
                .attempts(stored.id)?
                .into_iter()
                .zip(identities)
                .map(|(a, (uid, local_day))| Attempt {
                    uid,
                    puzzle_id: a.puzzle_id,
                    cycle_uid: a.cycle_id.map(|id| cycle_ids[&id].clone()),
                    kind: a.kind,
                    outcome: a.outcome,
                    time_ms: a.time_ms,
                    moves_played: a.moves_played,
                    created_at: a.created_at,
                    local_day,
                })
                .collect();
            state.attempts.insert(uid.clone(), attempts);
            state.sets.insert(
                uid.clone(),
                Set {
                    uid: uid.clone(),
                    fields,
                    created_at: stored.created_at,
                    seed: stored.seed,
                    puzzles: self.set_puzzles(stored.id)?,
                    cycles,
                },
            );
            ids.insert(stored.id, uid);
        }
        self.export_review(&mut state, &ids)?;
        Ok(state)
    }
    fn export_preferences(&self) -> Result<BTreeMap<String, Field>> {
        let mut settings = BTreeMap::new();
        if let Some(json) = self.setting("preferences")? {
            let values: BTreeMap<String, Value> = serde_json::from_str(&json)?;
            for (key, value) in values {
                let updated_at = self
                    .connection
                    .query_row(
                        "SELECT updated_at FROM sync_fields WHERE key=?1",
                        [&key],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or(0);
                settings.insert(key, Field { value, updated_at });
            }
        }
        Ok(settings)
    }
    fn export_review(&self, state: &mut State, ids: &BTreeMap<i64, String>) -> Result<()> {
        let mut stmt = self.connection.prepare(
            "SELECT puzzle_id,set_id,substr(uid,1,32),last_missed_at,resolved_at FROM review_items",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })?;
        for row in rows {
            let (id, set_id, uid, added_at, resolved_at) = row?;
            let item = Review {
                uid,
                set_uids: [ids[&set_id].clone()].into(),
                added_at,
                resolved_at,
            };
            state
                .review
                .entry(id)
                .and_modify(|old| *old = merge::review(old, &item))
                .or_insert(item);
        }
        Ok(())
    }
    /// Re-merge inside the write transaction so training performed during fetch is retained.
    pub fn import_sync(&mut self, remote: &State) -> crate::sync::Result<State> {
        remote.validate()?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(StoreError::from)?;
        let merged = merge::merge(&self.export_sync_rows()?, remote)?;
        self.import_sync_rows(&merged)?;
        tx.commit().map_err(StoreError::from)?;
        Ok(merged)
    }
    fn import_sync_rows(&self, state: &State) -> Result<()> {
        let mut set_ids = BTreeMap::new();
        let mut cycle_ids = BTreeMap::new();
        for (uid, set) in &state.sets {
            let p: crate::sets::BuilderParams = serde_json::from_value(Value::Object(
                set.fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.value.clone()))
                    .collect(),
            ))?;
            self.connection.execute("INSERT INTO sets(uid,name,focus_theme,target_rating,variance,size,target_cycles,created_at,preset_id,order_mode,restrict_recommended,seed) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) ON CONFLICT(uid) DO UPDATE SET name=excluded.name,focus_theme=excluded.focus_theme,target_rating=excluded.target_rating,variance=excluded.variance,size=excluded.size,target_cycles=excluded.target_cycles,preset_id=excluded.preset_id,order_mode=excluded.order_mode,restrict_recommended=excluded.restrict_recommended",params![uid,p.name,p.focus_theme,p.target_rating,p.variance,p.size,p.target_cycles,set.created_at,p.preset_id,if p.shuffle_each_cycle {"shuffle"} else {"fixed"},p.restrict_recommended,set.seed.to_string()])?;
            let id: i64 =
                self.connection
                    .query_row("SELECT id FROM sets WHERE uid=?1", [uid], |r| r.get(0))?;
            self.connection.execute(
                "UPDATE sets SET name_updated_at=?1,target_cycles_updated_at=?2 WHERE id=?3",
                params![
                    set.fields["name"].updated_at,
                    set.fields["target_cycles"].updated_at,
                    id
                ],
            )?;
            for (index, puzzle) in set.puzzles.iter().enumerate() {
                self.connection.execute("INSERT INTO set_puzzles(set_id,position,puzzle_id,puzzle_json) VALUES(?1,?2,?3,?4) ON CONFLICT(set_id,position) DO NOTHING",params![id,i64::try_from(index+1).map_err(|_|StoreError::InvalidData("Set too large.".into()))?,puzzle.id,serde_json::to_string(puzzle)?])?;
            }
            for (key, field) in &set.fields {
                self.connection.execute("INSERT INTO sync_fields(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",params![format!("set/{uid}/{key}"),field.value.to_string(),field.updated_at])?;
            }
            // Completed cycles first, preserving the one-active-cycle constraint.
            let mut cycles: Vec<_> = set.cycles.values().collect();
            cycles.sort_by_key(|c| (c.completed_at.is_none(), c.number));
            for cycle in cycles {
                self.connection.execute("INSERT INTO cycles(uid,set_id,number,started_at,completed_at,order_json) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(uid) DO UPDATE SET started_at=excluded.started_at,completed_at=excluded.completed_at",params![cycle.uid,id,cycle.number,cycle.started_at,cycle.completed_at,serde_json::to_string(&cycle.order)?])?;
                let cycle_id: i64 = self.connection.query_row(
                    "SELECT id FROM cycles WHERE uid=?1",
                    [&cycle.uid],
                    |r| r.get(0),
                )?;
                cycle_ids.insert(cycle.uid.clone(), cycle_id);
            }
            set_ids.insert(uid.clone(), id);
        }
        for (uid, attempts) in &state.attempts {
            for a in attempts {
                self.connection.execute("INSERT INTO attempts(uid,puzzle_id,set_id,cycle_id,kind,outcome,time_ms,moves_played,created_at,local_day) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(uid) DO NOTHING",params![a.uid,a.puzzle_id,set_ids[uid],a.cycle_uid.as_ref().map(|uid|cycle_ids[uid]),enum_name(&a.kind)?,enum_name(&a.outcome)?,i64::try_from(a.time_ms).map_err(|_|StoreError::InvalidData("Attempt time too large.".into()))?,serde_json::to_string(&a.moves_played)?,a.created_at,a.local_day])?;
            }
        }
        for (puzzle_id, item) in &state.review {
            for uid in &item.set_uids {
                // Review identity is canonical per puzzle/set; the wire UID is shared by
                // all origins of this puzzle, while SQLite retains one row per set.
                let review_uid = format!("{}-{uid}", item.uid);
                self.connection.execute("INSERT INTO review_items(uid,puzzle_id,set_id,added_at,last_missed_at,status,cleared_at,resolved_at) VALUES(?1,?2,?3,?4,?4,?5,?6,?7) ON CONFLICT(puzzle_id,set_id) DO UPDATE SET uid=excluded.uid,added_at=excluded.added_at,last_missed_at=excluded.last_missed_at,status=excluded.status,cleared_at=excluded.cleared_at,resolved_at=excluded.resolved_at",params![review_uid,puzzle_id,set_ids[uid],item.added_at,if item.resolved(){"cleared"}else{"pending"},item.resolved_at.filter(|_|item.resolved()),item.resolved_at])?;
            }
        }
        if !state.settings.is_empty() {
            let mut values = serde_json::to_value(self.settings()?)?;
            for (key, field) in &state.settings {
                values[key] = field.value.clone();
                self.connection.execute("INSERT INTO sync_fields(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",params![key,field.value.to_string(),field.updated_at])?;
            }
            self.set_setting("preferences", &serde_json::to_string(&values)?)?;
        }
        self.rebuild_motivation(state)?;
        Ok(())
    }
    fn rebuild_motivation(&self, state: &State) -> Result<()> {
        let mut activity = crate::motivation::Motivation::default();
        let mut attempts: Vec<_> = state
            .attempts
            .values()
            .flatten()
            .filter(|a| a.kind != crate::stats::AttemptKind::Demo)
            .collect();
        attempts.sort_by_key(|a| {
            (
                a.local_day
                    .unwrap_or(a.created_at.div_euclid(86400) + 719_163),
                a.created_at,
                &a.uid,
            )
        });
        for a in attempts {
            activity.record(
                a.local_day
                    .unwrap_or(a.created_at.div_euclid(86400) + 719_163),
                a.outcome,
                false,
            );
        }
        activity.xp = activity.xp.saturating_add(
            u64::try_from(
                state
                    .sets
                    .values()
                    .flat_map(|s| s.cycles.values())
                    .filter(|c| c.completed_at.is_some())
                    .count(),
            )
            .unwrap_or(u64::MAX)
            .saturating_mul(50),
        );
        self.set_setting("motivation", &serde_json::to_string(&activity)?)
    }
}
fn enum_name(value: &impl serde::Serialize) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| StoreError::InvalidData("Invalid enum value.".into()))
}
