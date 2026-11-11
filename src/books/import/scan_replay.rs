//! Game text replayed from move 1 on a scanned page, as an independent check of a board's side.
//!
//! Every OCR line on the page that opens a game (`1.` followed by a move) starts a stream that
//! runs on in reading order. The stream is replayed from the initial position, one legal move
//! per printed token, while every printed move number agrees with the ply count. A board is
//! later certified only when its whole recognized placement equals a replayed one, so the
//! text binds to a board by its pieces, not by where it sits on the page. A placement reached
//! with White to move in one place and Black in another is dropped.
//!
//! OCR misreads are tolerated only inside a closed confusion table and by one dropped
//! character. A token that fits no legal move, or too many, ends the stream.

use super::scan_caption::OcrLine;
use crate::books::diagram::Frame;
use shakmaty::{Chess, Move, Position, Role, san::San};
use std::collections::HashMap;

/// Lines below this OCR confidence are not read as game text.
const LINE_CONFIDENCE_MIN: f32 = 0.8;
/// Most positions a stream may carry at once while a misread token stays ambiguous.
const BEAM_MAX: usize = 48;
/// A line wider than this fraction of the page spans both columns.
const WIDE_LINE: f64 = 0.6;
/// When more than this fraction of the lines is wide, the page is read as a single column.
const WIDE_PAGE: f64 = 0.3;
/// Most placements stored for one page.
const PLACEMENTS_MAX: usize = 2000;

/// Placements (first FEN field) reached by the page's game text, each with `true` when White is
/// to move there. Placements reached with both sides to move are left out.
pub(super) fn replayed_placements(
    lines: &[OcrLine],
    frames: &[Frame],
    width: usize,
) -> Vec<(String, bool)> {
    let texts = reading_order(lines, frames, width);
    let mut sides: HashMap<String, (bool, bool)> = HashMap::new();
    for (index, text) in texts.iter().enumerate() {
        for start in game_starts(text) {
            let mut stream = text[start..].to_owned();
            for next in &texts[index + 1..] {
                stream.push(' ');
                stream.push_str(next);
            }
            for (placement, white) in replay(&tokens(&stream)) {
                let entry = sides.entry(placement).or_insert((false, false));
                if white {
                    entry.0 = true;
                } else {
                    entry.1 = true;
                }
            }
        }
    }
    let mut out: Vec<(String, bool)> = sides
        .into_iter()
        .filter(|(_, (white, black))| white != black)
        .map(|(placement, (white, _))| (placement, white))
        .collect();
    out.sort();
    if out.len() > PLACEMENTS_MAX {
        return Vec::new();
    }
    out
}

/// Confident lines outside every board, joined per baseline, in column reading order.
#[allow(clippy::cast_precision_loss)]
fn reading_order(lines: &[OcrLine], frames: &[Frame], width: usize) -> Vec<String> {
    let width = width as f64;
    let inside = |line: &OcrLine| {
        let x = f64::midpoint(line.left, line.right);
        let y = f64::midpoint(line.top, line.bottom);
        frames.iter().any(|frame| {
            (frame[0] as f64..=frame[2] as f64).contains(&x)
                && (frame[1] as f64..=frame[3] as f64).contains(&y)
        })
    };
    let kept: Vec<&OcrLine> = lines
        .iter()
        .filter(|line| {
            line.confidence.is_finite()
                && line.confidence >= LINE_CONFIDENCE_MIN
                && line.right > line.left
                && line.bottom > line.top
                && !inside(line)
        })
        .collect();
    let wide = kept
        .iter()
        .filter(|line| line.right - line.left > WIDE_LINE * width)
        .count();
    let single = wide as f64 > WIDE_PAGE * kept.len().max(1) as f64;
    let column = |line: &OcrLine| {
        if single {
            0
        } else {
            usize::from(f64::midpoint(line.left, line.right) >= width / 2.0)
        }
    };
    let mut ordered = kept;
    ordered.sort_by(|a, b| {
        column(a)
            .cmp(&column(b))
            .then(a.top.total_cmp(&b.top))
            .then(a.left.total_cmp(&b.left))
    });
    let mut out: Vec<(usize, f64, f64, f64, String)> = Vec::new();
    for line in ordered {
        let height = line.bottom - line.top;
        if let Some(last) = out.last_mut()
            && last.0 == column(line)
            && (last.1 - line.top).abs() < height * 0.5
        {
            if line.left >= last.2 {
                last.4.push(' ');
                last.4.push_str(&line.text);
                last.2 = line.right;
            } else {
                last.4 = format!("{} {}", line.text, last.4);
                last.3 = line.left;
            }
            continue;
        }
        out.push((
            column(line),
            line.top,
            line.right,
            line.left,
            line.text.clone(),
        ));
    }
    out.into_iter().map(|line| line.4).collect()
}

