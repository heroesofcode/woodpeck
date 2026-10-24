//! Native desktop and import/screenshot command entry point.
use std::process::ExitCode;
use woodpeck::{
    cli::{self, Command},
    source::{CancelFlag, ImportOptions, import_location_options},
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
        Command::Import {
            location,
            size,
            limit,
        } => {
            let path = database_path().ok_or("Could not find the application data directory.")?;
            let mut store = Store::open(&path)?;
            let cancel = CancelFlag::default();
            {
                let started = std::time::Instant::now();
                let result = import_location_options(
                    &mut store,
                    &location,
                    size,
                    ImportOptions { limit },
                    &cancel,
                    |p| {
                        eprint!(
                            "\r{} bytes · {} rows · {} accepted · {} rejected",
                            p.bytes, p.scanned, p.accepted, p.rejected
                        );
                    },
                )?;
                eprintln!();
                // Include the final WAL checkpoint and close in the summary and wall time.
                drop(store);
                println!(
                    "Imported {} puzzles from {} rows · {} bytes · {:.2}s · {} database bytes into {}.",
                    result.accepted,
                    result.scanned,
                    result.bytes,
                    started.elapsed().as_secs_f64(),
                    std::fs::metadata(&path)?.len(),
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
