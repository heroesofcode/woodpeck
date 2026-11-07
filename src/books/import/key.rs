//! Bind a printed exercise number to the side named by its solution.
//!
//! Puzzle books often print a bare number above a diagram and the moves later, in a
//! numbered key: `12 - 1. Nf3` is White to move and `12 - 1... c5` is Black. The number
//! on the diagram is matched to that key. The side is stored apart from captions and
//! printed marks; the resolver applies it only while every stronger signal left the
//! side unknown.
//!
//! A number under a board is left alone. On a page of stacked diagrams that number
//! belongs to the board below it, and rank digits beside a frame are not exercise
//! numbers. Several numbers in one diagram's band, a missing key, two keys that
//! disagree, or a first move that is not clearly White or Black all abstain.

use super::caption::{self, Rect, TextBox};
use std::collections::HashMap;
use std::path::Path;

/// One hit inside a solution line.
struct Hit {
    number: u32,
    /// `Some(true)` is White. `None` is a first move that is not clearly either side.
    side: Option<bool>,
    len: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Voted {
    White,
    Black,
    Unclear,
}

/// Sides named by a solution key. A number whose entries disagree, or whose first move
/// is not clearly White or Black, is present as `None` so a binder can tell a conflict
/// from a number the key never mentions.
pub(super) fn solution_sides(text: &str) -> HashMap<u32, Option<bool>> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut voted: HashMap<u32, Voted> = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        let entries = line_entries(lines[i]);
        if !entries.is_empty() {
            for hit in entries {
                vote(&mut voted, hit.number, named(hit.side));
            }
            i += 1;
            continue;
        }
        if let Some(number) = dangling_exercise(lines[i])
            && let Some(side) = lines.get(i + 1).and_then(|line| leading_move_side(line))
        {
            vote(&mut voted, number, side);
            i += 2;
            continue;
        }
        i += 1;
    }
    voted
        .into_iter()
        .map(|(number, side)| {
            let side = match side {
                Voted::White => Some(true),
                Voted::Black => Some(false),
                Voted::Unclear => None,
            };
            (number, side)
        })
        .collect()
}

fn vote(voted: &mut HashMap<u32, Voted>, number: u32, side: Voted) {
    voted
        .entry(number)
        .and_modify(|have| {
            if *have != side {
                *have = Voted::Unclear;
            }
        })
        .or_insert(side);
}

fn named(side: Option<bool>) -> Voted {
    match side {
        Some(true) => Voted::White,
        Some(false) => Voted::Black,
        None => Voted::Unclear,
    }
}

/// Exercise numbers printed above each diagram. `None` abstains: no number in the band,
/// or more than one.
pub(super) fn diagram_numbers(diagrams: &[Rect], texts: &[TextBox]) -> Vec<Option<u32>> {
    let mut owned: Vec<Vec<u32>> = vec![Vec::new(); diagrams.len()];
    for line in caption::lines_of(texts.to_vec()) {
        let Some(number) = number_label(&line.text) else {
            continue;
        };
        if let Some(index) = nearest_above(diagrams, line.rect) {
            owned[index].push(number);
        }
    }
    owned
        .into_iter()
        .map(|numbers| {
            let mut numbers = numbers;
            numbers.sort_unstable();
            numbers.dedup();
            match numbers.as_slice() {
                [number] => Some(*number),
                _ => None,
            }
        })
        .collect()
}

/// One side per diagram from `solutions`, which is the whole key, not the page order.
/// A diagram with no number, no matching entry, or a conflict stays `None`.
pub(super) fn bind_answer_keys(
    diagrams: &[Rect],
    texts: &[TextBox],
    solutions: &HashMap<u32, Option<bool>>,
) -> Vec<Option<bool>> {
    diagram_numbers(diagrams, texts)
        .into_iter()
        .map(|number| {
            number.and_then(|number| match solutions.get(&number) {
                Some(Some(side)) => Some(*side),
                _ => None,
            })
        })
        .collect()
}

/// A short label that is only an exercise number: `12`, `No. 12`, `Diagram 12`, `(12)`.
fn number_label(text: &str) -> Option<u32> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.chars().count() > 24 {
        return None;
    }
    let lower = text.to_lowercase();
    let rest = strip_number_prefix(&lower).unwrap_or(lower.as_str());
    let rest =
        rest.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '.' | ':' | '#' | '№'));
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) || rest.len() > 4 {
        return None;
    }
    let number: u32 = rest.parse().ok()?;
    (1..=9999).contains(&number).then_some(number)
}

