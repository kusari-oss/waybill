//! Per-component user-defined identifiers (milestone 076).
//!
//! A `--component-id <PURL>=<scheme>:<value>` flag is parsed once at
//! CLI parse time into a `ComponentIdentifierFlag`. Per-format emitters
//! (`mikebom-cli/src/generate/{cyclonedx,spdx}/...`) consume the
//! flag list and append the identifier to every component whose
//! emitted `purl` byte-equals `selector_purl`.
//!
//! Built-in scheme names (`repo`, `git`, `image`, `attestation`,
//! `subject`) are rejected at parse time per FR-009 — those slots are
//! reserved for document-level use.

use super::{IdentifierError, IdentifierValue, SchemeName};

/// One parsed `--component-id <PURL>=<scheme>:<value>` flag.
///
/// Lifetime: parsed once at CLI parse time, stored in
/// `ScanArgs.component_id` / `RunArgs.component_id`, threaded through
/// `ScanArtifacts.component_identifiers` to per-format emitters.
#[derive(Debug, Clone)]
pub struct ComponentIdentifierFlag {
    /// Exact PURL string the operator typed. Matched byte-identically
    /// against `components[].purl` per research §5 — no glob, no
    /// version-range, no fuzzy matching.
    pub selector_purl: String,
    /// User-defined scheme name. Validated against milestone 073's
    /// FR-004 regex (`^[a-z][a-z0-9_-]*$`). Built-in scheme names are
    /// rejected at parse time per FR-009.
    pub scheme: SchemeName,
    /// The identifier value. Non-empty (enforced by `IdentifierValue`).
    pub value: IdentifierValue,
}

/// Errors emitted while parsing a `--component-id` flag value.
#[derive(Debug, thiserror::Error)]
pub enum ComponentIdentifierFlagError {
    #[error("--component-id missing `=` separator: {0:?} (expected form: --component-id <PURL>=<SCHEME>:<VALUE>)")]
    MissingEquals(String),

    #[error("--component-id PURL (LHS of `=`) is empty: {0:?}")]
    EmptyPurl(String),

    #[error("--component-id RHS missing `:` separator: {0:?} (expected form: <SCHEME>:<VALUE>)")]
    MissingColon(String),

    #[error("--component-id scheme is empty")]
    EmptyScheme,

    #[error("--component-id value is empty")]
    EmptyValue,

    #[error("--component-id scheme `{0}` is reserved for document-level built-in usage; user-defined schemes only at this layer (allowed regex: ^[a-z][a-z0-9_-]*$, excluding `repo`, `git`, `image`, `attestation`, `subject`)")]
    BuiltinSchemeRejected(String),

    #[error("--component-id scheme `{0}` fails the FR-004 regex from milestone 073: {1}")]
    InvalidSchemeName(String, IdentifierError),
}

impl ComponentIdentifierFlag {
    /// Parse a flag value of form `<PURL>=<scheme>:<value>`.
    ///
    /// Splits on the FIRST `=` (the LHS is the selector PURL; the RHS
    /// is `<scheme>:<value>`). Splits the RHS on the FIRST `:`
    /// (PURLs and values may contain `:` and `=` after these initial
    /// splits — they're preserved verbatim).
    ///
    /// Rejects:
    /// - Missing `=` separator
    /// - Empty LHS (selector_purl)
    /// - Missing `:` separator on RHS
    /// - Empty scheme
    /// - Empty value
    /// - Built-in scheme names (`repo`, `git`, `image`, `attestation`,
    ///   `subject`) per FR-009 — operators directed to document-level
    ///   flags or warned that the scheme is reserved
    /// - Scheme failing milestone 073's FR-004 regex
    ///
    /// Apply VR-076-003 in detail.
    pub fn parse(raw: &str) -> Result<Self, ComponentIdentifierFlagError> {
        let Some(eq_idx) = raw.find('=') else {
            return Err(ComponentIdentifierFlagError::MissingEquals(raw.to_string()));
        };
        let lhs = &raw[..eq_idx];
        let rhs = &raw[eq_idx + 1..];
        if lhs.is_empty() {
            return Err(ComponentIdentifierFlagError::EmptyPurl(raw.to_string()));
        }
        let Some(colon_idx) = rhs.find(':') else {
            return Err(ComponentIdentifierFlagError::MissingColon(rhs.to_string()));
        };
        let scheme_str = &rhs[..colon_idx];
        let value_str = &rhs[colon_idx + 1..];
        if scheme_str.is_empty() {
            return Err(ComponentIdentifierFlagError::EmptyScheme);
        }
        if value_str.is_empty() {
            return Err(ComponentIdentifierFlagError::EmptyValue);
        }
        // Validate the scheme name first — `SchemeName::new` enforces
        // the FR-004 regex.
        let scheme = SchemeName::new(scheme_str.to_string()).map_err(|e| {
            ComponentIdentifierFlagError::InvalidSchemeName(scheme_str.to_string(), e)
        })?;
        // FR-009: reject built-in scheme names. The `--component-id`
        // flag is for user-defined schemes only — built-in slots are
        // reserved for document-level usage.
        if super::BuiltinScheme::from_scheme_name(&scheme).is_some() {
            return Err(ComponentIdentifierFlagError::BuiltinSchemeRejected(
                scheme_str.to_string(),
            ));
        }
        let value = IdentifierValue::new(value_str.to_string()).map_err(|_| {
            // The empty-value case is already short-circuited above;
            // any other `IdentifierValue::new` failure is unreachable
            // today, but route to the EmptyValue variant defensively.
            ComponentIdentifierFlagError::EmptyValue
        })?;
        Ok(Self {
            selector_purl: lhs.to_string(),
            scheme,
            value,
        })
    }
}

