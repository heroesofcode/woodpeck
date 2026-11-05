//! Blind-line drill: watch a short sequence, then play the next move from memory.
use shakmaty::{Chess, Position, uci::UciMove};
use std::time::Duration;

const PREVIEW_STEP: Duration = Duration::from_millis(700);
const CONCEAL_FOR: Duration = Duration::from_millis(900);
const FIRST_DELAY: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TickEvent {
    Preview(String),
    ReadyToAnswer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Auto-playing `preview` on the board.
    Watching,
    /// Brief pause so the played line is no longer on screen.
    Concealed,
    /// Player must enter `answer`.
    Answering,
    Cleared,
    Missed,
}

#[derive(Clone, Copy, Debug)]
struct Puzzle {
    fen: &'static str,
    preview: &'static [&'static str],
    answer: &'static str,
}

#[derive(Clone, Debug)]
pub struct Round {
    pub fen: String,
    pub preview: Vec<String>,
    pub answer: String,
    pub phase: Phase,
    /// Index of the next preview ply to reveal while `Watching`.
    pub step: usize,
    pub deadline: Duration,
    pub last_move: Option<String>,
}

impl Round {
    fn from_puzzle(puzzle: Puzzle, now: Duration) -> Option<Self> {
        let mut position: Chess = puzzle
            .fen
            .parse::<shakmaty::fen::Fen>()
            .ok()?
            .into_position(shakmaty::CastlingMode::Chess960)
            .ok()?;
        for uci in puzzle.preview {
            let parsed = parse_uci(uci)?;
            let mv = parsed.to_move(&position).ok()?;
            position = position.play(mv).ok()?;
        }
        let parsed = parse_uci(puzzle.answer)?;
        parsed.to_move(&position).ok()?;
        Some(Self {
            fen: puzzle.fen.to_string(),
            preview: puzzle.preview.iter().map(|s| (*s).to_string()).collect(),
            answer: puzzle.answer.to_string(),
            phase: Phase::Watching,
            step: 0,
            deadline: now.saturating_add(FIRST_DELAY),
            last_move: None,
        })
    }

    pub fn start_position(&self) -> Chess {
        self.fen
            .parse::<shakmaty::fen::Fen>()
            .expect("validated fen")
            .into_position(shakmaty::CastlingMode::Chess960)
            .expect("legal fen")
    }

    pub fn position_after_preview(&self) -> Chess {
        let mut position = self.start_position();
        for uci in &self.preview {
            let mv = parse_uci(uci)
                .and_then(|u| u.to_move(&position).ok())
                .expect("validated preview");
            position = position.play(mv).expect("legal preview");
        }
        position
    }

    /// Advance timed phases. Emits preview UCIs, then `ReadyToAnswer` once.
    pub fn tick(&mut self, now: Duration) -> Option<TickEvent> {
        match self.phase {
            Phase::Watching if now >= self.deadline => {
                if self.step < self.preview.len() {
                    let uci = self.preview[self.step].clone();
                    self.step += 1;
                    self.deadline = now.saturating_add(PREVIEW_STEP);
                    Some(TickEvent::Preview(uci))
                } else {
                    self.phase = Phase::Concealed;
                    self.deadline = now.saturating_add(CONCEAL_FOR);
                    None
                }
            }
            Phase::Concealed if now >= self.deadline => {
                self.phase = Phase::Answering;
                Some(TickEvent::ReadyToAnswer)
            }
            _ => None,
        }
    }

    pub fn submit(&mut self, uci: &str) -> Phase {
        if self.phase != Phase::Answering {
            return self.phase;
        }
        self.last_move = Some(uci.to_owned());
        self.phase = if uci == self.answer {
            Phase::Cleared
        } else {
            Phase::Missed
        };
        self.phase
    }
}

fn parse_uci(uci: &str) -> Option<UciMove> {
    uci.parse().ok()
}

#[derive(Clone, Debug)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    #[allow(clippy::cast_possible_truncation)]
    fn gen_range(&mut self, max: usize) -> usize {
        if max == 0 {
            0
        } else {
            usize::try_from(self.next() % u64::try_from(max).unwrap_or(1)).unwrap_or(0)
        }
    }
}

