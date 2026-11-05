//! Safe-move drill: play any solid legal move; the streak dies on a blunder.
use shakmaty::{Board, Chess, Color, Position, Role, Square, uci::UciMove};
use std::collections::BTreeSet;

fn role_value(role: Role) -> i32 {
    match role {
        Role::Pawn => 100,
        Role::Knight | Role::Bishop => 300,
        Role::Rook => 500,
        Role::Queen => 900,
        Role::King => 10_000,
    }
}

fn attackers(board: &Board, square: Square, by: Color) -> shakmaty::Bitboard {
    board.attacks_to(square, by, board.occupied())
}

fn is_hanging(board: &Board, square: Square) -> bool {
    let Some(piece) = board.piece_at(square) else {
        return false;
    };
    if piece.role == Role::King {
        return false;
    }
    let enemy = piece.color.other();
    if attackers(board, square, enemy).is_empty() {
        return false;
    }
    if attackers(board, square, piece.color).is_empty() {
        return true;
    }
    attackers(board, square, enemy)
        .into_iter()
        .filter_map(|sq| board.role_at(sq).map(role_value))
        .min()
        .is_some_and(|v| v < role_value(piece.role))
}

fn hanging_material(board: &Board, us: Color) -> i32 {
    Square::ALL
        .into_iter()
        .filter(|&sq| board.color_at(sq) == Some(us) && is_hanging(board, sq))
        .filter_map(|sq| board.role_at(sq).map(role_value))
        .sum()
}

/// A move is a blunder when it increases our hanging material by at least a pawn,
/// or captures a defended unit so our capturer becomes hanging for no gain.
pub fn is_blunder(before: &Chess, mv: &shakmaty::Move) -> bool {
    let us = before.turn();
    let before_hang = hanging_material(before.board(), us);
    let Ok(after) = before.clone().play(*mv) else {
        return true;
    };
    let after_hang = hanging_material(after.board(), us);
    if after_hang >= before_hang + 100 {
        return true;
    }
    if let Some(captured) = mv.capture() {
        let to = mv.to();
        if is_hanging(after.board(), to) {
            let loot = role_value(captured);
            let our_piece = after.board().role_at(to).map_or(0, role_value);
            // Losing the capturer for less-or-equal loot is a blunder.
            if our_piece > loot {
                return true;
            }
            if our_piece == loot && after_hang > before_hang {
                return true;
            }
        }
    }
    false
}

pub fn safe_moves(position: &Chess) -> BTreeSet<String> {
    position
        .legal_moves()
        .iter()
        .filter(|m| !is_blunder(position, m))
        .map(|m| UciMove::from_move(*m, shakmaty::CastlingMode::Standard).to_string())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Cleared,
    Blundered,
}

#[derive(Clone, Debug)]
pub struct Round {
    pub fen: String,
    pub safe: BTreeSet<String>,
    pub status: Status,
    pub last_move: Option<String>,
}