/// `clap::value_parser` adapter for `--component-id`.
///
/// Returns `Result<ComponentIdentifierFlag, String>` because clap
/// requires a `String` error type for value parsers; the
/// `ComponentIdentifierFlagError` `Display` impl provides
/// human-readable error text.
pub fn parse_component_id_flag(raw: &str) -> Result<ComponentIdentifierFlag, String> {
    ComponentIdentifierFlag::parse(raw).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_happy_path_simple() {
        let f = ComponentIdentifierFlag::parse(
            "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2",
        )
        .unwrap();
        assert_eq!(f.selector_purl, "pkg:cargo/serde@1.0.0");
        assert_eq!(f.scheme.as_str(), "kusari-id");
        assert_eq!(f.value.as_str(), "asset-shared-lib-v2");
    }

    #[test]
    fn parse_value_may_contain_colons() {
        // Splits RHS on FIRST `:` only — value may contain `:`.
        let f = ComponentIdentifierFlag::parse(
            "pkg:cargo/foo@1.0.0=internal_ticket:PROJ-456:sub:1",
        )
        .unwrap();
        assert_eq!(f.selector_purl, "pkg:cargo/foo@1.0.0");
        assert_eq!(f.scheme.as_str(), "internal_ticket");
        assert_eq!(f.value.as_str(), "PROJ-456:sub:1");
    }

    #[test]
    fn parse_purl_may_contain_equals_after_first() {
        // Splits LHS on FIRST `=` only — PURLs containing `=` (rare;
        // qualifier syntax) survive on the value side. We split LHS on
        // the FIRST `=`, so `pkg:type/name?key=val=scheme:value`
        // takes everything up to FIRST `=` as LHS. This case is
        // technically degenerate (PURL containing `=` AND a separate
        // scheme:value section). Here we test the documented behavior:
        // LHS = up to first `=`, RHS = rest.
        let f =
            ComponentIdentifierFlag::parse("pkg:cargo/serde=acme:foo").unwrap();
        assert_eq!(f.selector_purl, "pkg:cargo/serde");
        assert_eq!(f.scheme.as_str(), "acme");
        assert_eq!(f.value.as_str(), "foo");
    }

    #[test]
    fn parse_rejects_missing_equals() {
        let err = ComponentIdentifierFlag::parse("pkg:cargo/foo@1.0.0").unwrap_err();
        assert!(matches!(
            err,
            ComponentIdentifierFlagError::MissingEquals(_)
        ));
    }

    #[test]
    fn parse_rejects_empty_purl() {
        let err = ComponentIdentifierFlag::parse("=acme:foo").unwrap_err();
        assert!(matches!(err, ComponentIdentifierFlagError::EmptyPurl(_)));
    }

    #[test]
    fn parse_rejects_missing_colon() {
        let err = ComponentIdentifierFlag::parse("pkg:cargo/foo@1.0.0=acme").unwrap_err();
        assert!(matches!(
            err,
            ComponentIdentifierFlagError::MissingColon(_)
        ));
    }

    #[test]
    fn parse_rejects_empty_scheme() {
        let err = ComponentIdentifierFlag::parse("pkg:cargo/foo@1.0.0=:foo").unwrap_err();
        assert!(matches!(err, ComponentIdentifierFlagError::EmptyScheme));
    }

    #[test]
    fn parse_rejects_empty_value() {
        let err = ComponentIdentifierFlag::parse("pkg:cargo/foo@1.0.0=acme:").unwrap_err();
        assert!(matches!(err, ComponentIdentifierFlagError::EmptyValue));
    }

    #[test]
    fn parse_rejects_builtin_scheme_repo() {
        let err = ComponentIdentifierFlag::parse(
            "pkg:cargo/foo@1.0.0=repo:git@github.com:foo/bar.git",
        )
        .unwrap_err();
        match err {
            ComponentIdentifierFlagError::BuiltinSchemeRejected(s) => {
                assert_eq!(s, "repo");
            }
            other => panic!("expected BuiltinSchemeRejected, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_builtin_scheme_subject() {
        // Milestone 076 — the new built-in must also be rejected for
        // per-component flags per FR-009.
        let err = ComponentIdentifierFlag::parse(
            "pkg:cargo/foo@1.0.0=subject:sha256:abc",
        )
        .unwrap_err();
        match err {
            ComponentIdentifierFlagError::BuiltinSchemeRejected(s) => {
                assert_eq!(s, "subject");
            }
            other => panic!("expected BuiltinSchemeRejected, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_all_builtin_schemes() {
        for scheme in &["repo", "git", "image", "attestation", "subject"] {
            let raw = format!("pkg:cargo/foo@1.0.0={scheme}:value");
            let err = ComponentIdentifierFlag::parse(&raw).unwrap_err();
            assert!(
                matches!(err, ComponentIdentifierFlagError::BuiltinSchemeRejected(_)),
                "scheme {scheme} should be rejected as built-in; got {err:?}"
            );
        }
    }

    #[test]
    fn parse_rejects_invalid_scheme_name_uppercase() {
        let err = ComponentIdentifierFlag::parse("pkg:cargo/foo@1.0.0=Acme:foo").unwrap_err();
        assert!(matches!(
            err,
            ComponentIdentifierFlagError::InvalidSchemeName(_, _)
        ));
    }

    #[test]
    fn parse_clap_adapter_returns_string_error() {
        let err = parse_component_id_flag("malformed").unwrap_err();
        assert!(err.contains("missing `=` separator"));
    }
}
