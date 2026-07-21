//! `FingerprintRecord` — one library's identity claim (v1, milestone 108).
//! `CorpusRecordV2` — multi-indicator record (v2, milestone 110).
//!
//! Both shapes coexist. v1 records (the legacy milestone-108 corpus) are
//! upgraded to v2 in memory at load time per spec FR-005 + the 2026-06-03
//! /speckit-clarify Q3 clarification — a single SymbolSet indicator with
//! `Confidence::from_pct_in_range_const::<70>()` (the design-doc §7
//! "threshold-met exported symbols" baseline mapping to the `medium`
//! bucket). See `loader.rs::upgrade_v1_to_v2`.
//!
//! v2 records are detected at load time by the presence of the
//! `schema_version` field (v1 records have no such field). This is a
//! record-level detection rather than the plan.md-proposed archive-level
//! `VERSION` file because existing milestone-108 archives have no
//! `VERSION` file and adding one would be a breaking change to the
//! public corpus contract.
//!
//! Schema versioned at v1 in
//! `kusari-sandbox/waybill-fingerprints/schema/fingerprint-record.v1.json`.
//! v2 schema is published at `docs/reference/corpus-record-v2.schema.json`
//! per FR-004. waybill-cli at load time treats records as TRUSTED
//! (sigstore signature verified on the archive itself; per-record
//! defensive validation is FR-001's `#[serde(deny_unknown_fields)]` gate).

use std::collections::BTreeMap;

use waybill_common::types::purl::Purl;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::confidence::Confidence;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum RecordValidationError {
    #[error("record has empty `library` field")]
    EmptyLibrary,
    #[error("record's `target_purl` failed parse: {raw}")]
    InvalidTargetPurl { raw: String },
    #[error("record's `symbols` list is empty")]
    EmptySymbols,
    #[error("record's `min_symbols` is zero")]
    ZeroMinSymbols,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FingerprintRecord {
    pub library: String,
    pub target_purl: String,
    pub symbols: Vec<String>,
    pub min_symbols: u32,
    #[serde(default)]
    pub version_hint: Option<String>,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[allow(dead_code)]
impl FingerprintRecord {
    /// FR-010 defensive validation. Sibling-repo CI catches these at
    /// PR time; this is the runtime fallback for SHA-override paths.
    pub fn validate(&self) -> Result<(), RecordValidationError> {
        if self.library.trim().is_empty() {
            return Err(RecordValidationError::EmptyLibrary);
        }
        if self.symbols.is_empty() {
            return Err(RecordValidationError::EmptySymbols);
        }
        if self.min_symbols == 0 {
            return Err(RecordValidationError::ZeroMinSymbols);
        }
        if Purl::new(&self.target_purl).is_err() {
            return Err(RecordValidationError::InvalidTargetPurl {
                raw: self.target_purl.clone(),
            });
        }
        Ok(())
    }
}

// =====================================================================
// v2 record types (milestone 110)
// =====================================================================

/// Closed set of indicator kinds the v2 matcher understands. Per
/// data-model.md.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // Phase 2: declared; Phase 4 (matcher) consumes.
pub(crate) enum IndicatorKind {
    /// ELF .dynsym / Mach-O LC_SYMTAB / PE IMAGE_EXPORT_DIRECTORY
    ExportedSymbols,
    /// .rodata literals like "OpenSSL 3.1.4"
    VersionString,
    /// .note.gnu.build-id (ELF)
    BuildId,
    /// LC_UUID (Mach-O)
    MachoUuid,
    /// CodeView GUID:age (PE)
    PePdb,
    /// Versioned ELF symbols (OPENSSL_3_0_0, GLIBC_2.34, etc.)
    AbiMarker,
}

#[allow(dead_code)]
impl IndicatorKind {
    /// SBOM-annotation-friendly indicator-kind name. Stable per FR-017.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ExportedSymbols => "exported_symbols",
            Self::VersionString => "version_string",
            Self::BuildId => "build_id",
            Self::MachoUuid => "macho_uuid",
            Self::PePdb => "pe_pdb",
            Self::AbiMarker => "abi_marker",
        }
    }
}

