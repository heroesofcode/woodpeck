use super::{CsvPuzzles, ImportOptions, ImportProgress, LibrarySize, SourceError};
use crate::{puzzle::Puzzle, store::Store};
use std::{
    io::{BufReader, Cursor, Read},
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
    progress: impl FnMut(ImportProgress),
    save: impl FnMut(&[Puzzle]) -> Result<(), SourceError>,
) -> Result<ImportProgress, SourceError> {
    import_stream_options(reader, ImportOptions::default(), cancel, progress, save)
}
pub fn import_stream_options(
    reader: impl Read,
    options: ImportOptions,
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
    // A network Read may return fewer than four bytes before EOF. Collect the entire magic.
    let mut prefix = Vec::with_capacity(4);
    let sniffed = buffered.by_ref().take(4).read_to_end(&mut prefix);
    cancel.check()?;
    sniffed?;
    let compressed = matches!(
        prefix.as_slice(),
        [0x28, 0xb5, 0x2f, 0xfd] | [0x50..=0x5f, 0x2a, 0x4d, 0x18]
    );
    let input = Cursor::new(prefix).chain(buffered);
    let decoded: Box<dyn Read + '_> = if compressed {
        Box::new(zstd::stream::read::Decoder::new(input)?)
    } else {
        Box::new(input)
    };
    let mut rows = CsvPuzzles::new(decoded);
    let mut batch = Vec::with_capacity(IMPORT_BATCH_SIZE);
    progress(ImportProgress::default());
    loop {
        if options
            .limit
            .is_some_and(|limit| rows.progress().accepted >= limit)
        {
            break;
        }
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
    store.record_library_import(LibrarySize::Starter, chrono::Utc::now().timestamp())?;
    Ok(result)
}
pub fn import_location(
    store: &mut Store,
    location: &str,
    cancel: &CancelFlag,
    progress: impl FnMut(ImportProgress),
) -> Result<ImportProgress, SourceError> {
    import_location_options(
        store,
        location,
        LibrarySize::Full,
        ImportOptions::default(),
        cancel,
        progress,
    )
}
pub fn import_location_options(
    store: &mut Store,
    location: &str,
    size: LibrarySize,
    options: ImportOptions,
    cancel: &CancelFlag,
    progress: impl FnMut(ImportProgress),
) -> Result<ImportProgress, SourceError> {
    let reader = open_location(location, cancel)?;
    let result = import_stream_options(reader, options, cancel, progress, |batch| {
        Ok(store.upsert_puzzles(batch)?)
    })?;
    cancel.check()?;
    store.record_library_import(size, chrono::Utc::now().timestamp())?;
    Ok(result)
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
    fn import_skips_lines_without_a_player_move_before_saving() {
        let p = crate::puzzle::fixture();
        let input = format!(
            "PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes\nempty,{},,1000,75,100,1000,fork\nsetup,{},b7f7,1000,75,100,1000,fork\nplayable,{},b7f7 h5g3,1000,75,100,1000,fork\n",
            p.fen, p.fen, p.fen
        );
        let mut store = Store::memory().unwrap();
        let progress =
            import_reader(&mut store, input.as_bytes(), &CancelFlag::default(), |_| {}).unwrap();
        assert_eq!(progress.scanned, 3);
        assert_eq!(progress.accepted, 1);
        assert_eq!(progress.rejected, 2);
        assert_eq!(store.puzzle_count().unwrap(), 1);
        assert!(store.puzzle("empty").unwrap().is_none());
        assert!(store.puzzle("setup").unwrap().is_none());
        assert_eq!(
            store.puzzle("playable").unwrap().unwrap().moves,
            "b7f7 h5g3"
        );
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

#[cfg(test)]
mod streaming_tests {
    use super::*;
    use std::fmt::Write;
    fn rows(count: usize, quality: bool) -> String {
        let p = crate::puzzle::fixture();
        let mut input = "PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags,DailyDate\n".to_owned();
        for i in 0..count {
            writeln!(
                input,
                "p{i},{},{},1000,75,{},1000,fork,,,",
                p.fen,
                p.moves,
                if quality { 100 } else { 0 }
            )
            .unwrap();
        }
        input
    }
    struct Fragmented<R>(R);
    impl<R: Read> Read for Fragmented<R> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let n = buffer.len().min(1);
            self.0.read(&mut buffer[..n])
        }
    }
    struct Cancelling<'a> {
        bytes: &'a [u8],
        flag: CancelFlag,
        boundary: usize,
    }
    impl Read for Cancelling<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.boundary == 0 {
                self.flag.cancel();
                return Ok(0);
            }
            let len = buffer.len().min(self.boundary);
            let n = self.bytes.read(&mut buffer[..len])?;
            self.boundary -= n;
            Ok(n)
        }
    }

    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("broken transport"))
        }
    }

    #[test]
    fn magic_detection_handles_one_byte_reads_and_reports_compressed_bytes() {
        let input = rows(3, true);
        let compressed = zstd::stream::encode_all(input.as_bytes(), 1).unwrap();
        let mut store = Store::memory().unwrap();
        let result = import_reader(
            &mut store,
            Fragmented(compressed.as_slice()),
            &CancelFlag::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(result.accepted, 3);
        assert_eq!(result.bytes, compressed.len() as u64);
    }
    #[test]
    fn zstd_skippable_prefix_matches_the_full_lichess_dump() {
        let input = rows(3, true);
        let compressed = zstd::stream::encode_all(input.as_bytes(), 1).unwrap();
        for marker in 0x50..=0x5f {
            let mut prefixed = vec![marker, 0x2a, 0x4d, 0x18, 4, 0, 0, 0, 0xe4, 4, 0x8a, 0];
            prefixed.extend_from_slice(&compressed);
            let mut store = Store::memory().unwrap();
            let result = import_reader(
                &mut store,
                prefixed.as_slice(),
                &CancelFlag::default(),
                |_| {},
            )
            .unwrap();
            assert_eq!(result.accepted, 3);
            assert_eq!(result.bytes, prefixed.len() as u64);
        }
    }
    #[test]
    fn cancellation_retains_committed_batches_and_discards_incomplete_batch() {
        let input = rows(2500, true);
        let flag = CancelFlag::default();
        let mut saved = Vec::new();
        let result = import_stream(
            input.as_bytes(),
            &flag,
            |p| {
                if p.scanned >= 1000 {
                    flag.cancel();
                }
            },
            |batch| {
                saved.push(batch.len());
                Ok(())
            },
        );
        assert!(matches!(result, Err(SourceError::Cancelled)));
        assert_eq!(saved, [1000]);
        // A read cancellation after 1100 accepted rows drops the remaining 100.
        let flag = CancelFlag::default();
        let header = input.lines().next().unwrap().len() + 1;
        let boundary = header
            + input
                .lines()
                .skip(1)
                .take(1100)
                .map(|l| l.len() + 1)
                .sum::<usize>();
        saved.clear();
        let result = import_stream(
            Cancelling {
                bytes: input.as_bytes(),
                flag: flag.clone(),
                boundary,
            },
            &flag,
            |_| {},
            |batch| {
                saved.push(batch.len());
                Ok(())
            },
        );
        assert!(matches!(result, Err(SourceError::Cancelled)));
        assert_eq!(saved, [1000]);
    }
    #[test]
    fn rejected_rows_still_report_progress_and_allow_cancel() {
        let input = rows(2500, false);
        let flag = CancelFlag::default();
        let mut reports = vec![];
        let result = import_stream(
            input.as_bytes(),
            &flag,
            |p| {
                reports.push(p);
                if p.scanned >= 1000 {
                    flag.cancel();
                }
            },
            |_| panic!("no accepted puzzles"),
        );
        assert!(matches!(result, Err(SourceError::Cancelled)));
        assert_eq!(reports.last().unwrap().rejected, 1000);
    }
    #[test]
    fn malformed_zstd_and_header_read_errors_propagate_instead_of_succeeding_empty() {
        let flag = CancelFlag::default();
        assert!(
            import_stream(
                &[0x28, 0xb5, 0x2f, 0xfd, 0, 1, 2][..],
                &flag,
                |_| {},
                |_| Ok(())
            )
            .is_err()
        );
        assert!(CsvPuzzles::new(Broken).next().unwrap().is_err());
    }
}

