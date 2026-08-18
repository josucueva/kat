//! KAT - semantic software repository.
//!
//! Thin command-line binary over the `kat` library crate. This implements the
//! invocation contract in `docs/cli.md`; the CLI layer only parses and
//! dispatches, it never owns repository semantics.

use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

use kat::domain::element::Lifecycle;
use kat::domain::identity::{ChangeId, ElementId, ObjectId, RelationshipId};
use kat::domain::ontology::OntologyVersion;
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
use kat::encoding::decode_canonical;
use kat::encoding::object::CanonicalPayload;
use kat::repository::change::{
    ChangeContext, ChangeError, CreateElementInput, DeprecateElementInput, LinkElementInput,
    PublishedChange, PublishedDeprecateChange, PublishedSupersedeChange, PublishedUpdateChange,
    SupersedeElementInput, UnlinkElementInput, UpdateElementInput, apply_create_element,
    apply_deprecate_element, apply_link_element, apply_supersede_element, apply_unlink_element,
    apply_update_element, persist_prepared_change, persist_prepared_deprecate_change,
    persist_prepared_link_change, persist_prepared_supersede_change,
    persist_prepared_unlink_change, persist_prepared_update_change, prepare_change,
    prepare_change_revision, prepare_deprecate_change_revision, prepare_link_change_revision,
    prepare_supersede_change_revision, prepare_unlink_change_revision,
    prepare_update_change_revision, publish_persisted_change, publish_persisted_deprecate_change,
    publish_persisted_link_change, publish_persisted_supersede_change,
    publish_persisted_unlink_change, publish_persisted_update_change,
    validate_create_element_invariants, validate_create_element_ontology,
    validate_deprecate_element_invariants, validate_deprecate_element_ontology,
    validate_link_element_invariants, validate_link_element_ontology,
    validate_supersede_element_invariants, validate_supersede_element_ontology,
    validate_unlink_element_invariants, validate_update_element_invariants,
    validate_update_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::object_store::ObjectStore;
use kat::repository::open::{Repository, open_repository};
use kat::repository::query::{
    ArtifactAccountabilityReport, ArtifactAccountabilityStatus, ArtifactFilter, ElementView,
    HistoryEntry, ImpactResult, ListFilter, QueryError, RepositoryStatus, TraceResult,
    TraceTreeNode, TraversalDirection, analyze_artifact_accountability_filtered, analyze_impact,
    history, history_entry_touches_element, inspect_draft_session, list_elements,
    repository_status, show_element, trace_origin,
};
use kat::repository::resolve::{
    ResolveError, resolve_element_id, resolve_element_in_state, resolve_relationship_id,
    resolve_relationship_in_state,
};
use kat::repository::validation::repository::{ValidationReport, validate_repository};

use kat::repository::change::{
    AccountArtifactInput, StagedOperationInput, account_artifact, commit_draft_session,
    stage_operation_into_session,
};
use kat::repository::session::{
    abort_draft_session, begin_draft_session, has_draft_session, read_draft_session,
};

use clap::Parser;
use kat::cli::machine_presenter::MachinePresenter;
use kat::cli::{Cli, Command, OntologyCommands};

use kat::repository::query::{
    OntologySummary, OntologyTypeView, inspect_ontology, show_ontology_type,
};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => cmd_init(),
        Command::Status { compact, json } => run_status(compact, json),
        Command::Context {
            roots,
            direction,
            depth,
            categorize,
            compact,
            json,
        } => run_context(roots, direction, depth, categorize, compact, json),
        Command::Author {
            claims_file,
            example,
            json,
        } => run_author(claims_file, example, json),
        Command::Check { compact, json } => run_check(compact, json),
        Command::Commit { json } => run_commit(json),
        Command::Abort { json } => run_abort(json),
        Command::List {
            element_type,
            type_flag,
            lifecycle,
        } => run_list(element_type, type_flag, lifecycle),
        Command::Create {
            element_type,
            title,
            description,
        } => run_create(element_type, title, description),
        Command::Update {
            element_id,
            title,
            description,
        } => run_update(element_id, title, description),
        Command::Deprecate { element_id } => run_deprecate(element_id),
        Command::Supersede {
            existing_element_id,
            replacement_type,
            title,
            description,
        } => run_supersede(existing_element_id, replacement_type, title, description),
        Command::Link {
            relationship_type,
            source_element_id,
            target_element_id,
            description,
        } => run_link(
            relationship_type,
            source_element_id,
            target_element_id,
            description,
        ),
        Command::Unlink {
            relationship_id,
            description,
        } => run_unlink(relationship_id, description),
        Command::Account {
            artifact_id,
            description,
        } => run_account(artifact_id, description),
        Command::Show {
            element_id,
            compact,
        } => run_show(element_id, compact),
        Command::History {
            oneline,
            limit,
            element,
            compact,
        } => cmd_history(oneline, limit, element, compact),
        Command::Trace {
            element_id,
            paths,
            max_depth,
            compact,
        } => run_trace(element_id, paths, max_depth, compact),
        Command::Impact {
            element_id,
            max_depth,
            compact,
        } => run_impact(element_id, max_depth, compact),
        Command::Validate { coverage, compact } => cmd_validate(coverage, compact),
        Command::Artifacts {
            stale,
            artifact_id,
            compact,
        } => cmd_artifacts(stale, artifact_id, compact),
        Command::Change { command } => cmd_change(command),
        Command::Ontology { compact, command } => cmd_ontology(compact, command),
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

fn run_status(compact: bool, json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            if json {
                MachinePresenter::present_error(None, "NotInRepository", &error.to_string());
            } else {
                eprintln!("kat status: {error}");
            }
            return ExitCode::FAILURE;
        }
    };

    match repository_status(&repository) {
        Ok(status) => {
            if json {
                MachinePresenter::present_success(&repository, &status);
            } else if compact {
                print_repository_status_compact(&status);
            } else {
                print_repository_status(&status);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if json {
                MachinePresenter::present_error(
                    Some(&repository),
                    "StatusError",
                    &error.to_string(),
                );
            } else {
                eprintln!("kat status: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn print_repository_status_compact(status: &RepositoryStatus) {
    let stale = status.accountability.stale;
    let stale_suffix = if stale == 1 { "artifact" } else { "artifacts" };
    println!(
        "{} elements · {} relationships · {} violations · {} stale {}",
        status.knowledge.active_elements,
        status.knowledge.total_relationships,
        status.consistency.violations,
        stale,
        stale_suffix
    );
}

fn run_list(
    element_type_pos: Option<String>,
    type_flag: Option<String>,
    lifecycle_flag: Option<String>,
) -> ExitCode {
    let type_arg = match (element_type_pos, type_flag) {
        (Some(pos), Some(flag)) => {
            if pos != flag {
                eprintln!(
                    "kat list: conflicting type arguments: positional '{pos}' and --type '{flag}'"
                );
                return ExitCode::FAILURE;
            }
            Some(pos)
        }
        (Some(pos), None) => Some(pos),
        (None, Some(flag)) => Some(flag),
        (None, None) => None,
    };

    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat list: {error}");
            return ExitCode::FAILURE;
        }
    };

    let type_id = if let Some(ref arg) = type_arg {
        let context = match prepare_change(&repository) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("kat list: {error}");
                return ExitCode::FAILURE;
            }
        };
        match resolve_element_type(&context.ontology, arg) {
            Ok(resolved) => Some(resolved),
            Err(message) => {
                eprintln!("kat list: {message}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };

    let lifecycle = if let Some(ref flag) = lifecycle_flag {
        match flag.to_lowercase().as_str() {
            "active" => Some(Lifecycle::Active),
            "deprecated" => Some(Lifecycle::Deprecated),
            "superseded" => Some(Lifecycle::Superseded),
            _ => {
                eprintln!(
                    "kat list: invalid lifecycle state '{flag}' (expected: active, deprecated, superseded)"
                );
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };

    let filter = ListFilter { type_id, lifecycle };

    match list_elements(&repository, filter) {
        Ok(elements) => {
            print_element_list(&elements);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("kat list: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_element_list(elements: &[ElementView]) {
    if elements.is_empty() {
        println!("none");
        return;
    }

    println!("{:<10} {:<16} {:<11} TITLE", "ID", "TYPE", "STATE");
    for view in elements {
        let short_id = &view.element_id.to_string()[..8];
        let short_type = view
            .element
            .type_id
            .rsplit('/')
            .next()
            .unwrap_or(&view.element.type_id);
        let state = format_lifecycle(view.element.lifecycle);
        let title = view
            .element
            .properties
            .iter()
            .find(|(k, _)| k == "title")
            .and_then(|(_, v)| match v {
                PropertyValue::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .unwrap_or("none");

        println!("{short_id:<10} {short_type:<16} {state:<11} {title}");
    }
}

fn is_mutation_cmd(cmd: &str) -> bool {
    matches!(
        cmd,
        "update" | "deprecate" | "supersede" | "link" | "unlink" | "account"
    )
}

fn resolve_cli_element_id(
    repository: &Repository,
    input: &str,
    cmd: &str,
) -> Result<ElementId, ExitCode> {
    let res = if is_mutation_cmd(cmd) && has_draft_session(repository.root_dir()) {
        if let Ok(session) = read_draft_session(repository.root_dir()) {
            resolve_element_in_state(&session.working_state, input)
        } else {
            resolve_element_id(repository, input)
        }
    } else {
        resolve_element_id(repository, input)
    };

    match res {
        Ok(id) => Ok(id),
        Err(ResolveError::Ambiguous {
            input, candidates, ..
        }) => {
            eprintln!("kat {cmd}: error: element ID prefix '{input}' is ambiguous");
            eprintln!();
            eprintln!("matches:");
            for cand_id_str in &candidates {
                let show_info = ElementId::from_str(cand_id_str)
                    .ok()
                    .and_then(|id| show_element(repository, id).ok());
                if let Some(view) = show_info {
                    let short_type = view
                        .element
                        .type_id
                        .rsplit('/')
                        .next()
                        .unwrap_or(&view.element.type_id);
                    let title = view
                        .element
                        .properties
                        .iter()
                        .find(|(k, _)| k == "title")
                        .and_then(|(_, v)| match v {
                            PropertyValue::Text(t) => Some(t.as_str()),
                            _ => None,
                        })
                        .unwrap_or("none");
                    eprintln!("  {cand_id_str}  {short_type:<16}  {title}");
                } else {
                    eprintln!("  {cand_id_str}");
                }
            }
            Err(ExitCode::FAILURE)
        }
        Err(ResolveError::NotFound { input }) => {
            eprintln!("kat {cmd}: element {input} not found in the accepted state");
            Err(ExitCode::FAILURE)
        }
        Err(ResolveError::PrefixTooShort { input }) => {
            eprintln!(
                "kat {cmd}: identifier prefix '{input}' is too short (minimum 8 hex digits required)"
            );
            Err(ExitCode::FAILURE)
        }
        Err(ResolveError::InvalidIdentifier { input }) => {
            eprintln!("kat {cmd}: invalid element ID: {input}");
            Err(ExitCode::FAILURE)
        }
        Err(error) => {
            eprintln!("kat {cmd}: {error}");
            Err(ExitCode::FAILURE)
        }
    }
}

fn resolve_cli_relationship_id(
    repository: &Repository,
    input: &str,
    cmd: &str,
) -> Result<RelationshipId, ExitCode> {
    let res = if is_mutation_cmd(cmd) && has_draft_session(repository.root_dir()) {
        if let Ok(session) = read_draft_session(repository.root_dir()) {
            resolve_relationship_in_state(&session.working_state, input)
        } else {
            resolve_relationship_id(repository, input)
        }
    } else {
        resolve_relationship_id(repository, input)
    };

    match res {
        Ok(id) => Ok(id),
        Err(ResolveError::Ambiguous {
            input, candidates, ..
        }) => {
            eprintln!("kat {cmd}: error: relationship ID prefix '{input}' is ambiguous");
            eprintln!();
            eprintln!("matches:");
            for cand_id_str in candidates {
                eprintln!("  {cand_id_str}");
            }
            Err(ExitCode::FAILURE)
        }
        Err(ResolveError::NotFound { input }) => {
            eprintln!("kat {cmd}: relationship {input} not found in the accepted state");
            Err(ExitCode::FAILURE)
        }
        Err(ResolveError::PrefixTooShort { input }) => {
            eprintln!(
                "kat {cmd}: identifier prefix '{input}' is too short (minimum 8 hex digits required)"
            );
            Err(ExitCode::FAILURE)
        }
        Err(ResolveError::InvalidIdentifier { input }) => {
            eprintln!("kat {cmd}: invalid relationship ID: {input}");
            Err(ExitCode::FAILURE)
        }
        Err(error) => {
            eprintln!("kat {cmd}: {error}");
            Err(ExitCode::FAILURE)
        }
    }
}

// ---------------------------------------------------------------------------
// Centralized CLI Presentation Formatting Helpers
// (Ref: docs/cli-presentation.md)
// ---------------------------------------------------------------------------

/// Abbreviates a content-addressed `ObjectId` hex digest to 12 characters for display.
fn short_object_id(id: &ObjectId) -> String {
    let s = id.to_string();
    if s.len() >= 12 {
        s[..12].to_string()
    } else {
        s
    }
}

/// Formats an operation as a human-readable space-separated string.
fn format_operation_name(operation: &Operation) -> &'static str {
    match operation {
        Operation::CreateElement { .. } => "create element",
        Operation::UpdateElement { .. } => "update element",
        Operation::DeprecateElement { .. } => "deprecate element",
        Operation::Supersede { .. } => "supersede element",
        Operation::Link { .. } => "link",
        Operation::Unlink { .. } => "unlink",
        Operation::AccountArtifact { .. } => "account artifact",
    }
}

/// Formats element lifecycle in lowercase.
fn format_lifecycle(lifecycle: Lifecycle) -> &'static str {
    match lifecycle {
        Lifecycle::Active => "active",
        Lifecycle::Deprecated => "deprecated",
        Lifecycle::Superseded => "superseded",
    }
}

/// Formats artifact accountability status in lowercase.
fn format_accountability_status(status: ArtifactAccountabilityStatus) -> &'static str {
    match status {
        ArtifactAccountabilityStatus::Current => "current",
        ArtifactAccountabilityStatus::Stale => "stale",
        ArtifactAccountabilityStatus::Unaccounted => "unaccounted",
    }
}

fn print_repository_status(status: &RepositoryStatus) {
    println!("KAT repository");
    println!();
    println!("Repository");
    println!("  repository:  {}", status.repository_id);
    println!("  software:    {}", status.software_id);
    println!("  state:       {}", short_object_id(&status.state_id));

    if let Some(change_id) = status.change_id {
        println!("  change:      {}", short_object_id(&change_id));
    } else {
        println!("  change:      none");
    }
    println!("  ontology:    {}", short_object_id(&status.ontology_id));

    if let Some(ref latest) = status.latest_change {
        println!();
        println!("Latest change");
        println!("  revision:    {}", short_object_id(&latest.revision_id));
        println!("  operation:   {}", latest.operation_kind);
        println!(
            "  description: {}",
            latest.description.as_deref().unwrap_or("none")
        );
    }

    println!();
    println!("Knowledge");
    println!("  elements:       {}", status.knowledge.total_elements);
    println!("    active:        {}", status.knowledge.active_elements);
    println!(
        "    deprecated:    {}",
        status.knowledge.deprecated_elements
    );
    println!(
        "    superseded:    {}",
        status.knowledge.superseded_elements
    );
    println!("  relationships:  {}", status.knowledge.total_relationships);

    println!();
    println!("Consistency");
    println!(
        "  violations:             {}",
        status.consistency.violations
    );
    println!(
        "  unverified constraints: {}",
        status.consistency.unverified_constraints
    );

    println!();
    println!("Accountability");
    println!("  current:      {}", status.accountability.current);
    println!("  stale:        {}", status.accountability.stale);
    println!("  unaccounted:  {}", status.accountability.unaccounted);
}

/// Parsed `kat create` arguments.
struct CreateArgs {
    type_arg: String,
    title: String,
    description: Option<String>,
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

/// Resolves a CLI relationship-type argument: if it contains `/` it's returned
/// as-is (e.g. `kat.core/addresses`); otherwise it is looked up as a short name for a
/// relationship type in the base ontology whose ID ends in `/short-name`
/// (e.g. `addresses` -> `kat.core/addresses`), so the authoritative base
/// ontology is the only source of relationship type names — never a hardcoded CLI table.
fn resolve_relationship_type(ontology: &OntologyVersion, arg: &str) -> Result<String, String> {
    if arg.contains('/') {
        return Ok(arg.to_string());
    }
    let mut matches = ontology
        .relationship_types
        .iter()
        .filter(|definition| definition.type_id.rsplit('/').next() == Some(arg));
    match (matches.next(), matches.next()) {
        (Some(only), None) => Ok(only.type_id.clone()),
        (None, _) => Err(format!("unknown relationship type '{arg}'")),
        (Some(_), Some(_)) => Err(format!("ambiguous relationship type '{arg}'")),
    }
}

/// `kat create <type> --title "..." [--description "..."]` — run a
/// `CreateElement` change end to end through the Change Engine and publish it
/// (thin dispatch; all semantics live in the library).
///
/// Identity (ElementId, ChangeId) is generated here; the engine stays
/// deterministic. The resulting stable identifiers are printed for the
/// caller (and for `kat show`/`kat history`).
fn run_create(type_arg: String, title: String, description: Option<String>) -> ExitCode {
    let parsed = CreateArgs {
        type_arg,
        title,
        description,
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

    if has_draft_session(repository.root_dir()) {
        let mut session = match read_draft_session(repository.root_dir()) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("kat create: {err}");
                return ExitCode::FAILURE;
            }
        };

        let element_id = ElementId::new();
        let mut properties = Vec::new();
        properties.push(("title".to_string(), PropertyValue::Text(parsed.title)));
        if let Some(desc) = parsed.description {
            properties.push(("description".to_string(), PropertyValue::Text(desc)));
        }
        let input = CreateElementInput {
            element_id,
            type_id,
            properties,
        };

        let op = match stage_operation_into_session(
            &repository,
            &mut session,
            StagedOperationInput::CreateElement(input),
        ) {
            Ok(op) => op,
            Err(err) => return fail_create(err),
        };

        println!("staged create element");
        println!("  element_id:        {element_id}");
        if let Operation::CreateElement { new_version } = op {
            println!("  version_id:        {new_version}");
        }
        println!("  change operations: {}", session.operations.len());
        return ExitCode::SUCCESS;
    }

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

/// Parsed `kat update` arguments.
struct UpdateArgs {
    element_id: ElementId,
    title: Option<String>,
    description: Option<String>,
}

/// `kat update <element-id> [--title "..."] [--description "..."]` — run an
/// `UpdateElement` change end to end through the Change Engine and publish it
/// (thin dispatch; all semantics live in the library).
fn run_update(
    element_id_str: String,
    title: Option<String>,
    description: Option<String>,
) -> ExitCode {
    if title.is_none() && description.is_none() {
        eprintln!(
            "kat update: at least one property flag (--title, --description) must be supplied"
        );
        return ExitCode::FAILURE;
    }

    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat update: {error}");
            return ExitCode::FAILURE;
        }
    };

    let element_id = match resolve_cli_element_id(&repository, &element_id_str, "update") {
        Ok(id) => id,
        Err(code) => return code,
    };

    let parsed = UpdateArgs {
        element_id,
        title,
        description,
    };

    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat update: {error}");
            return ExitCode::FAILURE;
        }
    };

    if has_draft_session(repository.root_dir()) {
        let mut session = match read_draft_session(repository.root_dir()) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("kat update: {err}");
                return ExitCode::FAILURE;
            }
        };

        let previous_version_id = match session
            .working_state
            .elements
            .iter()
            .find(|e| e.element_id == parsed.element_id)
        {
            Some(entry) => entry.version,
            None => {
                eprintln!(
                    "kat update: element {} not found in base state",
                    parsed.element_id
                );
                return ExitCode::FAILURE;
            }
        };

        let mut properties = Vec::new();
        if let Some(title) = &parsed.title {
            properties.push(("title".to_string(), PropertyValue::Text(title.clone())));
        }
        if let Some(description) = &parsed.description {
            properties.push((
                "description".to_string(),
                PropertyValue::Text(description.clone()),
            ));
        }

        let input = UpdateElementInput {
            element_id: parsed.element_id,
            expected_version: previous_version_id,
            properties,
        };

        let op = match stage_operation_into_session(
            &repository,
            &mut session,
            StagedOperationInput::UpdateElement(input),
        ) {
            Ok(op) => op,
            Err(err) => return fail_update(err),
        };

        println!("staged update element");
        println!("  element_id:        {}", parsed.element_id);
        if let Operation::UpdateElement { new_version, .. } = op {
            println!("  version_id:        {new_version}");
        }
        println!("  change operations: {}", session.operations.len());
        return ExitCode::SUCCESS;
    }

    let context = match prepare_change(&repository) {
        Ok(context) => context,
        Err(error) => return fail_update(error),
    };

    let previous_version_id = match context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == parsed.element_id)
    {
        Some(entry) => entry.version,
        None => {
            eprintln!(
                "kat update: element {} not found in the base state",
                parsed.element_id
            );
            return ExitCode::FAILURE;
        }
    };

    let published = match update_pipeline(&repository, context, previous_version_id, &parsed) {
        Ok(published) => published,
        Err(error) => return fail_update(error),
    };

    let prepared = &published.persisted.prepared;
    println!("element_id: {}", prepared.update.element.element_id);
    println!(
        "previous_version_id: {}",
        prepared.update.previous_version_id
    );
    println!("version_id: {}", prepared.update.element_version_id);
    println!("state_id: {}", prepared.state_id);
    println!("change_id: {}", prepared.change.change_id);
    println!("change_revision_id: {}", prepared.change_revision_id);
    ExitCode::SUCCESS
}

fn fail_update(error: ChangeError) -> ExitCode {
    match error {
        ChangeError::Precondition(
            kat::repository::change::PreconditionError::ElementNotActive(id),
        ) => {
            eprintln!("kat update: element {id} is not active in the base state");
        }
        ChangeError::Conflict => {
            eprintln!(
                "kat update: the accepted repository state changed while updating; nothing was published. Re-run kat update."
            );
        }
        other => eprintln!("kat update: {other}"),
    }
    ExitCode::FAILURE
}

fn update_pipeline(
    repository: &Repository,
    context: ChangeContext,
    expected_version: ObjectId,
    parsed: &UpdateArgs,
) -> Result<PublishedUpdateChange, ChangeError> {
    let mut properties = Vec::new();
    if let Some(title) = &parsed.title {
        properties.push(("title".to_string(), PropertyValue::Text(title.clone())));
    }
    if let Some(description) = &parsed.description {
        properties.push((
            "description".to_string(),
            PropertyValue::Text(description.clone()),
        ));
    }
    let input = UpdateElementInput {
        element_id: parsed.element_id,
        expected_version,
        properties,
    };
    let prepared = apply_update_element(repository, context, input)?;
    let validated = validate_update_element_ontology(prepared)?;
    let validated = validate_update_element_invariants(validated)?;
    let revision = prepare_update_change_revision(validated, ChangeId::new(), None)?;
    let persisted = persist_prepared_update_change(repository, revision)?;
    publish_persisted_update_change(repository, persisted)
}

/// Parsed `kat deprecate` arguments.
struct DeprecateArgs {
    element_id: ElementId,
}

/// `kat deprecate <element-id>` — run a `DeprecateElement` change end to end
/// through the Change Engine and publish it (thin dispatch; all semantics live
/// in the library).
fn run_deprecate(element_id_str: String) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat deprecate: {error}");
            return ExitCode::FAILURE;
        }
    };

    let element_id = match resolve_cli_element_id(&repository, &element_id_str, "deprecate") {
        Ok(id) => id,
        Err(code) => return code,
    };

    let parsed = DeprecateArgs { element_id };

    if has_draft_session(repository.root_dir()) {
        let mut session = match read_draft_session(repository.root_dir()) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("kat deprecate: {err}");
                return ExitCode::FAILURE;
            }
        };

        let expected_version = match session
            .working_state
            .elements
            .iter()
            .find(|e| e.element_id == parsed.element_id)
        {
            Some(entry) => entry.version,
            None => {
                eprintln!(
                    "kat deprecate: element {} not found in base state",
                    parsed.element_id
                );
                return ExitCode::FAILURE;
            }
        };

        let input = DeprecateElementInput {
            element_id: parsed.element_id,
            expected_version,
        };

        let op = match stage_operation_into_session(
            &repository,
            &mut session,
            StagedOperationInput::DeprecateElement(input),
        ) {
            Ok(op) => op,
            Err(err) => return fail_deprecate(err),
        };

        println!("staged deprecate element");
        println!("  element_id:        {}", parsed.element_id);
        if let Operation::DeprecateElement { new_version, .. } = op {
            println!("  version_id:        {new_version}");
        }
        println!("  change operations: {}", session.operations.len());
        return ExitCode::SUCCESS;
    }

    let context = match prepare_change(&repository) {
        Ok(context) => context,
        Err(error) => return fail_deprecate(error),
    };

    let expected_version = match context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == parsed.element_id)
    {
        Some(entry) => entry.version,
        None => {
            eprintln!(
                "kat deprecate: element {} not found in the base state",
                parsed.element_id
            );
            return ExitCode::FAILURE;
        }
    };

    let published = match deprecate_pipeline(&repository, context, expected_version, &parsed) {
        Ok(published) => published,
        Err(error) => return fail_deprecate(error),
    };

    let prepared = &published.persisted.prepared;
    println!("element_id: {}", prepared.deprecation.element.element_id);
    println!(
        "previous_version_id: {}",
        prepared.deprecation.previous_version_id
    );
    println!("version_id: {}", prepared.deprecation.element_version_id);
    println!("state_id: {}", prepared.state_id);
    println!("change_id: {}", prepared.change.change_id);
    println!("change_revision_id: {}", prepared.change_revision_id);
    ExitCode::SUCCESS
}

