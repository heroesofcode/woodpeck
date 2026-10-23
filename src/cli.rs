//! Tiny argument parser; help/version never open a database or start a download.
use crate::source::LICHESS_DATABASE_URL;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Start,
    Import(String),
    Help,
    Version,
}
pub const HELP: &str = "woodpeck — local Woodpecker Method puzzle training\n\nUsage: woodpeck [--help | --version]\n       woodpeck import <path-or-url>\n       woodpeck import --lichess\n\nImports plain CSV or zstd CSV into the local database.\n--lichess streams the full Lichess puzzle database.\nData: $XDG_DATA_HOME/woodpeck/woodpeck.db (Linux).\nPuzzles from the Lichess puzzle database (CC0).";
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let args: Vec<_> = args.into_iter().collect();
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
#[cfg(test)]
mod tests {
    use super::*;
    fn args(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|s| (*s).into()))
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
