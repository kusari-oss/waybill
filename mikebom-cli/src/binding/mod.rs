//! Milestone 072 — cross-tier SBOM binding.
//!
//! Owns the binding-hash algorithm (FR-002), per-component
//! `SourceDocumentBinding` annotation shape (FR-001), per-ecosystem
//! source-input extraction, and consumer-side verification logic
//! (FR-005). Plus the data types backing all of it.
//!
//! Layout:
//!
//! - this file (`mod.rs`) — public types: `BindingHashInputs`,
//!   `BindingHash`, `BindingStrength`, `SourceDocumentId`,
//!   `SourceDocumentBinding`, `VexPropagationMode`. Plus `BindingError`.
//! - `hash.rs` — `compute_binding_hash(inputs) -> BindingHash` per
//!   contracts/binding-hash-v1.md.
//! - `source_inputs.rs` — `extract_source_inputs_for_component(...)
//!   -> BindingHashInputs` dispatching per ecosystem.
//! - `annotation.rs` — JSON serialization helpers for the
//!   `mikebom:source-document-binding` annotation across CDX
//!   property-string form and SPDX envelope-value form.
//! - `verify.rs` — `verify_binding(image, source) -> VerifyReport`
//!   for the `mikebom sbom verify-binding` subcommand.
//!
//! Per Constitution Principle IV, every domain value is a newtype
//! or enum. Production code uses `anyhow::Result` / `BindingError`;
//! test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

pub mod alias;
pub mod annotation;
pub mod hash;
pub mod identifiers;
pub mod source_inputs;
pub mod user_metadata;
pub mod verify;

pub use alias::{parse_pkg_alias, AliasError, AliasMap, PurlAlias};

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

/// Errors emitted by the binding module.
#[derive(Debug, thiserror::Error)]
pub enum BindingError {
    #[error("binding hash hex must be 64 lowercase hex chars, got {0:?}")]
    InvalidHashHex(String),

    #[error("binding annotation JSON parse failed: {0}")]
    AnnotationDecodeJson(#[from] serde_json::Error),

    #[error("binding annotation envelope schema mismatch: expected {expected}, got {got}")]
    EnvelopeSchemaMismatch { expected: String, got: String },

    #[error("binding annotation envelope field mismatch: expected {expected}, got {got:?}")]
    EnvelopeFieldMismatch {
        expected: String,
        got: Option<String>,
    },

    #[error("io error reading source-input file {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("source SBOM at {0} could not be loaded")]
    SourceSbomLoad(String),
}

// ---------------------------------------------------------------------
// Data types (data-model.md)
// ---------------------------------------------------------------------

/// FR-002 layered binding hash inputs. Each side is `Option<String>`
/// because not every project carries every input (e.g., maven has no
/// canonical lockfile).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingHashInputs {
    /// VCS commit identifier (40-char SHA-1 hex from `git rev-parse HEAD`,
    /// Go BuildInfo `vcs.revision`, cargo-auditable embedded VCS, etc.).
    /// Tolerant of any string — we don't validate VCS-format here.
    pub vcs: Option<String>,
    /// Lowercase hex SHA-256 of the project's lockfile bytes
    /// (Cargo.lock / package-lock.json / Gemfile.lock / go.sum /
    /// poetry.lock / requirements*.txt's `--hash=` content).
    pub lockfile: Option<String>,
    /// Lowercase hex SHA-256 of the project's top-level manifest bytes
    /// as on disk (Cargo.toml / package.json / pom.xml / *.gemspec /
    /// pyproject.toml / go.mod). No re-serialization before hashing.
    pub manifest: Option<String>,
}

impl BindingHashInputs {
    /// Empty input set — no evidence at all. Caller emits `Unknown`
    /// strength with `reason: "no-evidence"`.
    pub fn empty() -> Self {
        Self {
            vcs: None,
            lockfile: None,
            manifest: None,
        }
    }

    /// Count populated sides. Drives `BindingStrength` derivation.
    pub fn populated_count(&self) -> usize {
        self.vcs.is_some() as usize
            + self.lockfile.is_some() as usize
            + self.manifest.is_some() as usize
    }
}

/// FR-002 layered binding hash output. Newtype around the
/// lowercase-hex SHA-256 string. Construction is validated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct BindingHash(String);

impl BindingHash {
    /// Construct from a 64-character lowercase hex string. Returns
    /// `BindingError::InvalidHashHex` on malformed input.
    pub fn from_hex(hex: impl Into<String>) -> Result<Self, BindingError> {
        let s = hex.into();
        if s.len() != 64 {
            return Err(BindingError::InvalidHashHex(s));
        }
        if !s.chars().all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f')) {
            return Err(BindingError::InvalidHashHex(s));
        }
        Ok(Self(s))
    }

    /// Borrow the hex representation.
    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

