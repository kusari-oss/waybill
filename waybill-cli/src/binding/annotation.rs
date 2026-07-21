//! Milestone 072 T007 — `waybill:source-document-binding` annotation
//! serialize/deserialize helpers.
//!
//! Two carrier shapes per `contracts/source-document-binding-annotation.md`
//! C-3:
//!
//! - **CDX 1.6** — `components[].properties[]` entry where
//!   `name == "waybill:source-document-binding"` and `value` is the
//!   JSON-encoded `SourceDocumentBinding` (a string, single-line, no
//!   whitespace beyond what `serde_json::to_string` produces).
//! - **SPDX 2.3 / SPDX 3** — wrapped inside the existing
//!   `MikebomAnnotationCommentV1` envelope (`{ "schema":
//!   "waybill-annotation/v1", "field": "waybill:source-document-binding",
//!   "value": <SourceDocumentBinding-as-real-JSON-object> }`), serialized
//!   into `Package.annotations[].comment` (SPDX 2.3) /
//!   `Annotation.statement` (SPDX 3).
//!
//! The CDX side encodes the binding as a JSON-string; the SPDX-envelope
//! side encodes as a real JSON object inside `value`. Both round-trip
//! through these helpers, and the milestone-071 cross-format-parity
//! canonicalization equates the two via `canonicalize_atomic_values`'s
//! "string-encoded JSON looks like JSON → recursively decode" rule
//! (see `parity/extractors/common.rs::canonicalize_atomic_values`).
//!
//! Constant: `BINDING_PROPERTY_NAME = "waybill:source-document-binding"`.
//! This is the stable annotation key per
//! `contracts/source-document-binding-annotation.md` C-7.

use serde_json::Value;

use crate::binding::{BindingError, SourceDocumentBinding};

/// The stable annotation key per
/// `contracts/source-document-binding-annotation.md` C-3 + C-7. Used
/// across CDX `properties[].name`, SPDX 2.3 envelope `field`, SPDX 3
/// envelope `field`.
pub const BINDING_PROPERTY_NAME: &str = "waybill:source-document-binding";

/// Milestone 116 — source-tier main-module property listing produced
/// binary names. Read by `SourceSbomContext::load()` at bind-time to
/// build the `binary_name_to_purl` auto-alias index. See
/// `specs/116-produces-binaries/contracts/property.md`.
pub const PRODUCES_BINARIES_PROPERTY_NAME: &str = "waybill:produces-binaries";

/// Serialize a `SourceDocumentBinding` to the CDX-property-string
/// shape per `contracts/source-document-binding-annotation.md` C-3 CDX
/// 1.6 example: a single-line JSON-encoded string suitable for
/// `properties[].value`. The keys are emitted in serde-derive order
/// (`source_doc_id, hash, strength, reason, algo`); for byte-stable
/// emission across reruns, callers either rely on serde's deterministic
/// derive ordering or pass through the milestone-071
/// `canonicalize_for_compare` helper at parity-test time.
pub fn serialize_to_cdx_property(b: &SourceDocumentBinding) -> Result<String, BindingError> {
    Ok(serde_json::to_string(b)?)
}

/// Deserialize a CDX-property-string-form `SourceDocumentBinding`
/// back into the typed struct. Tolerates arbitrary key order on the
/// wire per `contracts/source-document-binding-annotation.md` C-6.
pub fn deserialize_from_cdx_property(
    serialized: &str,
) -> Result<SourceDocumentBinding, BindingError> {
    Ok(serde_json::from_str(serialized)?)
}

/// Build the SPDX-envelope `value` for a `SourceDocumentBinding`. The
/// caller wraps this inside `MikebomAnnotationCommentV1` (with `field
/// == BINDING_PROPERTY_NAME`) and serializes the envelope into the
/// SPDX 2.3 `Package.annotations[].comment` / SPDX 3
/// `Annotation.statement` carrier.
///
/// Returns a real JSON object (not a JSON-string) since the SPDX
/// carrier nests the value inside another JSON object — no
/// double-stringification needed.
pub fn serialize_to_envelope_value(
    b: &SourceDocumentBinding,
) -> Result<Value, BindingError> {
    Ok(serde_json::to_value(b)?)
}

