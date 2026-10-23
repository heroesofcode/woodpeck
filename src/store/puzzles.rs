use super::{Result, Store, StoreError};
use crate::{
    puzzle::Puzzle,
    sets::{PuzzleQuery, QueryError, ThemeFilter, ThemeMatch},
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
        themes: ThemeFilter<'_>,
    ) -> Result<Vec<Puzzle>> {
        let (sql, values) = theme_query(minimum, maximum, themes);
        let mut stmt = self.connection.prepare(&sql)?;
        Ok(stmt
            .query_map(rusqlite::params_from_iter(values), read_puzzle)?
            .collect::<std::result::Result<_, _>>()?)
    }
}
/// Builds a parameterized query; theme keys are always bound, never interpolated.
fn theme_query(
    minimum: u16,
    maximum: u16,
    themes: ThemeFilter<'_>,
) -> (String, Vec<rusqlite::types::Value>) {
    use rusqlite::types::Value;
    const HAS: &str = "EXISTS(SELECT 1 FROM puzzle_themes t WHERE t.puzzle_id=p.id AND t.theme";
    let placeholders = |n: usize| vec!["?"; n].join(",");
    let mut sql = String::from(
        "SELECT id,fen,moves,rating,rd,popularity,nb_plays,themes_json FROM puzzles p WHERE rating BETWEEN ? AND ?",
    );
    let mut values = vec![
        Value::from(i64::from(minimum)),
        Value::from(i64::from(maximum)),
    ];
    if !themes.include.is_empty() {
        let clause = match themes.mode {
            ThemeMatch::Any => [
                " AND ",
                HAS,
                " IN (",
                &placeholders(themes.include.len()),
                "))",
            ]
            .concat(),
            ThemeMatch::All => [" AND ", HAS, "=?)"].concat().repeat(themes.include.len()),
        };
        sql.push_str(&clause);
        values.extend(themes.include.iter().cloned().map(Value::from));
    }
    if !themes.exclude.is_empty() {
        sql.push_str(
            &[
                " AND NOT ",
                HAS,
                " IN (",
                &placeholders(themes.exclude.len()),
                "))",
            ]
            .concat(),
        );
        values.extend(themes.exclude.iter().cloned().map(Value::from));
    }
    sql.push_str(" ORDER BY rating,id");
    (sql, values)
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
        themes: ThemeFilter<'_>,
    ) -> std::result::Result<Vec<Puzzle>, QueryError> {
        self.query_puzzles(minimum, maximum, themes)
            .map_err(|e| QueryError(e.to_string()))
    }
}