/// FR-012 cross-tier identity strength. Derived from
/// `BindingHashInputs::populated_count()` plus verification status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStrength {
    /// All three sides populated AND match source-tier recomputation.
    Verified,
    /// Exactly two sides populated AND both match.
    Weak,
    /// Fewer than two sides populated, OR any present side fails to
    /// match. Always paired with a non-empty `reason` per FR-003.
    Unknown,
}

impl BindingStrength {
    /// Pre-verification strength derivation. After verification, the
    /// caller may downgrade to `Unknown` if any side fails to match
    /// regardless of populated_count.
    pub fn from_inputs(inputs: &BindingHashInputs) -> Self {
        match inputs.populated_count() {
            3 => Self::Verified,
            2 => Self::Weak,
            _ => Self::Unknown,
        }
    }
}

/// Milestone 116 — provenance of an applied PURL alias on a binding result.
///
/// Carried on the `SourceDocumentBinding.alias_source` field whenever
/// `alias_from` / `alias_to` are populated (paired-presence invariant).
/// Lets auditors distinguish operator-supplied aliases (milestone 111's
/// `--pkg-alias` flag) from automatic aliases derived at bind-time from
/// the source-tier `mikebom:produces-binaries` declaration (milestone 116).
///
/// Pre-feature SBOMs that carry milestone-111 aliases without this field
/// deserialize cleanly via `#[serde(default)]`; consumers SHOULD interpret
/// absent `alias_source` as implicitly `OperatorSupplied` (only possible
/// source pre-feature). Post-feature, every alias-bearing envelope
/// populates this field explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AliasSource {
    /// `--pkg-alias` flag (or `MIKEBOM_PKG_ALIAS` env var). Milestone 111.
    OperatorSupplied,
    /// Source-tier `mikebom:produces-binaries` declaration auto-aliased the
    /// image-tier `pkg:generic/<name>` to the source-tier ecosystem PURL.
    /// Milestone 116.
    AutomaticFromProducesBinaries,
}

/// Stable identifier for the source-tier SBOM document.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceDocumentId {
    /// SHA-256 hex of the canonical source SBOM bytes.
    /// Verifier-computable.
    pub sha256: String,
    /// Optional IRI (URL, urn:uuid:..., file path) for human-readable
    /// cross-reference. May be a local file path during local CI runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iri: Option<String>,
}

/// FR-001 per-component binding annotation payload.
///
/// Carried on every non-source-tier component (i.e.,
/// `mikebom:sbom-tier: build` or `deployed`) in:
///
/// - CDX `properties[]` with `name == "mikebom:source-document-binding"`
///   and `value == JSON-encoded SourceDocumentBinding`.
/// - SPDX 2.3 `Package.annotations[].comment` wrapped in the existing
///   `MikebomAnnotationCommentV1` envelope.
/// - SPDX 3 `Annotation.statement` with the same envelope shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceDocumentBinding {
    /// Pointer to the source-tier SBOM document.
    pub source_doc_id: SourceDocumentId,
    /// Per-component layered hash. `None` when `strength == Unknown`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<BindingHash>,
    /// Cross-tier identity strength.
    pub strength: BindingStrength,
    /// FR-003 transparency: explicit reason for `Unknown` strength.
    /// Optional for `Verified` / `Weak`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Algorithm version. Always `"v1"` for milestone 072. Milestone
    /// 111's additive `alias_from`/`alias_to` fields do NOT bump this
    /// version — they are metadata, not algorithm changes
    /// (specs/111-pkg-alias-binding/research.md §2).
    #[serde(default = "default_algo_v1")]
    pub algo: String,
    /// Milestone 111 — when this binding result was reached via a
    /// `--pkg-alias` declaration, the LHS PURL the operator declared.
    /// `None` otherwise. MUST be `Some` iff `alias_to` is `Some`
    /// (paired-presence invariant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias_from: Option<mikebom_common::types::purl::Purl>,
    /// Milestone 111 — pair of `alias_from`. When `Some`, this is the
    /// RHS PURL the binder matched against in the bind-source SBOM.
    /// Paired-presence invariant with `alias_from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias_to: Option<mikebom_common::types::purl::Purl>,
    /// Milestone 116 — provenance of the applied alias. `Some(_)` iff
    /// `alias_from` is `Some` (extended paired-presence invariant). Pre-
    /// milestone-116 SBOMs lack this field and deserialize as `None`;
    /// consumers SHOULD interpret absent on an alias-bearing binding as
    /// implicit [`AliasSource::OperatorSupplied`] (only possible pre-116).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias_source: Option<AliasSource>,
}

