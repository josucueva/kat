//! Physical repository: object store, reference store, metadata,
//! initialization, and open/integrity.

pub mod change;
pub mod error;
pub mod init;
pub mod metadata;
pub mod object_store;
pub mod open;
pub mod ref_store;
pub mod validation;

pub use change::{
    ChangeContext, ChangeError, CreateElementInput, PersistedChange, PreconditionError,
    PreparedChangeRevision, PreparedElementCreation, ValidatedElementCreation,
    apply_create_element, persist_prepared_change, prepare_change, prepare_change_revision,
    validate_create_element_invariants, validate_create_element_ontology,
};
pub use open::{Repository, open_repository};
pub use validation::invariant::InvariantError;
pub use validation::ontology::OntologyError;
