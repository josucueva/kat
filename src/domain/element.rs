//! Knowledge element versions and lifecycle
//! (see `spec/canonical-format.cddl`, `knowledge-element-version`).

use std::fmt;

use crate::domain::identity::ElementId;
use crate::domain::property::PropertyValue;

/// Lifecycle of a knowledge element version.
///
/// The canonical numeric values (`0` active, `1` deprecated, `2` superseded)
/// are assigned explicitly by the encoder (step 0.4); this enum does not
/// hard-code them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
pub enum Lifecycle {
    /// `0`
    Active,
    /// `1`
    Deprecated,
    /// `2`
    Superseded,
}

impl fmt::Display for Lifecycle {
    /// Boring, deterministic human-oriented rendering for CLI display (not a
    /// canonical form; the canonical numeric values are assigned by the
    /// encoder).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Lifecycle::Active => "active",
            Lifecycle::Deprecated => "deprecated",
            Lifecycle::Superseded => "superseded",
        })
    }
}

/// One immutable version of a knowledge element.
///
/// Holds semantic identity and values only; the canonical ObjectId is derived
/// from its encoding and is never cached here.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeElementVersion {
    /// Stable identity of the element, unchanged across versions.
    pub element_id: ElementId,
    /// Ontology element type identifier (e.g. `kat.core/requirement`).
    pub type_id: String,
    /// Lifecycle of this version.
    pub lifecycle: Lifecycle,
    /// Ontology-defined semantic properties (canonical ordered pairs).
    pub properties: Vec<(String, PropertyValue)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_display_is_deterministic_lowercase() {
        assert_eq!(Lifecycle::Active.to_string(), "active");
        assert_eq!(Lifecycle::Deprecated.to_string(), "deprecated");
        assert_eq!(Lifecycle::Superseded.to_string(), "superseded");
    }
}
