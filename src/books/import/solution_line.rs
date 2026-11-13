//! A board's solution printed as the first lines right under it, in the board's own column:
//! "Name – Name (1957): 1 … Qb1+ 2 Bc1". Only a line that opens with move 1 is read. The
//! mover ("1 …" is Black) and the move are stored beside the pixels; the resolver keeps the
//! side only when nothing else named one and the move is legal for exactly that side.
//! Whether the lines read as that one solution is stored too: nothing numbered comes before
//! move 1, White's move 1 never follows Black's, and the numbering never skips ahead. Only such
//! a solution may correct a side read from moves elsewhere in the text. Text around the board that
//! poses the position for both sides ("Whichever side is to move", "White to play … If Black
//! is to move") stores nothing.

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
/// Text this many board heights under the board's bottom edge may still pose its position.
const POSED_BELOW_MAX: f64 = 1.0;
/// Text this many board heights above the board's top edge may still pose its position.
const ABOVE_MAX: f64 = 0.45;

/// The column window under `board`, cut at the next picture down in it.
fn window(index: usize, boards: &[Rect]) -> Rect {
    window_reaching(index, boards, BELOW_MAX)
}

/// The column window reaching `reach` board heights under `board`, cut at the next picture down.
fn window_reaching(index: usize, boards: &[Rect], reach: f64) -> Rect {
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
        .fold(board.bottom - reach * height, f64::max);
    Rect {
        left,
        bottom: floor,
        right,
        top,
    }
}

/// The column window over `board`, cut at the next picture up in it.
fn window_above(index: usize, boards: &[Rect]) -> Rect {
    let board = boards[index];
    let width = (board.right - board.left).max(1.0);
    let height = (board.top - board.bottom).max(1.0);
    let left = board.left - COLUMN_BEFORE * width;
    let right = board.left + COLUMN_AFTER * width;
    let bottom = board.top - BELOW_SLACK * height;
    let ceiling = boards
        .iter()
        .enumerate()
        .filter(|(at, other)| {
            *at != index && other.bottom > bottom && other.right > left && other.left < right
        })
        .map(|(_, other)| other.bottom)
        .fold(board.top + ABOVE_MAX * height, f64::min);
    Rect {
        left,
        bottom,
        right,
        top: ceiling,
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
    let mut lines = lines_in(window, words);
    lines.truncate(LINES);
    lines
}

/// Every line in `window`, top to bottom, each with its words left to right.
fn lines_in(window: Rect, words: &[TextBox]) -> Vec<Vec<&TextBox>> {
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
    for line in &mut lines {
        line.sort_by(|a, b| a.rect.left.total_cmp(&b.rect.left));
    }
    lines
}

/// The first move printed under a board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SolutionLine {
    /// `true` when move 1 is White's.
    pub white: bool,
    pub san: String,
    /// The numbered moves in the lines read as this one solution.
    pub single: bool,
}

fn joined(lines: &[Vec<&TextBox>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One move per board, when the first line under it opens with move 1. A line that also sits
/// under another board leaves both open, and so does text around the board that poses the
/// position for both sides. Past a board's own lines, text that also sits around another board
/// is that board's.
pub(super) fn bind_solution_lines(
    diagrams: &[Rect],
    words: &[TextBox],
) -> Vec<Option<SolutionLine>> {
    let windows: Vec<Rect> = (0..diagrams.len()).map(|i| window(i, diagrams)).collect();
    let above: Vec<Rect> = (0..diagrams.len())
        .map(|i| window_above(i, diagrams))
        .collect();
    let far: Vec<Rect> = (0..diagrams.len())
        .map(|i| window_reaching(i, diagrams, POSED_BELOW_MAX))
        .collect();
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
            let text = joined(&lines);
            let (number, white, san) = first_move(&text)?;
            if number != 1 {
                return None;
            }
            // Beyond this board's own lines, text that also sits around another board is that
            // board's, not this one's.
            let theirs = |word: &TextBox| {
                !inside(*own, word)
                    && (0..diagrams.len()).any(|other| {
                        other != index
                            && (inside(windows[other], word)
                                || inside(above[other], word)
                                || inside(far[other], word))
                    })
            };
            let mine: Vec<TextBox> = words.iter().filter(|w| !theirs(w)).cloned().collect();
            let around = format!(
                "{} {}",
                joined(&lines_in(above[index], &mine)),
                joined(&lines_in(far[index], &mine))
            );
            if poses_both_sides(&around) {
                return None;
            }
            let single = single_line(&numbered_moves(&text), white, &san);
            Some(SolutionLine { white, san, single })
        })
        .collect()
}

