//! Bind a printed side-to-move caption to the diagram it belongs to.
//!
//! Captions such as "White to move" are matched from poppler word boxes (`pdftotext -bbox`)
//! and, when that text layer is empty, from the glyph lines already extracted. Each caption
//! is attached by its place on the page, so one page may carry two diagrams of different
//! sides. The stored side is applied later only while the resolver still has `Turn::Unknown`.

use shakmaty::Color;
use std::path::Path;

/// A rectangle in PDF user space: origin at the bottom left, `top` greater than `bottom`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Rect {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

/// One word or line, already in the same page space as [`Rect`].
#[derive(Clone, Debug)]
pub(super) struct TextBox {
    pub rect: Rect,
    pub text: String,
}

/// Phrases [`crate::books::resolve::caption_turn`] accepts, longest first at use time.
/// Stripping them leaves only a diagram number or a short suffix ("and win").
const PHRASES: &[&str] = &[
    "white to move",
    "black to move",
    "white to play",
    "black to play",
    "white's move",
    "black's move",
    "white\u{2019}s move",
    "black\u{2019}s move",
    "white moves",
    "white plays",
    "black moves",
    "black plays",
    "wei\u{00df} am zug",
    "weiss am zug",
    "schwarz am zug",
    "trait aux blancs",
    "trait aux noirs",
    "vez das brancas",
    "vez das pretas",
    "vez das negras",
    "tocca al bianco",
    "tocca al nero",
    "wit aan zet",
    "zwart aan zet",
    "ход белых",
    "ход черных",
    "ход чёрных",
    "brancas jogam",
    "jogam as brancas",
    "jogam brancas",
    "pretas jogam",
    "jogam as pretas",
    "jogam pretas",
    "negras jogam",
    "jogam as negras",
    "juegan las blancas",
    "juegan blancas",
    "blancas juegan",
    "mueven las blancas",
    "mueven blancas",
    "juegan las negras",
    "juegan negras",
    "negras juegan",
    "mueven las negras",
    "mueven negras",
    "wei\u{00df} zieht",
    "weiss zieht",
    "schwarz zieht",
    "les blancs jouent",
    "blancs jouent",
    "les noirs jouent",
    "noirs jouent",
    "muove il bianco",
    "il bianco muove",
    "muove il nero",
    "il nero muove",
];

const RESIDUE: &[&str] = &[
    "and", "win", "draw", "mate", "diagram", "fig", "figure", "no", "nr",
];

/// `Some(true)` is White to move. Only a short line that is a side caption counts.
pub(super) fn side_caption(text: &str) -> Option<bool> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if !caption_length(&text) {
        return None;
    }
    // "12a) White moves" — the exercise label is not part of the phrase.
    let text = strip_exercise_label(&text);
    if !caption_length(text) {
        return None;
    }
    if let Some(side) = crate::books::resolve::caption_turn(text) {
        // A phrase buried in other words is commentary, not the caption under a diagram.
        return residue_ok(text).then_some(side == Color::White);
    }
    wider_side(text).or_else(|| scrambled_side(text))
}

fn caption_length(text: &str) -> bool {
    !text.is_empty() && text.chars().count() <= 60 && text.split_whitespace().count() <= 8
}

/// Digits, an optional letter, then `)` `.` or `:`. "12a) White wins" loses the "12a)".
fn strip_exercise_label(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut end = 0;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == 0 {
        return text;
    }
    if end < bytes.len() && bytes[end].is_ascii_alphabetic() {
        end += 1;
    }
    if end < bytes.len() && matches!(bytes[end], b')' | b'.' | b':') {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_whitespace() {
            end += 1;
        }
        return &text[end..];
    }
    text
}

/// Verbs that name a side only as a caption under a diagram: "Black wins", "White to draw",
/// "Black has the move". They are not read from nearby prose.
fn wider_side(text: &str) -> Option<bool> {
    const WIDER: &[(&str, bool)] = &[
        ("white has the move", true),
        ("black has the move", false),
        ("white to win", true),
        ("black to win", false),
        ("white to draw", true),
        ("black to draw", false),
        ("white wins", true),
        ("black wins", false),
        ("white draws", true),
        ("black draws", false),
        ("white mates", true),
        ("black mates", false),
    ];
    let lower = text.to_lowercase();
    let mut found = None;
    let mut rest = lower.clone();
    for (phrase, white) in WIDER {
        let Some(at) = phrase_at(&rest, phrase) else {
            continue;
        };
        if found.is_some() {
            return None;
        }
        found = Some(*white);
        rest.replace_range(at..at + phrase.len(), " ");
    }
    let white = found?;
    words_are_residue(&rest).then_some(white)
}

