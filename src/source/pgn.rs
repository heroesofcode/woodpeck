use super::SourceError;
use crate::puzzle::Puzzle;
use shakmaty::{Color, EnPassantMode, Position, fen::Fen, san::SanPlus};
use std::fmt::Write;
/// Export from the player's initial position, after the opponent's setup, matching the sample PGN.
pub fn export_pgn(name: &str, puzzles: &[Puzzle]) -> Result<String, SourceError> {
    let mut output = String::new();
    for (i, p) in puzzles.iter().enumerate() {
        let line = p
            .validate()
            .map_err(|e| SourceError::Invalid(e.to_string()))?;
        let mut position = line.positions[1].clone();
        let tags = [
            ("Event", format!("{name} #{}", i + 1)),
            ("Site", p.lichess_url()),
            ("Date", "????.??.??".into()),
            ("Round", (i + 1).to_string()),
            ("White", "?".into()),
            ("Black", "?".into()),
            ("Result", "*".into()),
            ("SetUp", "1".into()),
            (
                "FEN",
                Fen::from_position(&position, EnPassantMode::Legal).to_string(),
            ),
            ("PuzzleId", p.id.clone()),
            ("PuzzleRating", p.rating.to_string()),
            (
                "Annotator",
                "Lichess puzzle database (CC0) via woodpeck".into(),
            ),
        ];
        for (key, value) in tags {
            writeln!(output, "[{key} \"{}\"]", escape(&value))
                .map_err(|e| SourceError::Invalid(e.to_string()))?;
        }
        output.push('\n');
        for (index, uci) in line.moves.iter().skip(1).enumerate() {
            if position.turn() == Color::White {
                write!(output, "{}.", position.fullmoves())
                    .map_err(|e| SourceError::Invalid(e.to_string()))?;
            } else if index == 0 {
                write!(output, "{}...", position.fullmoves())
                    .map_err(|e| SourceError::Invalid(e.to_string()))?;
            }
            let m = uci
                .to_move(&position)
                .map_err(|e| SourceError::Invalid(e.to_string()))?;
            write!(output, "{} ", SanPlus::from_move(position.clone(), m))
                .map_err(|e| SourceError::Invalid(e.to_string()))?;
            position.play_unchecked(m);
        }
        output.push_str("*\n\n");
    }
    Ok(output)
}
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_reference_fen_tags_and_san() {
        let p = crate::puzzle::fixture();
        let output = export_pgn("Forks", &[p]).unwrap();
        assert!(output.contains("[PuzzleId \"KEPe0\"]"));
        assert!(output.contains("[FEN \"5r1k/5Rb1/p2P2Pp/1p5n/2p5/P4B1P/1pP5/5R1K b - - 2 32\"]"));
        assert!(output.contains("32...Ng3+ 33.Kg2 Nxf1 *"));
        let reference = include_str!("../../tests/fixtures/reference.pgn");
        assert!(reference.contains("32...Ng3+ 33.Kg2 Nxf1 *"));
    }
    #[test]
    fn escape_headers_and_reject_invalid_puzzles() {
        let p = crate::puzzle::fixture();
        assert!(
            export_pgn("My \"set\"\nnext", std::slice::from_ref(&p))
                .unwrap()
                .contains("My \\\"set\\\" next")
        );
        let mut bad = p;
        bad.moves.clear();
        assert!(export_pgn("bad", &[bad]).is_err());
        assert_eq!(export_pgn("empty", &[]).unwrap(), "");
    }
}