/// `true` when `moves` read as one solution opening with `san`: it comes first, no White move 1
/// follows Black's, and no move number is more than one past the one before it.
fn single_line(moves: &[(u32, bool, String)], white: bool, san: &str) -> bool {
    let Some(((1, first_white, first), rest)) = moves.split_first() else {
        return false;
    };
    if *first_white != white || first != san {
        return false;
    }
    let mut last = 1;
    for (number, mover, _) in rest {
        if *number > last + 1 || (!white && *number == 1 && *mover) {
            return false;
        }
        last = *number;
    }
    true
}

/// Phrases that pose a position for either side to move.
const EITHER_SIDE: &[&str] = &[
    "whichever side",
    "either side to move",
    "either side to play",
    "whoever is to move",
    "whoever moves",
    "no matter who moves",
    "no matter which side",
];

/// `true` when `text` poses the position for both sides: "Whichever side is to move", or a turn
/// phrase for White and another for Black ("White to play …  If Black is to move …").
fn poses_both_sides(text: &str) -> bool {
    let lower: String = text
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if EITHER_SIDE.iter().any(|phrase| lower.contains(phrase)) {
        return true;
    }
    let words: Vec<&str> = lower
        .split(' ')
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\''))
        .collect();
    let mut white = false;
    let mut black = false;
    for (i, word) in words.iter().enumerate() {
        let side = match *word {
            "white" => &mut white,
            "black" => &mut black,
            _ => continue,
        };
        // "White to play", "Black is to move", "White, to move,".
        let rest: Vec<&str> = words[i + 1..].iter().take(3).copied().collect();
        let turn = match rest.as_slice() {
            ["to", verb, ..] | ["is", "to", verb, ..] => matches!(*verb, "play" | "move"),
            _ => false,
        };
        if turn {
            *side = true;
        }
    }
    white && black
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
    numbered(text, san).into_iter().next()
}

/// Every numbered move in `text`, promotions written without `=` ("g8N") included.
fn numbered_moves(text: &str) -> Vec<(u32, bool, String)> {
    numbered(text, |word| {
        san(word).or_else(|| {
            let bare = word.trim_end_matches(|c: char| "!?+#X,.;:)".contains(c));
            let chars: Vec<char> = bare.chars().collect();
            let promotes = matches!(
                chars.as_slice(),
                [file, rank, piece] | [_, 'x', file, rank, piece]
                    if ('a'..='h').contains(file)
                        && matches!(*rank, '1' | '8')
                        && "QRBN".contains(*piece)
            );
            promotes.then(|| {
                let (body, piece) = bare.split_at(bare.len() - 1);
                format!("{body}={piece}")
            })
        })
    })
}

