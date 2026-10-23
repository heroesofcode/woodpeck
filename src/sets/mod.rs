//! Set-builder parameters, exact presets, and deterministic stratified generation.
mod generation;
mod presets;
pub use generation::{GeneratedSet, GenerationError, PuzzleQuery, QueryError, generate};
pub use presets::{PRESETS, Preset, PresetKind, THEMES, theme_label, theme_name};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub const CYCLE_CARDS: [(u8, &str); 3] = [
    (3, "Minimum effective"),
    (5, "Recommended"),
    (7, "Deep mastery"),
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuilderParams {
    pub name: String,
    /// Inclusive lower bound of puzzle ratings in the set.
    pub rating_min: u16,
    /// Inclusive upper bound of puzzle ratings in the set.
    pub rating_max: u16,
    pub size: u32,
    pub target_cycles: u8,
    /// Lichess theme keys a puzzle must carry (any or all of them, per `theme_match`).
    #[serde(default)]
    pub include_themes: Vec<String>,
    /// Lichess theme keys a puzzle must not carry.
    #[serde(default)]
    pub exclude_themes: Vec<String>,
    #[serde(default)]
    pub theme_match: ThemeMatch,
    pub preset_id: Option<String>,
    pub shuffle_each_cycle: bool,
}
impl Default for BuilderParams {
    fn default() -> Self {
        Self {
            name: "Starter Set".into(),
            rating_min: 900,
            rating_max: 1100,
            size: 100,
            target_cycles: 5,
            include_themes: Vec::new(),
            exclude_themes: Vec::new(),
            theme_match: ThemeMatch::Any,
            preset_id: Some("general".into()),
            shuffle_each_cycle: false,
        }
    }
}
/// How the included themes combine. Excluded themes always reject a puzzle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMatch {
    #[default]
    Any,
    All,
}
/// Where a theme sits in the builder's include/exclude picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeSelection {
    Neutral,
    Included,
    Excluded,
}
/// Builder themes whose label or key contains `query`, ignoring case and spacing.
pub fn search_themes(query: &str) -> impl Iterator<Item = (&'static str, &'static str)> {
    let normalize = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let needle = normalize(query);
    THEMES.into_iter().filter(move |(key, label)| {
        needle.is_empty() || normalize(key).contains(&needle) || normalize(label).contains(&needle)
    })
}
/// Borrowed theme constraints shared by local generation, `SQLite` queries and API fallback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemeFilter<'a> {
    pub include: &'a [String],
    pub exclude: &'a [String],
    pub mode: ThemeMatch,
}
impl ThemeFilter<'_> {
    /// Exclusions win over inclusions; an empty include list accepts every theme.
    pub fn matches(&self, themes: &[String]) -> bool {
        let has = |t: &String| themes.contains(t);
        if self.exclude.iter().any(has) {
            return false;
        }
        match self.mode {
            _ if self.include.is_empty() => true,
            ThemeMatch::Any => self.include.iter().any(has),
            ThemeMatch::All => self.include.iter().all(has),
        }
    }
    pub fn is_unconstrained(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }
}
/// Puzzle ratings a set range may span (the Lichess puzzle database's practical bounds).
pub const RATING_BOUNDS: (u16, u16) = (400, 3200);
/// Narrowest allowed range, so a set always has room for a spread of puzzles.
pub const MIN_RANGE_WIDTH: u16 = 100;
/// Builder slider and quick-range granularity.
pub const RANGE_STEP: u16 = 25;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error(
    "Choose a name, 20–500 puzzles, 1–10 cycles, a rating range within 400–3200 that is at least 100 points wide, and known themes that are not both included and excluded."
)]
pub struct InvalidParams;
/// Whether `(min, max)` is an allowed inclusive rating range.
pub fn valid_range(min: u16, max: u16) -> bool {
    RATING_BOUNDS.0 <= min && max <= RATING_BOUNDS.1 && max >= min.saturating_add(MIN_RANGE_WIDTH)
}
/// Round to the builder step, clamp into bounds and widen to the minimum width, keeping it valid.
pub fn normalize_range(min: u16, max: u16) -> (u16, u16) {
    let round = |v: u16| {
        (v.saturating_add(RANGE_STEP / 2) / RANGE_STEP * RANGE_STEP)
            .clamp(RATING_BOUNDS.0, RATING_BOUNDS.1)
    };
    let (mut min, mut max) = (round(min.min(max)), round(min.max(max)));
    if max < min + MIN_RANGE_WIDTH {
        max = (min + MIN_RANGE_WIDTH).min(RATING_BOUNDS.1);
        min = max - MIN_RANGE_WIDTH;
    }
    (min, max)
}
/// One end of a rating range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeHandle {
    Min,
    Max,
}
/// Move one end of a valid range to `value`, clamped to the bounds. If that would make the range
/// narrower than the minimum width, the other end is pushed along so the result stays valid.
pub fn drag_range((min, max): (u16, u16), handle: RangeHandle, value: u16) -> (u16, u16) {
    let (low, high) = RATING_BOUNDS;
    match handle {
        RangeHandle::Min => {
            let min = value.clamp(low, high - MIN_RANGE_WIDTH);
            (min, max.clamp(min + MIN_RANGE_WIDTH, high))
        }
        RangeHandle::Max => {
            let max = value.clamp(low + MIN_RANGE_WIDTH, high);
            (min.clamp(low, max - MIN_RANGE_WIDTH), max)
        }
    }
}
/// Exact range for a pre-range set (`target ± variance`, clamped to the old 400–3000 limits).
/// Bands narrower than the minimum width, possible only at the old edges, grow upward.
/// The `SQLite` migration and the sync upgrade must both use this rule.
pub fn legacy_range(target: u16, variance: u16) -> (u16, u16) {
    let min = target
        .saturating_sub(variance)
        .clamp(RATING_BOUNDS.0, RATING_BOUNDS.1 - MIN_RANGE_WIDTH);
    let max = target.saturating_add(variance).min(3000);
    (min, max.max(min + MIN_RANGE_WIDTH))
}
/// One-click ranges relative to the player's puzzle rating, shown beside the range slider.
pub fn quick_ranges(user_rating: u16) -> [(&'static str, (u16, u16)); 4] {
    let r = user_rating;
    [
        (
            "Easy",
            normalize_range(r.saturating_sub(500), r.saturating_sub(200)),
        ),
        (
            "My rating ±150",
            normalize_range(r.saturating_sub(150), r.saturating_add(150)),
        ),
        (
            "Hard",
            normalize_range(r.saturating_add(100), r.saturating_add(400)),
        ),
        ("Recommended", {
            let (low, high) = recommended_range(r);
            normalize_range(low, high)
        }),
    ]
}
impl BuilderParams {
    pub fn validate(&self) -> Result<(), InvalidParams> {
        if self.name.trim().is_empty()
            || !(20..=500).contains(&self.size)
            || !(1..=10).contains(&self.target_cycles)
            || !valid_range(self.rating_min, self.rating_max)
            || !valid_theme_list(&self.include_themes)
            || !valid_theme_list(&self.exclude_themes)
            || self
                .include_themes
                .iter()
                .any(|t| self.exclude_themes.contains(t))
        {
            return Err(InvalidParams);
        }
        Ok(())
    }
    /// The inclusive `(min, max)` rating range.
    pub fn range(&self) -> (u16, u16) {
        (self.rating_min, self.rating_max)
    }
    pub fn set_range(&mut self, (min, max): (u16, u16)) {
        (self.rating_min, self.rating_max) = (min, max);
    }
    /// Inclusive on both ends.
    pub fn rating_in_range(&self, rating: u16) -> bool {
        (self.rating_min..=self.rating_max).contains(&rating)
    }
    pub fn estimated_time(&self) -> (Duration, Duration) {
        let n = u64::from(self.size) * u64::from(self.target_cycles);
        (Duration::from_secs(n * 29), Duration::from_secs(n * 90))
    }
    /// General sets cap each theme's share; a chosen include focus intentionally skips the cap.
    pub fn is_general(&self) -> bool {
        self.preset_id.as_deref() == Some("general") && self.include_themes.is_empty()
    }
    pub fn theme_filter(&self) -> ThemeFilter<'_> {
        ThemeFilter {
            include: &self.include_themes,
            exclude: &self.exclude_themes,
            mode: self.theme_match,
        }
    }
    pub fn theme_selection(&self, key: &str) -> ThemeSelection {
        if self.include_themes.iter().any(|t| t == key) {
            ThemeSelection::Included
        } else if self.exclude_themes.iter().any(|t| t == key) {
            ThemeSelection::Excluded
        } else {
            ThemeSelection::Neutral
        }
    }
    /// Remove a theme from whichever list holds it.
    pub fn clear_theme(&mut self, key: &str) {
        self.include_themes.retain(|t| t != key);
        self.exclude_themes.retain(|t| t != key);
    }
    /// Toggle a theme through none → include → exclude → none, keeping lists sorted.
    pub fn cycle_theme(&mut self, key: &str) {
        if let Some(i) = self.include_themes.iter().position(|t| t == key) {
            self.include_themes.remove(i);
            insert_sorted(&mut self.exclude_themes, key);
        } else if let Some(i) = self.exclude_themes.iter().position(|t| t == key) {
            self.exclude_themes.remove(i);
        } else {
            insert_sorted(&mut self.include_themes, key);
        }
    }
    /// Short human summary used by set cards, e.g. "Forks or Pins · not Mates".
    pub fn theme_summary(&self) -> Option<String> {
        let joiner = match self.theme_match {
            ThemeMatch::Any => " or ",
            ThemeMatch::All => " + ",
        };
        let include = self
            .include_themes
            .iter()
            .map(|t| theme_name(t))
            .collect::<Vec<_>>()
            .join(joiner);
        let exclude = self
            .exclude_themes
            .iter()
            .map(|t| theme_name(t))
            .collect::<Vec<_>>()
            .join(", ");
        match (include.is_empty(), exclude.is_empty()) {
            (true, true) => None,
            (false, true) => Some(include),
            (true, false) => Some(format!("Not {exclude}")),
            (false, false) => Some(format!("{include} · not {exclude}")),
        }
    }
}
fn insert_sorted(list: &mut Vec<String>, key: &str) {
    if let Err(i) = list.binary_search_by(|t| t.as_str().cmp(key)) {
        list.insert(i, key.into());
    }
}
/// Known keys only, sorted and without duplicates, so stored and synced params are canonical.
fn valid_theme_list(list: &[String]) -> bool {
    list.iter().all(|t| THEMES.iter().any(|(key, _)| key == t))
        && list.windows(2).all(|w| w[0] < w[1])
}
/// A rating below 700 still has a usable single-value recommended slider at 800.
pub fn recommended_range(rating: u16) -> (u16, u16) {
    let low = rating.saturating_sub(400).clamp(800, 3000);
    (low, rating.saturating_add(100).clamp(low, 3000))
}
/// Inferred thresholds from the range midpoint: at least 200 below your rating is comfortable.
pub fn difficulty_caption((min, max): (u16, u16), rating: u16) -> &'static str {
    let target = u16::midpoint(min, max);
    if target <= rating.saturating_sub(200) {
        "Comfortable"
    } else if target <= rating {
        "Moderate"
    } else {
        "Challenging"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_presets_and_prefills() {
        assert_eq!(PRESETS.len(), 12);
        assert_eq!(Preset::find("general").unwrap().prefill(1200), None);
        for p in PRESETS {
            let b = p.builder(1200);
            assert_eq!(b.size, 100);
            assert_eq!(b.target_cycles, 5);
            b.validate().unwrap();
            match p.kind {
                PresetKind::Theme(theme) => {
                    assert_eq!(b.include_themes, vec![theme.to_string()]);
                    assert_eq!(b.exclude_themes, Vec::<String>::new());
                    assert_eq!(b.range(), (900, 1100));
                }
                PresetKind::Level(a, z) => assert_eq!(b.range(), (a, z)),
                PresetKind::General => {
                    assert!(b.is_general());
                    assert_eq!(b.range(), (900, 1100));
                }
            }
        }
        assert_eq!(
            Preset::find("theme-forks").unwrap().builder(800).range(),
            (700, 900)
        );
        assert!(Preset::find("unknown").is_none());
    }
    #[test]
    fn ranges_captions_and_time() {
        assert_eq!(recommended_range(1200), (800, 1300));
        assert_eq!(recommended_range(1800), (1400, 1900));
        assert_eq!(recommended_range(400), (800, 800));
        assert_eq!(difficulty_caption((900, 1100), 1200), "Comfortable");
        assert_eq!(difficulty_caption((1000, 1200), 1200), "Moderate");
        assert_eq!(difficulty_caption((1200, 1400), 1200), "Challenging");
        let p = BuilderParams::default();
        assert_eq!(p.range(), (900, 1100));
        assert_eq!(
            p.estimated_time(),
            (Duration::from_secs(14500), Duration::from_mins(750))
        );
    }
    #[test]
    fn range_validation_normalization_and_legacy_mapping() {
        assert!(valid_range(400, 500));
        assert!(valid_range(3100, 3200));
        assert!(valid_range(1000, 1100));
        assert!(!valid_range(1000, 1099), "narrower than the minimum width");
        assert!(!valid_range(1200, 1200), "min == max is too narrow");
        assert!(!valid_range(1300, 1200), "min above max");
        assert!(!valid_range(399, 600));
        assert!(!valid_range(3000, 3201));
        assert_eq!(normalize_range(1012, 1138), (1000, 1150));
        assert_eq!(normalize_range(1200, 1200), (1200, 1300));
        assert_eq!(normalize_range(1300, 1200), (1200, 1300));
        assert_eq!(normalize_range(3190, 3300), (3100, 3200));
        assert_eq!(normalize_range(0, 10), (400, 500));
        for (min, max) in [(0, 0), (1200, 1200), (3300, 3300), (1013, 1037)] {
            let (a, b) = normalize_range(min, max);
            assert!(valid_range(a, b), "{min}-{max} -> {a}-{b}");
        }
        assert_eq!(legacy_range(1000, 100), (900, 1100));
        assert_eq!(legacy_range(1137, 75), (1062, 1212));
        assert_eq!(legacy_range(420, 50), (400, 500));
        assert_eq!(legacy_range(2990, 200), (2790, 3000));
        assert_eq!(legacy_range(3000, 50), (2950, 3050));
        for (label, (a, b)) in quick_ranges(1200) {
            assert!(valid_range(a, b), "{label}");
            assert_eq!(a % RANGE_STEP, 0);
        }
        assert_eq!(quick_ranges(1200)[1], ("My rating ±150", (1050, 1350)));
        // Dragging keeps min <= max - 100, pushing the other handle at the limit.
        assert_eq!(drag_range((900, 1100), RangeHandle::Min, 950), (950, 1100));
        assert_eq!(
            drag_range((900, 1100), RangeHandle::Min, 1050),
            (1050, 1150)
        );
        assert_eq!(
            drag_range((900, 1100), RangeHandle::Min, 3200),
            (3100, 3200)
        );
        assert_eq!(drag_range((900, 1100), RangeHandle::Min, 0), (400, 1100));
        assert_eq!(drag_range((900, 1100), RangeHandle::Max, 950), (850, 950));
        assert_eq!(drag_range((900, 1100), RangeHandle::Max, 400), (400, 500));
        assert_eq!(drag_range((900, 1100), RangeHandle::Max, 9999), (900, 3200));
        for value in (0..3500).step_by(37) {
            for handle in [RangeHandle::Min, RangeHandle::Max] {
                let (a, b) = drag_range((1000, 1100), handle, value);
                assert!(valid_range(a, b));
            }
        }
        assert_eq!(quick_ranges(400)[0].1, (400, 500));
        assert_eq!(quick_ranges(3000)[2].1, (3100, 3200));
    }
    #[test]
    fn rejects_invalid_builder_values() {
        let b = BuilderParams::default();
        for bad in [
            BuilderParams {
                size: 19,
                ..b.clone()
            },
            BuilderParams {
                size: 501,
                ..b.clone()
            },
            BuilderParams {
                rating_min: 1100,
                rating_max: 1100,
                ..b.clone()
            },
            BuilderParams {
                rating_min: 1200,
                rating_max: 1100,
                ..b.clone()
            },
            BuilderParams {
                rating_min: 300,
                ..b.clone()
            },
            BuilderParams {
                rating_max: 3250,
                ..b.clone()
            },
            BuilderParams {
                target_cycles: 11,
                ..b.clone()
            },
            BuilderParams {
                name: " ".into(),
                ..b.clone()
            },
            BuilderParams {
                include_themes: vec!["unknown".into()],
                ..b.clone()
            },
            BuilderParams {
                exclude_themes: vec!["unknown".into()],
                ..b.clone()
            },
            BuilderParams {
                include_themes: vec!["pin".into(), "fork".into()],
                ..b.clone()
            },
            BuilderParams {
                include_themes: vec!["fork".into(), "fork".into()],
                ..b.clone()
            },
            BuilderParams {
                include_themes: vec!["fork".into()],
                exclude_themes: vec!["fork".into()],
                ..b.clone()
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }
    #[test]
    fn focus_labels_and_cycle_cards() {
        assert!(THEMES.len() >= 15);
        assert!(THEMES.windows(2).all(|w| w[0].0 != w[1].0));
        assert!(THEMES.iter().all(|(key, _)| !key.is_empty()));
        assert_eq!(theme_label("backRankMate"), "Back Rank Mate");
        assert_eq!(theme_name("fork"), "Forks");
        assert_eq!(theme_name("someNewTheme"), "Some New Theme");
        assert_eq!(CYCLE_CARDS[1], (5, "Recommended"));
    }
    fn themes(list: &[&str]) -> Vec<String> {
        list.iter().map(|t| (*t).to_string()).collect()
    }
    #[test]
    fn theme_filter_any_all_exclude_overlap_and_empty() {
        let fork_pin = themes(&["fork", "pin"]);
        let fork = themes(&["fork"]);
        let mate = themes(&["mate", "short"]);
        let none: Vec<String> = Vec::new();
        let include = themes(&["fork", "pin"]);
        let exclude = themes(&["mate"]);
        let any = ThemeFilter {
            include: &include,
            exclude: &[],
            mode: ThemeMatch::Any,
        };
        assert!(any.matches(&fork) && any.matches(&fork_pin));
        assert!(!any.matches(&mate) && !any.matches(&none));
        let all = ThemeFilter {
            mode: ThemeMatch::All,
            ..any
        };
        assert!(all.matches(&fork_pin));
        assert!(!all.matches(&fork) && !all.matches(&none));
        let excluding = ThemeFilter {
            include: &[],
            exclude: &exclude,
            mode: ThemeMatch::All,
        };
        assert!(excluding.matches(&fork) && excluding.matches(&none));
        assert!(!excluding.matches(&mate));
        // Overlap: exclusion wins, even in match-any mode.
        let overlap = ThemeFilter {
            include: &fork,
            exclude: &fork,
            mode: ThemeMatch::Any,
        };
        assert!(!overlap.matches(&fork_pin));
        let empty = ThemeFilter::default();
        assert!(empty.is_unconstrained());
        assert!(empty.matches(&none) && empty.matches(&mate));
    }
    #[test]
    fn cycling_themes_keeps_canonical_lists_and_summary() {
        let mut b = BuilderParams::default();
        assert_eq!(b.theme_summary(), None);
        b.cycle_theme("pin");
        b.cycle_theme("fork");
        assert_eq!(b.include_themes, themes(&["fork", "pin"]));
        b.validate().unwrap();
        assert_eq!(b.theme_summary().unwrap(), "Forks or Pins");
        b.theme_match = ThemeMatch::All;
        assert_eq!(b.theme_summary().unwrap(), "Forks + Pins");
        b.cycle_theme("pin");
        assert_eq!(b.include_themes, themes(&["fork"]));
        assert_eq!(b.exclude_themes, themes(&["pin"]));
        assert_eq!(b.theme_summary().unwrap(), "Forks · not Pins");
        b.cycle_theme("pin");
        b.cycle_theme("fork");
        b.cycle_theme("mate");
        b.cycle_theme("mate");
        assert_eq!(b.include_themes, Vec::<String>::new());
        assert_eq!(b.exclude_themes, themes(&["fork", "mate"]));
        assert_eq!(b.theme_summary().unwrap(), "Not Forks, Mates");
        b.validate().unwrap();
        assert!(!b.is_general() || b.include_themes.is_empty());
    }
    #[test]
    fn legacy_params_without_theme_lists_deserialize() {
        let mut value = serde_json::to_value(BuilderParams::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        for key in ["include_themes", "exclude_themes", "theme_match"] {
            object.remove(key);
        }
        let params: BuilderParams = serde_json::from_value(value).unwrap();
        assert_eq!(params, BuilderParams::default());
        assert_eq!(
            serde_json::to_value(ThemeMatch::All).unwrap(),
            serde_json::json!("all")
        );
    }
    #[test]
    fn theme_search_and_selection_state() {
        let found: Vec<_> = search_themes("back rank").map(|(k, _)| k).collect();
        assert_eq!(found, vec!["backRankMate"]);
        assert!(search_themes("MATEIN2").any(|(k, _)| k == "mateIn2"));
        assert_eq!(search_themes("").count(), THEMES.len());
        assert_eq!(search_themes("zzz-nothing").count(), 0);
        let mut b = BuilderParams::default();
        assert_eq!(b.theme_selection("fork"), ThemeSelection::Neutral);
        b.cycle_theme("fork");
        assert_eq!(b.theme_selection("fork"), ThemeSelection::Included);
        b.cycle_theme("fork");
        assert_eq!(b.theme_selection("fork"), ThemeSelection::Excluded);
        b.clear_theme("fork");
        assert_eq!(b.theme_selection("fork"), ThemeSelection::Neutral);
    }
}
