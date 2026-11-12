//! A board's solution printed as the first lines right under it, in the board's own column:
//! "Name – Name (1957): 1 … Qb1+ 2 Bc1". Only a line that opens with move 1 is read. The
//! mover ("1 …" is Black) and the move are stored beside the pixels; the resolver keeps the
//! side only when nothing else named one and the move is legal for exactly that side.

use super::caption::{Rect, TextBox};

/// Text may start this many board heights under the board's bottom edge.
const BELOW_MAX: f64 = 0.45;
/// Text may start this many board heights above the board's bottom edge (boxes overlap).
const BELOW_SLACK: f64 = 0.02;
/// The column starts this many board widths left of the board.
const COLUMN_BEFORE: f64 = 0.3;
/// The column ends this many board widths right of the board's left edge.
const COLUMN_AFTER: f64 = 1.25;
/// Only the first lines under the board are read.
const LINES: usize = 3;
/// Words whose bottoms differ by less than this many word heights share a line.
const SAME_LINE: f64 = 0.35;

/// The column window under `board`, cut at the next picture down in it.
fn window(index: usize, boards: &[Rect]) -> Rect {
    let board = boards[index];
    let width = (board.right - board.left).max(1.0);
    let height = (board.top - board.bottom).max(1.0);
    let left = board.left - COLUMN_BEFORE * width;
    let right = board.left + COLUMN_AFTER * width;
    let top = board.bottom + BELOW_SLACK * height;
    let floor = boards
        .iter()
        .enumerate()
        .filter(|(at, other)| {
            *at != index && other.top < top && other.right > left && other.left < right
        })
        .map(|(_, other)| other.top)
        .fold(board.bottom - BELOW_MAX * height, f64::max);
    Rect {
        left,
        bottom: floor,
        right,
        top,
    }
}

fn inside(window: Rect, word: &TextBox) -> bool {
    word.rect.left >= window.left
        && word.rect.left < window.right
        && word.rect.top <= window.top
        && word.rect.top >= window.bottom
}

fn word_height(word: &TextBox) -> f64 {
    (word.rect.top - word.rect.bottom).max(1.0)
}

/// The first [`LINES`] lines in `window`, top to bottom, each with its words left to right.
fn first_lines(window: Rect, words: &[TextBox]) -> Vec<Vec<&TextBox>> {
    let mut inside_words: Vec<&TextBox> = words.iter().filter(|w| inside(window, w)).collect();
    inside_words.sort_by(|a, b| b.rect.bottom.total_cmp(&a.rect.bottom));
    let mut lines: Vec<Vec<&TextBox>> = Vec::new();
    for word in inside_words {
        let joins = lines.last().is_some_and(|line| {
            let anchor = line[0];
            let h = word_height(anchor).min(word_height(word));
            (anchor.rect.bottom - word.rect.bottom).abs() <= SAME_LINE * h
        });
        if joins {
            lines.last_mut().expect("line").push(word);
        } else {
            lines.push(vec![word]);
        }
    }
    lines.truncate(LINES);
    for line in &mut lines {
        line.sort_by(|a, b| a.rect.left.total_cmp(&b.rect.left));
    }
    lines
}

