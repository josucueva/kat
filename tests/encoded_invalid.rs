//! Encoded-invalid vector harness (step 0.10).
//!
//! Walks `spec/vectors/invalid/encoded/*.cbor` and asserts the strict decoder
//! rejects every raw malformed canonical byte sequence. These fixtures were
//! documented as "decoder tests pending" in step 0.5; the decoder now makes
//! them executable.
//!
//! Per `spec/vectors/README.md`, these raw bytes are the negative compatibility
//! contract: a conforming implementation must reject every one.

use std::fs;
use std::path::{Path, PathBuf};

use kat::encoding::decode::DecodingError;
use kat::encoding::decode_canonical;

const ENCODED_INVALID_DIR: &str = "spec/vectors/invalid/encoded";

/// Fixtures whose rejection variant is deterministic when the raw bytes are
/// decoded as a top-level canonical object.
///
/// The two UUID fixtures (`uuid-wrong-length`, `uuid-wrong-tag`) are bare
/// tagged values, so at the top level they are rejected as a non-map object
/// shape; their precise `InvalidUuid` behavior is covered by the decoder unit
/// tests inside a real property-map context.
fn expected_variant(name: &str) -> Option<DecodingError> {
    match name {
        "duplicate-cbor-map-key" => Some(DecodingError::DuplicateMapKey),
        "indefinite-array" => Some(DecodingError::NonCanonicalEncoding),
        "indefinite-map" => Some(DecodingError::NonCanonicalEncoding),
        "malformed-object-id" => Some(DecodingError::InvalidObjectId),
        "non-shortest-integer" => Some(DecodingError::NonCanonicalEncoding),
        "object-kind-payload-mismatch" => Some(DecodingError::InvalidObjectShape),
        "unsupported-envelope-version" => Some(DecodingError::UnsupportedEnvelopeVersion(2)),
        "unsupported-schema-version" => Some(DecodingError::UnsupportedSchemaVersion(2)),
        _ => None,
    }
}

fn encoded_invalid_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(ENCODED_INVALID_DIR)
        .unwrap_or_else(|e| panic!("cannot read {ENCODED_INVALID_DIR}: {e}"))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cbor"))
        .collect();
    paths.sort();
    paths
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .expect("fixture file stem")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn all_encoded_invalid_fixtures_are_rejected() {
    let paths = encoded_invalid_paths();
    assert!(
        !paths.is_empty(),
        "no encoded-invalid vectors found under {ENCODED_INVALID_DIR}"
    );

    for path in paths {
        let name = stem(&path);
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let result = decode_canonical(&bytes);
        assert!(
            result.is_err(),
            "{name}: expected rejection but the decoder accepted the bytes"
        );
        if let Some(expected) = expected_variant(&name) {
            assert_eq!(
                result.unwrap_err(),
                expected,
                "{name}: rejected with an unexpected variant"
            );
        }
    }
}
