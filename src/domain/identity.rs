//! Stable semantic identities and immutable content identity.
//!
//! KAT distinguishes two identity kinds (see `docs/prototype-design.md`,
//! "Identity Model"):
//!
//! * **Stable semantic identity** — UUIDv4; identifies a concept that
//!   persists through evolution (Repository, Software, Element, Relationship,
//!   Change, Ontology).
//! * **Immutable object identity** — [`ObjectId`], a SHA-256 digest over the
//!   deterministic CBOR encoding of an immutable canonical object. The digest
//!   is computed in `encoding::hash` (step 0.6); this type only holds the
//!   32 digest bytes.
//!
//! Canonical rules (from `spec/canonical-format.cddl`): UUIDs use CBOR tag 37
//! containing exactly 16 bytes; textual Object IDs are 64 lowercase
//! hexadecimal characters. CBOR encoding of these values is a canonical-format
//! concern (step 0.4) and is intentionally not implemented here.

use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

/// Length, in characters, of the canonical textual form of an [`ObjectId`].
const OBJECT_ID_TEXT_LEN: usize = 64;

/// Defines one strongly typed UUID semantic ID.
///
/// Each semantic ID is a distinct newtype (not an alias) so different kinds
/// of identity cannot be interchanged at compile time.
macro_rules! define_uuid_semantic_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
        pub struct $name(Uuid);

        // `Default` is intentionally not implemented: a random v4 ID is not a
        // deterministic "default" value. `new()` covers generation.
        #[allow(clippy::new_without_default)]
        impl $name {
            /// Generates a new random (version 4) ID.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing UUID.
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns the underlying UUID.
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::from_str(s)?))
            }
        }
    };
}

define_uuid_semantic_id! {
    /// Stable identity of a KAT repository.
    RepositoryId
}

define_uuid_semantic_id! {
    /// Stable identity of the software system a repository describes.
    SoftwareId
}

define_uuid_semantic_id! {
    /// Stable identity of a knowledge element, unchanged while the element
    /// evolves through new versions.
    ElementId
}

define_uuid_semantic_id! {
    /// Stable identity of a semantic relationship, unchanged while the
    /// relationship evolves through new versions.
    RelationshipId
}

define_uuid_semantic_id! {
    /// Stable identity of a logical Change, shared by its revisions.
    ChangeId
}

define_uuid_semantic_id! {
    /// Stable identity of the repository ontology, unchanged across ontology
    /// versions.
    OntologyId
}

/// Immutable content identity: the SHA-256 digest over a canonical object's
/// deterministic CBOR bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ObjectId([u8; 32]);

impl ObjectId {
    /// Creates an `ObjectId` from its raw 32 digest bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw 32 digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Consumes this `ObjectId`, returning the raw 32 digest bytes.
    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for ObjectId {
    /// Always writes exactly 64 lowercase hexadecimal characters.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0u8; OBJECT_ID_TEXT_LEN];
        hex::encode_to_slice(self.0, &mut buf)
            .expect("32 bytes always encode to exactly 64 hex characters");
        f.write_str(std::str::from_utf8(&buf).expect("hex output is ASCII"))
    }
}

/// Error returned when parsing a textual [`ObjectId`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
pub enum ObjectIdParseError {
    /// The text was not exactly 64 characters long.
    #[error("object ID must contain exactly 64 hexadecimal characters")]
    InvalidLength,

    /// A character was not a lowercase hexadecimal digit (`0-9`, `a-f`).
    #[error("object ID must use lowercase hexadecimal characters")]
    InvalidCharacter,
}

impl FromStr for ObjectId {
    type Err = ObjectIdParseError;

    /// Parses the canonical textual form: exactly 64 lowercase hex digits.
    ///
    /// Uppercase digits are deliberately rejected to keep the canonical
    /// textual representation unambiguous (lowercase is the canonical form).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != OBJECT_ID_TEXT_LEN {
            return Err(ObjectIdParseError::InvalidLength);
        }

        let mut bytes = [0u8; 32];
        for (byte, pair) in bytes.iter_mut().zip(s.as_bytes().chunks_exact(2)) {
            let high = decode_hex_nibble(pair[0])?;
            let low = decode_hex_nibble(pair[1])?;
            *byte = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
}

