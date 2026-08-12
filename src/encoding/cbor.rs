//! Deterministic CBOR encoder (RFC 8949 core deterministic profile, §4.2.1).
//!
//! [`canonical_bytes`] is the public entry point: it structurally validates a
//! [`CanonicalObject`] and then encodes it to the exact canonical bytes
//! required by `spec/canonical-format.cddl` and `docs/canonical-format.md`.
//!
//! ```text
//! CanonicalObject
//!       |
//!       v
//! structural validation   (refuses structurally non-canonical objects)
//!       |
//!       v
//! explicit CBOR encoder   (emits every protocol number literally)
//!       |
//!       v
//! canonical bytes
//! ```
//!
//! The encoder never sorts or repairs malformed values: a structurally
//! non-canonical object is rejected, not silently normalized. All protocol
//! numbers (envelope fields, object kinds, lifecycle values, operation
//! identifiers, tags) are emitted explicitly; nothing relies on Rust enum
//! discriminants or field names.

use std::cmp::Ordering;

use uuid::Uuid;

use crate::domain::change::ChangeRevision;
use crate::domain::element::{KnowledgeElementVersion, Lifecycle};
use crate::domain::identity::ObjectId;
use crate::domain::ontology::{ElementTypeDefinition, OntologyVersion, RelationshipTypeDefinition};
use crate::domain::operation::Operation;
use crate::domain::property::PropertyValue;
use crate::domain::relationship::RelationshipVersion;
use crate::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use crate::encoding::error::EncodingError;
use crate::encoding::object::{
    CanonicalObject, CanonicalPayload, ENVELOPE_VERSION, ObjectKind, SCHEMA_VERSION,
};
use crate::encoding::validate::{CanonicalStructureError, CanonicalValidate};

/// Encodes a canonical object to its exact deterministic CBOR bytes.
///
/// Refuses to encode objects that are not structurally canonical: validation
/// runs first, so unsorted or duplicate collections can never be smuggled
/// past the encoder.
pub fn canonical_bytes(object: &CanonicalObject) -> Result<Vec<u8>, EncodingError> {
    object
        .validate_canonical_structure()
        .map_err(EncodingError::InvalidCanonicalStructure)?;
    let mut writer = CborWriter::new();
    encode_canonical_object(&mut writer, object)?;
    Ok(writer.into_vec())
}

/// In-memory destination for deterministic CBOR output. Writes are infallible:
/// every value KAT encodes (i64 integers, `usize` lengths) fits the CBOR
/// number space, so the only encoding error is structural (validation).
#[derive(Default)]
struct CborWriter {
    buf: Vec<u8>,
}

impl CborWriter {
    fn new() -> Self {
        Self::default()
    }

    fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    /// Emits a major-type header using the shortest argument encoding.
    fn write_head(&mut self, major: u8, arg: u64) {
        let initial = major << 5;
        if arg < 24 {
            self.buf.push(initial | arg as u8);
        } else if arg <= 0xff {
            self.buf.push(initial | 24);
            self.buf.push(arg as u8);
        } else if arg <= 0xffff {
            self.buf.push(initial | 25);
            self.buf.extend_from_slice(&(arg as u16).to_be_bytes());
        } else if arg <= 0xffff_ffff {
            self.buf.push(initial | 26);
            self.buf.extend_from_slice(&(arg as u32).to_be_bytes());
        } else {
            self.buf.push(initial | 27);
            self.buf.extend_from_slice(&arg.to_be_bytes());
        }
    }

    /// Major type 0: unsigned integer.
    fn write_uint(&mut self, value: u64) {
        self.write_head(0, value);
    }

    /// Major type 1: negative integer; `magnitude` encodes `-1 - magnitude`.
    fn write_neg_int(&mut self, magnitude: u64) {
        self.write_head(1, magnitude);
    }

    /// Major type 0/1: signed integer.
    fn write_int(&mut self, value: i64) {
        if value >= 0 {
            self.write_uint(value as u64);
        } else {
            self.write_neg_int((-(value + 1)) as u64);
        }
    }

    /// Major type 2: byte string.
    fn write_byte_string(&mut self, bytes: &[u8]) {
        self.write_head(2, bytes.len() as u64);
        self.buf.extend_from_slice(bytes);
    }

    /// Major type 3: text string.
    fn write_text(&mut self, text: &str) {
        self.write_head(3, text.len() as u64);
        self.buf.extend_from_slice(text.as_bytes());
    }

