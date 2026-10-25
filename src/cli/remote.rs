//! User-facing remote configuration and fetch/push/sync workflows.

use std::collections::HashSet;
use std::path::Path;
use std::process::ExitCode;

use crate::cli::machine_presenter::MachinePresenter;
use crate::domain::identity::RepositoryRevisionId;
use crate::domain::workspace::WorkspaceBackend;
use crate::repository::http::HttpSemanticTransport;
use crate::repository::open::{Repository, open_repository};
use crate::repository::remote::{
    FetchOutcome, RemoteCoordinator, RemoteError, RemoteTransportError, SemanticTransport,
};
use crate::repository::remote_config::{RemoteConfig, RemoteConfigError};
use crate::repository::topology::{DivergenceState, compare_ancestry};
use crate::repository::workspace::git::GitWorkspaceBackend;
use crate::repository::workspace::git_transport::{
    GitWorkspaceBinding, GitWorkspaceTransport, WorkspaceBackendId,
};
use crate::repository::workspace::{WorkspaceError, open_workspace};

const TOKEN_ENV: &str = "KAT_REMOTE_TOKEN";

pub fn run_remote_add(
    name: String,
    hub_url: String,
    git_url: String,
    backend_id: String,
    json: bool,
) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => return failure(None, json, "remote add", "NOT_IN_REPOSITORY", error),
    };
    let backend_id = match backend_id.parse::<uuid::Uuid>() {
        Ok(id) => WorkspaceBackendId::from_uuid(id),
        Err(_) => {
            return failure(
                Some(&repository),
                json,
                "remote add",
                "INVALID_BACKEND_ID",
                "WorkspaceBackendId must be a UUID",
            );
        }
    };
    let binding = GitWorkspaceBinding {
        repository_id: repository.metadata.repository_id,
        workspace_backend_id: backend_id,
    };
    if let Err(error) = RemoteConfig::validate(&name, &hub_url, &git_url) {
        return failure(
            Some(&repository),
            json,
            "remote add",
            remote_config_code(&error),
            error,
        );
    }
    if let Err(error) =
        GitWorkspaceTransport::configure(repository.root_dir(), git_url.clone(), binding)
    {
        return failure(
            Some(&repository),
            json,
            "remote add",
            "REMOTE_CONFIGURATION_FAILED",
            error,
        );
    }
    let remote = match RemoteConfig::add(
        repository.root_dir(),
        &name,
        binding.repository_id,
        binding.workspace_backend_id,
        hub_url,
        git_url,
    ) {
        Ok(remote) => remote,
        Err(error) => {
            return failure(
                Some(&repository),
                json,
                "remote add",
                remote_config_code(&error),
                error,
            );
        }
    };
    let data = remote_value(&remote);
    if json {
        MachinePresenter::present_success(&repository, &data);
    } else {
        println!("Added remote {}", remote.name());
        println!("  Hub:       {}", remote.hub_url);
        println!("  workspace: {}", remote.git_url);
        println!("  backend:   {}", remote.workspace_backend_id);
    }
    ExitCode::SUCCESS
}

pub fn run_remote_list(json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => return failure(None, json, "remote list", "NOT_IN_REPOSITORY", error),
    };
    let remotes = match RemoteConfig::list(repository.root_dir(), repository.metadata.repository_id)
    {
        Ok(remotes) => remotes,
        Err(error) => {
            return failure(
                Some(&repository),
                json,
                "remote list",
                remote_config_code(&error),
                error,
            );
        }
    };
    if json {
        MachinePresenter::present_success(
            &repository,
            &remotes.iter().map(remote_value).collect::<Vec<_>>(),
        );
    } else if remotes.is_empty() {
        println!("No remotes configured.");
    } else {
        for remote in remotes {
            println!("{}", remote.name());
            println!("  Hub:       {}", remote.hub_url);
            println!("  workspace: {}", remote.git_url);
            println!("  backend:   {}", remote.workspace_backend_id);
        }
    }
    ExitCode::SUCCESS
}

