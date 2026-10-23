use super::SourceError;
use crate::puzzle::Puzzle;
use serde::Deserialize;
use std::io::Read;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Row {
    puzzle_id: String,
    #[serde(rename = "FEN")]
    fen: String,
    moves: String,
    rating: u16,
    rating_deviation: u16,
    popularity: i16,
    nb_plays: u64,
    themes: String,
}
impl From<Row> for Puzzle {
    fn from(row: Row) -> Self {
        Self {
            id: row.puzzle_id,
            fen: row.fen,
            moves: row.moves,
            rating: row.rating,
            rd: row.rating_deviation,
            popularity: row.popularity,
            nb_plays: row.nb_plays,
            themes: row.themes.split_whitespace().map(str::to_owned).collect(),
        }
    }
}
pub fn passes_quality(p: &Puzzle) -> bool {
    p.popularity >= 80
        && p.rd <= 100
        && p.nb_plays >= 500
        && !p.themes.iter().any(|t| t == "veryLong")
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportProgress {
    pub scanned: u64,
    pub accepted: u64,
    pub rejected: u64,
}
/// Lazy iterator: supports streaming uncompressed CSV or a zstd decoder without buffering the DB.
pub struct CsvPuzzles<R: Read> {
    rows: ::csv::DeserializeRecordsIntoIter<R, Row>,
    progress: ImportProgress,
}
impl<R: Read> CsvPuzzles<R> {
    pub fn new(reader: R) -> Self {
        Self {
            rows: ::csv::Reader::from_reader(reader).into_deserialize(),
            progress: ImportProgress::default(),
        }
    }
    pub fn progress(&self) -> ImportProgress {
        self.progress
    }
}
impl<R: Read> Iterator for CsvPuzzles<R> {
    type Item = Result<Puzzle, SourceError>;
    fn next(&mut self) -> Option<Self::Item> {
        for row in self.rows.by_ref() {
            self.progress.scanned += 1;
            let puzzle = match row {
                Ok(row) => Puzzle::from(row),
                Err(e) => return Some(Err(e.into())),
            };
            if passes_quality(&puzzle) && puzzle.validate().is_ok() {
                self.progress.accepted += 1;
                return Some(Ok(puzzle));
            }
            self.progress.rejected += 1;
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;
    fn csv() -> String {
        let p = crate::puzzle::fixture();
        let mut out="PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags\n".to_owned();
        for (i, (rd, pop, plays, theme)) in [
            (100, 80, 500, "fork"),
            (101, 100, 1000, "fork"),
            (90, 79, 1000, "fork"),
            (90, 100, 499, "fork"),
            (90, 100, 1000, "fork veryLong"),
        ]
        .iter()
        .enumerate()
        {
            writeln!(
                out,
                "p{i},{},{},1000,{rd},{pop},{plays},{theme},,",
                p.fen, p.moves
            )
            .unwrap();
        }
        out
    }
    #[test]
    fn streaming_quality_boundaries_and_progress() {
        let input = csv();
        let mut rows = CsvPuzzles::new(input.as_bytes());
        let accepted = rows.by_ref().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(accepted.len(), 1);
        assert_eq!(
            rows.progress(),
            ImportProgress {
                scanned: 5,
                accepted: 1,
                rejected: 4
            }
        );
    }
    #[test]
    fn reads_compressed_stream() {
        let input = csv();
        let compressed = zstd::stream::encode_all(input.as_bytes(), 1).unwrap();
        let reader = zstd::stream::read::Decoder::new(compressed.as_slice()).unwrap();
        assert_eq!(CsvPuzzles::new(reader).count(), 1);
    }
    #[test]
    fn malformed_csv_reports_error() {
        assert!(
            CsvPuzzles::new("PuzzleId,FEN\nx,bad".as_bytes())
                .next()
                .unwrap()
                .is_err()
        );
    }
    #[test]
    fn invalid_chess_is_filtered() {
        let input = csv().replace(&crate::puzzle::fixture().fen, "invalid");
        assert_eq!(CsvPuzzles::new(input.as_bytes()).count(), 0);
    }
}
