use super::{Result, StoreError};
use rusqlite::Connection;
pub const VERSION: u32 = 5;
pub fn migrate(connection: &mut Connection) -> Result<()> {
    migrate_until(connection, VERSION)
}
/// Apply migrations up to `target`, letting tests stage data at an older schema.
pub(super) fn migrate_until(connection: &mut Connection, target: u32) -> Result<()> {
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > VERSION {
        return Err(StoreError::NewerSchema(version));
    }
    let tx = connection.transaction()?;
    if version < 1 && target >= 1 {
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
    if version < 2 && target >= 2 {
        tx.execute_batch(
            "CREATE INDEX puzzles_rating ON puzzles(rating);
        CREATE INDEX puzzle_themes_theme_rating ON puzzle_themes(theme,rating);
        CREATE UNIQUE INDEX cycle_puzzle_once ON attempts(cycle_id,puzzle_id) WHERE kind='cycle';
        CREATE UNIQUE INDEX one_active_cycle ON cycles(set_id) WHERE completed_at IS NULL;
        CREATE INDEX attempts_lifetime ON attempts(puzzle_id,kind,outcome);
        CREATE INDEX review_pending_recent ON review_items(status,last_missed_at DESC);",
        )?;
    }
    if version < 3 && target >= 3 {
        migrate_sync(&tx)?;
    }
    if version < 4 && target >= 4 {
        migrate_theme_filters(&tx)?;
    }
    if version < 5 && target >= 5 {
        migrate_rating_range(&tx)?;
    }
    tx.pragma_update(None, "user_version", target.max(version))?;
    tx.commit()?;
    Ok(())
}

fn migrate_sync(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
            "ALTER TABLE sets ADD COLUMN uid TEXT NOT NULL DEFAULT '';
             UPDATE sets SET uid=lower(hex(randomblob(16)));
             CREATE UNIQUE INDEX sets_uid ON sets(uid);
             ALTER TABLE sets ADD COLUMN name_updated_at INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE sets ADD COLUMN target_cycles_updated_at INTEGER NOT NULL DEFAULT 0;
             UPDATE sets SET name_updated_at=created_at*1000,target_cycles_updated_at=created_at*1000;
             CREATE TRIGGER sets_identity AFTER INSERT ON sets WHEN NEW.uid='' BEGIN
                UPDATE sets SET uid=lower(hex(randomblob(16))),name_updated_at=NEW.created_at*1000,
                target_cycles_updated_at=NEW.created_at*1000 WHERE id=NEW.id;
             END;
             CREATE TRIGGER sets_name_clock AFTER UPDATE OF name ON sets WHEN OLD.name!=NEW.name BEGIN
                UPDATE sets SET name_updated_at=max(OLD.name_updated_at+1,CAST(unixepoch('subsec')*1000 AS INTEGER)) WHERE id=NEW.id;
             END;
             CREATE TRIGGER sets_target_clock AFTER UPDATE OF target_cycles ON sets WHEN OLD.target_cycles!=NEW.target_cycles BEGIN
                UPDATE sets SET target_cycles_updated_at=max(OLD.target_cycles_updated_at+1,CAST(unixepoch('subsec')*1000 AS INTEGER)) WHERE id=NEW.id;
             END;
             ALTER TABLE cycles ADD COLUMN uid TEXT NOT NULL DEFAULT '';
             UPDATE cycles SET uid=(SELECT uid FROM sets WHERE id=set_id)||'-'||number;
             CREATE UNIQUE INDEX cycles_uid ON cycles(uid);
             CREATE TRIGGER cycles_identity AFTER INSERT ON cycles WHEN NEW.uid='' BEGIN
                UPDATE cycles SET uid=(SELECT uid FROM sets WHERE id=NEW.set_id)||'-'||NEW.number WHERE id=NEW.id;
             END;
             ALTER TABLE attempts ADD COLUMN uid TEXT NOT NULL DEFAULT '';
             UPDATE attempts SET uid=lower(hex(randomblob(16)));
             CREATE UNIQUE INDEX attempts_uid ON attempts(uid);
             CREATE TRIGGER attempts_identity AFTER INSERT ON attempts WHEN NEW.uid='' BEGIN
                UPDATE attempts SET uid=lower(hex(randomblob(16))) WHERE id=NEW.id;
             END;
             ALTER TABLE attempts ADD COLUMN local_day INTEGER;
             ALTER TABLE review_items ADD COLUMN uid TEXT NOT NULL DEFAULT '';
             UPDATE review_items SET uid=lower(hex(randomblob(16)));
             CREATE UNIQUE INDEX review_uid ON review_items(uid);
             CREATE TRIGGER review_identity AFTER INSERT ON review_items WHEN NEW.uid='' BEGIN
                UPDATE review_items SET uid=lower(hex(randomblob(16))) WHERE puzzle_id=NEW.puzzle_id AND set_id=NEW.set_id;
             END;
             ALTER TABLE review_items ADD COLUMN resolved_at INTEGER;
             UPDATE review_items SET resolved_at=cleared_at;
             CREATE TRIGGER review_resolution AFTER UPDATE OF cleared_at ON review_items WHEN NEW.cleared_at IS NOT NULL BEGIN
                UPDATE review_items SET resolved_at=max(coalesce(OLD.resolved_at,NEW.cleared_at),NEW.cleared_at) WHERE puzzle_id=NEW.puzzle_id AND set_id=NEW.set_id;
             END;
             CREATE TABLE sync_fields (key TEXT PRIMARY KEY,value TEXT NOT NULL,updated_at INTEGER NOT NULL);
             CREATE TABLE sync_changes (id INTEGER PRIMARY KEY CHECK(id=1),generation INTEGER NOT NULL);
             INSERT INTO sync_changes VALUES(1,0);
             DROP INDEX cycle_puzzle_once;
             CREATE INDEX cycle_puzzle ON attempts(cycle_id,puzzle_id);",
        )?;

    for table in ["sets", "cycles", "attempts", "review_items"] {
        for operation in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!(
                    "CREATE TRIGGER {table}_sync_{operation} AFTER {operation} ON {table} BEGIN UPDATE sync_changes SET generation=generation+1; END;"
                ))?;
        }
    }

    Ok(())
}

