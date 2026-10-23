//! Set-builder parameters, exact presets, and deterministic stratified generation.
mod generation;
mod presets;
pub use generation::{GeneratedSet, GenerationError, PuzzleQuery, QueryError, generate};
pub use presets::{PRESETS, Preset, PresetKind, THEME_FOCUS, theme_label};
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
    pub focus_theme: Option<String>,
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
            focus_theme: None,
            restrict_recommended: true,
            preset_id: Some("general".into()),
            shuffle_each_cycle: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error(
    "Choose a name, 20–500 puzzles, 1–10 cycles, a valid target rating and variance ±50–200 in steps of 25."
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
            || self
                .focus_theme
                .as_ref()
                .is_some_and(|t| !THEME_FOCUS[1..].iter().any(|(key, _)| key == t))
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
    pub fn is_general(&self) -> bool {
        self.preset_id.as_deref() == Some("general") && self.focus_theme.is_none()
    }
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
                    assert_eq!(b.focus_theme.as_deref(), Some(theme));
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
                focus_theme: Some("unknown".into()),
                ..b.clone()
            },
        ] {
            assert!(bad.validate(1200).is_err());
        }
    }
    #[test]
    fn focus_labels_and_cycle_cards() {
        assert_eq!(THEME_FOCUS.len(), 16);
        assert_eq!(theme_label("backRankMate"), "Back Rank Mate");
        assert_eq!(CYCLE_CARDS[1], (5, "Recommended"));
    }
}
