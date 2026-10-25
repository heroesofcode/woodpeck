use super::{Outcome, Puzzle, ValidatedPuzzle, ValidationError, parse_uci};
use shakmaty::{Chess, Color, Position};
use std::time::Duration;
use thiserror::Error;

pub const OPPONENT_DELAY: Duration = Duration::from_millis(80);
pub const ANIMATION: Duration = Duration::from_millis(140);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Loading,
    OpponentTurn,
    PlayerTurn,
    Complete,
    Incorrect,
    FailedReview,
    Skipped,
    Invalid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum InputError {
    #[error("The board is not interactive.")]
    NotInteractive,
    #[error("This is not a legal move.")]
    IllegalMove,
}

/// Positions start immediately before the missed move, excluding the wrong move.
#[derive(Clone, Debug)]
pub struct Walkthrough {
    positions: Vec<Chess>,
    cursor: usize,
}
impl Walkthrough {
    pub fn position(&self) -> &Chess {
        &self.positions[self.cursor]
    }
    pub fn progress(&self) -> (usize, usize) {
        (self.cursor, self.positions.len() - 1)
    }
    pub fn is_complete(&self) -> bool {
        self.cursor + 1 == self.positions.len()
    }
    pub fn next(&mut self) {
        self.cursor = (self.cursor + 1).min(self.positions.len() - 1);
    }
    pub fn previous(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
}

/// The caller supplies monotonically increasing time, including for animation updates.
#[derive(Clone, Debug)]
pub struct Engine {
    state: State,
    line: Option<ValidatedPuzzle>,
    position: Option<Chess>,
    index: usize,
    deadline: Duration,
    opponent_applied: bool,
    error: Option<ValidationError>,
    walkthrough: Option<Walkthrough>,
    moves_played: Vec<String>,
    allow_any_final_checkmate: bool,
}
impl Engine {
    pub fn new(puzzle: &Puzzle, now: Duration, allow_any_final_checkmate: bool) -> Self {
        let (line, error) = match puzzle.validate() {
            Ok(line) => (Some(line), None),
            Err(e) => (None, Some(e)),
        };
        Self {
            state: State::Loading,
            position: line.as_ref().map(|l| l.positions[0].clone()),
            line,
            index: 0,
            deadline: now,
            opponent_applied: false,
            error,
            walkthrough: None,
            moves_played: Vec::new(),
            allow_any_final_checkmate,
        }
    }
    /// Rebase a prepared, still-loading engine when it becomes visible.
    pub(crate) fn start_at(&mut self, now: Duration) {
        debug_assert_eq!(self.state, State::Loading);
        self.deadline = now;
    }
    /// Schedule state changes without polling at a fixed frame rate.
    pub fn next_deadline(&self) -> Option<Duration> {
        matches!(
            self.state,
            State::Loading | State::OpponentTurn | State::Incorrect
        )
        .then_some(self.deadline)
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn error(&self) -> Option<ValidationError> {
        self.error
    }
    pub fn position(&self) -> Option<&Chess> {
        self.walkthrough
            .as_ref()
            .filter(|_| self.state == State::FailedReview)
            .map(Walkthrough::position)
            .or(self.position.as_ref())
    }
    pub fn orientation(&self) -> Option<Color> {
        self.line.as_ref().map(|l| !l.positions[0].turn())
    }
    pub fn moves_played(&self) -> &[String] {
        &self.moves_played
    }
    pub fn walkthrough(&mut self) -> Option<&mut Walkthrough> {
        self.walkthrough.as_mut()
    }
    pub fn outcome(&self) -> Option<Outcome> {
        match self.state {
            State::Complete => Some(Outcome::Correct),
            State::Incorrect | State::FailedReview => Some(Outcome::Incorrect),
            State::Skipped => Some(Outcome::Skipped),
            State::Invalid => Some(Outcome::Invalid),
            _ => None,
        }
    }
    pub fn update(&mut self, now: Duration) {
        if self.state == State::Loading {
            if self.error.is_some() {
                self.state = State::Invalid;
                return;
            }
            self.state = State::OpponentTurn;
            self.deadline += OPPONENT_DELAY;
        }
        if self.state == State::Incorrect && now >= self.deadline {
            self.state = State::FailedReview;
        }
        if self.state != State::OpponentTurn || now < self.deadline {
            return;
        }
        if !self.opponent_applied {
            if let Some(line) = &self.line {
                self.index += 1;
                self.position = Some(line.positions[self.index].clone());
            }
            self.opponent_applied = true;
            self.deadline += ANIMATION;
        }
        if now >= self.deadline {
            self.state = if self
                .line
                .as_ref()
                .is_some_and(|l| self.index == l.moves.len())
                && !self.moves_played.is_empty()
            {
                State::Complete
            } else {
                State::PlayerTurn
            };
        }
    }
    pub fn play(&mut self, input: &str, now: Duration) -> Result<(), InputError> {
        if self.state != State::PlayerTurn {
            return Err(InputError::NotInteractive);
        }
        let position = self.position.as_ref().ok_or(InputError::NotInteractive)?;
        let line = self.line.as_ref().ok_or(InputError::NotInteractive)?;
        let uci = parse_uci(input).map_err(|()| InputError::IllegalMove)?;
        let m = uci.to_move(position).map_err(|_| InputError::IllegalMove)?;
        let mut after = position.clone();
        after.play_unchecked(m);
        let correct = uci == line.moves[self.index]
            || (self.allow_any_final_checkmate
                && self.index + 1 == line.moves.len()
                && line.positions[self.index + 1].is_checkmate()
                && after.is_checkmate());
        self.moves_played.push(input.to_owned());
        self.position = Some(after);
        if correct {
            self.index += 1;
            if self.index == line.moves.len() {
                self.state = State::Complete;
            } else {
                self.state = State::OpponentTurn;
                self.opponent_applied = false;
                self.deadline = now + OPPONENT_DELAY;
            }
        } else {
            self.walkthrough = Some(Walkthrough {
                positions: line.positions[self.index..].to_vec(),
                cursor: 0,
            });
            self.state = State::Incorrect;
            self.deadline = now + ANIMATION;
        }
        Ok(())
    }
    pub fn skip(&mut self) -> Result<(), InputError> {
        if self.state != State::PlayerTurn {
            return Err(InputError::NotInteractive);
        }
        self.state = State::Skipped;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzle::fixture;
    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }
    #[test]
    fn timings_orientation_and_complete() {
        let mut e = Engine::new(&fixture(), at(0), true);
        assert_eq!(e.state(), State::Loading);
        assert_eq!(e.orientation(), Some(Color::Black));
        assert!(e.skip().is_err());
        e.update(at(0));
        let initial = e.position().cloned();
        e.update(at(79));
        assert_eq!(e.position(), initial.as_ref());
        e.update(at(80));
        assert_ne!(e.position(), initial.as_ref());
        assert_eq!(e.state(), State::OpponentTurn);
        e.update(at(219));
        assert_eq!(e.state(), State::OpponentTurn);
        e.update(at(220));
        assert_eq!(e.state(), State::PlayerTurn);
        e.play("h5g3", at(300)).unwrap();
        e.update(at(520));
        assert_eq!(e.state(), State::PlayerTurn);
        e.play("g3f1", at(600)).unwrap();
        assert_eq!(e.outcome(), Some(Outcome::Correct));
        assert!(e.play("a1a2", at(700)).is_err());
    }
    #[test]
    fn wrong_move_ends_attempt_and_walks_correct_line() {
        let mut e = Engine::new(&fixture(), at(0), false);
        e.update(at(220));
        let before = e.position().cloned();
        e.play("h5f6", at(300)).unwrap();
        assert_eq!(e.outcome(), Some(Outcome::Incorrect));
        assert_eq!(e.moves_played(), &["h5f6"]);
        e.update(at(439));
        assert_eq!(e.state(), State::Incorrect);
        e.update(at(440));
        assert_eq!(e.state(), State::FailedReview);
        assert_eq!(e.position(), before.as_ref());
        let w = e.walkthrough().unwrap();
        w.previous();
        assert_eq!(w.progress(), (0, 3));
        for _ in 0..5 {
            w.next();
        }
        assert!(w.is_complete());
        assert_eq!(w.progress(), (3, 3));
        w.previous();
        assert!(!w.is_complete());
    }
    #[test]
    fn skip_only_player_turn_and_illegal_input_does_not_end_attempt() {
        let mut e = Engine::new(&fixture(), at(0), false);
        e.update(at(220));
        assert!(e.play("h5h1", at(230)).is_err());
        assert_eq!(e.state(), State::PlayerTurn);
        e.skip().unwrap();
        assert_eq!(e.outcome(), Some(Outcome::Skipped));
        assert!(e.skip().is_err());
    }
    #[test]
    fn invalid_is_safe() {
        let mut p = fixture();
        p.fen.clear();
        let mut e = Engine::new(&p, at(0), false);
        e.update(at(500));
        assert_eq!(e.state(), State::Invalid);
        assert_eq!(e.orientation(), None);
        assert_eq!(e.outcome(), Some(Outcome::Invalid));
    }
    #[test]
    fn setup_only_line_is_invalid_and_never_completes() {
        let mut p = fixture();
        p.moves = "b7f7".into();
        for allow_any_final_checkmate in [false, true] {
            let mut e = Engine::new(&p, at(0), allow_any_final_checkmate);
            for ms in [0, 80, 220, 1000] {
                e.update(at(ms));
                assert_eq!(e.state(), State::Invalid);
                assert_eq!(e.outcome(), Some(Outcome::Invalid));
                assert_eq!(e.error(), Some(ValidationError::EmptySolution));
                assert_eq!(e.moves_played(), &[] as &[String]);
                assert_eq!(e.play("h5g3", at(ms)), Err(InputError::NotInteractive));
            }
        }
    }
    #[test]
    fn opponent_completion_requires_a_player_move_even_if_validation_is_bypassed() {
        let mut e = Engine::new(&fixture(), at(0), false);
        e.line.as_mut().unwrap().moves.truncate(1);
        e.line.as_mut().unwrap().positions.truncate(2);
        e.update(at(1000));
        assert_ne!(e.state(), State::Complete);
        assert_eq!(e.outcome(), None);
        assert_eq!(e.moves_played(), &[] as &[String]);
    }
    #[test]
    fn line_ending_with_opponent_move_completes_after_correct_player_input() {
        let mut p = fixture();
        p.moves = "b7f7 h5g3 h1g2".into();
        let mut e = Engine::new(&p, at(0), false);
        e.update(at(220));
        assert_eq!(e.state(), State::PlayerTurn);
        assert_eq!(e.outcome(), None);
        e.play("h5g3", at(300)).unwrap();
        e.update(at(520));
        assert_eq!(e.state(), State::Complete);
        assert_eq!(e.outcome(), Some(Outcome::Correct));
        assert_eq!(e.moves_played(), &["h5g3"]);
    }
    #[test]
    fn promotion_matches_piece() {
        let mut p = fixture();
        p.fen = "7k/P7/8/8/8/8/8/7K b - - 0 1".into();
        p.moves = "h8g7 a7a8n".into();
        let mut e = Engine::new(&p, at(0), false);
        e.update(at(220));
        assert_eq!(e.orientation(), Some(Color::White));
        assert!(e.play("a7a8", at(250)).is_err());
        e.play("a7a8n", at(300)).unwrap();
        assert_eq!(e.state(), State::Complete);
        let mut e = Engine::new(&p, at(0), false);
        e.update(at(220));
        e.play("a7a8q", at(300)).unwrap();
        assert_eq!(e.state(), State::Incorrect);
    }
    #[test]
    fn alternative_final_mate_requires_flag_and_expected_mate() {
        let mut p = fixture();
        p.fen = "7k/p4K2/6Q1/8/8/8/8/8 b - - 0 1".into();
        p.moves = "a7a6 g6g7".into();
        for (flag, state) in [(true, State::Complete), (false, State::Incorrect)] {
            let mut e = Engine::new(&p, at(0), flag);
            e.update(at(220));
            e.play("g6h6", at(300)).unwrap();
            assert_eq!(e.state(), state);
        }
        p.moves = "a7a6 g6g5".into();
        let mut e = Engine::new(&p, at(0), true);
        e.update(at(220));
        e.play("g6h6", at(300)).unwrap();
        assert_eq!(e.state(), State::Incorrect);
    }
}

#[cfg(test)]
mod special_move_tests {
    use super::*;
    use crate::puzzle::fixture;
    #[test]
    fn castling_for_both_sides_uses_standard_uci() {
        let mut p = fixture();
        p.fen = "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1".into();
        p.moves = "e8g8 e1c1".into();
        let mut engine = Engine::new(&p, Duration::ZERO, false);
        engine.update(Duration::from_millis(220));
        engine.play("e1c1", Duration::from_secs(1)).unwrap();
        assert_eq!(engine.state(), State::Complete);
        let board = engine.position().unwrap().board();
        assert_eq!(board.king_of(Color::White), Some(shakmaty::Square::C1));
        assert_eq!(
            board.piece_at(shakmaty::Square::D1).unwrap().role,
            shakmaty::Role::Rook
        );
    }
    #[test]
    fn en_passant_removes_the_captured_pawn() {
        let mut p = fixture();
        p.fen = "7k/8/8/8/3p4/8/4P3/7K w - - 0 1".into();
        p.moves = "e2e4 d4e3".into();
        let mut engine = Engine::new(&p, Duration::ZERO, false);
        engine.update(Duration::from_millis(220));
        engine.play("d4e3", Duration::from_secs(1)).unwrap();
        assert_eq!(engine.state(), State::Complete);
        let board = engine.position().unwrap().board();
        assert_eq!(board.piece_at(shakmaty::Square::E4), None);
        assert_eq!(
            board.piece_at(shakmaty::Square::E3).unwrap().color,
            Color::Black
        );
    }
    #[test]
    fn nonzero_clock_origin_and_large_update_preserve_deadlines() {
        let mut engine = Engine::new(&fixture(), Duration::from_secs(100), false);
        engine.update(Duration::from_millis(100_079));
        assert_eq!(engine.state(), State::OpponentTurn);
        engine.update(Duration::from_millis(100_220));
        assert_eq!(engine.state(), State::PlayerTurn);
        engine.play("h5g3", Duration::from_secs(101)).unwrap();
        engine.update(Duration::from_secs(200));
        assert_eq!(engine.state(), State::PlayerTurn);
    }
}
