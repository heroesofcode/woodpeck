use super::{Mode, Palette};
use crate::settings::ColorTheme;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use thiserror::Error;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemePaths {
    pub current: PathBuf,
    pub theme: PathBuf,
    pub name: Option<String>,
}
/// HOME is supplied explicitly; tests and portable installs never mutate process environment.
pub fn discover(home: &Path) -> Option<ThemePaths> {
    for relative in [".local/state/omarchy/current", ".config/omarchy/current"] {
        let current = home.join(relative);
        let theme = current.join("theme");
        if theme.is_dir() {
            let name = fs::read_to_string(current.join("theme.name"))
                .ok()
                .map(|s| s.trim().to_owned());
            return Some(ThemePaths {
                current,
                theme,
                name,
            });
        }
    }
    None
}
#[derive(Debug, Error)]
pub enum ThemeError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
}
pub fn load(paths: &ThemePaths) -> Result<Option<Palette>, ThemeError> {
    let marker = paths.theme.join("light.mode").is_file();
    for (file, legacy) in [("colors.toml", false), ("alacritty.toml", true)] {
        match fs::read_to_string(paths.theme.join(file)) {
            Ok(s) => {
                return Ok(Some(if legacy {
                    Palette::alacritty(&s, marker)?
                } else {
                    Palette::parse(&s, marker)?
                }));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(None)
}
pub fn select(choice: ColorTheme, omarchy: Option<Palette>) -> Palette {
    match choice {
        ColorTheme::Light => Palette::builtin(Mode::Light),
        ColorTheme::Dark => Palette::builtin(Mode::Dark),
        ColorTheme::System | ColorTheme::Omarchy => {
            omarchy.unwrap_or_else(|| Palette::builtin(Mode::Dark))
        }
    }
}
/// Polls discovery again so parent/symlink swaps are picked up, not just writes in one directory.
/// Call off the UI thread; a missing or malformed theme uses the built-in fallback.
pub struct ThemePoller {
    home: PathBuf,
    choice: ColorTheme,
    last_poll: Option<Duration>,
    last: Option<Palette>,
}
impl ThemePoller {
    pub fn new(home: PathBuf, choice: ColorTheme) -> Self {
        Self {
            home,
            choice,
            last_poll: None,
            last: None,
        }
    }
    pub fn poll(&mut self, now: Duration) -> Option<Palette> {
        if self
            .last_poll
            .is_some_and(|then| now.saturating_sub(then) < Duration::from_secs(2))
        {
            return None;
        }
        self.last_poll = Some(now);
        let palette = select(
            self.choice,
            discover(&self.home).and_then(|p| load(&p).ok().flatten()),
        );
        if self.last.as_ref() == Some(&palette) {
            return None;
        }
        self.last = Some(palette.clone());
        Some(palette)
    }
}
/// Fontconfig is optional; absence, unsuccessful status, and invalid output all fall back to egui.
pub fn font_file(family: &str) -> Option<PathBuf> {
    let output = Command::new("fc-match")
        .args(["-f", "%{file}", family])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let file = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    file.is_file().then_some(file)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn injected_home_modern_legacy_and_name() {
        let home = tempfile::tempdir().unwrap();
        assert!(discover(home.path()).is_none());
        let old = home.path().join(".config/omarchy/current/theme");
        fs::create_dir_all(&old).unwrap();
        assert_eq!(discover(home.path()).unwrap().theme, old);
        let new = home.path().join(".local/state/omarchy/current/theme");
        fs::create_dir_all(&new).unwrap();
        fs::write(new.parent().unwrap().join("theme.name"), "Tokyo Night\n").unwrap();
        let paths = discover(home.path()).unwrap();
        assert_eq!(paths.theme, new);
        assert_eq!(paths.name.as_deref(), Some("Tokyo Night"));
    }
    #[test]
    fn alacritty_fallback_marker_and_colors_priority() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".config/omarchy/current/theme");
        fs::create_dir_all(&dir).unwrap();
        let paths = discover(home.path()).unwrap();
        assert_eq!(load(&paths).unwrap(), None);
        fs::write(dir.join("alacritty.toml"),"[colors.primary]\nbackground='0xffffff'\nforeground='#111111'\n[colors.normal]\nred='#ff0000'\n[colors.bright]\nred='#ff5555'").unwrap();
        let p = load(&paths).unwrap().unwrap();
        assert_eq!(p.mode, Mode::Light);
        assert_eq!(p.get("bright_red"), Some("#ff5555"));
        fs::write(dir.join("colors.toml"), "background='#000000'").unwrap();
        assert_eq!(load(&paths).unwrap().unwrap().mode, Mode::Dark);
        fs::write(dir.join("light.mode"), "").unwrap();
        assert_eq!(load(&paths).unwrap().unwrap().mode, Mode::Light);
    }
    #[test]
    fn poll_reloads_swapped_theme_and_falls_back() {
        let home = tempfile::tempdir().unwrap();
        let mut poll = ThemePoller::new(home.path().into(), ColorTheme::System);
        assert_eq!(poll.poll(Duration::ZERO).unwrap().mode, Mode::Dark);
        let dir = home.path().join(".local/state/omarchy/current/theme");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("colors.toml"), "background='#ffffff'").unwrap();
        assert!(poll.poll(Duration::from_secs(1)).is_none());
        assert_eq!(poll.poll(Duration::from_secs(2)).unwrap().mode, Mode::Light);
        assert!(poll.poll(Duration::from_secs(4)).is_none());
        fs::rename(&dir, dir.with_file_name("old")).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("colors.toml"), "background='#001122'").unwrap();
        assert_eq!(
            poll.poll(Duration::from_secs(6)).unwrap().get("background"),
            Some("#001122")
        );
        fs::write(dir.join("colors.toml"), "malformed [").unwrap();
        assert_eq!(
            poll.poll(Duration::from_secs(8)).unwrap().get("background"),
            Some("#1b1916")
        );
    }
    #[test]
    fn explicit_settings_override_desktop() {
        let p = Palette::builtin(Mode::Light);
        assert_eq!(select(ColorTheme::Dark, Some(p.clone())).mode, Mode::Dark);
        assert_eq!(select(ColorTheme::Omarchy, Some(p)).mode, Mode::Light);
        assert_eq!(select(ColorTheme::System, None).mode, Mode::Dark);
    }
}
