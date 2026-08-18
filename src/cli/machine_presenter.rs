//! Machine interface JSON output presenter.
//!
//! Formats `CommonResultEnvelope<T>` DTOs for stdout emission when `--json` is specified.
//! See `docs/implementation/v0.4/machine-interface.md` and `docs/implementation/v0.4/cli.md`.

use crate::domain::machine::CommonResultEnvelope;
use crate::repository::open::Repository;

/// Presenter for machine-readable JSON envelope outputs.
pub struct MachinePresenter;

impl MachinePresenter {
    /// Formats a `CommonResultEnvelope<T>` into a UTF-8 JSON string ready for stdout emission.
    ///
    /// Validates `INV-MI-01` structural invariants before serialization.
    pub fn render_envelope<T: serde::Serialize>(
        envelope: &CommonResultEnvelope<T>,
    ) -> Result<String, String> {
        envelope
            .validate_invariants()
            .map_err(|err| format!("Machine envelope invariant failure: {err}"))?;

        serde_json::to_string_pretty(envelope)
            .map_err(|err| format!("JSON serialization error: {err}"))
    }

    /// Renders a successful machine result envelope and prints it to stdout.
    pub fn present_success<T: serde::Serialize>(repository: &Repository, data: &T) {
        let repo_id = repository.metadata.repository_id;
        let state_id = repository.accepted.state;
        let envelope = CommonResultEnvelope::success(Some(repo_id), Some(state_id), data);
        if let Ok(json) = Self::render_envelope(&envelope) {
            println!("{json}");
        }
    }

    /// Renders an error machine result envelope and prints it to stdout.
    pub fn present_error(repository: Option<&Repository>, code: &str, message: &str) {
        Self::present_error_with_details(repository, code, message, serde_json::Value::Null);
    }

    /// Renders an error machine result envelope with structured details and prints it to stdout.
    pub fn present_error_with_details(
        repository: Option<&Repository>,
        code: &str,
        message: &str,
        details: serde_json::Value,
    ) {
        let repo_id = repository.map(|r| r.metadata.repository_id);
        let state_id = repository.map(|r| r.accepted.state);
        let envelope = CommonResultEnvelope::<()>::failure(
            repo_id,
            state_id,
            code,
            message,
            details,
        );
        if let Ok(json) = Self::render_envelope(&envelope) {
            println!("{json}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::{ObjectId, RepositoryId};
    use uuid::Uuid;

    #[test]
    fn render_envelope_validates_and_serializes_cleanly() {
        let repo_id = RepositoryId::from_uuid(Uuid::nil());
        let state_id = ObjectId::from_bytes([0x11; 32]);
        let envelope = CommonResultEnvelope::success(
            Some(repo_id),
            Some(state_id),
            serde_json::json!({ "status": "ok" }),
        );

        let json = MachinePresenter::render_envelope(&envelope).unwrap();
        assert!(json.contains("\"success\": true"));
        assert!(json.contains("\"interface_schema_version\": 1"));
        assert!(json.contains("\"status\": \"ok\""));
    }
}
