use super::{Result, Store, StoreError};
use crate::{
    puzzle::Outcome,
    review::{
        ReviewCard, ReviewChange, ReviewItem, ReviewSort, ReviewStatus, filter_and_sort, transition,
    },
    stats::{Accuracy, Attempt, AttemptKind, CycleStats},
};
use rusqlite::{Connection, OptionalExtension, params};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedAttempt {
    pub id: i64,
    pub review_change: ReviewChange,
    pub cycle_completed: bool,
}
impl Store {
    /// Attempt insertion, review upsert, and cycle completion commit together or all roll back.
    pub fn record_attempt(&mut self, attempt: &Attempt) -> Result<RecordedAttempt> {
        use chrono::{Datelike, TimeZone};
        let day = chrono::Local
            .timestamp_opt(attempt.created_at, 0)
            .single()
            .ok_or_else(|| StoreError::InvalidData("Attempt timestamp is out of range.".into()))?
            .date_naive()
            .num_days_from_ce();
        self.record_attempt_on_day(attempt, i64::from(day))
    }
    /// Explicit local calendar day supports deterministic clocks and streak tests.
    pub fn record_attempt_on_day(
        &mut self,
        attempt: &Attempt,
        day: i64,
    ) -> Result<RecordedAttempt> {
        let time_ms = i64::try_from(attempt.time_ms.max(1)).map_err(|_| {
            StoreError::InvalidData("Recorded time exceeds SQLite integer range.".into())
        })?;
        if attempt.kind == AttemptKind::Cycle {
            let id = attempt
                .cycle_id
                .ok_or_else(|| StoreError::InvalidData("Cycle attempt needs a cycle.".into()))?;
            let cycle = self.cycle(id)?.ok_or(StoreError::NotFound)?;
            if cycle.set_id != attempt.set_id
                || cycle.completed_at.is_some()
                || self.cycle_progress(id)?.next_puzzle() != Some(&attempt.puzzle_id)
            {
                return Err(StoreError::InvalidData(
                    "Cycle attempt must answer the next pending puzzle of its own set.".into(),
                ));
            }
        } else if attempt.cycle_id.is_some() {
            return Err(StoreError::InvalidData(
                "Review and demo attempts cannot affect a cycle.".into(),
            ));
        }
        let tx = self.connection.transaction()?;
        let existing = read_review(&tx, attempt.set_id, &attempt.puzzle_id)?;
        if attempt.kind == AttemptKind::Review
            && !existing
                .as_ref()
                .is_some_and(|i| i.status == ReviewStatus::Pending)
        {
            return Err(StoreError::InvalidData(
                "Only pending items can be solved in Review.".into(),
            ));
        }
        let review_change = transition(existing.as_ref(), attempt);
        tx.execute("INSERT INTO attempts(puzzle_id,set_id,cycle_id,kind,outcome,time_ms,moves_played,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![attempt.puzzle_id,attempt.set_id,attempt.cycle_id,kind_name(attempt.kind),outcome_name(attempt.outcome),time_ms,serde_json::to_string(&attempt.moves_played)?,attempt.created_at])?;
        let id = tx.last_insert_rowid();
        if let ReviewChange::Save(item) = &review_change {
            tx.execute("INSERT INTO review_items(puzzle_id,set_id,added_at,last_missed_at,status,cleared_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(puzzle_id,set_id) DO UPDATE SET added_at=excluded.added_at,last_missed_at=excluded.last_missed_at,status=excluded.status,cleared_at=excluded.cleared_at",params![item.puzzle_id,item.set_id,item.added_at,item.last_missed_at,if item.status==ReviewStatus::Pending {"pending"} else {"cleared"},item.cleared_at])?;
        }
        let mut cycle_completed = false;
        if let Some(cycle_id) = attempt.cycle_id {
            let (count,size):(u32,u32)=tx.query_row("SELECT (SELECT count(*) FROM attempts WHERE cycle_id=?1 AND kind='cycle'),size FROM sets WHERE id=?2",params![cycle_id,attempt.set_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if count == size {
                tx.execute(
                    "UPDATE cycles SET completed_at=?1 WHERE id=?2",
                    params![attempt.created_at, cycle_id],
                )?;
                cycle_completed = true;
            }
        }
        if attempt.kind != AttemptKind::Demo {
            let json: Option<String> = tx
                .query_row(
                    "SELECT value FROM settings WHERE key='motivation'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let mut activity: crate::motivation::Motivation = json
                .map(|s| serde_json::from_str(&s))
                .transpose()?
                .unwrap_or_default();
            activity.record(day, attempt.outcome, cycle_completed);
            tx.execute("INSERT INTO settings(key,value) VALUES('motivation',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&activity)?])?;
        }
        tx.commit()?;
        Ok(RecordedAttempt {
            id,
            review_change,
            cycle_completed,
        })
    }
    pub fn attempts(&self, set_id: i64) -> Result<Vec<Attempt>> {
        let mut stmt=self.connection.prepare("SELECT puzzle_id,set_id,cycle_id,kind,outcome,time_ms,moves_played,created_at FROM attempts WHERE set_id=?1 ORDER BY created_at,id")?;
        let rows = stmt
            .query_map([set_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    super::unsigned(r, 5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i64>(7)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(
                |(puzzle_id, set_id, cycle_id, kind, outcome, time_ms, moves, created_at)| {
                    Ok(Attempt {
                        puzzle_id,
                        set_id,
                        cycle_id,
                        kind: parse_kind(&kind)?,
                        outcome: parse_outcome(&outcome)?,
                        time_ms,
                        moves_played: serde_json::from_str(&moves)?,
                        created_at,
                    })
                },
            )
            .collect()
    }
    pub fn cycle_stats(&self, id: i64) -> Result<CycleStats> {
        let cycle = self.cycle(id)?.ok_or(StoreError::NotFound)?;
        Ok(CycleStats::from_attempts(
            self.attempts(cycle.set_id)?
                .iter()
                .filter(|a| a.cycle_id == Some(id)),
        ))
    }
    pub fn lifetime_accuracy(&self, puzzle_id: &str) -> Result<Accuracy> {
        Ok(self.connection.query_row("SELECT coalesce(sum(outcome='correct'),0),count(*) FROM attempts WHERE puzzle_id=?1 AND kind!='demo' AND outcome!='invalid'",[puzzle_id],|r|Ok(Accuracy {correct:r.get(0)?,attempts:r.get(1)?}))?)
    }
    pub fn review_item(&self, set_id: i64, puzzle_id: &str) -> Result<Option<ReviewItem>> {
        read_review(&self.connection, set_id, puzzle_id)
    }
    pub fn pending_review_count(&self) -> Result<u32> {
        Ok(self.connection.query_row(
            "SELECT count(*) FROM review_items WHERE status='pending'",
            [],
            |r| r.get(0),
        )?)
    }
    pub fn review_cards(&self, theme: Option<&str>, sort: ReviewSort) -> Result<Vec<ReviewCard>> {
        let mut stmt=self.connection.prepare("SELECT r.puzzle_id,r.set_id,r.added_at,r.last_missed_at,r.status,r.cleared_at,s.name,p.position,p.puzzle_json FROM review_items r JOIN sets s ON s.id=r.set_id JOIN set_puzzles p ON p.set_id=r.set_id AND p.puzzle_id=r.puzzle_id WHERE r.status='pending'")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    review_from_row(r)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, u32>(7)?,
                    r.get::<_, String>(8)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let cards = rows
            .into_iter()
            .map(|(item, set_name, position, json)| {
                let accuracy = self.lifetime_accuracy(&item.puzzle_id)?;
                Ok(ReviewCard {
                    item,
                    set_name,
                    position,
                    puzzle: serde_json::from_str(&json)?,
                    accuracy,
                })
            })
            .collect::<Result<_>>()?;
        Ok(filter_and_sort(cards, theme, sort))
    }
}
fn review_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewItem> {
    let status: String = r.get(4)?;
    Ok(ReviewItem {
        puzzle_id: r.get(0)?,
        set_id: r.get(1)?,
        added_at: r.get(2)?,
        last_missed_at: r.get(3)?,
        status: if status == "pending" {
            ReviewStatus::Pending
        } else {
            ReviewStatus::Cleared
        },
        cleared_at: r.get(5)?,
    })
}
fn read_review(conn: &Connection, set_id: i64, puzzle_id: &str) -> Result<Option<ReviewItem>> {
    Ok(conn.query_row("SELECT puzzle_id,set_id,added_at,last_missed_at,status,cleared_at FROM review_items WHERE set_id=?1 AND puzzle_id=?2",params![set_id,puzzle_id],review_from_row).optional()?)
}
fn kind_name(kind: AttemptKind) -> &'static str {
    match kind {
        AttemptKind::Cycle => "cycle",
        AttemptKind::Review => "review",
        AttemptKind::Demo => "demo",
    }
}
fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Correct => "correct",
        Outcome::Incorrect => "incorrect",
        Outcome::Skipped => "skipped",
        Outcome::Invalid => "invalid",
    }
}
fn parse_kind(s: &str) -> Result<AttemptKind> {
    match s {
        "cycle" => Ok(AttemptKind::Cycle),
        "review" => Ok(AttemptKind::Review),
        "demo" => Ok(AttemptKind::Demo),
        _ => Err(StoreError::InvalidData(format!(
            "Unknown attempt kind: {s}"
        ))),
    }
}
fn parse_outcome(s: &str) -> Result<Outcome> {
    match s {
        "correct" => Ok(Outcome::Correct),
        "incorrect" => Ok(Outcome::Incorrect),
        "skipped" => Ok(Outcome::Skipped),
        "invalid" => Ok(Outcome::Invalid),
        _ => Err(StoreError::InvalidData(format!("Unknown outcome: {s}"))),
    }
}
