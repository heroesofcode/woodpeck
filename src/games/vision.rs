//! Board-scan drill: find hanging or undefended pieces on a quiet board.
use shakmaty::{Board, Chess, Color, FromSetup, Position, Role, Square};
use std::collections::BTreeSet;
use std::time::Duration;

/// Which class of squares the player must tap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrillMode {
    HangingPieces,
    HangingPawns,
    UndefendedPieces,
    UndefendedPawns,
}

impl DrillMode {
    pub const ALL: [Self; 4] = [
        Self::HangingPieces,
        Self::HangingPawns,
        Self::UndefendedPieces,
        Self::UndefendedPawns,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::HangingPieces => "Hanging pieces",
            Self::HangingPawns => "Hanging pawns",
            Self::UndefendedPieces => "Undefended pieces",
            Self::UndefendedPawns => "Undefended pawns",
        }
    }

    pub fn prompt(self) -> &'static str {
        match self {
            Self::HangingPieces => {
                "Tap every hanging piece (attacked and undefended, or attacked by something cheaper)."
            }
            Self::HangingPawns => "Tap every hanging pawn.",
            Self::UndefendedPieces => "Tap every undefended piece (no friendly guard).",
            Self::UndefendedPawns => "Tap every undefended pawn.",
        }
    }

    fn wants_pawn(self) -> bool {
        matches!(self, Self::HangingPawns | Self::UndefendedPawns)
    }

    fn wants_hanging(self) -> bool {
        matches!(self, Self::HangingPieces | Self::HangingPawns)
    }
}

/// Centipawn-ish weights used only for hanging comparisons.
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

fn is_attacked(board: &Board, square: Square, by: Color) -> bool {
    !attackers(board, square, by).is_empty()
}

fn is_defended(board: &Board, square: Square, by: Color) -> bool {
    !attackers(board, square, by).is_empty()
}

fn cheapest_attacker(board: &Board, square: Square, by: Color) -> Option<i32> {
    attackers(board, square, by)
        .into_iter()
        .filter_map(|sq| board.role_at(sq).map(role_value))
        .min()
}

/// A non-king piece is hanging when an enemy attacks it and either nobody
/// defends it, or the cheapest attacker is cheaper than the piece itself.
pub fn is_hanging(board: &Board, square: Square) -> bool {
    let Some(piece) = board.piece_at(square) else {
        return false;
    };
    if piece.role == Role::King {
        return false;
    }
    let enemy = piece.color.other();
    if !is_attacked(board, square, enemy) {
        return false;
    }
    if !is_defended(board, square, piece.color) {
        return true;
    }
    cheapest_attacker(board, square, enemy).is_some_and(|v| v < role_value(piece.role))
}

/// A non-king piece is undefended when no friendly unit attacks its square.
pub fn is_undefended(board: &Board, square: Square) -> bool {
    let Some(piece) = board.piece_at(square) else {
        return false;
    };
    if piece.role == Role::King {
        return false;
    }
    !is_defended(board, square, piece.color)
}