    /// Major type 4: definite-length array header.
    fn write_array_header(&mut self, len: usize) {
        self.write_head(4, len as u64);
    }

    /// Major type 5: definite-length map header.
    fn write_map_header(&mut self, len: usize) {
        self.write_head(5, len as u64);
    }

    /// Major type 6: tag.
    fn write_tag(&mut self, tag: u64) {
        self.write_head(6, tag);
    }

    /// Major type 7: `true`.
    fn write_true(&mut self) {
        self.buf.push(0xf5);
    }

    /// Major type 7: `false`.
    fn write_false(&mut self) {
        self.buf.push(0xf4);
    }

    /// Major type 7: `null`.
    fn write_null(&mut self) {
        self.buf.push(0xf6);
    }
}

/// Deterministic CBOR encoding of a text string, used both for emitting text
/// values and for computing canonical map-key order.
fn encoded_text(text: &str) -> Vec<u8> {
    let mut writer = CborWriter::new();
    writer.write_text(text);
    writer.into_vec()
}

/// Compares two text strings by their deterministic CBOR encodings.
///
/// This is the RFC 8949 §4.2.1 map-key ordering rule: keys are ordered by
/// bytewise comparison of their full deterministic encoded forms. Shared by
/// the structural validator and the encoder so they always agree.
pub(crate) fn cmp_encoded_text(a: &str, b: &str) -> Ordering {
    encoded_text(a).cmp(&encoded_text(b))
}

/// CBOR tag for UUIDs (`spec/canonical-format.cddl`: tag 37, 16 bytes).
const UUID_TAG: u64 = 37;

/// Encodes a UUID as tag 37 containing exactly 16 bytes (never a text UUID).
fn write_uuid(writer: &mut CborWriter, uuid: &Uuid) {
    writer.write_tag(UUID_TAG);
    writer.write_byte_string(uuid.as_bytes());
}

/// Encodes an ObjectId as a 32-byte byte string.
fn write_object_id(writer: &mut CborWriter, id: &ObjectId) {
    writer.write_byte_string(id.as_bytes());
}

/// Encodes the canonical object envelope map.
fn encode_canonical_object(
    writer: &mut CborWriter,
    object: &CanonicalObject,
) -> Result<(), EncodingError> {
    writer.write_map_header(4);
    writer.write_uint(0);
    writer.write_uint(ENVELOPE_VERSION);
    writer.write_uint(1);
    writer.write_uint(object_kind_number(object.object_kind()));
    writer.write_uint(2);
    writer.write_uint(SCHEMA_VERSION);
    writer.write_uint(3);
    encode_payload(writer, &object.payload)
}

/// Numeric protocol identifier for an object kind.
fn object_kind_number(kind: ObjectKind) -> u64 {
    match kind {
        ObjectKind::KnowledgeElementVersion => 1,
        ObjectKind::RelationshipVersion => 2,
        ObjectKind::ChangeRevision => 3,
        ObjectKind::SemanticState => 4,
        ObjectKind::OntologyVersion => 5,
    }
}

/// Encodes an object-kind-specific payload.
fn encode_payload(
    writer: &mut CborWriter,
    payload: &CanonicalPayload,
) -> Result<(), EncodingError> {
    match payload {
        CanonicalPayload::KnowledgeElementVersion(v) => encode_knowledge_element_version(writer, v),
        CanonicalPayload::RelationshipVersion(v) => encode_relationship_version(writer, v),
        CanonicalPayload::ChangeRevision(v) => encode_change_revision(writer, v),
        CanonicalPayload::SemanticState(v) => encode_semantic_state(writer, v),
        CanonicalPayload::OntologyVersion(v) => encode_ontology_version(writer, v),
    }
}

/// Numeric protocol value for a lifecycle.
fn lifecycle_number(lifecycle: Lifecycle) -> u64 {
    match lifecycle {
        Lifecycle::Active => 0,
        Lifecycle::Deprecated => 1,
        Lifecycle::Superseded => 2,
    }
}

