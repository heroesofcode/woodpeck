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
    let text = text.trim().to_lowercase();
    text.split_whitespace().count() <= 10
        && [
            "diagram",
            "figure",
            "fig.",
            "white to",
            "black to",
            "white's move",
            "black's move",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}