fn fail_deprecate(error: ChangeError) -> ExitCode {
    match error {
        ChangeError::Precondition(
            kat::repository::change::PreconditionError::ElementNotActive(id),
        ) => {
            eprintln!("kat deprecate: element {id} is not active in the base state");
        }
        ChangeError::Conflict => {
            eprintln!(
                "kat deprecate: the accepted repository state changed while deprecating; nothing was published. Re-run kat deprecate."
            );
        }
        other => eprintln!("kat deprecate: {other}"),
    }
    ExitCode::FAILURE
}

fn deprecate_pipeline(
    repository: &Repository,
    context: ChangeContext,
    expected_version: ObjectId,
    parsed: &DeprecateArgs,
) -> Result<PublishedDeprecateChange, ChangeError> {
    let input = DeprecateElementInput {
        element_id: parsed.element_id,
        expected_version,
    };
    let prepared = apply_deprecate_element(repository, context, input)?;
    let validated = validate_deprecate_element_ontology(prepared)?;
    let validated = validate_deprecate_element_invariants(validated)?;
    let revision = prepare_deprecate_change_revision(validated, ChangeId::new(), None)?;
    let persisted = persist_prepared_deprecate_change(repository, revision)?;
    publish_persisted_deprecate_change(repository, persisted)
}

