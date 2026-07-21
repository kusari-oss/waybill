//! Milestone 073 — identifiers.
//!
//! `(scheme, value)` pairs attached at document level to emitted
//! SBOMs. Built-in schemes (`repo:`, `git:`, `image:`, `attestation:`)
//! ride standards-native carriers per format (Constitution Principle V);
//! user-defined schemes ride a `waybill:identifiers` annotation
//! per Principle V's documented-exception path.
//!
//! Naming note: SPDX 3 already calls these "external identifiers"
//! (`Element.externalIdentifier[]`). The earlier `source-identifier`
//! naming anchored on the most-common case (source repos) but the
//! same mechanism handles image / attestation / user-defined
//! identifiers — so the term was generalized to "identifier" before
//! the milestone shipped. Milestone-072's `SourceDocumentBinding` is
//! a DIFFERENT, sibling concept (binding back to a source-tier SBOM
//! document) and intentionally retains its name.
//!
//! Layout:
//!
//! - this file (`mod.rs`) — public types: `Identifier`, `SchemeName`,
//!   `IdentifierValue`, `IdentifierKind`, `BuiltinScheme`, plus
//!   `IdentifierError`. The CLI surface parses dedicated flags
//!   (`--repo`, `--git-ref`, `--image`, `--attestation`, `--id`)
//!   into `Identifier` values via `Identifier::from_parts_with_label`
//!   and the user-defined `--id <scheme>=<value>` parser.
//! - `auto_detect.rs` — `auto_detect_repo_identifier(scan_root)` for
//!   the 3-step git-remote fallback (FR-001), and
//!   `image_reference_to_identifier(resolved)` for the image-tier
//!   auto-detection (FR-008).
//! - `validators.rs` — per-built-in-scheme syntactic validators
//!   (`validate_repo`, `validate_git`, `validate_image`,
//!   `validate_attestation`) per research.md §1. Soft-fail mode:
//!   a malformed value emits `tracing::warn!` and downgrades the
//!   identifier's `kind` to `IdentifierKind::UserDefined`.

pub mod auto_detect;
pub mod component_id;
pub mod validators;

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

/// Errors emitted by the identifier module.
///
/// Construction failures (`MissingSeparator`, `EmptyScheme`,
/// `EmptyValue`, `InvalidSchemeName`) are reported to the CLI parse
/// layer so clap rejects the flag before any scan work begins.
/// `BuiltinValidation` is the soft-fail signal — caller logs at
/// `tracing::warn!` and downgrades the identifier to
/// `IdentifierKind::UserDefined` per VR-005.
#[derive(Debug, thiserror::Error)]
pub enum IdentifierError {
    #[error("identifier missing `:` separator: {0:?}")]
    MissingSeparator(String),

    #[error("identifier scheme is empty")]
    EmptyScheme,

    #[error("identifier value is empty")]
    EmptyValue,

    #[error("scheme {0:?} fails regex `^[a-z][a-z0-9_-]*$` (FR-004)")]
    InvalidSchemeName(String),

    /// Soft-fail bubbled up from a built-in scheme's value validator.
    /// Caller logs `tracing::warn!` and downgrades to
    /// `IdentifierKind::UserDefined`.
    #[error("built-in scheme `{scheme}` value validation failed: {reason}")]
    BuiltinValidation { scheme: String, reason: String },
}

// ---------------------------------------------------------------------
// SchemeName
// ---------------------------------------------------------------------

/// Newtype around the scheme prefix. Construction validates against
/// the FR-004 regex `^[a-z][a-z0-9_-]*$`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SchemeName(String);

