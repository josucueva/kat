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
    ChangeContext, ChangeError, CreateElementInput, DeprecateElementInput, LinkElementInput,
    PersistedChange, PersistedDeprecateChange, PersistedLinkChange, PersistedSupersedeChange,
    PersistedUpdateChange, PreconditionError, PreparedChangeRevision,
    PreparedDeprecateChangeRevision, PreparedElementCreation, PreparedElementDeprecation,
    PreparedElementLinked, PreparedElementSuperseded, PreparedElementUnlinked,
    PreparedElementUpdate, PreparedLinkChangeRevision, PreparedSupersedeChangeRevision,
    PreparedUnlinkChangeRevision, PreparedUpdateChangeRevision, PublishedChange,
    PublishedDeprecateChange, PublishedLinkChange, PublishedSupersedeChange, PublishedUpdateChange,
    SupersedeElementInput, UnlinkElementInput, UpdateElementInput, ValidatedElementCreation,
    ValidatedElementDeprecation, ValidatedElementLinked, ValidatedElementSuperseded,
    ValidatedElementUnlinked, ValidatedElementUpdate, apply_create_element,
    apply_deprecate_element, apply_link_element, apply_supersede_element, apply_unlink_element,
    apply_update_element, persist_prepared_change, persist_prepared_deprecate_change,
    persist_prepared_link_change, persist_prepared_supersede_change,
    persist_prepared_update_change, prepare_change, prepare_change_revision,
    prepare_deprecate_change_revision, prepare_link_change_revision,
    prepare_supersede_change_revision, prepare_unlink_change_revision,
    prepare_update_change_revision, publish_persisted_change, publish_persisted_deprecate_change,
    publish_persisted_link_change, publish_persisted_supersede_change,
    publish_persisted_update_change, validate_create_element_invariants,
    validate_create_element_ontology, validate_deprecate_element_invariants,
    validate_deprecate_element_ontology, validate_link_element_invariants,
    validate_link_element_ontology, validate_supersede_element_invariants,
    validate_supersede_element_ontology, validate_unlink_element_invariants,
    validate_update_element_invariants, validate_update_element_ontology,
};
pub use open::{Repository, open_repository};
pub use query::{ElementView, HistoryEntry, QueryError, history, show_element};
pub use validation::invariant::InvariantError;
pub use validation::ontology::{OntologyError, validate_element_type, validate_relationship};
