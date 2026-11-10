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
    /// A PGN tag pair such as `[White "Morphy"]` or `[FEN "..."]`.
    Tag,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub span: Range<usize>,
}
/// Length of a PGN tag pair body after its `[`: a name, a quoted value and `]`.
fn tag_pair(rest: &str) -> Option<usize> {
    let name_end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    if name_end == 0 || !rest.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let after = rest[name_end..].trim_start();
    let value = after.strip_prefix('"')?;
    let close = value.find('"')?;
    let tail = &value[close + 1..];
    let bracket = tail.find(']')?;
    if !tail[..bracket].trim().is_empty() {
        return None;
    }
    Some(rest.len() - tail.len() + bracket + 1)
}
/// The name and value of a tag pair token.
pub fn tag(raw: &str) -> Option<(&str, &str)> {
    let body = raw.strip_prefix('[')?.strip_suffix(']')?.trim();
    let (name, value) = body.split_once(char::is_whitespace)?;
    Some((name, value.trim().strip_prefix('"')?.strip_suffix('"')?))
}
/// What a word means given the token before it: an evaluation sign, a result, a move number,
/// a label number ("Game 12", a year) or plain text.
fn word_kind(word: &str, previous: Option<(&Kind, &str)>) -> Kind {
    if matches!(word, "+-" | "-+" | "=" | "+/-" | "±" | "∓" | "⩲" | "⩱") {
        Kind::Nag
    } else if matches!(word, "1-0" | "0-1" | "1/2-1/2" | "½-½" | "*") {
        Kind::Result
    } else if let Ok(n) = word.trim_end_matches('.').parse::<u32>() {
        let named = previous.is_some_and(|(kind, raw)| *kind == Kind::Word && label_word(raw));
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
        if c == '['
            && let Some(len) = tag_pair(&text[i..])
        {
            i += len;
            result.push(Token {
                kind: Kind::Tag,
                span: start..i,
            });
            continue;
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
            // A printed ellipsis is its own token, also in "30…e5" and "…e5".
            '\u{2026}' => Kind::Word,
            _ => {
                i += spaced_move_tail(c, &text[i..]);
                while i < text.len() {
                    let next = text[i..].chars().next().expect("character boundary");
                    if next.is_whitespace()
                        || (next == '\u{2026}'
                            && text[start..i].bytes().all(|b| b.is_ascii_digit()))
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
                let previous = result.last().map(|t| (&t.kind, &text[t.span.clone()]));
                word_kind(&text[start..i], previous)
            }
        };
        result.push(Token {
            kind,
            span: start..i,
        });
    }
    result
}
/// The spaces to skip after a figurine when the next word completes a move: some books print
/// "♖ af8" or "♘ xe5" for one move.
fn spaced_move_tail(figurine: char, rest: &str) -> usize {
    if !('\u{2654}'..='\u{265f}').contains(&figurine) {
        return 0;
    }
    let word = rest.trim_start_matches([' ', '\u{a0}', '\u{2009}', '\u{202f}']);
    let gap = rest.len() - word.len();
    let end = word
        .find(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | ',' | ';' | '|'))
        .unwrap_or(word.len());
    let tail = word[..end].trim_end_matches(['+', '#', '!', '?', '.', ':']);
    let bytes: Vec<char> = tail
        .chars()
        .filter(|c| !matches!(c, 'x' | '×' | ':' | '-'))
        .collect();
    let square = |f: char, r: char| ('a'..='h').contains(&f) && ('1'..='8').contains(&r);
    let fits = match bytes.as_slice() {
        [f, r] => square(*f, *r),
        [d, f, r] => (('a'..='h').contains(d) || ('1'..='8').contains(d)) && square(*f, *r),
        [df, dr, f, r] => square(*df, *dr) && square(*f, *r),
        _ => false,
    };
    if gap > 0 && fits { gap } else { 0 }
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
/// Figurine fonts that type their pieces on Latin-1 codes, in KQRBNP order.
const FONT_FIGURINES: [&str; 2] = ["¢£¶•§ß", "¢£¦¥¤"];
fn role_letter(c: char, alphabet: &str) -> char {
    if let Some(i) = "♔♕♖♗♘♙"
        .chars()
        .position(|p| p == c)
        .or_else(|| "♚♛♜♝♞♟".chars().position(|p| p == c))
    {
        return "KQRBNP".chars().nth(i).expect("piece index");
    }
    for font in FONT_FIGURINES {
        if let Some(i) = font.chars().position(|p| p == c) {
            return "KQRBNP".chars().nth(i).expect("font piece index");
        }
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
        .replace('0', "O")
        // Typeset castling often joins the O's with an en or em dash.
        .replace(['\u{2013}', '\u{2014}', '\u{2212}'], "-");
    let mut result = String::new();
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        result.push(
            if c.is_uppercase()
                || ('♔'..='♟').contains(&c)
                || FONT_FIGURINES.iter().any(|f| f.contains(c))
            {
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
    match candidates(pos, text).as_slice() {
        [m] => Ok(*m),
        [] => Err(MatchError::Illegal),
        _ => Err(MatchError::Ambiguous),
    }
}
/// Every legal move a descriptive text could mean ("B-Kt2" may name either bishop), or failing
/// that, every piece move an unmapped figurine could mean.
pub fn candidates(pos: &Chess, text: &str) -> Vec<Move> {
    let moves: Vec<Move> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| descriptive(pos, *m, text))
        .collect();
    if moves.is_empty() {
        unmapped_piece(pos, text)
    } else {
        moves
    }
}
/// A figurine font without a Unicode mapping turns the piece into stray glyphs ("lbxe4",
/// "'ii'd8+"). Such a token is a piece move (never a pawn) to the square that ends it; the
/// caller still needs it to be the only legal reading or to be settled by the moves after it.
fn unmapped_piece(pos: &Chess, text: &str) -> Vec<Move> {
    let text = text.trim_end_matches(['+', '#', '!', '?']);
    let bytes = text.as_bytes();
    let n = bytes.len();
    if n < 3 || !(b'a'..=b'h').contains(&bytes[n - 2]) || !(b'1'..=b'8').contains(&bytes[n - 1]) {
        return vec![];
    }
    let prefix = &text[..n - 2];
    let (glyphs, capture) = match prefix.strip_suffix('x') {
        Some(g) => (g, true),
        None => (prefix, false),
    };
    // Piece letters, files and ranks are ordinary notation, which a failed parse already
    // rejected.
    // A Unicode figurine is a mapped piece, so it names its own role.
    let ordinary = glyphs.is_empty()
        || glyphs
            .chars()
            .any(|c| ('\u{2654}'..='\u{265f}').contains(&c))
        || glyphs
            .chars()
            .all(|c| "KQRBNP".contains(c) || ('a'..='h').contains(&c) || ('1'..='8').contains(&c))
        || glyphs.chars().count() > 6
        || glyphs.contains(char::is_whitespace);
    if ordinary {
        return vec![];
    }
    let Ok(to) = text[n - 2..].parse::<shakmaty::Square>() else {
        return vec![];
    };
    pos.legal_moves()
        .into_iter()
        .filter(|m| {
            m.to() == to && m.role() != Role::Pawn && !m.is_castle() && (!capture || m.is_capture())
        })
        .collect()
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
/// "Q. to her Kt's 3d", "K. to his B's sq.", "Q. Kt. to his 5th": the pronoun names the piece's
/// own wing, or its own file when no file follows.
fn own_wing(piece: &str, destination: &str) -> Option<String> {
    let rest = destination
        .strip_prefix("HIS")
        .or_else(|| destination.strip_prefix("HER"))?;
    let name = piece.strip_suffix('P').unwrap_or(piece);
    let wing = rest.starts_with('Q') || rest.starts_with('K') && !rest.starts_with("KT");
    Some(if wing {
        rest.to_string()
    } else if rest.starts_with(|c: char| c.is_ascii_digit()) || rest.starts_with("SQ") {
        format!("{name}{rest}")
    } else if let Some(side) = (name != "KT")
        .then(|| name.chars().next())
        .flatten()
        .filter(|c| matches!(c, 'K' | 'Q'))
    {
        // "Q. B.", "K. Kt." name a wing; a plain "Kt." does not.
        format!("{side}{rest}")
    } else if destination.starts_with("HER") {
        format!("Q{rest}")
    } else {
        rest.to_string()
    })
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
    let owned = own_wing(piece, destination);
    let destination = owned.as_deref().unwrap_or(destination);
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
    #[test]
    fn castling_joined_by_a_dash_of_any_width_parses() {
        use shakmaty::{CastlingMode, Chess, fen::Fen};
        let fen: Fen = "2rqkb1r/pp2pppp/nn6/2p1P3/2PP2b1/1PN1BN2/P3B1PP/R2QK2R w KQ - 0 12"
            .parse()
            .expect("fen");
        let pos: Chess = fen.into_position(CastlingMode::Standard).expect("pos");
        for text in ["0-0", "O-O", "0\u{2013}0", "O\u{2014}O"] {
            let m = super::parse(&pos, text, "KQRBN").expect(text);
            assert!(m.is_castle(), "{text}");
        }
        let text = "12.0\u{2013}0 Bxf3 13.Bxf3";
        let words: Vec<&str> = tokens(text).iter().map(|t| &text[t.span.clone()]).collect();
        assert_eq!(words, ["12.", "0\u{2013}0", "Bxf3", "13.", "Bxf3"]);
    }

    use super::*;
    #[test]
    fn a_spaced_figurine_joins_the_move_it_starts() {
        let text = "30\u{2026} \u{2656} af8 31. \u{2658} xe5+ the \u{2656} on a1 \u{2655} d1 and";
        let words: Vec<&str> = tokens(text).iter().map(|t| &text[t.span.clone()]).collect();
        assert_eq!(
            words,
            [
                "30",
                "\u{2026}",
                "\u{2656} af8",
                "31.",
                "\u{2658} xe5+",
                "the",
                "\u{2656}",
                "on",
                "a1",
                "\u{2655} d1",
                "and"
            ]
        );
        assert_eq!(normalize("\u{2656} af8", ""), "Raf8");
    }
    #[test]
    fn a_printed_ellipsis_splits_from_numbers_and_moves() {
        let text = "30\u{2026}e5 31\u{2026} \u{2026}Nf6 1...d5";
        let words: Vec<(Kind, &str)> = tokens(text)
            .into_iter()
            .map(|t| (t.kind, &text[t.span]))
            .collect();
        assert_eq!(
            words,
            [
                (Kind::Number(30), "30"),
                (Kind::Word, "\u{2026}"),
                (Kind::Word, "e5"),
                (Kind::Number(31), "31"),
                (Kind::Word, "\u{2026}"),
                (Kind::Word, "\u{2026}"),
                (Kind::Word, "Nf6"),
                (Kind::Number(1), "1..."),
                (Kind::Word, "d5"),
            ]
        );
    }
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
    fn unmapped_figurines_name_some_piece_on_the_square() {
        let mut p = Chess::default();
        for uci in ["e2e4", "e7e5"] {
            let m = uci.parse::<UciMove>().unwrap().to_move(&p).unwrap();
            p.play_unchecked(m);
        }
        let only = |text: &str| -> Vec<String> {
            candidates(&p, text)
                .iter()
                .map(|m| m.to_uci(shakmaty::CastlingMode::Standard).to_string())
                .collect()
        };
        assert_eq!(only("lbc3"), ["b1c3"]);
        assert_eq!(only("lbf3"), ["g1f3", "d1f3"]);
        assert_eq!(only("'ii'h5+"), ["d1h5"]);
        assert_eq!(only("..tc4"), ["f1c4"]);
        // King, queen, bishop and knight all reach e2; the moves after it have to settle it.
        assert_eq!(only("lDe2").len(), 4);
        // Files, ranks and piece letters are ordinary notation; a capture needs a capture.
        assert_eq!(only("bf3"), Vec::<String>::new());
        assert_eq!(only("lbxf3"), Vec::<String>::new());
        assert_eq!(only("lbf3extra"), Vec::<String>::new());
        // Pawns are never the hidden piece.
        assert_eq!(only("lbd4"), Vec::<String>::new());
        // A Unicode figurine names its piece: no rook reaches e2, so no other piece stands in.
        assert_eq!(only("\u{2656}e2"), Vec::<String>::new());
        assert_eq!(only("\u{265c}e2"), Vec::<String>::new());
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
        for text in ["§f3", "¤f3", "Kt-KB3", "K. Kt. to B's 3d."] {
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
    #[test]
    fn possessive_pronouns_name_the_pieces_own_wing() {
        let uci = |fen: &str, text: &str| {
            let pos: Chess = fen
                .parse::<shakmaty::fen::Fen>()
                .unwrap()
                .into_position(shakmaty::CastlingMode::Standard)
                .unwrap();
            parse(&pos, text, "KQRBN")
                .map(|m| m.to_uci(shakmaty::CastlingMode::Standard).to_string())
        };
        let open = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
        let c3 = "rnbqkbnr/pppp1ppp/8/4p3/4P3/2P5/PP1P1PPP/RNBQKBNR w KQkq - 0 2";
        assert_eq!(uci(c3, "Q. to her Kt's 3d.").as_deref(), Ok("d1b3"));
        assert_eq!(
            uci(open, "Q. to her 4th.").as_deref(),
            Err(&MatchError::Illegal)
        );
        assert_eq!(uci(open, "K. to his 2d.").as_deref(), Ok("e1e2"));
        assert_eq!(uci(open, "K. B. to his Q. B's 4th.").as_deref(), Ok("f1c4"));
        assert_eq!(uci(open, "Q. Kt. to his B's 3d.").as_deref(), Ok("b1c3"));
        assert_eq!(uci(open, "K. Kt. to his B's 3d.").as_deref(), Ok("g1f3"));
        // A plain "Kt." names no wing, so either knight could go to a Bishop's third.
        assert_eq!(
            uci(open, "Kt. to his B's 3d.").as_deref(),
            Err(&MatchError::Ambiguous)
        );
        let knight = "rnbqkbnr/pppp1ppp/8/4p3/4P3/2N5/PPPP1PPP/R1BQKBNR w KQkq - 0 2";
        assert_eq!(uci(knight, "Q. Kt. to his 5th.").as_deref(), Ok("c3b5"));
    }
    #[test]
    fn tag_pairs_are_single_tokens() {
        let text = "[PlyCount \"39\"] [1... Nf6] [Site \"?\"]";
        let kinds: Vec<_> = tokens(text).into_iter().map(|t| t.kind).collect();
        assert_eq!(kinds[0], Kind::Tag);
        assert_eq!(kinds[1], Kind::Open);
        assert_eq!(kinds.last(), Some(&Kind::Tag));
        assert_eq!(tag("[PlyCount \"39\"]"), Some(("PlyCount", "39")));
    }
}
