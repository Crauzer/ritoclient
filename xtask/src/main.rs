//! Workspace tooling. Run through the alias in `.cargo/config.toml`:
//!
//! ```text
//! cargo xtask ritoclient-snapshot   # needs a live Riot Client
//! cargo xtask ritoclient-codegen    # offline, reads schema/
//! ```

mod codegen;
mod snapshot;
mod surface;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let outcome = match args.first().map(String::as_str) {
        Some("ritoclient-snapshot") => snapshot::run(&args[1..]),
        Some("ritoclient-codegen") => codegen::run(&args[1..]),
        Some(other) => Err(format!("unknown task `{other}`\n\n{USAGE}")),
        None => Err(USAGE.to_string()),
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  ritoclient-snapshot [--client-build <version>] [--schema-dir <dir>]
      Take a schema snapshot from the live Riot Client into schema/.
      Wakes a tray-idle client (which opens its window) - the collapsed
      surface serves nothing worth snapshotting.

  ritoclient-codegen [--schema-dir <dir>]
      Regenerate crates/ritoclient-api/src from schema/. Offline.";
