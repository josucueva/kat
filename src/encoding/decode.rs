//! Canonical object decoder (step 0.10).
//!
//! The decoder is deliberately **as strict as the encoder** (the mirror of
//! `encoding::cbor::canonical_bytes`). It never repairs or normalizes input:
//! any non-canonical or malformed object is rejected.
//!
//! ```text
//! raw object bytes
//!     ↓
//! strict CBOR decoder        definite lengths, shortest integers,
//!                            canonical map-key order, no duplicate keys,
//!                            valid UTF-8
//!     ↓
//! canonical protocol         envelope, object kinds, exact field sets,
//! structure                  exact tuple lengths, UUID/ObjectId rules
//!     ↓
//! typed CanonicalObject
//!     ↓
//! structural validation      CanonicalValidate: sorted/unique collections,
//!                            minimal cardinality
//! ```
//!
//! [`decode_canonical`] is the public entry point. Decoding errors are a
//! distinct type ([`DecodingError`]) from encoding errors so the two failure
//! domains never blur.

use uuid::Uuid;

use crate::domain::change::ChangeRevision;
use crate::domain::element::{KnowledgeElementVersion, Lifecycle};
use crate::domain::identity::{ChangeId, ElementId, ObjectId, OntologyId, RelationshipId};
use crate::domain::ontology::{ElementTypeDefinition, OntologyVersion, RelationshipTypeDefinition};
use crate::domain::operation::{Operation, RelationshipReconciliation};
use crate::domain::property::PropertyValue;
use crate::domain::relationship::RelationshipVersion;
use crate::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use crate::encoding::object::{
    CanonicalObject, CanonicalPayload, ENVELOPE_VERSION, SCHEMA_VERSION,
};
use crate::encoding::validate::{CanonicalStructureError, CanonicalValidate};

/// Error produced while decoding canonical objects. Distinct from
/// [`crate::encoding::error::EncodingError`]: encoding and decoding are
/// separate failure domains and stay separate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodingError {
    /// Input ended in the middle of a value.
    #[error("unexpected end of CBOR input")]
    UnexpectedEof,
    /// Bytes remain after the top-level canonical object.
    #[error("trailing data after canonical object")]
    TrailingData,
    /// Malformed CBOR (reserved additional info, invalid UTF-8, ...).
    #[error("invalid CBOR data")]
    InvalidCbor,
    /// Well-formed CBOR that is not in the canonical deterministic form.
    #[error("non-canonical CBOR encoding")]
    NonCanonicalEncoding,
    /// A CBOR map contains a duplicate key.
    #[error("duplicate CBOR map key")]
    DuplicateMapKey,
    /// The envelope version is not supported.
    #[error("unsupported envelope version: {0}")]
    UnsupportedEnvelopeVersion(u64),
    /// The object schema version is not supported.
    #[error("unsupported schema version: {0}")]
    UnsupportedSchemaVersion(u64),
    /// The object kind is not a known protocol identifier.
    #[error("unknown object kind: {0}")]
    UnknownObjectKind(u64),
    /// The value does not match its required protocol shape (kind/payload
    /// mismatch, wrong field set, wrong tuple length, wrong value type).
    #[error("invalid object shape")]
    InvalidObjectShape,
    /// A UUID is not tag 37 containing exactly 16 bytes.
    #[error("invalid UUID encoding")]
    InvalidUuid,
    /// An ObjectId is not a byte string of exactly 32 bytes.
    #[error("invalid ObjectId encoding")]
    InvalidObjectId,
    /// An operation is not a known identifier or has the wrong shape.
    #[error("invalid operation")]
    InvalidOperation,
    /// The decoded object is structurally non-canonical.
    #[error("cannot decode structurally non-canonical object: {0}")]
    InvalidCanonicalStructure(CanonicalStructureError),
}

