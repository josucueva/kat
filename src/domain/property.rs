//! Canonical property values (see `spec/canonical-format.cddl`, `property-value`).

use std::fmt;

use uuid::Uuid;

/// A canonical property value.
///
/// Exactly the variants supported by the CDDL; floating-point values are
/// intentionally not representable in v0.1. Maps are represented as ordered
/// `(key, value)` pairs so that malformed input such as duplicate or unsorted
/// keys remains observable to the canonical validator instead of being
/// silently normalized or dropped at construction time.
#[derive(
    Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub enum PropertyValue {
    /// `null`
    Null,
    /// `bool`
    Bool(bool),
    /// `int`
    Integer(i64),
    /// `tstr`
    Text(String),
    /// `bstr`
    Bytes(Vec<u8>),
    /// `uuid` (CBOR tag 37)
    Uuid(Uuid),
    /// `property-list`; element order is preserved
    List(Vec<PropertyValue>),
    /// `property-map`; ordered pairs preserve malformed duplicates for validation
    Map(Vec<(String, PropertyValue)>),
}

impl fmt::Display for PropertyValue {
    /// Boring, deterministic human-oriented rendering for CLI display.
    ///
    /// This is **not** a canonical format (the canonical form is CBOR, see
    /// `spec/canonical-format.cddl`); it exists only so the CLI can render
    /// property values without owning presentation logic.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropertyValue::Null => f.write_str("null"),
            PropertyValue::Bool(b) => write!(f, "{b}"),
            PropertyValue::Integer(i) => write!(f, "{i}"),
            PropertyValue::Text(t) => f.write_str(t),
            PropertyValue::Bytes(b) => {
                f.write_str("0x")?;
                for byte in b {
                    write!(f, "{:02x}", byte)?;
                }
                Ok(())
            }
            PropertyValue::Uuid(u) => write!(f, "{u}"),
            PropertyValue::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            PropertyValue::Map(entries) => {
                f.write_str("{")?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                f.write_str("}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_property_value_variants_constructible() {
        let uuid = Uuid::new_v4();
        let value = PropertyValue::Map(vec![
            ("bool".to_string(), PropertyValue::Bool(true)),
            ("bytes".to_string(), PropertyValue::Bytes(vec![1, 2, 3])),
            ("int".to_string(), PropertyValue::Integer(-42)),
            (
                "list".to_string(),
                PropertyValue::List(vec![
                    PropertyValue::Null,
                    PropertyValue::Text("x".to_string()),
                ]),
            ),
            ("null".to_string(), PropertyValue::Null),
            ("text".to_string(), PropertyValue::Text("hello".to_string())),
            ("uuid".to_string(), PropertyValue::Uuid(uuid)),
        ]);

        // Exercise the derived traits.
        let cloned = value.clone();
        assert_eq!(value, cloned);
        assert!(format!("{value:?}").contains("hello"));

        // Every variant is present.
        let PropertyValue::Map(entries) = &value else {
            panic!("expected a map");
        };
        assert_eq!(entries.len(), 7);
    }

    #[test]
    fn property_value_display_is_deterministic() {
        let uuid = Uuid::from_u128(7);
        let value = PropertyValue::Map(vec![
            ("bool".to_string(), PropertyValue::Bool(true)),
            ("bytes".to_string(), PropertyValue::Bytes(vec![0x01, 0xff])),
            ("int".to_string(), PropertyValue::Integer(-42)),
            (
                "list".to_string(),
                PropertyValue::List(vec![
                    PropertyValue::Null,
                    PropertyValue::Text("x".to_string()),
                ]),
            ),
            ("null".to_string(), PropertyValue::Null),
            ("text".to_string(), PropertyValue::Text("hello".to_string())),
            ("uuid".to_string(), PropertyValue::Uuid(uuid)),
        ]);

        assert_eq!(
            value.to_string(),
            "{bool: true, bytes: 0x01ff, int: -42, list: [null, x], null: null, text: hello, uuid: 00000000-0000-0000-0000-000000000007}"
        );
    }
}
