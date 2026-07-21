//! Self-identity resolver for the matcher's self-suppression rule
//! (milestone 110, FR-015, design doc §7.1).
//!
//! Phase 2: stub. The resolver-ladder priority order (operator override →
//! cmake → cargo → npm → PEP 621 → git remote) is implemented in Phase 6
//! (T058–T062, US4). For Phase 2, this module exists so dependent types
//! compile; `matches_record()` returns `false` unconditionally and the
//! matcher behaves as if no self-identity is ever resolved.

use waybill_common::types::purl::Purl;

use super::record::CorpusRecordV2;

/// The scanned project's own identity, used to suppress matcher emissions
/// that would otherwise identify the project as a third-party dep of
/// itself (the "openssl source tree contains openssl" footgun).
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[allow(dead_code)] // Phase 2: declared; Phase 6 (US4) wires the resolver.
pub(crate) struct SelfIdentity {
    /// Bare library name extracted from cmake `project()` /
    /// cargo `[package].name` / npm `package.json::name` /
    /// PEP 621 `[project].name` / `--scan-as` override.
    pub bare_name: Option<String>,
    /// Full PURL when resolvable (e.g., git remote + cargo).
    pub purl: Option<Purl>,
}

#[allow(dead_code)]
impl SelfIdentity {
    /// Whether this self-identity matches the given corpus record's
    /// canonical PURL or any of its alias PURLs (case-insensitive name
    /// + namespace comparison per research R8).
    ///
    /// Phase 2 stub: returns `false` unconditionally. Phase 6 (T059)
    /// implements the actual matching logic.
    pub(crate) fn matches_record(&self, _record: &CorpusRecordV2) -> bool {
        false
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn default_self_identity_matches_nothing() {
        let identity = SelfIdentity::default();
        assert!(identity.bare_name.is_none());
        assert!(identity.purl.is_none());
    }

    #[test]
    fn stub_matches_record_returns_false() {
        // Phase 2 stub: always false. Phase 6 will replace this with the
        // real matching logic. This test pins the current behavior so
        // anyone touching the stub before Phase 6 sees an obvious failure.
        let identity = SelfIdentity {
            bare_name: Some("openssl".to_string()),
            purl: None,
        };
        let record_json = r#"{
          "id": "test",
          "purl": "pkg:github/openssl/openssl@openssl-3.1.4",
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
            "extracted_from": "test",
            "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "extraction_toolchain": "test",
            "extracted_at": "2026-06-01T12:00:00Z"
          },
          "schema_version": 2
        }"#;
        let record: CorpusRecordV2 = serde_json::from_str(record_json).unwrap();
        assert!(!identity.matches_record(&record), "stub MUST return false in Phase 2");
    }
}