/// Decodes raw bytes into a typed [`CanonicalObject`].
///
/// Runs the strict reader, the protocol-structure decoders, and then the
/// canonical structural validator (so the decoder is as strict as
/// [`crate::encoding::canonical_bytes`]). Fail-closed: non-canonical or
/// malformed input is rejected, never repaired.
pub fn decode_canonical(bytes: &[u8]) -> Result<CanonicalObject, DecodingError> {
    let mut reader = CborReader::new(bytes);
    let value = reader.read_value()?;
    if !reader.is_eof() {
        return Err(DecodingError::TrailingData);
    }
    let object = decode_envelope(&value)?;
    object
        .validate_canonical_structure()
        .map_err(DecodingError::InvalidCanonicalStructure)?;
    Ok(object)
}

/// Strict CBOR primitive reader.
///
/// Enforces RFC 8949 §4.2.1 core deterministic requirements at the byte
/// level: definite lengths only, shortest integer forms, canonical map-key
/// ordering, and no duplicate map keys. It parses into a small typed tree
/// ([`CborValue`]) that the protocol decoders then walk.
struct CborReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> CborReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn is_eof(&self) -> bool {
        self.pos == self.bytes.len()
    }

    fn read_byte(&mut self) -> Result<u8, DecodingError> {
        let byte = self
            .bytes
            .get(self.pos)
            .copied()
            .ok_or(DecodingError::UnexpectedEof)?;
        self.pos += 1;
        Ok(byte)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodingError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(DecodingError::UnexpectedEof)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(DecodingError::UnexpectedEof)?;
        self.pos = end;
        Ok(slice)
    }

    fn read_u16(&mut self) -> Result<u16, DecodingError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, DecodingError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, DecodingError> {
        let b = self.take(8)?;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// Reads an unsigned integer argument, enforcing the shortest form and
    /// rejecting indefinite (additional info 31) and reserved (28-30) forms.
    fn read_uint_arg(&mut self, info: u8) -> Result<u64, DecodingError> {
        match info {
            0..=23 => Ok(u64::from(info)),
            24 => {
                let v = u64::from(self.read_byte()?);
                if v < 24 {
                    return Err(DecodingError::NonCanonicalEncoding);
                }
                Ok(v)
            }
            25 => {
                let v = u64::from(self.read_u16()?);
                if v < 0x100 {
                    return Err(DecodingError::NonCanonicalEncoding);
                }
                Ok(v)
            }
            26 => {
                let v = u64::from(self.read_u32()?);
                if v < 0x1_0000 {
                    return Err(DecodingError::NonCanonicalEncoding);
                }
                Ok(v)
            }
            27 => {
                let v = self.read_u64()?;
                if v < 0x1_0000_0000 {
                    return Err(DecodingError::NonCanonicalEncoding);
                }
                Ok(v)
            }
            28..=30 => Err(DecodingError::InvalidCbor),
            31 => Err(DecodingError::NonCanonicalEncoding), // indefinite
            // Additional info is masked to 5 bits by the caller.
            _ => unreachable!("additional info is 5 bits"),
        }
    }

    /// Reads one CBOR value into a strict tree.
    fn read_value(&mut self) -> Result<CborValue, DecodingError> {
        let initial = self.read_byte()?;
        let major = initial >> 5;
        let info = initial & 0x1f;
        match major {
            0 => Ok(CborValue::UInt(self.read_uint_arg(info)?)),
            1 => Ok(CborValue::NInt(self.read_uint_arg(info)?)),
            2 => {
                let len = usize::try_from(self.read_uint_arg(info)?)
                    .map_err(|_| DecodingError::InvalidCbor)?;
                Ok(CborValue::Bytes(self.take(len)?.to_vec()))
            }
            3 => {
                let len = usize::try_from(self.read_uint_arg(info)?)
                    .map_err(|_| DecodingError::InvalidCbor)?;
                let raw = self.take(len)?;
                let text = std::str::from_utf8(raw).map_err(|_| DecodingError::InvalidCbor)?;
                Ok(CborValue::Text(text.to_string()))
            }
            4 => {
                let len = usize::try_from(self.read_uint_arg(info)?)
                    .map_err(|_| DecodingError::InvalidCbor)?;
                let mut items = Vec::with_capacity(len.min(1024));
                for _ in 0..len {
                    items.push(self.read_value()?);
                }
                Ok(CborValue::Array(items))
            }
            5 => {
                let len = usize::try_from(self.read_uint_arg(info)?)
                    .map_err(|_| DecodingError::InvalidCbor)?;
                let mut entries = Vec::with_capacity(len.min(1024));
                let mut previous_key: Option<Vec<u8>> = None;
                for _ in 0..len {
                    let key_start = self.pos;
                    let key = self.read_value()?;
                    let key_end = self.pos;
                    let value = self.read_value()?;
                    // Canonical map-key ordering (RFC 8949 §4.2.1): keys are
                    // ordered by bytewise comparison of their encoded forms.
                    let key_bytes = self.bytes[key_start..key_end].to_vec();
                    if let Some(previous) = &previous_key {
                        if &key_bytes == previous {
                            return Err(DecodingError::DuplicateMapKey);
                        }
                        if &key_bytes < previous {
                            return Err(DecodingError::NonCanonicalEncoding);
                        }
                    }
                    previous_key = Some(key_bytes);
                    entries.push((key, value));
                }
                Ok(CborValue::Map(entries))
            }
            6 => {
                let tag = self.read_uint_arg(info)?;
                let content = self.read_value()?;
                Ok(CborValue::Tag(tag, Box::new(content)))
            }
            7 => self.read_simple(info),
            _ => unreachable!("major type is 3 bits"),
        }
    }

    /// Major type 7: only `false`, `true`, and `null` are in the canonical
    /// value model; floats and simple values are rejected.
    fn read_simple(&mut self, info: u8) -> Result<CborValue, DecodingError> {
        match info {
            20 => Ok(CborValue::Bool(false)),
            21 => Ok(CborValue::Bool(true)),
            22 => Ok(CborValue::Null),
            _ => Err(DecodingError::InvalidCbor),
        }
    }
}

