//! Operator-supplied PURL alias for cross-tier binding (milestone 111).
//!
//! Declares the `PurlAlias` newtype, the `AliasMap` container, the
//! `AliasError` enum, and the clap `value_parser` for the
//! `--pkg-alias LHS=RHS` CLI flag.
//!
//! Per `specs/111-pkg-alias-binding/data-model.md` and
//! `specs/111-pkg-alias-binding/contracts/cli-flags.md`. Each alias
//! pair is canonicalized at construction (both sides through
//! `Purl::new`); equality and lookup are on the canonical form.
//!
//! The `AliasMap` is a `Vec`-backed insertion-ordered container:
//! deterministic for SBOM emission and cheap to scan at the
//! single-digit-N scale operators actually use (Constitution Principle
//! VI — keep the dependency surface minimal; no new `HashMap` /
//! `BTreeMap` machinery needed when linear search wins on N < 10).

use waybill_common::types::purl::{Purl, PurlError};

/// A single `(LHS, RHS)` PURL alias declared by the operator.
///
/// LHS is the image-tier component PURL the binder will look up;
/// RHS is the source-tier canonical PURL that LHS should be treated
/// as for cross-tier binding match purposes. Both sides are
/// canonicalized via [`Purl::new`].
///
/// Invariants enforced by [`PurlAlias::try_new`]:
/// - Both `lhs` and `rhs` parse as valid PURLs.
/// - `lhs != rhs` (operator-error sentinel — aliases must rewrite).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurlAlias {
    lhs: Purl,
    rhs: Purl,
}

impl PurlAlias {
    /// Construct from raw `&str` LHS and RHS. Both sides are run
    /// through [`Purl::new`] for canonicalization. Returns the
    /// appropriate [`AliasError`] variant on any failure.
    pub fn try_new(lhs_raw: &str, rhs_raw: &str) -> Result<Self, AliasError> {
        let lhs = Purl::new(lhs_raw).map_err(|source| AliasError::MalformedLhs {
            lhs: lhs_raw.to_string(),
            source,
        })?;
        let rhs = Purl::new(rhs_raw).map_err(|source| AliasError::MalformedRhs {
            rhs: rhs_raw.to_string(),
            source,
        })?;
        if lhs == rhs {
            return Err(AliasError::LhsEqualsRhs(Box::new(lhs)));
        }
        Ok(Self { lhs, rhs })
    }

    /// LHS PURL (canonical form). The PURL the operator declared as
    /// the image-tier component identifier.
    pub fn lhs(&self) -> &Purl {
        &self.lhs
    }

    /// RHS PURL (canonical form). The source-tier counterpart the
    /// binder matches against.
    pub fn rhs(&self) -> &Purl {
        &self.rhs
    }
}

/// Payload for the [`AliasError::ConflictingRhs`] variant. Boxed at
/// the variant level so the enum's largest variant doesn't dominate
/// the size of every `Result<_, AliasError>` (`clippy::result_large_err`).
#[derive(Debug)]
pub struct ConflictingRhsPayload {
    pub lhs: Purl,
    pub existing_rhs: Purl,
    pub new_rhs: Purl,
}

impl core::fmt::Display for ConflictingRhsPayload {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "LHS '{}' declared twice with conflicting RHS values: \
             '{}' and '{}'",
            self.lhs, self.existing_rhs, self.new_rhs
        )
    }
}

/// Errors emitted by the alias subsystem. Each variant's `Display`
/// message satisfies the SC-003 single-line-actionable contract
/// (names both the misconfiguration and the corrective next step).
#[derive(Debug, thiserror::Error)]
pub enum AliasError {
    #[error(
        "--pkg-alias value '{raw}' is missing the '=' separator; \
         expected format: 'LHS_PURL=RHS_PURL'"
    )]
    MissingSeparator { raw: String },

    #[error("--pkg-alias LHS PURL '{lhs}' failed to parse: {source}")]
    MalformedLhs {
        lhs: String,
        #[source]
        source: PurlError,
    },

    #[error("--pkg-alias RHS PURL '{rhs}' failed to parse: {source}")]
    MalformedRhs {
        rhs: String,
        #[source]
        source: PurlError,
    },

    /// Boxed so the variant's three `Purl` fields don't bloat the
    /// `Result<_, AliasError>` size used pervasively in the binding
    /// path (`clippy::result_large_err`).
    #[error(
        "--pkg-alias LHS '{0}' identical to RHS; aliases must specify \
         distinct PURLs (did you mean to declare a different RHS?)"
    )]
    LhsEqualsRhs(Box<Purl>),

    /// Boxed payload for the same `result_large_err` reason.
    #[error(
        "--pkg-alias {0} (only one mapping per LHS is permitted; \
         resolve the conflict and re-run)"
    )]
    ConflictingRhs(Box<ConflictingRhsPayload>),
}