/// Inverse of `serialize_to_envelope_value` — decode the SPDX-envelope
/// `value` field back to a typed `SourceDocumentBinding`.
pub fn deserialize_from_envelope_value(
    value: &Value,
) -> Result<SourceDocumentBinding, BindingError> {
    Ok(serde_json::from_value(value.clone())?)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::binding::{BindingHash, BindingStrength, SourceDocumentId};

    fn fixture_verified() -> SourceDocumentBinding {
        SourceDocumentBinding {
            source_doc_id: SourceDocumentId {
                sha256: "e".repeat(64),
                iri: Some("https://example.org/sbom/foo-source.cdx.json".to_string()),
            },
            hash: Some(BindingHash::from_hex("a".repeat(64)).unwrap()),
            strength: BindingStrength::Verified,
            reason: None,
            algo: "v1".to_string(),
            alias_from: None,
            alias_to: None,
            alias_source: None,
        }
    }

    fn fixture_unknown() -> SourceDocumentBinding {
        SourceDocumentBinding::unknown(
            SourceDocumentId {
                sha256: "0".repeat(64),
                iri: None,
            },
            "base-layer-system-package",
        )
    }

    /// CDX property serde round-trip: serialize → deserialize →
    /// equality. Confirms wire-shape stability for the CDX side.
    #[test]
    fn cdx_property_round_trip_verified() {
        let b = fixture_verified();
        let s = serialize_to_cdx_property(&b).unwrap();
        // The string is a JSON-encoded object — must start with `{`
        // and end with `}`.
        assert!(s.starts_with('{'));
        assert!(s.ends_with('}'));
        let back = deserialize_from_cdx_property(&s).unwrap();
        assert_eq!(back, b);
    }

    /// CDX property serde round-trip for the `Unknown` strength
    /// shape per `contracts/source-document-binding-annotation.md`
    /// C-4.
    #[test]
    fn cdx_property_round_trip_unknown() {
        let b = fixture_unknown();
        let s = serialize_to_cdx_property(&b).unwrap();
        let back = deserialize_from_cdx_property(&s).unwrap();
        assert_eq!(back, b);
        assert_eq!(back.strength, BindingStrength::Unknown);
        assert_eq!(back.reason.as_deref(), Some("base-layer-system-package"));
    }

    /// SPDX envelope-value round-trip: serialize → deserialize →
    /// equality. The envelope itself (the `MikebomAnnotationCommentV1`
    /// wrapper at `generate/spdx/annotations.rs`) is not exercised
    /// here — only the inner `value` shape, which is what these
    /// helpers own.
    #[test]
    fn envelope_value_round_trip_verified() {
        let b = fixture_verified();
        let v = serialize_to_envelope_value(&b).unwrap();
        // The envelope value is a real JSON object (not a string).
        assert!(v.is_object());
        // The strength field must come through as a JSON string with
        // the snake_case-renamed enum variant.
        assert_eq!(v.get("strength").and_then(|v| v.as_str()), Some("verified"));
        let back = deserialize_from_envelope_value(&v).unwrap();
        assert_eq!(back, b);
    }

    /// CDX-side encodes as a string; SPDX-side encodes as a real
    /// object — but the "logical" content is identical. This test
    /// confirms the equivalence by parsing the CDX string back into
    /// a `Value`, then comparing to the SPDX-side `Value`.
    #[test]
    fn cdx_string_form_logically_equals_envelope_object_form() {
        let b = fixture_verified();
        let cdx_str = serialize_to_cdx_property(&b).unwrap();
        let cdx_as_value: Value = serde_json::from_str(&cdx_str).unwrap();
        let envelope_value = serialize_to_envelope_value(&b).unwrap();
        assert_eq!(cdx_as_value, envelope_value);
    }

    /// Confirm the binding-property-name constant matches the
    /// contract C-3 / C-7 string. Lock against accidental rename.
    #[test]
    fn binding_property_name_constant_locked() {
        assert_eq!(BINDING_PROPERTY_NAME, "waybill:source-document-binding");
    }

    /// Tolerate-arbitrary-key-order: the CDX-side deserializer must
    /// accept a payload whose keys are in non-canonical order
    /// (`reason` first, then `strength`, etc.) — `serde_json::from_str`
    /// is order-insensitive by default; this test explicitly locks
    /// that contract per C-6.
    #[test]
    fn deserializer_tolerates_arbitrary_key_order() {
        let payload = format!(
            r#"{{"reason":"verification-failed","strength":"unknown","algo":"v1","source_doc_id":{{"sha256":"{}"}}}}"#,
            "0".repeat(64),
        );
        let parsed = deserialize_from_cdx_property(&payload).unwrap();
        assert_eq!(parsed.strength, BindingStrength::Unknown);
        assert_eq!(parsed.reason.as_deref(), Some("verification-failed"));
        assert!(parsed.hash.is_none());
    }
}