/// A small strict CBOR value tree. Map keys are preserved so the protocol
/// decoders can enforce exact key sets and shapes.
#[derive(Debug)]
enum CborValue {
    /// Major type 0.
    UInt(u64),
    /// Major type 1: `-1 - magnitude`.
    NInt(u64),
    /// Major type 2.
    Bytes(Vec<u8>),
    /// Major type 3 (valid UTF-8).
    Text(String),
    /// Major type 4.
    Array(Vec<CborValue>),
    /// Major type 5, in canonical key order, duplicates rejected.
    Map(Vec<(CborValue, CborValue)>),
    /// Major type 6.
    Tag(u64, Box<CborValue>),
    /// Major type 7.
    Bool(bool),
    /// Major type 7.
    Null,
}

/// Decodes the canonical object envelope, dispatching to the payload decoder
/// selected by the object kind.
fn decode_envelope(value: &CborValue) -> Result<CanonicalObject, DecodingError> {
    let map = expect_map(value)?;
    check_exact_keys(map, &[0, 1, 2, 3])?;

    let envelope_version = expect_uint(map_get(map, 0)?)?;
    if envelope_version != ENVELOPE_VERSION {
        return Err(DecodingError::UnsupportedEnvelopeVersion(envelope_version));
    }

    let kind = expect_uint(map_get(map, 1)?)?;

    let schema_version = expect_uint(map_get(map, 2)?)?;
    if schema_version != SCHEMA_VERSION {
        return Err(DecodingError::UnsupportedSchemaVersion(schema_version));
    }

    let payload = expect_map(map_get(map, 3)?)?;
    let payload = match kind {
        1 => CanonicalPayload::KnowledgeElementVersion(decode_knowledge_element_version(payload)?),
        2 => CanonicalPayload::RelationshipVersion(decode_relationship_version(payload)?),
        3 => CanonicalPayload::ChangeRevision(decode_change_revision(payload)?),
        4 => CanonicalPayload::SemanticState(decode_semantic_state(payload)?),
        5 => CanonicalPayload::OntologyVersion(decode_ontology_version(payload)?),
        other => return Err(DecodingError::UnknownObjectKind(other)),
    };

    Ok(CanonicalObject { payload })
}

