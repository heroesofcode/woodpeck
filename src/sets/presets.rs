use super::BuilderParams;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetKind {
    General,
    Theme(&'static str),
    Level(u16, u16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub starter_set_name: &'static str,
    pub description: &'static str,
    pub kind: PresetKind,
}
pub const PRESETS: [Preset; 12] = [
    Preset {
        id: "general",
        label: "General Starter",
        starter_set_name: "Starter Set",
        description: "A balanced first set based on your current training level.",
        kind: PresetKind::General,
    },
    Preset {
        id: "theme-forks",
        label: "Forks",
        starter_set_name: "Forks Starter Set",
        description: "Train double attacks until the geometry is immediate.",
        kind: PresetKind::Theme("fork"),
    },
    Preset {
        id: "theme-pins",
        label: "Pins",
        starter_set_name: "Pins Starter Set",
        description: "Spot restriction tactics and pinned defenders faster.",
        kind: PresetKind::Theme("pin"),
    },
    Preset {
        id: "theme-mates",
        label: "Mates",
        starter_set_name: "Mates Starter Set",
        description: "Internalize the mating patterns that decide real games.",
        kind: PresetKind::Theme("mate"),
    },
    Preset {
        id: "theme-sacrifices",
        label: "Sacrifices",
        starter_set_name: "Sacrifices Starter Set",
        description: "Drill forcing ideas where material gives way to initiative.",
        kind: PresetKind::Theme("sacrifice"),
    },
    Preset {
        id: "theme-skewers",
        label: "Skewers",
        starter_set_name: "Skewers Starter Set",
        description: "Practice long-line tactics that win material cleanly.",
        kind: PresetKind::Theme("skewer"),
    },
    Preset {
        id: "theme-back-rank-mate",
        label: "Back Rank Mate",
        starter_set_name: "Back Rank Mate Starter Set",
        description: "Punish unsafe back ranks before the chance disappears.",
        kind: PresetKind::Theme("backRankMate"),
    },
    Preset {
        id: "level-800-1000",
        label: "800-1000",
        starter_set_name: "800-1000 Starter Set",
        description: "A confidence-building starter band for newer club players.",
        kind: PresetKind::Level(800, 1000),
    },
    Preset {
        id: "level-1000-1200",
        label: "1000-1200",
        starter_set_name: "1000-1200 Starter Set",
        description: "A practical starter band for early tactical consistency.",
        kind: PresetKind::Level(1000, 1200),
    },
    Preset {
        id: "level-1200-1400",
        label: "1200-1400",
        starter_set_name: "1200-1400 Starter Set",
        description: "A solid range for players building repeatable pattern speed.",
        kind: PresetKind::Level(1200, 1400),
    },
    Preset {
        id: "level-1400-1600",
        label: "1400-1600",
        starter_set_name: "1400-1600 Starter Set",
        description: "A sharper band for players ready to compress cycle times.",
        kind: PresetKind::Level(1400, 1600),
    },
    Preset {
        id: "level-1600-1800",
        label: "1600-1800",
        starter_set_name: "1600-1800 Starter Set",
        description: "A stronger starter band for advanced club-level repetition.",
        kind: PresetKind::Level(1600, 1800),
    },
];
impl Preset {
    pub fn find(id: &str) -> Option<&'static Self> {
        PRESETS.iter().find(|p| p.id == id)
    }
    /// Matches the source prefill function: general has no explicit form prefill.
    pub fn prefill(self, user_rating: u16) -> Option<BuilderParams> {
        let mut p = BuilderParams {
            name: self.starter_set_name.into(),
            preset_id: Some(self.id.into()),
            ..Default::default()
        };
        match self.kind {
            PresetKind::General => return None,
            PresetKind::Theme(theme) => {
                p.set_range(around(user_rating.saturating_sub(200).max(800)));
                p.include_themes = vec![theme.into()];
            }
            PresetKind::Level(low, high) => p.set_range((low, high)),
        }
        Some(p)
    }
    pub fn builder(self, user_rating: u16) -> BuilderParams {
        self.prefill(user_rating).unwrap_or_else(|| {
            let mut p = BuilderParams::default();
            p.set_range(around(user_rating.saturating_sub(200).clamp(800, 3000)));
            p
        })
    }
}
/// A ±100 range centred a little below the player, the default difficulty for presets.
fn around(center: u16) -> (u16, u16) {
    super::normalize_range(center.saturating_sub(100), center.saturating_add(100))
}
/// Lichess puzzle theme keys offered by the set builder, with display labels.
pub const THEMES: [(&str, &str); 57] = [
    ("mate", "Mates"),
    ("mateIn1", "Mate in 1"),
    ("mateIn2", "Mate in 2"),
    ("mateIn3", "Mate in 3"),
    ("mateIn4", "Mate in 4"),
    ("backRankMate", "Back Rank Mate"),
    ("smotheredMate", "Smothered Mate"),
    ("anastasiaMate", "Anastasia's Mate"),
    ("arabianMate", "Arabian Mate"),
    ("hookMate", "Hook Mate"),
    ("bodenMate", "Boden's Mate"),
    ("doubleBishopMate", "Double Bishop Mate"),
    ("dovetailMate", "Dovetail Mate"),
    ("fork", "Forks"),
    ("pin", "Pins"),
    ("skewer", "Skewers"),
    ("discoveredAttack", "Discovered Attacks"),
    ("doubleCheck", "Double Checks"),
    ("xRayAttack", "X-Ray Attacks"),
    ("attraction", "Attraction"),
    ("deflection", "Deflection"),
    ("decoy", "Decoy"),
    ("clearance", "Clearance"),
    ("interference", "Interference"),
    ("intermezzo", "Intermezzo"),
    ("capturingDefender", "Capturing the Defender"),
    ("sacrifice", "Sacrifices"),
    ("hangingPiece", "Hanging Pieces"),
    ("trappedPiece", "Trapped Pieces"),
    ("exposedKing", "Exposed King"),
    ("kingsideAttack", "Kingside Attack"),
    ("queensideAttack", "Queenside Attack"),
    ("attackingF2F7", "Attacking f2/f7"),
    ("quietMove", "Quiet Moves"),
    ("defensiveMove", "Defensive Moves"),
    ("zugzwang", "Zugzwang"),
    ("advancedPawn", "Advanced Pawns"),
    ("promotion", "Promotion"),
    ("underPromotion", "Underpromotion"),
    ("enPassant", "En Passant"),
    ("castling", "Castling"),
    ("opening", "Opening"),
    ("middlegame", "Middlegame"),
    ("endgame", "Endgame"),
    ("rookEndgame", "Rook Endgames"),
    ("bishopEndgame", "Bishop Endgames"),
    ("knightEndgame", "Knight Endgames"),
    ("pawnEndgame", "Pawn Endgames"),
    ("queenEndgame", "Queen Endgames"),
    ("queenRookEndgame", "Queen & Rook Endgames"),
    ("crushing", "Crushing"),
    ("advantage", "Advantage"),
    ("equality", "Equality"),
    ("oneMove", "One Move"),
    ("short", "Short"),
    ("long", "Long"),
    ("veryLong", "Very Long"),
];
/// Lichess category tags that describe *every* puzzle rather than a tactical motif: game
/// phase (each puzzle has exactly one), solution length (exactly one), evaluation outcome
/// (exactly one) and game source. They stay selectable as filters, but counting them toward a
/// general set's per-theme diversity cap would starve the set: one phase tag per puzzle caps a
/// set at three times the cap regardless of how many candidates exist.
pub const CATEGORY_THEMES: [&str; 14] = [
    "opening",
    "middlegame",
    "endgame",
    "oneMove",
    "short",
    "long",
    "veryLong",
    "mate",
    "crushing",
    "advantage",
    "equality",
    "master",
    "masterVsMaster",
    "superGM",
];
/// Whether `key` is a universal category tag (see [`CATEGORY_THEMES`]) rather than a motif.
pub fn is_category_theme(key: &str) -> bool {
    CATEGORY_THEMES.contains(&key)
}
/// Curated builder label when known, otherwise the title-cased Lichess key.
pub fn theme_name(key: &str) -> String {
    THEMES
        .iter()
        .find(|(k, _)| *k == key)
        .map_or_else(|| theme_label(key), |(_, label)| (*label).into())
}
pub fn theme_label(key: &str) -> String {
    let mut result = String::new();
    for (i, c) in key.chars().enumerate() {
        if i == 0 {
            result.extend(c.to_uppercase());
        } else {
            if c.is_uppercase() {
                result.push(' ');
            }
            result.push(c);
        }
    }
    result
}
