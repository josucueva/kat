//! KAT - semantic software repository.
//!
//! Thin command-line binary over the `kat` library crate. This implements the
//! invocation contract in `docs/cli.md`; the CLI layer only parses and
//! dispatches, it never owns repository semantics.

use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

use kat::domain::identity::{ChangeId, ElementId, ObjectId};
use kat::domain::ontology::OntologyVersion;
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
use kat::repository::change::{
    ChangeContext, ChangeError, CreateElementInput, PublishedChange, apply_create_element,
    persist_prepared_change, prepare_change, prepare_change_revision, publish_persisted_change,
    validate_create_element_invariants, validate_create_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::open::{Repository, open_repository};
use kat::repository::query::{HistoryEntry, QueryError, history, show_element};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("init") => cmd_init(),
        Some("create") => cmd_create(&args[1..]),
        Some("show") => cmd_show(&args[1..]),
        Some("history") => cmd_history(&args[1..]),
        Some(other) => {
            eprintln!("kat: unknown command '{other}'");
            eprintln!(
                "usage: kat init | kat create <type> --title \"...\" [--description \"...\"] | kat show <element-id> | kat history"
            );
            ExitCode::FAILURE
        }
        None => {
            eprintln!("kat: missing command");
            eprintln!(
                "usage: kat init | kat create <type> --title \"...\" [--description \"...\"] | kat show <element-id> | kat history"
            );
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

/// Parsed `kat create` arguments.
struct CreateArgs {
    type_arg: String,
    title: String,
    description: Option<String>,
}

/// Parses exactly the `cli.md` sketch: `kat create <type> --title "..."
/// [--description "..."]`. No generic `--property` support yet.
fn parse_create_args(args: &[String]) -> Result<CreateArgs, String> {
    let (type_arg, rest) = args
        .split_first()
        .ok_or_else(|| "expected <type> --title \"...\"".to_string())?;
    let mut title: Option<String> = None;
    let mut description: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        let value = rest
            .get(i + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--title" => {
                if title.is_some() {
                    return Err("duplicate --title".to_string());
                }
                title = Some(value.clone());
            }
            "--description" => {
                if description.is_some() {
                    return Err("duplicate --description".to_string());
                }
                description = Some(value.clone());
            }
            other => return Err(format!("unknown option '{other}'")),
        }
        i += 2;
    }
    let title = title.ok_or_else(|| "--title is required".to_string())?;
    Ok(CreateArgs {
        type_arg: type_arg.clone(),
        title,
        description,
    })
}

/// Maps a CLI type argument to a canonical element type ID.
///
/// Fully-qualified IDs (containing `/`) pass through and are validated by the
/// engine against the repository ontology. Short names resolve to the unique
/// element type in the base ontology whose ID ends in `/short-name`
/// (e.g. `requirement` -> `kat.core/requirement`), so the authoritative base
/// ontology is the only source of type names — never a hardcoded CLI table.
fn resolve_element_type(ontology: &OntologyVersion, arg: &str) -> Result<String, String> {
    if arg.contains('/') {
        return Ok(arg.to_string());
    }
    let mut matches = ontology
        .element_types
        .iter()
        .filter(|definition| definition.type_id.rsplit('/').next() == Some(arg));
    match (matches.next(), matches.next()) {
        (Some(only), None) => Ok(only.type_id.clone()),
        (None, _) => Err(format!("unknown element type '{arg}'")),
        (Some(_), Some(_)) => Err(format!("ambiguous element type '{arg}'")),
    }
}

/// `kat create <type> --title "..." [--description "..."]` — run a
/// `CreateElement` change end to end through the Change Engine and publish it
/// (thin dispatch; all semantics live in the library).
///
/// Identity (ElementId, ChangeId) is generated here; the engine stays
/// deterministic. The resulting stable identifiers are printed for the
/// caller (and for `kat show`/`kat history`).
fn cmd_create(args: &[String]) -> ExitCode {
    let parsed = match parse_create_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("kat create: {message}");
            eprintln!("usage: kat create <type> --title \"...\" [--description \"...\"]");
            return ExitCode::FAILURE;
        }
    };

    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat create: {error}");
            return ExitCode::FAILURE;
        }
    };

    // Prepare first so the base ontology is available to resolve the type
    // argument against (short name -> canonical ID).
    let context = match prepare_change(&repository) {
        Ok(context) => context,
        Err(error) => return fail_create(error),
    };
    let type_id = match resolve_element_type(&context.ontology, &parsed.type_arg) {
        Ok(type_id) => type_id,
        Err(message) => {
            eprintln!("kat create: {message}");
            return ExitCode::FAILURE;
        }
    };

    let published = match create_pipeline(&repository, context, type_id, &parsed) {
        Ok(published) => published,
        Err(error) => return fail_create(error),
    };

    let prepared = &published.persisted.prepared;
    println!("element_id: {}", prepared.creation.element.element_id);
    println!("version_id: {}", prepared.creation.element_version_id);
    println!("state_id: {}", prepared.state_id);
    println!("change_id: {}", prepared.change.change_id);
    println!("change_revision_id: {}", prepared.change_revision_id);
    ExitCode::SUCCESS
}

/// Prints a change-engine failure and returns the failure exit code. A CAS
/// conflict is reported explicitly and never retried automatically.
fn fail_create(error: ChangeError) -> ExitCode {
    match error {
        ChangeError::Conflict => {
            eprintln!(
                "kat create: the accepted repository state changed while creating; nothing was published. Re-run kat create."
            );
        }
        other => eprintln!("kat create: {other}"),
    }
    ExitCode::FAILURE
}

/// Runs a prepared `CreateElement` through the full engine pipeline and
/// publishes it: apply -> ontology -> invariants -> revision -> persist ->
/// publish. `--title`/`--description` become text element properties; the
/// engine owns canonical normalization.
fn create_pipeline(
    repository: &Repository,
    context: ChangeContext,
    type_id: String,
    parsed: &CreateArgs,
) -> Result<PublishedChange, ChangeError> {
    let mut properties = vec![(
        "title".to_string(),
        PropertyValue::Text(parsed.title.clone()),
    )];
    if let Some(description) = &parsed.description {
        properties.push((
            "description".to_string(),
            PropertyValue::Text(description.clone()),
        ));
    }
    let input = CreateElementInput {
        element_id: ElementId::new(),
        type_id,
        properties,
    };
    let prepared = apply_create_element(context, input)?;
    let validated = validate_create_element_ontology(prepared)?;
    let validated = validate_create_element_invariants(validated)?;
    let revision = prepare_change_revision(validated, ChangeId::new(), None)?;
    let persisted = persist_prepared_change(repository, revision)?;
    publish_persisted_change(repository, persisted)
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