/// Requires a map to contain exactly the given integer keys (no missing, no
/// extra, no duplicate — the reader already rejects duplicate keys).
fn check_exact_keys(map: &[(CborValue, CborValue)], expected: &[u64]) -> Result<(), DecodingError> {
    if map.len() != expected.len() {
        return Err(DecodingError::InvalidObjectShape);
    }
    for (key, _) in map {
        match key {
            CborValue::UInt(u) if expected.contains(u) => {}
            _ => return Err(DecodingError::InvalidObjectShape),
        }
    }
    Ok(())
}

/// Looks up an integer-keyed entry in a protocol map.
fn map_get(map: &[(CborValue, CborValue)], key: u64) -> Result<&CborValue, DecodingError> {
    map.iter()
        .find_map(|(k, v)| match k {
            CborValue::UInt(u) if *u == key => Some(v),
            _ => None,
        })
        .ok_or(DecodingError::InvalidObjectShape)
}

fn expect_map(value: &CborValue) -> Result<&[(CborValue, CborValue)], DecodingError> {
    match value {
        CborValue::Map(entries) => Ok(entries),
        _ => Err(DecodingError::InvalidObjectShape),
    }
}

fn expect_array(value: &CborValue) -> Result<&[CborValue], DecodingError> {
    match value {
        CborValue::Array(items) => Ok(items),
        _ => Err(DecodingError::InvalidObjectShape),
    }
}

fn expect_uint(value: &CborValue) -> Result<u64, DecodingError> {
    match value {
        CborValue::UInt(u) => Ok(*u),
        _ => Err(DecodingError::InvalidObjectShape),
    }
}

fn expect_text(value: &CborValue) -> Result<&str, DecodingError> {
    match value {
        CborValue::Text(s) => Ok(s),
        _ => Err(DecodingError::InvalidObjectShape),
    }
}

/// Requires tag 37 containing exactly 16 bytes.
fn expect_uuid(value: &CborValue) -> Result<Uuid, DecodingError> {
    match value {
        CborValue::Tag(37, inner) => match inner.as_ref() {
            CborValue::Bytes(bytes) if bytes.len() == 16 => {
                let mut arr = [0u8; 16];
                arr.copy_from_slice(bytes);
                Ok(Uuid::from_bytes(arr))
            }
            _ => Err(DecodingError::InvalidUuid),
        },
        _ => Err(DecodingError::InvalidUuid),
    }
}

/// Requires a byte string of exactly 32 bytes.
fn expect_object_id(value: &CborValue) -> Result<ObjectId, DecodingError> {
    match value {
        CborValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(bytes);
            Ok(ObjectId::from_bytes(arr))
        }
        CborValue::Bytes(_) => Err(DecodingError::InvalidObjectId),
        _ => Err(DecodingError::InvalidObjectShape),
    }
}

fn decode_lifecycle(value: u64) -> Result<Lifecycle, DecodingError> {
    match value {
        0 => Ok(Lifecycle::Active),
        1 => Ok(Lifecycle::Deprecated),
        2 => Ok(Lifecycle::Superseded),
        _ => Err(DecodingError::InvalidObjectShape),
    }
}

/// Decodes one property map. Key ordering and duplicates were already
/// enforced by the strict reader.
fn decode_property_map(value: &CborValue) -> Result<Vec<(String, PropertyValue)>, DecodingError> {
    let entries = expect_map(value)?;
    let mut out = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        let key = match key {
            CborValue::Text(s) => s.clone(),
            _ => return Err(DecodingError::InvalidObjectShape),
        };
        out.push((key, decode_property_value(value)?));
    }
    Ok(out)
}

