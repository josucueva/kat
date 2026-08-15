//! Physical repository: object store, reference store, metadata,
//! initialization, and open/integrity.

pub mod change;
pub mod error;
pub mod init;
pub mod metadata;
pub mod object_store;
pub mod open;
pub mod query;
pub mod ref_store;
pub mod resolve;
pub mod session;
pub mod validation;

pub use session::{
    DRAFT_SESSION_VERSION, DraftSession, DraftSessionError, DraftSessionState, abort_draft_session,
    begin_draft_session, draft_session_dir, draft_session_path, has_draft_session,
    mark_draft_session_stale, read_draft_session, write_draft_session_atomic,
};

pub use change::{
    ChangeContext, ChangeError, CreateElementInput, DeprecateElementInput, LinkElementInput,
    PersistedChange, PersistedDeprecateChange, PersistedLinkChange, PersistedSupersedeChange,
    PersistedUnlinkChange, PersistedUpdateChange, PreconditionError, PreparedChangeRevision,
    PreparedDeprecateChangeRevision, PreparedElementCreation, PreparedElementDeprecation,
    PreparedElementLinked, PreparedElementSuperseded, PreparedElementUnlinked,
    PreparedElementUpdate, PreparedLinkChangeRevision, PreparedSupersedeChangeRevision,
    PreparedUnlinkChangeRevision, PreparedUpdateChangeRevision, PublishedChange,
    PublishedDeprecateChange, PublishedLinkChange, PublishedSupersedeChange, PublishedUnlinkChange,
    PublishedUpdateChange, StagedOperationInput, SupersedeElementInput, UnlinkElementInput,
    UpdateElementInput, ValidatedElementCreation, ValidatedElementDeprecation,
    ValidatedElementLinked, ValidatedElementSuperseded, ValidatedElementUnlinked,
    ValidatedElementUpdate, apply_create_element, apply_deprecate_element, apply_link_element,
    apply_supersede_element, apply_unlink_element, apply_update_element, commit_draft_session,
    persist_prepared_change, persist_prepared_deprecate_change, persist_prepared_link_change,
    persist_prepared_supersede_change, persist_prepared_unlink_change,
    persist_prepared_update_change, prepare_change, prepare_change_revision,
    prepare_deprecate_change_revision, prepare_link_change_revision,
    prepare_supersede_change_revision, prepare_unlink_change_revision,
    prepare_update_change_revision, publish_persisted_change, publish_persisted_deprecate_change,
    publish_persisted_link_change, publish_persisted_supersede_change,
    publish_persisted_unlink_change, publish_persisted_update_change, stage_operation_into_session,
    validate_create_element_invariants, validate_create_element_ontology,
    validate_deprecate_element_invariants, validate_deprecate_element_ontology,
    validate_link_element_invariants, validate_link_element_ontology,
    validate_supersede_element_invariants, validate_supersede_element_ontology,
    validate_unlink_element_invariants, validate_update_element_invariants,
    validate_update_element_ontology,
};
pub use open::{Repository, open_repository};
pub use query::{
    ArtifactAccountability, ArtifactAccountabilityReport, ArtifactAccountabilityStatus,
    ArtifactBaseline, ElementView, HistoryEntry, ImpactPath, ImpactResult, ImpactStep,
    ImpactedElement, ListFilter, QueryError, RelationshipNeighborhood, RelationshipView, TracePath,
    TraceResult, TraceStep, TraversalDirection, analyze_artifact_accountability, analyze_impact,
    history, history_entry_touches_element, impact_propagation_direction, list_elements,
    origin_traversal_direction, show_element, trace_origin,
};
pub use resolve::{ResolveError, resolve_element_id, resolve_relationship_id};
pub use validation::invariant::InvariantError;
pub mod validation_repository {
    pub use crate::repository::validation::repository::*;
}
pub use validation::ontology::{OntologyError, validate_element_type, validate_relationship};
pub use validation::repository::{
    UnverifiedConstraint, ValidationReport, ValidationViolation, ValidationViolationKind,
    validate_repository, validate_repository_state,
};
