//! CLI entry point; the native application window arrives in the UI milestone.
use std::process::ExitCode;
use woodpeck::{
    cli::{self, Command},
    source::{CancelFlag, import_location, import_starter},
    store::{Store, database_path},
};
fn main() -> ExitCode {
    let command = match cli::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}\n\n{}", cli::HELP);
            return ExitCode::from(2);
        }
    };
    if let Err(error) = run(command) {
        eprintln!("woodpeck: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Help => println!("{}", cli::HELP),
        Command::Version => println!("woodpeck {}", env!("CARGO_PKG_VERSION")),
        Command::Start | Command::Import(_) => {
            let path = database_path().ok_or("Could not find the application data directory.")?;
            let mut store = Store::open(&path)?;
            let cancel = CancelFlag::default();
            if let Command::Import(location) = command {
                let result = import_location(&mut store, &location, &cancel, |p| {
                    eprint!(
                        "\r{} bytes · {} rows · {} accepted · {} rejected",
                        p.bytes, p.scanned, p.accepted, p.rejected
                    );
                })?;
                eprintln!();
                println!(
                    "Imported {} puzzles from {} rows into {}.",
                    result.accepted,
                    result.scanned,
                    path.display()
                );
            } else {
                import_starter(&mut store, false, &cancel, |_| {})?;
                println!(
                    "woodpeck: {} local puzzles ready in {}. The desktop UI arrives in the next milestone.",
                    store.puzzle_count()?,
                    path.display()
                );
            }
        }
    }
    Ok(())
}
