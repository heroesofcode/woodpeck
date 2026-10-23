use super::{CsvPuzzles, ImportProgress, SourceError};
use crate::{puzzle::Puzzle, store::Store};
use std::{
    io::{BufRead, BufReader, Cursor, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub const LICHESS_DATABASE_URL: &str = "https://database.lichess.org/lichess_db_puzzle.csv.zst";
pub const STARTER_VERSION: &str = "2026-10-22.1";
pub const STARTER_BYTES: &[u8] = include_bytes!("../../assets/puzzles-starter.csv.zst");
pub const IMPORT_BATCH_SIZE: usize = 1000;

#[derive(Clone, Debug, Default)]
pub struct CancelFlag(Arc<AtomicBool>);
impl CancelFlag {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<(), SourceError> {
        if self.is_cancelled() {
            Err(SourceError::Cancelled)
        } else {
            Ok(())
        }
    }
}
struct Counted<R> {
    reader: R,
    bytes: Arc<AtomicU64>,
    cancel: CancelFlag,
}
impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other("Operation cancelled."));
        }
        let n = self.reader.read(buffer)?;
        self.bytes.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}
/// Stream CSV (plain or zstd, detected by magic), committing only bounded batches.
/// On cancellation, committed batches remain; an incomplete batch is discarded.
pub fn import_stream(
    reader: impl Read,
    cancel: &CancelFlag,
    mut progress: impl FnMut(ImportProgress),
    mut save: impl FnMut(&[Puzzle]) -> Result<(), SourceError>,
) -> Result<ImportProgress, SourceError> {
    cancel.check()?;
    let bytes = Arc::new(AtomicU64::new(0));
    let counted = Counted {
        reader,
        bytes: bytes.clone(),
        cancel: cancel.clone(),
    };
    let mut buffered = BufReader::new(counted);
    let compressed = buffered.fill_buf()?.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]);
    let decoded: Box<dyn Read + '_> = if compressed {
        Box::new(zstd::stream::read::Decoder::with_buffer(buffered)?)
    } else {
        Box::new(buffered)
    };
    let mut rows = CsvPuzzles::new(decoded);
    let mut batch = Vec::with_capacity(IMPORT_BATCH_SIZE);
    progress(ImportProgress::default());
    loop {
        cancel.check()?;
        let next = rows.next_row();
        cancel.check()?;
        let Some(row) = next else { break };
        if let Some(puzzle) = row? {
            batch.push(puzzle);
        }
        if batch.len() == IMPORT_BATCH_SIZE {
            cancel.check()?;
            save(&batch)?;
            batch.clear();
        }
        if rows.progress().scanned.is_multiple_of(1000) {
            progress(ImportProgress {
                bytes: bytes.load(Ordering::Relaxed),
                ..rows.progress()
            });
        }
    }
    cancel.check()?;
    if !batch.is_empty() {
        save(&batch)?;
    }
    let result = ImportProgress {
        bytes: bytes.load(Ordering::Relaxed),
        ..rows.progress()
    };
    progress(result);
    cancel.check()?;
    Ok(result)
}
pub fn import_reader(
    store: &mut Store,
    reader: impl Read,
    cancel: &CancelFlag,
    progress: impl FnMut(ImportProgress),
) -> Result<ImportProgress, SourceError> {
    import_stream(reader, cancel, progress, |batch| {
        Ok(store.upsert_puzzles(batch)?)
    })
}
/// Marker advances only after a successful import. Force implements “re-import starter”.
pub fn import_starter(
    store: &mut Store,
    force: bool,
    cancel: &CancelFlag,
    progress: impl FnMut(ImportProgress),
) -> Result<ImportProgress, SourceError> {
    import_starter_version(store, STARTER_VERSION, force, cancel, progress)
}
fn import_starter_version(
    store: &mut Store,
    version: &str,
    force: bool,
    cancel: &CancelFlag,
    progress: impl FnMut(ImportProgress),
) -> Result<ImportProgress, SourceError> {
    cancel.check()?;
    if !force && store.setting("starter_version")?.as_deref() == Some(version) {
        return Ok(ImportProgress::default());
    }
    let result = import_reader(store, Cursor::new(STARTER_BYTES), cancel, progress)?;
    store.set_setting("starter_version", version)?;
    Ok(result)
}
pub fn import_location(
    store: &mut Store,
    location: &str,
    cancel: &CancelFlag,
    progress: impl FnMut(ImportProgress),
) -> Result<ImportProgress, SourceError> {
    let reader = open_location(location, cancel)?;
    import_reader(store, reader, cancel, progress)
}
pub fn open_location(
    location: &str,
    cancel: &CancelFlag,
) -> Result<Box<dyn Read + Send>, SourceError> {
    cancel.check()?;
    if location.starts_with("https://") || location.starts_with("http://") {
        let response = super::client::request(&super::client::agent(), location, cancel)?;
        Ok(Box::new(response.into_body().into_reader()))
    } else {
        Ok(Box::new(std::fs::File::open(location)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_eleven_columns_and_ten_columns_with_missing_trailing_fields() {
        let fixture = include_str!("../../tests/fixtures/lichess.csv");
        let full = CsvPuzzles::new(fixture.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_ne!(full.len(), 0);
        let ten = fixture
            .lines()
            .map(|l| l.rsplit_once(',').unwrap().0)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            CsvPuzzles::new(ten.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            full
        );
        let short = fixture
            .lines()
            .enumerate()
            .map(|(i, l)| if i == 0 { l } else { l.trim_end_matches(',') })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            CsvPuzzles::new(short.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            full
        );
    }
    #[test]
    fn progress_counts_bytes_rows_and_cancel_discards_pending_batch() {
        let input = include_bytes!("../../tests/fixtures/lichess.csv");
        let mut store = Store::memory().unwrap();
        let mut reports = vec![];
        let p = import_reader(&mut store, input.as_slice(), &CancelFlag::default(), |p| {
            reports.push(p);
        })
        .unwrap();
        assert_eq!(p.scanned, 12);
        assert_eq!(p.accepted + p.rejected, 12);
        assert_eq!(p.bytes, input.len() as u64);
        assert_eq!(store.puzzle_count().unwrap(), p.accepted);
        assert_eq!(reports.last(), Some(&p));
        let flag = CancelFlag::default();
        assert!(matches!(
            import_reader(&mut store, input.as_slice(), &flag, |_| flag.cancel()),
            Err(SourceError::Cancelled)
        ));
    }
    #[test]
    fn starter_is_versioned_idempotent_and_reimportable() {
        let mut s = Store::memory().unwrap();
        let c = CancelFlag::default();
        let first = import_starter(&mut s, false, &c, |_| {}).unwrap();
        assert_eq!(first.accepted, 8540);
        assert_eq!(s.puzzle_count().unwrap(), 8540);
        assert_eq!(
            import_starter(&mut s, false, &c, |_| {}).unwrap().scanned,
            0
        );
        assert_eq!(
            import_starter(&mut s, true, &c, |_| {}).unwrap().accepted,
            8540
        );
        assert_eq!(
            import_starter_version(&mut s, "next", false, &c, |_| {})
                .unwrap()
                .accepted,
            8540
        );
        c.cancel();
        assert!(import_starter_version(&mut s, "cancelled", false, &c, |_| {}).is_err());
        assert_eq!(
            s.setting("starter_version").unwrap().as_deref(),
            Some("next")
        );
    }
}
