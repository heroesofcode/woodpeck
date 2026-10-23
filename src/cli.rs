//! Tiny argument parser; help/version never open a database or start a download.
use crate::source::LICHESS_DATABASE_URL;
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Start,
    Screenshot {
        path: std::path::PathBuf,
        screen: crate::app::Screen,
        size: [f32; 2],
    },
    Import(String),
    Help,
    Version,
}
pub const HELP: &str = "woodpeck — local Woodpecker Method puzzle training\n\nUsage: woodpeck [--help | --version]\n       woodpeck import <path-or-url>\n       woodpeck import --lichess\n\nImports plain CSV or zstd CSV into the local database.\n--lichess streams the full Lichess puzzle database.\nScreenshot: --screenshot <out.png> [--screen dashboard|training|review|progress|builder|settings] [--size WxH]\nData override: $WOODPECK_DATA_DIR\nData: $XDG_DATA_HOME/woodpeck/woodpeck.db (Linux).\nPuzzles from the Lichess puzzle database (CC0).";
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let args: Vec<_> = args.into_iter().collect();
    if args.first().is_some_and(|s| s == "--screenshot") {
        return screenshot(&args);
    }
    match args.as_slice() {
        [] => Ok(Command::Start),
        [flag] if flag == "--help" || flag == "-h" => Ok(Command::Help),
        [flag] if flag == "--version" || flag == "-V" => Ok(Command::Version),
        [command, flag] if command == "import" && (flag == "--help" || flag == "-h") => {
            Ok(Command::Help)
        }
        [command, location] if command == "import" => {
            if location == "--lichess" {
                Ok(Command::Import(LICHESS_DATABASE_URL.into()))
            } else if location.starts_with('-') {
                Err(format!("Unknown import option: {location}"))
            } else {
                Ok(Command::Import(location.clone()))
            }
        }
        _ => Err("Expected import <path-or-url>, import --lichess, --help or --version.".into()),
    }
}
fn screenshot(args: &[String]) -> Result<Command, String> {
    let path = args
        .get(1)
        .filter(|s| !s.starts_with('-'))
        .ok_or("Expected screenshot output path")?
        .into();
    let mut screen = crate::app::Screen::Dashboard;
    let mut size: [f32; 2] = [1280.0, 820.0];
    let (pairs, remainder) = args[2..].as_chunks::<2>();
    for pair in pairs {
        match pair[0].as_str() {
            "--screen" => {
                screen = crate::app::Screen::parse(&pair[1]).ok_or("Unknown screenshot screen")?;
            }
            "--size" => {
                let (w, h) = pair[1].split_once('x').ok_or("Expected WxH")?;
                size = [
                    w.parse().map_err(|_| "Invalid width")?,
                    h.parse().map_err(|_| "Invalid height")?,
                ];
                if !size[0].is_finite()
                    || !size[1].is_finite()
                    || size[0] < 340.0
                    || size[1] < 480.0
                {
                    return Err("Minimum screenshot size is 340x480".into());
                }
            }
            _ => return Err("Unknown screenshot option".into()),
        }
    }
    if !remainder.is_empty() {
        return Err("Screenshot options require values".into());
    }
    Ok(Command::Screenshot { path, screen, size })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|s| (*s).into()))
    }
    #[test]
    fn screenshot_arguments_are_bounded_and_complete() {
        assert!(matches!(
            args(&[
                "--screenshot",
                "/tmp/a.png",
                "--screen",
                "review",
                "--size",
                "360x640"
            ]),
            Ok(Command::Screenshot {
                screen: crate::app::Screen::Review,
                size: [360.0, 640.0],
                ..
            })
        ));
        for bad in [
            vec!["--screenshot"],
            vec!["--screenshot", "x", "--screen"],
            vec!["--screenshot", "x", "--screen", "unknown"],
            vec!["--screenshot", "x", "--size", "NaNx800"],
            vec!["--screenshot", "x", "--size", "100x100"],
        ] {
            assert!(args(&bad).is_err());
        }
    }
    #[test]
    fn tiny_cli_parses_all_entry_points_and_rejects_ambiguous_arguments() {
        assert_eq!(args(&[]), Ok(Command::Start));
        assert_eq!(args(&["--help"]), Ok(Command::Help));
        assert_eq!(args(&["--version"]), Ok(Command::Version));
        assert_eq!(args(&["import", "--help"]), Ok(Command::Help));
        assert_eq!(
            args(&["import", "--lichess"]),
            Ok(Command::Import(LICHESS_DATABASE_URL.into()))
        );
        for source in ["data.csv.zst", "https://example.com/puzzles.csv.zst"] {
            assert_eq!(
                args(&["import", source]),
                Ok(Command::Import(source.into()))
            );
        }
        for bad in [
            &["import"][..],
            &["import", "--unknown"],
            &["import", "a", "b"],
            &["--other"],
        ] {
            assert!(args(bad).is_err());
        }
    }
}
