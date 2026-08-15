//! KAT - semantic software repository.
//!
//! Thin command-line binary over the `kat` library crate. This implements the
//! invocation contract in `docs/cli.md`; the CLI layer only parses and
//! dispatches, it never owns repository semantics.

use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

use kat::domain::identity::{ChangeId, ElementId, ObjectId, RelationshipId};
use kat::domain::ontology::OntologyVersion;
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
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
use kat::repository::open::{Repository, open_repository};
use kat::repository::query::{
    ArtifactAccountabilityReport, ArtifactAccountabilityStatus, HistoryEntry, ImpactResult,
    QueryError, RepositoryStatus, TraceResult, TraversalDirection, analyze_artifact_accountability,
    analyze_impact, history, repository_status, show_element, trace_origin,
};
use kat::repository::validation::repository::{ValidationReport, validate_repository};

pub mod cli;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => cmd_init(),
        Command::Status => run_status(),
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
        Command::Show { element_id } => run_show(element_id),
        Command::History => cmd_history(),
        Command::Trace { element_id } => run_trace(element_id),
        Command::Impact { element_id } => run_impact(element_id),
        Command::Validate => cmd_validate(),
        Command::Artifacts => cmd_artifacts(),
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

fn run_status() -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat status: {error}");
            return ExitCode::FAILURE;
        }
    };

    match repository_status(&repository) {
        Ok(status) => {
            print_repository_status(&status);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("kat status: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_repository_status(status: &RepositoryStatus) {
    let abbreviate = |id: &ObjectId| -> String {
        let s = id.to_string();
        if s.len() >= 12 {
            s[..12].to_string()
        } else {
            s
        }
    };

    println!("KAT repository");
    println!();
    println!("Repository");
    println!("  repository:  {}", status.repository_id);
    println!("  software:    {}", status.software_id);
    println!("  state:       {}", abbreviate(&status.state_id));

    if let Some(change_id) = status.change_id {
        println!("  change:      {}", abbreviate(&change_id));
    } else {
        println!("  change:      none");
    }
    println!("  ontology:    {}", abbreviate(&status.ontology_id));

    if let Some(ref latest) = status.latest_change {
        println!();
        println!("Latest change");
        println!("  revision:    {}", abbreviate(&latest.revision_id));
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
    let element_id = match ElementId::from_str(&element_id_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat update: invalid element ID: {element_id_str}");
            return ExitCode::FAILURE;
        }
    };

    if title.is_none() && description.is_none() {
        eprintln!(
            "kat update: at least one property flag (--title, --description) must be supplied"
        );
        return ExitCode::FAILURE;
    }

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
    let element_id = match ElementId::from_str(&element_id_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat deprecate: invalid element ID: {element_id_str}");
            return ExitCode::FAILURE;
        }
    };

    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat deprecate: {error}");
            return ExitCode::FAILURE;
        }
    };

    let context = match prepare_change(&repository) {
        Ok(context) => context,
        Err(error) => return fail_deprecate(error),
    };

    let parsed = DeprecateArgs { element_id };

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
    let existing_element_id = match ElementId::from_str(&existing_id_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat supersede: invalid element ID: {existing_id_str}");
            return ExitCode::FAILURE;
        }
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
    let source_element_id = match ElementId::from_str(&source_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat link: invalid source element ID: {source_str}");
            return ExitCode::FAILURE;
        }
    };

    let target_element_id = match ElementId::from_str(&target_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat link: invalid target element ID: {target_str}");
            return ExitCode::FAILURE;
        }
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
    let relationship_id = match RelationshipId::from_str(&relationship_id_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat unlink: invalid relationship ID: {relationship_id_str}");
            return ExitCode::FAILURE;
        }
    };

    let parsed = UnlinkArgs {
        relationship_id,
        description,
    };

    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat unlink: {error}");
            return ExitCode::FAILURE;
        }
    };

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