/// Byte offsets where `1.` opens a game: not part of a longer number, not `1...`, and followed
/// by a move character.
fn game_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for (i, &c) in bytes.iter().enumerate() {
        if c != b'1' || (i > 0 && bytes[i - 1].is_ascii_digit()) {
            continue;
        }
        let mut j = i + 1;
        if bytes.get(j) == Some(&b' ') {
            j += 1;
        }
        if bytes.get(j) != Some(&b'.') || bytes.get(j + 1) == Some(&b'.') {
            continue;
        }
        j += 1;
        if bytes.get(j) == Some(&b' ') {
            j += 1;
        }
        if bytes.get(j).is_some_and(u8::is_ascii_alphanumeric) {
            out.push(i);
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    /// A move number; `true` when it introduces Black's move (`12...`).
    Number(u32, bool),
    Move(String),
    /// Text that cannot be split safely; the stream ends here.
    Break,
}

/// Move numbers and move words, with parenthesised and bracketed text dropped.
fn tokens(text: &str) -> Vec<Token> {
    let mut flat = String::with_capacity(text.len());
    let mut depth = 0usize;
    let mut previous_digit = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' | '[' => {
                depth += 1;
                flat.push(' ');
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                flat.push(' ');
            }
            // "21 .Bf1": a space read between a move number and its dot.
            ' ' if depth == 0 && previous_digit && chars.peek() == Some(&'.') => {}
            _ if depth == 0 => flat.push(c),
            _ => {}
        }
        previous_digit = c.is_ascii_digit();
    }
    let mut out = Vec::new();
    for word in flat.split_whitespace() {
        let mut rest = word;
        // "2. ... Nf6": an ellipsis set apart from its move number makes it Black's.
        let after_dots = rest.trim_start_matches(['.', '\u{2026}']);
        let dots = rest.len() - after_dots.len();
        if let Some(Token::Number(number, false)) = out.last()
            && (rest.starts_with('\u{2026}') || dots >= 2)
        {
            let number = *number;
            out.pop();
            out.push(Token::Number(number, true));
            rest = after_dots;
        }
        while !rest.is_empty() {
            if let Some((number, black, after)) = move_number(rest) {
                out.push(Token::Number(number, black));
                rest = after;
                continue;
            }
            if let Some((mv, after)) = glued(rest) {
                out.push(Token::Move(clean(mv)));
                rest = after;
                continue;
            }
            if rest
                .char_indices()
                .skip(1)
                .any(|(i, _)| move_number(&rest[i..]).is_some())
            {
                out.push(Token::Break);
            } else {
                out.push(Token::Move(clean(rest)));
            }
            break;
        }
    }
    out
}

/// `12.`, `12...` or `12…` at the start of `text`.
fn move_number(text: &str) -> Option<(u32, bool, &str)> {
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let number: u32 = text[..digits].parse().ok()?;
    let rest = &text[digits..];
    if let Some(after) = rest.strip_prefix("...") {
        Some((number, true, after))
    } else if let Some(after) = rest.strip_prefix('\u{2026}') {
        Some((number, true, after))
    } else {
        rest.strip_prefix('.').map(|after| (number, false, after))
    }
}

/// A move glued to the next move number, as in `Nb47.Nd4`: the move ends on a rank digit or a
/// castling sign.
fn glued(text: &str) -> Option<(&str, &str)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (k, &(i, c)) in chars.iter().enumerate().skip(1) {
        let (_, before) = chars[k - 1];
        let ends_square = ('1'..='8').contains(&before)
            && k >= 2
            && matches!(chars[k - 2].1, 'a'..='h' | 'q' | '9');
        let ends_castle = matches!(before, 'O' | '0') && k >= 3 && chars[k - 2].1 == '-';
        if (ends_square || ends_castle) && c.is_ascii_digit() && move_number(&text[i..]).is_some() {
            return Some((&text[..i], &text[i..]));
        }
    }
    None
}

/// The move word without check signs, evaluation glyphs or trailing punctuation.
fn clean(word: &str) -> String {
    let mut word = word.trim().to_owned();
    for suffix in ["+=", "=+", "-+", "+-", "+/-", "-/+", "oo", "co", "∞"] {
        if let Some(stripped) = word.strip_suffix(suffix) {
            word = stripped.to_owned();
        }
    }
    word.trim_end_matches(|c: char| "!?+#±∓∞;:,.=".contains(c))
        .to_owned()
}

