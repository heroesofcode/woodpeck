//! Tiny argument parser; help/version never open a database or start a download.
use crate::source::{LICHESS_DATABASE_URL, LibrarySize};
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Start,
    Sync(SyncCommand),
    Screenshot {
        path: std::path::PathBuf,
        screen: crate::app::Screen,
        size: [f32; 2],
    },
    Import {
        location: String,
        size: LibrarySize,
        limit: Option<u64>,
    },
    Help,
    Version,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncCommand {
    Init(Option<String>),
    Now,
    Status,
    Disable,
}
pub const HELP: &str = "woodpeck — local Woodpecker Method puzzle training\n\nUsage: woodpeck [--help | --version]\n       woodpeck import <path-or-url>\n       woodpeck import --lichess [--size medium|full] [--limit N]\n       woodpeck import <path-or-url> [--size medium|full] [--limit N]\n       woodpeck sync init <remote-url> | --create\n       woodpeck sync now | status | disable\n\nOpt-in progress sync requires a PRIVATE repository and system git.\nImports plain CSV or zstd CSV into the local database.\n--lichess streams the Lichess puzzle database; default size is full.\n--limit caps accepted puzzles for any source (overrides --size limit).\nScreenshot: --screenshot <out.png> [--screen dashboard|training|training-solved|review|progress|builder|settings|welcome|welcome-progress|welcome-done] [--size WxH]\nData override: $WOODPECK_DATA_DIR\nData: $XDG_DATA_HOME/woodpeck/woodpeck.db (Linux).\nPuzzles from the Lichess puzzle database (CC0).";
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let args: Vec<_> = args.into_iter().collect();
    if args.first().is_some_and(|s| s == "--screenshot") {
        return screenshot(&args);
    }
    if args.first().is_some_and(|s| s == "import")
        && args.get(1).is_none_or(|s| s != "--help" && s != "-h")
    {
        return import(&args[1..]);
    }
    match args.as_slice() {
        [] => Ok(Command::Start),
        [flag] if flag == "--help" || flag == "-h" => Ok(Command::Help),
        [flag] if flag == "--version" || flag == "-V" => Ok(Command::Version),
        [command, flag] if command == "import" && (flag == "--help" || flag == "-h") => {
            Ok(Command::Help)
        }
        [command, flag] if command == "sync" && (flag == "--help" || flag == "-h") => Ok(Command::Help),
        [command, action, remote] if command == "sync" && action == "init" => {
            if remote == "--create" { Ok(Command::Sync(SyncCommand::Init(None))) }
            else if remote.starts_with('-') || remote.is_empty() { Err("Expected a remote URL or --create.".into()) }
            else { Ok(Command::Sync(SyncCommand::Init(Some(remote.clone())))) }
        }
        [command, action] if command == "sync" => match action.as_str() {
            "now" => Ok(Command::Sync(SyncCommand::Now)),
            "status" => Ok(Command::Sync(SyncCommand::Status)),
            "disable" => Ok(Command::Sync(SyncCommand::Disable)),
            _ => Err("Expected sync init <remote-url>, sync init --create, sync now, sync status or sync disable.".into()),
        },
        _ => Err("Expected import, sync, --help or --version.".into()),
    }
}
fn import(args: &[String]) -> Result<Command, String> {
    let mut location = None;
    let mut size = LibrarySize::Full;
    let mut explicit_size = false;
    let mut limit = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--lichess" if location.is_none() => location = Some(LICHESS_DATABASE_URL.into()),
            "--size" if !explicit_size => {
                i += 1;
                size = args
                    .get(i)
                    .and_then(|s| LibrarySize::parse(s))
                    .filter(|s| *s != LibrarySize::Starter)
                    .ok_or("Expected size medium or full")?;
                explicit_size = true;
            }
            "--limit" if limit.is_none() => {
                i += 1;
                limit = Some(
                    args.get(i)
                        .and_then(|s| s.parse::<u64>().ok())
                        .filter(|n| *n > 0)
                        .ok_or("Expected a positive accepted-puzzle limit")?,
                );
            }
            value if !value.starts_with('-') && location.is_none() => {
                location = Some(value.to_owned());
            }
            _ => return Err("Unknown or duplicate import argument".into()),
        }
        i += 1;
    }
    Ok(Command::Import {
        location: location.ok_or("Expected import location or --lichess")?,
        size,
        limit: limit.or(size.limit()),
    })
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
    fn library_options() {
        assert!(matches!(
            args(&["import", "--lichess", "--size", "medium"]),
            Ok(Command::Import {
                size: LibrarySize::Medium,
                limit: Some(200_000),
                ..
            })
        ));
        assert!(matches!(
            args(&["import", "p.csv", "--size", "medium", "--limit", "7"]),
            Ok(Command::Import { limit: Some(7), .. })
        ));
        assert!(matches!(
            args(&["import", "--limit", "10", "p.csv"]),
            Ok(Command::Import {
                limit: Some(10),
                ..
            })
        ));
        for bad in [
            vec!["import", "--lichess", "--size", "starter"],
            vec!["import", "p", "--limit", "0"],
            vec!["import", "p", "--limit", "-1"],
            vec!["import", "p", "--size"],
            vec!["import", "--limit", "3"],
            vec!["import", "p", "--limit", "4", "--limit", "5"],
        ] {
            assert!(args(&bad).is_err());
        }
        for screen in [
            "training-solved",
            "welcome",
            "welcome-progress",
            "welcome-done",
        ] {
            assert!(args(&["--screenshot", "demo.png", "--screen", screen]).is_ok());
        }
    }
    #[test]
    fn sync_commands_and_errors() {
        assert_eq!(
            args(&[
                "sync",
                "init",
                "https://github.com/<user>/woodpeck-data.git"
            ]),
            Ok(Command::Sync(SyncCommand::Init(Some(
                "https://github.com/<user>/woodpeck-data.git".into()
            ))))
        );
        assert_eq!(
            args(&["sync", "init", "--create"]),
            Ok(Command::Sync(SyncCommand::Init(None)))
        );
        for (action, command) in [
            ("now", SyncCommand::Now),
            ("status", SyncCommand::Status),
            ("disable", SyncCommand::Disable),
        ] {
            assert_eq!(args(&["sync", action]), Ok(Command::Sync(command)));
        }
        assert_eq!(args(&["sync", "--help"]), Ok(Command::Help));
        for bad in [
            vec!["sync"],
            vec!["sync", "init"],
            vec!["sync", "init", "--unknown"],
            vec!["sync", "now", "extra"],
            vec!["sync", "unknown"],
            vec!["sync", "init", "a", "b"],
        ] {
            assert!(args(&bad).is_err());
        }
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
            Ok(Command::Import {
                location: LICHESS_DATABASE_URL.into(),
                size: LibrarySize::Full,
                limit: None
            })
        );
        for source in ["data.csv.zst", "https://example.com/puzzles.csv.zst"] {
            assert_eq!(
                args(&["import", source]),
                Ok(Command::Import {
                    location: source.into(),
                    size: LibrarySize::Full,
                    limit: None
                })
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
