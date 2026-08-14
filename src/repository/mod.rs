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
    ChangeContext, ChangeError, CreateElementInput, PersistedChange, PersistedUpdateChange,
    PreconditionError, PreparedChangeRevision, PreparedElementCreation, PreparedElementUpdate,
    PreparedUpdateChangeRevision, PublishedChange, PublishedUpdateChange, UpdateElementInput,
    ValidatedElementCreation, ValidatedElementUpdate, apply_create_element, apply_update_element,
    persist_prepared_change, persist_prepared_update_change, prepare_change,
    prepare_change_revision, prepare_update_change_revision, publish_persisted_change,
    publish_persisted_update_change, validate_create_element_invariants,
    validate_create_element_ontology, validate_update_element_invariants,
    validate_update_element_ontology,
};
pub use open::{Repository, open_repository};
pub use query::{ElementView, HistoryEntry, QueryError, history, show_element};
pub use validation::invariant::InvariantError;
pub use validation::ontology::OntologyError;
