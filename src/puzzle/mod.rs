//! Validated chess positions and the time-driven training state machine.
mod engine;
pub use engine::{Engine, InputError, State, Walkthrough};

use serde::{Deserialize, Serialize};
use shakmaty::{CastlingMode, Chess, Position, fen::Fen, uci::UciMove};
use thiserror::Error;

/// A complete source row. Sets retain their own immutable copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Puzzle {
    pub id: String,
    pub fen: String,
    pub moves: String,
    pub rating: u16,
    pub rd: u16,
    pub popularity: i16,
    pub nb_plays: u64,
    pub themes: Vec<String>,
}

impl Puzzle {
    pub fn lichess_url(&self) -> String {
        format!("https://lichess.org/training/{}", self.id)
    }

    pub fn validate(&self) -> Result<ValidatedPuzzle, ValidationError> {
        let position: Chess = self
            .fen
            .parse::<Fen>()
            .map_err(|_| ValidationError::InvalidFen)?
            .into_position(CastlingMode::Standard)
            .map_err(|_| ValidationError::InvalidFen)?;
        let mut positions = vec![position.clone()];
        let mut current = position;
        let mut moves = Vec::new();
        for token in self.moves.split_whitespace() {
            let uci = parse_uci(token).map_err(|()| ValidationError::InvalidSolutionMove)?;
            let m = uci
                .to_move(&current)
                .map_err(|_| ValidationError::IllegalSolutionMove)?;
            current.play_unchecked(m);
            positions.push(current.clone());
            moves.push(uci);
        }
        // Lichess lines start with the opponent's setup move, before the player acts.
        if moves.len() < 2 {
            return Err(ValidationError::EmptySolution);
        }
        Ok(ValidatedPuzzle { positions, moves })
    }
}

pub(crate) fn parse_uci(token: &str) -> Result<UciMove, ()> {
    let bytes = token.as_bytes();
    if !(bytes.len() == 4 || bytes.len() == 5)
        || !matches!(bytes[0], b'a'..=b'h')
        || !matches!(bytes[1], b'1'..=b'8')
        || !matches!(bytes[2], b'a'..=b'h')
        || !matches!(bytes[3], b'1'..=b'8')
        || (bytes.len() == 5 && !b"qrbn".contains(&bytes[4]))
    {
        return Err(());
    }
    token.parse().map_err(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("The puzzle position could not be loaded.")]
    InvalidFen,
    #[error("The puzzle does not contain a player solution.")]
    EmptySolution,
    #[error("The puzzle contains an invalid solution move.")]
    InvalidSolutionMove,
    #[error("The puzzle contains an illegal solution move.")]
    IllegalSolutionMove,
}
impl ValidationError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidFen => "invalid_fen",
            Self::EmptySolution => "empty_solution",
            Self::InvalidSolutionMove => "invalid_solution_move",
            Self::IllegalSolutionMove => "illegal_solution_move",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedPuzzle {
    pub positions: Vec<Chess>,
    pub moves: Vec<UciMove>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Correct,
    Incorrect,
    Skipped,
    Invalid,
}
impl Outcome {
    pub fn is_miss(self) -> bool {
        matches!(self, Self::Incorrect | Self::Skipped)
    }
}

#[cfg(test)]
pub(crate) fn fixture() -> Puzzle {
    Puzzle {
        id: "KEPe0".into(),
        fen: "5r1k/1R4b1/p2P2Pp/1p5n/2p5/P4B1P/1pP5/5R1K w - - 1 32".into(),
        moves: "b7f7 h5g3 h1g2 g3f1".into(),
        rating: 1020,
        rd: 78,
        popularity: 100,
        nb_plays: 21735,
        themes: vec!["fork".into(), "endgame".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_entire_line() {
        assert_eq!(fixture().validate().unwrap().positions.len(), 5);
    }
    #[test]
    fn requires_a_player_move_after_the_opponent_setup() {
        let mut p = fixture();
        for moves in ["", " \t ", "b7f7", " b7f7 \t"] {
            p.moves = moves.into();
            let error = p.validate().unwrap_err();
            assert_eq!(error, ValidationError::EmptySolution);
            assert_eq!(error.code(), "empty_solution");
        }
        p.moves = "b7f7 h5g3".into();
        assert_eq!(p.validate().unwrap().moves.len(), 2);
    }
    #[test]
    fn validation_codes_and_messages() {
        let mut p = fixture();
        p.fen = "broken".into();
        assert_eq!(p.validate().unwrap_err(), ValidationError::InvalidFen);
        p = fixture();
        p.moves.clear();
        assert_eq!(p.validate().unwrap_err().code(), "empty_solution");
        p.moves = "0000".into();
        assert_eq!(
            p.validate().unwrap_err(),
            ValidationError::InvalidSolutionMove
        );
        p.moves = "b7f7 h5g3 h1h8".into();
        assert_eq!(
            p.validate().unwrap_err(),
            ValidationError::IllegalSolutionMove
        );
        for token in ["a7a8k", "a7a8Q", "e2e4junk", "éé"] {
            assert!(parse_uci(token).is_err());
        }
    }
}
