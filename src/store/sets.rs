use super::{Result, Store, StoreError};
use crate::{
    puzzle::Puzzle,
    sets::{BuilderParams, ThemeMatch},
    stats::{AttemptKind, CycleError, CycleProgress, cycle_order},
};
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSet {
    pub id: i64,
    pub params: BuilderParams,
    pub created_at: i64,
    pub seed: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CycleRecord {
    pub id: i64,
    pub set_id: i64,
    pub number: u8,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub order: Vec<String>,
}
impl Store {
    pub fn create_set(
        &mut self,
        params: &BuilderParams,
        puzzles: &[Puzzle],
        created_at: i64,
        seed: u64,
    ) -> Result<i64> {
        params
            .validate()
            .map_err(|e| StoreError::InvalidData(e.to_string()))?;
        if puzzles.len() != params.size as usize
            || puzzles.iter().map(|p| &p.id).collect::<BTreeSet<_>>().len() != puzzles.len()
        {
            return Err(StoreError::InvalidData(
                "A set must contain exactly its stated number of distinct puzzles.".into(),
            ));
        }
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO sets(name,include_themes,exclude_themes,theme_match,rating_min,rating_max,size,target_cycles,created_at,preset_id,order_mode,seed) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![params.name,serde_json::to_string(&params.include_themes)?,serde_json::to_string(&params.exclude_themes)?,theme_match_name(params.theme_match),params.rating_min,params.rating_max,params.size,params.target_cycles,created_at,params.preset_id,if params.shuffle_each_cycle {"shuffle"} else {"fixed"},seed.to_string()])?;
        let id = tx.last_insert_rowid();
        for (i, p) in puzzles.iter().enumerate() {
            tx.execute("INSERT INTO set_puzzles(set_id,position,puzzle_id,puzzle_json) VALUES(?1,?2,?3,?4)",params![id,i64::try_from(i+1).map_err(|_|StoreError::InvalidData("Set is too large.".into()))?,p.id,serde_json::to_string(p)?])?;
        }
        tx.commit()?;
        Ok(id)
    }
    pub fn set(&self, id: i64) -> Result<Option<StoredSet>> {
        Ok(self.connection.query_row("SELECT id,name,include_themes,rating_min,rating_max,size,target_cycles,created_at,preset_id,order_mode,seed,exclude_themes,theme_match FROM sets WHERE id=?1",[id],read_set).optional()?)
    }
    pub fn sets(&self) -> Result<Vec<StoredSet>> {
        let mut stmt=self.connection.prepare("SELECT id,name,include_themes,rating_min,rating_max,size,target_cycles,created_at,preset_id,order_mode,seed,exclude_themes,theme_match FROM sets ORDER BY created_at,id")?;
        Ok(stmt
            .query_map([], read_set)?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn delete_set(&mut self, id: i64) -> Result<()> {
        let tx = self.connection.transaction()?;
        tx.execute("DELETE FROM attempts WHERE set_id=?1", [id])?;
        tx.execute("DELETE FROM review_items WHERE set_id=?1", [id])?;
        tx.execute("DELETE FROM cycles WHERE set_id=?1", [id])?;
        if tx.execute("DELETE FROM sets WHERE id=?1", [id])? == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit()?;
        Ok(())
    }
    pub fn rename_set(&self, id: i64, name: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidData("A set needs a name.".into()));
        }
        if self
            .connection
            .execute("UPDATE sets SET name=?1 WHERE id=?2", params![name, id])?
            == 0
        {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
    /// Read one frozen puzzle without deserializing the whole set.
    pub fn set_puzzle(&self, set_id: i64, puzzle_id: &str) -> Result<Option<Puzzle>> {
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT puzzle_json FROM set_puzzles WHERE set_id=?1 AND puzzle_id=?2",
                params![set_id, puzzle_id],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|json| serde_json::from_str(&json).map_err(StoreError::from))
            .transpose()
    }
    pub fn set_puzzles(&self, id: i64) -> Result<Vec<Puzzle>> {
        let mut stmt = self
            .connection
            .prepare("SELECT puzzle_json FROM set_puzzles WHERE set_id=?1 ORDER BY position")?;
        let rows = stmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }
    /// Resume an active cycle, otherwise start the next using the frozen set membership.
    pub fn start_cycle(&mut self, set_id: i64, now: i64) -> Result<CycleRecord> {
        if let Some(active) = self
            .cycles(set_id)?
            .into_iter()
            .find(|c| c.completed_at.is_none())
        {
            return Ok(active);
        }
        let set = self.set(set_id)?.ok_or(StoreError::NotFound)?;
        let number = self.cycles(set_id)?.last().map_or(1, |c| c.number + 1);
        if number > 10 {
            return Err(CycleError::MaximumReached.into());
        }
        if number > set.params.target_cycles {
            return Err(CycleError::TargetReached.into());
        }
        let fixed: Vec<_> = self
            .set_puzzles(set_id)?
            .into_iter()
            .map(|p| p.id)
            .collect();
        let order = cycle_order(&fixed, set.params.shuffle_each_cycle, set.seed, number);
        self.connection.execute(
            "INSERT INTO cycles(set_id,number,started_at,order_json) VALUES(?1,?2,?3,?4)",
            params![set_id, number, now, serde_json::to_string(&order)?],
        )?;
        Ok(CycleRecord {
            id: self.connection.last_insert_rowid(),
            set_id,
            number,
            started_at: now,
            completed_at: None,
            order,
        })
    }
    pub fn extend_set(&self, set_id: i64) -> Result<u8> {
        let set = self.set(set_id)?.ok_or(StoreError::NotFound)?;
        let cycles = self.cycles(set_id)?;
        if !cycles
            .last()
            .is_some_and(|c| c.completed_at.is_some() && c.number == set.params.target_cycles)
        {
            return Err(CycleError::Incomplete.into());
        }
        if set.params.target_cycles >= 10 {
            return Err(CycleError::MaximumReached.into());
        }
        let target = set.params.target_cycles + 1;
        self.connection.execute(
            "UPDATE sets SET target_cycles=?1 WHERE id=?2",
            params![target, set_id],
        )?;
        Ok(target)
    }
    pub fn cycle(&self, id: i64) -> Result<Option<CycleRecord>> {
        Ok(self.connection.query_row("SELECT id,set_id,number,started_at,completed_at,order_json FROM cycles WHERE id=?1",[id],read_cycle).optional()?)
    }
    pub fn cycles(&self, set_id: i64) -> Result<Vec<CycleRecord>> {
        let mut stmt=self.connection.prepare("SELECT id,set_id,number,started_at,completed_at,order_json FROM cycles WHERE set_id=?1 ORDER BY number")?;
        Ok(stmt
            .query_map([set_id], read_cycle)?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn cycle_progress(&self, cycle_id: i64) -> Result<CycleProgress> {
        let cycle = self.cycle(cycle_id)?.ok_or(StoreError::NotFound)?;
        let set = self.set(cycle.set_id)?.ok_or(StoreError::NotFound)?;
        let mut progress = CycleProgress::new(cycle.number, set.params.target_cycles, cycle.order)?;
        let mut seen = BTreeSet::new();
        for a in self
            .attempts(cycle.set_id)?
            .iter()
            .filter(|a| a.kind == AttemptKind::Cycle && a.cycle_id == Some(cycle_id))
        {
            if seen.insert(&a.puzzle_id) {
                progress.record(&a.puzzle_id, a.outcome)?;
            }
        }
        Ok(progress)
    }
}
fn conversion_error(
    column: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(error))
}
pub(super) fn theme_match_name(mode: ThemeMatch) -> &'static str {
    match mode {
        ThemeMatch::Any => "any",
        ThemeMatch::All => "all",
    }
}
fn read_set(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredSet> {
    let seed: String = r.get(10)?;
    let mode: String = r.get(9)?;
    let themes = |column: usize| -> rusqlite::Result<Vec<String>> {
        let json: String = r.get(column)?;
        serde_json::from_str(&json).map_err(|e| conversion_error(column, e))
    };
    let theme_match: String = r.get(12)?;
    Ok(StoredSet {
        id: r.get(0)?,
        params: BuilderParams {
            name: r.get(1)?,
            include_themes: themes(2)?,
            exclude_themes: themes(11)?,
            theme_match: if theme_match == "all" {
                ThemeMatch::All
            } else {
                ThemeMatch::Any
            },
            rating_min: r.get(3)?,
            rating_max: r.get(4)?,
            size: r.get(5)?,
            target_cycles: r.get(6)?,
            preset_id: r.get(8)?,
            shuffle_each_cycle: mode == "shuffle",
        },
        created_at: r.get(7)?,
        seed: seed.parse().map_err(|e| conversion_error(10, e))?,
    })
}
fn read_cycle(r: &rusqlite::Row<'_>) -> rusqlite::Result<CycleRecord> {
    let order: String = r.get(5)?;
    Ok(CycleRecord {
        id: r.get(0)?,
        set_id: r.get(1)?,
        number: r.get(2)?,
        started_at: r.get(3)?,
        completed_at: r.get(4)?,
        order: serde_json::from_str(&order).map_err(|e| conversion_error(5, e))?,
    })
}