/// Parsed `kat supersede` arguments.
struct SupersedeArgs {
    existing_element_id: ElementId,
    replacement_type_arg: String,
    title: String,
    description: Option<String>,
}

/// `kat supersede <existing-id> <replacement-type> --title "..." [--description "..."]` — run a
/// `SupersedeElement` change end to end through the Change Engine and publish it.
fn run_supersede(
    existing_id_str: String,
    replacement_type: String,
    title: String,
    description: Option<String>,
) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat supersede: {error}");
            return ExitCode::FAILURE;
        }
    };

    let existing_element_id =
        match resolve_cli_element_id(&repository, &existing_id_str, "supersede") {
            Ok(id) => id,
            Err(code) => return code,
        };

    let parsed = SupersedeArgs {
        existing_element_id,
        replacement_type_arg: replacement_type,
        title,
        description,
    };

    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat supersede: {error}");
            return ExitCode::FAILURE;
        }
    };

    let context = match prepare_change(&repository) {
        Ok(context) => context,
        Err(error) => return fail_supersede(error),
    };

    let previous_version_id = match context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == parsed.existing_element_id)
    {
        Some(entry) => entry.version,
        None => {
            eprintln!(
                "kat supersede: element {} not found in the base state",
                parsed.existing_element_id
            );
            return ExitCode::FAILURE;
        }
    };

    let replacement_type_id =
        match resolve_element_type(&context.ontology, &parsed.replacement_type_arg) {
            Ok(type_id) => type_id,
            Err(message) => {
                eprintln!("kat supersede: {message}");
                return ExitCode::FAILURE;
            }
        };

    if has_draft_session(repository.root_dir()) {
        let mut session = match read_draft_session(repository.root_dir()) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("kat supersede: {err}");
                return ExitCode::FAILURE;
            }
        };

        let expected_existing_version = match session
            .working_state
            .elements
            .iter()
            .find(|e| e.element_id == parsed.existing_element_id)
        {
            Some(entry) => entry.version,
            None => {
                eprintln!(
                    "kat supersede: element {} not found in candidate working state",
                    parsed.existing_element_id
                );
                return ExitCode::FAILURE;
            }
        };

        let replacement_element_id = ElementId::new();
        let relationship_id = RelationshipId::new();
        let mut replacement_properties = Vec::new();
        replacement_properties.push(("title".to_string(), PropertyValue::Text(parsed.title)));
        if let Some(desc) = parsed.description {
            replacement_properties.push(("description".to_string(), PropertyValue::Text(desc)));
        }

        let input = SupersedeElementInput {
            existing_element_id: parsed.existing_element_id,
            expected_existing_version,
            replacement_element_id,
            replacement_type_id,
            replacement_properties,
            relationship_id,
        };

        let _op = match stage_operation_into_session(
            &repository,
            &mut session,
            StagedOperationInput::SupersedeElement(input),
        ) {
            Ok(op) => op,
            Err(err) => return fail_supersede(err),
        };

        println!("staged supersede element");
        println!("  existing_element_id:    {}", parsed.existing_element_id);
        println!("  replacement_element_id: {replacement_element_id}");
        println!("  change operations:      {}", session.operations.len());
        return ExitCode::SUCCESS;
    }

    let published = match supersede_pipeline(
        &repository,
        context,
        previous_version_id,
        replacement_type_id,
        &parsed,
    ) {
        Ok(published) => published,
        Err(error) => return fail_supersede(error),
    };

    let prepared = &published.persisted.prepared;
    println!(
        "existing_element_id: {}",
        prepared.supersede.existing_element_id
    );
    println!(
        "previous_version_id: {}",
        prepared.supersede.previous_existing_version_id
    );
    println!(
        "superseded_version_id: {}",
        prepared.supersede.new_existing_version_id
    );
    println!();
    println!(
        "replacement_element_id: {}",
        prepared.supersede.replacement_element_id
    );
    println!(
        "replacement_version_id: {}",
        prepared.supersede.replacement_version_id
    );
    println!();
    println!("relationship_id: {}", prepared.supersede.relationship_id);
    println!(
        "relationship_version_id: {}",
        prepared.supersede.relationship_version_id
    );
    println!();
    println!("state_id: {}", prepared.state_id);
    println!("change_id: {}", prepared.change.change_id);
    println!("change_revision_id: {}", prepared.change_revision_id);
    ExitCode::SUCCESS
}

fn fail_supersede(error: ChangeError) -> ExitCode {
    match error {
        ChangeError::Precondition(
            kat::repository::change::PreconditionError::ElementNotActive(id),
        ) => {
            eprintln!("kat supersede: element {id} is not active in the base state");
        }
        ChangeError::Conflict => {
            eprintln!(
                "kat supersede: the accepted repository state changed while superseding; nothing was published. Re-run kat supersede."
            );
        }
        other => eprintln!("kat supersede: {other}"),
    }
    ExitCode::FAILURE
}

fn supersede_pipeline(
    repository: &Repository,
    context: ChangeContext,
    expected_existing_version: ObjectId,
    replacement_type_id: String,
    parsed: &SupersedeArgs,
) -> Result<PublishedSupersedeChange, ChangeError> {
    let mut replacement_properties = vec![(
        "title".to_string(),
        PropertyValue::Text(parsed.title.clone()),
    )];
    if let Some(description) = &parsed.description {
        replacement_properties.push((
            "description".to_string(),
            PropertyValue::Text(description.clone()),
        ));
    }
    let input = SupersedeElementInput {
        existing_element_id: parsed.existing_element_id,
        expected_existing_version,
        replacement_element_id: ElementId::new(),
        replacement_type_id,
        replacement_properties,
        relationship_id: RelationshipId::new(),
    };
    let prepared = apply_supersede_element(repository, context, input)?;
    let validated = validate_supersede_element_ontology(prepared)?;
    let validated = validate_supersede_element_invariants(validated)?;
    let revision = prepare_supersede_change_revision(validated, ChangeId::new(), None)?;
    let persisted = persist_prepared_supersede_change(repository, revision)?;
    publish_persisted_supersede_change(repository, persisted)
}

/// Parsed `kat link` arguments.
struct LinkArgs {
    relationship_type_arg: String,
    source_element_id: ElementId,
    target_element_id: ElementId,
    description: Option<String>,
}

/// `kat link <relationship-type> <source-element-id> <target-element-id> [--description "..."]` —
/// run a `LinkElement` change end to end through the Change Engine and publish it.
fn run_link(
    type_str: String,
    source_str: String,
    target_str: String,
    description: Option<String>,
) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat link: {error}");
            return ExitCode::FAILURE;
        }
    };

    let source_element_id = match resolve_cli_element_id(&repository, &source_str, "link") {
        Ok(id) => id,
        Err(code) => return code,
    };

    let target_element_id = match resolve_cli_element_id(&repository, &target_str, "link") {
        Ok(id) => id,
        Err(code) => return code,
    };

    let parsed = LinkArgs {
        relationship_type_arg: type_str,
        source_element_id,
        target_element_id,
        description,
    };

    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat link: {error}");
            return ExitCode::FAILURE;
        }
    };

    let context = match prepare_change(&repository) {
        Ok(context) => context,
        Err(error) => return fail_link(error),
    };

    let relationship_type_id =
        match resolve_relationship_type(&context.ontology, &parsed.relationship_type_arg) {
            Ok(type_id) => type_id,
            Err(message) => {
                eprintln!("kat link: {message}");
                return ExitCode::FAILURE;
            }
        };

    if has_draft_session(repository.root_dir()) {
        let mut session = match read_draft_session(repository.root_dir()) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("kat link: {err}");
                return ExitCode::FAILURE;
            }
        };

        let relationship_id = RelationshipId::new();
        let input = LinkElementInput {
            relationship_id,
            relationship_type_id,
            source_element_id: parsed.source_element_id,
            target_element_id: parsed.target_element_id,
            properties: vec![],
        };

        let _op = match stage_operation_into_session(
            &repository,
            &mut session,
            StagedOperationInput::LinkElement(input),
        ) {
            Ok(op) => op,
            Err(err) => return fail_link(err),
        };

        println!("staged link");
        println!("  relationship_id:   {relationship_id}");
        println!("  change operations: {}", session.operations.len());
        return ExitCode::SUCCESS;
    }

    let relationship_id = RelationshipId::new();
    let change_id = ChangeId::new();

    let prepared = match apply_link_element(
        &repository,
        context,
        LinkElementInput {
            relationship_id,
            relationship_type_id,
            source_element_id: parsed.source_element_id,
            target_element_id: parsed.target_element_id,
            properties: vec![],
        },
    ) {
        Ok(prepared) => prepared,
        Err(error) => return fail_link(error),
    };

    let ont_validated = match validate_link_element_ontology(prepared) {
        Ok(validated) => validated,
        Err(error) => return fail_link(error),
    };

    let inv_validated = match validate_link_element_invariants(ont_validated) {
        Ok(validated) => validated,
        Err(error) => return fail_link(error),
    };

    let prepared_revision =
        match prepare_link_change_revision(inv_validated, change_id, parsed.description) {
            Ok(revision) => revision,
            Err(error) => return fail_link(error),
        };

    let persisted = match persist_prepared_link_change(&repository, prepared_revision) {
        Ok(persisted) => persisted,
        Err(error) => return fail_link(error),
    };

    let published = match publish_persisted_link_change(&repository, persisted) {
        Ok(published) => published,
        Err(error) => return fail_link(error),
    };

    let prepared = &published.persisted.prepared;
    println!("relationship_id: {}", prepared.link.relationship_id);
    println!(
        "relationship_version_id: {}",
        prepared.link.relationship_version_id
    );
    println!(
        "source_element_id: {}",
        prepared.link.relationship.source_element_id
    );
    println!(
        "target_element_id: {}",
        prepared.link.relationship.target_element_id
    );
    println!("state_id: {}", prepared.state_id);
    println!("change_id: {}", prepared.change.change_id);
    println!("change_revision_id: {}", prepared.change_revision_id);
    ExitCode::SUCCESS
}

