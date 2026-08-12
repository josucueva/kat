//! KAT - semantic software repository.
//!
//! Thin command-line binary over the `kat` library crate. This implements the
//! invocation contract in `docs/cli.md`; the CLI layer only parses and
//! dispatches, it never owns repository semantics.

use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

use kat::domain::identity::{ElementId, ObjectId};
use kat::domain::operation::Operation;
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
use kat::repository::query::{HistoryEntry, QueryError, history, show_element};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("init") => cmd_init(),
        Some("show") => cmd_show(&args[1..]),
        Some("history") => cmd_history(&args[1..]),
        Some(other) => {
            eprintln!("kat: unknown command '{other}'");
            eprintln!("usage: kat init | kat show <element-id> | kat history");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("kat: missing command");
            eprintln!("usage: kat init | kat show <element-id> | kat history");
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

/// `kat history` — display the accepted Change history, newest first
/// (read-only; thin dispatch over [`history`]).
fn cmd_history(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("kat history: expected no arguments");
        eprintln!("usage: kat history");
        return ExitCode::FAILURE;
    }
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat history: {error}");
            return ExitCode::FAILURE;
        }
    };
    match history(&repository) {
        Ok(entries) => {
            for (i, entry) in entries.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                print_history_entry(entry);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("kat history: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Prints one history entry in the stable diagnostic form:
///
/// ```text
/// revision_id: <C1>
/// change_id: <ChangeId>
/// result_state: <S1>
/// base_states:
///   <S0>
/// dependencies:
///   none
/// operations:
///   create_element <V1>
/// description: none
/// ```
fn print_history_entry(entry: &HistoryEntry) {
    println!("revision_id: {}", entry.revision_id);
    println!("change_id: {}", entry.change.change_id);
    println!("result_state: {}", entry.change.result_state);
    println!("base_states:");
    print_ids_or_none(&entry.change.base_states);
    println!("dependencies:");
    print_ids_or_none(&entry.change.dependencies);
    println!("operations:");
    for operation in &entry.change.operations {
        println!("  {}", format_operation(operation));
    }
    println!(
        "description: {}",
        entry.change.description.as_deref().unwrap_or("none")
    );
}

fn print_ids_or_none(ids: &[ObjectId]) {
    if ids.is_empty() {
        println!("  none");
    } else {
        for id in ids {
            println!("  {id}");
        }
    }
}

/// Boring deterministic rendering of one operation: `<name> <arg> ...` with
/// the canonical ObjectIds exactly as stored (no enrichment — e.g.
/// `CreateElement` displays V1, not the element it belongs to).
fn format_operation(operation: &Operation) -> String {
    match operation {
        Operation::CreateElement { new_version } => format!("create_element {new_version}"),
        Operation::UpdateElement {
            element_id,
            expected_version,
            new_version,
        } => format!("update_element {element_id} {expected_version} {new_version}"),
        Operation::DeprecateElement {
            element_id,
            expected_version,
            new_version,
        } => format!("deprecate_element {element_id} {expected_version} {new_version}"),
        Operation::Link {
            new_relationship_version,
        } => format!("link {new_relationship_version}"),
        Operation::Unlink {
            relationship_id,
            expected_version,
        } => format!("unlink {relationship_id} {expected_version}"),
        Operation::Supersede {
            existing_element,
            expected_existing_version,
            replacement_element,
            replacement_version,
            superseding_relationship,
        } => format!(
            "supersede {existing_element} {expected_existing_version} {replacement_element} {replacement_version} {superseding_relationship}"
        ),
    }
}

#[cfg(test)]
mod tests {
    /// Proves the binary target's test harness runs.
    #[test]
    fn harness_works() {}
}
