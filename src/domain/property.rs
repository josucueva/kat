//! Canonical property values (see `spec/canonical-format.cddl`, `property-value`).

use uuid::Uuid;

/// A canonical property value.
///
/// Exactly the variants supported by the CDDL; floating-point values are
/// intentionally not representable in v0.1. Maps are represented as ordered
/// `(key, value)` pairs so that malformed input such as duplicate or unsorted
/// keys remains observable to the canonical validator instead of being
/// silently normalized or dropped at construction time.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
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
}
