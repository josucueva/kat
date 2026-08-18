//! Machine interface DTO types, global result envelope, and error structures.
//!
//! See `docs/implementation/v0.4/machine-interface.md`.

use serde::{Deserialize, Serialize};

use crate::domain::identity::{ObjectId, RepositoryId};

/// Current machine interface DTO schema version (v1).
///
/// Decoupled from KAT binary crate version.
pub const INTERFACE_SCHEMA_VERSION: u32 = 1;

/// Common top-level machine result envelope (`CommonResultEnvelope<T>`).
///
/// Enforces `INV-MI-01`:
/// - `success == true` iff `data.is_some() && error.is_none()`
/// - `success == false` iff `data.is_none() && error.is_some()`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonResultEnvelope<T> {
    /// KAT binary version string.
    pub kat_version: String,
    /// Machine interface DTO schema version (`1` for v0.4).
    pub interface_schema_version: u32,
    /// Execution status flag.
    pub success: bool,
    /// Canonical repository UUID string (`None` if repository context could not be resolved).
    pub repository_id: Option<RepositoryId>,
    /// 64-hex string of current accepted `SemanticState` ObjectId (`None` if context unresolved).
    pub accepted_state_id: Option<ObjectId>,
    /// Operation-specific DTO payload (populated iff `success == true`).
    pub data: Option<T>,
    /// Error details (populated iff `success == false`).
    pub error: Option<ErrorEnvelope>,
}

impl<T> CommonResultEnvelope<T> {
    /// Constructs a successful machine envelope.
    pub fn success(
        repository_id: Option<RepositoryId>,
        accepted_state_id: Option<ObjectId>,
        data: T,
    ) -> Self {
        Self {
            kat_version: env!("CARGO_PKG_VERSION").to_string(),
            interface_schema_version: INTERFACE_SCHEMA_VERSION,
            success: true,
            repository_id,
            accepted_state_id,
            data: Some(data),
            error: None,
        }
    }

    /// Constructs an error machine envelope.
    pub fn failure(
        repository_id: Option<RepositoryId>,
        accepted_state_id: Option<ObjectId>,
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            kat_version: env!("CARGO_PKG_VERSION").to_string(),
            interface_schema_version: INTERFACE_SCHEMA_VERSION,
            success: false,
            repository_id,
            accepted_state_id,
            data: None,
            error: Some(ErrorEnvelope {
                code: code.into(),
                message: message.into(),
                details,
            }),
        }
    }

    /// Validates `INV-MI-01` envelope structural invariants.
    pub fn validate_invariants(&self) -> Result<(), &'static str> {
        if self.success {
            if self.data.is_none() || self.error.is_some() {
                return Err(
                    "INV-MI-01 violation: success == true requires data != None and error == None",
                );
            }
        } else if self.data.is_some() || self.error.is_none() {
            return Err(
                "INV-MI-01 violation: success == false requires data == None and error != None",
            );
        }
        Ok(())
    }
}

/// Structured error payload when `success == false`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    /// Machine error code string (e.g. `"NOT_IN_REPOSITORY"`, `"ONTOLOGY_TARGET_TYPE_DISALLOWED"`).
    pub code: String,
    /// Human-readable error summary message.
    pub message: String,
    /// Detailed JSON context object.
    pub details: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn success_envelope_construction_and_invariants() {
        let repo_id = RepositoryId::from_uuid(Uuid::nil());
        let state_id = ObjectId::from_bytes([0xaa; 32]);
        let envelope =
            CommonResultEnvelope::success(Some(repo_id), Some(state_id), "payload".to_string());

        assert!(envelope.success);
        assert_eq!(envelope.interface_schema_version, 1);
        assert!(envelope.validate_invariants().is_ok());

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"interface_schema_version\":1"));
        assert!(json.contains("\"data\":\"payload\""));
        assert!(json.contains("\"error\":null"));

        let deserialized: CommonResultEnvelope<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope, deserialized);
    }

    #[test]
    fn failure_envelope_construction_with_nullable_repo_context() {
        let envelope: CommonResultEnvelope<()> = CommonResultEnvelope::failure(
            None,
            None,
            "NOT_IN_REPOSITORY",
            "No KAT repository found at path",
            serde_json::json!({ "path": "/tmp" }),
        );

        assert!(!envelope.success);
        assert!(envelope.validate_invariants().is_ok());

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"repository_id\":null"));
        assert!(json.contains("\"accepted_state_id\":null"));
        assert!(json.contains("\"code\":\"NOT_IN_REPOSITORY\""));
    }

    #[test]
    fn invalid_envelope_fails_invariant_validation() {
        let invalid_env = CommonResultEnvelope {
            kat_version: env!("CARGO_PKG_VERSION").to_string(),
            interface_schema_version: 1,
            success: true,
            repository_id: None,
            accepted_state_id: None,
            data: Some("payload"),
            error: Some(ErrorEnvelope {
                code: "ERR".to_string(),
                message: "msg".to_string(),
                details: serde_json::json!({}),
            }),
        };

        assert!(invalid_env.validate_invariants().is_err());
    }
}