/// Original short lines (no third-party puzzle text). Each preview is legal from `fen`;
/// `answer` is legal from the position after the preview.
const BANK: &[Puzzle] = &[
    Puzzle {
        // Opening fragment: 1.e4 e5 2.Nf3 — next is the natural Nc6.
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        preview: &["e2e4", "e7e5", "g1f3"],
        answer: "b8c6",
    },
    Puzzle {
        // Back-rank idea: rook checks, king steps, then the mate.
        fen: "6k1/5ppp/8/8/8/8/5PPP/4R1K1 w - - 0 1",
        preview: &["e1e7", "g8h8"],
        answer: "e7e8",
    },
    Puzzle {
        // Queen checks; remember to capture it.
        fen: "4k3/8/8/8/8/8/3Q4/4K3 w - - 0 1",
        preview: &["d2d8"],
        answer: "e8d8",
    },
    Puzzle {
        // Two-ply capture sequence ending with a recapture ask.
        fen: "4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1",
        preview: &["d5c6"],
        answer: "e8d8",
    },
    Puzzle {
        // Short castle then a quiet king move to remember.
        fen: "4k3/8/8/8/8/8/4P3/4K2R w K - 0 1",
        preview: &["e1g1"],
        answer: "e8e7",
    },
    Puzzle {
        // Knight tour preview; answer develops the other knight.
        fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        preview: &["e7e5", "g1f3", "b8c6"],
        answer: "f1c4",
    },
];

pub fn generate(seed: u64, now: Duration) -> Round {
    let mut rng = Rng::new(seed);
    for _ in 0..32 {
        let puzzle = BANK[rng.gen_range(BANK.len())];
        if let Some(round) = Round::from_puzzle(puzzle, now) {
            return round;
        }
    }
    Round::from_puzzle(BANK[0], now).expect("bank puzzle is valid")
}

#[derive(Clone, Debug)]
pub struct Session {
    pub round: Round,
    pub score: u32,
    pub streak: u32,
    pub best_streak: u32,
    pub seed: u64,
}

impl Session {
    pub fn start(seed: u64, now: Duration) -> Self {
        Self {
            round: generate(seed, now),
            score: 0,
            streak: 0,
            best_streak: 0,
            seed,
        }
    }

    pub fn tick(&mut self, now: Duration) -> Option<TickEvent> {
        self.round.tick(now)
    }

    pub fn submit(&mut self, uci: &str) -> Phase {
        let phase = self.round.submit(uci);
        match phase {
            Phase::Cleared => {
                self.score += 1;
                self.streak += 1;
                self.best_streak = self.best_streak.max(self.streak);
            }
            Phase::Missed => {
                self.streak = 0;
            }
            _ => {}
        }
        phase
    }

    pub fn next_round(&mut self, now: Duration) {
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        self.round = generate(self.seed, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_lines_are_legal() {
        for puzzle in BANK {
            let round = Round::from_puzzle(*puzzle, Duration::ZERO).expect("puzzle");
            assert_ne!(round.preview.len(), 0);
            let after = round.position_after_preview();
            let answer = parse_uci(&round.answer).unwrap();
            assert!(answer.to_move(&after).is_ok());
        }
    }

    #[test]
    fn watching_emits_preview_then_conceals() {
        let mut session = Session::start(1, Duration::ZERO);
        assert_eq!(session.round.phase, Phase::Watching);
        let mut now = Duration::ZERO;
        let mut seen = Vec::new();
        for _ in 0..16 {
            now += Duration::from_millis(500);
            if let Some(TickEvent::Preview(uci)) = session.tick(now) {
                seen.push(uci);
            }
            if session.round.phase == Phase::Concealed {
                break;
            }
        }
        assert_eq!(seen, session.round.preview);
        assert_eq!(session.round.phase, Phase::Concealed);
        now = session.round.deadline;
        assert_eq!(session.tick(now), Some(TickEvent::ReadyToAnswer));
        assert_eq!(session.round.phase, Phase::Answering);
    }

    #[test]
    fn correct_answer_scores_and_wrong_breaks_streak() {
        let mut session = Session::start(2, Duration::ZERO);
        // Skip to answering.
        session.round.phase = Phase::Answering;
        let answer = session.round.answer.clone();
        assert_eq!(session.submit(&answer), Phase::Cleared);
        assert_eq!(session.score, 1);
        assert_eq!(session.streak, 1);
        session.next_round(Duration::from_secs(1));
        session.round.phase = Phase::Answering;
        assert_eq!(session.submit("a2a3"), Phase::Missed);
        assert_eq!(session.streak, 0);
    }
}