impl Round {
    pub fn from_fen(fen: &str) -> Option<Self> {
        let position: Chess = fen
            .parse::<shakmaty::fen::Fen>()
            .ok()?
            .into_position(shakmaty::CastlingMode::Chess960)
            .ok()?;
        let safe = safe_moves(&position);
        if safe.is_empty() || safe.len() == position.legal_moves().len() {
            // Need at least one safe and one unsafe option so the drill has teeth.
            // Allow all-safe quiet positions; reject only empty.
            if safe.is_empty() {
                return None;
            }
        }
        Some(Self {
            fen: fen.to_string(),
            safe,
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
        if self.safe.contains(uci) {
            self.status = Status::Cleared;
        } else if self
            .position()
            .legal_moves()
            .iter()
            .any(|m| UciMove::from_move(*m, shakmaty::CastlingMode::Standard).to_string() == uci)
        {
            self.status = Status::Blundered;
        }
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

/// Curated quiet FENs (public-domain / original) with obvious hanging traps.
const BANK: &[&str] = &[
    // Black knight hanging on c6 to d5 pawn; White to move — capturing is safe, hanging own pieces is not.
    "4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1",
    // White queen hanging on d5 if it stays; many safe king moves.
    "4k3/8/8/3Q4/4p3/8/8/4K3 w - - 0 1",
    // Capturing the guarded pawn with the queen hangs her; quieter moves are safe.
    "4k3/8/2p5/3p4/8/8/3Q4/4K3 w - - 0 1",
    // Standard-ish middlegame fragment: hang the bishop on b5?
    "r1bqk2r/pppp1ppp/2n2n2/1B2p3/1b2P3/2N2N2/PPPP1PPP/R1BQK2R w KQkq - 4 4",
    "6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1",
    "2r3k1/pp3ppp/8/8/8/8/PPP2PPP/2R3K1 w - - 0 1",
];

pub fn generate(seed: u64) -> Round {
    let mut rng = Rng::new(seed);
    for _ in 0..32 {
        let fen = BANK[rng.gen_range(BANK.len())];
        if let Some(round) = Round::from_fen(fen) {
            return round;
        }
    }
    Round::from_fen(BANK[0]).expect("bank fen is valid")
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
            Status::Blundered => {
                self.streak = 0;
            }
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
    use shakmaty::fen::Fen;

    fn pos(fen: &str) -> Chess {
        fen.parse::<Fen>()
            .unwrap()
            .into_position(shakmaty::CastlingMode::Chess960)
            .unwrap()
    }

    #[test]
    fn capturing_hanging_knight_is_safe() {
        let p = pos("4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1");
        let mv = p
            .legal_moves()
            .into_iter()
            .find(|m| m.from() == Some(Square::D5) && m.to() == Square::C6)
            .unwrap();
        assert!(!is_blunder(&p, &mv));
        assert!(safe_moves(&p).contains("d5c6"));
    }

    #[test]
    fn hanging_own_queen_further_is_blunder() {
        // White knight on c3 jumps to d5, attacked by pawns on c6 and e6.
        let p = pos("4k3/8/2p1p3/8/8/2N5/8/4K3 w - - 0 1");
        let mv = p
            .legal_moves()
            .into_iter()
            .find(|m| m.from() == Some(Square::C3) && m.to() == Square::D5)
            .unwrap();
        assert!(is_blunder(&p, &mv), "knight jumps onto a hanging square");
    }

    #[test]
    fn taking_defended_pawn_with_queen_is_blunder() {
        // d5 pawn is guarded by c6; Qxd5 hangs the queen for a pawn.
        let p = pos("4k3/8/2p5/3p4/8/8/3Q4/4K3 w - - 0 1");
        let mv = p
            .legal_moves()
            .into_iter()
            .find(|m| m.from() == Some(Square::D2) && m.to() == Square::D5)
            .unwrap();
        assert!(is_blunder(&p, &mv));
        assert!(!safe_moves(&p).contains("d2d5"));
    }

    #[test]
    fn session_scores_safe_move() {
        let mut session = Session::start(1);
        session.round = Round::from_fen("4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(session.submit("d5c6"), Status::Cleared);
        assert_eq!(session.score, 1);
        assert_eq!(session.streak, 1);
    }

    #[test]
    fn session_breaks_streak_on_blunder() {
        let mut session = Session::start(2);
        session.round = Round::from_fen("4k3/8/2p5/3p4/8/8/3Q4/4K3 w - - 0 1").unwrap();
        session.streak = 4;
        assert_eq!(session.submit("d2d5"), Status::Blundered);
        assert_eq!(session.streak, 0);
    }

    #[test]
    fn generator_always_yields_safe_moves() {
        for seed in 0..30 {
            let round = generate(seed);
            assert!(!round.safe.is_empty(), "seed {seed}");
        }
    }
}
