//! Dev-only JSON Schema validation harness for milestone-110 v2 corpus records.
//!
//! Per research R5: production code path uses `serde_json::Deserializer` with
//! `#[serde(deny_unknown_fields)]` for strict-shape rejection. This test
//! complements that by validating fixture records against the published JSON
//! Schema (the public contract for third-party corpus authors per FR-004).
//!
//! When fixture corpora are added under
//! `mikebom-cli/tests/fixtures/fingerprints_v2/corpora/`, this test discovers
//! them automatically and validates each.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::fs;
use std::path::{Path, PathBuf};

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("contracts/corpus-record-v2.schema.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fingerprints_v2/corpora")
}

fn load_schema() -> serde_json::Value {
    let raw = fs::read_to_string(schema_path()).expect("schema file present");
    serde_json::from_str(&raw).expect("schema file is valid JSON")
}

/// Discover all `*.json` fixture records under `tests/fixtures/fingerprints_v2/corpora/`.
fn discover_fixture_records(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(path);
        } else if path.is_dir() {
            out.extend(discover_fixture_records(&path));
        }
    }
    out.sort();
    out
}

#[test]
fn schema_file_is_valid_json_schema() {
    // Smoke check: the schema itself parses + has the expected top-level shape.
    let schema = load_schema();
    assert_eq!(
        schema.get("$schema").and_then(|s| s.as_str()),
        Some("https://json-schema.org/draft/2020-12/schema"),
        "schema MUST declare Draft 2020-12"
    );
    assert_eq!(
        schema.get("title").and_then(|s| s.as_str()),
        Some("mikebom fingerprint corpus record (v2)")
    );
}

#[test]
fn all_fixture_records_validate_against_schema() {
    let schema_json = load_schema();
    let validator =
        jsonschema::validator_for(&schema_json).expect("schema compiles to a JSON Schema validator");

    let fixtures = discover_fixture_records(&fixtures_dir());
    assert!(
        !fixtures.is_empty(),
        "expected at least one fixture record under {}",
        fixtures_dir().display()
    );

    let mut failures: Vec<String> = Vec::new();
    for fixture_path in &fixtures {
        let raw = fs::read_to_string(fixture_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", fixture_path.display()));
        let instance: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("parse {}: {e}", fixture_path.display()));
        let validation = validator.validate(&instance);
        if let Err(error) = validation {
            failures.push(format!("{}: {error}", fixture_path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "JSON Schema validation failed for {} fixture(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn schema_rejects_record_missing_required_indicators() {
    let schema_json = load_schema();
    let validator =
        jsonschema::validator_for(&schema_json).expect("schema compiles");
    let bad = serde_json::json!({
        "id": "missing-indicators",
        "purl": "pkg:generic/test",
        "version_range": "unknown",
        "indicators": {},
        "provenance": {
            "tier": "manual-curation",
            "extracted_from": "https://example.com/source",
            "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "extraction_toolchain": "test",
            "extracted_at": "2026-06-01T12:00:00Z"
        },
        "schema_version": 2
    });
    assert!(
        validator.validate(&bad).is_err(),
        "schema should reject empty indicators map per `minProperties: 1`"
    );
}

#[test]
fn schema_rejects_record_with_wrong_schema_version() {
    let schema_json = load_schema();
    let validator =
        jsonschema::validator_for(&schema_json).expect("schema compiles");
    let bad = serde_json::json!({
        "id": "wrong-version",
        "purl": "pkg:generic/test",
        "version_range": "unknown",
        "indicators": {
            "exported_symbols": {
                "type": "symbol-set",
                "required": ["a"],
                "min_match": 1,
                "confidence_baseline": 0.70
            }
        },
        "provenance": {
            "tier": "manual-curation",
            "extracted_from": "https://example.com/source",
            "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "extraction_toolchain": "test",
            "extracted_at": "2026-06-01T12:00:00Z"
        },
        "schema_version": 3
    });
    assert!(
        validator.validate(&bad).is_err(),
        "schema should reject schema_version != 2 per `const: 2`"
    );
}