/// Index of `phrase` bounded by non-letters, so "white wins" is not part of a longer word.
fn phrase_at(text: &str, phrase: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(offset) = text[start..].find(phrase) {
        let at = start + offset;
        let before = text[..at].chars().next_back();
        let after = text[at + phrase.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphabetic());
        if boundary(before) && boundary(after) {
            return Some(at);
        }
        start = at + phrase.len();
    }
    None
}

fn residue_ok(text: &str) -> bool {
    let mut lower = text.to_lowercase();
    let mut phrases: Vec<&str> = PHRASES.to_vec();
    phrases.sort_by_key(|phrase| std::cmp::Reverse(phrase.len()));
    for phrase in phrases {
        if lower.contains(phrase) {
            lower = lower.replace(phrase, " ");
        }
    }
    words_are_residue(&lower)
}

fn words_are_residue(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .all(|word| word.chars().all(|c| c.is_ascii_digit()) || RESIDUE.contains(&word))
}

/// "White o t omve": the side word is intact and "to move" / "to play" / "'s move" survives
/// as the same letters with the spaces broken.
fn scrambled_side(text: &str) -> Option<bool> {
    let letters: String = text
        .chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    if letters.contains("white") && letters.contains("black") {
        return None;
    }
    let mut found = None;
    for (stem, white) in [("white", true), ("black", false)] {
        let Some(index) = letters.find(stem) else {
            continue;
        };
        if verb_anagram(&letters[index + stem.len()..]) {
            if found.is_some() {
                return None;
            }
            found = Some(white);
        }
    }
    found
}

fn verb_anagram(rest: &str) -> bool {
    const VERBS: [&str; 3] = ["tomove", "toplay", "smove"];
    for verb in VERBS {
        let n = verb.len();
        for offset in 0..=2 {
            let Some(window) = rest.get(offset..offset + n) else {
                continue;
            };
            if !same_letters(window, verb) {
                continue;
            }
            let tail = &rest[offset + n..];
            if tail.is_empty() || matches!(tail, "andwin" | "anddraw" | "andmate") {
                return true;
            }
        }
    }
    false
}