impl SchemeName {
    /// Construct from a string. Validates against `^[a-z][a-z0-9_-]*$`
    /// (FR-004). Empty strings are rejected.
    pub fn new(s: impl Into<String>) -> Result<Self, IdentifierError> {
        let s = s.into();
        if s.is_empty() {
            return Err(IdentifierError::EmptyScheme);
        }
        let mut chars = s.chars();
        let first = chars.next().ok_or(IdentifierError::EmptyScheme)?;
        if !first.is_ascii_lowercase() {
            return Err(IdentifierError::InvalidSchemeName(s));
        }
        for c in chars {
            let ok = c.is_ascii_lowercase()
                || c.is_ascii_digit()
                || c == '_'
                || c == '-';
            if !ok {
                return Err(IdentifierError::InvalidSchemeName(s));
            }
        }
        Ok(Self(s))
    }

    /// Borrow the scheme as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------
// IdentifierValue
// ---------------------------------------------------------------------

/// Newtype around the post-`:` value. Opaque post-parse — built-in
/// scheme validators inspect it but the type itself doesn't enforce
/// structure. Empty values are rejected (VR-002).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct IdentifierValue(String);

impl IdentifierValue {
    /// Construct from anything string-like. Empty values are rejected.
    pub fn new(s: impl Into<String>) -> Result<Self, IdentifierError> {
        let s = s.into();
        if s.is_empty() {
            return Err(IdentifierError::EmptyValue);
        }
        Ok(Self(s))
    }

    /// Borrow the value as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------
// BuiltinScheme + IdentifierKind
// ---------------------------------------------------------------------

/// Closed registry of recognized built-in schemes (research.md §2).
///
/// Milestone 076 extends the registry with `Subject` — a fifth variant
/// declaring "this SBOM describes the artifact with the given content
/// hash" (`subject:<algo>:<hex>`). Build-tier scans auto-detect
/// `subject:` from the in-toto attestation envelope's subject set;
/// source-tier and image-tier scans accept manual `--subject-hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinScheme {
    Repo,
    Git,
    Image,
    Attestation,
    /// Milestone 076: content-hash subject identifier
    /// (`subject:sha256:<hex>` or `subject:sha512:<hex>`).
    Subject,
}

impl BuiltinScheme {
    /// Resolve a scheme name to a built-in variant, or `None` for
    /// user-defined schemes.
    pub fn from_scheme_name(name: &SchemeName) -> Option<Self> {
        match name.as_str() {
            "repo" => Some(Self::Repo),
            "git" => Some(Self::Git),
            "image" => Some(Self::Image),
            "attestation" => Some(Self::Attestation),
            // Milestone 076.
            "subject" => Some(Self::Subject),
            _ => None,
        }
    }

    /// Scheme name as a string slice.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repo => "repo",
            Self::Git => "git",
            Self::Image => "image",
            Self::Attestation => "attestation",
            Self::Subject => "subject",
        }
    }

    /// CDX 1.6 `externalReferences[].type` value for this scheme
    /// (research.md §2 mapping).
    ///
    /// Milestone 076 — `Subject` reuses the CDX 1.6 `attestation` enum
    /// value per research §1: a `subject:` identifier IS attestation
    /// metadata declaring the artifact-of-record's content hash, and
    /// CDX 1.6 has no narrower native type for content-hash subjects.
    /// The two co-exist in `metadata.component.externalReferences[]`
    /// — distinguishable by the `url` shape (digest vs IRI).
    pub fn cdx_external_reference_type(self) -> &'static str {
        match self {
            Self::Repo | Self::Git => "vcs",
            Self::Image => "distribution",
            Self::Attestation | Self::Subject => "attestation",
        }
    }

    /// SPDX 2.3 `Package.externalRefs[].referenceCategory`. Uniformly
    /// `"PERSISTENT-ID"` for all built-in schemes per FR-005 + FR-004
    /// (milestone 076).
    pub fn spdx23_reference_category(self) -> &'static str {
        "PERSISTENT-ID"
    }
}

/// Two-variant enum classifying whether the scheme is recognized by
/// waybill and its built-in validator passed.
///
/// `UserDefined` is also the soft-fail destination for a built-in
/// scheme whose value failed validation (research.md §1) — the
/// identifier emits as opaque under `waybill:identifiers`
/// rather than crashing the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierKind {
    /// One of the 4 built-in schemes; value passed validation.
    Builtin(BuiltinScheme),
    /// Either a non-built-in scheme (operator-defined) OR a built-in
    /// scheme whose value failed validation (soft-fail per VR-005).
    UserDefined,
}