/// Insertion-ordered container of validated aliases for one scan.
///
/// Backed by `Vec<PurlAlias>` for determinism in SBOM emission order
/// and to avoid pulling in `BTreeMap`/`HashMap` machinery for the
/// single-digit-N scale operators actually use. `get` is linear-scan;
/// at N < 10 the constant factor of hash/tree overhead exceeds the
/// linear cost.
#[derive(Debug, Clone, Default)]
pub struct AliasMap {
    entries: Vec<PurlAlias>,
}

impl AliasMap {
    /// Construct an empty alias map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an alias. Same-LHS-same-RHS is idempotent. Same-LHS-
    /// different-RHS returns [`AliasError::ConflictingRhs`].
    pub fn insert(&mut self, alias: PurlAlias) -> Result<(), AliasError> {
        for existing in &self.entries {
            if existing.lhs == alias.lhs {
                if existing.rhs == alias.rhs {
                    // Idempotent: same alias declared twice is fine.
                    return Ok(());
                }
                return Err(AliasError::ConflictingRhs(Box::new(
                    ConflictingRhsPayload {
                        lhs: alias.lhs,
                        existing_rhs: existing.rhs.clone(),
                        new_rhs: alias.rhs,
                    },
                )));
            }
        }
        self.entries.push(alias);
        Ok(())
    }

    /// Look up the RHS for a given LHS PURL. Returns `None` when no
    /// alias was declared for `lhs`.
    pub fn get(&self, lhs: &Purl) -> Option<&Purl> {
        self.entries
            .iter()
            .find(|a| &a.lhs == lhs)
            .map(|a| &a.rhs)
    }

    /// True when no aliases have been declared.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of declared aliases.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Iterate over the declared aliases in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &PurlAlias> {
        self.entries.iter()
    }
}

