//! Per-computer library choices and first-run policy, independent of the UI and sync.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum LibrarySize {
    Starter,
    #[default]
    Medium,
    Full,
}
pub const STARTER_COUNT: u64 = 8_540;
pub const RATING_BANDS: usize = 13;
pub const STARTER_HISTOGRAM: [u64; RATING_BANDS] =
    [9, 544, 894, 891, 896, 892, 891, 891, 893, 819, 769, 151, 0];
pub const MEDIUM_HISTOGRAM: [u64; RATING_BANDS] = [
    1378, 4428, 16929, 31591, 32376, 34849, 28947, 21597, 15643, 8269, 2902, 1003, 88,
];
pub const FULL_HISTOGRAM: [u64; RATING_BANDS] = [
    15607, 51582, 200_536, 367_536, 376_274, 407_908, 336_316, 254_551, 181_758, 97184, 34051,
    11346, 1154,
];
/// Measured from the 2026-10-22 dump, including final SQLite checkpoint/close.
pub struct LibraryEstimate {
    pub download_bytes: u64,
    pub disk_bytes: u64,
    pub local_seconds: f64,
    pub disk_label: &'static str,
    pub time_label: &'static str,
}
pub const STARTER_ESTIMATE: LibraryEstimate = LibraryEstimate {
    download_bytes: 0,
    disk_bytes: 4_706_304,
    local_seconds: 0.28,
    disk_label: "≈5 MB",
    time_label: "Ready offline",
};
pub const MEDIUM_ESTIMATE: LibraryEstimate = LibraryEstimate {
    download_bytes: 26_354_267,
    disk_bytes: 101_732_352,
    local_seconds: 15.03,
    disk_label: "≈102 MB",
    time_label: "≈20–60 seconds",
};
pub const FULL_ESTIMATE: LibraryEstimate = LibraryEstimate {
    download_bytes: 307_234_795,
    disk_bytes: 1_208_946_688,
    local_seconds: 524.61,
    disk_label: "≈1.21 GB",
    time_label: "≈10–17 minutes",
};
pub fn rating_band(rating: u16) -> usize {
    usize::from(rating.saturating_sub(400) / 200).min(RATING_BANDS - 1)
}
impl LibrarySize {
    pub const ALL: [Self; 3] = [Self::Starter, Self::Medium, Self::Full];
    pub fn key(self) -> &'static str {
        match self {
            Self::Starter => "starter",
            Self::Medium => "medium",
            Self::Full => "full",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.key() == value)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Starter => "Starter",
            Self::Medium => "Medium",
            Self::Full => "Full",
        }
    }
    pub fn limit(self) -> Option<u64> {
        match self {
            Self::Starter => Some(STARTER_COUNT),
            Self::Medium => Some(200_000),
            Self::Full => None,
        }
    }
    pub fn puzzles(self) -> &'static str {
        match self {
            Self::Starter => "8,540",
            Self::Medium => "200,000",
            Self::Full => "≈2.3 million",
        }
    }
    pub fn estimate(self) -> &'static LibraryEstimate {
        match self {
            Self::Starter => &STARTER_ESTIMATE,
            Self::Medium => &MEDIUM_ESTIMATE,
            Self::Full => &FULL_ESTIMATE,
        }
    }
    pub fn download_bytes(self) -> u64 {
        self.estimate().download_bytes
    }
    pub fn disk(self) -> &'static str {
        self.estimate().disk_label
    }
    pub fn time(self) -> &'static str {
        self.estimate().time_label
    }
    pub fn histogram(self) -> &'static [u64; RATING_BANDS] {
        match self {
            Self::Starter => &STARTER_HISTOGRAM,
            Self::Medium => &MEDIUM_HISTOGRAM,
            Self::Full => &FULL_HISTOGRAM,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnboardingDecision {
    Show,
    AlreadyDone,
    MarkDone,
}
pub fn onboarding_decision(done: bool, count: u64) -> OnboardingDecision {
    if done {
        OnboardingDecision::AlreadyDone
    } else if count > STARTER_COUNT + 1_000 {
        OnboardingDecision::MarkDone
    } else {
        OnboardingDecision::Show
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ImportOptions {
    pub limit: Option<u64>,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn choices_and_first_run() {
        assert_eq!(LibrarySize::Medium.limit(), Some(200_000));
        assert_eq!(LibrarySize::Full.limit(), None);
        for size in LibrarySize::ALL {
            assert_eq!(LibrarySize::parse(size.key()), Some(size));
        }
        assert_eq!(onboarding_decision(false, 0), OnboardingDecision::Show);
        assert_eq!(
            onboarding_decision(false, STARTER_COUNT),
            OnboardingDecision::Show
        );
        assert_eq!(
            onboarding_decision(false, 20_000),
            OnboardingDecision::MarkDone
        );
        assert_eq!(
            onboarding_decision(true, 0),
            OnboardingDecision::AlreadyDone
        );
        assert_eq!(rating_band(399), 0);
        assert_eq!(rating_band(600), 1);
        assert_eq!(rating_band(3200), 12);
    }
}
