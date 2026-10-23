use super::{Result, StoreError};
use rusqlite::Connection;
pub const VERSION: u32 = 2;
pub fn migrate(connection: &mut Connection) -> Result<()> {
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > VERSION {
        return Err(StoreError::NewerSchema(version));
    }
    let tx = connection.transaction()?;
    if version < 1 {
        tx.execute_batch("CREATE TABLE puzzles (
            id TEXT PRIMARY KEY, fen TEXT NOT NULL, moves TEXT NOT NULL, rating INTEGER NOT NULL,
            rd INTEGER NOT NULL, popularity INTEGER NOT NULL, nb_plays INTEGER NOT NULL, themes_json TEXT NOT NULL
        );
        CREATE TABLE puzzle_themes (puzzle_id TEXT NOT NULL REFERENCES puzzles(id) ON DELETE CASCADE,theme TEXT NOT NULL,rating INTEGER NOT NULL,PRIMARY KEY(puzzle_id,theme));
        CREATE TABLE sets (
            id INTEGER PRIMARY KEY,name TEXT NOT NULL,focus_theme TEXT,target_rating INTEGER NOT NULL,variance INTEGER NOT NULL,
            size INTEGER NOT NULL CHECK(size BETWEEN 20 AND 500),target_cycles INTEGER NOT NULL CHECK(target_cycles BETWEEN 1 AND 10),
            created_at INTEGER NOT NULL,preset_id TEXT,order_mode TEXT NOT NULL CHECK(order_mode IN ('fixed','shuffle')),
            restrict_recommended INTEGER NOT NULL,seed TEXT NOT NULL
        );
        CREATE TABLE set_puzzles (
            set_id INTEGER NOT NULL REFERENCES sets(id) ON DELETE CASCADE,position INTEGER NOT NULL CHECK(position>0),puzzle_id TEXT NOT NULL,puzzle_json TEXT NOT NULL,
            PRIMARY KEY(set_id,position),UNIQUE(set_id,puzzle_id)
        );
        CREATE TABLE cycles (
            id INTEGER PRIMARY KEY,set_id INTEGER NOT NULL REFERENCES sets(id) ON DELETE CASCADE,
            number INTEGER NOT NULL CHECK(number BETWEEN 1 AND 10),started_at INTEGER NOT NULL,completed_at INTEGER,order_json TEXT NOT NULL,
            UNIQUE(set_id,number),UNIQUE(id,set_id)
        );
        CREATE TABLE attempts (
            id INTEGER PRIMARY KEY,puzzle_id TEXT NOT NULL,set_id INTEGER NOT NULL,cycle_id INTEGER,
            kind TEXT NOT NULL CHECK(kind IN ('cycle','review','demo')),
            outcome TEXT NOT NULL CHECK(outcome IN ('correct','incorrect','skipped','invalid')),
            time_ms INTEGER NOT NULL CHECK(time_ms>=1),moves_played TEXT NOT NULL,created_at INTEGER NOT NULL,
            CHECK((kind='cycle' AND cycle_id IS NOT NULL) OR (kind!='cycle' AND cycle_id IS NULL)),
            FOREIGN KEY(set_id,puzzle_id) REFERENCES set_puzzles(set_id,puzzle_id),
            FOREIGN KEY(cycle_id,set_id) REFERENCES cycles(id,set_id)
        );
        CREATE TABLE review_items (
            puzzle_id TEXT NOT NULL,set_id INTEGER NOT NULL,added_at INTEGER NOT NULL,last_missed_at INTEGER NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('pending','cleared')),cleared_at INTEGER,
            CHECK((status='pending' AND cleared_at IS NULL) OR (status='cleared' AND cleared_at IS NOT NULL)),
            PRIMARY KEY(puzzle_id,set_id),FOREIGN KEY(set_id,puzzle_id) REFERENCES set_puzzles(set_id,puzzle_id)
        );
        CREATE TABLE settings (key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
    }
    if version < 2 {
        tx.execute_batch(
            "CREATE INDEX puzzles_rating ON puzzles(rating);
        CREATE INDEX puzzle_themes_theme_rating ON puzzle_themes(theme,rating);
        CREATE UNIQUE INDEX cycle_puzzle_once ON attempts(cycle_id,puzzle_id) WHERE kind='cycle';
        CREATE UNIQUE INDEX one_active_cycle ON cycles(set_id) WHERE completed_at IS NULL;
        CREATE INDEX attempts_lifetime ON attempts(puzzle_id,kind,outcome);
        CREATE INDEX review_pending_recent ON review_items(status,last_missed_at DESC);",
        )?;
    }
    tx.pragma_update(None, "user_version", VERSION)?;
    tx.commit()?;
    Ok(())
}
