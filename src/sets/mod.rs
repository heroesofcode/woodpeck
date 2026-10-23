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
    pub target_rating: u16,
    pub variance: u16,
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
    pub restrict_recommended: bool,
    pub preset_id: Option<String>,
    pub shuffle_each_cycle: bool,
}
impl Default for BuilderParams {
    fn default() -> Self {
        Self {
            name: "Starter Set".into(),
            target_rating: 1000,
            variance: 100,
            size: 100,
            target_cycles: 5,
            include_themes: Vec::new(),
            exclude_themes: Vec::new(),
            theme_match: ThemeMatch::Any,
            restrict_recommended: true,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error(
    "Choose a name, 20–500 puzzles, 1–10 cycles, a valid target rating, variance ±50–200 in steps of 25, and known themes that are not both included and excluded."
)]
pub struct InvalidParams;
impl BuilderParams {
    pub fn validate(&self, user_rating: u16) -> Result<(), InvalidParams> {
        let (low, high) = self.target_limits(user_rating);
        if self.name.trim().is_empty()
            || !(20..=500).contains(&self.size)
            || !(1..=10).contains(&self.target_cycles)
            || !(50..=200).contains(&self.variance)
            || !self.variance.is_multiple_of(25)
            || !(low..=high).contains(&self.target_rating)
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
    pub fn target_limits(&self, user_rating: u16) -> (u16, u16) {
        if self.restrict_recommended {
            recommended_range(user_rating)
        } else {
            (400, 3000)
        }
    }
    pub fn band(&self) -> (u16, u16) {
        (
            self.target_rating.saturating_sub(self.variance).max(400),
            self.target_rating.saturating_add(self.variance).min(3000),
        )
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
/// Inferred thresholds: at least 200 below your rating is comfortable; above it challenging.
pub fn difficulty_caption(target: u16, rating: u16) -> &'static str {
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
            assert_eq!(b.variance, 100);
            b.validate(1200).unwrap();
            match p.kind {
                PresetKind::Theme(theme) => {
                    assert_eq!(b.include_themes, vec![theme.to_string()]);
                    assert_eq!(b.exclude_themes, Vec::<String>::new());
                    assert_eq!(b.target_rating, 1000);
                    assert!(b.restrict_recommended);
                }
                PresetKind::Level(a, z) => {
                    assert_eq!(b.target_rating, u16::midpoint(a, z));
                    assert!(!b.restrict_recommended);
                }
                PresetKind::General => assert!(b.is_general()),
            }
        }
        assert_eq!(
            Preset::find("theme-forks")
                .unwrap()
                .builder(800)
                .target_rating,
            800
        );
        assert!(Preset::find("unknown").is_none());
    }
    #[test]
    fn ranges_captions_and_time() {
        assert_eq!(recommended_range(1200), (800, 1300));
        assert_eq!(recommended_range(1800), (1400, 1900));
        assert_eq!(recommended_range(400), (800, 800));
        assert_eq!(difficulty_caption(1000, 1200), "Comfortable");
        assert_eq!(difficulty_caption(1100, 1200), "Moderate");
        assert_eq!(difficulty_caption(1300, 1200), "Challenging");
        let p = BuilderParams::default();
        assert_eq!(p.band(), (900, 1100));
        assert_eq!(
            p.estimated_time(),
            (Duration::from_secs(14500), Duration::from_mins(750))
        );
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
                variance: 60,
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
                target_rating: 1500,
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
            assert!(bad.validate(1200).is_err());
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
        b.validate(1200).unwrap();
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
        b.validate(1200).unwrap();
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
}