/// Per-indicator spec — the typed shape inside a record's `indicators` map.
/// Tagged via `type` discriminator so JSON looks like
/// `{"type": "symbol-set", "required": [...], "min_match": 8, "confidence_baseline": 0.70}`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "kebab-case")]
#[allow(dead_code)] // Phase 2: declared; Phase 4 (matcher) consumes.
pub(crate) enum IndicatorSpec {
    SymbolSet {
        required: Vec<String>,
        min_match: usize,
        confidence_baseline: Confidence,
        #[serde(default)]
        suppress_when_self_identity_matches: bool,
    },
    RodataLiteral {
        patterns: Vec<String>,
        confidence_baseline: Confidence,
        #[serde(default)]
        suppress_when_self_identity_matches: bool,
    },
    ExactHash {
        sha_or_uuid_set: Vec<String>,
        confidence_baseline: Confidence,
        #[serde(default = "default_suppress_true")]
        suppress_when_self_identity_matches: bool,
    },
}

fn default_suppress_true() -> bool {
    true
}

/// Provenance tier — where the record came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)] // Phase 2: declared.
pub(crate) enum ProvenanceTier {
    /// Tier 1 — extracted from canonical packages (deb/rpm/apk/etc.).
    AutomatedIngestion,
    /// Tier 2 — built from upstream source in a reproducible pipeline.
    ReproducibleBuild,
    /// Tier 3 — hand-curated by a corpus maintainer.
    ManualCuration,
}

/// Provenance metadata for a v2 record. Per data-model.md.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // Phase 2: declared.
pub(crate) struct Provenance {
    pub tier: ProvenanceTier,
    pub extracted_from: String,
    pub extracted_from_sha256: String, // 64-hex; see JSON Schema validation
    pub extraction_toolchain: String,
    pub extracted_at: String, // RFC 3339; serde_json roundtrips chrono if needed later
    #[serde(default = "default_verified_true")]
    pub verified: bool,
}

fn default_verified_true() -> bool {
    true
}

