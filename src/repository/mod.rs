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

pub use change::{ChangeContext, ChangeError, prepare_change};
pub use open::{Repository, open_repository};