/// Decodes one lowercase hexadecimal digit.
fn decode_hex_nibble(b: u8) -> Result<u8, ObjectIdParseError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(ObjectIdParseError::InvalidCharacter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use uuid::Uuid;

    /// Asserts that a UUID is a random (version 4) RFC 4122 UUID.
    fn assert_v4(uuid: Uuid) {
        assert_eq!(uuid.get_version(), Some(uuid::Version::Random));
        assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122);
    }

    #[test]
    fn uuid_semantic_ids_generate_v4() {
        assert_v4(RepositoryId::new().as_uuid());
        assert_v4(SoftwareId::new().as_uuid());
        assert_v4(ElementId::new().as_uuid());
        assert_v4(RelationshipId::new().as_uuid());
        assert_v4(ChangeId::new().as_uuid());
        assert_v4(OntologyId::new().as_uuid());
    }

    #[test]
    fn uuid_semantic_ids_textual_round_trip() {
        let id = ElementId::new();
        let parsed = ElementId::from_str(&id.to_string()).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn uuid_semantic_ids_wrap_and_expose_uuid() {
        let uuid = Uuid::new_v4();
        let id = ChangeId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn semantic_id_types_are_distinct() {
        // Compile-time property: each wrapper is its own type, so a value of
        // one type cannot be used where another is expected (e.g. calling a
        // function that takes `ElementId` with a `RelationshipId` fails to
        // compile). These identity functions pin each exact type.
        fn element_identity(id: ElementId) -> ElementId {
            id
        }
        fn relationship_identity(id: RelationshipId) -> RelationshipId {
            id
        }

        // The same underlying UUID is representable in both types...
        let uuid = Uuid::nil();
        let element = element_identity(ElementId::from_uuid(uuid));
        let relationship = relationship_identity(RelationshipId::from_uuid(uuid));
        assert_eq!(element.as_uuid(), relationship.as_uuid());
        // ...yet the wrapper types remain distinct. This assignment would NOT
        // compile, which is the property under test:
        // let _wrong: ElementId = relationship;
    }

    #[test]
    fn object_id_display_is_lowercase_hex() {
        let mut bytes = [0u8; 32];
        bytes[..5].copy_from_slice(&[0x12, 0x34, 0xab, 0xcd, 0xef]);

        let text = ObjectId::from_bytes(bytes).to_string();

        assert_eq!(text.len(), 64);
        assert_eq!(&text[..10], "1234abcdef");
        assert!(text[10..].bytes().all(|b| b == b'0'));
        // Every character must be a hex digit that is not uppercase (digits
        // `0-9` and letters `a-f`); uppercase `A-F` must never appear.
        assert!(
            text.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
    }

    #[test]
    fn object_id_parses_lowercase_hex() {
        let text = format!("1234abcdef{}", "0".repeat(54));
        let id = ObjectId::from_str(&text).unwrap();

        let mut expected = [0u8; 32];
        expected[..5].copy_from_slice(&[0x12, 0x34, 0xab, 0xcd, 0xef]);
        assert_eq!(id.into_bytes(), expected);
    }

    #[test]
    fn object_id_round_trip() {
        let bytes = [0x5a; 32];
        let id = ObjectId::from_bytes(bytes);
        let parsed = ObjectId::from_str(&id.to_string()).unwrap();
        assert_eq!(id, parsed);
        assert_eq!(parsed.as_bytes(), &bytes);
    }

    #[test]
    fn object_id_rejects_short_text() {
        let err = ObjectId::from_str(&"0".repeat(63)).unwrap_err();
        assert_eq!(err, ObjectIdParseError::InvalidLength);
    }

    #[test]
    fn object_id_rejects_long_text() {
        let err = ObjectId::from_str(&"0".repeat(65)).unwrap_err();
        assert_eq!(err, ObjectIdParseError::InvalidLength);
    }

    #[test]
    fn object_id_rejects_non_hex_characters() {
        let mut text = "0".repeat(63);
        text.push('g');
        let err = ObjectId::from_str(&text).unwrap_err();
        assert_eq!(err, ObjectIdParseError::InvalidCharacter);
    }

    #[test]
    fn object_id_rejects_uppercase() {
        let err = ObjectId::from_str(&"A".repeat(64)).unwrap_err();
        assert_eq!(err, ObjectIdParseError::InvalidCharacter);
    }
}
