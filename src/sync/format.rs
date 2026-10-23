//! Versioned, deterministic wire types and file encoding; no Git or database dependencies.
use super::{Result, SyncError};
use crate::{
    puzzle::{Outcome, Puzzle},
    stats::AttemptKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// 2: set `focus_theme` became `include_themes`, `exclude_themes` and `theme_match`.
/// 3: set `target_rating`, `variance` and `restrict_recommended` became `rating_min`/`rating_max`.
pub const SCHEMA_VERSION: u32 = 3;
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
/// Rewrites a schema 1 set's single focus theme into the schema 2 theme fields.
/// The legacy clock is reused so an upgraded copy joins cleanly with migrated local data.
fn upgrade_v1_set(set: &mut Set) {
    let legacy = set.fields.remove("focus_theme");
    let updated_at = legacy
        .as_ref()
        .map_or(set.created_at.saturating_mul(1000), |f| f.updated_at);
    let include = match legacy.map(|f| f.value) {
        Some(Value::String(theme)) if !theme.is_empty() => vec![Value::String(theme)],
        _ => Vec::new(),
    };
    for (key, value) in [
        ("include_themes", Value::Array(include)),
        ("exclude_themes", Value::Array(Vec::new())),
        ("theme_match", Value::String("any".into())),
    ] {
        set.fields
            .entry(key.into())
            .or_insert(Field { value, updated_at });
    }
}
/// Rewrites a schema 2 set's `target_rating ± variance` band into the schema 3 inclusive range
/// using [`crate::sets::legacy_range`], exactly like the `SQLite` migration. Both range fields take
/// the later legacy clock. Malformed legacy values are left in place for validation to reject.
fn upgrade_v2_set(set: &mut Set) {
    let number = |key: &str| {
        set.fields
            .get(key)
            .and_then(|f| f.value.as_u64())
            .and_then(|v| u16::try_from(v).ok())
    };
    let (Some(target), Some(variance)) = (number("target_rating"), number("variance")) else {
        return;
    };
    let updated_at = ["target_rating", "variance"]
        .iter()
        .filter_map(|key| set.fields.get(*key).map(|f| f.updated_at))
        .max()
        .unwrap_or(set.created_at.saturating_mul(1000));
    for key in ["target_rating", "variance", "restrict_recommended"] {
        set.fields.remove(key);
    }
    let (min, max) = crate::sets::legacy_range(target, variance);
    for (key, value) in [("rating_min", min), ("rating_max", max)] {
        set.fields.insert(
            key.into(),
            Field {
                value: Value::from(value),
                updated_at,
            },
        );
    }
}
impl State {
    pub fn validate(&self) -> Result<()> {
        if self.manifest.schema_version > SCHEMA_VERSION {
            return Err(SyncError::NewerSchema(self.manifest.schema_version));
        }
        if self.manifest != Manifest::default() {
            return Err(SyncError::Invalid("Unsupported manifest.".into()));
        }
        let known_settings = serde_json::to_value(crate::settings::Settings::default())?;
        if self
            .settings
            .keys()
            .any(|key| known_settings.get(key).is_none())
        {
            return Err(SyncError::Invalid("Unknown preference field.".into()));
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
            let known_fields = serde_json::to_value(&params)?;
            if set.fields.keys().any(|key| known_fields.get(key).is_none()) {
                return Err(SyncError::Invalid("Unknown set field.".into()));
            }
            params
                .validate()
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
        let mut manifest: Manifest = serde_json::from_str(get("manifest.json")?)?;
        if manifest.schema_version > SCHEMA_VERSION {
            return Err(SyncError::NewerSchema(manifest.schema_version));
        }
        let from_version = manifest.schema_version;
        if from_version >= 1 {
            // Older snapshots are upgraded in memory; the next push writes the current schema.
            manifest.schema_version = SCHEMA_VERSION;
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
                let mut set: Set = serde_json::from_str(contents)?;
                if from_version < 2 {
                    upgrade_v1_set(&mut set);
                }
                if from_version < 3 {
                    upgrade_v2_set(&mut set);
                }
                state.sets.insert(uid.into(), set);
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
    /// A set exactly as an older release wrote it: schema 1 has `focus_theme`, schema 2 has
    /// the theme lists; both still carry `target_rating ± variance`.
    fn legacy_set(uid: &str, version: u32) -> Set {
        let mut value = serde_json::json!({
            "name": "Legacy", "target_rating": 1137, "variance": 75, "size": 20,
            "target_cycles": 5, "restrict_recommended": true, "preset_id": "general",
            "shuffle_each_cycle": false,
        });
        let extra = if version == 1 {
            serde_json::json!({"focus_theme": "fork"})
        } else {
            serde_json::json!({"include_themes": ["fork"], "exclude_themes": ["mate"], "theme_match": "any"})
        };
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let clock = |key: &str| match key {
            "focus_theme" => 9,
            "target_rating" => 6,
            "variance" => 8,
            _ => 5,
        };
        Set {
            uid: uid.into(),
            fields: serde_json::from_value::<BTreeMap<String, Value>>(value)
                .unwrap()
                .into_iter()
                .map(|(k, value)| {
                    let updated_at = clock(&k);
                    (k, Field { value, updated_at })
                })
                .collect(),
            created_at: 1,
            seed: 7,
            puzzles: (0..20)
                .map(|i| {
                    let mut p = crate::puzzle::fixture();
                    p.id = format!("p{i:02}");
                    p
                })
                .collect(),
            cycles: BTreeMap::new(),
        }
    }
    fn legacy_files(uid: &str, version: u32) -> BTreeMap<String, String> {
        let mut files = State::default().files().unwrap();
        files.insert(
            "manifest.json".into(),
            format!("{{\"app\":\"woodpeck\",\"schema_version\":{version}}}"),
        );
        files.insert(
            format!("sets/{uid}.json"),
            pretty(&legacy_set(uid, version)).unwrap(),
        );
        files
    }
    #[test]
    fn upgrades_schema_one_and_two_sets_to_rating_ranges() {
        let uid = "b".repeat(32);
        for version in [1, 2] {
            let state = State::from_files(&legacy_files(&uid, version)).unwrap();
            assert_eq!(state.manifest, Manifest::default());
            let upgraded = &state.sets[&uid].fields;
            for gone in [
                "focus_theme",
                "target_rating",
                "variance",
                "restrict_recommended",
            ] {
                assert!(!upgraded.contains_key(gone), "{gone}");
            }
            // 1137 ± 75, the same rule the SQLite migration applies; the later legacy clock wins.
            assert_eq!(upgraded["rating_min"].value, serde_json::json!(1062));
            assert_eq!(upgraded["rating_max"].value, serde_json::json!(1212));
            assert_eq!(upgraded["rating_min"].updated_at, 8);
            assert_eq!(upgraded["rating_max"].updated_at, 8);
            assert_eq!(
                upgraded["include_themes"].value,
                serde_json::json!(["fork"])
            );
            let expected_exclude = if version == 1 {
                serde_json::json!([])
            } else {
                serde_json::json!(["mate"])
            };
            assert_eq!(upgraded["exclude_themes"].value, expected_exclude);
            if version == 1 {
                assert_eq!(upgraded["include_themes"].updated_at, 9);
            }
            // The upgraded snapshot is written back in the current schema and round-trips.
            let rewritten = state.files().unwrap();
            assert!(
                rewritten["manifest.json"]
                    .contains(&format!("\"schema_version\": {SCHEMA_VERSION}"))
            );
            assert_eq!(
                State::from_files(&rewritten).unwrap().files().unwrap(),
                rewritten
            );
            // A current-schema file must not carry legacy fields.
            let mut outdated = rewritten.clone();
            outdated.insert(
                format!("sets/{uid}.json"),
                pretty(&legacy_set(&uid, version)).unwrap(),
            );
            assert!(State::from_files(&outdated).is_err());
        }
    }
    #[test]
    fn upgrade_edge_cases_and_invalid_ranges() {
        let uid = "c".repeat(32);
        // A null schema 1 focus upgrades to an empty include list.
        let mut null_focus = legacy_set(&uid, 1);
        null_focus.fields.get_mut("focus_theme").unwrap().value = Value::Null;
        upgrade_v1_set(&mut null_focus);
        assert_eq!(
            null_focus.fields["include_themes"].value,
            serde_json::json!([])
        );
        // A band clamped at the old floor grows to the minimum width, like the migration.
        let mut edge = legacy_set(&uid, 2);
        edge.fields.get_mut("target_rating").unwrap().value = serde_json::json!(420);
        edge.fields.get_mut("variance").unwrap().value = serde_json::json!(50);
        upgrade_v2_set(&mut edge);
        assert_eq!(edge.fields["rating_min"].value, serde_json::json!(400));
        assert_eq!(edge.fields["rating_max"].value, serde_json::json!(500));
        // Malformed legacy values are not guessed at; validation rejects the snapshot.
        let mut files = legacy_files(&uid, 2);
        let mut bad = legacy_set(&uid, 2);
        bad.fields.get_mut("variance").unwrap().value = serde_json::json!("wide");
        files.insert(format!("sets/{uid}.json"), pretty(&bad).unwrap());
        assert!(State::from_files(&files).is_err());
        // Current-schema ranges are validated: min == max, inverted and out of bounds fail.
        let good = State::from_files(&legacy_files(&uid, 2)).unwrap();
        for (min, max) in [(1200, 1200), (1300, 1200), (300, 1000), (3000, 3300)] {
            let mut state = good.clone();
            let fields = &mut state.sets.get_mut(&uid).unwrap().fields;
            fields.get_mut("rating_min").unwrap().value = serde_json::json!(min);
            fields.get_mut("rating_max").unwrap().value = serde_json::json!(max);
            assert!(state.validate().is_err(), "{min}-{max}");
        }
    }
}
