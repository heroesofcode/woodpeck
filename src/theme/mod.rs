//! Omarchy palette resolution and desktop-independent theme assets.
mod color;
mod discovery;
mod palette;
use crate::settings::BoardTheme;
pub use color::{Rgb, oklch_to_srgb};
pub use discovery::{ThemePaths, ThemePoller, discover, font_file, load, select};
pub use palette::{Mode, Palette};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardColors {
    pub dark: Rgb,
    pub light: Rgb,
    pub lip: Rgb,
}
impl BoardTheme {
    pub fn colors(self) -> BoardColors {
        let (d, l) = match self {
            Self::Peck => ((0.60, 0.10, 145.0), (0.96, 0.03, 145.0)),
            Self::Tournament => ((0.54, 0.06, 70.0), (0.95, 0.03, 85.0)),
            Self::Slate => ((0.50, 0.02, 255.0), (0.93, 0.01, 255.0)),
            Self::Blueprint => ((0.52, 0.08, 245.0), (0.95, 0.02, 230.0)),
            Self::Rosewood => ((0.47, 0.10, 20.0), (0.94, 0.025, 35.0)),
            Self::Lagoon => ((0.52, 0.08, 190.0), (0.95, 0.025, 190.0)),
            Self::Plum => ((0.45, 0.08, 315.0), (0.94, 0.025, 320.0)),
            Self::Carbon => ((0.32, 0.02, 250.0), (0.86, 0.01, 250.0)),
        };
        let dark = oklch_to_srgb(d.0, d.1, d.2);
        let light = oklch_to_srgb(l.0, l.1, l.2);
        BoardColors {
            dark,
            light,
            lip: dark.mix(Rgb::BLACK, 0.24),
        }
    }
}
/// Values consumed by the egui adapter, with safe defaults for incomplete themes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticColors {
    pub background: Rgb,
    pub panel: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    pub selection: Rgb,
    pub correct: Rgb,
    pub incorrect: Rgb,
    pub warning: Rgb,
}
impl Palette {
    pub fn semantic(&self) -> SemanticColors {
        let fallback = Self::builtin(self.mode);
        let get = |key: &str, default: Rgb| {
            self.color(key)
                .or_else(|| fallback.color(key))
                .unwrap_or(default)
        };
        SemanticColors {
            background: get("background", Rgb::BLACK),
            panel: get("lighter_background", Rgb::BLACK),
            text: get("foreground", Rgb::WHITE),
            muted: get("muted", Rgb(128, 128, 128)),
            accent: get("accent", Rgb(107, 152, 91)),
            selection: get("selection", Rgb(80, 80, 80)),
            correct: get("green", Rgb(114, 166, 92)),
            incorrect: get("red", Rgb(220, 98, 98)),
            warning: get("yellow", Rgb(214, 175, 85)),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn board_themes_and_lip() {
        for t in BoardTheme::ALL {
            let c = t.colors();
            assert_ne!(c.dark, c.light);
            assert_eq!(c.lip, c.dark.mix(Rgb::BLACK, 0.24));
            assert!(c.light.is_light());
        }
    }
    #[test]
    fn semantic_uses_palette_and_safe_fallback() {
        let p = Palette::parse("green='#123456'", false).unwrap();
        assert_eq!(p.semantic().correct, Rgb(18, 52, 86));
        assert_eq!(p.semantic().background, Rgb(27, 25, 22));
    }
}