/// Encodes one property map (canonical key order + every value).
fn write_property_map(
    writer: &mut CborWriter,
    entries: &[(String, PropertyValue)],
) -> Result<(), EncodingError> {
    // Re-verify canonical key order by encoded bytes. `canonical_bytes`
    // validates structure first, so this is defense in depth; if it ever
    // fires, validation and encoding disagreed and we fail closed.
    let mut previous: Option<Vec<u8>> = None;
    for (key, _) in entries {
        let key_bytes = encoded_text(key);
        if let Some(previous) = &previous
            && previous >= &key_bytes
        {
            return Err(EncodingError::InvalidCanonicalStructure(
                CanonicalStructureError::PropertyKeysUnordered,
            ));
        }
        previous = Some(key_bytes);
    }

    writer.write_map_header(entries.len());
    for (key, value) in entries {
        writer.write_text(key);
        write_property_value(writer, value)?;
    }
    Ok(())
}

/// Encodes one canonical property value.
fn write_property_value(
    writer: &mut CborWriter,
    value: &PropertyValue,
) -> Result<(), EncodingError> {
    match value {
        PropertyValue::Null => {
            writer.write_null();
            Ok(())
        }
        PropertyValue::Bool(true) => {
            writer.write_true();
            Ok(())
        }
        PropertyValue::Bool(false) => {
            writer.write_false();
            Ok(())
        }
        PropertyValue::Integer(n) => {
            writer.write_int(*n);
            Ok(())
        }
        PropertyValue::Text(s) => {
            writer.write_text(s);
            Ok(())
        }
        PropertyValue::Bytes(bytes) => {
            writer.write_byte_string(bytes);
            Ok(())
        }
        PropertyValue::Uuid(uuid) => {
            write_uuid(writer, uuid);
            Ok(())
        }
        PropertyValue::List(items) => {
            writer.write_array_header(items.len());
            for item in items {
                write_property_value(writer, item)?;
            }
            Ok(())
        }
        PropertyValue::Map(entries) => write_property_map(writer, entries),
    }
}

/// Encodes a KnowledgeElementVersion payload (map fields 0-3).
fn encode_knowledge_element_version(
    writer: &mut CborWriter,
    v: &KnowledgeElementVersion,
) -> Result<(), EncodingError> {
    writer.write_map_header(4);
    writer.write_uint(0);
    write_uuid(writer, &v.element_id.as_uuid());
    writer.write_uint(1);
    writer.write_text(&v.type_id);
    writer.write_uint(2);
    writer.write_uint(lifecycle_number(v.lifecycle));
    writer.write_uint(3);
    write_property_map(writer, &v.properties)
}

/// Encodes a RelationshipVersion payload (map fields 0-4).
fn encode_relationship_version(
    writer: &mut CborWriter,
    v: &RelationshipVersion,
) -> Result<(), EncodingError> {
    writer.write_map_header(5);
    writer.write_uint(0);
    write_uuid(writer, &v.relationship_id.as_uuid());
    writer.write_uint(1);
    write_uuid(writer, &v.source_element_id.as_uuid());
    writer.write_uint(2);
    writer.write_text(&v.relationship_type);
    writer.write_uint(3);
    write_uuid(writer, &v.target_element_id.as_uuid());
    writer.write_uint(4);
    write_property_map(writer, &v.properties)
}

/// Encodes an OntologyVersion payload (map fields 0-2).
fn encode_ontology_version(
    writer: &mut CborWriter,
    o: &OntologyVersion,
) -> Result<(), EncodingError> {
    writer.write_map_header(3);
    writer.write_uint(0);
    write_uuid(writer, &o.ontology_id.as_uuid());
    writer.write_uint(1);
    writer.write_array_header(o.element_types.len());
    for definition in &o.element_types {
        write_element_type_definition(writer, definition);
    }
    writer.write_uint(2);
    writer.write_array_header(o.relationship_types.len());
    for definition in &o.relationship_types {
        write_relationship_type_definition(writer, definition);
    }
    Ok(())
}

/// Encodes one element type definition (map fields 0-1).
fn write_element_type_definition(writer: &mut CborWriter, definition: &ElementTypeDefinition) {
    writer.write_map_header(2);
    writer.write_uint(0);
    writer.write_text(&definition.type_id);
    writer.write_uint(1);
    writer.write_text(&definition.name);
}

/// Encodes one relationship type definition (map fields 0-3).
fn write_relationship_type_definition(
    writer: &mut CborWriter,
    definition: &RelationshipTypeDefinition,
) {
    writer.write_map_header(4);
    writer.write_uint(0);
    writer.write_text(&definition.type_id);
    writer.write_uint(1);
    writer.write_text(&definition.name);
    writer.write_uint(2);
    write_text_array(writer, &definition.allowed_source_types);
    writer.write_uint(3);
    write_text_array(writer, &definition.allowed_target_types);
}

