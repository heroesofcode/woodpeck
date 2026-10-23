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
        Command::Sync(command) => run_sync(command)?,
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

fn run_sync(command: cli::SyncCommand) -> Result<(), Box<dyn std::error::Error>> {
    let path = database_path().ok_or("Could not find the application data directory.")?;
    let data = path.parent().ok_or("Invalid data directory.")?;
    let mut store = Store::open(&path)?;
    let config = match command {
        cli::SyncCommand::Init(remote) => {
            woodpeck::sync::init(&mut store, data, remote.as_deref())?
        }
        cli::SyncCommand::Now => {
            woodpeck::sync::synchronize(&mut store, data, std::time::Duration::from_secs(45))?
        }
        cli::SyncCommand::Status => woodpeck::sync::config(&store)?,
        cli::SyncCommand::Disable => {
            let config = woodpeck::sync::config(&store)?;
            woodpeck::sync::configure(&store, &config.remote, false)?
        }
    };
    println!("Sync: {}", config.status.label());
    println!("Enabled: {} · Pending: {}", config.enabled, config.pending);
    if let Some(at) = config
        .last_sync
        .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
    {
        println!("Last sync: {}", at.to_rfc3339());
    }
    if let Some(error) = config.last_error {
        println!("Last error: {error}");
    }
    Ok(())
}
