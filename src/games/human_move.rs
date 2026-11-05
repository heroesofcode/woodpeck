//! Human-move drill: pick the natural popular move, not a deep engine line.
use shakmaty::{Chess, Position, uci::UciMove};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Cleared,
    Missed,
}

#[derive(Clone, Copy, Debug)]
struct Puzzle {
    fen: &'static str,
    /// Acceptable "human" replies (popular / natural). Exact UCI match.
    answers: &'static [&'static str],
}

#[derive(Clone, Debug)]
pub struct Round {
    pub fen: String,
    pub answers: BTreeSet<String>,
    pub status: Status,
    pub last_move: Option<String>,
}

impl Round {
    fn from_puzzle(puzzle: Puzzle) -> Option<Self> {
        let position: Chess = puzzle
            .fen
            .parse::<shakmaty::fen::Fen>()
            .ok()?
            .into_position(shakmaty::CastlingMode::Chess960)
            .ok()?;
        let mut answers = BTreeSet::new();
        for uci in puzzle.answers {
            let parsed: UciMove = uci.parse().ok()?;
            parsed.to_move(&position).ok()?;
            answers.insert((*uci).to_string());
        }
        if answers.is_empty() {
            return None;
        }
        Some(Self {
            fen: puzzle.fen.to_string(),
            answers,
            status: Status::Playing,
            last_move: None,
        })
    }

    pub fn position(&self) -> Chess {
        self.fen
            .parse::<shakmaty::fen::Fen>()
            .expect("validated fen")
            .into_position(shakmaty::CastlingMode::Chess960)
            .expect("legal fen")
    }

    pub fn submit(&mut self, uci: &str) -> Status {
        if self.status != Status::Playing {
            return self.status;
        }
        self.last_move = Some(uci.to_owned());
        let legal =
            self.position().legal_moves().iter().any(|m| {
                UciMove::from_move(*m, shakmaty::CastlingMode::Standard).to_string() == uci
            });
        if !legal {
            return self.status;
        }
        self.status = if self.answers.contains(uci) {
            Status::Cleared
        } else {
            Status::Missed
        };
        self.status
    }
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

/// Original curated "what would a human play" positions — not engine-best lists.
const BANK: &[Puzzle] = &[
    Puzzle {
        // Opening: after 1.e4, the natural reply.
        fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        answers: &["e7e5", "c7c5"],
    },
    Puzzle {
        // After 1.e4 e5 2.Nf3, develop the knight.
        fen: "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
        answers: &["b8c6", "g8f6"],
    },
    Puzzle {
        // Recapture the hanging knight with the pawn — natural.
        fen: "4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1",
        answers: &["d5c6"],
    },
    Puzzle {
        // Castle when it is obviously safe and natural.
        fen: "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
        answers: &["e1g1"],
    },
    Puzzle {
        // Take the free queen — humans do not leave it.
        fen: "4k3/8/8/8/8/8/3q4/4K3 w - - 0 1",
        answers: &["e1d2"],
    },
    Puzzle {
        // Develop bishop in Italian-ish setup.
        fen: "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3",
        answers: &["f8c5", "g8f6"],
    },
];

pub fn generate(seed: u64) -> Round {
    let mut rng = Rng::new(seed);
    for _ in 0..32 {
        let puzzle = BANK[rng.gen_range(BANK.len())];
        if let Some(round) = Round::from_puzzle(puzzle) {
            return round;
        }
    }
    Round::from_puzzle(BANK[0]).expect("bank puzzle is valid")
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
    pub fn start(seed: u64) -> Self {
        Self {
            round: generate(seed),
            score: 0,
            streak: 0,
            best_streak: 0,
            seed,
        }
    }
    pub fn submit(&mut self, uci: &str) -> Status {
        let status = self.round.submit(uci);
        match status {
            Status::Cleared => {
                self.score += 1;
                self.streak += 1;
                self.best_streak = self.best_streak.max(self.streak);
            }
            Status::Missed => self.streak = 0,
            Status::Playing => {}
        }
        status
    }
    pub fn next_round(&mut self) {
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        self.round = generate(self.seed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bank_answers_are_legal() {
        for puzzle in BANK {
            Round::from_puzzle(*puzzle).expect("puzzle");
        }
    }
    #[test]
    fn popular_reply_scores_and_other_legal_misses() {
        let mut session = Session::start(1);
        // Force known puzzle: opening after e4
        session.round = Round::from_puzzle(BANK[0]).unwrap();
        assert_eq!(session.submit("e7e5"), Status::Cleared);
        assert_eq!(session.score, 1);
        session.next_round();
        session.round = Round::from_puzzle(BANK[0]).unwrap();
        // a6 is legal but not popular here
        assert_eq!(session.submit("a7a6"), Status::Missed);
        assert_eq!(session.streak, 0);
    }
}