fn default_algo_v1() -> String {
    "v1".to_string()
}

impl SourceDocumentBinding {
    /// Helper: synthesize the `Unknown` shape with a reason. Use for
    /// the FR-003 transparency path (e.g., base-layer system package,
    /// sideloaded binary).
    pub fn unknown(source_doc_id: SourceDocumentId, reason: impl Into<String>) -> Self {
        Self {
            source_doc_id,
            hash: None,
            strength: BindingStrength::Unknown,
            reason: Some(reason.into()),
            algo: default_algo_v1(),
            alias_from: None,
            alias_to: None,
            alias_source: None,
        }
    }
}

/// FR-007 propagation mode for `mikebom sbom enrich --vex-propagation-mode`.
/// (Used by US2 milestone-072 work; declared here so the foundational
/// data model is complete and present from PR-A.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum VexPropagationMode {
    /// Pre-072 behavior — propagate by PURL match without binding check.
    Permissive,
    /// Default in milestone 072. Propagate but tag binding-unverified
    /// statements with a structured caveat.
    #[default]
    Caveated,
    /// Refuse propagation when binding strength != Verified.
    Strict,
}

// ---------------------------------------------------------------------
// Public re-exports
// ---------------------------------------------------------------------
//
// Re-exports are added incrementally as each submodule lands:
//   - T005 → `pub use hash::compute_binding_hash;`
//   - T006 → `pub use source_inputs::extract_source_inputs_for_component;`
//   - T007 → annotation helpers
//   - T015 → `pub use verify::{verify_binding, VerifyReport, VerifyRow};`
//
// Through PR-A T002 the submodules are present (so the directory layout
// is committed) but stay empty until their respective tasks land.