/// Encodes a text array (used for allowed source/target type lists).
fn write_text_array(writer: &mut CborWriter, items: &[String]) {
    writer.write_array_header(items.len());
    for item in items {
        writer.write_text(item);
    }
}

/// Encodes a SemanticState payload (map fields 0-2).
fn encode_semantic_state(
    writer: &mut CborWriter,
    state: &SemanticState,
) -> Result<(), EncodingError> {
    writer.write_map_header(3);
    writer.write_uint(0);
    write_object_id(writer, &state.ontology_version);
    writer.write_uint(1);
    writer.write_array_header(state.elements.len());
    for entry in &state.elements {
        write_element_state_entry(writer, entry);
    }
    writer.write_uint(2);
    writer.write_array_header(state.relationships.len());
    for entry in &state.relationships {
        write_relationship_state_entry(writer, entry);
    }
    Ok(())
}

/// Encodes one element state entry `[ElementId, ObjectId]`.
fn write_element_state_entry(writer: &mut CborWriter, entry: &ElementStateEntry) {
    writer.write_array_header(2);
    write_uuid(writer, &entry.element_id.as_uuid());
    write_object_id(writer, &entry.version);
}

/// Encodes one relationship state entry `[RelationshipId, ObjectId]`.
fn write_relationship_state_entry(writer: &mut CborWriter, entry: &RelationshipStateEntry) {
    writer.write_array_header(2);
    write_uuid(writer, &entry.relationship_id.as_uuid());
    write_object_id(writer, &entry.version);
}

/// Encodes a ChangeRevision payload (map fields 0-4, optional 5).
fn encode_change_revision(
    writer: &mut CborWriter,
    change: &ChangeRevision,
) -> Result<(), EncodingError> {
    writer.write_map_header(if change.description.is_some() { 6 } else { 5 });
    writer.write_uint(0);
    write_uuid(writer, &change.change_id.as_uuid());
    writer.write_uint(1);
    write_object_id_array(writer, &change.base_states);
    writer.write_uint(2);
    write_object_id(writer, &change.result_state);
    writer.write_uint(3);
    writer.write_array_header(change.operations.len());
    for operation in &change.operations {
        encode_operation(writer, operation)?;
    }
    writer.write_uint(4);
    write_object_id_array(writer, &change.dependencies);
    if let Some(description) = &change.description {
        writer.write_uint(5);
        writer.write_text(description);
    }
    Ok(())
}

/// Encodes an ObjectId array (base states / dependencies).
fn write_object_id_array(writer: &mut CborWriter, ids: &[ObjectId]) {
    writer.write_array_header(ids.len());
    for id in ids {
        write_object_id(writer, id);
    }
}

