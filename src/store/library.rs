use super::{Result, Store};
use crate::source::{LibrarySize, RATING_BANDS};
impl Store {
    /// Local markers deliberately bypass the synced preferences and their field clocks.
    pub fn record_library_import(&self, size: LibrarySize, at: i64) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let previous = self
            .setting("library_size")?
            .and_then(|s| LibrarySize::parse(&s));
        self.set_setting(
            "library_size",
            previous.map_or(size, |old| old.max(size)).key(),
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
