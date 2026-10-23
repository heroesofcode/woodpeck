use super::{Result, Store, StoreError};
use crate::{
    puzzle::Puzzle,
    sets::{PuzzleQuery, QueryError},
};
use rusqlite::{OptionalExtension, params};
impl Store {
    /// Import rows in a single transaction; active sets use their independent snapshots.
    pub fn upsert_puzzles(&mut self, puzzles: &[Puzzle]) -> Result<()> {
        let tx = self.connection.transaction()?;
        for p in puzzles {
            let plays = i64::try_from(p.nb_plays).map_err(|_| {
                StoreError::InvalidData("Play count exceeds SQLite integer range.".into())
            })?;
            tx.execute("INSERT INTO puzzles(id,fen,moves,rating,rd,popularity,nb_plays,themes_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET fen=excluded.fen,moves=excluded.moves,rating=excluded.rating,rd=excluded.rd,popularity=excluded.popularity,nb_plays=excluded.nb_plays,themes_json=excluded.themes_json",params![p.id,p.fen,p.moves,p.rating,p.rd,p.popularity,plays,serde_json::to_string(&p.themes)?])?;
            tx.execute("DELETE FROM puzzle_themes WHERE puzzle_id=?1", [&p.id])?;
            for theme in &p.themes {
                tx.execute(
                    "INSERT OR IGNORE INTO puzzle_themes(puzzle_id,theme,rating) VALUES(?1,?2,?3)",
                    params![p.id, theme, p.rating],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn puzzle(&self, id: &str) -> Result<Option<Puzzle>> {
        Ok(self.connection.query_row("SELECT id,fen,moves,rating,rd,popularity,nb_plays,themes_json FROM puzzles WHERE id=?1",[id],read_puzzle).optional()?)
    }
    pub fn puzzle_count(&self) -> Result<u64> {
        Ok(self
            .connection
            .query_row("SELECT count(*) FROM puzzles", [], |r| {
                super::unsigned(r, 0)
            })?)
    }
    pub fn query_puzzles(
        &self,
        minimum: u16,
        maximum: u16,
        theme: Option<&str>,
    ) -> Result<Vec<Puzzle>> {
        let mut stmt=self.connection.prepare("SELECT id,fen,moves,rating,rd,popularity,nb_plays,themes_json FROM puzzles p WHERE rating BETWEEN ?1 AND ?2 AND (?3 IS NULL OR EXISTS(SELECT 1 FROM puzzle_themes t WHERE t.puzzle_id=p.id AND t.theme=?3)) ORDER BY rating,id")?;
        Ok(stmt
            .query_map(params![minimum, maximum, theme], read_puzzle)?
            .collect::<std::result::Result<_, _>>()?)
    }
}
fn read_puzzle(row: &rusqlite::Row<'_>) -> rusqlite::Result<Puzzle> {
    let themes: String = row.get(7)?;
    Ok(Puzzle {
        id: row.get(0)?,
        fen: row.get(1)?,
        moves: row.get(2)?,
        rating: row.get(3)?,
        rd: row.get(4)?,
        popularity: row.get(5)?,
        nb_plays: super::unsigned(row, 6)?,
        themes: serde_json::from_str(&themes).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e))
        })?,
    })
}
impl PuzzleQuery for Store {
    fn query(
        &self,
        minimum: u16,
        maximum: u16,
        theme: Option<&str>,
    ) -> std::result::Result<Vec<Puzzle>, QueryError> {
        self.query_puzzles(minimum, maximum, theme)
            .map_err(|e| QueryError(e.to_string()))
    }
}