/// What a printed character may stand for after OCR, beyond itself.
fn confusable(printed: char, real: char) -> bool {
    printed == real
        || match printed {
            '0' | 'O' => matches!(real, 'O' | 'Q'),
            'Q' => real == 'O',
            '2' | 'Z' => real == 'Q',
            'q' | '9' => real == 'g',
            'l' | 'I' | '|' | 'i' => real == '1',
            'S' => real == '5',
            'B' => real == '8',
            '8' => real == 'B',
            'C' => real == 'c',
            _ => false,
        }
}

fn fits(printed: &[char], real: &[char]) -> bool {
    printed.len() == real.len() && printed.iter().zip(real).all(|(&p, &r)| confusable(p, r))
}

/// How books print `mv`: SAN, SAN without `x`, and a pawn capture as two files (`dc`).
fn spellings(pos: &Chess, mv: Move) -> Vec<Vec<char>> {
    let san = San::from_move(pos, mv).to_string();
    let mut out = vec![san.chars().collect::<Vec<_>>()];
    if san.contains('x') {
        out.push(san.chars().filter(|&c| c != 'x').collect());
    }
    if mv.role() == Role::Pawn
        && mv.is_capture()
        && let Some(from) = mv.from()
    {
        let mut short = vec![from.file().char(), mv.to().file().char()];
        if let Some(promotion) = mv.promotion() {
            short.push('=');
            short.push(promotion.upper_char());
        }
        out.push(short);
    }
    out
}

/// Legal moves that `word` may name. One dropped character is tried only when nothing fits.
fn candidates(pos: &Chess, word: &str) -> Vec<Move> {
    let printed: Vec<char> = word.chars().collect();
    if printed.is_empty() {
        return Vec::new();
    }
    let all: Vec<(Move, Vec<Vec<char>>)> = pos
        .legal_moves()
        .into_iter()
        .map(|mv| {
            let spelled = spellings(pos, mv);
            (mv, spelled)
        })
        .collect();
    let exact: Vec<Move> = all
        .iter()
        .filter(|(_, spelled)| spelled.iter().any(|real| fits(&printed, real)))
        .map(|(mv, _)| *mv)
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    all.iter()
        .filter(|(_, spelled)| {
            spelled.iter().any(|real| {
                real.len() == printed.len() + 1
                    && (0..real.len()).any(|skip| {
                        let shorter: Vec<char> = real
                            .iter()
                            .enumerate()
                            .filter(|&(i, _)| i != skip)
                            .map(|(_, &c)| c)
                            .collect();
                        fits(&printed, &shorter)
                    })
            })
        })
        .map(|(mv, _)| *mv)
        .collect()
}