fn strip_number_prefix(lower: &str) -> Option<&str> {
    const PREFIXES: &[&str] = &[
        "diagram", "diag", "figure", "fig", "dia", "no.", "nr.", "no", "nr", "nº", "n°", "№",
    ];
    for prefix in PREFIXES {
        let Some(rest) = lower.strip_prefix(prefix) else {
            continue;
        };
        let rest = rest.trim_start_matches(['.', ':', ' ', '#']);
        if rest.starts_with(|c: char| c.is_ascii_digit() || c == '(') {
            return Some(rest);
        }
    }
    None
}

fn nearest_above(diagrams: &[Rect], number: Rect) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    let mut second = f64::INFINITY;
    for (index, diagram) in diagrams.iter().enumerate() {
        let Some(cost) = above_cost(*diagram, number) else {
            continue;
        };
        if let Some((_, best_cost)) = best {
            if cost < best_cost {
                second = best_cost;
                best = Some((index, cost));
            } else if cost < second {
                second = cost;
            }
        } else {
            best = Some((index, cost));
        }
    }
    let (index, cost) = best?;
    (diagrams.len() == 1 || second - cost >= 8.0).then_some(index)
}

/// Cost of a number sitting above `diagram`. `None` when it is below, beside, or too far.
fn above_cost(diagram: Rect, number: Rect) -> Option<f64> {
    let d_height = (diagram.top - diagram.bottom).max(1.0);
    let d_width = (diagram.right - diagram.left).max(1.0);
    let n_width = (number.right - number.left).max(1.0);
    let center = f64::midpoint(number.left, number.right);
    let slack = 0.18 * d_width;
    let h_overlap = (diagram.right.min(number.right) - diagram.left.max(number.left)).max(0.0);
    let center_near = (diagram.left - slack..=diagram.right + slack).contains(&center);
    if h_overlap <= 0.0 && !center_near {
        return None;
    }
    // A digit in the gutter, or a rank label just left of the frame, is not this number.
    if !(diagram.left..=diagram.right).contains(&center) && h_overlap / n_width < 0.5 {
        return None;
    }
    let gap = number.bottom - diagram.top;
    let into = diagram.top - number.bottom;
    let above_limit = (0.45 * d_height).clamp(36.0, 78.0);
    let into_limit = (0.12 * d_height).clamp(4.0, 16.0);
    if gap > above_limit || into > into_limit || number.top < diagram.top - into_limit {
        return None;
    }
    let h_pen = if h_overlap > 0.0 {
        (1.0 - (h_overlap / n_width).clamp(0.0, 1.0)) * 18.0
    } else if center < diagram.left {
        diagram.left - center
    } else {
        center - diagram.right
    };
    Some(gap.max(0.0) + h_pen)
}

fn line_entries(line: &str) -> Vec<Hit> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut at_line_start = true;
    let mut after_entry = false;
    while i < line.len() {
        let start = skip_ws(line, i);
        if start >= line.len() {
            break;
        }
        i = start;
        if let Some(hit) = match_entry(&line[i..], at_line_start || after_entry) {
            i += hit.len;
            out.push(hit);
            after_entry = true;
            at_line_start = false;
        } else {
            let next = take_token(line, i);
            i = if next > i {
                next
            } else {
                i + line[i..].chars().next().map_or(1, char::len_utf8)
            };
            after_entry = false;
            at_line_start = false;
        }
    }
    out
}

/// `12 -` at the end of a line, with the move on the next line. The dash keeps this from
/// pairing a bare diagram number with whatever game follows it.
fn dangling_exercise(line: &str) -> Option<u32> {
    let line = line.trim();
    let last = line.chars().next_back()?;
    if !is_dash(last) {
        return None;
    }
    let body = line[..line.len() - last.len_utf8()].trim();
    if body.contains(is_dash) {
        return None;
    }
    number_label(body)
}

fn leading_move_side(line: &str) -> Option<Voted> {
    let line = line.trim();
    if !solutionish(line) {
        return None;
    }
    let mut i = take_digits(line, 0);
    if i == 0 {
        return None;
    }
    let (side, after) = read_dots(line, i)?;
    i = skip_ws(line, after);
    let end = take_token(line, i);
    looks_like_move(&line[i..end]).then_some(named(side))
}

