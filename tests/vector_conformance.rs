//! Conformance harness for the KAT canonical format.
//!
//! Walks `spec/vectors/valid/*.json`, builds the logical object from each
//! fixture's diagnostic representation, encodes it with `canonical_bytes`,
//! and asserts both the exact canonical bytes (`cbor_hex`) and the derived
//! ObjectId (`object_id`). The stored ObjectIds were independently computed,
//! so these assertions prove the full identity chain against externally
//! derived values.
//!
//! Per `spec/vectors/README.md`, the fixture JSON is test metadata only —
//! never canonical KAT data.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;

use kat::domain::change::ChangeRevision;
use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{ChangeId, ElementId, ObjectId, OntologyId, RelationshipId};
use kat::domain::ontology::{ElementTypeDefinition, OntologyVersion, RelationshipTypeDefinition};
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
use kat::domain::relationship::RelationshipVersion;
use kat::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use kat::encoding::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
// Aliased to avoid colliding with the local JSON `object_id` parser helper.
use kat::encoding::object_id as hash_object_id;

const VALID_VECTORS_DIR: &str = "spec/vectors/valid";

fn valid_vector_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(VALID_VECTORS_DIR)
        .unwrap_or_else(|e| panic!("cannot read {VALID_VECTORS_DIR}: {e}"))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
}

#[test]
fn valid_vectors_encode_to_exact_canonical_bytes() {
    let paths = valid_vector_paths();
    assert!(
        !paths.is_empty(),
        "no valid vectors found under {VALID_VECTORS_DIR}"
    );

    for path in paths {
        let fixture = read_fixture(&path);
        let name = fixture["name"].as_str().unwrap().to_string();
        let object = build_logical_object(&fixture["object"], &name);
        let bytes = canonical_bytes(&object)
            .unwrap_or_else(|e| panic!("{name}: canonical_bytes failed: {e}"));
        assert_eq!(
            hex::encode(&bytes),
            fixture["cbor_hex"].as_str().unwrap(),
            "cbor bytes mismatch for {name}"
        );
        assert_eq!(
            hash_object_id(&bytes).to_string(),
            fixture["object_id"].as_str().unwrap(),
            "object_id mismatch for {name}"
        );
    }
}

