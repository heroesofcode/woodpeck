//! Conservative word evidence for repairing a physical PDF line break.
use std::collections::{HashMap, HashSet};

/// Whole words elsewhere on the page, excluding once-seen line-edge fragments.
pub(crate) fn vocabulary(text: &str) -> HashSet<String> {
    let mut counts = HashMap::new();
    for word in text
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
    {
        *counts.entry(word.to_lowercase()).or_insert(0usize) += 1;
    }
    let mut words: HashSet<_> = counts.keys().cloned().collect();
    let mut forget = |word: &str| {
        let word = word
            .trim_matches(|c: char| !c.is_alphabetic())
            .to_lowercase();
        if counts.get(&word) == Some(&1) {
            words.remove(&word);
        }
    };
    for pair in text.lines().collect::<Vec<_>>().windows(2) {
        if let Some(word) = pair[0].split_whitespace().last() {
            forget(word);
        }
        if let Some(word) = pair[1].split_whitespace().next() {
            forget(word);
        }
    }
    words
}

/// A missing hyphen is only recoverable when the whole word is attested, neither
/// fragment is independently attested, and both fragments are substantial.
pub(crate) fn lost_hyphen(left: &str, right: &str, words: &HashSet<String>) -> bool {
    let a = left
        .rsplit(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let b = right
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or_default()
        .to_lowercase();
    a.chars().count() >= 3
        && b.chars().count() >= 3
        && !words.contains(&a)
        && !words.contains(&b)
        && words.contains(&format!("{a}{b}"))
}

pub(crate) fn caption(text: &str) -> bool {
    let text = text.trim();
    if text.split_whitespace().take(11).count() > 10 {
        return false;
    }
    let end = text
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(text.len());
    let label = &text[..end];
    if ["diagram", "figure", "fig"]
        .iter()
        .any(|prefix| label.eq_ignore_ascii_case(prefix))
    {
        let rest =
            text[end..].trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ':'));
        return rest.split_whitespace().next().is_none_or(|number| {
            let number = number.trim_matches(['(', ')', '.', ':']);
            number.starts_with(|c: char| c.is_ascii_digit())
                || (!number.is_empty() && number.chars().all(|c| "IVXLCDM".contains(c)))
        });
    }
    if !["white", "black"]
        .iter()
        .any(|prefix| label.eq_ignore_ascii_case(prefix))
    {
        return false;
    }
    let text = text.trim_end_matches(['.', ':']).to_ascii_lowercase();
    [
        "white to move",
        "black to move",
        "white to play",
        "black to play",
        "white's move",
        "black's move",
    ]
    .iter()
    .any(|prefix| {
        text.strip_prefix(prefix)
            .is_some_and(|rest| matches!(rest, "" | " and win" | " and draw"))
    })
}

#[cfg(test)]
mod tests {
    use super::caption;

    #[test]
    fn captions_require_labels_or_complete_turn_instructions() {
        for text in [
            "Diagram 1",
            "Figure IV. The position after the exchange",
            "Fig. (2)",
            "Fig.1",
            "Figure: 2. The open file",
            "Figure:",
            "DIAGRAM.",
            "Diagram(3)",
            "DIAGRAM",
            "White to move.",
            "BLACK TO PLAY AND WIN",
            "Black's move:",
        ] {
            assert!(caption(text), "{text}");
        }
        for text in [
            "Figures show several possible plans",
            "Diagramming the position helps us find",
            "Figure out where the knight belongs",
            "Figure: out where the knight belongs",
            "Fig.ment of the imagination",
            "Diagram-based teaching encourages exploration",
            "White to attack the centre must first develop",
            "White's movement restricts the bishop",
            "Black to play the knight instead of the bishop",
            "",
        ] {
            assert!(!caption(text), "{text}");
        }
    }
}