fn solutionish(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    !words.is_empty() && words.len() <= 16 && words.iter().all(|word| move_or_note(word))
}

fn move_or_note(word: &str) -> bool {
    let bare = word.trim_matches(|c: char| {
        matches!(
            c,
            '(' | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | '+'
                | '#'
                | '"'
                | '\''
        )
    });
    if bare.is_empty() || looks_like_move(bare) {
        return true;
    }
    let core = bare.trim_matches(|c: char| c == '.' || c == '…');
    if !core.is_empty()
        && core
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '/' | '.' | '…'))
    {
        return true;
    }
    matches!(
        bare.to_ascii_lowercase().as_str(),
        "and" | "win" | "draw" | "mate" | "or" | "with" | "check"
    )
}

fn match_entry(s: &str, allow_dot: bool) -> Option<Hit> {
    let mut i = label_len(s);
    if s[i..].starts_with('(') {
        i += 1;
    }
    let digits_at = i;
    i = take_digits(s, i);
    if i == digits_at || i - digits_at > 4 {
        return None;
    }
    let number: u32 = s[digits_at..i].parse().ok()?;
    if number == 0 {
        return None;
    }
    let mut closed = false;
    if s[i..].starts_with(')') {
        i += 1;
        closed = true;
    }
    let mut j = skip_ws(s, i);
    let sep = s[j..].chars().next()?;
    let sep_len = sep.len_utf8();
    let dashed = is_dash(sep) || sep == ':';
    if dashed || (sep == '.' && allow_dot && !continues_ellipsis(&s[j + sep_len..])) {
        j += sep_len;
        j = skip_ws(s, j);
        // `12. - 1. Nf3`: the dot closes the exercise number and the dash introduces the move.
        if let Some(extra) = s[j..].chars().next()
            && is_dash(extra)
        {
            j += extra.len_utf8();
            j = skip_ws(s, j);
        }
    } else if closed && sep.is_ascii_digit() {
        // `(12) 1. Nf3`: the parenthesis already separated the exercise from the move.
    } else {
        return None;
    }
    let move_at = j;
    j = take_digits(s, j);
    if j == move_at || j - move_at > 3 {
        return None;
    }
    let move_no: u32 = s[move_at..j].parse().ok()?;
    if move_no == 0 {
        return None;
    }
    let (side, after_dots) = read_dots(s, j)?;
    j = skip_ws(s, after_dots);
    let token_end = take_token(s, j);
    if token_end == j || !looks_like_move(&s[j..token_end]) {
        return None;
    }
    Some(Hit {
        number,
        side,
        len: token_end,
    })
}

fn continues_ellipsis(s: &str) -> bool {
    s.starts_with('.') || s.starts_with('…')
}

/// `Some(true)` is a single dot (White). `Some(false)` is an ellipsis (Black).
/// `None` inside `Some` is `..`, which does not name a side.
fn read_dots(s: &str, mut i: usize) -> Option<(Option<bool>, usize)> {
    if s[i..].starts_with('…') {
        i += '…'.len_utf8();
        return Some((Some(false), skip_extra_dots(s, i)));
    }
    if !s[i..].starts_with('.') {
        return None;
    }
    let mut count = 0;
    while s[i..].starts_with('.') {
        count += 1;
        i += 1;
    }
    if count >= 3 {
        return Some((Some(false), i));
    }
    if count == 2 {
        return Some((None, i));
    }
    let j = skip_ws(s, i);
    if s[j..].starts_with('…') {
        return Some((Some(false), skip_extra_dots(s, j + '…'.len_utf8())));
    }
    if s[j..].starts_with("...") {
        return Some((Some(false), j + 3));
    }
    Some((Some(true), i))
}

fn skip_extra_dots(s: &str, mut i: usize) -> usize {
    loop {
        let j = skip_ws(s, i);
        if s[j..].starts_with('…') {
            i = j + '…'.len_utf8();
        } else if s[j..].starts_with('.') {
            i = j;
            while s[i..].starts_with('.') {
                i += 1;
            }
        } else {
            return i;
        }
    }
}

fn label_len(s: &str) -> usize {
    const LABELS: &[&str] = &["no.", "nr.", "no", "nr", "nº", "n°", "#", "№"];
    for label in LABELS {
        if s.len() >= label.len()
            && s.as_bytes()[..label.len()].eq_ignore_ascii_case(label.as_bytes())
        {
            let rest = &s[label.len()..];
            let trimmed = rest.trim_start();
            if trimmed.starts_with(|c: char| c.is_ascii_digit() || c == '(') {
                return s.len() - trimmed.len();
            }
        }
    }
    0
}