// ---------------------------------------------------------------------
// Identifier
// ---------------------------------------------------------------------

/// Canonical type. One `Identifier` per `(scheme, value)` pair attached
/// to an SBOM document.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Identifier {
    pub scheme: SchemeName,
    pub value: IdentifierValue,
    /// Skipped during serialization — `IdentifierKind` is reconstituted
    /// at parse time from the scheme + a value-validator re-run if a
    /// future code path round-trips through serde.
    #[serde(skip, default = "default_kind_user_defined")]
    pub kind: IdentifierKind,
    /// Optional human-readable origin info — populated by
    /// auto-detection (`"auto-detected from git remote `origin`"`) or
    /// `None` for manual flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
}

fn default_kind_user_defined() -> IdentifierKind {
    IdentifierKind::UserDefined
}

impl Identifier {
    /// Parse `<scheme>:<value>` from a CLI flag value. Splits on the
    /// FIRST `:` only (VR-003). Built-in schemes get value-validated;
    /// validation failure emits a `tracing::warn!` and downgrades the
    /// `kind` to `IdentifierKind::UserDefined` (research.md §1
    /// soft-fail).
    pub fn parse(raw: &str) -> Result<Self, IdentifierError> {
        let split = raw.find(':');
        let Some(idx) = split else {
            return Err(IdentifierError::MissingSeparator(raw.to_string()));
        };
        let scheme_str = &raw[..idx];
        let value_str = &raw[idx + 1..];
        let scheme = SchemeName::new(scheme_str.to_string())?;
        let value = IdentifierValue::new(value_str.to_string())?;
        let kind = match BuiltinScheme::from_scheme_name(&scheme) {
            Some(b) => match validators::validate_for_scheme(b, value.as_str()) {
                Ok(()) => IdentifierKind::Builtin(b),
                Err(err) => {
                    tracing::warn!(
                        scheme = scheme.as_str(),
                        value = value.as_str(),
                        reason = %err,
                        "built-in identifier scheme failed value validation; \
                         downgrading to user-defined and emitting via \
                         waybill:identifiers annotation"
                    );
                    IdentifierKind::UserDefined
                }
            },
            None => IdentifierKind::UserDefined,
        };
        Ok(Self {
            scheme,
            value,
            kind,
            source_label: None,
        })
    }

    /// Construct an `Identifier` directly with a known kind. Used by
    /// auto-detection paths where the value is known to be valid (we
    /// constructed it ourselves) and a `source_label` is populated.
    pub fn from_parts_with_label(
        scheme: SchemeName,
        value: IdentifierValue,
        kind: IdentifierKind,
        source_label: Option<String>,
    ) -> Self {
        Self {
            scheme,
            value,
            kind,
            source_label,
        }
    }

    /// Whether this identifier resolved to a recognized built-in scheme.
    pub fn is_builtin(&self) -> bool {
        matches!(self.kind, IdentifierKind::Builtin(_))
    }

    /// The wire-form representation: `<scheme>:<value>`.
    pub fn as_wire(&self) -> String {
        format!("{}:{}", self.scheme.as_str(), self.value.as_str())
    }
}

// ---------------------------------------------------------------------
// Tier-agnostic resolution pipeline
// ---------------------------------------------------------------------