pub use annotation::{
    deserialize_from_cdx_property, deserialize_from_envelope_value,
    serialize_to_cdx_property, serialize_to_envelope_value, BINDING_PROPERTY_NAME,
    PRODUCES_BINARIES_PROPERTY_NAME,
};
pub use hash::{compute_binding_hash, BINDING_HASH_ALGO_V1};
pub use source_inputs::{extract_source_inputs, BindingEcosystem};
pub use verify::{
    compute_binding_for_source_tree, verify_binding, verify_binding_from_paths, SourceSbomContext,
    VerifyReport, VerifyRow, VerifySummary,
};

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn populated_count_zero_one_two_three() {
        let z = BindingHashInputs::empty();
        assert_eq!(z.populated_count(), 0);

        let one = BindingHashInputs {
            vcs: Some("abc".into()),
            ..BindingHashInputs::empty()
        };
        assert_eq!(one.populated_count(), 1);

        let two = BindingHashInputs {
            vcs: Some("abc".into()),
            lockfile: Some("def".into()),
            manifest: None,
        };
        assert_eq!(two.populated_count(), 2);

        let three = BindingHashInputs {
            vcs: Some("abc".into()),
            lockfile: Some("def".into()),
            manifest: Some("ghi".into()),
        };
        assert_eq!(three.populated_count(), 3);
    }

    #[test]
    fn binding_strength_derive_from_populated_count() {
        let three = BindingHashInputs {
            vcs: Some("a".into()),
            lockfile: Some("b".into()),
            manifest: Some("c".into()),
        };
        assert_eq!(
            BindingStrength::from_inputs(&three),
            BindingStrength::Verified
        );

        let two = BindingHashInputs {
            vcs: Some("a".into()),
            lockfile: Some("b".into()),
            manifest: None,
        };
        assert_eq!(BindingStrength::from_inputs(&two), BindingStrength::Weak);

        let one = BindingHashInputs {
            vcs: Some("a".into()),
            ..BindingHashInputs::empty()
        };
        assert_eq!(
            BindingStrength::from_inputs(&one),
            BindingStrength::Unknown
        );

        let zero = BindingHashInputs::empty();
        assert_eq!(
            BindingStrength::from_inputs(&zero),
            BindingStrength::Unknown
        );
    }

    #[test]
    fn binding_hash_from_hex_validates_length_and_chars() {
        // 64 lowercase hex chars — accepted.
        let valid = "0".repeat(64);
        assert!(BindingHash::from_hex(valid).is_ok());

        // Wrong length — rejected.
        assert!(BindingHash::from_hex("abc").is_err());

        // Uppercase hex — rejected (must be lowercase per contract C-3).
        let upper = "A".repeat(64);
        assert!(BindingHash::from_hex(upper).is_err());

        // Non-hex chars — rejected.
        let bad = format!("{}{}", "a".repeat(63), "z");
        assert!(BindingHash::from_hex(bad).is_err());
    }

    #[test]
    fn unknown_helper_produces_correct_shape() {
        let id = SourceDocumentId {
            sha256: "deadbeef".into(),
            iri: None,
        };
        let b = SourceDocumentBinding::unknown(id.clone(), "no-evidence");
        assert_eq!(b.source_doc_id, id);
        assert_eq!(b.strength, BindingStrength::Unknown);
        assert_eq!(b.reason.as_deref(), Some("no-evidence"));
        assert!(b.hash.is_none());
        assert_eq!(b.algo, "v1");
    }

    #[test]
    fn vex_propagation_mode_defaults_to_caveated() {
        assert_eq!(
            VexPropagationMode::default(),
            VexPropagationMode::Caveated
        );
    }

    // ──────────────────────────────────────────────────────────────
    // Milestone 111 — wire-compatibility tests for the extended
    // SourceDocumentBinding envelope (alias_from / alias_to additive
    // fields). Per specs/111-pkg-alias-binding/research.md §2.
    // ──────────────────────────────────────────────────────────────

    fn fixture_no_alias() -> SourceDocumentBinding {
        SourceDocumentBinding {
            source_doc_id: SourceDocumentId {
                sha256: "e".repeat(64),
                iri: None,
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

    fn fixture_with_alias() -> SourceDocumentBinding {
        use mikebom_common::types::purl::Purl;
        SourceDocumentBinding {
            source_doc_id: SourceDocumentId {
                sha256: "e".repeat(64),
                iri: None,
            },
            hash: Some(BindingHash::from_hex("a".repeat(64)).unwrap()),
            strength: BindingStrength::Verified,
            reason: None,
            algo: "v1".to_string(),
            alias_from: Some(Purl::new("pkg:generic/baz").unwrap()),
            alias_to: Some(Purl::new("pkg:cargo/baz@1.0.0").unwrap()),
            alias_source: Some(AliasSource::OperatorSupplied),
        }
    }

    #[test]
    fn envelope_without_alias_omits_alias_fields_in_serialization() {
        // SC-004 byte-identity guarantee: when no alias is present,
        // the serialized form MUST NOT include alias_from / alias_to
        // keys. Pre-milestone-111 SBOM consumers see the same wire
        // shape they always have.
        let b = fixture_no_alias();
        let s = serde_json::to_string(&b).unwrap();
        assert!(
            !s.contains("alias_from"),
            "no-alias envelope must NOT emit alias_from; got {s}"
        );
        assert!(
            !s.contains("alias_to"),
            "no-alias envelope must NOT emit alias_to; got {s}"
        );
    }

    #[test]
    fn envelope_with_alias_includes_both_fields_in_serialization() {
        let b = fixture_with_alias();
        let s = serde_json::to_string(&b).unwrap();
        assert!(s.contains("\"alias_from\":\"pkg:generic/baz\""));
        assert!(s.contains("\"alias_to\":\"pkg:cargo/baz@1.0.0\""));
    }

    #[test]
    fn envelope_round_trips_with_alias() {
        let b = fixture_with_alias();
        let s = serde_json::to_string(&b).unwrap();
        let parsed: SourceDocumentBinding = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, b, "round-trip MUST preserve alias_from/alias_to");
    }

    #[test]
    fn envelope_round_trips_without_alias() {
        let b = fixture_no_alias();
        let s = serde_json::to_string(&b).unwrap();
        let parsed: SourceDocumentBinding = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, b);
        assert!(parsed.alias_from.is_none());
        assert!(parsed.alias_to.is_none());
    }

    #[test]
    fn envelope_deserializes_pre_feature_shape_with_no_alias_fields() {
        // Wire-compat: pre-milestone-111 SBOMs (without alias_from /
        // alias_to keys) MUST deserialize successfully into the new
        // struct shape, with both fields defaulting to None via
        // #[serde(default)].
        let pre_feature_json = format!(
            r#"{{"source_doc_id":{{"sha256":"{}"}},"hash":"{}","strength":"verified","algo":"v1"}}"#,
            "e".repeat(64),
            "a".repeat(64)
        );
        let parsed: SourceDocumentBinding =
            serde_json::from_str(&pre_feature_json).unwrap();
        assert!(parsed.alias_from.is_none());
        assert!(parsed.alias_to.is_none());
        assert_eq!(parsed.strength, BindingStrength::Verified);
    }
}