#[cfg(test)]
mod location_tests {
    use super::*;
    use std::{io::Write, net::TcpListener};
    #[test]
    fn streams_url_download_and_path_using_magic_instead_of_filename() {
        let fixture = include_bytes!("../../tests/fixtures/lichess.csv");
        let compressed = zstd::stream::encode_all(fixture.as_slice(), 1).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("without-an-extension");
        std::fs::write(&path, &compressed).unwrap();
        let mut from_file = Store::memory().unwrap();
        let file = import_location(
            &mut from_file,
            path.to_str().unwrap(),
            &CancelFlag::default(),
            |_| {},
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/puzzles", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /puzzles "));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                compressed.len()
            )
            .unwrap();
            // Small chunks also exercise the streaming HTTP body reader.
            for chunk in compressed.chunks(64) {
                stream.write_all(chunk).unwrap();
            }
        });
        let mut from_url = Store::memory().unwrap();
        let downloaded =
            import_location(&mut from_url, &url, &CancelFlag::default(), |_| {}).unwrap();
        server.join().unwrap();
        assert_eq!(downloaded, file);
        assert_eq!(
            from_file.puzzle_count().unwrap(),
            from_url.puzzle_count().unwrap()
        );
    }
}

#[cfg(test)]
mod limit_tests {
    use super::*;
    use std::fmt::Write;
    fn input() -> String {
        let p = crate::puzzle::fixture();
        let mut text =
            "PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes\n".to_owned();
        for i in 0..5000 {
            writeln!(
                text,
                "p{i},{},{},1000,75,{},1000,fork",
                p.fen,
                p.moves,
                if i % 2 == 0 { 0 } else { 100 }
            )
            .unwrap();
        }
        text
    }
    #[test]
    fn limit_counts_accepted_only_flushes_final_batch_and_stops_reading() {
        let input = input();
        for (limit, expected) in [(Some(7), 7), (Some(3000), 2500), (None, 2500), (Some(0), 0)] {
            let mut saved = 0;
            let mut reports = Vec::new();
            let result = import_stream_options(
                input.as_bytes(),
                ImportOptions { limit },
                &CancelFlag::default(),
                |p| reports.push(p),
                |batch| {
                    saved += batch.len();
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(result.accepted, expected);
            assert_eq!(saved as u64, expected);
            assert_eq!(reports.last(), Some(&result));
            assert_eq!(result.bands.iter().sum::<u64>(), expected);
            if limit == Some(7) {
                assert_eq!(result.scanned, 14);
                assert_eq!(result.rejected, 7);
                assert!(result.bytes < input.len() as u64);
            }
        }
    }
    #[test]
    fn cancelled_import_does_not_advance_library_markers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("puzzles.csv");
        std::fs::write(&path, input()).unwrap();
        let mut store = Store::memory().unwrap();
        store
            .record_library_import(LibrarySize::Starter, 123)
            .unwrap();
        let flag = CancelFlag::default();
        let result = import_location_options(
            &mut store,
            path.to_str().unwrap(),
            LibrarySize::Full,
            ImportOptions::default(),
            &flag,
            |p| {
                if p.accepted >= 1000 {
                    flag.cancel();
                }
            },
        );
        assert!(matches!(result, Err(SourceError::Cancelled)));
        assert!(store.puzzle_count().unwrap() >= 1000);
        assert_eq!(
            store.setting("library_size").unwrap().as_deref(),
            Some("starter")
        );
        assert_eq!(
            store.setting("library_imported_at").unwrap().as_deref(),
            Some("123")
        );
    }
}