fn read_fixture(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn uuid_str(v: &Value) -> Uuid {
    Uuid::parse_str(
        v.as_str()
            .unwrap_or_else(|| panic!("expected uuid string, got {v}")),
    )
    .unwrap_or_else(|e| panic!("bad uuid {v}: {e}"))
}

fn object_id(v: &Value) -> ObjectId {
    let s = v
        .as_str()
        .unwrap_or_else(|| panic!("expected object id string, got {v}"));
    s.parse::<ObjectId>()
        .unwrap_or_else(|e| panic!("bad object id {s}: {e}"))
}

fn lifecycle(s: &str) -> Lifecycle {
    match s {
        "active" => Lifecycle::Active,
        "deprecated" => Lifecycle::Deprecated,
        "superseded" => Lifecycle::Superseded,
        other => panic!("unknown lifecycle {other}"),
    }
}

fn properties(v: &Value) -> Vec<(String, PropertyValue)> {
    v.as_object()
        .unwrap_or_else(|| panic!("properties must be a JSON object: {v}"))
        .iter()
        .map(|(key, value)| (key.clone(), property_value(value)))
        .collect()
}

fn property_value(v: &Value) -> PropertyValue {
    match v {
        Value::Null => PropertyValue::Null,
        Value::Bool(b) => PropertyValue::Bool(*b),
        Value::Number(n) => PropertyValue::Integer(
            n.as_i64()
                .unwrap_or_else(|| panic!("integer out of range: {v}")),
        ),
        Value::String(s) => PropertyValue::Text(s.clone()),
        Value::Array(items) => PropertyValue::List(items.iter().map(property_value).collect()),
        Value::Object(map) => {
            if map.len() == 1 {
                if let Some(u) = map.get("$uuid") {
                    return PropertyValue::Uuid(uuid_str(u));
                }
                if let Some(b) = map.get("$bytes") {
                    let hex = b
                        .as_str()
                        .unwrap_or_else(|| panic!("$bytes must be a hex string: {v}"));
                    return PropertyValue::Bytes(
                        hex::decode(hex).unwrap_or_else(|e| panic!("bad $bytes hex {hex}: {e}")),
                    );
                }
            }
            PropertyValue::Map(
                map.iter()
                    .map(|(key, value)| (key.clone(), property_value(value)))
                    .collect(),
            )
        }
    }
}

fn operation(v: &Value) -> Operation {
    match v["kind"].as_str().unwrap() {
        "create-element" => Operation::CreateElement {
            new_version: object_id(&v["new_version"]),
        },
        "update-element" => Operation::UpdateElement {
            element_id: ElementId::from_uuid(uuid_str(&v["element_id"])),
            expected_version: object_id(&v["expected_version"]),
            new_version: object_id(&v["new_version"]),
        },
        "deprecate-element" => Operation::DeprecateElement {
            element_id: ElementId::from_uuid(uuid_str(&v["element_id"])),
            expected_version: object_id(&v["expected_version"]),
            new_version: object_id(&v["new_version"]),
        },
        "link" => Operation::Link {
            new_relationship_version: object_id(&v["new_relationship_version"]),
        },
        "unlink" => Operation::Unlink {
            relationship_id: RelationshipId::from_uuid(uuid_str(&v["relationship_id"])),
            expected_version: object_id(&v["expected_version"]),
        },
        "supersede" => Operation::Supersede {
            existing_element: ElementId::from_uuid(uuid_str(&v["existing_element"])),
            expected_existing_version: object_id(&v["expected_existing_version"]),
            replacement_element: ElementId::from_uuid(uuid_str(&v["replacement_element"])),
            replacement_version: object_id(&v["replacement_version"]),
            superseding_relationship: object_id(&v["superseding_relationship"]),
        },
        other => panic!("unknown operation kind {other}"),
    }
}

fn build_logical_object(v: &Value, name: &str) -> CanonicalObject {
    let kind = v["kind"]
        .as_str()
        .unwrap_or_else(|| panic!("{name}: object missing kind"));
    let payload = match kind {
        "knowledge-element-version" => {
            CanonicalPayload::KnowledgeElementVersion(KnowledgeElementVersion {
                element_id: ElementId::from_uuid(uuid_str(&v["element_id"])),
                type_id: v["type"].as_str().unwrap().to_string(),
                lifecycle: lifecycle(v["lifecycle"].as_str().unwrap()),
                properties: properties(&v["properties"]),
            })
        }
        "relationship-version" => CanonicalPayload::RelationshipVersion(RelationshipVersion {
            relationship_id: RelationshipId::from_uuid(uuid_str(&v["relationship_id"])),
            source_element_id: ElementId::from_uuid(uuid_str(&v["source_element_id"])),
            relationship_type: v["relationship_type"].as_str().unwrap().to_string(),
            target_element_id: ElementId::from_uuid(uuid_str(&v["target_element_id"])),
            properties: properties(&v["properties"]),
        }),
        "ontology-version" => CanonicalPayload::OntologyVersion(OntologyVersion {
            ontology_id: OntologyId::from_uuid(uuid_str(&v["ontology_id"])),
            element_types: v["element_types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| ElementTypeDefinition {
                    type_id: t["type_id"].as_str().unwrap().to_string(),
                    name: t["name"].as_str().unwrap().to_string(),
                })
                .collect(),
            relationship_types: v["relationship_types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| RelationshipTypeDefinition {
                    type_id: t["type_id"].as_str().unwrap().to_string(),
                    name: t["name"].as_str().unwrap().to_string(),
                    allowed_source_types: t["allowed_source_types"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s.as_str().unwrap().to_string())
                        .collect(),
                    allowed_target_types: t["allowed_target_types"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s.as_str().unwrap().to_string())
                        .collect(),
                })
                .collect(),
        }),
        "semantic-state" => CanonicalPayload::SemanticState(SemanticState {
            ontology_version: object_id(&v["ontology_version"]),
            elements: v["elements"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| ElementStateEntry {
                    element_id: ElementId::from_uuid(uuid_str(&e["element_id"])),
                    version: object_id(&e["version"]),
                })
                .collect(),
            relationships: v["relationships"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| RelationshipStateEntry {
                    relationship_id: RelationshipId::from_uuid(uuid_str(&r["relationship_id"])),
                    version: object_id(&r["version"]),
                })
                .collect(),
        }),
        "change-revision" => CanonicalPayload::ChangeRevision(ChangeRevision {
            change_id: ChangeId::from_uuid(uuid_str(&v["change_id"])),
            base_states: v["base_states"]
                .as_array()
                .unwrap()
                .iter()
                .map(object_id)
                .collect(),
            result_state: object_id(&v["result_state"]),
            operations: v["operations"]
                .as_array()
                .unwrap()
                .iter()
                .map(operation)
                .collect(),
            dependencies: v["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .map(object_id)
                .collect(),
            description: v
                .get("description")
                .map(|d| d.as_str().unwrap().to_string()),
        }),
        other => panic!("{name}: unknown object kind {other}"),
    };
    CanonicalObject { payload }
}
