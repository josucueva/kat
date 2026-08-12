//! KAT - semantic software repository.
//!
//! Thin command-line binary over the `kat` library crate. This implements the
//! invocation contract in `docs/cli.md`; the CLI layer only parses and
//! dispatches, it never owns repository semantics.

use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

use kat::domain::identity::ElementId;
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
use kat::repository::query::{QueryError, show_element};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("init") => cmd_init(),
        Some("show") => cmd_show(&args[1..]),
        Some(other) => {
            eprintln!("kat: unknown command '{other}'");
            eprintln!("usage: kat init | kat show <element-id>");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("kat: missing command");
            eprintln!("usage: kat init | kat show <element-id>");
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

/// `kat show <element-id>` — display the currently accepted version of an
/// element (read-only; thin dispatch over [`show_element`]).
fn cmd_show(args: &[String]) -> ExitCode {
    let [element_id_arg] = args else {
        eprintln!("kat show: expected exactly one argument");
        eprintln!("usage: kat show <element-id>");
        return ExitCode::FAILURE;
    };
    let element_id = match ElementId::from_str(element_id_arg) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat show: invalid element ID: {element_id_arg}");
            return ExitCode::FAILURE;
        }
    };
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat show: {error}");
            return ExitCode::FAILURE;
        }
    };
    match show_element(&repository, element_id) {
        Ok(view) => {
            println!("element_id: {}", view.element_id);
            println!("version_id: {}", view.version_id);
            println!("type: {}", view.element.type_id);
            println!("lifecycle: {}", view.element.lifecycle);
            for (key, value) in &view.element.properties {
                println!("{key}: {value}");
            }
            ExitCode::SUCCESS
        }
        Err(QueryError::ElementNotFound(id)) => {
            eprintln!("kat show: element {id} not found in the accepted state");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("kat show: {error}");
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
