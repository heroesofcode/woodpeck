//! Table-driven piece alphabets; descriptive moves are matched against legal candidates.
use shakmaty::{Chess, Color, Move, Position, Role, san::San, uci::UciMove};
use std::ops::Range;

pub const ALPHABETS: &[(&str, &str)] = &[
    ("English", "KQRBN"),
    ("Portuguese", "RDTBC"),
    ("Spanish", "RDTAC"),
    ("German", "KDTLS"),
    ("French", "RDTFC"),
    ("Dutch", "KDTLP"),
    ("Italian", "RDTAC"),
    ("Polish", "KHWGS"),
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Number(u32),
    /// A number that names an example, diagram, game, page or year rather than a move.
    Label,
    Open,
    Close,
    Word,
    Nag,
    Result,
    Comment,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub span: Range<usize>,
}
pub fn tokens(text: &str) -> Vec<Token> {
    let mut result: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let start = i;
        let c = text[i..].chars().next().expect("character boundary");
        i += c.len_utf8();
        if c.is_whitespace() || matches!(c, ',' | ';' | '|') {
            continue;
        }
        // Older algebraic books print move numbers as (1), not variation delimiters.
        if c == '('
            && let Some(end) = text[i..].find(')')
            && let Ok(n) = text[i..i + end].parse::<u32>()
        {
            i += end + 1;
            result.push(Token {
                kind: Kind::Number(n),
                span: start..i,
            });
            continue;
        }
        if c == '('
            && let Some(end) = text[i..].find(')')
        {
            let promotion = text[i..i + end].trim();
            if matches!(promotion, "Q" | "R" | "B" | "N")
                && let Some(last) = result
                    .last_mut()
                    .filter(|t| text[t.span.clone()].ends_with(['8', '1']))
            {
                i += end + 1;
                last.span.end = i;
                last.kind = Kind::Word;
                continue;
            }
        }
        let kind = match c {
            '{' => {
                while i < text.len() && text.as_bytes()[i] != b'}' {
                    i += 1;
                }
                if i < text.len() {
                    i += 1;
                }
                Kind::Comment
            }
            '(' | '[' => Kind::Open,
            ')' | ']' => Kind::Close,
            '!' | '?' | '$' => {
                while i < text.len()
                    && (text.as_bytes()[i].is_ascii_digit()
                        || matches!(text.as_bytes()[i], b'!' | b'?'))
                {
                    i += 1;
                }
                Kind::Nag
            }
            _ => {
                while i < text.len() {
                    let next = text[i..].chars().next().expect("character boundary");
                    if next.is_whitespace()
                        || matches!(
                            next,
                            '(' | ')' | '[' | ']' | ',' | ';' | '|' | '!' | '?' | '$'
                        )
                    {
                        break;
                    }
                    // Separate move numbers even when printed as 1.e4.
                    if next == '.' && text[start..i].bytes().all(|b| b.is_ascii_digit()) {
                        while i < text.len() && text.as_bytes()[i] == b'.' {
                            i += 1;
                        }
                        break;
                    }
                    i += next.len_utf8();
                }
                let word = &text[start..i];
                if matches!(word, "+-" | "-+" | "=" | "+/-" | "±" | "∓" | "⩲" | "⩱") {
                    Kind::Nag
                } else if matches!(word, "1-0" | "0-1" | "1/2-1/2" | "½-½" | "*") {
                    Kind::Result
                } else if let Ok(n) = word.trim_end_matches('.').parse::<u32>() {
                    let named = result
                        .last()
                        .is_some_and(|t| t.kind == Kind::Word && label_word(&text[t.span.clone()]));
                    // No game reaches move 1000: four digits are years.
                    if named || n >= 1000 {
                        Kind::Label
                    } else {
                        Kind::Number(n)
                    }
                } else {
                    Kind::Word
                }
            }
        };
        result.push(Token {
            kind,
            span: start..i,
        });
    }
    result
}
/// What a labelled number names, from the word printed before it ("Example 8", "Game No. 2").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelKind {
    /// A new game from the initial position.
    Game,
    /// A new example or problem whose position the text alone does not give.
    Example,
    /// A cross-reference: a diagram, page, chapter or move named in prose.
    Reference,
}
/// The kind of label introduced by `word`, if it introduces one.
pub fn label_kind(word: &str) -> Option<LabelKind> {
    // "Game 2" and "No. 2" label the number; "A game: 1. e4" introduces a move.
    let word = word
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_end_matches('.');
    if word.ends_with(|c: char| !c.is_alphanumeric()) {
        return None;
    }
    let word = word.to_lowercase();
    match word.as_str() {
        "game" | "games" | "partie" | "partida" | "partita" | "spiel" | "partij" | "partia"
        | "round" => Some(LabelKind::Game),
        "example" | "examples" | "ex" | "problem" | "problems" | "exercise" | "exercises"
        | "position" | "positions" | "study" | "studies" | "puzzle" | "beispiel" | "aufgabe"
        | "stellung" | "exemple" | "ejemplo" | "esempio" | "voorbeeld" | "przykład" | "zadanie" => {
            Some(LabelKind::Example)
        }
        "diagram" | "diagrams" | "diag" | "dia" | "fig" | "figure" | "figures" | "no" | "nr"
        | "number" | "page" | "pages" | "pp" | "chapter" | "chap" | "section" | "part" | "move"
        | "moves" | "diagramm" | "diagramme" | "diagrama" | "diagramma" | "diagramie"
        | "rozdział" | "seite" | "kapitel" => Some(LabelKind::Reference),
        _ => None,
    }
}
fn label_word(word: &str) -> bool {
    label_kind(word).is_some()
}
fn role_letter(c: char, alphabet: &str) -> char {
    if let Some(i) = "♔♕♖♗♘♙"
        .chars()
        .position(|p| p == c)
        .or_else(|| "♚♛♜♝♞♟".chars().position(|p| p == c))
    {
        return "KQRBNP".chars().nth(i).expect("piece index");
    }
    if let Some(i) = "¢£¶•§ß".chars().position(|p| p == c) {
        return "KQRBNP".chars().nth(i).expect("font piece index");
    }
    alphabet
        .chars()
        .position(|p| p == c)
        .and_then(|i| "KQRBN".chars().nth(i))
        .unwrap_or(c)
}
pub fn normalize(text: &str, alphabet: &str) -> String {
    let text = text
        .trim()
        .trim_end_matches(['+', '#', '!', '?'])
        .replace('0', "O");
    let mut result = String::new();
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        result.push(
            if c.is_uppercase() || ('♔'..='♟').contains(&c) || "¢£¶•§ß".contains(c) {
                role_letter(c, alphabet)
            } else {
                c
            },
        );
    }
    // Older algebraic promotion omits '='.
    let bytes = result.as_bytes();
    if bytes.len() >= 3
        && matches!(bytes[bytes.len() - 1], b'Q' | b'R' | b'B' | b'N')
        && matches!(bytes[bytes.len() - 2], b'1' | b'8')
    {
        result.insert(result.len() - 1, '=');
    }
    result
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchError {
    Illegal,
    Ambiguous,
}
pub fn parse(pos: &Chess, text: &str, alphabet: &str) -> Result<Move, MatchError> {
    let legacy = text.replace("Kt", "N").replace(':', "x");
    let normalized = normalize(&legacy, alphabet);
    // Old algebraic writes P-e4 / Kt-f3; square letters distinguish it from descriptive.
    let algebraic = normalized.replace('-', "");
    let algebraic = algebraic.strip_prefix('P').unwrap_or(&algebraic);
    if let Ok(san) = algebraic.parse::<San>()
        && let Ok(m) = san.to_move(pos)
    {
        return Ok(m);
    }
    if let Ok(san) = normalized.parse::<San>()
        && let Ok(m) = san.to_move(pos)
    {
        return Ok(m);
    }
    let lan = normalized
        .trim_start_matches(['K', 'Q', 'R', 'B', 'N'])
        .replace(['-', 'x', '='], "");
    if let Ok(uci) = lan.to_ascii_lowercase().parse::<UciMove>()
        && let Ok(m) = uci.to_move(pos)
    {
        return Ok(m);
    }
    let candidates: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| descriptive(pos, *m, text))
        .collect();
    match candidates.as_slice() {
        [m] => Ok(*m),
        [] => Err(MatchError::Illegal),
        _ => Err(MatchError::Ambiguous),
    }
}
fn desc_role(s: &str) -> Option<Role> {
    match s {
        "P" => Some(Role::Pawn),
        "N" | "KT" => Some(Role::Knight),
        "B" => Some(Role::Bishop),
        "R" => Some(Role::Rook),
        "Q" => Some(Role::Queen),
        "K" => Some(Role::King),
        _ => None,
    }
}
fn desc_file(s: &str, file: u32) -> bool {
    match s {
        "QR" => file == 0,
        "QKT" | "QN" => file == 1,
        "QB" => file == 2,
        "Q" => file == 3,
        "K" => file == 4,
        "KB" => file == 5,
        "KKT" | "KN" => file == 6,
        "KR" => file == 7,
        "R" => file == 0 || file == 7,
        "KT" | "N" => file == 1 || file == 6,
        "B" => file == 2 || file == 5,
        _ => false,
    }
}
fn descriptive(pos: &Chess, m: Move, text: &str) -> bool {
    let mut s = text
        .to_uppercase()
        .replace([' ', '\u{a0}', '.', '+', '#'], "")
        .replace(['—', '–'], "-")
        .replace('×', "X");
    for (from, to) in [
        ("'S", ""),
        ("’S", ""),
        ("TAKES", "X"),
        ("TO", "-"),
        ("1ST", "1"),
        ("2D", "2"),
        ("3D", "3"),
        ("4TH", "4"),
        ("5TH", "5"),
        ("6TH", "6"),
        ("7TH", "7"),
        ("8TH", "8"),
    ] {
        s = s.replace(from, to);
    }
    for suffix in ["DISCH", "DOUBLECH", "DBLCH", "CH", "MATE", "EP"] {
        if s.ends_with(suffix) {
            s.truncate(s.len() - suffix.len());
        }
    }
    if matches!(s.as_str(), "CASTLES" | "CASTLESQR" | "CASTLESKR") {
        let kingside = s != "CASTLESQR";
        return matches!(m, Move::Castle { .. })
            && (m.to().file() > m.from().expect("castle origin").file()) == kingside;
    }
    for piece in ["Q", "R", "B", "N"] {
        s = s.replace(&format!("({piece})"), &format!("={piece}"));
    }
    let mut promotion = None;
    if let Some((prefix, piece)) = s.split_once('=') {
        promotion = piece.chars().next().and_then(Role::from_char);
        s = prefix.into();
    }
    if m.promotion() != promotion {
        return false;
    }
    let Some((piece, destination)) = s.split_once(['-', 'X']).or_else(|| {
        ["KT", "P", "N", "B", "R", "Q", "K"]
            .iter()
            .find_map(|p| s.strip_prefix(p).filter(|d| !d.is_empty()).map(|d| (*p, d)))
    }) else {
        return false;
    };
    let role = if piece.ends_with('P') {
        Some(Role::Pawn)
    } else {
        desc_role(piece).or_else(|| piece.get(1..).and_then(desc_role))
    };
    if role != Some(m.role()) {
        return false;
    }
    if desc_role(piece).is_none() {
        let source = m.from().expect("ordinary move source");
        let file = piece.strip_suffix('P').unwrap_or(piece);
        let initial_rank = if pos.turn().is_white() { 0 } else { 7 };
        let original_candidate = (pos.board().by_role(m.role()) & pos.board().by_color(pos.turn()))
            .into_iter()
            .any(|square| {
                u32::from(square.rank()) == initial_rank
                    && desc_file(file, u32::from(square.file()))
            });
        let needs_file = m.role() == Role::Pawn
            || u32::from(source.rank()) == initial_rank
            || original_candidate;
        if needs_file && !desc_file(file, u32::from(source.file())) {
            return false;
        }
    }
    if s.contains('X') != m.is_capture() {
        return false;
    }
    if let Some(target) = desc_role(destination) {
        return m.capture() == Some(target);
    }
    if let Some(file) = destination.strip_suffix('P') {
        return m.capture() == Some(Role::Pawn) && desc_file(file, u32::from(m.to().file()));
    }
    let destination = destination.replace("SQ", "1");
    let Some(rank) = destination.chars().last().and_then(|c| c.to_digit(10)) else {
        return false;
    };
    let file = &destination[..destination.len() - 1];
    let actual_rank = u32::from(m.to().rank()) + 1;
    let expected_rank = if pos.turn() == Color::White {
        rank
    } else {
        9_u32.saturating_sub(rank)
    };
    actual_rank == expected_rank && desc_file(file, u32::from(m.to().file()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alphabets_and_long_algebraic() {
        let p = Chess::default();
        for (text, alphabet, uci) in [
            ("♘f3", "KQRBN", "g1f3"),
            ("Cf3", "RDTBC", "g1f3"),
            ("Sf3", "KDTLS", "g1f3"),
            ("e2-e4", "KQRBN", "e2e4"),
            ("Ng1-f3", "KQRBN", "g1f3"),
        ] {
            assert_eq!(
                parse(&p, text, alphabet)
                    .unwrap()
                    .to_uci(shakmaty::CastlingMode::Standard)
                    .to_string(),
                uci
            );
        }
    }
    #[test]
    fn descriptive_matches_from_movers_side_and_rejects_ambiguity() {
        let mut p = Chess::default();
        for (text, uci) in [
            ("P-K4", "e2e4"),
            ("P - K 4", "e7e5"),
            ("Kt-KB3", "g1f3"),
            ("N-QB3", "b8c6"),
            ("B-Kt5", "f1b5"),
        ] {
            let m = parse(&p, text, "KQRBN").unwrap();
            assert_eq!(m.to_uci(shakmaty::CastlingMode::Standard).to_string(), uci);
            p.play_unchecked(m);
        }
        assert_eq!(
            parse(&Chess::default(), "N-B3", "KQRBN"),
            Err(MatchError::Ambiguous)
        );
        assert_eq!(
            parse(&Chess::default(), "e5", "KQRBN"),
            Err(MatchError::Illegal)
        );
    }
    #[test]
    fn punctuation_and_utf8_offsets() {
        let s = "1.♘f3!? (1... e5 [2. d4]) $1 ½-½";
        let ts = tokens(s);
        assert_eq!(ts[0].kind, Kind::Number(1));
        assert_eq!(&s[ts[1].span.clone()], "♘f3");
        assert_eq!(ts.iter().filter(|t| t.kind == Kind::Open).count(), 2);
        assert_eq!(ts.iter().filter(|t| t.kind == Kind::Nag).count(), 2);
        assert_eq!(ts.last().unwrap().kind, Kind::Result);
    }
}

#[cfg(test)]
mod typography_tests {
    use super::*;
    use crate::books::resolve::position;
    #[test]
    fn descriptive_squares_checks_promotions_and_font_figurines() {
        for (fen, text, uci) in [
            ("k7/8/8/8/8/8/8/4K2R w - - 0 1", "R - R 7", "h1h7"),
            ("k7/8/8/8/8/8/8/4KQ2 w - - 0 1", "Q B 7", "f1f7"),
            ("k7/8/8/8/8/8/8/5K2 w - - 0 1", "K Kt 1", "f1g1"),
            ("7k/8/8/8/8/8/8/R4K2 w - - 0 1", "R-Q sq", "a1d1"),
            ("k7/8/8/8/8/8/8/4K3 w - - 0 1", "K-B sq", "e1f1"),
            ("k7/4P3/8/8/8/8/8/4K3 w - - 0 1", "P-K8 (Q)", "e7e8q"),
            ("k7/4P3/8/8/8/8/8/4K3 w - - 0 1", "e7-e8Q", "e7e8q"),
        ] {
            let pos = position(fen).unwrap();
            assert_eq!(
                parse(&pos, text, "KQRBN")
                    .unwrap()
                    .to_uci(shakmaty::CastlingMode::Standard)
                    .to_string(),
                uci,
                "{text}"
            );
        }
        let qualified = position("k7/8/8/6N1/8/8/8/1N2K3 w - - 0 1").unwrap();
        assert_eq!(
            parse(&qualified, "QKt-B3", "KQRBN")
                .unwrap()
                .to_uci(shakmaty::CastlingMode::Standard)
                .to_string(),
            "b1c3"
        );
        for text in ["§f3", "Kt-KB3", "K. Kt. to B's 3d."] {
            assert_eq!(
                parse(&Chess::default(), text, "KQRBN")
                    .unwrap()
                    .to_uci(shakmaty::CastlingMode::Standard)
                    .to_string(),
                "g1f3"
            );
        }
        assert_eq!(
            parse(&Chess::default(), "P-Q4 dis ch", "KQRBN")
                .unwrap()
                .to_uci(shakmaty::CastlingMode::Standard)
                .to_string(),
            "d2d4"
        );
    }
}