/// Replace the single focus theme with include/exclude lists; a focus becomes `include=[focus]`.
/// Sync clocks move with the field so a migrated set never looks newer than its remote copy.
fn migrate_theme_filters(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "ALTER TABLE sets ADD COLUMN include_themes TEXT NOT NULL DEFAULT '[]';
         ALTER TABLE sets ADD COLUMN exclude_themes TEXT NOT NULL DEFAULT '[]';
         ALTER TABLE sets ADD COLUMN theme_match TEXT NOT NULL DEFAULT 'any' CHECK(theme_match IN ('any','all'));
         UPDATE sets SET include_themes=json_array(focus_theme) WHERE focus_theme IS NOT NULL AND focus_theme!='';
         INSERT OR IGNORE INTO sync_fields(key,value,updated_at)
            SELECT 'set/'||s.uid||'/'||k.name,CASE k.name WHEN 'include_themes' THEN s.include_themes WHEN 'exclude_themes' THEN '[]' ELSE json_quote('any') END,f.updated_at
            FROM sync_fields f JOIN sets s ON f.key='set/'||s.uid||'/focus_theme'
            CROSS JOIN (SELECT 'include_themes' AS name UNION ALL SELECT 'exclude_themes' UNION ALL SELECT 'theme_match') k;
         DELETE FROM sync_fields WHERE key LIKE 'set/%/focus_theme';
         ALTER TABLE sets DROP COLUMN focus_theme;",
    )
}

/// Replace `target_rating ± variance` (and the recommended-range toggle) with an explicit
/// inclusive range, using the same rule as [`crate::sets::legacy_range`]: the old band clamped
/// to 400–3000, grown upward to the 100-point minimum width. Both range fields take the later of
/// the two legacy sync clocks, matching the portable-format upgrade.
fn migrate_rating_range(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "ALTER TABLE sets ADD COLUMN rating_min INTEGER NOT NULL DEFAULT 900 CHECK(rating_min BETWEEN 400 AND 3200);
         ALTER TABLE sets ADD COLUMN rating_max INTEGER NOT NULL DEFAULT 1100 CHECK(rating_max BETWEEN 400 AND 3200 AND rating_max>=rating_min+100);
         UPDATE sets SET rating_min=min(3100,max(400,target_rating-variance)),
            rating_max=max(min(3000,target_rating+variance),min(3100,max(400,target_rating-variance))+100);
         INSERT OR REPLACE INTO sync_fields(key,value,updated_at)
            SELECT 'set/'||s.uid||'/'||k.name,CASE k.name WHEN 'rating_min' THEN s.rating_min ELSE s.rating_max END,max(f.updated_at)
            FROM sets s JOIN sync_fields f ON f.key IN ('set/'||s.uid||'/target_rating','set/'||s.uid||'/variance')
            CROSS JOIN (SELECT 'rating_min' AS name UNION ALL SELECT 'rating_max') k
            GROUP BY s.id,k.name;
         DELETE FROM sync_fields WHERE key LIKE 'set/%/target_rating' OR key LIKE 'set/%/variance' OR key LIKE 'set/%/restrict_recommended';
         ALTER TABLE sets DROP COLUMN target_rating;
         ALTER TABLE sets DROP COLUMN variance;
         ALTER TABLE sets DROP COLUMN restrict_recommended;",
    )
}