fn run_show(element_id_str: String) -> ExitCode {
    let element_id = match ElementId::from_str(&element_id_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat show: invalid element ID: {element_id_str}");
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

fn cmd_history() -> ExitCode {
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

fn run_trace(element_id_str: String) -> ExitCode {
    let element_id = match ElementId::from_str(&element_id_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat trace: invalid element ID: {element_id_str}");
            return ExitCode::FAILURE;
        }
    };
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat trace: {error}");
            return ExitCode::FAILURE;
        }
    };

    match trace_origin(&repository, element_id) {
        Ok(result) => {
            print_trace_result(&repository, &result);
            ExitCode::SUCCESS
        }
        Err(QueryError::ElementNotFound(id)) => {
            eprintln!("kat trace: element {id} not found in the accepted state");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("kat trace: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_trace_result(repository: &Repository, result: &TraceResult) {
    println!("element_id: {}", result.root_element_id);
    if let Ok(view) = show_element(repository, result.root_element_id) {
        println!("type: {}", view.element.type_id);
        println!("lifecycle: {}", view.element.lifecycle);
        if let Some((_, PropertyValue::Text(title))) =
            view.element.properties.iter().find(|(k, _)| k == "title")
        {
            println!("title: {title}");
        }
    }

    if result.paths.is_empty() {
        println!("origin: none");
        return;
    }

    println!("origin_paths:");
    for (path_idx, path) in result.paths.iter().enumerate() {
        println!("  path {}:", path_idx + 1);
        for step in &path.steps {
            let dir_label = match step.direction {
                TraversalDirection::Forward => "forward",
                TraversalDirection::Backward => "backward",
            };
            print!(
                "    - via {} ({dir_label}) -> {}",
                step.relationship_type_id, step.to_element_id
            );
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

fn run_impact(element_id_str: String) -> ExitCode {
    let element_id = match ElementId::from_str(&element_id_str) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("kat impact: invalid element ID: {element_id_str}");
            return ExitCode::FAILURE;
        }
    };
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat impact: {error}");
            return ExitCode::FAILURE;
        }
    };

    match analyze_impact(&repository, element_id) {
        Ok(result) => {
            print_impact_result(&repository, &result);
            ExitCode::SUCCESS
        }
        Err(QueryError::ElementNotFound(id)) => {
            eprintln!("kat impact: element {id} not found in the accepted state");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("kat impact: {error}");
            ExitCode::FAILURE
        }
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
    println!("directly_changed:");
    for id in &result.directly_changed {
        print!("  {id}");
        if let Ok(view) = show_element(repository, *id) {
            print!(" [{}]", view.element.type_id);
            print_title_property(&view.element.properties);
        }
        println!();
    }

    println!();
    println!("semantically_affected:");
    if result.semantically_affected.is_empty() {
        println!("  none");
    } else {
        for elem in &result.semantically_affected {
            print!("  {}", elem.element_id);
            print!(" [{}]", elem.type_id);
            if let Ok(view) = show_element(repository, elem.element_id) {
                print_title_property(&view.element.properties);
            }
            println!();
            for path in &elem.paths {
                for step in &path.steps {
                    let dir_label = match step.direction {
                        TraversalDirection::Forward => "forward",
                        TraversalDirection::Backward => "backward",
                    };
                    println!(
                        "    via {} ({dir_label}) -> {}",
                        step.relationship_type_id, step.from_element_id
                    );
                }
            }
        }
    }

    println!();
    println!("affected_artifacts:");
    if result.affected_artifacts.is_empty() {
        println!("  none");
    } else {
        for elem in &result.affected_artifacts {
            print!("  {}", elem.element_id);
            print!(" [{}]", elem.type_id);
            if let Ok(view) = show_element(repository, elem.element_id) {
                print_title_property(&view.element.properties);
            }
            println!();
            for path in &elem.paths {
                for step in &path.steps {
                    let dir_label = match step.direction {
                        TraversalDirection::Forward => "forward",
                        TraversalDirection::Backward => "backward",
                    };
                    println!(
                        "    via {} ({dir_label}) -> {}",
                        step.relationship_type_id, step.from_element_id
                    );
                }
            }
        }
    }
}

fn print_title_property(properties: &[(String, PropertyValue)]) {
    if let Some((_, PropertyValue::Text(title))) = properties.iter().find(|(k, _)| k == "title") {
        print!(" \"{title}\"");
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

/// `kat validate` — evaluate current repository state against ontology rules,
/// state invariants, and relationship constraints (read-only; thin dispatch over [`validate_repository`]).
fn cmd_validate() -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat validate: {error}");
            return ExitCode::FAILURE;
        }
    };

    match validate_repository(&repository) {
        Ok(report) => {
            print_validation_report(&repository, &report);
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

fn print_validation_report(repository: &Repository, report: &ValidationReport) {
    if !report.violations.is_empty() {
        println!("violations:");
        for v in &report.violations {
            print!("  - [{:?}] {}", v.kind, v.message);
            if let Some(rel_id) = v.relationship_id {
                print!(" (relationship: {rel_id})");
            }
            println!();
        }
        println!();
    }

    if !report.unverified_constraints.is_empty() {
        println!("unverified_constraints:");
        for c in &report.unverified_constraints {
            print!("  {}", c.constraint_element_id);
            if let Some(ref title) = c.title {
                print!(" \"{title}\"");
            }
            println!(" [reason: no executable validation rule]");
            if c.constrained_element_ids.is_empty() {
                println!("    constrained_elements: none");
            } else {
                println!("    constrained_elements:");
                for target_id in &c.constrained_element_ids {
                    print!("      {target_id}");
                    if let Ok(view) = show_element(repository, *target_id) {
                        print!(" [{}]", view.element.type_id);
                        print_title_property(&view.element.properties);
                    }
                    println!();
                }
            }
        }
        println!();
    }

    if report.violations.is_empty() {
        println!("semantic consistency: no violations detected");
        if !report.unverified_constraints.is_empty() {
            println!(
                "unverified constraints: {}",
                report.unverified_constraints.len()
            );
        }
    } else {
        println!(
            "semantic consistency: {} violation(s) detected",
            report.violations.len()
        );
        if !report.unverified_constraints.is_empty() {
            println!(
                "unverified constraints: {}",
                report.unverified_constraints.len()
            );
        }
    }
}

/// `kat artifacts` — evaluate artifact accountability across all active `kat.core/artifact` elements
/// against current accepted state (read-only; thin dispatch over [`analyze_artifact_accountability`]).
fn cmd_artifacts() -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("kat artifacts: {error}");
            return ExitCode::FAILURE;
        }
    };

    match analyze_artifact_accountability(&repository) {
        Ok(report) => {
            print_artifact_accountability_report(&report);
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

fn print_artifact_accountability_report(report: &ArtifactAccountabilityReport) {
    println!("artifact accountability:");
    println!();

    if report.artifacts.is_empty() {
        println!("  no active artifacts found");
        println!();
        println!("summary:");
        println!("  current: 0");
        println!("  stale: 0");
        println!("  unaccounted: 0");
        return;
    }

    let mut current_count = 0;
    let mut stale_count = 0;
    let mut unaccounted_count = 0;

    for a in &report.artifacts {
        match a.status {
            ArtifactAccountabilityStatus::Current => current_count += 1,
            ArtifactAccountabilityStatus::Stale => stale_count += 1,
            ArtifactAccountabilityStatus::Unaccounted => unaccounted_count += 1,
        }

        print!("  {}", a.artifact_element_id);
        if let Some(ref title) = a.title {
            print!(" \"{title}\"");
        }
        println!();

        let status_str = match a.status {
            ArtifactAccountabilityStatus::Current => "current",
            ArtifactAccountabilityStatus::Stale => "stale",
            ArtifactAccountabilityStatus::Unaccounted => "unaccounted",
        };
        println!("    status: {status_str}");

        if a.baselines.is_empty() {
            println!("    accountability relationships: none");
        } else {
            println!("    accountability relationships:");
            for b in &a.baselines {
                let status_flag = if b.is_stale { "STALE" } else { "CURRENT" };
                println!(
                    "      - {} -> {} [{}] status: {}",
                    b.relationship_type, b.upstream_element_id, b.upstream_type_id, status_flag
                );
                println!("        baseline version: {}", b.baseline_version);
                println!("        current version:  {}", b.current_version);
            }
        }
        println!();
    }

    println!("summary:");
    println!("  current: {current_count}");
    println!("  stale: {stale_count}");
    println!("  unaccounted: {unaccounted_count}");
}

#[cfg(test)]
mod tests {
    /// Proves the binary target's test harness runs.
    #[test]
    fn harness_works() {}
}