/// One move per board: `Some((true, "Qg3"))` when the first line under it opens with White's
/// move 1. A line that also sits under another board leaves both open.
pub(super) fn bind_solution_lines(
    diagrams: &[Rect],
    words: &[TextBox],
) -> Vec<Option<(bool, String)>> {
    let windows: Vec<Rect> = (0..diagrams.len()).map(|i| window(i, diagrams)).collect();
    windows
        .iter()
        .enumerate()
        .map(|(index, own)| {
            let lines = first_lines(*own, words);
            let shared = lines.iter().flatten().any(|word| {
                windows
                    .iter()
                    .enumerate()
                    .any(|(other, w)| other != index && inside(*w, word))
            });
            if shared {
                return None;
            }
            let text = lines
                .iter()
                .map(|line| {
                    line.iter()
                        .map(|w| w.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join(" ");
            first_move(&text).and_then(|(number, white, san)| (number == 1).then_some((white, san)))
        })
        .collect()
}

/// Figurines become piece letters, a lone piece letter joins the square after it, and an
/// ellipsis becomes three dots.
fn tokens(text: &str) -> Vec<String> {
    let mapped: String = text
        .chars()
        .flat_map(|c| {
            let piece = match c {
                '\u{2654}' | '\u{265A}' => Some('K'),
                '\u{2655}' | '\u{265B}' => Some('Q'),
                '\u{2656}' | '\u{265C}' => Some('R'),
                '\u{2657}' | '\u{265D}' => Some('B'),
                '\u{2658}' | '\u{265E}' => Some('N'),
                _ => None,
            };
            match (piece, c) {
                (Some(p), _) => vec![p],
                (None, '\u{2659}' | '\u{265F}') => vec![],
                (None, '\u{2026}') => vec!['.', '.', '.'],
                (None, '\u{00D7}') => vec!['x'],
                (None, '\u{2013}' | '\u{2014}' | '\u{2212}') => vec!['-'],
                (None, c) => vec![c],
            }
        })
        .collect();
    // "1..." and "1.e4" glue the number to what follows; split them apart.
    let mut raw: Vec<String> = Vec::new();
    for word in mapped.split_whitespace() {
        let digits = word.chars().take_while(char::is_ascii_digit).count();
        let decimal = digits > 0
            && word[digits..].starts_with('.')
            && word[digits + 1..].starts_with(|c: char| c.is_ascii_digit());
        if decimal {
            raw.push(word.to_owned());
        } else if digits > 0 && digits < word.len() && word[digits..].starts_with('.') {
            let rest = &word[digits..];
            let dots = rest.chars().take_while(|&c| c == '.').count();
            raw.push(word[..digits].to_owned());
            raw.push(rest[..dots].to_owned());
            if dots < rest.len() {
                raw.push(rest[dots..].to_owned());
            }
        } else if word.chars().all(|c| c == '.') && word.len() > 1 {
            raw.push(word.to_owned());
        } else if let Some(stripped) = word.strip_prefix("...")
            && !stripped.is_empty()
        {
            raw.push("...".to_owned());
            raw.push(stripped.to_owned());
        } else {
            raw.push(word.to_owned());
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let word = &raw[i];
        let lone = matches!(word.as_str(), "K" | "Q" | "R" | "B" | "N");
        if lone
            && let Some(next) = raw.get(i + 1)
            && next
                .trim_start_matches('x')
                .starts_with(|c: char| ('a'..='h').contains(&c))
        {
            out.push(format!("{word}{next}"));
            i += 2;
            continue;
        }
        out.push(word.clone());
        i += 1;
    }
    out
}

/// A move as printed, without check, mate or annotation marks; `None` when it is not SAN.
fn san(word: &str) -> Option<String> {
    let bare = word.trim_end_matches(|c: char| "!?+#X,.;:)".contains(c));
    let castle = bare.replace('0', "O");
    if castle == "O-O" || castle == "O-O-O" {
        return Some(castle);
    }
    let bytes = bare.as_bytes();
    let mut at = 0;
    if bytes.first().is_some_and(|b| b"KQRBN".contains(b)) {
        at += 1;
    }
    let body = &bare[at..];
    let (body, promotion) = match body.split_once('=') {
        Some((body, piece)) if matches!(piece, "Q" | "R" | "B" | "N") => (body, Some(piece)),
        Some(_) => return None,
        None => (body, None),
    };
    let chars: Vec<char> = body.chars().collect();
    let n = chars.len();
    if n < 2 || !('a'..='h').contains(&chars[n - 2]) || !('1'..='8').contains(&chars[n - 1]) {
        return None;
    }
    let mut prefix = &chars[..n - 2];
    if prefix.last() == Some(&'x') {
        prefix = &prefix[..prefix.len() - 1];
    }
    let ok = match prefix {
        [] => true,
        [c] => ('a'..='h').contains(c) || ('1'..='8').contains(c),
        [f, r] => ('a'..='h').contains(f) && ('1'..='8').contains(r),
        _ => false,
    };
    if !ok || (promotion.is_some() && at == 1) {
        return None;
    }
    Some(bare.to_owned())
}

fn ellipsis(word: &str) -> bool {
    word == "..." || word == ". . ."
}

/// The first numbered move in `text`: its number, `true` for White, and the move.
/// "1 …" / "1..." / "1. ..." name Black; "1" / "1." name White.
pub(super) fn first_move(text: &str) -> Option<(u32, bool, String)> {
    let words = tokens(text);
    for (i, word) in words.iter().enumerate() {
        if word.is_empty() || word.len() > 3 || !word.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // "1.5", "64." page numbers and dates glued to dots are not move numbers.
        if i > 0 && words[i - 1].ends_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let Ok(number) = word.parse::<u32>() else {
            continue;
        };
        let at = |k: usize, text: &str| words.get(k).is_some_and(|w| w == text);
        let dots = |k: usize| -> usize {
            if words.get(k).is_some_and(|w| ellipsis(w)) {
                1
            } else if at(k, ".") && at(k + 1, ".") && at(k + 2, ".") {
                3
            } else if at(k, "..") && at(k + 1, ".") {
                2
            } else {
                0
            }
        };
        let (white, j) = match (dots(i + 1), at(i + 1, ".")) {
            (0, true) => match dots(i + 2) {
                0 => (true, i + 2),
                n => (false, i + 2 + n),
            },
            (0, false) => (true, i + 1),
            (n, _) => (false, i + 1 + n),
        };
        if let Some(found) = words.get(j).and_then(|w| san(w)) {
            return Some((number, white, found));
        }
    }
    None
}

/// Stamp the first move printed under each board onto its picture. Only pages with a poppler
/// text layer take part; a missing `pdftotext` is a no-op.
pub(super) fn stamp_solution_lines(
    pdf: &std::path::Path,
    pictures: &mut [Vec<super::picture::Placed>],
    cover: Option<&[u8]>,
) {
    if pictures.iter().all(Vec::is_empty) {
        return;
    }
    let Some(poppler) = super::caption::bbox_document(pdf) else {
        return;
    };
    for (page, placed) in pictures.iter_mut().enumerate() {
        let Some(words) = poppler.get(page).filter(|words| !words.is_empty()) else {
            continue;
        };
        if placed.is_empty() {
            continue;
        }
        let diagrams: Vec<Rect> = placed
            .iter()
            .map(|picture| Rect {
                left: picture.left,
                bottom: picture.bottom,
                right: picture.right,
                top: picture.top,
            })
            .collect();
        let moves = bind_solution_lines(&diagrams, words);
        for (picture, found) in placed.iter_mut().zip(moves) {
            let Some((white, san)) = found else {
                continue;
            };
            if cover.is_some_and(|bytes| bytes == picture.bytes.as_slice()) {
                continue;
            }
            let Some(png) = super::pdf::diagram_png(&picture.bytes) else {
                continue;
            };
            picture.bytes = crate::books::diagram::with_solution_move(&png, white, &san);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Rect, TextBox, bind_solution_lines, first_move};

    fn rect(left: f64, bottom: f64, right: f64, top: f64) -> Rect {
        Rect {
            left,
            bottom,
            right,
            top,
        }
    }

    fn word(text: &str, left: f64, bottom: f64) -> TextBox {
        let width = 6.0 * text.chars().count() as f64;
        TextBox {
            text: text.into(),
            rect: rect(left, bottom, left + width, bottom + 12.0),
        }
    }

    fn line(text: &str, left: f64, bottom: f64) -> Vec<TextBox> {
        let mut x = left;
        text.split(' ')
            .map(|w| {
                let out = word(w, x, bottom);
                x = out.rect.right + 3.0;
                out
            })
            .collect()
    }

    #[test]
    fn move_one_names_white_and_an_ellipsis_names_black() {
        assert_eq!(first_move("1 Qg3!"), Some((1, true, "Qg3".into())));
        assert_eq!(first_move("1. e4 e5"), Some((1, true, "e4".into())));
        assert_eq!(first_move("1.Nf3+"), Some((1, true, "Nf3".into())));
        assert_eq!(
            first_move("Aaa – Bbb (1957): 1 … Qb1+ 2 Bc1"),
            Some((1, false, "Qb1".into()))
        );
        assert_eq!(first_move("1 ... Nxe3!"), Some((1, false, "Nxe3".into())));
        assert_eq!(first_move("1... Rd1!!"), Some((1, false, "Rd1".into())));
        assert_eq!(first_move("1…Rd1"), Some((1, false, "Rd1".into())));
        assert_eq!(first_move("1. ... Kh8"), Some((1, false, "Kh8".into())));
        assert_eq!(first_move("1 . . . Kh8"), Some((1, false, "Kh8".into())));
        assert_eq!(first_move("1 d8=Q+"), Some((1, true, "d8=Q".into())));
        assert_eq!(first_move("1 0-0-0"), Some((1, true, "O-O-O".into())));
    }

    #[test]
    fn figurines_and_split_piece_letters_join_the_square() {
        assert_eq!(first_move("1 \u{2655}g3!"), Some((1, true, "Qg3".into())));
        assert_eq!(
            first_move("1 … \u{265E} xe3"),
            Some((1, false, "Nxe3".into()))
        );
        assert_eq!(
            first_move("1  …   N xe3! 2 fxe3"),
            Some((1, false, "Nxe3".into()))
        );
        assert_eq!(
            first_move("1 R b8! The rook"),
            Some((1, true, "Rb8".into()))
        );
        assert_eq!(first_move("1 \u{2659}e4"), Some((1, true, "e4".into())));
    }

    #[test]
    fn later_numbers_and_years_are_not_move_one() {
        assert_eq!(
            first_move("Here (1934) 12 Qg3"),
            Some((12, true, "Qg3".into()))
        );
        assert_eq!(first_move("Page 1634 and nothing else"), None);
        assert_eq!(
            first_move("1 … The lightning strike 2 Qe6+!"),
            Some((2, true, "Qe6".into()))
        );
        assert_eq!(first_move("1.5 Nf3"), None);
        assert_eq!(first_move("No moves here."), None);
    }

    #[test]
    fn the_first_line_under_a_board_binds_to_it() {
        let boards = [rect(100.0, 500.0, 250.0, 650.0)];
        let mut words = line("Smith – Jones (1950): 1 … Qb1+ 2 Bc1", 90.0, 470.0);
        words.extend(line("Black wins.", 90.0, 455.0));
        assert_eq!(
            bind_solution_lines(&boards, &words),
            vec![Some((false, "Qb1".into()))]
        );
    }

    #[test]
    fn a_line_under_two_boards_or_not_move_one_abstains() {
        // Two boards side by side whose columns both hold the line.
        let boards = [
            rect(100.0, 500.0, 250.0, 650.0),
            rect(200.0, 500.0, 350.0, 650.0),
        ];
        let words = line("1 Qg3!", 210.0, 470.0);
        assert_eq!(bind_solution_lines(&boards, &words), vec![None, None]);
        // Move 2 is a line from inside the game, not the board's first move.
        let boards = [rect(100.0, 500.0, 250.0, 650.0)];
        let words = line("2 Qe6+! fxe6", 100.0, 470.0);
        assert_eq!(bind_solution_lines(&boards, &words), vec![None]);
        // Text far below the board, or in another column, is not read.
        let words = line("1 Qg3!", 100.0, 300.0);
        assert_eq!(bind_solution_lines(&boards, &words), vec![None]);
        let words = line("1 Qg3!", 400.0, 470.0);
        assert_eq!(bind_solution_lines(&boards, &words), vec![None]);
    }

    #[test]
    fn the_next_board_down_ends_the_window() {
        let boards = [
            rect(100.0, 500.0, 250.0, 650.0),
            rect(100.0, 440.0, 250.0, 495.0),
        ];
        // The line sits under the second board, not the first.
        let words = line("1 Qg3!", 100.0, 420.0);
        assert_eq!(bind_solution_lines(&boards, &words)[0], None);
    }
}