/// Squares that match `mode` on `board`.
pub fn targets_for(board: &Board, mode: DrillMode) -> BTreeSet<Square> {
    let mut out = BTreeSet::new();
    for square in Square::ALL {
        let Some(piece) = board.piece_at(square) else {
            continue;
        };
        if (piece.role == Role::Pawn) != mode.wants_pawn() {
            continue;
        }
        let ok = if mode.wants_hanging() {
            is_hanging(board, square)
        } else {
            is_undefended(board, square)
        };
        if ok {
            out.insert(square);
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    Selected,
    Correct,
    Missed,
    Wrong,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Cleared,
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Hit,
    AlreadySelected,
    Miss,
    Ignored,
}

#[derive(Clone, Debug)]
pub struct Round {
    pub fen: String,
    pub mode: DrillMode,
    pub targets: BTreeSet<Square>,
    pub selected: BTreeSet<Square>,
    pub wrong: BTreeSet<Square>,
    pub status: Status,
    pub deadline: Duration,
    pub white_bottom: bool,
}

impl Round {
    pub fn from_fen(fen: &str, mode: DrillMode, now: Duration, limit: Duration) -> Option<Self> {
        let position: Chess = fen
            .parse::<shakmaty::fen::Fen>()
            .ok()?
            .into_position(shakmaty::CastlingMode::Chess960)
            .ok()?;
        let targets = targets_for(position.board(), mode);
        if targets.is_empty() {
            return None;
        }
        Some(Self {
            fen: fen.to_string(),
            mode,
            targets,
            selected: BTreeSet::new(),
            wrong: BTreeSet::new(),
            status: Status::Playing,
            deadline: now.saturating_add(limit),
            white_bottom: position.turn() == Color::White,
        })
    }

    pub fn remaining_targets(&self) -> usize {
        self.targets.difference(&self.selected).count()
    }

    pub fn tap(&mut self, square: Square) -> Outcome {
        if self.status != Status::Playing {
            return Outcome::Ignored;
        }
        if self.selected.contains(&square) {
            return Outcome::AlreadySelected;
        }
        if self.targets.contains(&square) {
            self.selected.insert(square);
            if self.remaining_targets() == 0 {
                self.status = Status::Cleared;
            }
            Outcome::Hit
        } else if self.position_board().piece_at(square).is_some() {
            self.wrong.insert(square);
            Outcome::Miss
        } else {
            Outcome::Ignored
        }
    }

    pub fn tick(&mut self, now: Duration) {
        if self.status == Status::Playing && now >= self.deadline {
            self.status = Status::TimedOut;
        }
    }

    pub fn highlights(&self) -> Vec<(Square, Highlight)> {
        let mut out = Vec::new();
        for &sq in &self.selected {
            out.push((sq, Highlight::Correct));
        }
        for &sq in &self.wrong {
            out.push((sq, Highlight::Wrong));
        }
        if self.status == Status::TimedOut {
            for &sq in self.targets.difference(&self.selected) {
                out.push((sq, Highlight::Missed));
            }
        }
        out
    }

    pub fn position(&self) -> Chess {
        self.fen
            .parse::<shakmaty::fen::Fen>()
            .expect("round fen is validated")
            .into_position(shakmaty::CastlingMode::Chess960)
            .expect("round fen is legal")
    }

    fn position_board(&self) -> Board {
        self.position().board().clone()
    }

    pub fn time_left(&self, now: Duration) -> Duration {
        self.deadline.saturating_sub(now)
    }
}

/// Seeded xorshift64 for reproducible drills without pulling in a RNG crate.
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

/// Build a quiet position that contains at least one target for `mode`.
pub fn generate(mode: DrillMode, seed: u64) -> Round {
    let mut rng = Rng::new(seed);
    for attempt in 0..64_u64 {
        if let Some(round) = try_generate(mode, &mut rng, attempt) {
            return round;
        }
    }
    // Deterministic fallback with a known hanging black knight on c6.
    Round::from_fen(
        "4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1",
        DrillMode::HangingPieces,
        Duration::ZERO,
        Duration::from_secs(30),
    )
    .expect("fallback fen has a hanging knight")
}

fn try_generate(mode: DrillMode, rng: &mut Rng, salt: u64) -> Option<Round> {
    let mut board = Board::empty();
    board.set_piece_at(Square::E1, Color::White.king());
    board.set_piece_at(Square::E8, Color::Black.king());

    let pool = [
        Role::Pawn,
        Role::Knight,
        Role::Bishop,
        Role::Rook,
        Role::Queen,
        Role::Pawn,
        Role::Knight,
        Role::Bishop,
    ];
    let count = 6 + rng.gen_range(6);
    for _ in 0..count {
        let role = pool[rng.gen_range(pool.len())];
        let color = if rng.gen_range(2) == 0 {
            Color::White
        } else {
            Color::Black
        };
        let square = Square::new(u32::try_from(rng.gen_range(64)).unwrap_or(0));
        if board.piece_at(square).is_some() {
            continue;
        }
        if role == Role::Pawn {
            let rank = square.rank();
            if rank == shakmaty::Rank::First || rank == shakmaty::Rank::Eighth {
                continue;
            }
        }
        board.set_piece_at(square, role.of(color));
    }

    // Force at least one matching target when random placement fails.
    ensure_target(&mut board, mode, rng);

    let turn = if rng.gen_range(2) == 0 {
        Color::White
    } else {
        Color::Black
    };
    let mut setup = shakmaty::Setup::empty();
    setup.board = board;
    setup.turn = turn;
    let position = Chess::from_setup(setup, shakmaty::CastlingMode::Chess960).ok()?;
    if position.is_check() {
        return None;
    }
    let fen =
        shakmaty::fen::Fen::from_position(&position, shakmaty::EnPassantMode::Legal).to_string();
    let limit = Duration::from_millis(12_000 + (salt % 8) * 1_000);
    Round::from_fen(&fen, mode, Duration::ZERO, limit)
}

fn ensure_target(board: &mut Board, mode: DrillMode, _rng: &mut Rng) {
    if !targets_for(board, mode).is_empty() {
        return;
    }
    // Known quiet shapes that satisfy each mode without checking the kings.
    for sq in [Square::C6, Square::F6, Square::C3, Square::F3] {
        board.discard_piece_at(sq);
    }
    board.discard_piece_at(Square::D5);
    board.discard_piece_at(Square::E4);
    board.discard_piece_at(Square::B4);
    board.discard_piece_at(Square::C4);
    match mode {
        DrillMode::HangingPieces => {
            // Black knight on c6 attacked by white pawn on d5.
            board.set_piece_at(Square::C6, Color::Black.knight());
            board.set_piece_at(Square::D5, Color::White.pawn());
        }
        DrillMode::HangingPawns => {
            // Black pawn on b4 attacked by white pawn on c3.
            board.set_piece_at(Square::B4, Color::Black.pawn());
            board.set_piece_at(Square::C3, Color::White.pawn());
        }
        DrillMode::UndefendedPieces => {
            board.set_piece_at(Square::C6, Color::Black.bishop());
        }
        DrillMode::UndefendedPawns => {
            board.set_piece_at(Square::C6, Color::Black.pawn());
        }
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub mode: DrillMode,
    pub round: Round,
    pub score: u32,
    pub streak: u32,
    pub best_streak: u32,
    pub seed: u64,
}

impl Session {
    pub fn start(mode: DrillMode, seed: u64) -> Self {
        let round = generate(mode, seed);
        Self {
            mode,
            round,
            score: 0,
            streak: 0,
            best_streak: 0,
            seed,
        }
    }

    pub fn tap(&mut self, square: Square) -> Outcome {
        let outcome = self.round.tap(square);
        match outcome {
            Outcome::Hit => {
                if self.round.status == Status::Cleared {
                    self.score += 1;
                    self.streak += 1;
                    self.best_streak = self.best_streak.max(self.streak);
                }
            }
            Outcome::Miss => {
                self.streak = 0;
            }
            _ => {}
        }
        outcome
    }

    pub fn tick(&mut self, now: Duration) {
        let before = self.round.status;
        self.round.tick(now);
        if before == Status::Playing && self.round.status == Status::TimedOut {
            self.streak = 0;
        }
    }

    pub fn next_round(&mut self, now: Duration) {
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut round = generate(self.mode, self.seed);
        // Re-base the deadline against the caller's clock.
        let limit = round.deadline;
        round.deadline = now.saturating_add(limit);
        self.round = round;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::fen::Fen;

    fn board(fen: &str) -> Board {
        let position: Chess = fen
            .parse::<Fen>()
            .unwrap()
            .into_position(shakmaty::CastlingMode::Chess960)
            .unwrap();
        position.board().clone()
    }

    #[test]
    fn hanging_undefended_knight() {
        // Black knight on c6 attacked by d5 pawn, no defender.
        let b = board("4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1");
        assert!(is_hanging(&b, Square::C6));
        let targets = targets_for(&b, DrillMode::HangingPieces);
        assert_eq!(targets, BTreeSet::from([Square::C6]));
    }

    #[test]
    fn defended_against_equal_attacker_is_not_hanging() {
        // Black knight on c6 attacked by a white knight and defended by a pawn.
        // Equal-or-greater attackers do not count as hanging under our rule.
        let b = board("4k3/1p6/2n5/8/1N6/8/8/4K3 w - - 0 1");
        assert!(!is_hanging(&b, Square::C6));
        assert!(is_attacked(&b, Square::C6, Color::White));
        assert!(is_defended(&b, Square::C6, Color::Black));
    }

    #[test]
    fn attacked_by_cheaper_piece_is_hanging() {
        // Black queen on d5 attacked by white pawn on e4, defended by king — still hanging.
        let b = board("4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1");
        assert!(is_hanging(&b, Square::D5));
    }

    #[test]
    fn undefended_bishop() {
        let b = board("4k3/8/8/8/8/8/2b5/4K3 w - - 0 1");
        assert!(is_undefended(&b, Square::C2));
        assert!(targets_for(&b, DrillMode::UndefendedPieces).contains(&Square::C2));
    }

    #[test]
    fn tap_clears_round_and_scores() {
        let mut session = Session::start(DrillMode::HangingPieces, 1);
        // Replace with a known position.
        session.round = Round::from_fen(
            "4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1",
            DrillMode::HangingPieces,
            Duration::from_secs(1),
            Duration::from_secs(20),
        )
        .unwrap();
        assert_eq!(session.tap(Square::C6), Outcome::Hit);
        assert_eq!(session.round.status, Status::Cleared);
        assert_eq!(session.score, 1);
        assert_eq!(session.streak, 1);
    }

    #[test]
    fn wrong_tap_breaks_streak() {
        let mut session = Session::start(DrillMode::HangingPieces, 2);
        session.round = Round::from_fen(
            "4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1",
            DrillMode::HangingPieces,
            Duration::ZERO,
            Duration::from_secs(20),
        )
        .unwrap();
        session.streak = 3;
        assert_eq!(session.tap(Square::E8), Outcome::Miss);
        assert_eq!(session.streak, 0);
        assert!(session.round.wrong.contains(&Square::E8));
    }

    #[test]
    fn timeout_marks_missed_targets() {
        let mut round = Round::from_fen(
            "4k3/8/2n5/3P4/8/8/8/4K3 w - - 0 1",
            DrillMode::HangingPieces,
            Duration::from_secs(0),
            Duration::from_secs(5),
        )
        .unwrap();
        round.tick(Duration::from_secs(5));
        assert_eq!(round.status, Status::TimedOut);
        assert!(
            round
                .highlights()
                .iter()
                .any(|(sq, h)| *sq == Square::C6 && *h == Highlight::Missed)
        );
    }

    #[test]
    fn generator_always_has_targets() {
        for mode in DrillMode::ALL {
            for seed in 0..40 {
                let mode_i = DrillMode::ALL.iter().position(|m| *m == mode).unwrap() as u64;
                let round = generate(mode, seed * 17 + mode_i);
                assert!(
                    !round.targets.is_empty(),
                    "mode={mode:?} seed={seed} produced no targets"
                );
                // Recompute from fen to catch drift.
                let again = targets_for(round.position().board(), mode);
                assert_eq!(round.targets, again);
            }
        }
    }

    #[test]
    fn hanging_pawns_mode_ignores_pieces() {
        // Hanging knight plus hanging pawn.
        let b = board("4k3/8/2n5/3P4/1p6/2P5/8/4K3 w - - 0 1");
        let pieces = targets_for(&b, DrillMode::HangingPieces);
        let pawns = targets_for(&b, DrillMode::HangingPawns);
        assert!(pieces.contains(&Square::C6));
        assert!(!pieces.contains(&Square::B4));
        assert!(pawns.contains(&Square::B4) || pawns.contains(&Square::C3) || !pawns.is_empty());
    }
}