fn looks_like_move(token: &str) -> bool {
    let token = token.trim_end_matches(['+', '#', '!', '?']);
    if token.is_empty() {
        return false;
    }
    let lower = token.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "o-o" | "o-o-o" | "0-0" | "0-0-0" | "castles"
    ) {
        return true;
    }
    if token.contains(|c: char| ('♔'..='♟').contains(&c)) {
        return true;
    }
    token.chars().any(|c| c.is_ascii_digit()) && token.chars().any(|c| c.is_ascii_alphabetic())
}

fn is_dash(c: char) -> bool {
    matches!(c, '-' | '–' | '—' | '−' | '‒' | '―')
}

fn skip_ws(s: &str, i: usize) -> usize {
    s[i..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map_or(s.len(), |(k, _)| i + k)
}

fn take_digits(s: &str, i: usize) -> usize {
    s[i..]
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map_or(s.len(), |(k, _)| i + k)
}

fn take_token(s: &str, i: usize) -> usize {
    s[i..]
        .char_indices()
        .find(|(_, c)| {
            c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '|')
        })
        .map_or(s.len(), |(k, _)| i + k)
}

fn page_texts(
    poppler: Option<&Vec<Vec<TextBox>>>,
    pages: &[Vec<super::pdf::Line>],
    index: usize,
) -> Vec<TextBox> {
    if let Some(words) = poppler.and_then(|doc| doc.get(index))
        && !words.is_empty()
    {
        return words.clone();
    }
    pages
        .get(index)
        .map(|lines| caption::glyph_boxes(lines))
        .unwrap_or_default()
}

