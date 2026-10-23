//! Native desktop and import/screenshot command entry point.
use std::process::ExitCode;
use woodpeck::{
    cli::{self, Command},
    source::{CancelFlag, import_location},
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
        Command::Start => woodpeck::app::run(None)?,
        Command::Screenshot { path, screen, size } => {
            woodpeck::app::run(Some(woodpeck::app::Screenshot { path, screen, size }))?;
        }
        Command::Import(location) => {
            let path = database_path().ok_or("Could not find the application data directory.")?;
            let mut store = Store::open(&path)?;
            let cancel = CancelFlag::default();
            {
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
            }
        }
    }
    Ok(())
}