pub fn run_fetch(name: String, json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => return failure(None, json, "fetch", "NOT_IN_REPOSITORY", error),
    };
    let (config, semantic, workspace) = match open_transports(&repository, &name) {
        Ok(clients) => clients,
        Err((code, message)) => {
            return failure(Some(&repository), json, "fetch", code, message);
        }
    };
    let coordinator = RemoteCoordinator::new(
        repository.metadata.repository_id,
        config.name(),
        repository.object_store(),
        repository.ref_store(),
        &semantic,
        &workspace,
    );
    match coordinator.fetch() {
        Ok(outcome) => present_fetch(&repository, &name, &outcome, json),
        Err(error) => remote_failure(&repository, &name, "fetch", error, json),
    }
}

pub fn run_push(name: String, reference: String, json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => return failure(None, json, "push", "NOT_IN_REPOSITORY", error),
    };
    let (config, semantic, workspace_transport) = match open_transports(&repository, &name) {
        Ok(clients) => clients,
        Err((code, message)) => {
            return failure(Some(&repository), json, "push", code, message);
        }
    };
    let workspace = match current_clean_workspace(&repository) {
        Ok(workspace) => workspace,
        Err((code, message)) => {
            return failure(Some(&repository), json, "push", code, message);
        }
    };
    if let Err(error) = repository
        .ref_store()
        .validate_ref_name(&format!("local/{reference}"))
    {
        return failure(Some(&repository), json, "push", "INVALID_REFERENCE", error);
    }
    let remote_refs = match semantic.list_references(repository.metadata.repository_id) {
        Ok(remote_refs) => remote_refs,
        Err(error) => {
            return transport_failure(&repository, &name, "push", error, json);
        }
    };
    let mut targets = remote_refs
        .named
        .iter()
        .filter(|remote_ref| remote_ref.name == reference)
        .map(|remote_ref| remote_ref.target);
    let expected = targets.next();
    if targets.next().is_some() {
        return failure(
            Some(&repository),
            json,
            "push",
            "REMOTE_REFERENCE_INVALID",
            "remote returned a duplicate named reference",
        );
    }
    let candidate = workspace.base_revision;
    if let Some(expected) = expected {
        match revision_has_ancestor(&repository, candidate, expected) {
            Ok(true) => {}
            Ok(false) => {
                return failure(
                    Some(&repository),
                    json,
                    "push",
                    "PUSH_NON_FAST_FORWARD",
                    format!(
                        "remote reference {reference} at {expected} is not an ancestor of local revision {candidate}"
                    ),
                );
            }
            Err(error) => {
                return failure(
                    Some(&repository),
                    json,
                    "push",
                    "PUSH_HISTORY_UNAVAILABLE",
                    error,
                );
            }
        }
    }
    let coordinator = RemoteCoordinator::new(
        repository.metadata.repository_id,
        config.name(),
        repository.object_store(),
        repository.ref_store(),
        &semantic,
        &workspace_transport,
    );
    match coordinator.publish(&reference, expected, candidate) {
        Ok(()) => {
            let data = serde_json::json!({
                "remote": name,
                "reference": reference,
                "expected_remote_target": expected,
                "published_target": candidate,
            });
            if json {
                MachinePresenter::present_success(&repository, &data);
            } else {
                println!(
                    "Published {} to {}/{} (expected {}).",
                    candidate,
                    name,
                    reference,
                    expected.map_or_else(|| "absent".to_owned(), |target| target.to_string())
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => remote_failure(&repository, &name, "push", error, json),
    }
}

fn revision_has_ancestor(
    repository: &Repository,
    descendant: RepositoryRevisionId,
    ancestor: RepositoryRevisionId,
) -> Result<bool, String> {
    let mut pending = vec![descendant];
    let mut visited = HashSet::new();
    while let Some(current) = pending.pop() {
        if current == ancestor {
            return Ok(true);
        }
        if visited.insert(current) {
            pending.extend(
                repository
                    .read_revision(current)
                    .map_err(|error| error.to_string())?
                    .parents,
            );
        }
    }
    Ok(false)
}

pub fn run_sync(name: String, json: bool) -> ExitCode {
    let repository = match open_repository(Path::new(".")) {
        Ok(repository) => repository,
        Err(error) => return failure(None, json, "sync", "NOT_IN_REPOSITORY", error),
    };
    let workspace_state = match current_workspace(&repository) {
        Ok(workspace) => workspace,
        Err((code, message)) => return failure(Some(&repository), json, "sync", code, message),
    };
    let (config, semantic, workspace_transport) = match open_transports(&repository, &name) {
        Ok(clients) => clients,
        Err((code, message)) => {
            return failure(Some(&repository), json, "sync", code, message);
        }
    };
    let coordinator = RemoteCoordinator::new(
        repository.metadata.repository_id,
        config.name(),
        repository.object_store(),
        repository.ref_store(),
        &semantic,
        &workspace_transport,
    );
    let fetch = match coordinator.fetch() {
        Ok(outcome) => outcome,
        Err(error) => return remote_failure(&repository, &name, "sync", error, json),
    };
    let mut topology = Vec::new();
    let remote_refs = match repository.ref_store().list_remote_refs() {
        Ok(remote_refs) => remote_refs,
        Err(error) => {
            return failure(
                Some(&repository),
                json,
                "sync",
                "TRACKING_REFERENCE_ERROR",
                error,
            );
        }
    };
    for remote_ref in remote_refs.iter().filter(|item| item.remote == name) {
        match compare_ancestry(
            repository.object_store(),
            workspace_state.base_revision,
            remote_ref.target,
        ) {
            Ok(state) => topology.push(topology_value(
                &remote_ref.name,
                workspace_state.base_revision,
                remote_ref.target,
                state,
            )),
            Err(error) => {
                let details = serde_json::json!({
                    "fetch": fetch_value(&name, &fetch),
                    "reference": remote_ref.name,
                    "error": error.to_string(),
                });
                return failure_with_details(
                    Some(&repository),
                    json,
                    "sync",
                    "TOPOLOGY_CHECK_FAILED",
                    error,
                    details,
                );
            }
        }
    }
    let data = serde_json::json!({
        "remote": name,
        "local_revision": workspace_state.base_revision,
        "fetch": fetch_value(&name, &fetch),
        "topology": topology,
    });
    if fetch.failed.is_empty() {
        if json {
            MachinePresenter::present_success(&repository, &data);
        } else {
            println!("Synchronized observations from {name}.");
            if topology.is_empty() {
                println!("  no tracked remote named references");
            } else {
                for item in topology {
                    println!("  {}: {}", item["reference"], item["state"]);
                }
            }
        }
        ExitCode::SUCCESS
    } else {
        partial_fetch_failure(&repository, "sync", &name, &fetch, data, json)
    }
}

fn open_transports(
    repository: &Repository,
    name: &str,
) -> Result<(RemoteConfig, HttpSemanticTransport, GitWorkspaceTransport), (&'static str, String)> {
    let config = RemoteConfig::open(
        repository.root_dir(),
        name,
        repository.metadata.repository_id,
    )
    .map_err(|error| (remote_config_code(&error), error.to_string()))?;
    let token = std::env::var(TOKEN_ENV).map_err(|_| {
        (
            "REMOTE_CREDENTIAL_MISSING",
            format!("set {TOKEN_ENV} for this command; credentials are never stored in KAT"),
        )
    })?;
    let semantic = HttpSemanticTransport::new(&config.hub_url, &token).map_err(|_| {
        (
            "REMOTE_CONFIGURATION_INVALID",
            "Hub client configuration is invalid".into(),
        )
    })?;
    let workspace = GitWorkspaceTransport::open_with_runtime_token(repository.root_dir(), &token)
        .map_err(|_| {
        (
            "REMOTE_CONFIGURATION_INVALID",
            "workspace Git client configuration is invalid".into(),
        )
    })?;
    if workspace.binding() != config.binding() || workspace.url() != config.git_url {
        return Err((
            "REMOTE_BINDING_MISMATCH",
            "configured remote differs from the repository's immutable workspace binding".into(),
        ));
    }
    Ok((config, semantic, workspace))
}

fn current_workspace(
    repository: &Repository,
) -> Result<crate::domain::workspace::Workspace, (&'static str, String)> {
    let backend = GitWorkspaceBackend::open(repository.root_dir())
        .map_err(|error| ("WORKSPACE_UNAVAILABLE", error.to_string()))?;
    open_workspace(repository.root_dir(), &backend)
        .map_err(|error: WorkspaceError| ("WORKSPACE_UNAVAILABLE", error.to_string()))
}

fn current_clean_workspace(
    repository: &Repository,
) -> Result<crate::domain::workspace::Workspace, (&'static str, String)> {
    let workspace = current_workspace(repository)?;
    let revision = repository
        .read_revision(workspace.base_revision)
        .map_err(|error| ("WORKSPACE_UNAVAILABLE", error.to_string()))?;
    if revision.semantic_state.as_object_id() != repository.accepted.state {
        return Err((
            "PUSH_WORKSPACE_STATE_MISMATCH",
            "the workspace base does not represent the current accepted semantic state; finalize a repository revision before pushing".into(),
        ));
    }
    let backend = GitWorkspaceBackend::open(repository.root_dir())
        .map_err(|error| ("WORKSPACE_UNAVAILABLE", error.to_string()))?;
    let state = backend
        .inspect_working_state(&revision.workspace_snapshot)
        .map_err(|error| ("WORKSPACE_UNAVAILABLE", error.to_string()))?;
    if !state.changes.is_clean() {
        return Err((
            "PUSH_WORKSPACE_DIRTY",
            "the working tree differs from the workspace base; finalize a revision before pushing"
                .into(),
        ));
    }
    Ok(workspace)
}

fn present_fetch(
    repository: &Repository,
    name: &str,
    outcome: &FetchOutcome,
    json: bool,
) -> ExitCode {
    let data = fetch_value(name, outcome);
    if !outcome.failed.is_empty() {
        return partial_fetch_failure(repository, "fetch", name, outcome, data, json);
    }
    if json {
        MachinePresenter::present_success(repository, &data);
    } else {
        println!(
            "Fetched {name}: {} updated, {} unchanged, {} removed, {} unnamed heads.",
            outcome.updated.len(),
            outcome.unchanged.len(),
            outcome.removed.len(),
            outcome.unnamed_heads.len()
        );
    }
    ExitCode::SUCCESS
}

fn partial_fetch_failure(
    repository: &Repository,
    command: &str,
    remote: &str,
    outcome: &FetchOutcome,
    details: serde_json::Value,
    json: bool,
) -> ExitCode {
    const MESSAGE: &str = "content verified, but some remote-tracking references were not updated";
    if json {
        return failure_with_details(
            Some(repository),
            true,
            command,
            "FETCH_PARTIAL_TRACKING_UPDATE",
            MESSAGE,
            details,
        );
    }

    eprintln!("kat {command} {remote}: {MESSAGE}");
    for item in &outcome.updated {
        eprintln!("  updated {} -> {}", item.name, item.target);
    }
    for item in &outcome.unchanged {
        eprintln!("  unchanged {} -> {}", item.name, item.target);
    }
    for item in &outcome.removed {
        eprintln!("  removed {} (was {})", item.name, item.target);
    }
    for item in &outcome.failed {
        eprintln!("  failed {}: {}", item.name, item.error);
    }
    for head in &outcome.unnamed_heads {
        eprintln!("  unnamed head {head}");
    }
    if let Some(topology) = details["topology"].as_array() {
        for item in topology {
            if let (Some(reference), Some(state)) =
                (item["reference"].as_str(), item["state"].as_str())
            {
                eprintln!("  {reference}: {state}");
            }
        }
    }
    ExitCode::FAILURE
}

fn fetch_value(name: &str, outcome: &FetchOutcome) -> serde_json::Value {
    let reference = |item: &crate::domain::reference::RemoteReference| serde_json::json!({ "name": item.name, "target": item.target });
    serde_json::json!({
        "remote": name,
        "updated": outcome.updated.iter().map(reference).collect::<Vec<_>>(),
        "unchanged": outcome.unchanged.iter().map(reference).collect::<Vec<_>>(),
        "removed": outcome.removed.iter().map(reference).collect::<Vec<_>>(),
        "failed": outcome.failed.iter().map(|item| serde_json::json!({
            "name": item.name,
            "expected": item.expected,
            "target": item.target,
            "error": item.error.to_string(),
        })).collect::<Vec<_>>(),
        "unnamed_heads": outcome.unnamed_heads,
    })
}

fn topology_value(
    name: &str,
    local: RepositoryRevisionId,
    remote: RepositoryRevisionId,
    state: DivergenceState,
) -> serde_json::Value {
    let (state, details) = match state {
        DivergenceState::Same => ("Same", serde_json::Value::Null),
        DivergenceState::LocalAhead => ("LocalAhead", serde_json::Value::Null),
        DivergenceState::OtherAhead => ("OtherAhead", serde_json::Value::Null),
        DivergenceState::Diverged { common_base } => (
            "Diverged",
            serde_json::json!({ "common_base": common_base }),
        ),
        DivergenceState::AmbiguousMergeBase { bases } => (
            "AmbiguousMergeBase",
            serde_json::json!({ "ambiguous_merge_bases": bases }),
        ),
        DivergenceState::Unrelated => ("Unrelated", serde_json::json!({ "unrelated": true })),
    };
    serde_json::json!({
        "reference": name,
        "local_revision": local,
        "remote_revision": remote,
        "state": state,
        "details": details,
    })
}

fn remote_value(remote: &RemoteConfig) -> serde_json::Value {
    serde_json::json!({
        "name": remote.name(),
        "repository_id": remote.repository_id,
        "workspace_backend_id": remote.workspace_backend_id,
        "hub_url": remote.hub_url,
        "git_url": remote.git_url,
    })
}

fn remote_failure(
    repository: &Repository,
    remote: &str,
    command: &str,
    error: RemoteError,
    json: bool,
) -> ExitCode {
    let code = remote_error_code(&error);
    failure_with_details(
        Some(repository),
        json,
        command,
        code,
        error.to_string(),
        serde_json::json!({ "remote": remote, "error_kind": code }),
    )
}

fn transport_failure(
    repository: &Repository,
    remote: &str,
    command: &str,
    error: RemoteTransportError,
    json: bool,
) -> ExitCode {
    let code = transport_error_code(&error);
    failure_with_details(
        Some(repository),
        json,
        command,
        code,
        error.to_string(),
        serde_json::json!({ "remote": remote, "error_kind": code }),
    )
}

fn remote_error_code(error: &RemoteError) -> &'static str {
    match error {
        RemoteError::Transport(error) => transport_error_code(error),
        RemoteError::MissingRemoteObject(_)
        | RemoteError::ObjectHashMismatch { .. }
        | RemoteError::UnexpectedObjectKind { .. }
        | RemoteError::ConflictingObjectKinds(_)
        | RemoteError::SemanticChangeStateMismatch { .. }
        | RemoteError::MissingRemoteSnapshot(_)
        | RemoteError::FetchedSnapshotUnavailable(_)
        | RemoteError::DuplicateRemoteReference(_)
        | RemoteError::NamedAndUnnamedHead(_)
        | RemoteError::DuplicateUnnamedHead(_)
        | RemoteError::RevisionParentCycle
        | RemoteError::SemanticClosureMismatch
        | RemoteError::PhysicalClosureMismatch => "REMOTE_CONTENT_INTEGRITY_FAILED",
        _ => "REMOTE_OPERATION_FAILED",
    }
}

fn transport_error_code(error: &RemoteTransportError) -> &'static str {
    match error {
        RemoteTransportError::ReferenceConflict => "REMOTE_REFERENCE_CONFLICT",
        RemoteTransportError::RepositoryMismatch { .. } => "REMOTE_BINDING_MISMATCH",
        RemoteTransportError::InvalidData => "REMOTE_CONTENT_REJECTED",
        RemoteTransportError::Other(message) if message.contains("authentication required") => {
            "REMOTE_AUTHENTICATION_FAILED"
        }
        RemoteTransportError::Other(message) if message.contains("permission denied") => {
            "REMOTE_PERMISSION_DENIED"
        }
        RemoteTransportError::Other(message) if message.contains("binding mismatch") => {
            "REMOTE_BINDING_MISMATCH"
        }
        RemoteTransportError::Other(message)
            if message.contains("remote rejected invalid KAT data") =>
        {
            "REMOTE_CONTENT_REJECTED"
        }
        RemoteTransportError::ObjectIntegrity(_) | RemoteTransportError::SnapshotIntegrity(_) => {
            "REMOTE_CONTENT_INTEGRITY_FAILED"
        }
        _ => "REMOTE_TRANSPORT_FAILED",
    }
}

