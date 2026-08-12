//! KAT - semantic software repository.
//!
//! Thin command-line binary over the `kat` library crate. This implements the
//! invocation contract in `docs/cli.md`; the CLI layer only parses and
//! dispatches, it never owns repository semantics.

use std::env;
use std::path::Path;
use std::process::ExitCode;

use kat::repository::init::init_repository;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("init") => cmd_init(),
        Some(other) => {
            eprintln!("kat: unknown command '{other}'");
            eprintln!("usage: kat init");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("kat: missing command");
            eprintln!("usage: kat init");
            ExitCode::FAILURE
        }
    }
}

/// `kat init` — initialize a KAT repository in the current directory.
fn cmd_init() -> ExitCode {
    match init_repository(Path::new(".")) {
        Ok(result) => {
            println!("initialized KAT repository");
            println!("  repository_id: {}", result.repository_id);
            println!("  software_id:   {}", result.software_id);
            println!("  ontology (O1): {}", result.ontology);
            println!("  state (S0):    {}", result.state);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("kat init: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    /// Proves the binary target's test harness runs.
    #[test]
    fn harness_works() {}
}
