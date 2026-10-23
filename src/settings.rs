//! Serializable application preferences; persisted atomically as a settings value.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorTheme {
    #[default]
    System,
    Omarchy,
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PieceRendering {
    #[default]
    Vector,
    Cburnett,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardTheme {
    #[default]
    Peck,
    Tournament,
    Slate,
    Blueprint,
    Rosewood,
    Lagoon,
    Plum,
    Carbon,
}
impl BoardTheme {
    pub const ALL: [Self; 8] = [
        Self::Peck,
        Self::Tournament,
        Self::Slate,
        Self::Blueprint,
        Self::Rosewood,
        Self::Lagoon,
        Self::Plum,
        Self::Carbon,
    ];
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub board_theme: BoardTheme,
    pub piece_rendering: PieceRendering,
    pub completion_sound: bool,
    pub show_theme_labels: bool,
    pub auto_advance: bool,
    pub color_theme: ColorTheme,
    pub user_rating: u16,
    pub shuffle_each_cycle: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            board_theme: BoardTheme::Peck,
            piece_rendering: PieceRendering::Vector,
            completion_sound: true,
            show_theme_labels: true,
            auto_advance: false,
            color_theme: ColorTheme::System,
            user_rating: 1200,
            shuffle_each_cycle: false,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> bool {
        (400..=3000).contains(&self.user_rating)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_partial_migration() {
        let s: Settings = serde_json::from_str("{\"auto_advance\":true}").unwrap();
        assert!(s.auto_advance);
        assert_eq!(s.user_rating, 1200);
        assert_eq!(s.board_theme, BoardTheme::Peck);
        assert!(s.validate());
    }
    #[test]
    fn all_options_roundtrip() {
        for board_theme in BoardTheme::ALL {
            for color_theme in [
                ColorTheme::System,
                ColorTheme::Omarchy,
                ColorTheme::Light,
                ColorTheme::Dark,
            ] {
                let s = Settings {
                    board_theme,
                    color_theme,
                    piece_rendering: PieceRendering::Cburnett,
                    ..Default::default()
                };
                assert_eq!(
                    serde_json::from_str::<Settings>(&serde_json::to_string(&s).unwrap()).unwrap(),
                    s
                );
            }
        }
    }
}