/// Cross-record collision hints.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // Phase 2: declared.
pub(crate) struct CollisionSpec {
    #[serde(default)]
    pub look_alikes: Vec<LookAlike>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // Phase 2: declared.
pub(crate) struct LookAlike {
    pub purl: Purl,
    pub shared_indicators: Vec<IndicatorKind>,
}

/// A v2 corpus record — one (library, version-range, ABI) tuple.
/// Per `contracts/corpus-record-v2.schema.json` + data-model.md.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // Phase 2: declared; Phase 4 (matcher) + Phase 3 (v1-upgrade) consume.
pub(crate) struct CorpusRecordV2 {
    pub id: String,
    pub purl: Purl,
    #[serde(default)]
    pub purl_aliases: Vec<Purl>,
    #[serde(default)]
    pub cpe_candidates: Vec<String>,
    pub version_range: String,
    #[serde(default)]
    pub architectures: Vec<String>,
    #[serde(default)]
    pub abi: Option<String>,
    pub indicators: BTreeMap<IndicatorKind, IndicatorSpec>,
    #[serde(default)]
    pub collision: CollisionSpec,
    pub provenance: Provenance,
    /// MUST be 2 for v2 records. Validation in `validate_v2()`.
    pub schema_version: u8,
}

#[allow(dead_code)]
impl CorpusRecordV2 {
    /// Defensive post-deserialization validation per FR-001. Sigstore
    /// signature verification is the primary trust gate; this catches
    /// stale-cache / unsigned-test-fixture cases.
    pub(crate) fn validate_v2(&self) -> Result<(), CorpusError> {
        if self.schema_version != 2 {
            return Err(CorpusError::WrongSchemaVersion(self.schema_version));
        }
        if self.indicators.is_empty() {
            return Err(CorpusError::NoIndicators);
        }
        Ok(())
    }
}

/// Matcher + corpus error surface. Per data-model.md.
#[derive(Debug, Error)]
#[allow(dead_code)] // Phase 2: declared; Phase 3+ consume.
pub(crate) enum CorpusError {
    #[error("record schema_version {0} not supported; expected 2 (or 1 for backward-compat)")]
    WrongSchemaVersion(u8),
    #[error("record has no indicators")]
    NoIndicators,
    #[error("confidence {0} out of [0.0, 1.0]")]
    ConfidenceOutOfRange(f64),
    #[error("source {source_id} fetch failed: {kind:?}")]
    Fetch {
        source_id: String,
        kind: FetchFailureKind,
    },
    #[error("source {source_id} signature verification failed: {reason}")]
    SignatureFailure { source_id: String, reason: String },
    #[error("malformed record {record_id} in source {source_id}: {detail}")]
    MalformedRecord {
        source_id: String,
        record_id: String,
        detail: String,
    },
}

/// Closed-set categorization of fetch failure modes, for SC-005's
/// actionable-error-message contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Phase 2: declared; Phase 5 (fetch) consumes.
pub(crate) enum FetchFailureKind {
    MissingCredential,
    InvalidCredential,
    NetworkUnreachable,
    ArchiveMalformed,
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn minimal_valid_json() -> &'static str {
        r#"{
          "library": "openssl",
          "target_purl": "pkg:generic/openssl",
          "symbols": ["SSL_CTX_new", "SSL_new", "SSL_free"],
          "min_symbols": 2
        }"#
    }

    #[test]
    fn parses_minimal_valid_record() {
        let r: FingerprintRecord = serde_json::from_str(minimal_valid_json()).unwrap();
        assert_eq!(r.library, "openssl");
        assert_eq!(r.target_purl, "pkg:generic/openssl");
        assert_eq!(r.symbols.len(), 3);
        assert_eq!(r.min_symbols, 2);
        assert!(r.version_hint.is_none());
        assert!(r.variant.is_none());
        assert!(r.notes.is_none());
        r.validate().unwrap();
    }

    #[test]
    fn parses_record_with_optional_fields() {
        let json = r#"{
          "library": "openssl",
          "target_purl": "pkg:generic/openssl",
          "symbols": ["SSL_CTX_new", "SSL_new", "SSL_free"],
          "min_symbols": 2,
          "version_hint": ">=3.0",
          "variant": "libressl",
          "notes": "OpenBSD fork; ABI-compatible with OpenSSL"
        }"#;
        let r: FingerprintRecord = serde_json::from_str(json).unwrap();
        assert_eq!(r.version_hint.as_deref(), Some(">=3.0"));
        assert_eq!(r.variant.as_deref(), Some("libressl"));
        assert!(r.notes.is_some());
        r.validate().unwrap();
    }

    #[test]
    fn rejects_missing_required_field() {
        // Missing `min_symbols`.
        let json = r#"{
          "library": "openssl",
          "target_purl": "pkg:generic/openssl",
          "symbols": ["SSL_CTX_new"]
        }"#;
        assert!(serde_json::from_str::<FingerprintRecord>(json).is_err());
    }

    #[test]
    fn rejects_invalid_purl_in_target_purl() {
        let json = r#"{
          "library": "openssl",
          "target_purl": "not-a-purl",
          "symbols": ["SSL_CTX_new"],
          "min_symbols": 1
        }"#;
        let r: FingerprintRecord = serde_json::from_str(json).unwrap();
        assert!(matches!(
            r.validate(),
            Err(RecordValidationError::InvalidTargetPurl { .. })
        ));
    }

    #[test]
    fn rejects_zero_min_symbols() {
        let json = r#"{
          "library": "openssl",
          "target_purl": "pkg:generic/openssl",
          "symbols": ["SSL_CTX_new"],
          "min_symbols": 0
        }"#;
        let r: FingerprintRecord = serde_json::from_str(json).unwrap();
        assert!(matches!(
            r.validate(),
            Err(RecordValidationError::ZeroMinSymbols)
        ));
    }

    #[test]
    fn rejects_empty_symbols_list() {
        let json = r#"{
          "library": "openssl",
          "target_purl": "pkg:generic/openssl",
          "symbols": [],
          "min_symbols": 1
        }"#;
        let r: FingerprintRecord = serde_json::from_str(json).unwrap();
        assert!(matches!(
            r.validate(),
            Err(RecordValidationError::EmptySymbols)
        ));
    }

    // ============================================================
    // v2 record tests (milestone 110)
    // ============================================================

    fn minimal_valid_v2_json() -> &'static str {
        r#"{
          "id": "openssl-3.1.4-glibc-amd64",
          "purl": "pkg:github/openssl/openssl@openssl-3.1.4",
          "purl_aliases": ["pkg:deb/debian/libssl3@3.1.4-1"],
          "version_range": ">=3.1.4,<3.2.0",
          "architectures": ["x86_64-linux-gnu"],
          "indicators": {
            "exported_symbols": {
              "type": "symbol-set",
              "required": ["SSL_CTX_new", "SSL_new", "SSL_free"],
              "min_match": 2,
              "confidence_baseline": 0.70
            },
            "version_string": {
              "type": "rodata-literal",
              "patterns": ["OpenSSL 3.1.4"],
              "confidence_baseline": 0.95
            }
          },
          "provenance": {
            "tier": "automated-ingestion",
            "extracted_from": "https://deb.debian.org/debian/pool/main/o/openssl/libssl3_3.1.4-1_amd64.deb",
            "extracted_from_sha256": "abc123def456000000000000000000000000000000000000000000000000abcd",
            "extraction_toolchain": "waybill-corpus-builder@v0.3.1",
            "extracted_at": "2026-06-01T12:00:00Z"
          },
          "schema_version": 2
        }"#
    }

    #[test]
    fn parses_minimal_valid_v2_record() {
        let r: CorpusRecordV2 = serde_json::from_str(minimal_valid_v2_json()).unwrap();
        assert_eq!(r.id, "openssl-3.1.4-glibc-amd64");
        assert_eq!(r.purl_aliases.len(), 1);
        assert_eq!(r.indicators.len(), 2);
        assert_eq!(r.schema_version, 2);
        r.validate_v2().unwrap();
    }

    #[test]
    fn rejects_v2_record_with_wrong_schema_version() {
        let json = minimal_valid_v2_json().replace("\"schema_version\": 2", "\"schema_version\": 3");
        let r: CorpusRecordV2 = serde_json::from_str(&json).unwrap();
        assert!(matches!(r.validate_v2(), Err(CorpusError::WrongSchemaVersion(3))));
    }

    #[test]
    fn rejects_v2_record_with_no_indicators() {
        // Strip the indicators map but keep schema_version=2 — validation
        // should reject at validate_v2() (post-deserialization gate).
        let json = r#"{
          "id": "test",
          "purl": "pkg:generic/test",
          "version_range": "unknown",
          "indicators": {},
          "provenance": {
            "tier": "manual-curation",
            "extracted_from": "test",
            "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "extraction_toolchain": "test",
            "extracted_at": "2026-06-01T12:00:00Z"
          },
          "schema_version": 2
        }"#;
        let r: CorpusRecordV2 = serde_json::from_str(json).unwrap();
        assert!(matches!(r.validate_v2(), Err(CorpusError::NoIndicators)));
    }

    #[test]
    fn rejects_v2_record_with_unknown_field_at_top_level() {
        let json = r#"{
          "id": "test",
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
            "extracted_from": "test",
            "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "extraction_toolchain": "test",
            "extracted_at": "2026-06-01T12:00:00Z"
          },
          "schema_version": 2,
          "extra_field_not_in_schema": "should reject"
        }"#;
        // `#[serde(deny_unknown_fields)]` rejects at deserialize time.
        assert!(serde_json::from_str::<CorpusRecordV2>(json).is_err());
    }

    #[test]
    fn indicator_spec_round_trips_via_tagged_enum() {
        let json = r#"{
          "type": "symbol-set",
          "required": ["a", "b"],
          "min_match": 1,
          "confidence_baseline": 0.85
        }"#;
        let spec: IndicatorSpec = serde_json::from_str(json).unwrap();
        match spec {
            IndicatorSpec::SymbolSet { required, min_match, .. } => {
                assert_eq!(required.len(), 2);
                assert_eq!(min_match, 1);
            }
            _ => panic!("expected SymbolSet variant"),
        }
    }

    #[test]
    fn indicator_kind_serializes_snake_case() {
        let kind = IndicatorKind::ExportedSymbols;
        assert_eq!(serde_json::to_string(&kind).unwrap(), "\"exported_symbols\"");
        assert_eq!(IndicatorKind::ExportedSymbols.as_str(), "exported_symbols");
        assert_eq!(IndicatorKind::VersionString.as_str(), "version_string");
        assert_eq!(IndicatorKind::BuildId.as_str(), "build_id");
    }

    #[test]
    fn exact_hash_indicator_defaults_self_suppression_to_true() {
        // Per data-model.md: ExactHash's `suppress_when_self_identity_matches`
        // defaults to true (Build-IDs of the project itself are still useful
        // but for the self-suppression suite, the design treats them as
        // opted-in to suppression by default).
        let json = r#"{
          "type": "exact-hash",
          "sha_or_uuid_set": ["abc123"],
          "confidence_baseline": 0.99
        }"#;
        let spec: IndicatorSpec = serde_json::from_str(json).unwrap();
        match spec {
            IndicatorSpec::ExactHash { suppress_when_self_identity_matches, .. } => {
                assert!(suppress_when_self_identity_matches);
            }
            _ => panic!("expected ExactHash variant"),
        }
    }

    #[test]
    fn symbol_set_indicator_defaults_self_suppression_to_false() {
        // SymbolSet does NOT opt-in by default (operator-overridable per record).
        let json = r#"{
          "type": "symbol-set",
          "required": ["a"],
          "min_match": 1,
          "confidence_baseline": 0.70
        }"#;
        let spec: IndicatorSpec = serde_json::from_str(json).unwrap();
        match spec {
            IndicatorSpec::SymbolSet { suppress_when_self_identity_matches, .. } => {
                assert!(!suppress_when_self_identity_matches);
            }
            _ => panic!("expected SymbolSet variant"),
        }
    }
}