fn fail_link(error: ChangeError) -> ExitCode {
    match error {
        ChangeError::Precondition(
            kat::repository::change::PreconditionError::ElementNotActive(id),
        ) => {
            eprintln!("kat link: element {id} is not active in the base state");
        }
        ChangeError::Conflict => {
            eprintln!(
                "kat link: the accepted repository state changed while linking; nothing was published. Re-run kat link."
            );
        }
        other => eprintln!("kat link: {other}"),
    }
    ExitCode::FAILURE
}

struct UnlinkArgs {
    relationship_id: RelationshipId,
    description: Option<String>,
}

fn run_unlink(relationship_id_str: String, description: Option<String>) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat unlink: {error}");
            return ExitCode::FAILURE;
        }
    };

    let relationship_id =
        match resolve_cli_relationship_id(&repository, &relationship_id_str, "unlink") {
            Ok(id) => id,
            Err(code) => return code,
        };

    let parsed = UnlinkArgs {
        relationship_id,
        description,
    };

    if has_draft_session(repository.root_dir()) {
        let mut session = match read_draft_session(repository.root_dir()) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("kat unlink: {err}");
                return ExitCode::FAILURE;
            }
        };

        let expected_version = match session
            .working_state
            .relationships
            .iter()
            .find(|r| r.relationship_id == parsed.relationship_id)
        {
            Some(entry) => entry.version,
            None => {
                eprintln!(
                    "kat unlink: relationship {} not found in base state",
                    parsed.relationship_id
                );
                return ExitCode::FAILURE;
            }
        };

        let input = UnlinkElementInput {
            relationship_id: parsed.relationship_id,
            expected_version,
        };

        let _op = match stage_operation_into_session(
            &repository,
            &mut session,
            StagedOperationInput::UnlinkElement(input),
        ) {
            Ok(op) => op,
            Err(err) => return fail_unlink(err),
        };

        println!("staged unlink");
        println!("  relationship_id:   {}", parsed.relationship_id);
        println!("  change operations: {}", session.operations.len());
        return ExitCode::SUCCESS;
    }

    let context = match prepare_change(&repository) {
        Ok(context) => context,
        Err(error) => return fail_unlink(error),
    };

    let expected_version = match context
        .base_state
        .relationships
        .iter()
        .find(|r| r.relationship_id == parsed.relationship_id)
    {
        Some(entry) => entry.version,
        None => {
            eprintln!(
                "kat unlink: relationship {} not found in the accepted state",
                parsed.relationship_id
            );
            return ExitCode::FAILURE;
        }
    };

    let change_id = ChangeId::new();

    let prepared = match apply_unlink_element(
        &repository,
        context,
        UnlinkElementInput {
            relationship_id: parsed.relationship_id,
            expected_version,
        },
    ) {
        Ok(prepared) => prepared,
        Err(error) => return fail_unlink(error),
    };

    let validated = match validate_unlink_element_invariants(prepared) {
        Ok(validated) => validated,
        Err(error) => return fail_unlink(error),
    };

    let prepared_revision =
        match prepare_unlink_change_revision(validated, change_id, parsed.description) {
            Ok(revision) => revision,
            Err(error) => return fail_unlink(error),
        };

    let persisted = match persist_prepared_unlink_change(&repository, prepared_revision) {
        Ok(persisted) => persisted,
        Err(error) => return fail_unlink(error),
    };

    let published = match publish_persisted_unlink_change(&repository, persisted) {
        Ok(published) => published,
        Err(error) => return fail_unlink(error),
    };

    let prepared = &published.persisted.prepared;
    println!("relationship_id: {}", prepared.unlink.relationship_id);
    println!("state_id: {}", prepared.state_id);
    println!("change_id: {}", prepared.change.change_id);
    println!("change_revision_id: {}", prepared.change_revision_id);
    ExitCode::SUCCESS
}

fn fail_unlink(error: ChangeError) -> ExitCode {
    match error {
        ChangeError::Precondition(
            kat::repository::change::PreconditionError::RelationshipNotFound(id),
        ) => {
            eprintln!("kat unlink: relationship {id} not found in the accepted state");
        }
        ChangeError::Conflict => {
            eprintln!(
                "kat unlink: the accepted repository state changed while unlinking; nothing was published. Re-run kat unlink."
            );
        }
        other => eprintln!("kat unlink: {other}"),
    }
    ExitCode::FAILURE
}

