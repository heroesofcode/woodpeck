//! A lone `W` or `B` printed beside a diagram's top edge names the side to move.
//!
//! Some publishers print the side to move as one capital letter just left of the board,
//! level with its top ranks, instead of a "White to move" caption. Scans turned into text
//! often read that boxed `W` as a lowercase `w`; a lowercase `b` is never taken, because it
//! is also a file letter printed along the frame.
//!
//! The letter must stand alone on its line, sit in the narrow band left of exactly one
//! diagram, and be the only letter there. A question under the board that names the other
//! side as the one acting ("How did Black ...?") abstains, since one of the two is misread.

use super::caption::{Rect, TextBox, lines_of};

/// `Some(true)` for White, `Some(false)` for Black.
fn letter_side(text: &str) -> Option<bool> {
    match text.trim() {
        "W" | "w" => Some(true),
        "B" => Some(false),
        _ => None,
    }
}

fn height(rect: Rect) -> f64 {
    (rect.top - rect.bottom).max(1.0)
}

fn width(rect: Rect) -> f64 {
    (rect.right - rect.left).max(1.0)
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

fn inside(diagram: Rect, rect: Rect) -> bool {
    let cx = f64::midpoint(rect.left, rect.right);
    let cy = f64::midpoint(rect.bottom, rect.top);
    (diagram.left..=diagram.right).contains(&cx) && (diagram.bottom..=diagram.top).contains(&cy)
}

/// The letter sits left of `diagram`, close to its frame and level with its top ranks.
fn beside_top(diagram: Rect, letter: Rect) -> bool {
    let d_width = width(diagram);
    let d_height = height(diagram);
    let gap = diagram.left - letter.right;
    if !(0.0..=0.22 * d_width).contains(&gap) {
        return false;
    }
    // Small next to the board: a letter, not a heading.
    if height(letter) > 0.12 * d_height {
        return false;
    }
    let cy = f64::midpoint(letter.bottom, letter.top);
    let from_top = diagram.top - cy;
    (-0.05 * d_height..=0.45 * d_height).contains(&from_top)
}

/// No other word on the letter's row within most of a letter height, so `B` is not the
/// first half of a split word such as `B lack`. Prose across a column gutter is farther.
fn isolated(letter: &TextBox, words: &[TextBox]) -> bool {
    let h = height(letter.rect);
    let reach = 0.8 * h;
    !words.iter().any(|other| {
        if std::ptr::eq(other, letter) {
            return false;
        }
        let same_row = overlap(
            letter.rect.bottom,
            letter.rect.top,
            other.rect.bottom,
            other.rect.top,
        ) > 0.3 * h.min(height(other.rect));
        let gap = if other.rect.left >= letter.rect.right {
            other.rect.left - letter.rect.right
        } else if other.rect.right <= letter.rect.left {
            letter.rect.left - other.rect.right
        } else {
            0.0
        };
        same_row && gap < reach
    })
}

/// The side a question names as the one acting: "How did White ...", "What should Black
/// play", "White to play". `None` when there is no such phrase or both sides are named.
pub(super) fn acting_side(text: &str) -> Option<bool> {
    const LEADS: &[&str] = &[
        "how did",
        "how does",
        "how do",
        "how can",
        "how could",
        "how should",
        "how would",
        "how might",
        "how will",
        "what did",
        "what does",
        "what should",
        "what would",
        "what can",
        "what could",
        "can",
        "should",
    ];
    let lower: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let mut found: Option<bool> = None;
    let mut conflict = false;
    let mut note = |white: bool| match found {
        Some(have) if have != white => conflict = true,
        _ => found = Some(white),
    };
    for (i, word) in words.iter().enumerate() {
        let white = match *word {
            "white" => true,
            "black" => false,
            _ => continue,
        };
        let led = LEADS.iter().any(|lead| {
            let lead: Vec<&str> = lead.split(' ').collect();
            i >= lead.len() && words[i - lead.len()..i] == lead[..]
        });
        let to_play = matches!(
            (words.get(i + 1), words.get(i + 2)),
            (Some(&"to"), Some(&("play" | "move")))
        );
        if led || to_play {
            note(white);
        }
    }
    if conflict { None } else { found }
}

/// Text under `diagram`, top line first, as one string: a question may wrap onto a second line.
fn text_below(diagram: Rect, diagrams: &[Rect], texts: &[TextBox]) -> String {
    let mut lines: Vec<TextBox> = lines_of(texts.to_vec())
        .into_iter()
        .filter(|line| {
            let below = diagram.bottom - line.rect.top;
            let reach = (0.4 * height(diagram)).clamp(30.0, 70.0);
            (-0.1 * height(diagram)..=reach).contains(&below)
                && overlap(diagram.left, diagram.right, line.rect.left, line.rect.right)
                    > 0.3 * width(line.rect).min(width(diagram))
                && !diagrams.iter().any(|other| inside(*other, line.rect))
        })
        .collect();
    lines.sort_by(|a, b| b.rect.top.total_cmp(&a.rect.top));
    lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One side per diagram from a lone letter beside it. A letter next to two diagrams, two
/// different letters, or a question naming the other side abstains.
pub(super) fn bind_side_letters(diagrams: &[Rect], texts: &[TextBox]) -> Vec<Option<bool>> {
    let mut owned: Vec<Vec<bool>> = vec![Vec::new(); diagrams.len()];
    for word in texts {
        let Some(white) = letter_side(&word.text) else {
            continue;
        };
        if diagrams.iter().any(|d| inside(*d, word.rect)) || !isolated(word, texts) {
            continue;
        }
        let near: Vec<usize> = diagrams
            .iter()
            .enumerate()
            .filter(|(_, d)| beside_top(**d, word.rect))
            .map(|(i, _)| i)
            .collect();
        if let [index] = near.as_slice() {
            owned[*index].push(white);
        }
    }
    owned
        .into_iter()
        .enumerate()
        .map(|(index, mut sides)| {
            sides.sort_unstable();
            sides.dedup();
            let [white] = sides.as_slice() else {
                return None;
            };
            let question = acting_side(&text_below(diagrams[index], diagrams, texts));
            match question {
                Some(named) if named != *white => None,
                _ => Some(*white),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Rect, TextBox, acting_side, bind_side_letters};

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
    fn a_letter_left_of_the_top_ranks_names_the_side() {
        let diagrams = [
            rect(60.0, 400.0, 216.0, 556.0),
            rect(262.0, 400.0, 418.0, 556.0),
        ];
        let texts = [
            text("w", 46.0, 520.0, 52.0, 530.0),
            text("B", 248.0, 520.0, 254.0, 530.0),
        ];
        assert_eq!(
            bind_side_letters(&diagrams, &texts),
            vec![Some(true), Some(false)]
        );
        // Prose ending across the column gutter, a letter height away, does not hide it.
        let gutter = [
            text("defend", 10.0, 520.0, 34.0, 530.0),
            text("w", 46.0, 520.0, 52.0, 530.0),
        ];
        assert_eq!(bind_side_letters(&diagrams[..1], &gutter), vec![Some(true)]);
        let upper = [text("W", 46.0, 520.0, 53.0, 530.0)];
        assert_eq!(bind_side_letters(&diagrams[..1], &upper), vec![Some(true)]);
    }

    #[test]
    fn a_lowercase_b_low_letters_and_split_words_are_not_side_letters() {
        let diagrams = [rect(60.0, 400.0, 216.0, 556.0)];
        // A file letter `b`.
        let file = [text("b", 46.0, 520.0, 52.0, 530.0)];
        assert_eq!(bind_side_letters(&diagrams, &file), vec![None]);
        // Level with the bottom ranks.
        let low = [text("W", 46.0, 410.0, 53.0, 420.0)];
        assert_eq!(bind_side_letters(&diagrams, &low), vec![None]);
        // `B lack` split by a broken text layer.
        let split = [
            text("B", 40.0, 520.0, 46.0, 530.0),
            text("lack", 48.0, 520.0, 58.0, 530.0),
        ];
        assert_eq!(bind_side_letters(&diagrams, &split), vec![None]);
        // Too far from the frame.
        let far = [text("W", 10.0, 520.0, 17.0, 530.0)];
        assert_eq!(bind_side_letters(&diagrams, &far), vec![None]);
        // Inside the picture: a printed piece letter, not a side.
        let inner = [text("B", 100.0, 520.0, 106.0, 530.0)];
        assert_eq!(bind_side_letters(&diagrams, &inner), vec![None]);
    }

    #[test]
    fn two_letters_or_a_letter_between_two_boards_abstain() {
        let diagrams = [rect(60.0, 400.0, 216.0, 556.0)];
        let both = [
            text("W", 46.0, 530.0, 53.0, 540.0),
            text("B", 46.0, 500.0, 52.0, 510.0),
        ];
        assert_eq!(bind_side_letters(&diagrams, &both), vec![None]);
        // Same letter twice is one answer.
        let twice = [
            text("W", 46.0, 530.0, 53.0, 540.0),
            text("W", 46.0, 500.0, 53.0, 510.0),
        ];
        assert_eq!(bind_side_letters(&diagrams, &twice), vec![Some(true)]);
        let stacked = [
            rect(60.0, 400.0, 216.0, 556.0),
            rect(60.0, 400.0, 216.0, 556.0),
        ];
        assert_eq!(
            bind_side_letters(&stacked, &[text("W", 46.0, 520.0, 53.0, 530.0)]),
            vec![None, None]
        );
    }

    #[test]
    fn a_question_naming_the_other_side_abstains() {
        let diagrams = [rect(60.0, 400.0, 216.0, 556.0)];
        let letter = text("w", 46.0, 520.0, 52.0, 530.0);
        let other = [
            letter.clone(),
            text("Both sides attack, but how", 60.0, 380.0, 210.0, 390.0),
            text("did Black strike first?", 60.0, 368.0, 180.0, 378.0),
        ];
        assert_eq!(bind_side_letters(&diagrams, &other), vec![None]);
        let same = [
            letter.clone(),
            text("How did White win?", 60.0, 380.0, 160.0, 390.0),
        ];
        assert_eq!(bind_side_letters(&diagrams, &same), vec![Some(true)]);
        // A colour that is not acting does not veto.
        let victim = [
            letter,
            text("What has Black overlooked?", 60.0, 380.0, 200.0, 390.0),
        ];
        assert_eq!(bind_side_letters(&diagrams, &victim), vec![Some(true)]);
    }

    #[test]
    fn acting_side_reads_leads_and_to_play() {
        assert_eq!(acting_side("How did White finish the game?"), Some(true));
        assert_eq!(acting_side("What should Black play?"), Some(false));
        assert_eq!(acting_side("Black to play and win"), Some(false));
        assert_eq!(acting_side("What has Black overlooked?"), None);
        assert_eq!(acting_side("How did White reply? How did Black win?"), None);
    }
}
