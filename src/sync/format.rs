//! Versioned, deterministic wire types and file encoding; no Git or database dependencies.
use super::{Result, SyncError};
use crate::{
    puzzle::{Outcome, Puzzle},
    stats::AttemptKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: u32 = 1;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub app: String,
    pub schema_version: u32,
}
impl Default for Manifest {
    fn default() -> Self {
        Self {
            app: "woodpeck".into(),
            schema_version: SCHEMA_VERSION,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub value: Value,
    /// Unix milliseconds. Equal clocks are resolved using canonical JSON value order.
    pub updated_at: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cycle {
    pub uid: String,
    pub number: u8,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub order: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Set {
    pub uid: String,
    pub fields: BTreeMap<String, Field>,
    pub created_at: i64,
    pub seed: u64,
    /// Ordered frozen membership, including all data required for offline training.
    pub puzzles: Vec<Puzzle>,
    pub cycles: BTreeMap<String, Cycle>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub uid: String,
    pub puzzle_id: String,
    pub cycle_uid: Option<String>,
    pub kind: AttemptKind,
    pub outcome: Outcome,
    pub time_ms: u64,
    pub moves_played: Vec<String>,
    pub created_at: i64,
    /// Original local calendar day, used to rebuild XP and streaks across time zones.
    pub local_day: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub uid: String,
    pub set_uids: BTreeSet<String>,
    /// Latest miss, including misses made while the item was already queued.
    pub added_at: i64,
    /// Retained tombstone, even after a later miss requeues the item.
    pub resolved_at: Option<i64>,
}
impl Review {
    pub fn resolved(&self) -> bool {
        self.resolved_at.is_some_and(|at| at > self.added_at)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub manifest: Manifest,
    pub settings: BTreeMap<String, Field>,
    pub sets: BTreeMap<String, Set>,
    /// Set UID to immutable attempt log, sorted by (`created_at`, uid).
    pub attempts: BTreeMap<String, Vec<Attempt>>,
    /// Puzzle ID to queue membership, shared across its sets.
    pub review: BTreeMap<String, Review>,
}
pub fn pretty(value: &impl Serialize) -> Result<String> {
    // Value's object representation is a BTreeMap, recursively sorting every JSON key.
    Ok(serde_json::to_string_pretty(&serde_json::to_value(value)?)? + "\n")
}
pub fn valid_uid(uid: &str) -> bool {
    uid.len() == 32
        && uid
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
impl State {
    pub fn validate(&self) -> Result<()> {
        if self.manifest.schema_version > SCHEMA_VERSION {
            return Err(SyncError::NewerSchema(self.manifest.schema_version));
        }
        if self.manifest != Manifest::default() {
            return Err(SyncError::Invalid("Unsupported manifest.".into()));
        }
        for (uid, set) in &self.sets {
            if !valid_uid(uid) || uid != &set.uid {
                return Err(SyncError::Invalid("Invalid set identity.".into()));
            }
            let params: crate::sets::BuilderParams = serde_json::from_value(Value::Object(
                set.fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.value.clone()))
                    .collect(),
            ))?;
            // Recommendation bounds are machine preferences; validate absolute bounds instead.
            let mut absolute = params.clone();
            absolute.restrict_recommended = false;
            absolute
                .validate(1200)
                .map_err(|e| SyncError::Invalid(e.to_string()))?;
            let ids: BTreeSet<_> = set.puzzles.iter().map(|p| p.id.clone()).collect();
            if ids.len() != set.puzzles.len() || ids.len() != params.size as usize {
                return Err(SyncError::Invalid("Invalid set membership.".into()));
            }
            for (cycle_uid, cycle) in &set.cycles {
                if cycle_uid != &cycle.uid
                    || cycle.uid != format!("{uid}-{}", cycle.number)
                    || !(1..=10).contains(&cycle.number)
                    || cycle.order.iter().cloned().collect::<BTreeSet<_>>() != ids
                    || cycle.order.len() != ids.len()
                {
                    return Err(SyncError::Invalid("Invalid cycle.".into()));
                }
            }
        }
        let mut attempt_uids = BTreeSet::new();
        for (set_uid, attempts) in &self.attempts {
            let set = self
                .sets
                .get(set_uid)
                .ok_or_else(|| SyncError::Invalid("Attempt set is missing.".into()))?;
            for attempt in attempts {
                if !valid_uid(&attempt.uid)
                    || !attempt_uids.insert(&attempt.uid)
                    || attempt.time_ms == 0
                    || attempt.time_ms > i64::MAX as u64
                    || !set.puzzles.iter().any(|p| p.id == attempt.puzzle_id)
                    || (attempt.kind == AttemptKind::Cycle) != attempt.cycle_uid.is_some()
                    || attempt
                        .cycle_uid
                        .as_ref()
                        .is_some_and(|uid| !set.cycles.contains_key(uid))
                {
                    return Err(SyncError::Invalid("Invalid attempt.".into()));
                }
            }
        }
        for (id, review) in &self.review {
            if !valid_uid(&review.uid)
                || review.set_uids.is_empty()
                || review.set_uids.iter().any(|uid| {
                    self.sets
                        .get(uid)
                        .is_none_or(|s| !s.puzzles.iter().any(|p| &p.id == id))
                })
            {
                return Err(SyncError::Invalid("Invalid review membership.".into()));
            }
        }
        let settings: crate::settings::Settings = serde_json::from_value(Value::Object(
            self.settings
                .iter()
                .map(|(k, v)| (k.clone(), v.value.clone()))
                .collect(),
        ))?;
        if !settings.validate() {
            return Err(SyncError::Invalid("Invalid preferences.".into()));
        }
        Ok(())
    }
    pub fn files(&self) -> Result<BTreeMap<String, String>> {
        self.validate()?;
        let mut files = BTreeMap::from([
            ("manifest.json".into(), pretty(&self.manifest)?),
            ("settings.json".into(), pretty(&self.settings)?),
            ("review.json".into(), pretty(&self.review)?),
        ]);
        for (uid, set) in &self.sets {
            files.insert(format!("sets/{uid}.json"), pretty(set)?);
            let mut attempts = self.attempts.get(uid).cloned().unwrap_or_default();
            attempts.sort_by(|a, b| (&a.created_at, &a.uid).cmp(&(&b.created_at, &b.uid)));
            files.insert(format!("attempts/{uid}.json"), pretty(&attempts)?);
        }
        Ok(files)
    }
    pub fn from_files(files: &BTreeMap<String, String>) -> Result<Self> {
        let get = |path: &str| {
            files
                .get(path)
                .ok_or_else(|| SyncError::Invalid(format!("Missing {path}.")))
        };
        let manifest: Manifest = serde_json::from_str(get("manifest.json")?)?;
        if manifest.schema_version > SCHEMA_VERSION {
            return Err(SyncError::NewerSchema(manifest.schema_version));
        }
        let mut state = Self {
            manifest,
            settings: serde_json::from_str(get("settings.json")?)?,
            review: serde_json::from_str(get("review.json")?)?,
            ..Self::default()
        };
        for (path, contents) in files {
            if let Some(uid) = path
                .strip_prefix("sets/")
                .and_then(|p| p.strip_suffix(".json"))
            {
                state
                    .sets
                    .insert(uid.into(), serde_json::from_str(contents)?);
            } else if let Some(uid) = path
                .strip_prefix("attempts/")
                .and_then(|p| p.strip_suffix(".json"))
            {
                state
                    .attempts
                    .insert(uid.into(), serde_json::from_str(contents)?);
            }
        }
        state.validate()?;
        Ok(state)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_roundtrip_and_future_schema() {
        let state = State::default();
        let files = state.files().unwrap();
        assert!(files.values().all(|s| s.ends_with('\n')));
        assert_eq!(State::from_files(&files).unwrap(), state);
        assert_eq!(
            pretty(&serde_json::json!({"z":{"b":1,"a":2},"a":0})).unwrap(),
            "{\n  \"a\": 0,\n  \"z\": {\n    \"a\": 2,\n    \"b\": 1\n  }\n}\n"
        );
        let mut files = files;
        files.insert(
            "manifest.json".into(),
            "{\"schema_version\":99,\"app\":\"woodpeck\"}".into(),
        );
        assert!(matches!(
            State::from_files(&files),
            Err(SyncError::NewerSchema(99))
        ));
    }
    #[test]
    fn identities_cannot_escape_directory() {
        for bad in ["../a", "", "ABCDEF0123456789abcdef0123456789", "a/b"] {
            assert!(!valid_uid(bad));
        }
        assert!(valid_uid("0123456789abcdef0123456789abcdef"));
    }
}