fn decode_property_value(value: &CborValue) -> Result<PropertyValue, DecodingError> {
    match value {
        CborValue::Null => Ok(PropertyValue::Null),
        CborValue::Bool(b) => Ok(PropertyValue::Bool(*b)),
        CborValue::UInt(u) => i64::try_from(*u)
            .map(PropertyValue::Integer)
            .map_err(|_| DecodingError::InvalidObjectShape),
        CborValue::NInt(magnitude) => {
            if *magnitude > i64::MAX as u64 {
                return Err(DecodingError::InvalidObjectShape);
            }
            Ok(PropertyValue::Integer(-1 - *magnitude as i64))
        }
        CborValue::Text(s) => Ok(PropertyValue::Text(s.clone())),
        CborValue::Bytes(b) => Ok(PropertyValue::Bytes(b.clone())),
        CborValue::Tag(_, _) => Ok(PropertyValue::Uuid(expect_uuid(value)?)),
        CborValue::Array(items) => Ok(PropertyValue::List(
            items
                .iter()
                .map(decode_property_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        CborValue::Map(_) => Ok(PropertyValue::Map(decode_property_map(value)?)),
    }
}

fn decode_knowledge_element_version(
    map: &[(CborValue, CborValue)],
) -> Result<KnowledgeElementVersion, DecodingError> {
    check_exact_keys(map, &[0, 1, 2, 3])?;
    Ok(KnowledgeElementVersion {
        element_id: ElementId::from_uuid(expect_uuid(map_get(map, 0)?)?),
        type_id: expect_text(map_get(map, 1)?)?.to_string(),
        lifecycle: decode_lifecycle(expect_uint(map_get(map, 2)?)?)?,
        properties: decode_property_map(map_get(map, 3)?)?,
    })
}

fn decode_relationship_version(
    map: &[(CborValue, CborValue)],
) -> Result<RelationshipVersion, DecodingError> {
    check_exact_keys(map, &[0, 1, 2, 3, 4])?;
    Ok(RelationshipVersion {
        relationship_id: RelationshipId::from_uuid(expect_uuid(map_get(map, 0)?)?),
        source_element_id: ElementId::from_uuid(expect_uuid(map_get(map, 1)?)?),
        relationship_type: expect_text(map_get(map, 2)?)?.to_string(),
        target_element_id: ElementId::from_uuid(expect_uuid(map_get(map, 3)?)?),
        properties: decode_property_map(map_get(map, 4)?)?,
    })
}

fn decode_ontology_version(
    map: &[(CborValue, CborValue)],
) -> Result<OntologyVersion, DecodingError> {
    check_exact_keys(map, &[0, 1, 2])?;
    Ok(OntologyVersion {
        ontology_id: OntologyId::from_uuid(expect_uuid(map_get(map, 0)?)?),
        element_types: expect_array(map_get(map, 1)?)?
            .iter()
            .map(decode_element_type_definition)
            .collect::<Result<Vec<_>, _>>()?,
        relationship_types: expect_array(map_get(map, 2)?)?
            .iter()
            .map(decode_relationship_type_definition)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn decode_element_type_definition(
    value: &CborValue,
) -> Result<ElementTypeDefinition, DecodingError> {
    let map = expect_map(value)?;
    check_exact_keys(map, &[0, 1])?;
    Ok(ElementTypeDefinition {
        type_id: expect_text(map_get(map, 0)?)?.to_string(),
        name: expect_text(map_get(map, 1)?)?.to_string(),
    })
}

fn decode_relationship_type_definition(
    value: &CborValue,
) -> Result<RelationshipTypeDefinition, DecodingError> {
    let map = expect_map(value)?;
    check_exact_keys(map, &[0, 1, 2, 3])?;
    Ok(RelationshipTypeDefinition {
        type_id: expect_text(map_get(map, 0)?)?.to_string(),
        name: expect_text(map_get(map, 1)?)?.to_string(),
        allowed_source_types: decode_text_array(map_get(map, 2)?)?,
        allowed_target_types: decode_text_array(map_get(map, 3)?)?,
    })
}

fn decode_text_array(value: &CborValue) -> Result<Vec<String>, DecodingError> {
    expect_array(value)?
        .iter()
        .map(|v| expect_text(v).map(str::to_string))
        .collect()
}

fn decode_semantic_state(map: &[(CborValue, CborValue)]) -> Result<SemanticState, DecodingError> {
    check_exact_keys(map, &[0, 1, 2])?;
    Ok(SemanticState {
        ontology_version: expect_object_id(map_get(map, 0)?)?,
        elements: expect_array(map_get(map, 1)?)?
            .iter()
            .map(decode_element_state_entry)
            .collect::<Result<Vec<_>, _>>()?,
        relationships: expect_array(map_get(map, 2)?)?
            .iter()
            .map(decode_relationship_state_entry)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn decode_element_state_entry(value: &CborValue) -> Result<ElementStateEntry, DecodingError> {
    let items = expect_array(value)?;
    if items.len() != 2 {
        return Err(DecodingError::InvalidObjectShape);
    }
    Ok(ElementStateEntry {
        element_id: ElementId::from_uuid(expect_uuid(&items[0])?),
        version: expect_object_id(&items[1])?,
    })
}

fn decode_relationship_state_entry(
    value: &CborValue,
) -> Result<RelationshipStateEntry, DecodingError> {
    let items = expect_array(value)?;
    if items.len() != 2 {
        return Err(DecodingError::InvalidObjectShape);
    }
    Ok(RelationshipStateEntry {
        relationship_id: RelationshipId::from_uuid(expect_uuid(&items[0])?),
        version: expect_object_id(&items[1])?,
    })
}

fn decode_change_revision(map: &[(CborValue, CborValue)]) -> Result<ChangeRevision, DecodingError> {
    let has_description = map.iter().any(|(key, _)| matches!(key, CborValue::UInt(5)));
    if has_description {
        check_exact_keys(map, &[0, 1, 2, 3, 4, 5])?;
    } else {
        check_exact_keys(map, &[0, 1, 2, 3, 4])?;
    }
    Ok(ChangeRevision {
        change_id: ChangeId::from_uuid(expect_uuid(map_get(map, 0)?)?),
        base_states: decode_object_id_array(map_get(map, 1)?)?,
        result_state: expect_object_id(map_get(map, 2)?)?,
        operations: expect_array(map_get(map, 3)?)?
            .iter()
            .map(decode_operation)
            .collect::<Result<Vec<_>, _>>()?,
        dependencies: decode_object_id_array(map_get(map, 4)?)?,
        description: if has_description {
            Some(expect_text(map_get(map, 5)?)?.to_string())
        } else {
            None
        },
    })
}

fn decode_object_id_array(value: &CborValue) -> Result<Vec<ObjectId>, DecodingError> {
    expect_array(value)?.iter().map(expect_object_id).collect()
}

/// Decodes one tagged operation array, enforcing the exact tuple length for
/// the operation identifier.
fn decode_operation(value: &CborValue) -> Result<Operation, DecodingError> {
    let items = expect_array(value)?;
    let kind = match items.first() {
        Some(CborValue::UInt(u)) => *u,
        _ => return Err(DecodingError::InvalidOperation),
    };
    match kind {
        1 => {
            if items.len() != 2 {
                return Err(DecodingError::InvalidOperation);
            }
            Ok(Operation::CreateElement {
                new_version: expect_object_id(&items[1])?,
            })
        }
        2 => {
            if items.len() != 4 {
                return Err(DecodingError::InvalidOperation);
            }
            Ok(Operation::UpdateElement {
                element_id: ElementId::from_uuid(expect_uuid(&items[1])?),
                expected_version: expect_object_id(&items[2])?,
                new_version: expect_object_id(&items[3])?,
            })
        }
        3 => {
            if items.len() != 4 {
                return Err(DecodingError::InvalidOperation);
            }
            Ok(Operation::DeprecateElement {
                element_id: ElementId::from_uuid(expect_uuid(&items[1])?),
                expected_version: expect_object_id(&items[2])?,
                new_version: expect_object_id(&items[3])?,
            })
        }
        4 => {
            if items.len() != 2 {
                return Err(DecodingError::InvalidOperation);
            }
            Ok(Operation::Link {
                new_relationship_version: expect_object_id(&items[1])?,
            })
        }
        5 => {
            if items.len() != 3 {
                return Err(DecodingError::InvalidOperation);
            }
            Ok(Operation::Unlink {
                relationship_id: RelationshipId::from_uuid(expect_uuid(&items[1])?),
                expected_version: expect_object_id(&items[2])?,
            })
        }
        6 => {
            if items.len() != 6 {
                return Err(DecodingError::InvalidOperation);
            }
            Ok(Operation::Supersede {
                existing_element: ElementId::from_uuid(expect_uuid(&items[1])?),
                expected_existing_version: expect_object_id(&items[2])?,
                replacement_element: ElementId::from_uuid(expect_uuid(&items[3])?),
                replacement_version: expect_object_id(&items[4])?,
                superseding_relationship: expect_object_id(&items[5])?,
            })
        }
        7 => {
            if items.len() != 3 {
                return Err(DecodingError::InvalidOperation);
            }
            let artifact_id = ElementId::from_uuid(expect_uuid(&items[1])?);
            let recon_items = expect_array(&items[2])?;
            let mut reconciliations = Vec::with_capacity(recon_items.len());
            for recon_val in recon_items {
                let fields = expect_array(recon_val)?;
                if fields.len() != 4 {
                    return Err(DecodingError::InvalidOperation);
                }
                reconciliations.push(RelationshipReconciliation {
                    relationship_id: RelationshipId::from_uuid(expect_uuid(&fields[0])?),
                    expected_relationship_version: expect_object_id(&fields[1])?,
                    target_element_id: ElementId::from_uuid(expect_uuid(&fields[2])?),
                    reconciled_target_version: expect_object_id(&fields[3])?,
                });
            }
            Ok(Operation::AccountArtifact {
                artifact_id,
                reconciliations,
            })
        }
        _ => Err(DecodingError::InvalidOperation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::canonical_bytes;
    use crate::encoding::object::{CanonicalObject, CanonicalPayload};
    use crate::encoding::validate::CanonicalStructureError;

    fn element_id(n: u8) -> ElementId {
        ElementId::from_uuid(Uuid::from_u128(n as u128))
    }

    fn relationship_id(n: u8) -> RelationshipId {
        RelationshipId::from_uuid(Uuid::from_u128(n as u128))
    }

    fn object_id(n: u8) -> ObjectId {
        ObjectId::from_bytes([n; 32])
    }

    fn ontology_id(n: u8) -> OntologyId {
        OntologyId::from_uuid(Uuid::from_u128(n as u128))
    }

    fn change_id(n: u8) -> ChangeId {
        ChangeId::from_uuid(Uuid::from_u128(n as u128))
    }

    fn round_trip(object: CanonicalObject) {
        let bytes = canonical_bytes(&object).unwrap();
        let decoded = decode_canonical(&bytes).unwrap();
        assert_eq!(decoded, object);
        let reencoded = canonical_bytes(&decoded).unwrap();
        assert_eq!(reencoded, bytes, "re-encode must reproduce the bytes");
    }

    #[test]
    fn round_trips_all_object_kinds() {
        round_trip(CanonicalObject {
            payload: CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
                element_id: element_id(1),
                type_id: "kat.core/requirement".into(),
                lifecycle: Lifecycle::Active,
                properties: vec![("title".into(), PropertyValue::Text("hello".into()))],
            }),
        });
        round_trip(CanonicalObject {
            payload: CanonicalPayload::RelationshipVersion(RelationshipVersion {
                relationship_id: relationship_id(1),
                source_element_id: element_id(1),
                relationship_type: "kat.core/addresses".into(),
                target_element_id: element_id(2),
                properties: vec![],
            }),
        });
        round_trip(CanonicalObject {
            payload: CanonicalPayload::OntologyVersion(OntologyVersion {
                ontology_id: ontology_id(1),
                element_types: vec![ElementTypeDefinition {
                    type_id: "kat.core/requirement".into(),
                    name: "Requirement".into(),
                }],
                relationship_types: vec![],
            }),
        });
        round_trip(CanonicalObject {
            payload: CanonicalPayload::SemanticState(SemanticState {
                ontology_version: object_id(1),
                elements: vec![ElementStateEntry {
                    element_id: element_id(1),
                    version: object_id(2),
                }],
                relationships: vec![RelationshipStateEntry {
                    relationship_id: relationship_id(1),
                    version: object_id(3),
                }],
            }),
        });
        round_trip(CanonicalObject {
            payload: CanonicalPayload::ChangeRevision(ChangeRevision {
                change_id: change_id(1),
                base_states: vec![object_id(1)],
                result_state: object_id(2),
                operations: vec![Operation::CreateElement {
                    new_version: object_id(3),
                }],
                dependencies: vec![],
                description: Some("desc".into()),
            }),
        });
    }

    #[test]
    fn decode_rejects_trailing_data() {
        let object = CanonicalObject {
            payload: CanonicalPayload::SemanticState(SemanticState {
                ontology_version: object_id(1),
                elements: vec![],
                relationships: vec![],
            }),
        };
        let mut bytes = canonical_bytes(&object).unwrap();
        bytes.push(0x00);
        assert_eq!(decode_canonical(&bytes), Err(DecodingError::TrailingData));
    }

    #[test]
    fn decode_rejects_unknown_object_kind() {
        // Envelope {0:1, 1:9, 2:1, 3:{}} with unknown kind 9.
        let bytes = hex::decode("a400010109020103a0").unwrap();
        assert_eq!(
            decode_canonical(&bytes),
            Err(DecodingError::UnknownObjectKind(9))
        );
    }

    #[test]
    fn decode_rejects_invalid_uuid_in_property_context() {
        // knowledge-element-version whose property value is tag 37 + 15 bytes
        // (invalid UUID: must be exactly 16 bytes).
        let mut bytes = vec![
            0xa4, 0x00, 0x01, 0x01, 0x01, 0x02, 0x01, 0x03, 0xa4, 0x00, 0xd8, 0x25, 0x50,
        ];
        bytes.extend_from_slice(&[0xaa; 16]); // element_id
        bytes.extend_from_slice(&[0x01, 0x63, b'r', b'e', b'q']); // type_id "req"
        bytes.extend_from_slice(&[0x02, 0x00]); // lifecycle active
        bytes.extend_from_slice(&[0x03, 0xa1, 0x61, b'u', 0xd8, 0x25, 0x4f]); // property "u": tag37 + len15
        bytes.extend_from_slice(&[0xbb; 15]);
        assert_eq!(decode_canonical(&bytes), Err(DecodingError::InvalidUuid));
    }

    #[test]
    fn decode_rejects_wrong_uuid_tag_in_property_context() {
        // Property value is tag 38 + 16 bytes (only tag 37 is a UUID).
        let mut bytes = vec![
            0xa4, 0x00, 0x01, 0x01, 0x01, 0x02, 0x01, 0x03, 0xa4, 0x00, 0xd8, 0x25, 0x50,
        ];
        bytes.extend_from_slice(&[0xaa; 16]); // element_id
        bytes.extend_from_slice(&[0x01, 0x63, b'r', b'e', b'q']); // type_id "req"
        bytes.extend_from_slice(&[0x02, 0x00]); // lifecycle active
        bytes.extend_from_slice(&[0x03, 0xa1, 0x61, b'u', 0xd8, 0x26, 0x50]); // property "u": tag38 + len16
        bytes.extend_from_slice(&[0xbb; 16]);
        assert_eq!(decode_canonical(&bytes), Err(DecodingError::InvalidUuid));
    }

    #[test]
    fn decode_rejects_structurally_non_canonical_change() {
        // ChangeRevision with an empty base_states array decodes cleanly but
        // is structurally non-canonical; decode_canonical must reject it.
        let mut bytes = vec![
            0xa4, 0x00, 0x01, 0x01, 0x03, 0x02, 0x01, 0x03, 0xa5, 0x00, 0xd8, 0x25, 0x50,
        ];
        bytes.extend_from_slice(&[0xaa; 16]); // change_id
        bytes.extend_from_slice(&[0x01, 0x80]); // base_states: []
        bytes.extend_from_slice(&[0x02, 0x58, 0x20]); // result_state: bstr(32)
        bytes.extend_from_slice(&[0xcc; 32]);
        bytes.extend_from_slice(&[0x03, 0x81, 0x82, 0x01, 0x58, 0x20]); // operations: [[1, bstr(32)]]
        bytes.extend_from_slice(&[0xdd; 32]);
        bytes.extend_from_slice(&[0x04, 0x80]); // dependencies: []
        assert_eq!(
            decode_canonical(&bytes),
            Err(DecodingError::InvalidCanonicalStructure(
                CanonicalStructureError::ChangeBaseStatesEmpty
            ))
        );
    }
}