/// Encodes one semantic operation as its canonical tagged array.
fn encode_operation(writer: &mut CborWriter, operation: &Operation) -> Result<(), EncodingError> {
    match operation {
        Operation::CreateElement { new_version } => {
            writer.write_array_header(2);
            writer.write_uint(1);
            write_object_id(writer, new_version);
        }
        Operation::UpdateElement {
            element_id,
            expected_version,
            new_version,
        } => {
            writer.write_array_header(4);
            writer.write_uint(2);
            write_uuid(writer, &element_id.as_uuid());
            write_object_id(writer, expected_version);
            write_object_id(writer, new_version);
        }
        Operation::DeprecateElement {
            element_id,
            expected_version,
            new_version,
        } => {
            writer.write_array_header(4);
            writer.write_uint(3);
            write_uuid(writer, &element_id.as_uuid());
            write_object_id(writer, expected_version);
            write_object_id(writer, new_version);
        }
        Operation::Link {
            new_relationship_version,
        } => {
            writer.write_array_header(2);
            writer.write_uint(4);
            write_object_id(writer, new_relationship_version);
        }
        Operation::Unlink {
            relationship_id,
            expected_version,
        } => {
            writer.write_array_header(3);
            writer.write_uint(5);
            write_uuid(writer, &relationship_id.as_uuid());
            write_object_id(writer, expected_version);
        }
        Operation::Supersede {
            existing_element,
            expected_existing_version,
            replacement_element,
            replacement_version,
            superseding_relationship,
        } => {
            writer.write_array_header(6);
            writer.write_uint(6);
            write_uuid(writer, &existing_element.as_uuid());
            write_object_id(writer, expected_existing_version);
            write_uuid(writer, &replacement_element.as_uuid());
            write_object_id(writer, replacement_version);
            write_object_id(writer, superseding_relationship);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::{ChangeId, ElementId, OntologyId, RelationshipId};
    use crate::encoding::validate::CanonicalStructureError;

    /// Lowercase hex of a UUID's 16 bytes (independent of the encoder).
    fn hex_uuid(n: u128) -> String {
        format!("{n:032x}")
    }

    /// Lowercase hex of an ObjectId whose 32 bytes are all `byte`.
    fn hex_object_id(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    fn object_id(byte: u8) -> ObjectId {
        ObjectId::from_bytes([byte; 32])
    }

    /// Runs one writer operation and returns the produced bytes.
    fn write_with(f: impl FnOnce(&mut CborWriter)) -> Vec<u8> {
        let mut writer = CborWriter::new();
        f(&mut writer);
        writer.into_vec()
    }

    // ------------------------------------------------------------------
    // CBOR primitives (RFC 8949 §3.1 test values)
    // ------------------------------------------------------------------

    #[test]
    fn uint_uses_shortest_encoding() {
        assert_eq!(write_with(|w| w.write_uint(0)), hex::decode("00").unwrap());
        assert_eq!(write_with(|w| w.write_uint(23)), hex::decode("17").unwrap());
        assert_eq!(
            write_with(|w| w.write_uint(24)),
            hex::decode("1818").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_uint(255)),
            hex::decode("18ff").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_uint(256)),
            hex::decode("190100").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_uint(65535)),
            hex::decode("19ffff").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_uint(65536)),
            hex::decode("1a00010000").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_uint(u64::MAX)),
            hex::decode("1bffffffffffffffff").unwrap()
        );
    }

    #[test]
    fn negative_integer_uses_shortest_encoding() {
        // magnitude n encodes -1 - n.
        assert_eq!(
            write_with(|w| w.write_neg_int(0)),
            hex::decode("20").unwrap()
        ); // -1
        assert_eq!(
            write_with(|w| w.write_neg_int(23)),
            hex::decode("37").unwrap()
        ); // -24
        assert_eq!(
            write_with(|w| w.write_neg_int(24)),
            hex::decode("3818").unwrap()
        ); // -25
        assert_eq!(
            write_with(|w| w.write_neg_int(255)),
            hex::decode("38ff").unwrap()
        ); // -256
        assert_eq!(
            write_with(|w| w.write_neg_int(256)),
            hex::decode("390100").unwrap()
        ); // -257
    }

    #[test]
    fn signed_integer_encoding() {
        assert_eq!(write_with(|w| w.write_int(0)), hex::decode("00").unwrap());
        assert_eq!(write_with(|w| w.write_int(-1)), hex::decode("20").unwrap());
        assert_eq!(write_with(|w| w.write_int(-24)), hex::decode("37").unwrap());
        assert_eq!(
            write_with(|w| w.write_int(-25)),
            hex::decode("3818").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_int(i64::MIN)),
            hex::decode("3b7fffffffffffffff").unwrap()
        );
    }

    #[test]
    fn byte_string_encoding() {
        assert_eq!(
            write_with(|w| w.write_byte_string(&[])),
            hex::decode("40").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_byte_string(b"ABC")),
            hex::decode("43414243").unwrap()
        );
        let long = vec![0u8; 24];
        let mut expected = hex::decode("5818").unwrap();
        expected.extend_from_slice(&long);
        assert_eq!(write_with(|w| w.write_byte_string(&long)), expected);
    }

    #[test]
    fn text_string_encoding() {
        assert_eq!(write_with(|w| w.write_text("")), hex::decode("60").unwrap());
        assert_eq!(
            write_with(|w| w.write_text("a")),
            hex::decode("6161").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_text("IETF")),
            hex::decode("6449455446").unwrap()
        );
        let long = "x".repeat(24);
        let mut expected = hex::decode("7818").unwrap();
        expected.extend_from_slice(long.as_bytes());
        assert_eq!(write_with(|w| w.write_text(&long)), expected);
    }

    #[test]
    fn collection_headers() {
        assert_eq!(
            write_with(|w| w.write_array_header(0)),
            hex::decode("80").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_array_header(2)),
            hex::decode("82").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_array_header(24)),
            hex::decode("9818").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_map_header(0)),
            hex::decode("a0").unwrap()
        );
        assert_eq!(
            write_with(|w| w.write_map_header(25)),
            hex::decode("b819").unwrap()
        );
    }

    #[test]
    fn tag_bool_and_null() {
        assert_eq!(write_with(|w| w.write_tag(0)), hex::decode("c0").unwrap());
        assert_eq!(
            write_with(|w| w.write_tag(37)),
            hex::decode("d825").unwrap()
        );
        assert_eq!(write_with(|w| w.write_true()), hex::decode("f5").unwrap());
        assert_eq!(write_with(|w| w.write_false()), hex::decode("f4").unwrap());
        assert_eq!(write_with(|w| w.write_null()), hex::decode("f6").unwrap());
    }

    #[test]
    fn uuid_is_tag_37_with_16_bytes() {
        let uuid = Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10);
        assert_eq!(
            write_with(|w| write_uuid(w, &uuid)),
            hex::decode("d825500102030405060708090a0b0c0d0e0f10").unwrap()
        );
    }

    #[test]
    fn object_id_is_32_byte_string() {
        let expected = format!("5820{}", hex_object_id(0xab));
        assert_eq!(
            write_with(|w| write_object_id(w, &object_id(0xab))),
            hex::decode(expected).unwrap()
        );
    }

    // ------------------------------------------------------------------
    // Golden objects (byte-for-byte)
    // ------------------------------------------------------------------

    #[test]
    fn knowledge_element_version_empty_properties_fixture() {
        // Authoritative fixture: spec/vectors/valid/
        // knowledge-element-version-empty-properties.json
        let object = CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
                element_id: ElementId::from_uuid(
                    Uuid::parse_str("7c8e0c81-b9fc-4c31-9974-b9db8fa72e51").unwrap(),
                ),
                type_id: "kat.core/requirement".to_string(),
                lifecycle: Lifecycle::Active,
                properties: vec![],
            }),
        };
        let expected = hex::decode(
            "a400010101020103a400d825507c8e0c81b9fc4c319974b9db8fa72e510174\
             6b61742e636f72652f726571756972656d656e74020003a0",
        )
        .unwrap();
        assert_eq!(canonical_bytes(&object).unwrap(), expected);
    }

    #[test]
    fn knowledge_element_version_with_properties_fixture() {
        // Exercises every scalar PropertyValue variant plus a list, in a
        // canonically ordered property map (encoded-byte order).
        let object = CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
                element_id: ElementId::from_uuid(Uuid::from_u128(1)),
                type_id: "kat.core/requirement".to_string(),
                lifecycle: Lifecycle::Active,
                properties: vec![
                    ("int".to_string(), PropertyValue::Integer(-1)),
                    ("bool".to_string(), PropertyValue::Bool(true)),
                    (
                        "list".to_string(),
                        PropertyValue::List(vec![
                            PropertyValue::Integer(1),
                            PropertyValue::Text("a".to_string()),
                        ]),
                    ),
                    ("null".to_string(), PropertyValue::Null),
                    ("text".to_string(), PropertyValue::Text("hi".to_string())),
                    ("uuid".to_string(), PropertyValue::Uuid(Uuid::from_u128(7))),
                    ("bytes".to_string(), PropertyValue::Bytes(vec![1, 2])),
                ],
            }),
        };
        let expected = format!(
            "a400010101020103a400d82550{element}01746b61742e636f72652f726571756972656d656e74020003a7\
             63696e7420\
             64626f6f6cf5\
             646c69737482016161\
             646e756c6cf6\
             6474657874626869\
             6475756964d82550{uuid7}\
             656279746573420102",
            element = hex_uuid(1),
            uuid7 = hex_uuid(7),
        );
        assert_eq!(
            canonical_bytes(&object).unwrap(),
            hex::decode(expected).unwrap()
        );
    }

    #[test]
    fn relationship_version_fixture() {
        let object = CanonicalObject {
            payload: CanonicalPayload::RelationshipVersion(RelationshipVersion {
                relationship_id: RelationshipId::from_uuid(Uuid::from_u128(1)),
                source_element_id: ElementId::from_uuid(Uuid::from_u128(2)),
                relationship_type: "kat.core/addresses".to_string(),
                target_element_id: ElementId::from_uuid(Uuid::from_u128(3)),
                properties: vec![],
            }),
        };
        let expected = format!(
            "a400010102020103a5\
             00d82550{rel}\
             01d82550{src}\
             02726b61742e636f72652f616464726573736573\
             03d82550{tgt}\
             04a0",
            rel = hex_uuid(1),
            src = hex_uuid(2),
            tgt = hex_uuid(3),
        );
        assert_eq!(
            canonical_bytes(&object).unwrap(),
            hex::decode(expected).unwrap()
        );
    }

    #[test]
    fn ontology_version_fixture() {
        let object = CanonicalObject {
            payload: CanonicalPayload::OntologyVersion(OntologyVersion {
                ontology_id: OntologyId::from_uuid(Uuid::from_u128(9)),
                element_types: vec![ElementTypeDefinition {
                    type_id: "kat.core/requirement".to_string(),
                    name: "Requirement".to_string(),
                }],
                relationship_types: vec![RelationshipTypeDefinition {
                    type_id: "kat.core/addresses".to_string(),
                    name: "Addresses".to_string(),
                    allowed_source_types: vec!["kat.core/design-decision".to_string()],
                    allowed_target_types: vec!["kat.core/requirement".to_string()],
                }],
            }),
        };
        let expected = format!(
            "a400010105020103a3\
             00d82550{oid}\
             0181a200746b61742e636f72652f726571756972656d656e74016b526571756972656d656e74\
             0281a400726b61742e636f72652f6164647265737365730169416464726573736573\
             028178186b61742e636f72652f64657369676e2d6465636973696f6e0381746b61742e636f72652f726571756972656d656e74",
            oid = hex_uuid(9),
        );
        assert_eq!(
            canonical_bytes(&object).unwrap(),
            hex::decode(expected).unwrap()
        );
    }

    #[test]
    fn semantic_state_fixture() {
        let object = CanonicalObject {
            payload: CanonicalPayload::SemanticState(SemanticState {
                ontology_version: object_id(0),
                elements: vec![ElementStateEntry {
                    element_id: ElementId::from_uuid(Uuid::from_u128(1)),
                    version: object_id(1),
                }],
                relationships: vec![RelationshipStateEntry {
                    relationship_id: RelationshipId::from_uuid(Uuid::from_u128(2)),
                    version: object_id(2),
                }],
            }),
        };
        let expected = format!(
            "a400010104020103a3\
             005820{oid0}\
             018182d82550{elem}5820{oid1}\
             028182d82550{rel}5820{oid2}",
            oid0 = hex_object_id(0),
            oid1 = hex_object_id(1),
            oid2 = hex_object_id(2),
            elem = hex_uuid(1),
            rel = hex_uuid(2),
        );
        assert_eq!(
            canonical_bytes(&object).unwrap(),
            hex::decode(expected).unwrap()
        );
    }

    #[test]
    fn change_revision_fixture() {
        let object = CanonicalObject {
            payload: CanonicalPayload::ChangeRevision(ChangeRevision {
                change_id: ChangeId::from_uuid(Uuid::from_u128(4)),
                base_states: vec![object_id(1)],
                result_state: object_id(2),
                operations: vec![Operation::CreateElement {
                    new_version: object_id(3),
                }],
                dependencies: vec![],
                description: None,
            }),
        };
        let expected = format!(
            "a400010103020103a5\
             00d82550{change}\
             01815820{oid1}\
             025820{oid2}\
             038182015820{oid3}\
             0480",
            change = hex_uuid(4),
            oid1 = hex_object_id(1),
            oid2 = hex_object_id(2),
            oid3 = hex_object_id(3),
        );
        assert_eq!(
            canonical_bytes(&object).unwrap(),
            hex::decode(expected).unwrap()
        );
    }

    // ------------------------------------------------------------------
    // Fail-closed behavior
    // ------------------------------------------------------------------

    #[test]
    fn canonical_bytes_refuses_structurally_invalid_objects() {
        let object = CanonicalObject {
            payload: CanonicalPayload::SemanticState(SemanticState {
                ontology_version: object_id(0),
                elements: vec![
                    ElementStateEntry {
                        element_id: ElementId::from_uuid(Uuid::from_u128(2)),
                        version: object_id(2),
                    },
                    ElementStateEntry {
                        element_id: ElementId::from_uuid(Uuid::from_u128(1)),
                        version: object_id(1),
                    },
                ],
                relationships: vec![],
            }),
        };
        assert_eq!(
            canonical_bytes(&object),
            Err(EncodingError::InvalidCanonicalStructure(
                CanonicalStructureError::SemanticElementsUnordered
            ))
        );
    }
}