/// `kat account <artifact-id> [--description "..."]` —
/// re-baselines direct accountability relationships of an artifact.
fn run_account(artifact_id_str: String, description: Option<String>) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat account: {error}");
            return ExitCode::FAILURE;
        }
    };

    let artifact_id = match resolve_cli_element_id(&repository, &artifact_id_str, "account") {
        Ok(id) => id,
        Err(code) => return code,
    };

    let root = repository.root_dir();
    if has_draft_session(root) {
        let mut session = match read_draft_session(root) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("kat account: {err}");
                return ExitCode::FAILURE;
            }
        };

        let input = AccountArtifactInput { artifact_id };
        let op = match stage_operation_into_session(
            &repository,
            &mut session,
            StagedOperationInput::AccountArtifact(input),
        ) {
            Ok(op) => op,
            Err(err) => {
                eprintln!("kat account: {err}");
                return ExitCode::FAILURE;
            }
        };

        println!("staged account artifact");
        println!("  artifact_id:       {artifact_id}");
        if let Operation::AccountArtifact {
            reconciliations, ..
        } = op
        {
            println!("  reconciliations:   {}", reconciliations.len());
        }
        println!("  change operations: {}", session.operations.len());
        return ExitCode::SUCCESS;
    }

    let input = AccountArtifactInput { artifact_id };
    match account_artifact(&repository, input, description) {
        Ok(published) => {
            let prep = &published.persisted.prepared;
            println!("artifact_id: {}", prep.account.artifact_id);
            println!("reconciliations: {}", prep.account.reconciliations.len());
            println!("state_id: {}", prep.state_id);
            println!("change_id: {}", prep.change.change_id);
            println!("change_revision_id: {}", prep.change_revision_id);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("kat account: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run_show(element_id_str: String, compact: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat show: {error}");
            return ExitCode::FAILURE;
        }
    };

    let element_id = match resolve_cli_element_id(&repository, &element_id_str, "show") {
        Ok(id) => id,
        Err(code) => return code,
    };
    match show_element(&repository, element_id) {
        Ok(view) => {
            if compact {
                print_show_element_view_compact(&view);
            } else {
                print_show_element_view(&view);
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

fn print_show_element_view_compact(view: &kat::repository::query::ElementView) {
    let id_short = &view.element_id.to_string()[..8];
    let type_short = view
        .element
        .type_id
        .rsplit('/')
        .next()
        .unwrap_or(&view.element.type_id);
    let state_short = format_lifecycle(view.element.lifecycle);
    let title = view
        .element
        .properties
        .iter()
        .find(|(k, _)| k == "title")
        .and_then(|(_, v)| match v {
            PropertyValue::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .unwrap_or("none");
    println!("{id_short}  {type_short:<16}  {state_short:<10}  {title}");
}

fn print_show_element_view(view: &kat::repository::query::ElementView) {
    println!("Element {}", view.element_id);
    println!();
    println!("Identity");
    println!("  version:     {}", short_object_id(&view.version_id));
    println!("  type:        {}", view.element.type_id);
    println!(
        "  lifecycle:   {}",
        format_lifecycle(view.element.lifecycle)
    );

    println!();
    println!("Details");
    let title = view
        .element
        .properties
        .iter()
        .find(|(k, _)| k == "title")
        .and_then(|(_, v)| match v {
            PropertyValue::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .unwrap_or("none");
    println!("  title:       {title}");

    let description = view
        .element
        .properties
        .iter()
        .find(|(k, _)| k == "description")
        .and_then(|(_, v)| match v {
            PropertyValue::Text(d) => Some(d.as_str()),
            _ => None,
        })
        .unwrap_or("none");
    println!("  description: {description}");

    println!();
    println!("Properties");
    let other_props: Vec<_> = view
        .element
        .properties
        .iter()
        .filter(|(k, _)| k != "title" && k != "description")
        .collect();
    if other_props.is_empty() {
        println!("  none");
    } else {
        for (k, v) in other_props {
            println!("  {k}: {v}");
        }
    }

    println!();
    println!("Relationships");
    let has_incoming = !view.relationships.incoming.is_empty();
    let has_outgoing = !view.relationships.outgoing.is_empty();

    if !has_incoming && !has_outgoing {
        println!("  none");
    } else {
        println!("  DIR  REL ID    TYPE             ELEMENT   TITLE");
        for rel in &view.relationships.incoming {
            let rel_short_id = &rel.relationship_id.to_string()[..8];
            let short_type = rel
                .relationship_type_id
                .rsplit('/')
                .next()
                .unwrap_or(&rel.relationship_type_id);
            let elem_short_id = &rel.other_element_id.to_string()[..8];
            let title = rel.other_title.as_deref().unwrap_or("none");
            println!("  in   {rel_short_id:<9} {short_type:<16} {elem_short_id:<9} {title}");
        }
        for rel in &view.relationships.outgoing {
            let rel_short_id = &rel.relationship_id.to_string()[..8];
            let short_type = rel
                .relationship_type_id
                .rsplit('/')
                .next()
                .unwrap_or(&rel.relationship_type_id);
            let elem_short_id = &rel.other_element_id.to_string()[..8];
            let title = rel.other_title.as_deref().unwrap_or("none");
            println!("  out  {rel_short_id:<9} {short_type:<16} {elem_short_id:<9} {title}");
        }
    }
}

fn cmd_history(
    oneline: bool,
    limit: Option<usize>,
    element_str: Option<String>,
    compact: bool,
) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat history: {error}");
            return ExitCode::FAILURE;
        }
    };

    let target_element_id = if let Some(ref el) = element_str {
        match resolve_cli_element_id(&repository, el, "history") {
            Ok(id) => Some(id),
            Err(code) => return code,
        }
    } else {
        None
    };

    if limit == Some(0) {
        eprintln!("kat history: --limit must be at least 1");
        return ExitCode::FAILURE;
    }

    match history(&repository) {
        Ok(mut entries) => {
            if let Some(target_id) = target_element_id {
                entries.retain(|e| {
                    history_entry_touches_element(&repository, e, target_id).unwrap_or(false)
                });
            }

            if let Some(l) = limit {
                entries.truncate(l);
            }

            let is_compact = oneline || compact;
            if !is_compact {
                let count = entries.len();
                let noun = if count == 1 { "revision" } else { "revisions" };
                println!("Accepted change history ({count} {noun})");
                println!();
            }

            for (i, entry) in entries.iter().enumerate() {
                if is_compact {
                    print_history_entry_oneline(&repository, entry);
                } else {
                    if i > 0 {
                        println!();
                    }
                    print_history_entry(entry);
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("kat history: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_history_entry_oneline(repository: &Repository, entry: &HistoryEntry) {
    let rev_short = short_object_id(&entry.revision_id);
    let op_summary = if entry.change.operations.len() == 1 {
        format_operation_name(&entry.change.operations[0])
    } else {
        &format!("{} operations", entry.change.operations.len())
    };

    let detail = if let Some(desc) = &entry.change.description {
        desc.clone()
    } else if entry.change.operations.len() == 1 {
        get_operation_title(repository.object_store(), &entry.change.operations[0])
            .unwrap_or_else(|| "none".into())
    } else {
        "none".into()
    };

    println!("{rev_short}  {op_summary:<18}  {detail}");
}

fn get_operation_title(store: &ObjectStore, op: &Operation) -> Option<String> {
    match op {
        Operation::CreateElement { new_version } | Operation::UpdateElement { new_version, .. } => {
            if let Some(CanonicalPayload::KnowledgeElementVersion(ev)) = store
                .get(*new_version)
                .ok()
                .and_then(|b| decode_canonical(&b).ok())
                .map(|o| o.payload)
            {
                return ev.properties.iter().find(|(k, _)| k == "title").and_then(
                    |(_, v)| match v {
                        PropertyValue::Text(t) => Some(t.clone()),
                        _ => None,
                    },
                );
            }
        }
        Operation::Supersede {
            replacement_version,
            ..
        } => {
            if let Some(CanonicalPayload::KnowledgeElementVersion(ev)) = store
                .get(*replacement_version)
                .ok()
                .and_then(|b| decode_canonical(&b).ok())
                .map(|o| o.payload)
            {
                return ev.properties.iter().find(|(k, _)| k == "title").and_then(
                    |(_, v)| match v {
                        PropertyValue::Text(t) => Some(t.clone()),
                        _ => None,
                    },
                );
            }
        }
        _ => {}
    }
    None
}

fn run_trace(
    element_id_str: String,
    paths: bool,
    max_depth: Option<usize>,
    compact: bool,
) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat trace: {error}");
            return ExitCode::FAILURE;
        }
    };

    let element_id = match resolve_cli_element_id(&repository, &element_id_str, "trace") {
        Ok(id) => id,
        Err(code) => return code,
    };

    match trace_origin(&repository, element_id, max_depth) {
        Ok(result) => {
            if compact {
                print_trace_result_compact(&repository, &result);
            } else if paths {
                print_trace_result_paths(&repository, &result);
            } else {
                print_trace_result_tree(&repository, &result);
            }
            ExitCode::SUCCESS
        }
        Err(QueryError::ElementNotFound(id)) => {
            eprintln!("kat trace: element {id} not found in the accepted state");
            ExitCode::FAILURE
        }
        Err(QueryError::InvalidMaxDepth(depth)) => {
            eprintln!("kat trace: max depth must be greater than 0, got {depth}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("kat trace: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_trace_result_compact(repository: &Repository, result: &TraceResult) {
    if result.paths.is_empty() {
        println!("none");
        return;
    }
    for (path_idx, path) in result.paths.iter().enumerate() {
        let mut steps_str = Vec::new();
        for step in &path.steps {
            let title = if let Ok(target_view) = show_element(repository, step.from_element_id) {
                target_view
                    .element
                    .properties
                    .iter()
                    .find(|(k, _)| k == "title")
                    .and_then(|(_, v)| match v {
                        PropertyValue::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| step.from_element_id.to_string()[..8].to_string())
            } else {
                step.from_element_id.to_string()[..8].to_string()
            };
            steps_str.push(title);
        }
        if let Some(last_step) = path.steps.last() {
            let title = if let Ok(target_view) = show_element(repository, last_step.to_element_id) {
                target_view
                    .element
                    .properties
                    .iter()
                    .find(|(k, _)| k == "title")
                    .and_then(|(_, v)| match v {
                        PropertyValue::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| last_step.to_element_id.to_string()[..8].to_string())
            } else {
                last_step.to_element_id.to_string()[..8].to_string()
            };
            steps_str.push(title);
        }
        let chain = steps_str.join(" -> ");
        if result.paths.len() > 1 {
            println!("{}. {chain}", path_idx + 1);
        } else {
            println!("{chain}");
        }
    }
}

fn print_trace_result_tree(repository: &Repository, result: &TraceResult) {
    println!("Trace origin for element {}", result.root_element_id);
    if let Ok(view) = show_element(repository, result.root_element_id) {
        println!("  type:        {}", view.element.type_id);
        println!(
            "  lifecycle:   {}",
            format_lifecycle(view.element.lifecycle)
        );
        if let Some((_, PropertyValue::Text(title))) =
            view.element.properties.iter().find(|(k, _)| k == "title")
        {
            println!("  title:       \"{title}\"");
        }
    }

    println!();
    if result.paths.is_empty() {
        println!("Origin paths");
        println!("  none");
        return;
    }

    let tree = result.to_tree();
    println!("Origin tree");
    print_tree_node(repository, &tree, "", true, true);
}

fn print_tree_node(
    repository: &Repository,
    node: &TraceTreeNode,
    prefix: &str,
    _is_last: bool,
    is_root: bool,
) {
    if is_root {
        println!("  {}", format_node_label(repository, node.element_id));
    }

    for (idx, child) in node.children.iter().enumerate() {
        let child_is_last = idx + 1 == node.children.len();
        let dir_str = match child.direction {
            TraversalDirection::Forward => "forward ->",
            TraversalDirection::Backward => "backward <-",
        };
        let branch = if child_is_last {
            "└── "
        } else {
            "├── "
        };
        let next_prefix = if child_is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };

        println!(
            "  {prefix}{branch}via {} ({dir_str})",
            child.relationship_type_id
        );
        let node_label = format_node_label(repository, child.target.element_id);
        println!("  {next_prefix}└── {node_label}");

        print_tree_node(
            repository,
            &child.target,
            &next_prefix,
            child_is_last,
            false,
        );
    }
}

fn format_node_label(repository: &Repository, element_id: ElementId) -> String {
    if let Ok(view) = show_element(repository, element_id) {
        let title_suffix = view
            .element
            .properties
            .iter()
            .find(|(k, _)| k == "title")
            .and_then(|(_, v)| match v {
                PropertyValue::Text(t) => Some(format!(" \"{t}\"")),
                _ => None,
            })
            .unwrap_or_default();
        format!("{} [{}]{title_suffix}", element_id, view.element.type_id)
    } else {
        format!("{element_id}")
    }
}

fn print_trace_result_paths(repository: &Repository, result: &TraceResult) {
    println!("Trace origin for element {}", result.root_element_id);
    if let Ok(view) = show_element(repository, result.root_element_id) {
        println!("  type:        {}", view.element.type_id);
        println!(
            "  lifecycle:   {}",
            format_lifecycle(view.element.lifecycle)
        );
        if let Some((_, PropertyValue::Text(title))) =
            view.element.properties.iter().find(|(k, _)| k == "title")
        {
            println!("  title:       \"{title}\"");
        }
    }

    println!();
    if result.paths.is_empty() {
        println!("Origin paths");
        println!("  none");
        return;
    }

    for (path_idx, path) in result.paths.iter().enumerate() {
        if path_idx > 0 {
            println!();
        }
        println!("Path {}", path_idx + 1);
        for (step_idx, step) in path.steps.iter().enumerate() {
            let dir_label = match step.direction {
                TraversalDirection::Forward => "forward ->",
                TraversalDirection::Backward => "backward <-",
            };
            println!("  Step {}", step_idx + 1);
            println!("    from:          {}", step.from_element_id);
            println!("    relationship:  {}", step.relationship_id);
            println!("    type:          {}", step.relationship_type_id);
            println!("    direction:     {dir_label}");
            print!("    to:            {}", step.to_element_id);
            if let Ok(target_view) = show_element(repository, step.to_element_id) {
                print!(" [{}]", target_view.element.type_id);
                if let Some((_, PropertyValue::Text(title))) = target_view
                    .element
                    .properties
                    .iter()
                    .find(|(k, _)| k == "title")
                {
                    print!(" \"{title}\"");
                }
            }
            println!();
        }
    }
}

fn run_impact(element_id_str: String, max_depth: Option<usize>, compact: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat impact: {error}");
            return ExitCode::FAILURE;
        }
    };

    let element_id = match resolve_cli_element_id(&repository, &element_id_str, "impact") {
        Ok(id) => id,
        Err(code) => return code,
    };

    match analyze_impact(&repository, element_id, max_depth) {
        Ok(result) => {
            if compact {
                print_impact_result_compact(&repository, &result);
            } else {
                print_impact_result(&repository, &result);
            }
            ExitCode::SUCCESS
        }
        Err(QueryError::ElementNotFound(id)) => {
            eprintln!("kat impact: element {id} not found in the accepted state");
            ExitCode::FAILURE
        }
        Err(QueryError::InvalidMaxDepth(depth)) => {
            eprintln!("kat impact: max depth must be greater than 0, got {depth}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("kat impact: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_impact_result_compact(repository: &Repository, result: &ImpactResult) {
    println!("CATEGORY  TYPE             ID        TITLE");
    for id in &result.directly_changed {
        let (type_name, title) = if let Ok(view) = show_element(repository, *id) {
            let t = view
                .element
                .properties
                .iter()
                .find(|(k, _)| k == "title")
                .and_then(|(_, v)| match v {
                    PropertyValue::Text(t) => Some(t.to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| "none".into());
            let short_t = view
                .element
                .type_id
                .rsplit('/')
                .next()
                .unwrap_or(&view.element.type_id)
                .to_string();
            (short_t, t)
        } else {
            ("unknown".into(), "none".into())
        };
        let short_id = &id.to_string()[..8];
        println!("direct    {type_name:<16} {short_id:<9} {title}");
    }
    for item in &result.semantically_affected {
        let title = if let Ok(view) = show_element(repository, item.element_id) {
            view.element
                .properties
                .iter()
                .find(|(k, _)| k == "title")
                .and_then(|(_, v)| match v {
                    PropertyValue::Text(t) => Some(t.to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| "none".into())
        } else {
            "none".into()
        };
        let type_name = item.type_id.rsplit('/').next().unwrap_or(&item.type_id);
        let short_id = &item.element_id.to_string()[..8];
        println!("semantic  {type_name:<16} {short_id:<9} {title}");
    }
    for item in &result.affected_artifacts {
        let title = if let Ok(view) = show_element(repository, item.element_id) {
            view.element
                .properties
                .iter()
                .find(|(k, _)| k == "title")
                .and_then(|(_, v)| match v {
                    PropertyValue::Text(t) => Some(t.to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| "none".into())
        } else {
            "none".into()
        };
        let type_name = item.type_id.rsplit('/').next().unwrap_or(&item.type_id);
        let short_id = &item.element_id.to_string()[..8];
        println!("artifact  {type_name:<16} {short_id:<9} {title}");
    }
}

/// directly_changed:
///   <root_id> [<type>] "<title>"
///
/// semantically_affected:
///   <target_id> [<type>] "<title>"
///     via kat.core/addresses (backward) -> <from_id>
///
/// affected_artifacts:
///   <artifact_id> [<type>] "<title>"
///     via kat.core/represents (backward) -> <impl_id>
/// ```
fn print_impact_result(repository: &Repository, result: &ImpactResult) {
    if let Some(root_id) = result.directly_changed.first() {
        println!("Impact analysis for element {root_id}");
    } else {
        println!("Impact analysis");
    }

    println!();
    println!("Directly changed");
    if result.directly_changed.is_empty() {
        println!("  none");
    } else {
        for id in &result.directly_changed {
            print!("  Element {id}");
            if let Ok(view) = show_element(repository, *id) {
                print!(" [{}]", view.element.type_id);
                print_title_property(&view.element.properties);
            }
            println!();
        }
    }

    println!();
    let sem_count = result.semantically_affected.len();
    println!("Semantically affected elements ({sem_count})");
    if result.semantically_affected.is_empty() {
        println!("  none");
    } else {
        for elem in &result.semantically_affected {
            print!("  Element {}", elem.element_id);
            print!(" [{}]", elem.type_id);
            if let Ok(view) = show_element(repository, elem.element_id) {
                print_title_property(&view.element.properties);
            }
            println!();
            for (path_idx, path) in elem.paths.iter().enumerate() {
                for (step_idx, step) in path.steps.iter().enumerate() {
                    let dir_label = match step.direction {
                        TraversalDirection::Forward => "forward ->",
                        TraversalDirection::Backward => "backward <-",
                    };
                    println!(
                        "    path {}, step {}: via {} ({dir_label}) from {}",
                        path_idx + 1,
                        step_idx + 1,
                        step.relationship_type_id,
                        step.from_element_id
                    );
                }
            }
        }
    }

    println!();
    let art_count = result.affected_artifacts.len();
    println!("Affected artifacts ({art_count})");
    if result.affected_artifacts.is_empty() {
        println!("  none");
    } else {
        for elem in &result.affected_artifacts {
            print!("  Artifact {}", elem.element_id);
            print!(" [{}]", elem.type_id);
            if let Ok(view) = show_element(repository, elem.element_id) {
                print_title_property(&view.element.properties);
            }
            println!();
            for (path_idx, path) in elem.paths.iter().enumerate() {
                for (step_idx, step) in path.steps.iter().enumerate() {
                    let dir_label = match step.direction {
                        TraversalDirection::Forward => "forward ->",
                        TraversalDirection::Backward => "backward <-",
                    };
                    println!(
                        "    path {}, step {}: via {} ({dir_label}) from {}",
                        path_idx + 1,
                        step_idx + 1,
                        step.relationship_type_id,
                        step.from_element_id
                    );
                }
            }
        }
    }

    println!();
    println!("Summary");
    println!(
        "  total impacted: {}",
        result.semantically_affected.len() + result.affected_artifacts.len()
    );
}

fn print_title_property(properties: &[(String, PropertyValue)]) {
    if let Some((_, PropertyValue::Text(title))) = properties.iter().find(|(k, _)| k == "title") {
        print!(" \"{title}\"");
    }
}

fn print_history_entry(entry: &HistoryEntry) {
    println!("Revision {}", short_object_id(&entry.revision_id));
    println!("  change:        {}", entry.change.change_id);
    println!(
        "  result_state:  {}",
        short_object_id(&entry.change.result_state)
    );
    println!("  base_states:");
    print_short_ids_or_none(&entry.change.base_states);
    println!("  dependencies:");
    print_short_ids_or_none(&entry.change.dependencies);
    println!(
        "  description:   {}",
        entry.change.description.as_deref().unwrap_or("none")
    );
    println!("  operations:");
    for operation in &entry.change.operations {
        println!("    {}", format_operation_name(operation));
        print_operation_details(operation);
    }
}

fn print_short_ids_or_none(ids: &[ObjectId]) {
    if ids.is_empty() {
        println!("    none");
    } else {
        for id in ids {
            println!("    {}", short_object_id(id));
        }
    }
}

fn print_operation_details(operation: &Operation) {
    match operation {
        Operation::CreateElement { new_version } => {
            println!("      version:     {}", short_object_id(new_version));
        }
        Operation::UpdateElement {
            element_id,
            expected_version,
            new_version,
        } => {
            println!("      element:     {element_id}");
            println!("      expected:    {}", short_object_id(expected_version));
            println!("      new_version: {}", short_object_id(new_version));
        }
        Operation::DeprecateElement {
            element_id,
            expected_version,
            new_version,
        } => {
            println!("      element:     {element_id}");
            println!("      expected:    {}", short_object_id(expected_version));
            println!("      new_version: {}", short_object_id(new_version));
        }
        Operation::Link {
            new_relationship_version,
        } => {
            println!(
                "      version:     {}",
                short_object_id(new_relationship_version)
            );
        }
        Operation::Unlink {
            relationship_id,
            expected_version,
        } => {
            println!("      relationship: {relationship_id}");
            println!("      expected:     {}", short_object_id(expected_version));
        }
        Operation::Supersede {
            existing_element,
            expected_existing_version,
            replacement_element,
            replacement_version,
            superseding_relationship,
        } => {
            println!("      existing:     {existing_element}");
            println!(
                "      expected:     {}",
                short_object_id(expected_existing_version)
            );
            println!("      replacement:  {replacement_element}");
            println!(
                "      rep_version:  {}",
                short_object_id(replacement_version)
            );
            println!(
                "      relationship: {}",
                short_object_id(superseding_relationship)
            );
        }
        Operation::AccountArtifact {
            artifact_id,
            reconciliations,
        } => {
            println!("      artifact:        {artifact_id}");
            println!("      reconciliations: {}", reconciliations.len());
        }
    }
}

fn cmd_validate(coverage: bool, compact: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat validate: {error}");
            return ExitCode::FAILURE;
        }
    };

    match validate_repository(&repository) {
        Ok(report) => {
            if coverage {
                if compact {
                    print_coverage_report_compact(&report);
                } else {
                    print_coverage_report(&report);
                }
            } else if compact {
                print_validation_report_compact(&report);
            } else {
                print_validation_report(&report);
            }
            if report.violations.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("kat validate: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_validation_report_compact(report: &ValidationReport) {
    let total_constraints = report.constraint_details.len();
    let evidence_backed_constraints = report
        .constraint_details
        .iter()
        .filter(|c| !c.validation_evidence.is_empty())
        .count();
    let coverage_pct = if total_constraints > 0 {
        (evidence_backed_constraints as f64 / total_constraints as f64) * 100.0
    } else {
        100.0
    };
    println!(
        "{} violations, {} unverified constraints, constraint_coverage: {}/{} ({:.1}%)",
        report.violations.len(),
        total_constraints,
        evidence_backed_constraints,
        total_constraints,
        coverage_pct
    );
}

fn print_coverage_report_compact(report: &ValidationReport) {
    let cat_parts: Vec<String> = report
        .category_summaries
        .iter()
        .map(|cat| {
            let pct = if cat.total_count > 0 {
                (cat.evidence_backed_count as f64 / cat.total_count as f64) * 100.0
            } else {
                100.0
            };
            format!(
                "{}: {}/{} ({:.1}%)",
                cat.category_type, cat.evidence_backed_count, cat.total_count, pct
            )
        })
        .collect();
    println!(
        "category_coverage: {}; uncovered: {} elements",
        cat_parts.join(", "),
        report.uncovered_elements.len()
    );
}

fn print_validation_report(report: &ValidationReport) {
    let total_constraints = report.constraint_details.len();
    let evidence_backed_constraints = report
        .constraint_details
        .iter()
        .filter(|c| !c.validation_evidence.is_empty())
        .count();
    let coverage_pct = if total_constraints > 0 {
        (evidence_backed_constraints as f64 / total_constraints as f64) * 100.0
    } else {
        100.0
    };

    println!("VALIDATION SUMMARY");
    println!();
    println!(
        "Mechanical Violations:                 {}",
        report.violations.len()
    );
    println!(
        "Mechanically Unverified Constraints:   {}",
        total_constraints
    );
    println!(
        "Validation Evidence Coverage:          {} / {} constraints evidence-backed ({:.1}%)",
        evidence_backed_constraints, total_constraints, coverage_pct
    );
    println!();

    println!("MECHANICAL VIOLATIONS ({})", report.violations.len());
    println!();
    if report.violations.is_empty() {
        println!("None.");
    } else {
        for v in &report.violations {
            print!("- [{:?}] {}", v.kind, v.message);
            if let Some(rel_id) = v.relationship_id {
                print!(" (relationship: {rel_id})");
            }
            println!();
        }
    }
    println!();

    println!(
        "MECHANICALLY UNVERIFIED CONSTRAINTS ({})",
        total_constraints
    );
    println!();
    if report.constraint_details.is_empty() {
        println!("None.");
    } else {
        for c in &report.constraint_details {
            print!("{}", c.constraint_id);
            if let Some(ref title) = c.title {
                print!(" \"{title}\"");
            }
            println!(" [reason: no executable validation rule]");
            if c.validation_evidence.is_empty() {
                println!("  Validation Evidence: 0 validations (Uncovered)");
            } else {
                print!(
                    "  Validation Evidence: {} validation(s): ",
                    c.validation_evidence.len()
                );
                let evidence_strs: Vec<String> = c
                    .validation_evidence
                    .iter()
                    .map(|e| {
                        if let Some(ref t) = e.title {
                            format!("{} (\"{}\")", e.validation_element_id, t)
                        } else {
                            e.validation_element_id.to_string()
                        }
                    })
                    .collect();
                println!("{}", evidence_strs.join(", "));
            }
            if c.constrained_element_ids.is_empty() {
                println!("    constrained_elements: none");
            } else {
                let targets: Vec<String> = c
                    .constrained_element_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect();
                println!("    constrained_elements: {}", targets.join(", "));
            }
        }
        println!();
        println!(
            "> Note: Evidence-backed constraints remain mechanically unverified by KAT (no executable rule engine)."
        );
    }
}

fn print_coverage_report(report: &ValidationReport) {
    println!("EVIDENCE COVERAGE BREAKDOWN");
    println!();
    if report.category_summaries.is_empty() {
        println!("No active knowledge elements found.");
    } else {
        println!(
            "{:<24} {:<8} {:<18} {:<12} {:<10}",
            "CATEGORY", "TOTAL", "EVIDENCE-BACKED", "UNCOVERED", "COVERAGE"
        );
        for cat in &report.category_summaries {
            let pct = if cat.total_count > 0 {
                (cat.evidence_backed_count as f64 / cat.total_count as f64) * 100.0
            } else {
                100.0
            };
            println!(
                "{:<24} {:<8} {:<18} {:<12} {:.1}%",
                cat.category_type,
                cat.total_count,
                cat.evidence_backed_count,
                cat.uncovered_count,
                pct
            );
        }
    }

    println!();
    println!(
        "UNCOVERED KNOWLEDGE ELEMENTS ({})",
        report.uncovered_elements.len()
    );
    println!();
    if report.uncovered_elements.is_empty() {
        println!("All active knowledge elements are backed by validation evidence.");
    } else {
        println!("{:<24} {:<38} {:<30}", "TYPE", "ELEMENT ID", "TITLE");
        for elem in &report.uncovered_elements {
            let title_str = elem.title.as_deref().unwrap_or("-");
            println!(
                "{:<24} {:<38} {:<30}",
                elem.type_id, elem.element_id, title_str
            );
        }
    }
}

fn cmd_artifacts(stale: bool, artifact_id: Option<String>, compact: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat artifacts: {error}");
            return ExitCode::FAILURE;
        }
    };

    let target_id = if let Some(ref q) = artifact_id {
        match resolve_element_id(&repository, q) {
            Ok(id) => Some(id),
            Err(err) => {
                eprintln!("kat artifacts: {err}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };

    let filter = ArtifactFilter {
        stale_only: stale,
        target_artifact_id: target_id,
    };

    match analyze_artifact_accountability_filtered(&repository, filter) {
        Ok(report) => {
            if compact {
                print_artifact_accountability_report_compact(&report);
            } else if target_id.is_some() {
                print_artifact_accountability_detail(&report, artifact_id.as_deref().unwrap_or(""));
            } else {
                print_artifact_accountability_report(&report, stale);
            }
            let has_stale_or_unaccounted = report.artifacts.iter().any(|a| {
                a.status == ArtifactAccountabilityStatus::Stale
                    || a.status == ArtifactAccountabilityStatus::Unaccounted
            });
            if has_stale_or_unaccounted {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("kat artifacts: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_artifact_accountability_report_compact(report: &ArtifactAccountabilityReport) {
    println!("STATUS       ARTIFACT");
    for item in &report.artifacts {
        let status_str = format_accountability_status(item.status);
        let title = item.title.as_deref().unwrap_or("none");
        println!("{status_str:<11}  {title}");
    }
}

fn print_artifact_accountability_detail(report: &ArtifactAccountabilityReport, query: &str) {
    if report.artifacts.is_empty() {
        println!("Artifact accountability detail for query '{query}'");
        println!("  no active artifact element matching query found or no accountability records.");
        return;
    }

    for a in &report.artifacts {
        println!("ARTIFACT ACCOUNTABILITY DETAIL");
        println!("  artifact_id: {}", a.artifact_element_id);
        println!("  type_id:     {}", a.artifact_type_id);
        if let Some(ref title) = a.title {
            println!("  title:       \"{title}\"");
        }
        println!("  status:      {}", format_accountability_status(a.status));
        println!();
        println!("ACCOUNTABILITY BASELINES ({})", a.baselines.len());
        if a.baselines.is_empty() {
            println!("  (no represents or derived-from relationships found)");
        } else {
            for b in &a.baselines {
                let stale_indicator = if b.is_stale { " [STALE]" } else { " [CURRENT]" };
                println!("  Relationship: {}", b.relationship_type);
                println!(
                    "    Upstream Target: {} ({})",
                    b.upstream_element_id, b.upstream_type_id
                );
                println!(
                    "    Recorded Baseline Version: {}{}",
                    short_object_id(&b.baseline_version),
                    stale_indicator
                );
                println!(
                    "    Current State Version:     {}",
                    short_object_id(&b.current_version)
                );
            }
        }
        println!();
        println!(
            "Note: KAT accountability evaluates semantic target-version alignment; physical file contents are not verified."
        );
    }
}

fn print_artifact_accountability_report(report: &ArtifactAccountabilityReport, stale_only: bool) {
    if stale_only {
        println!("STALE ARTIFACTS ({})", report.artifacts.len());
    } else {
        println!("Artifact accountability");
    }
    println!();

    if report.artifacts.is_empty() {
        if stale_only {
            println!("Artifacts (0)");
            println!("  no stale artifacts found");
        } else {
            println!("Artifacts (0)");
            println!("  no active artifacts found");
        }
        println!();
        println!("Repository summary");
        println!("  total:        {}", report.repository_summary.total);
        println!("  current:      {}", report.repository_summary.current);
        println!("  stale:        {}", report.repository_summary.stale);
        println!("  unaccounted:  {}", report.repository_summary.unaccounted);
        println!();
        println!(
            "Note: KAT accountability evaluates semantic target-version alignment; physical file contents are not verified."
        );
        return;
    }

    println!("Artifacts ({})", report.artifacts.len());
    for a in &report.artifacts {
        println!();
        print!("  Artifact {}", a.artifact_element_id);
        if let Some(ref title) = a.title {
            print!(" \"{title}\"");
        }
        println!();

        println!(
            "    status:      {}",
            format_accountability_status(a.status)
        );

        if a.baselines.is_empty() {
            println!("    baselines:   none");
        } else {
            println!("    baselines:");
            for b in &a.baselines {
                println!(
                    "      {} {} {} (version {})",
                    b.relationship_type,
                    b.upstream_type_id,
                    b.upstream_element_id,
                    short_object_id(&b.current_version)
                );
            }
        }
    }

    println!();
    println!("Repository summary");
    println!("  total:        {}", report.repository_summary.total);
    println!("  current:      {}", report.repository_summary.current);
    println!("  stale:        {}", report.repository_summary.stale);
    println!("  unaccounted:  {}", report.repository_summary.unaccounted);
    println!();
    println!(
        "Note: KAT accountability evaluates semantic target-version alignment; physical file contents are not verified."
    );
}

fn cmd_change(command: kat::cli::ChangeCommands) -> ExitCode {
    match command {
        kat::cli::ChangeCommands::Begin { description, json } => {
            cmd_change_begin(description, json)
        }
        kat::cli::ChangeCommands::Status { compact, json } => {
            cmd_change_status_porcelain(compact, json)
        }
        kat::cli::ChangeCommands::Commit { json } => run_commit(json),
        kat::cli::ChangeCommands::Abort { json } => run_abort(json),
    }
}

fn cmd_change_status_porcelain(compact: bool, json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            if json {
                MachinePresenter::present_error(None, "NotInRepository", &err.to_string());
            } else {
                eprintln!("kat change status: {err}");
            }
            return ExitCode::FAILURE;
        }
    };

    if json {
        let view = inspect_draft_session(&repository).ok().flatten();
        MachinePresenter::present_success(&repository, &view);
        ExitCode::SUCCESS
    } else {
        cmd_change_status(compact)
    }
}

fn run_context(
    roots: Vec<String>,
    direction: String,
    depth: Option<usize>,
    categorize: bool,
    compact: bool,
    json: bool,
) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            if json {
                MachinePresenter::present_error(None, "NotInRepository", &err.to_string());
            } else {
                eprintln!("kat context: {err}");
            }
            return ExitCode::FAILURE;
        }
    };

    let session = read_draft_session(repository.root_dir()).ok();

    let mut resolved_roots = Vec::new();
    for root_str in &roots {
        let resolved = if let Some(ref sess) = session {
            kat::repository::resolve::resolve_element_in_draft_session(sess, root_str)
        } else {
            kat::repository::resolve::resolve_element_id(&repository, root_str)
        };

        match resolved {
            Ok(id) => resolved_roots.push(id),
            Err(err) => {
                if json {
                    MachinePresenter::present_error(
                        Some(&repository),
                        "ResolveError",
                        &format!("failed to resolve root '{root_str}': {err}"),
                    );
                } else {
                    eprintln!("kat context: failed to resolve root '{root_str}': {err}");
                }
                return ExitCode::FAILURE;
            }
        }
    }

    let dir_enum = match direction.to_lowercase().as_str() {
        "downstream" => kat::repository::query::ContextDirection::Downstream,
        "both" => kat::repository::query::ContextDirection::Both,
        _ => kat::repository::query::ContextDirection::Upstream,
    };

    match kat::repository::query::retrieve_context(
        &repository,
        &resolved_roots,
        dir_enum,
        depth,
        categorize,
    ) {
        Ok(res) => {
            if json {
                MachinePresenter::present_success(&repository, &res);
            } else {
                kat::cli::context_presenter::present_human_context(&res, compact, depth);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if json {
                MachinePresenter::present_error(Some(&repository), "QueryError", &err.to_string());
            } else {
                eprintln!("kat context: {err}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run_author(claims_file: Option<String>, example: bool, json: bool) -> ExitCode {
    if example {
        if claims_file.is_some() {
            if json {
                MachinePresenter::present_error(
                    None,
                    "INVALID_ARGUMENTS",
                    "kat author: --example cannot be combined with CLAIMS_FILE",
                );
            } else {
                eprintln!("kat author: --example cannot be combined with CLAIMS_FILE");
            }
            return ExitCode::from(2);
        }
        if json {
            eprintln!("kat author: --example cannot be combined with --json");
            return ExitCode::from(2);
        }
        let template = serde_json::json!([
            {
                "kind": "create_element",
                "type_id": "kat.core/requirement",
                "title": "Auth Requirement",
                "description": "System shall support JWT auth",
                "handle": "@req-auth"
            },
            {
                "kind": "create_element",
                "type_id": "kat.core/implementation",
                "title": "Auth Module",
                "handle": "@imp-auth"
            },
            {
                "kind": "link_element",
                "relationship_type_id": "kat.core/realizes",
                "source_ref": "@imp-auth",
                "target_ref": "@req-auth"
            }
        ]);
        println!("{}", serde_json::to_string_pretty(&template).unwrap());
        return ExitCode::SUCCESS;
    }

    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            if json {
                MachinePresenter::present_error(None, "NOT_IN_REPOSITORY", &err.to_string());
            } else {
                eprintln!("kat author: {err}");
            }
            return ExitCode::FAILURE;
        }
    };

    let text = match &claims_file {
        Some(path) if path != "-" => match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) => {
                if json {
                    MachinePresenter::present_error(
                        Some(&repository),
                        "FILE_IO_ERROR",
                        &format!("failed to read claims file '{path}': {err}"),
                    );
                } else {
                    eprintln!("kat author: failed to read claims file '{path}': {err}");
                }
                return ExitCode::FAILURE;
            }
        },
        _ => {
            use std::io::Read;
            let mut stdin_buf = String::new();
            if let Err(err) = std::io::stdin().read_to_string(&mut stdin_buf) {
                if json {
                    MachinePresenter::present_error(
                        Some(&repository),
                        "FILE_IO_ERROR",
                        &format!("failed to read claims from stdin: {err}"),
                    );
                } else {
                    eprintln!("kat author: failed to read stdin: {err}");
                }
                return ExitCode::FAILURE;
            }
            stdin_buf
        }
    };

    let trimmed = text.trim();
    if trimmed.is_empty() {
        let res = kat::repository::author::AuthorBatchResult {
            claims_processed: 0,
            operations_staged: 0,
            workflow_references: std::collections::HashMap::new(),
        };
        if json {
            MachinePresenter::present_success(&repository, &res);
        } else {
            println!("Porcelain Authoring Compiler");
            println!("  claims_processed:  0");
            println!("  operations_staged: 0");
        }
        return ExitCode::SUCCESS;
    }

    let claims: Vec<kat::repository::author::AuthorClaim> =
        match kat::repository::author::parse_author_claims_json(trimmed) {
            Ok(json_claims) => json_claims,
            Err(err) => {
                if json {
                    MachinePresenter::present_error_with_details(
                        Some(&repository),
                        "AUTHOR_PARSE_ERROR",
                        &format!("failed to parse JSON authoring claims: {err}"),
                        serde_json::json!({
                            "line": err.line(),
                            "column": err.column(),
                            "reason": err.to_string()
                        }),
                    );
                } else {
                    eprintln!(
                        "kat author: JSON parse error at line {}, column {}: {}",
                        err.line(),
                        err.column(),
                        err
                    );
                }
                return ExitCode::FAILURE;
            }
        };

    match kat::repository::author::compile_and_stage_claims(&repository, &claims) {
        Ok(res) => {
            if json {
                MachinePresenter::present_success(&repository, &res);
            } else {
                println!("Porcelain Authoring Compiler");
                println!("  claims_processed:  {}", res.claims_processed);
                println!("  operations_staged: {}", res.operations_staged);
                if !res.workflow_references.is_empty() {
                    println!("  workflow_handles:  {}", res.workflow_references.len());
                    for (h, id) in &res.workflow_references {
                        println!("    {h:<16} -> {id}");
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if json {
                MachinePresenter::present_error(
                    Some(&repository),
                    "AUTHOR_COMPILATION_FAILED",
                    &err.to_string(),
                );
            } else {
                eprintln!("kat author: {err}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run_check(compact: bool, json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            if json {
                MachinePresenter::present_error(None, "NotInRepository", &err.to_string());
            } else {
                eprintln!("kat check: {err}");
            }
            return ExitCode::FAILURE;
        }
    };

    match kat::repository::validation::graph_quality::run_check(&repository) {
        Ok(report) => {
            if json {
                MachinePresenter::present_success(&repository, &report);
            } else if compact {
                println!(
                    "check status: clean={} / violations={} / gq_findings={}",
                    report.repository_clean,
                    report.mechanical_validation.violations.len(),
                    report.graph_quality.total_findings
                );
            } else {
                println!("KAT Repository Check");
                println!("  clean:                 {}", report.repository_clean);
                println!(
                    "  mechanical_violations: {}",
                    report.mechanical_validation.violations.len()
                );
                println!(
                    "  graph_quality_findings:{}",
                    report.graph_quality.total_findings
                );
                if !report.graph_quality.findings.is_empty() {
                    println!();
                    println!("GRAPH QUALITY ADVISORY DIAGNOSTICS");
                    for f in &report.graph_quality.findings {
                        println!("  - [{}] {}", f.rule_id, f.message);
                    }
                }
            }

            if report.repository_clean {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(err) => {
            if json {
                MachinePresenter::present_error(Some(&repository), "CheckError", &err.to_string());
            } else {
                eprintln!("kat check: {err}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run_commit(json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            if json {
                MachinePresenter::present_error(None, "NotInRepository", &err.to_string());
            } else {
                eprintln!("kat commit: {err}");
            }
            return ExitCode::FAILURE;
        }
    };

    match commit_draft_session(&repository) {
        Ok(published) => {
            let prepared = &published.persisted.prepared;
            if json {
                let commit_dto = serde_json::json!({
                    "change_id": prepared.change.change_id,
                    "change_revision_id": prepared.change_revision_id,
                    "state_id": prepared.state_id,
                    "operations_count": prepared.change.operations.len(),
                });
                MachinePresenter::present_success(&repository, &commit_dto);
            } else {
                println!("committed change transaction");
                println!("  change_id:          {}", prepared.change.change_id);
                println!("  change_revision_id: {}", prepared.change_revision_id);
                println!("  state_id:           {}", prepared.state_id);
                println!("  operations:         {}", prepared.change.operations.len());
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if json {
                let code = match err {
                    ChangeError::Conflict => "CasConflict",
                    _ => "CommitFailed",
                };
                MachinePresenter::present_error(Some(&repository), code, &err.to_string());
            } else {
                eprintln!("kat commit: {err}");
            }
            if matches!(err, ChangeError::Conflict) {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

fn run_abort(json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            if json {
                MachinePresenter::present_error(None, "NotInRepository", &err.to_string());
            } else {
                eprintln!("kat abort: {err}");
            }
            return ExitCode::FAILURE;
        }
    };

    match abort_draft_session(repository.root_dir()) {
        Ok(()) => {
            if json {
                let data = serde_json::json!({ "aborted": true });
                MachinePresenter::present_success(&repository, &data);
            } else {
                println!("aborted draft change transaction");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if json {
                MachinePresenter::present_error(Some(&repository), "AbortFailed", &err.to_string());
            } else {
                eprintln!("kat abort: {err}");
            }
            ExitCode::FAILURE
        }
    }
}

fn cmd_change_begin(description: Option<String>, json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            if json {
                MachinePresenter::present_error(None, "NotInRepository", &err.to_string());
            } else {
                eprintln!("kat change begin: {err}");
            }
            return ExitCode::FAILURE;
        }
    };

    match begin_draft_session(&repository, description) {
        Ok(session) => {
            if json {
                let session_dto = serde_json::json!({
                    "status": format!("{:?}", session.status),
                    "base_state_id": session.base_state_id,
                    "created_at": session.created_at,
                    "description": session.description,
                });
                MachinePresenter::present_success(&repository, &session_dto);
            } else {
                println!("opened draft change transaction");
                println!("  base_state: {}", short_object_id(&session.base_state_id));
                println!("  created_at: {}", session.created_at);
                if let Some(desc) = &session.description {
                    println!("  description: {}", desc);
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if json {
                MachinePresenter::present_error(Some(&repository), "BeginFailed", &err.to_string());
            } else {
                eprintln!("kat change begin: {err}");
            }
            ExitCode::FAILURE
        }
    }
}

fn cmd_change_status(compact: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            eprintln!("kat change status: {err}");
            return ExitCode::FAILURE;
        }
    };

    let view = match inspect_draft_session(&repository) {
        Ok(Some(v)) => v,
        Ok(None) => {
            if compact {
                println!("draft status: none");
            } else {
                println!("no open draft change transaction found at .kat/work/change/session.json");
            }
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("kat change status: {err}");
            return ExitCode::FAILURE;
        }
    };

    if compact {
        println!(
            "draft status: {} / base_state: {} / operations: {}",
            view.status,
            short_object_id(&view.base_state_id),
            view.staged_operations.len()
        );
        return ExitCode::SUCCESS;
    }

    println!("Draft Change Transaction");
    println!("  status:       {}", view.status);
    println!("  base_state:   {}", short_object_id(&view.base_state_id));
    if let Some(ref c) = view.base_change_id {
        println!("  base_change:  {}", short_object_id(c));
    }
    println!("  created_at:   {}", view.created_at);
    if let Some(ref desc) = view.description {
        println!("  description:  {desc}");
    }
    println!("  operations:   {}", view.staged_operations.len());

    println!();
    println!("STAGED OPERATIONS ({})", view.staged_operations.len());
    if view.staged_operations.is_empty() {
        println!("  (none)");
    } else {
        for op in &view.staged_operations {
            println!("  {}. {:<24} {}", op.index, op.operation_kind, op.summary);
        }
    }

    println!();
    println!("CANDIDATE EFFECT");
    println!("  elements:      {}", view.candidate_effect.total_elements);
    println!(
        "                 (+{} created, {} updated, {} deprecated, {} superseded)",
        view.candidate_effect.elements_created,
        view.candidate_effect.elements_updated,
        view.candidate_effect.elements_deprecated,
        view.candidate_effect.elements_superseded
    );
    println!(
        "  relationships: {}",
        view.candidate_effect.total_relationships
    );
    println!(
        "                 (+{} created, {} unlinked)",
        view.candidate_effect.relationships_created, view.candidate_effect.relationships_unlinked
    );

    println!();
    println!("ARTIFACT ACCOUNTABILITY PREVIEW");
    println!("  total:      {}", view.accountability_total_artifacts);
    println!(
        "  stale:      {} (artifacts that would be classified STALE if current candidate state were accepted)",
        view.accountability_stale_artifacts
    );
    println!(
        "  reconciled: {} in draft",
        view.accountability_reconciled_in_draft
    );

    println!();
    println!("CANDIDATE VALIDATION");
    if view.candidate_validation.violations.is_empty() {
        println!(
            "  status: Valid (0 violations, {} unverified constraints)",
            view.candidate_validation.constraint_details.len()
        );
    } else {
        println!(
            "  status: Invalid ({} mechanical violations)",
            view.candidate_validation.violations.len()
        );
        for v in &view.candidate_validation.violations {
            println!("  - [{:?}] {}", v.kind, v.message);
        }
    }

    ExitCode::SUCCESS
}

fn cmd_ontology(compact: bool, command: Option<OntologyCommands>) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repo) => repo,
        Err(err) => {
            eprintln!("kat ontology: {err}");
            return ExitCode::FAILURE;
        }
    };

    match command {
        None => match inspect_ontology(&repository) {
            Ok(summary) => {
                if compact {
                    print_ontology_summary_compact(&summary);
                } else {
                    print_ontology_summary(&summary);
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("kat ontology: {err}");
                ExitCode::FAILURE
            }
        },
        Some(OntologyCommands::Show { type_id }) => {
            match show_ontology_type(&repository, &type_id) {
                Ok(view) => {
                    if compact {
                        print_ontology_type_view_compact(&view);
                    } else {
                        print_ontology_type_view(&view);
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("kat ontology show: {err}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn print_ontology_summary(summary: &OntologySummary) {
    println!("ONTOLOGY");
    println!("  id:       {}", summary.ontology_id);
    println!("  version:  {}", summary.ontology_version_id);
    println!();

    println!("ELEMENT TYPES ({})", summary.element_types.len());
    let max_elem_type_len = summary
        .element_types
        .iter()
        .map(|e| e.type_id.len())
        .max()
        .unwrap_or(4)
        .max(4);
    println!("  {:width$}  NAME", "TYPE", width = max_elem_type_len);
    for elem in &summary.element_types {
        println!(
            "  {:width$}  {}",
            elem.type_id,
            elem.name,
            width = max_elem_type_len
        );
    }
    println!();

    println!("RELATIONSHIP TYPES ({})", summary.relationship_types.len());
    let max_rel_type_len = summary
        .relationship_types
        .iter()
        .map(|r| r.type_id.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let max_rel_name_len = summary
        .relationship_types
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let max_sources_len = summary
        .relationship_types
        .iter()
        .map(|r| r.allowed_source_types.join(", ").len())
        .max()
        .unwrap_or(7)
        .max(7);

    println!(
        "  {:t_width$}  {:n_width$}  {:s_width$}  TARGETS",
        "TYPE",
        "NAME",
        "SOURCES",
        t_width = max_rel_type_len,
        n_width = max_rel_name_len,
        s_width = max_sources_len
    );
    for rel in &summary.relationship_types {
        let sources_str = rel.allowed_source_types.join(", ");
        let targets_str = rel.allowed_target_types.join(", ");
        println!(
            "  {:t_width$}  {:n_width$}  {:s_width$}  {}",
            rel.type_id,
            rel.name,
            sources_str,
            targets_str,
            t_width = max_rel_type_len,
            n_width = max_rel_name_len,
            s_width = max_sources_len
        );
    }
}

fn print_ontology_summary_compact(summary: &OntologySummary) {
    println!("ELEMENT TYPES");
    for elem in &summary.element_types {
        let short = elem.type_id.rsplit('/').next().unwrap_or(&elem.type_id);
        println!("  {short}");
    }
    println!();

    println!("RELATIONSHIP TYPES");
    let max_rel_short_len = summary
        .relationship_types
        .iter()
        .map(|r| r.type_id.rsplit('/').next().unwrap_or(&r.type_id).len())
        .max()
        .unwrap_or(4)
        .max(4);
    let max_sources_short_len = summary
        .relationship_types
        .iter()
        .map(|r| {
            r.allowed_source_types
                .iter()
                .map(|s| s.rsplit('/').next().unwrap_or(s))
                .collect::<Vec<_>>()
                .join(", ")
                .len()
        })
        .max()
        .unwrap_or(7)
        .max(7);

    println!(
        "  {:t_width$}  {:s_width$}  TARGETS",
        "TYPE",
        "SOURCES",
        t_width = max_rel_short_len,
        s_width = max_sources_short_len
    );
    for rel in &summary.relationship_types {
        let rel_short = rel.type_id.rsplit('/').next().unwrap_or(&rel.type_id);
        let sources_short = rel
            .allowed_source_types
            .iter()
            .map(|s| s.rsplit('/').next().unwrap_or(s))
            .collect::<Vec<_>>()
            .join(", ");
        let targets_short = rel
            .allowed_target_types
            .iter()
            .map(|t| t.rsplit('/').next().unwrap_or(t))
            .collect::<Vec<_>>()
            .join(", ");

        println!(
            "  {:t_width$}  {:s_width$}  {}",
            rel_short,
            sources_short,
            targets_short,
            t_width = max_rel_short_len,
            s_width = max_sources_short_len
        );
    }
}

fn print_ontology_type_view(view: &OntologyTypeView) {
    match view {
        OntologyTypeView::Element(elem) => {
            println!("{}", elem.type_id);
            println!();
            println!("Kind:");
            println!("  element");
            println!();
            println!("Name:");
            println!("  {}", elem.name);
            println!();
            println!("Outgoing relationships:");
            if elem.outgoing.is_empty() {
                println!("  none");
            } else {
                let max_rel_id_len = elem
                    .outgoing
                    .iter()
                    .map(|cap| cap.relationship_type_id.len())
                    .max()
                    .unwrap_or(0);
                for cap in &elem.outgoing {
                    println!(
                        "  {:width$}   -> {}",
                        cap.relationship_type_id,
                        cap.counterpart_type_id,
                        width = max_rel_id_len
                    );
                }
            }
            println!();
            println!("Incoming relationships:");
            if elem.incoming.is_empty() {
                println!("  none");
            } else {
                let max_rel_id_len = elem
                    .incoming
                    .iter()
                    .map(|cap| cap.relationship_type_id.len())
                    .max()
                    .unwrap_or(0);
                for cap in &elem.incoming {
                    println!(
                        "  {:width$}   <- {}",
                        cap.relationship_type_id,
                        cap.counterpart_type_id,
                        width = max_rel_id_len
                    );
                }
            }
        }
        OntologyTypeView::Relationship(rel) => {
            println!("{}", rel.type_id);
            println!();
            println!("Kind:");
            println!("  relationship");
            println!();
            println!("Name:");
            println!("  {}", rel.name);
            println!();
            println!("Sources:");
            for src in &rel.allowed_source_types {
                println!("  {src}");
            }
            println!();
            println!("Targets:");
            for tgt in &rel.allowed_target_types {
                println!("  {tgt}");
            }
        }
    }
}

fn print_ontology_type_view_compact(view: &OntologyTypeView) {
    fn short(s: &str) -> &str {
        s.rsplit('/').next().unwrap_or(s)
    }

    match view {
        OntologyTypeView::Element(elem) => {
            println!("{}", short(&elem.type_id));
            println!("kind: element");
            println!();
            println!("outgoing:");
            if elem.outgoing.is_empty() {
                println!("  none");
            } else {
                for cap in &elem.outgoing {
                    println!(
                        "  {} -> {}",
                        short(&cap.relationship_type_id),
                        short(&cap.counterpart_type_id)
                    );
                }
            }
            println!();
            println!("incoming:");
            if elem.incoming.is_empty() {
                println!("  none");
            } else {
                for cap in &elem.incoming {
                    println!(
                        "  {} <- {}",
                        short(&cap.relationship_type_id),
                        short(&cap.counterpart_type_id)
                    );
                }
            }
        }
        OntologyTypeView::Relationship(rel) => {
            println!("{}", short(&rel.type_id));
            println!("kind: relationship");
            println!();
            println!("sources:");
            for src in &rel.allowed_source_types {
                println!("  {}", short(src));
            }
            println!();
            println!("targets:");
            for tgt in &rel.allowed_target_types {
                println!("  {}", short(tgt));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Proves the binary target's test harness runs.
    #[test]
    fn harness_works() {}
}