fn numbered(text: &str, read: impl Fn(&str) -> Option<String>) -> Vec<(u32, bool, String)> {
    let mut out = Vec::new();
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
        if let Some(found) = words.get(j).and_then(|w| read(w)) {
            out.push((number, white, found));
        }
    }
    out
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
            let Some(line) = found else {
                continue;
            };
            if cover.is_some_and(|bytes| bytes == picture.bytes.as_slice()) {
                continue;
            }
            let Some(png) = super::pdf::diagram_png(&picture.bytes) else {
                continue;
            };
            let png = crate::books::diagram::with_solution_move(&png, line.white, &line.san);
            picture.bytes = if line.single {
                crate::books::diagram::with_single_solution(&png)
            } else {
                png
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Rect, SolutionLine, TextBox, bind_solution_lines, first_move, poses_both_sides};

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
            vec![Some(SolutionLine {
                white: false,
                san: "Qb1".into(),
                single: true,
            })]
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
    fn only_lines_that_read_as_one_solution_are_single() {
        let boards = [rect(100.0, 500.0, 250.0, 650.0)];
        // Each half of the text on its own line, so that long texts fit the column.
        let single = |text: &str| {
            let words: Vec<&str> = text.split(' ').collect();
            let (first, second) = words.split_at(words.len().div_ceil(2));
            let mut lines = line(&first.join(" "), 90.0, 470.0);
            lines.extend(line(&second.join(" "), 90.0, 455.0));
            bind_solution_lines(&boards, &lines)[0]
                .as_ref()
                .map(|found| found.single)
        };
        assert_eq!(
            single("Aaa – Bbb (1957): 1 … Qb1+ 2 Bc1 Qxc1+ 3 Kxc1"),
            Some(true)
        );
        assert_eq!(single("1 Qxf6! gxf6 (1 … Rxd1 2 Qxg7+) 2 Bh4"), Some(true));
        // A promotion the move reader skips still counts as White's move 1 first.
        assert_eq!(single("If we play 1 g8N then 1 … g4! 2 Qe8"), Some(false));
        // White's move 1 after Black's.
        assert_eq!(single("1 … Ke8! or 1 Kd6 Kd8"), Some(false));
        // The text looks back from later in the game.
        assert_eq!(
            single("We see why 1 Rg3!! was needed. After 6 … Kd8 7 Kf7"),
            Some(false)
        );
    }

    #[test]
    fn a_position_posed_for_both_sides_stores_nothing() {
        let boards = [rect(100.0, 500.0, 250.0, 650.0)];
        // Over the board.
        let mut words = line("Whichever side is to move, it is a draw.", 90.0, 680.0);
        words.extend(line("1 … Ke8! 2 Kd6 Kd8", 90.0, 470.0));
        assert_eq!(bind_solution_lines(&boards, &words), vec![None]);
        // Under it, over several lines.
        let mut words = line("White to play can win by 1 Ba3+.", 90.0, 470.0);
        words.extend(line("Black's King must move.", 90.0, 455.0));
        words.extend(line("If Black is to", 90.0, 440.0));
        words.extend(line("move he plays 1 … Bb7+.", 90.0, 425.0));
        assert_eq!(bind_solution_lines(&boards, &words), vec![None]);
        // The other side's paragraph may sit further down.
        let mut words = line("White to play can win by 1 Ba3+.", 90.0, 470.0);
        words.extend(line("If Black is to move he plays 1 … Bb7+.", 90.0, 380.0));
        assert_eq!(bind_solution_lines(&boards, &words), vec![None]);
        // One side only is a solution.
        let words = line("White to play wins by 1 Ba3+.", 90.0, 470.0);
        assert_eq!(
            bind_solution_lines(&boards, &words),
            vec![Some(SolutionLine {
                white: true,
                san: "Ba3".into(),
                single: true,
            })]
        );
        assert!(!poses_both_sides("White resigned. Black to move."));
        assert!(poses_both_sides(
            "With Black to play 1 … Ke8. With White to move, however"
        ));
    }

    #[test]
    fn text_under_another_board_does_not_pose_this_one() {
        let boards = [
            rect(100.0, 500.0, 250.0, 650.0),
            rect(100.0, 250.0, 250.0, 400.0),
        ];
        // The upper board's solution sits over the lower board.
        let mut words = line("Black to play: 1 … Qb1", 90.0, 470.0);
        words.extend(line("White to play wins by 1 Ba3+.", 90.0, 220.0));
        assert_eq!(
            bind_solution_lines(&boards, &words),
            vec![
                Some(SolutionLine {
                    white: false,
                    san: "Qb1".into(),
                    single: true,
                }),
                Some(SolutionLine {
                    white: true,
                    san: "Ba3".into(),
                    single: true,
                }),
            ]
        );
    }

    #[test]
    fn a_caption_over_the_next_board_does_not_pose_this_one() {
        let boards = [
            rect(100.0, 500.0, 250.0, 650.0),
            rect(100.0, 250.0, 250.0, 380.0),
        ];
        let mut words = line("White to play wins by 1 Ba3+.", 90.0, 470.0);
        // The lower board's caption, in the gap over it.
        words.extend(line("Black to play.", 90.0, 395.0));
        words.extend(line("1 … Qb1", 90.0, 220.0));
        assert_eq!(
            bind_solution_lines(&boards, &words),
            vec![
                Some(SolutionLine {
                    white: true,
                    san: "Ba3".into(),
                    single: true,
                }),
                Some(SolutionLine {
                    white: false,
                    san: "Qb1".into(),
                    single: true,
                }),
            ]
        );
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
