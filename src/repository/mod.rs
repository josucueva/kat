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
pub mod validation;

pub use change::{
    ChangeContext, ChangeError, CreateElementInput, DeprecateElementInput, PersistedChange,
    PersistedDeprecateChange, PersistedUpdateChange, PreconditionError, PreparedChangeRevision,
    PreparedDeprecateChangeRevision, PreparedElementCreation, PreparedElementDeprecation,
    PreparedElementSuperseded, PreparedElementUpdate, PreparedUpdateChangeRevision,
    PublishedChange, PublishedDeprecateChange, PublishedUpdateChange, SupersedeElementInput,
    UpdateElementInput, ValidatedElementCreation, ValidatedElementDeprecation,
    ValidatedElementUpdate, apply_create_element, apply_deprecate_element, apply_supersede_element,
    apply_update_element, persist_prepared_change, persist_prepared_deprecate_change,
    persist_prepared_update_change, prepare_change, prepare_change_revision,
    prepare_deprecate_change_revision, prepare_update_change_revision, publish_persisted_change,
    publish_persisted_deprecate_change, publish_persisted_update_change,
    validate_create_element_invariants, validate_create_element_ontology,
    validate_deprecate_element_invariants, validate_deprecate_element_ontology,
    validate_supersede_element_ontology, validate_update_element_invariants,
    validate_update_element_ontology,
};
pub use open::{Repository, open_repository};
pub use query::{ElementView, HistoryEntry, QueryError, history, show_element};
pub use validation::invariant::InvariantError;
pub use validation::ontology::{OntologyError, validate_element_type, validate_relationship};
