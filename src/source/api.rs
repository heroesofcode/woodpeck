use super::SourceError;
use crate::puzzle::Puzzle;
use pgn_reader::{Reader, SanPlus, Visitor};
use serde::Deserialize;
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen};
use std::{io::Cursor, ops::ControlFlow};
#[derive(Deserialize)]
struct Batch {
    puzzles: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    game: Game,
    puzzle: ApiPuzzle,
}
#[derive(Deserialize)]
struct Game {
    pgn: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPuzzle {
    id: String,
    rating: u16,
    plays: u64,
    solution: Vec<String>,
    themes: Vec<String>,
    initial_ply: usize,
}
struct Mainline;
impl Visitor for Mainline {
    type Tags = ();
    type Movetext = Vec<SanPlus>;
    type Output = Vec<SanPlus>;
    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(())
    }
    fn begin_movetext(&mut self, (): ()) -> ControlFlow<Self::Output, Self::Movetext> {
        ControlFlow::Continue(Vec::new())
    }
    fn san(&mut self, moves: &mut Self::Movetext, san: SanPlus) -> ControlFlow<Self::Output> {
        moves.push(san);
        ControlFlow::Continue(())
    }
    fn end_game(&mut self, moves: Self::Movetext) -> Self::Output {
        moves
    }
}
/// Lichess initialPly is the zero-based index of the opponent setup move in game.pgn.
/// Save the FEN immediately *before* it and prepend that move to the player's solution.
pub fn parse_batch(json: &str) -> Result<Vec<Puzzle>, SourceError> {
    let batch: Batch = serde_json::from_str(json)?;
    batch.puzzles.into_iter().map(reconstruct).collect()
}
pub fn parse_puzzle(json: &str) -> Result<Puzzle, SourceError> {
    reconstruct(serde_json::from_str(json)?)
}
fn reconstruct(entry: Entry) -> Result<Puzzle, SourceError> {
    let p = entry.puzzle;
    let moves = Reader::new(Cursor::new(entry.game.pgn))
        .read_game(&mut Mainline)?
        .ok_or_else(|| SourceError::Invalid("Missing PGN game.".into()))?;
    let mut position = Chess::default();
    let mut before = None;
    let mut setup_move = None;
    if p.initial_ply >= moves.len() {
        return Err(SourceError::Invalid(
            "initialPly exceeds the PGN mainline.".into(),
        ));
    }
    for san in moves.into_iter().take(p.initial_ply + 1) {
        let m = san
            .san
            .to_move(&position)
            .map_err(|e| SourceError::Invalid(e.to_string()))?;
        before = Some(Fen::from_position(&position, EnPassantMode::Legal).to_string());
        setup_move = Some(m.to_uci(CastlingMode::Standard).to_string());
        position.play_unchecked(m);
    }
    if p.solution.is_empty() {
        return Err(SourceError::Invalid("Missing player solution.".into()));
    }
    let mut line =
        vec![setup_move.ok_or_else(|| SourceError::Invalid("Missing opponent move.".into()))?];
    line.extend(p.solution);
    let puzzle = Puzzle {
        id: p.id,
        fen: before.ok_or_else(|| SourceError::Invalid("Missing initial position.".into()))?,
        moves: line.join(" "),
        rating: p.rating,
        rd: 0,
        popularity: 100,
        nb_plays: p.plays,
        themes: p.themes,
    };
    puzzle
        .validate()
        .map_err(|e| SourceError::Invalid(e.to_string()))?;
    Ok(puzzle)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_fixture_reconstructs_five_legal_puzzles_with_setup() {
        let puzzles = parse_batch(include_str!("../../tests/fixtures/batch.json")).unwrap();
        assert_eq!(puzzles.len(), 5);
        let first = &puzzles[0];
        assert_eq!(first.id, "JkPgB");
        assert_eq!(first.moves, "h1g2 e8c6 g2g3 c6a8");
        assert_eq!(
            first.validate().unwrap().positions[1].turn(),
            shakmaty::Color::Black
        );
        for p in puzzles {
            assert_eq!(p.moves.split_whitespace().count(), 4);
            assert!(p.validate().is_ok());
            assert!(p.themes.contains(&"fork".into()));
        }
    }
    #[test]
    fn mainline_ignores_comments_variations_and_move_numbers() {
        let json = r#"{"puzzles":[{"game":{"pgn":"1. e4 {hi} (1. d4 d5) e5 2. Nf3 *"},"puzzle":{"id":"x","rating":1000,"plays":10,"solution":["b8c6"],"themes":[],"initialPly":2}}]}"#;
        let p = parse_batch(json).unwrap().remove(0);
        assert_eq!(p.moves, "g1f3 b8c6");
        assert!(p.fen.contains(" w "));
    }
    #[test]
    fn initial_ply_zero_and_last_ply_reconstruct_before_opponent_move() {
        for (initial_ply, solution, setup, expected_fen) in [
            (
                0,
                "e7e5",
                "e2e4",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            ),
            (
                2,
                "b8c6",
                "g1f3",
                "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
            ),
        ] {
            let json = serde_json::json!({"game": {"pgn": "1. e4 e5 2. Nf3"}, "puzzle": {
                "id": "boundary", "rating": 1000, "plays": 1000, "themes": [],
                "initialPly": initial_ply, "solution": [solution]
            }});
            let puzzle = parse_puzzle(&json.to_string()).unwrap();
            assert_eq!(puzzle.fen, expected_fen);
            assert_eq!(puzzle.moves, format!("{setup} {solution}"));
        }
    }
    #[test]
    fn rejects_missing_solution_out_of_bounds_and_illegal_pgn() {
        let base = include_str!("../../tests/fixtures/batch.json");
        let mut value: serde_json::Value = serde_json::from_str(base).unwrap();
        value["puzzles"][0]["puzzle"]["initialPly"] = 999.into();
        assert!(parse_batch(&value.to_string()).is_err());
        value["puzzles"][0]["puzzle"]["initialPly"] = 0.into();
        value["puzzles"][0]["game"]["pgn"] = "Qh8".into();
        assert!(parse_batch(&value.to_string()).is_err());
        assert!(parse_batch("{}").is_err());
    }
}