fn same_letters(a: &str, b: &str) -> bool {
    let mut a: Vec<char> = a.chars().collect();
    let mut b: Vec<char> = b.chars().collect();
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

fn height(rect: Rect) -> f64 {
    (rect.top - rect.bottom).max(1.0)
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Words on one baseline become one line, whatever order the PDF emitted them in and however
/// it had already grouped them. A column gutter stays split, so two captions on one row keep
/// their own sides.
pub(super) fn lines_of(mut words: Vec<TextBox>) -> Vec<TextBox> {
    words.sort_by(|a, b| {
        midpoint_y(&b.rect)
            .total_cmp(&midpoint_y(&a.rect))
            .then(a.rect.left.total_cmp(&b.rect.left))
    });
    let mut rows: Vec<Vec<TextBox>> = Vec::new();
    for word in words {
        if let Some(row) = rows.iter_mut().find(|row| aligned(&row[0], &word)) {
            row.push(word);
        } else {
            rows.push(vec![word]);
        }
    }
    rows.into_iter().flat_map(split_row).collect()
}

fn midpoint_y(rect: &Rect) -> f64 {
    f64::midpoint(rect.bottom, rect.top)
}

fn aligned(anchor: &TextBox, word: &TextBox) -> bool {
    let anchor = anchor.rect;
    let word = word.rect;
    let h = height(anchor).min(height(word)).max(1.0);
    // `|Δ(top + bottom)| < h` is a midpoint gap below half the shorter word.
    // A full line height would merge the next row of tight leading.
    overlap(anchor.bottom, anchor.top, word.bottom, word.top) > 0.45 * h
        || ((anchor.bottom + anchor.top) - (word.bottom + word.top)).abs() < h
}

fn split_row(mut words: Vec<TextBox>) -> Vec<TextBox> {
    words.sort_by(|a, b| a.rect.left.total_cmp(&b.rect.left));
    let mut groups: Vec<Vec<TextBox>> = Vec::new();
    for word in words {
        let join = groups.last().is_some_and(|line| {
            let right = line
                .iter()
                .map(|item| item.rect.right)
                .fold(f64::MIN, f64::max);
            let h = height(line[0].rect).min(height(word.rect)).max(1.0);
            let gap = word.rect.left - right;
            gap < (h * 1.6).max(14.0) && gap > -h
        });
        if join {
            groups.last_mut().expect("line").push(word);
        } else {
            groups.push(vec![word]);
        }
    }
    groups.into_iter().map(join_line).collect()
}

fn join_line(mut words: Vec<TextBox>) -> TextBox {
    words.sort_by(|a, b| a.rect.left.total_cmp(&b.rect.left));
    let mut text = String::new();
    let mut left = f64::MAX;
    let mut right = f64::MIN;
    let mut bottom = f64::MAX;
    let mut top = f64::MIN;
    let mut previous: Option<f64> = None;
    for word in &words {
        if let Some(end) = previous {
            let h = height(word.rect).max(1.0);
            if word.rect.left - end > 0.15 * h {
                text.push(' ');
            }
        }
        text.push_str(word.text.trim());
        left = left.min(word.rect.left);
        right = right.max(word.rect.right);
        bottom = bottom.min(word.rect.bottom);
        top = top.max(word.rect.top);
        previous = Some(word.rect.right);
    }
    TextBox {
        text,
        rect: Rect {
            left,
            bottom,
            right,
            top,
        },
    }
}

/// Cost of hanging `caption` on `diagram`. Lower is closer. `None` when the caption is not
/// in the band under or just over that diagram.
fn band_cost(diagram: Rect, caption: Rect) -> Option<f64> {
    let d_height = height(diagram);
    let d_width = (diagram.right - diagram.left).max(1.0);
    let c_width = (caption.right - caption.left).max(1.0);
    let h_overlap = overlap(diagram.left, diagram.right, caption.left, caption.right);
    let center = f64::midpoint(caption.left, caption.right);
    let slack = 0.15 * d_width;
    let center_near = (diagram.left - slack..=diagram.right + slack).contains(&center);
    if h_overlap <= 0.0 && !center_near {
        return None;
    }
    let overlap_ratio = (h_overlap / c_width).clamp(0.0, 1.0);
    let h_pen = if h_overlap > 0.0 {
        (1.0 - overlap_ratio) * 24.0
    } else if center < diagram.left {
        diagram.left - center
    } else {
        center - diagram.right
    };
    let below = diagram.bottom - caption.top;
    let below_limit = (0.45 * d_height).clamp(40.0, 110.0);
    if (-0.2 * d_height..=below_limit).contains(&below) {
        return Some(below.max(0.0) + h_pen);
    }
    let above = caption.bottom - diagram.top;
    let above_limit = 32.0_f64.min(0.22 * d_height);
    if (-4.0..=above_limit).contains(&above) {
        return Some(above.max(0.0) + 18.0 + h_pen);
    }
    None
}

fn nearest_diagram(diagrams: &[Rect], caption: Rect) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    let mut second = f64::INFINITY;
    for (index, diagram) in diagrams.iter().enumerate() {
        let Some(cost) = band_cost(*diagram, caption) else {
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
    (diagrams.len() == 1 || second - cost >= 6.0).then_some(index)
}

/// One side per diagram: `Some(true)` is White. Two captions in one diagram's band, or a
/// caption equally close to two diagrams, abstain.
pub(super) fn bind_side_captions(diagrams: &[Rect], texts: &[TextBox]) -> Vec<Option<bool>> {
    let captions: Vec<(Rect, bool)> = lines_of(texts.to_vec())
        .into_iter()
        .filter_map(|line| side_caption(&line.text).map(|white| (line.rect, white)))
        .collect();
    let mut owned: Vec<Vec<bool>> = vec![Vec::new(); diagrams.len()];
    for (rect, white) in captions {
        if let Some(index) = nearest_diagram(diagrams, rect) {
            owned[index].push(white);
        }
    }
    owned
        .into_iter()
        .map(|sides| match sides.as_slice() {
            [side] => Some(*side),
            _ => None,
        })
        .collect()
}

fn attr(el: &scraper::ElementRef<'_>, name: &str) -> Option<f64> {
    el.value()
        .attrs()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.parse().ok())
}

/// `pdftotext -bbox` pages. Word boxes are converted to PDF user space (y up).
pub(super) fn parse_bbox(html: &str) -> Vec<Vec<TextBox>> {
    let dom = scraper::Html::parse_document(html);
    let Ok(page_sel) = scraper::Selector::parse("page") else {
        return Vec::new();
    };
    let Ok(word_sel) = scraper::Selector::parse("word") else {
        return Vec::new();
    };
    let mut pages = Vec::new();
    for page in dom.select(&page_sel) {
        let height = attr(&page, "height").unwrap_or(0.0);
        let mut words = Vec::new();
        if height > 0.0 {
            for word in page.select(&word_sel) {
                let text = word.text().collect::<String>();
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                let (Some(xmin), Some(xmax), Some(ymin), Some(ymax)) = (
                    attr(&word, "xmin"),
                    attr(&word, "xmax"),
                    attr(&word, "ymin"),
                    attr(&word, "ymax"),
                ) else {
                    continue;
                };
                words.push(TextBox {
                    text: text.to_owned(),
                    rect: Rect {
                        left: xmin.min(xmax),
                        right: xmin.max(xmax),
                        bottom: height - ymin.max(ymax),
                        top: height - ymin.min(ymax),
                    },
                });
            }
        }
        pages.push(words);
    }
    pages
}

pub(super) fn glyph_boxes(lines: &[super::pdf::Line]) -> Vec<TextBox> {
    lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .map(|line| {
            let h = line.size.max(1.0);
            let right = if line.end > line.x + 0.5 {
                line.end
            } else {
                line.x + h * 6.0
            };
            TextBox {
                text: line.text.clone(),
                rect: Rect {
                    left: line.x,
                    right,
                    bottom: line.y - 0.25 * h,
                    top: line.y + 0.8 * h,
                },
            }
        })
        .collect()
}

pub(super) fn bbox_document(pdf: &Path) -> Option<Vec<Vec<TextBox>>> {
    let path = pdf.to_str().filter(|path| !path.is_empty())?;
    let output = std::process::Command::new("pdftotext")
        .args(["-bbox", "-q", path, "-"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_bbox(&String::from_utf8_lossy(&output.stdout)))
}

fn bounds(picture: &super::picture::Placed) -> Rect {
    Rect {
        left: picture.left,
        bottom: picture.bottom,
        right: picture.right,
        top: picture.top,
    }
}

/// Stamp a per-diagram caption side onto pictures that do not already carry a printed marker.
/// The resolver reads the chunk only when every other signal left the side unknown.
pub(super) fn stamp_geometry_captions(
    pdf: &Path,
    pages: &[Vec<super::pdf::Line>],
    pictures: &mut [Vec<super::picture::Placed>],
    cover: Option<&[u8]>,
) {
    if pictures.iter().all(Vec::is_empty) {
        return;
    }
    let poppler = bbox_document(pdf);
    for (i, placed) in pictures.iter_mut().enumerate() {
        if placed.is_empty() {
            continue;
        }
        let glyphs = pages
            .get(i)
            .map(|lines| glyph_boxes(lines))
            .unwrap_or_default();
        let texts: &[TextBox] = match poppler.as_ref().and_then(|doc| doc.get(i)) {
            Some(words) if !words.is_empty() => words,
            _ => &glyphs,
        };
        let diagrams: Vec<Rect> = placed.iter().map(bounds).collect();
        let sides = bind_side_captions(&diagrams, texts);
        for (picture, side) in placed.iter_mut().zip(sides) {
            let Some(white) = side else {
                continue;
            };
            if cover.is_some_and(|bytes| bytes == picture.bytes.as_slice()) {
                continue;
            }
            if crate::books::diagram::stated_side(&picture.bytes).is_some()
                || crate::books::diagram::stated_caption_side(&picture.bytes).is_some()
            {
                continue;
            }
            let Some(png) = super::pdf::diagram_png(&picture.bytes) else {
                continue;
            };
            picture.bytes = crate::books::diagram::with_caption_side(&png, white);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Rect, TextBox, bind_side_captions, parse_bbox, side_caption};

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

    #[test]
    fn captions_name_only_a_short_side_line() {
        assert_eq!(side_caption("White to move"), Some(true));
        assert_eq!(side_caption("White  to   move"), Some(true));
        assert_eq!(side_caption("BLACK TO PLAY AND WIN"), Some(false));
        assert_eq!(side_caption("White o t omve"), Some(true));
        assert_eq!(side_caption("Black o t omve"), Some(false));
        assert_eq!(side_caption("Schwarz am Zug"), Some(false));
        assert_eq!(side_caption("Vez das brancas"), Some(true));
        assert_eq!(side_caption("Trait aux noirs"), Some(false));
        assert_eq!(side_caption("Brancas jogam"), Some(true));
        assert_eq!(side_caption("24) White moves"), Some(true));
        assert_eq!(side_caption("1. e4 e5 2. Nf3"), None);
        assert_eq!(side_caption("White to move the knight"), None);
        assert_eq!(side_caption("White mates in 2"), None);
    }

    #[test]
    fn an_exercise_label_and_the_wider_verbs_name_the_side() {
        assert_eq!(side_caption("12a) White moves"), Some(true));
        assert_eq!(side_caption("8b) Black wins"), Some(false));
        assert_eq!(side_caption("3. White to win"), Some(true));
        assert_eq!(side_caption("7: Black draws"), Some(false));
        assert_eq!(side_caption("4) White mates"), Some(true));
        assert_eq!(side_caption("9a. Black to draw"), Some(false));
        assert_eq!(side_caption("2) White has the move"), Some(true));
        assert_eq!(side_caption("5c: Black has the move"), Some(false));
        // The same verbs inside a sentence are not a caption.
        assert_eq!(side_caption("White wins the exchange"), None);
        assert_eq!(side_caption("Black mates in 2"), None);
        assert_eq!(side_caption("12ab) White wins"), None);
        assert_eq!(side_caption("See how Black draws the game"), None);
    }

    #[test]
    fn a_mixed_page_keeps_each_diagram_its_own_side() {
        let diagrams = [
            rect(40.0, 400.0, 180.0, 540.0),
            rect(220.0, 400.0, 360.0, 540.0),
        ];
        let texts = [
            text("White to move", 40.0, 372.0, 140.0, 386.0),
            text("Black to move", 220.0, 372.0, 330.0, 386.0),
        ];
        assert_eq!(
            bind_side_captions(&diagrams, &texts),
            vec![Some(true), Some(false)]
        );
    }

    #[test]
    fn words_join_into_a_caption_by_geometry() {
        let diagrams = [rect(40.0, 400.0, 200.0, 540.0)];
        // The extractor emitted the line out of order, on slightly different baselines.
        let words = [
            text("wins", 118.0, 368.4, 156.0, 382.2),
            text("8b)", 42.0, 370.0, 68.0, 384.0),
            text("Black", 74.0, 369.2, 112.0, 383.0),
        ];
        assert_eq!(bind_side_captions(&diagrams, &words), vec![Some(false)]);
        // The same words far from the diagram are not its caption.
        let loose = [
            text("Black", 40.0, 700.0, 80.0, 714.0),
            text("wins", 86.0, 700.0, 120.0, 714.0),
        ];
        assert_eq!(bind_side_captions(&diagrams, &loose), vec![None]);
        // Split across what the PDF called two lines, still one caption under the board.
        let split = [
            text("White has", 42.0, 372.0, 110.0, 386.0),
            text("the move", 116.0, 371.2, 180.0, 385.4),
        ];
        assert_eq!(bind_side_captions(&diagrams, &split), vec![Some(true)]);
    }

    #[test]
    fn baselines_join_within_half_a_line_and_not_at_three_quarters() {
        let diagrams = [rect(40.0, 400.0, 220.0, 560.0)];
        let h = 16.0;
        let line = |word: &str, left: f64, shift: f64| {
            text(word, left, 370.0 - shift, left + 48.0, 370.0 + h - shift)
        };
        // 0.75h sits inside a full-height window and outside the half-height one.
        // Joining these would mint "White wins"; apart, neither word is a caption.
        let apart = [line("White", 42.0, 0.0), line("wins", 96.0, 0.75 * h)];
        assert_eq!(bind_side_captions(&diagrams, &apart), vec![None]);
        // 0.4h is inside the old half-height window, so the pair is one caption.
        let together = [line("Black", 42.0, 0.0), line("wins", 96.0, 0.4 * h)];
        assert_eq!(bind_side_captions(&diagrams, &together), vec![Some(false)]);
    }

    #[test]
    fn a_scrambled_caption_binds_to_the_diagram_above_it() {
        let diagrams = [rect(40.0, 400.0, 180.0, 540.0)];
        let words = [
            text("White", 42.0, 370.0, 78.0, 384.0),
            text("o", 82.0, 370.0, 90.0, 384.0),
            text("t", 94.0, 370.0, 100.0, 384.0),
            text("omve", 104.0, 370.0, 140.0, 384.0),
        ];
        assert_eq!(bind_side_captions(&diagrams, &words), vec![Some(true)]);
    }

    #[test]
    fn a_caption_binds_to_the_diagram_it_overlaps_not_its_neighbor() {
        // The line crosses the gutter, but most of it sits under the right diagram.
        let diagrams = [
            rect(30.0, 400.0, 160.0, 560.0),
            rect(180.0, 400.0, 330.0, 560.0),
        ];
        let texts = [text("Black to play", 145.0, 368.0, 255.0, 384.0)];
        assert_eq!(
            bind_side_captions(&diagrams, &texts),
            vec![None, Some(false)]
        );
    }

    #[test]
    fn two_captions_in_one_band_abstain() {
        let diagrams = [rect(40.0, 400.0, 200.0, 560.0)];
        let texts = [
            text("White to move", 40.0, 372.0, 150.0, 386.0),
            text("Black to play", 40.0, 350.0, 150.0, 364.0),
        ];
        assert_eq!(bind_side_captions(&diagrams, &texts), vec![None]);
    }

    #[test]
    fn bbox_words_on_a_mixed_page_bind_after_the_coordinate_flip() {
        let html = r#"
            <html><body><doc>
            <page width="400.000000" height="600.000000">
              <word xMin="40" yMin="210" xMax="90" yMax="222">White</word>
              <word xMin="94" yMin="210" xMax="108" yMax="222">to</word>
              <word xMin="112" yMin="210" xMax="150" yMax="222">move</word>
              <word xMin="220" yMin="210" xMax="270" yMax="222">Black</word>
              <word xMin="274" yMin="210" xMax="290" yMax="222">o</word>
              <word xMin="294" yMin="210" xMax="300" yMax="222">t</word>
              <word xMin="304" yMin="210" xMax="340" yMax="222">omve</word>
            </page>
            </doc></body></html>
        "#;
        let pages = parse_bbox(html);
        assert_eq!(pages.len(), 1);
        // y from the top of a 600-pt page: the word tops sit at 390, just under y = 400.
        let diagrams = [
            rect(30.0, 400.0, 180.0, 540.0),
            rect(210.0, 400.0, 360.0, 540.0),
        ];
        assert_eq!(
            bind_side_captions(&diagrams, &pages[0]),
            vec![Some(true), Some(false)]
        );
    }

    fn poppler_present() -> bool {
        std::process::Command::new("pdftotext")
            .arg("-v")
            .output()
            .is_ok()
    }

    #[test]
    fn a_generated_pdf_stamps_each_diagram_from_its_caption() {
        use lopdf::{Object, Stream, dictionary};
        if !poppler_present() {
            return;
        }
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("captions.pdf");
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
        // Two diagrams, captions just underneath, different sides. The second page's caption
        // is the scrambled letter order a broken extractor emits.
        let left = page(
            "q 140 0 0 140 40 280 cm /Im0 Do Q\n\
             q 140 0 0 140 220 280 cm /Im0 Do Q\n\
             BT /F1 12 Tf 40 258 Td (White to move) Tj ET\n\
             BT /F1 12 Tf 220 258 Td (Black to move) Tj ET\n",
        );
        let scrambled = page(
            "q 160 0 0 160 40 200 cm /Im0 Do Q\n\
             BT /F1 12 Tf 40 178 Td (White o t omve) Tj ET\n",
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![left.into(), scrambled.into()],
                "Count" => 2,
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc.save(&path).expect("pdf");
        let bytes = std::fs::read(&path).expect("read");
        let mut extracted = super::super::pdf::extract(&bytes).expect("extract");
        super::stamp_geometry_captions(
            &path,
            &extracted.pages,
            &mut extracted.pictures,
            extracted.cover.as_deref(),
        );
        let side = |page: usize, index: usize| {
            crate::books::diagram::stated_caption_side(&extracted.pictures[page][index].bytes)
        };
        assert_eq!(extracted.pictures[0].len(), 2, "two diagrams on page 1");
        assert_eq!(side(0, 0), Some(true), "left caption is White");
        assert_eq!(side(0, 1), Some(false), "right caption is Black");
        assert_eq!(extracted.pictures[1].len(), 1);
        assert_eq!(side(1, 0), Some(true), "scrambled caption is White");
    }
}
