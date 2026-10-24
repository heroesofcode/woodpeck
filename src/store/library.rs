use super::{Result, Store};
use crate::source::{LibrarySize, RATING_BANDS};
impl Store {
    /// Local markers deliberately bypass the synced preferences and their field clocks.
    pub fn record_library_import(&self, size: LibrarySize, at: i64) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let previous = self
            .setting("library_size")?
            .and_then(|s| LibrarySize::parse(&s));
        // Older installations have no marker. Committed puzzles also provide a floor
        // if a Starter marker was written before size inference was introduced.
        let inferred = LibrarySize::from_puzzle_count(self.puzzle_count()?);
        self.set_setting(
            "library_size",
            previous.unwrap_or(inferred).max(inferred).max(size).key(),
        )?;
        self.set_setting("library_imported_at", &at.to_string())?;
        tx.commit()?;
        Ok(())
    }
    /// One grouped query, including clamped underflow and the 2800+ overflow band.
    pub fn rating_histogram(&self) -> Result<[u64; RATING_BANDS]> {
        let mut bands = [0; RATING_BANDS];
        let mut query = self
            .connection
            .prepare("SELECT min(12,max(0,(rating-400)/200)),count(*) FROM puzzles GROUP BY 1")?;
        for row in query.query_map([], |r| Ok((r.get::<_, u16>(0)?, super::unsigned(r, 1)?)))? {
            let (band, count) = row?;
            bands[usize::from(band)] = count;
        }
        Ok(bands)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starter_import_infers_unmarked_large_stores() {
        let store = Store::memory().unwrap();
        // Only the committed row count matters to marker inference.
        store
            .connection
            .execute(
                "WITH RECURSIVE ids(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM ids WHERE n<?1)
             INSERT INTO puzzles SELECT n,'','',1000,0,0,0,'[]' FROM ids",
                [i64::try_from(crate::source::FULL_MIN_COUNT).unwrap()],
            )
            .unwrap();
        store
            .record_library_import(LibrarySize::Starter, 10)
            .unwrap();
        assert_eq!(
            store.setting("library_size").unwrap().as_deref(),
            Some("full")
        );
        store
            .connection
            .execute("DELETE FROM settings WHERE key='library_size'", [])
            .unwrap();
        store
            .connection
            .execute(
                "DELETE FROM puzzles WHERE CAST(id AS INTEGER)>?1",
                [i64::try_from(crate::source::MEDIUM_COUNT).unwrap()],
            )
            .unwrap();
        store
            .record_library_import(LibrarySize::Starter, 20)
            .unwrap();
        assert_eq!(
            store.setting("library_size").unwrap().as_deref(),
            Some("medium")
        );
        store.connection.execute("DELETE FROM puzzles", []).unwrap();
        store
            .record_library_import(LibrarySize::Starter, 30)
            .unwrap();
        assert_eq!(
            store.setting("library_size").unwrap().as_deref(),
            Some("medium")
        );
    }
    #[test]
    fn markers_never_downgrade_and_histogram_clamps_edges() {
        let mut store = Store::memory().unwrap();
        for (i, rating) in [200, 400, 599, 600, 2799, 2800, 3200]
            .into_iter()
            .enumerate()
        {
            let mut p = crate::puzzle::fixture();
            p.id = i.to_string();
            p.rating = rating;
            store.upsert_puzzles(&[p]).unwrap();
        }
        let bands = store.rating_histogram().unwrap();
        assert_eq!(bands[0], 3);
        assert_eq!(bands[1], 1);
        assert_eq!(bands[11], 1);
        assert_eq!(bands[12], 2);
        store.record_library_import(LibrarySize::Full, 10).unwrap();
        store
            .record_library_import(LibrarySize::Starter, 20)
            .unwrap();
        assert_eq!(
            store.setting("library_size").unwrap().as_deref(),
            Some("full")
        );
        assert_eq!(
            store.setting("library_imported_at").unwrap().as_deref(),
            Some("20")
        );
    }
}
