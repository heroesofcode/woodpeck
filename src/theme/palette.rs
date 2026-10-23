use super::Rgb;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub values: BTreeMap<String, String>,
    pub mode: Mode,
}
impl Palette {
    pub fn color(&self, key: &str) -> Option<Rgb> {
        self.values.get(key).and_then(|s| Rgb::parse(s))
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    /// Fallback is resolved as a key first, then treated as a literal, like the shell resolver.
    pub fn resolve<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        self.get(key)
            .or_else(|| self.get(fallback))
            .unwrap_or(fallback)
    }
    pub fn parse(input: &str, light_marker: bool) -> Result<Self, toml::de::Error> {
        let table = input.parse::<toml::Table>()?;
        let values = table
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_owned())))
            .collect();
        Ok(Self::from_values(values, light_marker))
    }
    /// Mirrors the supplied official resolver: canonical, legacy, ANSI, then derived colors.
    pub fn from_values(mut v: BTreeMap<String, String>, light_marker: bool) -> Self {
        const SHORT: [(&str, &str); 8] = [
            ("background", "bg"),
            ("dark_background", "dark_bg"),
            ("darker_background", "darker_bg"),
            ("lighter_background", "lighter_bg"),
            ("foreground", "fg"),
            ("dark_foreground", "dark_fg"),
            ("light_foreground", "light_fg"),
            ("bright_foreground", "bright_fg"),
        ];
        for (k, short) in SHORT {
            alias(&mut v, k, &[short]);
        }
        alias(&mut v, "background", &["color0"]);
        alias(&mut v, "foreground", &["color7"]);
        overwrite(&mut v, "color0", "background");
        overwrite(&mut v, "color7", "foreground");
        for (k, ansi) in [
            ("red", "color1"),
            ("green", "color2"),
            ("yellow", "color3"),
            ("blue", "color4"),
            ("magenta", "color5"),
            ("cyan", "color6"),
            ("bright_red", "color9"),
            ("bright_green", "color10"),
            ("bright_yellow", "color11"),
            ("bright_blue", "color12"),
            ("bright_magenta", "color13"),
            ("bright_cyan", "color14"),
        ] {
            alias(&mut v, k, &[ansi]);
        }
        alias(&mut v, "magenta", &["purple"]);
        alias(&mut v, "bright_magenta", &["bright_purple"]);
        for (k, keys) in [
            ("light_foreground", vec!["color7", "foreground"]),
            ("bright_foreground", vec!["color15", "foreground"]),
            ("lighter_background", vec!["color0", "background"]),
            ("dark_foreground", vec!["color8", "foreground"]),
            ("muted", vec!["color8", "dark_foreground"]),
            (
                "selection",
                vec!["selection_background", "color8", "color0", "background"],
            ),
            ("selection_background", vec!["selection"]),
            ("selection_foreground", vec!["bright_foreground"]),
            ("orange", vec!["yellow"]),
        ] {
            alias(&mut v, k, &keys);
        }
        overwrite(&mut v, "cursor", "bright_foreground");
        derive(&mut v, "brown", "orange", Rgb::BLACK, 0.5);
        derive(&mut v, "dark_background", "background", Rgb::BLACK, 0.25);
        derive(&mut v, "darker_background", "background", Rgb::BLACK, 0.5);
        for base in ["red", "yellow", "green", "cyan", "blue", "magenta"] {
            derive(&mut v, &format!("bright_{base}"), base, Rgb::WHITE, 0.2);
        }
        alias(&mut v, "purple", &["magenta"]);
        alias(&mut v, "bright_purple", &["bright_magenta"]);
        for (i, key) in [
            "background",
            "red",
            "green",
            "yellow",
            "blue",
            "magenta",
            "cyan",
            "foreground",
            "muted",
            "bright_red",
            "bright_green",
            "bright_yellow",
            "bright_blue",
            "bright_magenta",
            "bright_cyan",
            "bright_foreground",
        ]
        .iter()
        .enumerate()
        {
            alias(&mut v, &format!("color{i}"), &[key]);
        }
        for (k, short) in SHORT {
            overwrite(&mut v, short, k);
        }
        let mode = detect_mode(&v, light_marker);
        let name = if mode == Mode::Light { "light" } else { "dark" };
        v.insert("mode".into(), name.into());
        v.insert("theme_type".into(), name.into());
        Self { values: v, mode }
    }
    pub fn alacritty(input: &str, light_marker: bool) -> Result<Self, toml::de::Error> {
        let t = input.parse::<toml::Table>()?;
        let mut v: BTreeMap<String, String> = BTreeMap::new();
        if let Some(colors) = t.get("colors").and_then(toml::Value::as_table) {
            if let Some(primary) = colors.get("primary").and_then(toml::Value::as_table) {
                for key in ["background", "foreground"] {
                    if let Some(s) = primary.get(key).and_then(toml::Value::as_str) {
                        v.insert(key.into(), s.into());
                    }
                }
            }
            for (group, offset) in [("normal", 0), ("bright", 8)] {
                if let Some(colors) = colors.get(group).and_then(toml::Value::as_table) {
                    for (i, key) in [
                        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
                    ]
                    .iter()
                    .enumerate()
                    {
                        if let Some(s) = colors.get(*key).and_then(toml::Value::as_str) {
                            v.insert(format!("color{}", offset + i), s.into());
                        }
                    }
                }
            }
            if let Some(selection) = colors.get("selection").and_then(toml::Value::as_table) {
                for key in ["background", "foreground"] {
                    if let Some(s) = selection.get(key).and_then(toml::Value::as_str) {
                        v.insert(format!("selection_{key}"), s.into());
                    }
                }
            }
        }
        for val in v.values_mut() {
            if let Some(hex) = val.strip_prefix("0x") {
                *val = format!("#{hex}");
            }
        }
        Ok(Self::from_values(v, light_marker))
    }
    pub fn builtin(mode: Mode) -> Self {
        let (bg, fg) = if mode == Mode::Light {
            ("#faf9f3", "#24251e")
        } else {
            ("#1b1916", "#eeeee4")
        };
        let mut v: BTreeMap<String, String> = BTreeMap::new();
        for (k, val) in [
            ("background", bg),
            ("foreground", fg),
            ("accent", "#6b985b"),
            ("green", "#72a65c"),
            ("red", "#dc6262"),
            ("yellow", "#d6af55"),
            ("blue", "#699dcb"),
            ("magenta", "#b18bbf"),
            ("cyan", "#71b5ac"),
            ("mode", if mode == Mode::Light { "light" } else { "dark" }),
        ] {
            v.insert(k.into(), val.into());
        }
        Self::from_values(v, false)
    }
}
fn detect_mode(v: &BTreeMap<String, String>, light_marker: bool) -> Mode {
    match value(v, "mode").or_else(|| value(v, "theme_type")) {
        Some("light") => Mode::Light,
        Some(_) => Mode::Dark,
        None if light_marker => Mode::Light,
        None => {
            if value(v, "background")
                .and_then(Rgb::parse)
                .is_some_and(Rgb::is_light)
            {
                Mode::Light
            } else {
                Mode::Dark
            }
        }
    }
}
fn value<'a>(v: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    v.get(key).filter(|s| !s.is_empty()).map(String::as_str)
}
fn alias(v: &mut BTreeMap<String, String>, key: &str, keys: &[&str]) {
    if value(v, key).is_none()
        && let Some(s) = keys.iter().find_map(|k| value(v, k)).map(str::to_owned)
    {
        v.insert(key.into(), s);
    }
}
fn overwrite(v: &mut BTreeMap<String, String>, key: &str, from: &str) {
    if let Some(s) = value(v, from).map(str::to_owned) {
        v.insert(key.into(), s);
    }
}
fn derive(v: &mut BTreeMap<String, String>, key: &str, base: &str, to: Rgb, mix: f64) {
    if value(v, key).is_none()
        && let Some(c) = value(v, base).and_then(Rgb::parse)
    {
        v.insert(key.into(), c.mix(to, mix).hex());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_reference_palettes() {
        for (text, mode, bg) in [
            (
                include_str!("../../tests/fixtures/tokyo-night.toml"),
                Mode::Dark,
                "#1a1b26",
            ),
            (
                include_str!("../../tests/fixtures/flexoki-light.toml"),
                Mode::Light,
                "#FFFCF0",
            ),
        ] {
            let p = Palette::parse(text, false).unwrap();
            assert_eq!(p.mode, mode);
            assert_eq!(p.get("background"), Some(bg));
            assert_eq!(p.get("bg"), Some(bg));
            assert_eq!(p.get("color0"), Some(bg));
            assert_eq!(p.get("cursor"), p.get("bright_foreground"));
            for key in [
                "accent",
                "selection",
                "muted",
                "dark_background",
                "darker_background",
                "lighter_background",
                "foreground",
                "dark_foreground",
                "light_foreground",
                "bright_foreground",
                "red",
                "yellow",
                "orange",
                "green",
                "cyan",
                "blue",
                "magenta",
                "brown",
                "bright_red",
                "bright_yellow",
                "bright_green",
                "bright_cyan",
                "bright_blue",
                "bright_magenta",
            ] {
                assert!(p.color(key).is_some(), "{key}");
            }
        }
    }
    #[test]
    fn mode_precedence_and_luminance_boundary() {
        for (text, marker, mode) in [
            (
                "mode='dark'\ntheme_type='light'\nbackground='#ffffff'",
                true,
                Mode::Dark,
            ),
            (
                "theme_type='light'\nbackground='#000000'",
                false,
                Mode::Light,
            ),
            ("background='#000000'", true, Mode::Light),
            ("background='#7f7f7f'", false, Mode::Dark),
            ("background='#808080'", false, Mode::Light),
            ("", false, Mode::Dark),
        ] {
            assert_eq!(Palette::parse(text, marker).unwrap().mode, mode);
        }
    }
    #[test]
    fn precedence_legacy_ansi_and_derived_shades() {
        let p=Palette::parse("background='#102030'\nbg='#ffffff'\nfg='#c0c0c0'\ncolor0='#ff0000'\ncolor8='#505050'\ncolor15='#ffffff'\ncolor1='#800000'\ncolor3='#808000'\npurple='#800080'",false).unwrap();
        assert_eq!(p.get("bg"), Some("#102030"));
        assert_eq!(p.get("color0"), Some("#102030"));
        assert_eq!(p.get("foreground"), Some("#c0c0c0"));
        assert_eq!(p.get("selection"), Some("#505050"));
        assert_eq!(p.get("dark_foreground"), Some("#505050"));
        assert_eq!(p.get("light_foreground"), Some("#c0c0c0"));
        assert_eq!(p.get("cursor"), Some("#ffffff"));
        assert_eq!(p.get("dark_background"), Some("#0c1824"));
        assert_eq!(p.get("darker_background"), Some("#081018"));
        assert_eq!(p.get("bright_red"), Some("#993333"));
        assert_eq!(p.get("brown"), Some("#404000"));
        assert_eq!(p.get("magenta"), Some("#800080"));
        assert_eq!(p.get("color13"), p.get("bright_purple"));
        assert_eq!(p.resolve("absent", "red"), "#800000");
        assert_eq!(p.resolve("absent", "#abcdef"), "#abcdef");
    }
    #[test]
    fn all_short_names_override_ansi_and_alias_back() {
        let p=Palette::parse("bg='#010101'\ndark_bg='#020202'\ndarker_bg='#030303'\nlighter_bg='#040404'\nfg='#050505'\ndark_fg='#060606'\nlight_fg='#070707'\nbright_fg='#080808'\ncolor8='#090909'",false).unwrap();
        for (key, value) in [
            ("background", "#010101"),
            ("dark_background", "#020202"),
            ("darker_background", "#030303"),
            ("lighter_background", "#040404"),
            ("foreground", "#050505"),
            ("dark_foreground", "#060606"),
            ("light_foreground", "#070707"),
            ("bright_foreground", "#080808"),
        ] {
            assert_eq!(p.get(key), Some(value));
        }
        assert_eq!(p.get("muted"), Some("#090909"));
    }
    #[test]
    fn ansi_only_and_selection_aliases() {
        let p=Palette::parse("color0='#101010'\ncolor7='#eeeeee'\ncolor8='#888888'\ncolor5='#990099'\ncolor13='#ff00ff'\nselection_background='#123456'",false).unwrap();
        assert_eq!(p.get("background"), Some("#101010"));
        assert_eq!(p.get("magenta"), Some("#990099"));
        assert_eq!(p.get("bright_magenta"), Some("#ff00ff"));
        assert_eq!(p.get("selection"), Some("#123456"));
        assert_eq!(p.get("selection_foreground"), Some("#eeeeee"));
    }
    #[cfg(unix)]
    #[test]
    fn agrees_with_official_shell_cascade_for_every_resolved_key() {
        let input = "bg='#102030'\nfg='#c0d0e0'\ncolor8='#667788'\ncolor15='#ffffff'\ncolor1='#aabbcc'\ncolor2='#112233'\ncolor3='#445566'\ncolor4='#778899'\ncolor5='#99aabb'\ncolor6='#bbccdd'\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("colors.toml");
        std::fs::write(&path, input).unwrap();
        let output = std::process::Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/omarchy-theme-color.sh"
            ))
            .arg("--file")
            .arg(&path)
            .arg("--all")
            .output()
            .unwrap();
        assert!(output.status.success());
        let p = Palette::parse(input, false).unwrap();
        for line in String::from_utf8(output.stdout).unwrap().lines() {
            let (k, v) = line.split_once('\t').unwrap();
            assert_eq!(p.get(k), Some(v), "{k}");
        }
    }
}