fn remote_config_code(error: &RemoteConfigError) -> &'static str {
    match error {
        RemoteConfigError::InvalidName => "INVALID_REMOTE_NAME",
        RemoteConfigError::AlreadyExists => "REMOTE_ALREADY_CONFIGURED",
        RemoteConfigError::NotFound => "REMOTE_NOT_CONFIGURED",
        RemoteConfigError::Invalid(_) | RemoteConfigError::Io(_) => "REMOTE_CONFIGURATION_INVALID",
    }
}

fn failure(
    repository: Option<&Repository>,
    json: bool,
    command: &str,
    code: &str,
    message: impl std::fmt::Display,
) -> ExitCode {
    failure_with_details(
        repository,
        json,
        command,
        code,
        message,
        serde_json::Value::Null,
    )
}

fn failure_with_details(
    repository: Option<&Repository>,
    json: bool,
    command: &str,
    code: &str,
    message: impl std::fmt::Display,
    details: serde_json::Value,
) -> ExitCode {
    let message = message.to_string();
    if json {
        MachinePresenter::present_error_with_details(repository, code, &message, details);
    } else {
        eprintln!("kat {command}: {message}");
    }
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::ObjectId;

    fn revision(value: u8) -> RepositoryRevisionId {
        RepositoryRevisionId::from_object_id(ObjectId::from_bytes([value; 32]))
    }

    #[test]
    fn topology_json_keeps_all_states_distinct() {
        let base = revision(1);
        let a = revision(2);
        let b = revision(3);
        let cases = [
            (DivergenceState::Same, "Same"),
            (DivergenceState::LocalAhead, "LocalAhead"),
            (DivergenceState::OtherAhead, "OtherAhead"),
            (DivergenceState::Diverged { common_base: base }, "Diverged"),
            (
                DivergenceState::AmbiguousMergeBase { bases: vec![a, b] },
                "AmbiguousMergeBase",
            ),
            (DivergenceState::Unrelated, "Unrelated"),
        ];

        for (state, expected) in cases {
            assert_eq!(topology_value("main", a, b, state)["state"], expected);
        }
        let ambiguous = topology_value(
            "main",
            a,
            b,
            DivergenceState::AmbiguousMergeBase { bases: vec![a, b] },
        );
        assert_eq!(
            ambiguous["details"]["ambiguous_merge_bases"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let unrelated = topology_value("main", a, b, DivergenceState::Unrelated);
        assert_eq!(unrelated["details"]["unrelated"], true);
    }
}