/// Placements after each replayed ply, with `true` when White is to move there.
fn replay(tokens: &[Token]) -> Vec<(String, bool)> {
    let mut beams = vec![Chess::default()];
    let mut ply = 0u32;
    let mut started = false;
    let mut out = Vec::new();
    for token in tokens {
        match token {
            Token::Break => break,
            Token::Number(number, black) => {
                if !started {
                    if *number != 1 || *black {
                        return out;
                    }
                    started = true;
                }
                let Some(expected) = number.checked_sub(1).map(|n| 2 * n + u32::from(*black))
                else {
                    break;
                };
                if expected != ply {
                    break;
                }
            }
            Token::Move(word) => {
                if !started || word.is_empty() {
                    continue;
                }
                if word == "ECO" {
                    break;
                }
                let mut next: Vec<Chess> = Vec::new();
                for pos in &beams {
                    for mv in candidates(pos, word) {
                        let mut after = pos.clone();
                        after.play_unchecked(mv);
                        if !next.iter().any(|seen| seen.board() == after.board()) {
                            next.push(after);
                        }
                    }
                }
                if next.is_empty() || next.len() > BEAM_MAX {
                    break;
                }
                beams = next;
                ply += 1;
                let white = ply.is_multiple_of(2);
                for pos in &beams {
                    out.push((pos.board().to_string(), white));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, left: f64, top: f64, right: f64) -> OcrLine {
        OcrLine {
            text: text.to_owned(),
            confidence: 0.95,
            left,
            top,
            right,
            bottom: top + 20.0,
        }
    }

    fn side_of(found: &[(String, bool)], placement: &str) -> Option<bool> {
        found
            .iter()
            .find(|(p, _)| p == placement)
            .map(|&(_, white)| white)
    }

    const AFTER_E4_E5_NF3: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R";

    #[test]
    fn a_game_line_certifies_the_side_by_its_ply() {
        let lines = [line("1.e4 e5 2.Nf3", 50.0, 100.0, 400.0)];
        let found = replayed_placements(&lines, &[], 1000);
        assert_eq!(side_of(&found, AFTER_E4_E5_NF3), Some(false));
    }

    #[test]
    fn a_game_continues_across_lines_and_columns() {
        let lines = [
            line("1.e4 e5", 50.0, 1200.0, 400.0),
            line("2.Nf3 Nc6", 550.0, 100.0, 900.0),
        ];
        let found = replayed_placements(&lines, &[], 1000);
        assert_eq!(side_of(&found, AFTER_E4_E5_NF3), Some(false));
        assert!(found.iter().any(|(_, white)| *white));
    }

    #[test]
    fn ocr_confusions_and_book_spellings_are_read() {
        // 0-0 for O-O, q for g, two-file pawn capture, x left out.
        let lines = [line(
            "1.e4 d5 2.ed Qd5 3.Nc3 Qa5 4.Nf3 Nf6 5.Bc4 Bq4 6.0-0",
            0.0,
            0.0,
            900.0,
        )];
        let found = replayed_placements(&lines, &[], 1000);
        assert!(
            found
                .iter()
                .any(|(p, w)| !*w && p.ends_with("PPPP1PPP/R1BQ1RK1"))
        );
    }

    #[test]
    fn a_wrong_move_number_ends_the_stream() {
        let lines = [line("1.e4 e5 3.Nf3", 0.0, 0.0, 900.0)];
        let found = replayed_placements(&lines, &[], 1000);
        assert_eq!(side_of(&found, AFTER_E4_E5_NF3), None);
    }

    #[test]
    fn a_line_that_does_not_open_at_move_one_is_ignored() {
        let lines = [line("11.e4 e5 12.Nf3", 0.0, 0.0, 900.0)];
        assert_eq!(replayed_placements(&lines, &[], 1000), Vec::new());
    }

    #[test]
    fn variations_in_parentheses_are_skipped() {
        let lines = [line("1.e4 e5 (1...c5 2.Nf3) 2.Nf3", 0.0, 0.0, 900.0)];
        let found = replayed_placements(&lines, &[], 1000);
        assert_eq!(side_of(&found, AFTER_E4_E5_NF3), Some(false));
    }

    #[test]
    fn a_placement_reached_with_both_sides_to_move_is_dropped() {
        // The white king walks a triangle, so the board after 2.Ke2 recurs with White to move.
        let lines = [line(
            "1.e4 e5 2.Ke2 Nc6 3.Kd3 Nb8 4.Ke3 Nc6 5.Ke2 Nb8",
            0.0,
            0.0,
            900.0,
        )];
        let found = replayed_placements(&lines, &[], 1000);
        let after_ke2 = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPPKPPP/RNBQ1BNR";
        assert_eq!(side_of(&found, after_ke2), None);
        let after_e5 = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR";
        assert_eq!(side_of(&found, after_e5), Some(true));
    }

    #[test]
    fn lines_inside_a_board_frame_are_not_text() {
        let lines = [line("1.e4 e5 2.Nf3", 50.0, 100.0, 400.0)];
        let found = replayed_placements(&lines, &[[0, 0, 500, 500]], 1000);
        assert_eq!(found, Vec::new());
    }

    #[test]
    fn an_ellipsis_set_apart_from_its_number_is_black() {
        let spaced = tokens("1.e4 e5 2.Nf3 2. ... Nc6 3. \u{2026}Bb4");
        assert_eq!(spaced[5], Token::Number(2, true));
        assert_eq!(spaced[6], Token::Move("Nc6".to_owned()));
        assert_eq!(spaced[7], Token::Number(3, true));
        assert_eq!(spaced[8], Token::Move("Bb4".to_owned()));
        let found = replay(&tokens("1.e4 e5 2.Nf3 2. ... Nc6 3.Bb5"));
        assert_eq!(found.len(), 5);
        assert_eq!(
            side_of(
                &found,
                "r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R"
            ),
            Some(false)
        );
    }

    #[test]
    fn glued_numbers_split_on_the_rank_digit() {
        assert_eq!(
            tokens("Nb47.Nd4"),
            vec![
                Token::Move("Nb4".into()),
                Token::Number(7, false),
                Token::Move("Nd4".into())
            ]
        );
        assert_eq!(tokens("21 .Bf1")[0], Token::Number(21, false));
    }
}