/// Resolve the final list of identifiers for emission per
/// FR-006 + FR-009 override-position rule (milestone 073) and the
/// build-tier-extended same-rules-applied-per-scheme contract
/// (milestone 074 — multiple auto-detected entries supported).
///
/// **Tier-agnostic.** Source-tier `--path` scans pass at most one
/// auto-detected entry (`repo:` or `image:`). Build-tier
/// `waybill trace run` invocations pass up to two (`repo:` and
/// `git:`). Image-tier `--image` scans pass up to one (`image:`).
/// The override semantics apply per-scheme — manual `--repo` overrides
/// only the auto-detected `repo:`, leaves auto-detected `git:` alone
/// (and vice versa).
///
/// The algorithm in detail:
///
/// 1. Start with `auto_detected` in the order supplied by the caller.
/// 2. For each manual entry in supply order, apply one of three
///    cases:
///    - **Exact dedup against auto-detected** — when manual matches
///      an auto-detected entry on `(scheme, value)`, replace in
///      place. Manual inherits auto-detected position; the
///      auto-detected `source_label` is dropped; info-log notes the
///      dedup.
///    - **Exact dedup against earlier manual** — skip (first-
///      supplied wins; FR-009 last-clause).
///    - **Same-scheme-different-value override** — true FR-006
///      override. The matching auto-detected entry is dropped
///      (collapsed away); manual entry is appended in supply order,
///      NOT promoted to front. Both values are logged at info level.
///    - **No match** — append in supply order.
pub fn resolve_identifiers(
    auto_detected: Vec<Identifier>,
    manual: &[Identifier],
) -> Vec<Identifier> {
    let mut out: Vec<Identifier> = Vec::new();
    // Track which auto-detected entries are still active (i.e., not
    // yet supplanted by an exact-dedup or same-scheme-different-value
    // override). Indices align with `auto_detected`.
    let mut auto_active: Vec<bool> = vec![true; auto_detected.len()];
    for ad in &auto_detected {
        out.push(ad.clone());
    }
    for m in manual {
        // Step 2a: exact (scheme, value) dedup against any existing
        // entry. Manual inherits position when matching an active
        // auto-detected entry; first-manual-wins when matching an
        // earlier manual entry.
        if let Some(idx) = out
            .iter()
            .position(|e| e.scheme == m.scheme && e.value == m.value)
        {
            // Determine whether `out[idx]` corresponds to an active
            // auto-detected entry. Auto-detected entries occupy the
            // first `auto_detected.len()` positions, *but* their
            // indices may have been disturbed by `remove(...)` calls
            // from same-scheme-different-value overrides on earlier
            // iterations. Track this by matching against the original
            // `auto_detected` vec.
            let mut matched_auto_idx: Option<usize> = None;
            for (i, ad) in auto_detected.iter().enumerate() {
                if auto_active[i] && ad.scheme == out[idx].scheme && ad.value == out[idx].value {
                    matched_auto_idx = Some(i);
                    break;
                }
            }
            if let Some(ai) = matched_auto_idx {
                tracing::info!(
                    scheme = m.scheme.as_str(),
                    value = m.value.as_str(),
                    "manual identifier flag matches auto-detected identifier; \
                     emitting manual in auto-detected position (deduplicated)"
                );
                out[idx] = m.clone();
                auto_active[ai] = false;
            } else {
                tracing::info!(
                    scheme = m.scheme.as_str(),
                    value = m.value.as_str(),
                    "manual identifier flag duplicates an earlier manual \
                     identifier; first-supplied wins (skipping)"
                );
            }
            continue;
        }
        // Step 2b: same-scheme-different-value override against the
        // first matching active auto-detected entry (FR-006).
        let mut overridden: Option<usize> = None;
        for (i, ad) in auto_detected.iter().enumerate() {
            if auto_active[i] && ad.scheme == m.scheme && ad.value != m.value {
                overridden = Some(i);
                break;
            }
        }
        if let Some(ai) = overridden {
            let ad = &auto_detected[ai];
            if let Some(pos) = out
                .iter()
                .position(|e| e.scheme == ad.scheme && e.value == ad.value)
            {
                tracing::info!(
                    scheme = m.scheme.as_str(),
                    auto_detected_value = ad.value.as_str(),
                    manual_value = m.value.as_str(),
                    "manual identifier flag overrides auto-detected \
                     entry (same scheme, different value); dropping \
                     auto-detected, appending manual in supply order"
                );
                out.remove(pos);
            }
            auto_active[ai] = false;
            out.push(m.clone());
            continue;
        }
        // Step 2c: append in supply order.
        out.push(m.clone());
    }
    out
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn scheme_name_accepts_lowercase_letters_digits_underscore_hyphen() {
        SchemeName::new("repo").unwrap();
        SchemeName::new("git").unwrap();
        SchemeName::new("image").unwrap();
        SchemeName::new("attestation").unwrap();
        SchemeName::new("acme_corp_id").unwrap();
        SchemeName::new("internal-ticket").unwrap();
        SchemeName::new("a1b2c3").unwrap();
        SchemeName::new("a").unwrap();
    }

    #[test]
    fn scheme_name_rejects_uppercase_leading_digit_empty_special() {
        assert!(matches!(
            SchemeName::new(""),
            Err(IdentifierError::EmptyScheme)
        ));
        assert!(matches!(
            SchemeName::new("Repo"),
            Err(IdentifierError::InvalidSchemeName(_))
        ));
        assert!(matches!(
            SchemeName::new("1repo"),
            Err(IdentifierError::InvalidSchemeName(_))
        ));
        assert!(matches!(
            SchemeName::new("re po"),
            Err(IdentifierError::InvalidSchemeName(_))
        ));
        assert!(matches!(
            SchemeName::new("re.po"),
            Err(IdentifierError::InvalidSchemeName(_))
        ));
        assert!(matches!(
            SchemeName::new("re/po"),
            Err(IdentifierError::InvalidSchemeName(_))
        ));
    }

    #[test]
    fn identifier_value_rejects_empty() {
        assert!(matches!(
            IdentifierValue::new(""),
            Err(IdentifierError::EmptyValue)
        ));
        IdentifierValue::new("x").unwrap();
        IdentifierValue::new("git@github.com:foo/bar.git").unwrap();
    }

    #[test]
    fn parse_splits_on_first_colon_only() {
        let id = Identifier::parse("repo:git@github.com:foo/bar.git").unwrap();
        assert_eq!(id.scheme.as_str(), "repo");
        assert_eq!(id.value.as_str(), "git@github.com:foo/bar.git");

        let id = Identifier::parse(
            "image:docker.io/foo/bar:v1@sha256:abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdef0123",
        )
        .unwrap();
        assert_eq!(id.scheme.as_str(), "image");
        assert_eq!(
            id.value.as_str(),
            "docker.io/foo/bar:v1@sha256:abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdef0123"
        );
    }

    #[test]
    fn parse_missing_separator_errors() {
        assert!(matches!(
            Identifier::parse("no_colon_here"),
            Err(IdentifierError::MissingSeparator(_))
        ));
    }

    #[test]
    fn parse_empty_scheme_errors() {
        assert!(matches!(
            Identifier::parse(":value"),
            Err(IdentifierError::EmptyScheme)
        ));
    }

    #[test]
    fn parse_empty_value_errors() {
        assert!(matches!(
            Identifier::parse("repo:"),
            Err(IdentifierError::EmptyValue)
        ));
    }

    #[test]
    fn parse_invalid_scheme_name_errors() {
        assert!(matches!(
            Identifier::parse("REPO:value"),
            Err(IdentifierError::InvalidSchemeName(_))
        ));
        assert!(matches!(
            Identifier::parse("1repo:value"),
            Err(IdentifierError::InvalidSchemeName(_))
        ));
    }

    #[test]
    fn builtin_scheme_from_scheme_name_recognizes_four() {
        let s = SchemeName::new("repo").unwrap();
        assert_eq!(BuiltinScheme::from_scheme_name(&s), Some(BuiltinScheme::Repo));
        let s = SchemeName::new("git").unwrap();
        assert_eq!(BuiltinScheme::from_scheme_name(&s), Some(BuiltinScheme::Git));
        let s = SchemeName::new("image").unwrap();
        assert_eq!(
            BuiltinScheme::from_scheme_name(&s),
            Some(BuiltinScheme::Image)
        );
        let s = SchemeName::new("attestation").unwrap();
        assert_eq!(
            BuiltinScheme::from_scheme_name(&s),
            Some(BuiltinScheme::Attestation)
        );
    }

    #[test]
    fn builtin_scheme_from_scheme_name_rejects_user_defined() {
        let s = SchemeName::new("acme_corp_id").unwrap();
        assert_eq!(BuiltinScheme::from_scheme_name(&s), None);
        let s = SchemeName::new("internal-ticket").unwrap();
        assert_eq!(BuiltinScheme::from_scheme_name(&s), None);
    }

    #[test]
    fn builtin_scheme_cdx_external_reference_type_per_scheme() {
        assert_eq!(BuiltinScheme::Repo.cdx_external_reference_type(), "vcs");
        assert_eq!(BuiltinScheme::Git.cdx_external_reference_type(), "vcs");
        assert_eq!(
            BuiltinScheme::Image.cdx_external_reference_type(),
            "distribution"
        );
        assert_eq!(
            BuiltinScheme::Attestation.cdx_external_reference_type(),
            "attestation"
        );
        // Milestone 076 — `subject:` reuses the CDX `attestation` enum
        // value per research §1.
        assert_eq!(
            BuiltinScheme::Subject.cdx_external_reference_type(),
            "attestation"
        );
    }

    #[test]
    fn builtin_scheme_spdx23_reference_category_uniform() {
        // FR-005 — uniformly PERSISTENT-ID for all built-ins
        // (extended in milestone 076 to include `subject:`).
        assert_eq!(BuiltinScheme::Repo.spdx23_reference_category(), "PERSISTENT-ID");
        assert_eq!(BuiltinScheme::Git.spdx23_reference_category(), "PERSISTENT-ID");
        assert_eq!(BuiltinScheme::Image.spdx23_reference_category(), "PERSISTENT-ID");
        assert_eq!(
            BuiltinScheme::Attestation.spdx23_reference_category(),
            "PERSISTENT-ID"
        );
        assert_eq!(
            BuiltinScheme::Subject.spdx23_reference_category(),
            "PERSISTENT-ID"
        );
    }

    #[test]
    fn builtin_scheme_from_scheme_name_recognizes_subject() {
        // Milestone 076 — fifth built-in scheme.
        let s = SchemeName::new("subject").unwrap();
        assert_eq!(
            BuiltinScheme::from_scheme_name(&s),
            Some(BuiltinScheme::Subject)
        );
        assert_eq!(BuiltinScheme::Subject.as_str(), "subject");
    }

    #[test]
    fn parse_user_defined_scheme_is_user_defined_kind() {
        let id = Identifier::parse("acme_corp_id:abc123").unwrap();
        assert!(!id.is_builtin());
        assert!(matches!(id.kind, IdentifierKind::UserDefined));
    }

    #[test]
    fn parse_builtin_scheme_with_valid_value_is_builtin_kind() {
        let id = Identifier::parse("repo:git@github.com:foo/bar.git").unwrap();
        assert!(id.is_builtin());
        assert!(matches!(id.kind, IdentifierKind::Builtin(BuiltinScheme::Repo)));
    }

    #[test]
    fn parse_builtin_scheme_with_invalid_value_soft_fails_to_user_defined() {
        let id = Identifier::parse("repo:not_a_url_or_ssh").unwrap();
        assert!(!id.is_builtin());
        assert!(matches!(id.kind, IdentifierKind::UserDefined));
        // The wire form is still preserved verbatim.
        assert_eq!(id.scheme.as_str(), "repo");
        assert_eq!(id.value.as_str(), "not_a_url_or_ssh");
    }

    #[test]
    fn as_wire_round_trips() {
        let raw = "repo:git@github.com:foo/bar.git";
        let id = Identifier::parse(raw).unwrap();
        assert_eq!(id.as_wire(), raw);
    }
}