/// Stamp a per-diagram key side onto pictures. The chunk is separate from a caption and
/// from a printed marker; the resolver is what keeps those in force.
pub(super) fn stamp_answer_keys(
    pdf: &Path,
    pages: &[Vec<super::pdf::Line>],
    pictures: &mut [Vec<super::picture::Placed>],
    cover: Option<&[u8]>,
) {
    if pictures.iter().all(Vec::is_empty) {
        return;
    }
    let poppler = caption::bbox_document(pdf);
    let page_count = pictures.len().max(pages.len());
    let texts: Vec<Vec<TextBox>> = (0..page_count)
        .map(|index| page_texts(poppler.as_ref(), pages, index))
        .collect();
    let mut solution = String::new();
    for page in &texts {
        for line in caption::lines_of(page.clone()) {
            if !solution.is_empty() {
                solution.push('\n');
            }
            solution.push_str(&line.text);
        }
    }
    let solutions = solution_sides(&solution);
    for (page, placed) in pictures.iter_mut().enumerate() {
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
        let sides = bind_answer_keys(
            &diagrams,
            texts.get(page).map_or(&[], Vec::as_slice),
            &solutions,
        );
        for (picture, side) in placed.iter_mut().zip(sides) {
            let Some(white) = side else {
                continue;
            };
            if cover.is_some_and(|bytes| bytes == picture.bytes.as_slice()) {
                continue;
            }
            if crate::books::diagram::stated_key_side(&picture.bytes).is_some() {
                continue;
            }
            let Some(png) = super::pdf::diagram_png(&picture.bytes) else {
                continue;
            };
            picture.bytes = crate::books::diagram::with_key_side(&png, white);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Rect, TextBox, bind_answer_keys, diagram_numbers, number_label, solution_sides};

    fn rect(left: f64, bottom: f64, right: f64, top: f64) -> Rect {
        Rect {
            left,
            bottom,
            right,
            top,
        }
    }

    fn text(text: &str, left: f64, bottom: f64, right: f64, top: f64) -> TextBox {
        TextBox {
            text: text.into(),
            rect: rect(left, bottom, right, top),
        }
    }

    fn side(text: &str, number: u32) -> Option<bool> {
        solution_sides(text).get(&number).copied().flatten()
    }

    #[test]
    fn a_dash_key_names_white_from_a_dot_and_black_from_an_ellipsis() {
        let text = "\
12 - 1. Nf3
13 - 1... c5
14 – 1. d4
15 — 1... e5
16 − 1. c4
18 - 1. ... Nf6
19 - 1… c5
20 - 1. … e5
21 - 1...Nf6
No. 22 - 1. e4
";
        assert_eq!(side(text, 12), Some(true));
        assert_eq!(side(text, 13), Some(false));
        assert_eq!(side(text, 14), Some(true));
        assert_eq!(side(text, 15), Some(false));
        assert_eq!(side(text, 16), Some(true));
        assert_eq!(side(text, 18), Some(false));
        assert_eq!(side(text, 19), Some(false));
        assert_eq!(side(text, 20), Some(false));
        assert_eq!(side(text, 21), Some(false));
        assert_eq!(side(text, 22), Some(true));
    }

    #[test]
    fn parenthesis_colon_and_dotted_keys_name_the_same_sides() {
        let text = "\
(12) 1... c5
13) 1. d4
14: 1. c4
15. 1. Nf3 d5
16. - 1... e5
23 - 1. a3    24 - 1... a5
";
        assert_eq!(side(text, 12), Some(false));
        assert_eq!(side(text, 13), Some(true));
        assert_eq!(side(text, 14), Some(true));
        assert_eq!(side(text, 15), Some(true));
        assert_eq!(side(text, 16), Some(false));
        assert_eq!(side(text, 23), Some(true));
        assert_eq!(side(text, 24), Some(false));
    }

    #[test]
    fn a_move_on_the_line_after_a_dangling_dash_names_the_side() {
        let text = "22 -\n1. Nc3 d5 2. g3\n";
        assert_eq!(side(text, 22), Some(true));
        let text = "30 –\n1... c5 2. Nf3\n";
        assert_eq!(side(text, 30), Some(false));
    }

    #[test]
    fn a_game_score_and_a_mid_sentence_dotted_number_are_not_keys() {
        assert!(solution_sides("1. e4 e5 2. Nf3 Nc6").is_empty());
        assert!(solution_sides("12. Nf3 Nc6").is_empty());
        assert!(solution_sides("see 12. 1. e4 later in the note").is_empty());
        assert!(solution_sides("12 - Nf3").is_empty());
        assert_eq!(solution_sides("12 - 1.. Nf3").get(&12).copied(), Some(None));
    }

    #[test]
    fn disagreeing_keys_and_an_unclear_duplicate_abstain() {
        let text = "12 - 1. Nf3\n12 - 1... c5\n";
        assert_eq!(solution_sides(text).get(&12).copied(), Some(None));
        let text = "7 - 1.. e4\n7 - 1. Nf3\n";
        assert_eq!(solution_sides(text).get(&7).copied(), Some(None));
        // The same side twice is one answer, not a conflict.
        assert_eq!(side("4 - 1. e4\n4 - 1. d4\n", 4), Some(true));
    }

    #[test]
    fn the_number_above_a_board_binds_that_exercise_not_page_order() {
        // Solutions list 3 before 7. The left board is 7 and the right board is 3.
        let diagrams = [
            rect(40.0, 400.0, 200.0, 560.0),
            rect(240.0, 400.0, 400.0, 560.0),
        ];
        let texts = [
            text("7", 100.0, 566.0, 116.0, 580.0),
            text("3", 300.0, 566.0, 316.0, 580.0),
        ];
        let solutions = solution_sides("3 - 1... c5\n7 - 1. Nf3\n");
        assert_eq!(
            bind_answer_keys(&diagrams, &texts, &solutions),
            vec![Some(true), Some(false)]
        );
    }

    #[test]
    fn labels_above_the_board_are_numbers_and_a_caption_is_not() {
        assert_eq!(number_label("324"), Some(324));
        assert_eq!(number_label("No. 12"), Some(12));
        assert_eq!(number_label("Diagram 8"), Some(8));
        assert_eq!(number_label("(15)"), Some(15));
        assert_eq!(number_label("Fig. 3"), Some(3));
        assert_eq!(number_label("White to move"), None);
        assert_eq!(number_label("12 - 1. Nf3"), None);
        let diagrams = [rect(40.0, 400.0, 200.0, 560.0)];
        let texts = [text("No. 12", 70.0, 564.0, 140.0, 578.0)];
        assert_eq!(diagram_numbers(&diagrams, &texts), vec![Some(12)]);
    }

    #[test]
    fn missing_several_or_unmatched_numbers_abstain() {
        let diagrams = [rect(40.0, 400.0, 200.0, 560.0)];
        let several = [
            text("7", 70.0, 566.0, 86.0, 580.0),
            text("8", 120.0, 566.0, 136.0, 580.0),
        ];
        assert_eq!(diagram_numbers(&diagrams, &several), vec![None]);
        let none = [text("White to move", 50.0, 566.0, 160.0, 580.0)];
        assert_eq!(diagram_numbers(&diagrams, &none), vec![None]);
        // A page number far above the board is outside the band.
        let far = [text("39", 160.0, 700.0, 180.0, 714.0)];
        assert_eq!(diagram_numbers(&diagrams, &far), vec![None]);
        // Under the board: that band belongs to a diagram below, not this one.
        let below = [text("7", 100.0, 370.0, 116.0, 384.0)];
        assert_eq!(diagram_numbers(&diagrams, &below), vec![None]);
        let above = [text("7", 100.0, 566.0, 116.0, 580.0)];
        let solutions = solution_sides("8 - 1. e4\n");
        assert_eq!(bind_answer_keys(&diagrams, &above, &solutions), vec![None]);
        let conflict = solution_sides("7 - 1. e4\n7 - 1... c5\n");
        assert_eq!(bind_answer_keys(&diagrams, &above, &conflict), vec![None]);
    }

    #[test]
    fn a_number_in_the_gutter_is_not_given_to_either_diagram() {
        let diagrams = [
            rect(40.0, 400.0, 180.0, 560.0),
            rect(220.0, 400.0, 360.0, 560.0),
        ];
        let texts = [text("9", 190.0, 566.0, 206.0, 580.0)];
        assert_eq!(diagram_numbers(&diagrams, &texts), vec![None, None]);
    }

    #[test]
    fn a_number_that_overlaps_the_top_edge_still_belongs_to_that_board() {
        let diagrams = [rect(40.0, 400.0, 200.0, 560.0)];
        // The box dips 6pt into the frame. 6 is inside the overlap allowance.
        let texts = [text("4", 100.0, 554.0, 116.0, 568.0)];
        assert_eq!(diagram_numbers(&diagrams, &texts), vec![Some(4)]);
    }

    #[test]
    fn separate_words_on_one_baseline_still_read_as_the_number() {
        let diagrams = [rect(40.0, 300.0, 200.0, 460.0)];
        let texts = [
            text("No.", 70.0, 466.0, 98.0, 480.0),
            text("12", 102.0, 466.0, 122.0, 480.0),
        ];
        assert_eq!(diagram_numbers(&diagrams, &texts), vec![Some(12)]);
    }

    #[test]
    fn a_generated_pdf_stamps_the_side_from_a_solution_on_another_page() {
        use lopdf::{Object, Stream, dictionary};
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("keys.pdf");
        let mut doc = lopdf::Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let image = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 130,
                "Height" => 130,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            vec![180_u8; 130 * 130],
        );
        let image_id = doc.add_object(image);
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources = dictionary! {
            "Font" => dictionary! { "F1" => font_id },
            "XObject" => dictionary! { "Im0" => image_id },
        };
        let mut page = |content: &str| {
            let content_id =
                doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 400.into(), 500.into()],
                "Contents" => content_id,
                "Resources" => resources.clone(),
            })
        };
        // The number sits just above the picture. Each solution is printed on the other page.
        let first = page(
            "q 140 0 0 140 40 280 cm /Im0 Do Q\n\
             BT /F1 12 Tf 90 430 Td (12) Tj ET\n\
             BT /F1 12 Tf 40 40 Td (13 - 1. Nf3) Tj ET\n",
        );
        let second = page(
            "q 140 0 0 140 40 280 cm /Im0 Do Q\n\
             BT /F1 12 Tf 90 430 Td (13) Tj ET\n\
             BT /F1 12 Tf 40 40 Td (12 - 1... c5) Tj ET\n",
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![first.into(), second.into()],
                "Count" => 2,
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc.save(&path).expect("pdf");
        let bytes = std::fs::read(&path).expect("read");
        let mut extracted = super::super::pdf::extract(&bytes).expect("extract");
        super::stamp_answer_keys(
            &path,
            &extracted.pages,
            &mut extracted.pictures,
            extracted.cover.as_deref(),
        );
        let side = |page: usize| {
            crate::books::diagram::stated_key_side(&extracted.pictures[page][0].bytes)
        };
        assert_eq!(extracted.pictures[0].len(), 1);
        assert_eq!(extracted.pictures[1].len(), 1);
        assert_eq!(side(0), Some(false), "12 - 1... is Black");
        assert_eq!(side(1), Some(true), "13 - 1. is White");
    }
}