/// Clap `value_parser` for the `--pkg-alias LHS=RHS` flag.
///
/// The LHS/RHS separator is NOT simply the first `=`: a PURL's
/// qualifier section legitimately contains `=` (e.g. the binary
/// walker's file-level `pkg:generic/baz?file-sha256=<hex>` — the
/// primary US1 alias source), so the first `=` in the raw value may
/// belong to an LHS qualifier. Instead, each `=` position is tried
/// left-to-right; the first split whose right side starts with the
/// `pkg:` scheme prefix AND whose two sides both canonicalize via
/// [`Purl::new`] wins. When no candidate has a `pkg:`-prefixed right
/// side, the first-`=` split's parse errors are surfaced so the
/// operator sees the standard malformed-PURL diagnostics.
///
/// Returns `Result<PurlAlias, String>` per clap's parser-error
/// contract (clap calls `.to_string()` on the error type). Maps
/// `AliasError` variants verbatim via `.to_string()`.
///
/// NOTE: `ConflictingRhs` is NOT raised here — conflict detection
/// happens at [`AliasMap::insert`] time, after all CLI + env-var
/// values have been collected.
pub fn parse_pkg_alias(raw: &str) -> Result<PurlAlias, String> {
    let trimmed = raw.trim();
    if !trimmed.contains('=') {
        return Err(AliasError::MissingSeparator {
            raw: trimmed.to_string(),
        }
        .to_string());
    }
    // First viable split wins; first viable-candidate error (a split
    // whose RHS at least looked like a PURL) is kept for diagnostics.
    let mut candidate_err: Option<AliasError> = None;
    for (idx, _) in trimmed.match_indices('=') {
        let (lhs_raw, rhs_raw) = (&trimmed[..idx], &trimmed[idx + 1..]);
        if !rhs_raw.starts_with("pkg:") {
            continue;
        }
        match PurlAlias::try_new(lhs_raw, rhs_raw) {
            Ok(alias) => return Ok(alias),
            Err(e) => {
                candidate_err.get_or_insert(e);
            }
        }
    }
    if let Some(e) = candidate_err {
        return Err(e.to_string());
    }
    // No split produced a `pkg:`-prefixed RHS — fall back to the
    // first `=` so the operator gets the malformed-PURL message for
    // what they most plausibly intended as the boundary.
    let (lhs_raw, rhs_raw) = trimmed
        .split_once('=')
        .ok_or_else(|| {
            AliasError::MissingSeparator {
                raw: trimmed.to_string(),
            }
            .to_string()
        })?;
    PurlAlias::try_new(lhs_raw, rhs_raw).map_err(|e| e.to_string())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const LHS_GENERIC: &str = "pkg:generic/baz";
    const RHS_CARGO: &str = "pkg:cargo/baz@1.0.0";
    const RHS_CARGO_ALT: &str = "pkg:cargo/baz@1.1.0";

    // ─────────────────────────────────────────────────────────────
    // PurlAlias::try_new
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn try_new_succeeds_on_distinct_valid_purls() {
        let a = PurlAlias::try_new(LHS_GENERIC, RHS_CARGO).unwrap();
        assert_eq!(a.lhs().as_str(), LHS_GENERIC);
        assert_eq!(a.rhs().as_str(), RHS_CARGO);
    }

    #[test]
    fn try_new_rejects_malformed_lhs() {
        let err = PurlAlias::try_new("not-a-purl", RHS_CARGO).unwrap_err();
        assert!(
            matches!(err, AliasError::MalformedLhs { .. }),
            "expected MalformedLhs, got {err:?}"
        );
    }

    #[test]
    fn try_new_rejects_malformed_rhs() {
        let err = PurlAlias::try_new(LHS_GENERIC, "@@nope").unwrap_err();
        assert!(matches!(err, AliasError::MalformedRhs { .. }));
    }

    #[test]
    fn try_new_rejects_lhs_equals_rhs() {
        let err = PurlAlias::try_new(RHS_CARGO, RHS_CARGO).unwrap_err();
        assert!(matches!(err, AliasError::LhsEqualsRhs { .. }));
    }

    #[test]
    fn try_new_accepts_non_generic_lhs() {
        // C2 remediation per /speckit-analyze: the parser must accept
        // ANY PURL scheme on the LHS, not just `pkg:generic/*`.
        let a =
            PurlAlias::try_new("pkg:deb/debian/foo@1.0", "pkg:github/foo/foo@1.0").unwrap();
        assert_eq!(a.lhs().ecosystem(), "deb");
        assert_eq!(a.rhs().ecosystem(), "github");
    }

    // ─────────────────────────────────────────────────────────────
    // AliasMap insertion + lookup
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn insert_into_empty_map_succeeds() {
        let mut map = AliasMap::new();
        let alias = PurlAlias::try_new(LHS_GENERIC, RHS_CARGO).unwrap();
        map.insert(alias).unwrap();
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn insert_same_lhs_same_rhs_is_idempotent() {
        let mut map = AliasMap::new();
        let alias1 = PurlAlias::try_new(LHS_GENERIC, RHS_CARGO).unwrap();
        let alias2 = PurlAlias::try_new(LHS_GENERIC, RHS_CARGO).unwrap();
        map.insert(alias1).unwrap();
        map.insert(alias2).unwrap();
        assert_eq!(map.len(), 1, "idempotent insert must not duplicate");
    }

    #[test]
    fn insert_same_lhs_different_rhs_rejected() {
        let mut map = AliasMap::new();
        let alias1 = PurlAlias::try_new(LHS_GENERIC, RHS_CARGO).unwrap();
        let alias2 = PurlAlias::try_new(LHS_GENERIC, RHS_CARGO_ALT).unwrap();
        map.insert(alias1).unwrap();
        let err = map.insert(alias2).unwrap_err();
        assert!(matches!(err, AliasError::ConflictingRhs { .. }));
    }

    #[test]
    fn insert_same_rhs_different_lhs_accepted() {
        // U1 invariant per /speckit-analyze: multiple LHS aliases
        // targeting the same RHS are valid (workspace projects emit
        // distinct binaries from one source crate).
        let mut map = AliasMap::new();
        map.insert(PurlAlias::try_new("pkg:generic/baz-cli", RHS_CARGO).unwrap())
            .unwrap();
        map.insert(PurlAlias::try_new("pkg:generic/baz-daemon", RHS_CARGO).unwrap())
            .unwrap();
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn get_returns_rhs_for_known_lhs() {
        let mut map = AliasMap::new();
        let alias = PurlAlias::try_new(LHS_GENERIC, RHS_CARGO).unwrap();
        map.insert(alias).unwrap();
        let lhs_purl = Purl::new(LHS_GENERIC).unwrap();
        assert_eq!(map.get(&lhs_purl).unwrap().as_str(), RHS_CARGO);
    }

    #[test]
    fn get_returns_none_for_unknown_lhs() {
        let map = AliasMap::new();
        let lhs_purl = Purl::new(LHS_GENERIC).unwrap();
        assert!(map.get(&lhs_purl).is_none());
    }

    // ─────────────────────────────────────────────────────────────
    // parse_pkg_alias (CLI value_parser)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_succeeds_on_well_formed_input() {
        let alias = parse_pkg_alias(&format!("{LHS_GENERIC}={RHS_CARGO}")).unwrap();
        assert_eq!(alias.lhs().as_str(), LHS_GENERIC);
        assert_eq!(alias.rhs().as_str(), RHS_CARGO);
    }

    #[test]
    fn parse_trims_whitespace() {
        let alias =
            parse_pkg_alias(&format!("   {LHS_GENERIC}={RHS_CARGO}   ")).unwrap();
        assert_eq!(alias.lhs().as_str(), LHS_GENERIC);
    }

    #[test]
    fn parse_handles_qualifier_bearing_lhs() {
        // The binary walker's file-level PURL shape — the qualifier's
        // own `=` precedes the LHS/RHS separator and must not be
        // mistaken for it (US1 primary use case).
        let lhs = "pkg:generic/baz?file-sha256=446db4a0be0f85cb27407792fd4ff3de0a60a92b1055dc14ee810483df82b5c9";
        let alias = parse_pkg_alias(&format!("{lhs}={RHS_CARGO}")).unwrap();
        assert_eq!(alias.lhs().as_str(), lhs);
        assert_eq!(alias.rhs().as_str(), RHS_CARGO);
    }

    #[test]
    fn parse_handles_qualifier_bearing_rhs() {
        let rhs = "pkg:cargo/baz@1.0.0?repository_url=https%3A%2F%2Fexample.test";
        let alias = parse_pkg_alias(&format!("{LHS_GENERIC}={rhs}")).unwrap();
        assert_eq!(alias.lhs().as_str(), LHS_GENERIC);
        assert_eq!(alias.rhs().as_str(), rhs);
    }

    #[test]
    fn parse_rejects_missing_separator() {
        let err = parse_pkg_alias(LHS_GENERIC).unwrap_err();
        assert!(
            err.contains("missing the '=' separator"),
            "error message should name the missing separator; got: {err}"
        );
    }

    #[test]
    fn parse_rejects_malformed_lhs_with_named_error() {
        let err = parse_pkg_alias(&format!("not-a-purl={RHS_CARGO}")).unwrap_err();
        assert!(
            err.contains("LHS PURL 'not-a-purl' failed to parse"),
            "error should name the LHS; got: {err}"
        );
    }

    #[test]
    fn parse_rejects_malformed_rhs_with_named_error() {
        let err = parse_pkg_alias(&format!("{LHS_GENERIC}=@@nope")).unwrap_err();
        assert!(
            err.contains("RHS PURL '@@nope' failed to parse"),
            "error should name the RHS; got: {err}"
        );
    }
}
