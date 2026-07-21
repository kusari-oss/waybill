use serde::{Deserialize, Serialize};

/// A validated SPDX license expression.
///
/// Basic validation ensures the string is non-empty and contains
/// only characters valid in SPDX expressions. Full SPDX expression
/// parsing can be added later via a dedicated crate.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SpdxExpression(String);

#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("empty SPDX expression")]
    Empty,
    #[error("invalid SPDX expression: {0}")]
    Invalid(String),
}

/// Milestone 146 (closes #470): dedupe byte-identical top-level operands
/// in homogeneous AND-chain or OR-chain SPDX expressions before storing
/// the canonical form. Operates on the already-parsed `spdx::Expression`
/// tree via `iter()` (postfix order).
///
/// **Invariants** (from `specs/146-license-expression-dedup/contracts/`):
/// 1. Homogeneous AND-chain dedup — `"MIT AND MIT"` → `"MIT"`.
/// 2. Homogeneous OR-chain dedup — `"MIT OR MIT"` → `"MIT"`.
/// 3. `WITH` clauses atomic — `LicenseReq::Display` includes
///    ` WITH <exception>` suffix, so the byte-comparison treats
///    `GPL WITH Classpath` as a single operand.
/// 4. No-op for single-operand / already-deduped inputs.
/// 5. No-op for mixed AND/OR expressions (recursive dedup deferred
///    per spec Out of Scope §1).
///
/// **Constitution Principle V audit**: this is a semantics-preserving
/// canonicalization (`X AND X ≡ X`, `X OR X ≡ X` per SPDX 2.x grammar).
/// No new `mikebom:*` annotations introduced.
fn dedupe_top_level_operands(expr: &spdx::Expression) -> String {
    use spdx::expression::{ExprNode, Operator};

    let nodes: Vec<&ExprNode> = expr.iter().collect();
    if nodes.is_empty() {
        return expr.to_string();
    }

    // Single-operand expressions have no Op nodes — dedup is a no-op
    // (Invariant 4).
    let outermost_op = match nodes.last() {
        Some(ExprNode::Op(op)) => *op,
        _ => return expr.to_string(),
    };

    // If the expression has MIXED operators (both AND and OR at any
    // level), the outermost connector is `outermost_op` but inner
    // operands are sub-expressions we don't recursively examine in v1.0
    // (Invariant 5 + spec Out of Scope §1). Skip dedup to avoid
    // splitting structure we can't safely reassemble.
    let all_same_op = nodes.iter().all(|n| match n {
        ExprNode::Op(op) => *op == outermost_op,
        _ => true,
    });
    if !all_same_op {
        return expr.to_string();
    }

    // Homogeneous chain — collect req strings (including WITH clauses
    // via `LicenseReq::Display`) and dedupe in order of first occurrence.
    let mut seen = std::collections::BTreeSet::new();
    let mut unique: Vec<String> = Vec::new();
    let mut total_reqs = 0usize;
    for n in &nodes {
        if let ExprNode::Req(req) = n {
            total_reqs += 1;
            let s = req.req.to_string();
            if seen.insert(s.clone()) {
                unique.push(s);
            }
        }
    }

    // CRITICAL: if no duplicates exist, return `expr.to_string()`
    // unchanged. `Expression::Display` writes the original input
    // string verbatim (preserving `GPL-2.0-only` casing), whereas
    // rebuilding from `LicenseReq::Display` calls the spdx crate's
    // `LicenseId::Display` which normalizes legacy names to short
    // form (`GPL-2.0`). Only rebuild when we actually need to drop
    // duplicate operands — otherwise canonical-form drift breaks
    // every existing reader's `try_canonical` round-trip.
    if unique.len() == total_reqs {
        return expr.to_string();
    }

    if unique.len() <= 1 {
        return unique.into_iter().next().unwrap_or_default();
    }

    let sep = match outermost_op {
        Operator::And => " AND ",
        Operator::Or => " OR ",
    };
    unique.join(sep)
}

impl SpdxExpression {
    /// Permissive constructor — accepts any non-empty, non-control-char
    /// string. Use this when the source data isn't guaranteed to be a
    /// canonical SPDX expression (e.g. raw text from a Debian copyright
    /// file's `License:` field) and the caller has already extracted
    /// the best string available.
    pub fn new(raw: &str) -> Result<Self, LicenseError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LicenseError::Empty);
        }
        // Basic validation: SPDX expressions contain identifiers,
        // AND/OR/WITH operators, and parentheses
        if trimmed.contains(|c: char| c.is_control()) {
            return Err(LicenseError::Invalid(
                "contains control characters".to_string(),
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Strict constructor — runs the input through the `spdx` crate's
    /// real expression parser. On success, stores the canonical form
    /// the parser produces (e.g. `"GPL-2.0-or-later"` for input
    /// `"GPL-2.0-or-later "`). On failure, returns
    /// [`LicenseError::Invalid`] with the parser's error message.
    ///
    /// Use this when you want a downstream consumer to be able to trust
    /// that the stored value is a real SPDX 2.x expression — useful for
    /// the dpkg copyright reader, where we want to discard noisy
    /// free-form text rather than emit it as a "license."
    pub fn try_canonical(raw: &str) -> Result<Self, LicenseError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LicenseError::Empty);
        }
        match spdx::Expression::parse(trimmed) {
            Ok(expr) => {
                // Milestone 146 (closes #470): dedupe byte-identical
                // top-level operands in homogeneous AND-/OR-chains
                // before storing. SPDX 2.x defines `X AND X ≡ X` and
                // `X OR X ≡ X` as canonical equivalences; the spdx
                // crate's parse + Display round-trip preserves
                // duplicates verbatim (Yocto-built RPMs ship
                // `License: GPL-2.0-only AND GPL-2.0-only` in their
                // headers — issue #470), so we dedupe here. The
                // sibling `SpdxExpression::new` constructor below
                // DOES NOT apply this dedup (FR-006 — preserves the
                // lenient best-effort raw-storage contract).
                Ok(Self(dedupe_top_level_operands(&expr)))
            }
            Err(e) => Err(LicenseError::Invalid(e.to_string())),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// If the stored expression is a single SPDX identifier (no AND /
    /// OR / WITH operators, no parentheses, no ref prefix), return it
    /// verbatim. Otherwise return `None`.
    ///
    /// Used by the CycloneDX serializer to decide between the
    /// `license.id` shape (required for single SPDX identifiers per
    /// sbomqs's `comp_with_valid_licenses` check) and the
    /// `license.expression` shape (for compound expressions).
    ///
    /// Examples:
    /// - `"MIT"` → `Some("MIT")`
    /// - `"Apache-2.0+"` → `Some("Apache-2.0+")` (trailing `+` is
    ///   part of the identifier, not an operator)
    /// - `"MIT OR Apache-2.0"` → `None`
    /// - `"GPL-2.0-or-later WITH Classpath-exception-2.0"` → `None`
    /// - `"LicenseRef-foo"` → `None` (license refs aren't canonical IDs)
    pub fn as_spdx_id(&self) -> Option<&str> {
        let trimmed = self.0.trim();
        if trimmed.is_empty() {
            return None;
        }
        // License refs aren't SPDX-list identifiers.
        if trimmed.starts_with("LicenseRef-")
            || trimmed.starts_with("DocumentRef-")
        {
            return None;
        }
        // Compound expressions contain whitespace-separated operators
        // or parentheses. A bare identifier contains none of those.
        if trimmed.contains(char::is_whitespace) || trimmed.contains('(') {
            return None;
        }
        // Validate against the SPDX list via the spdx crate. `+` suffix
        // is part of the identifier and accepted. An unknown identifier
        // returns None (falls through to the expression shape).
        if spdx::Expression::parse(trimmed).is_err() {
            return None;
        }
        // Additional guard: the parsed expression must be a single
        // identifier, not a compound that happens to lack whitespace
        // (shouldn't exist in practice, but be defensive).
        //
        // spdx::Expression::requirements() yields one entry per
        // identifier in the expression; == 1 means single ID.
        let expr = spdx::Expression::parse(trimmed).ok()?;
        if expr.requirements().count() == 1 {
            Some(trimmed)
        } else {
            None
        }
    }
}

impl core::fmt::Display for SpdxExpression {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for SpdxExpression {
    type Error = LicenseError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(&s)
    }
}

impl From<SpdxExpression> for String {
    fn from(e: SpdxExpression) -> String {
        e.0
    }
}

/// Milestone 202 (FR-002, closes #579) — shared license-operand sanitizer.
///
/// Extracted from `mikebom-cli/src/scan_fs/package_db/rpm_file.rs::sanitize_to_license_ref_idstring`
/// (m185 US2). Same behavior: filter to alphanumeric + `-` + `.`, collapse
/// dash runs, trim leading/trailing dashes. Returns the sanitized id-suffix
/// (without a `LicenseRef-` prefix) or `None` if the filtered result is
/// empty. Callers responsible for prepending `LicenseRef-` to the return
/// value before emitting.
///
/// Consumers post-extraction:
/// - m185 SPDX 2.3 emission path in `rpm_file.rs` (via re-export from this module)
/// - m202 CDX splitter's `license_entry_for_token` Branch 3 (new caller for #579)
///
/// The shared home ensures CDX and SPDX 2.3 emitters produce byte-identical
/// `LicenseRef-<sanitized>` identifiers for the same input token — the FR-002
/// parity guarantee that lets downstream consumers cross-reference the two
/// wire-format outputs via the LicenseRef join key.
///
/// Worked examples (from the m185 doc comment):
///
/// | Input             | Output                  |
/// |-------------------|-------------------------|
/// | `"GPLv2+"`        | `Some("GPLv2")`         |
/// | `"My License v2"` | `Some("My-License-v2")` |
/// | `"(custom)"`      | `Some("custom")`        |
/// | `"LGPL-2.1+"`     | `Some("LGPL-2.1")`      |
/// | `"bzip2-1.0.4"`   | `Some("bzip2-1.0.4")`   |
/// | `"PD"`            | `Some("PD")`            |
/// | `"!@#$"`          | `None`                  |
/// | `""`              | `None`                  |
/// | `"---"`           | `None`                  |
pub fn sanitize_license_operand_to_ref(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut prev_was_dash = false;
    for c in s.chars() {
        let safe = c.is_ascii_alphanumeric() || c == '-' || c == '.';
        let emit = if safe { c } else { '-' };
        if emit == '-' {
            if !prev_was_dash {
                out.push('-');
                prev_was_dash = true;
            }
            // else: skip — collapses run of dashes to one
        } else {
            out.push(emit);
            prev_was_dash = false;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_license() {
        let l = SpdxExpression::new("MIT").unwrap();
        assert_eq!(l.as_str(), "MIT");
    }

    #[test]
    fn compound_expression() {
        let l = SpdxExpression::new("MIT OR Apache-2.0").unwrap();
        assert_eq!(l.as_str(), "MIT OR Apache-2.0");
    }

    #[test]
    fn empty_rejected() {
        assert!(SpdxExpression::new("").is_err());
        assert!(SpdxExpression::new("   ").is_err());
    }

    #[test]
    fn serde_round_trip() {
        let l = SpdxExpression::new("MIT OR Apache-2.0").unwrap();
        let json = serde_json::to_string(&l).unwrap();
        let back: SpdxExpression = serde_json::from_str(&json).unwrap();
        assert_eq!(l, back);
    }

    #[test]
    fn try_canonical_accepts_simple_id() {
        let l = SpdxExpression::try_canonical("MIT").unwrap();
        assert_eq!(l.as_str(), "MIT");
    }

    #[test]
    fn try_canonical_accepts_or_expression() {
        let l = SpdxExpression::try_canonical("MIT OR Apache-2.0").unwrap();
        // Canonical form should be deterministic; ordering preserved.
        assert!(l.as_str().contains("MIT"));
        assert!(l.as_str().contains("Apache-2.0"));
    }

    #[test]
    fn try_canonical_accepts_with_exception() {
        let l = SpdxExpression::try_canonical("GPL-2.0-or-later WITH Classpath-exception-2.0")
            .unwrap();
        assert!(l.as_str().contains("Classpath-exception-2.0"));
    }

    #[test]
    fn try_canonical_rejects_unknown_identifier() {
        // A free-form string the spdx parser doesn't recognise.
        let result = SpdxExpression::try_canonical("Some Random Free Text");
        assert!(result.is_err(), "should reject non-SPDX text");
    }

    #[test]
    fn try_canonical_rejects_empty() {
        assert!(matches!(
            SpdxExpression::try_canonical(""),
            Err(LicenseError::Empty)
        ));
        assert!(matches!(
            SpdxExpression::try_canonical("   "),
            Err(LicenseError::Empty)
        ));
    }

    // --- as_spdx_id (sbomqs score lift Fix 1) ----------------------------

    #[test]
    fn as_spdx_id_returns_single_identifier() {
        let l = SpdxExpression::new("MIT").unwrap();
        assert_eq!(l.as_spdx_id(), Some("MIT"));
    }

    #[test]
    fn as_spdx_id_returns_apache_2_0() {
        let l = SpdxExpression::new("Apache-2.0").unwrap();
        assert_eq!(l.as_spdx_id(), Some("Apache-2.0"));
    }

    #[test]
    fn as_spdx_id_accepts_trailing_plus() {
        // `+` suffix is part of the identifier (means "this version
        // or later"), not an operator.
        let l = SpdxExpression::new("Apache-2.0+").unwrap();
        assert_eq!(l.as_spdx_id(), Some("Apache-2.0+"));
    }

    #[test]
    fn as_spdx_id_returns_none_for_or_expression() {
        let l = SpdxExpression::new("MIT OR Apache-2.0").unwrap();
        assert_eq!(l.as_spdx_id(), None);
    }

    #[test]
    fn as_spdx_id_returns_none_for_with_exception() {
        let l =
            SpdxExpression::new("GPL-2.0-or-later WITH Classpath-exception-2.0").unwrap();
        assert_eq!(l.as_spdx_id(), None);
    }

    #[test]
    fn as_spdx_id_returns_none_for_and_expression() {
        let l = SpdxExpression::new("MIT AND Apache-2.0").unwrap();
        assert_eq!(l.as_spdx_id(), None);
    }

    #[test]
    fn as_spdx_id_returns_none_for_parenthesized() {
        let l = SpdxExpression::new("(MIT OR Apache-2.0)").unwrap();
        assert_eq!(l.as_spdx_id(), None);
    }

    #[test]
    fn as_spdx_id_returns_none_for_license_ref() {
        let l = SpdxExpression::new("LicenseRef-my-custom-license").unwrap();
        assert_eq!(l.as_spdx_id(), None);
    }

    #[test]
    fn as_spdx_id_returns_none_for_unknown_identifier() {
        let l = SpdxExpression::new("SomethingNotOnTheSpdxList").unwrap();
        assert_eq!(l.as_spdx_id(), None);
    }

    // -----------------------------------------------------------------
    // Milestone 146 (closes #470) — top-level operand dedup in
    // homogeneous AND-/OR-chain expressions. See
    // `specs/146-license-expression-dedup/contracts/spdx-expression-dedup.md`
    // for the full 7-invariant contract.
    // -----------------------------------------------------------------

    /// T004 [US1] (US1.1, SC-002 anchor): `MIT AND MIT` collapses to
    /// single-identifier `MIT`. The dominant Yocto-shaped input.
    #[test]
    fn try_canonical_dedupes_two_identical_and_operands() {
        let l = SpdxExpression::try_canonical("MIT AND MIT").unwrap();
        assert_eq!(l.as_str(), "MIT");
    }

    /// T005 [US1] (US1.2): distinct operand between duplicates is
    /// preserved.
    #[test]
    fn try_canonical_dedupes_with_distinct_operand_preserved() {
        let l = SpdxExpression::try_canonical("MIT AND Apache-2.0 AND MIT").unwrap();
        assert_eq!(l.as_str(), "MIT AND Apache-2.0");
    }

    /// T006 [US1] (US1.3): multiple occurrences of the same operand
    /// collapse to one, preserving first-occurrence order (FR-002).
    /// Uses Apache-2.0 + BSD-3-Clause (single-form license ids that
    /// the spdx 0.10 crate doesn't rename, unlike the `GPL-2.0-only` →
    /// `GPL-2.0` normalization in the legacy SPDX list).
    #[test]
    fn try_canonical_dedupes_multiple_occurrences_preserves_first_order() {
        let l = SpdxExpression::try_canonical(
            "Apache-2.0 AND Apache-2.0 AND BSD-3-Clause AND Apache-2.0",
        )
        .unwrap();
        assert_eq!(l.as_str(), "Apache-2.0 AND BSD-3-Clause");
    }

    /// T007 [US1] (US1.4 + FR-004): already-deduplicated input is a
    /// no-op.
    #[test]
    fn try_canonical_already_deduped_unchanged() {
        let l = SpdxExpression::try_canonical("MIT AND Apache-2.0").unwrap();
        assert_eq!(l.as_str(), "MIT AND Apache-2.0");
    }

    /// T008 [US1] (SC-005): `WITH` clauses are atomic operands
    /// (FR-003). Same WITH-clause on both sides dedupes; one side with
    /// and one side without the exception does NOT dedupe.
    #[test]
    fn try_canonical_with_clauses_preserved_atomic() {
        // Both operands byte-identical post-canonical (both have the
        // WITH clause) → dedupe fires.
        let same = SpdxExpression::try_canonical(
            "GPL-2.0-or-later WITH Classpath-exception-2.0 \
             AND GPL-2.0-or-later WITH Classpath-exception-2.0",
        )
        .unwrap();
        assert_eq!(
            same.as_str(),
            "GPL-2.0-or-later WITH Classpath-exception-2.0"
        );

        // Two operands DIFFER (one has WITH, one doesn't) → no dedupe.
        // The WITH-clause MUST NOT be split across the boundary; both
        // operands stay as distinct items.
        let diff = SpdxExpression::try_canonical(
            "GPL-2.0-or-later WITH Classpath-exception-2.0 \
             AND GPL-2.0-or-later",
        )
        .unwrap();
        assert_eq!(
            diff.as_str(),
            "GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later"
        );
    }

    /// T009 [US1] (FR-004): single-operand input is a no-op.
    #[test]
    fn try_canonical_single_operand_unchanged() {
        let l = SpdxExpression::try_canonical("MIT").unwrap();
        assert_eq!(l.as_str(), "MIT");
    }

    /// T010 [US1] (Invariant 5 + spec Out of Scope §1): mixed-operator
    /// expressions are NOT deduped in v1.0 (recursive dedup deferred).
    /// Uses the spdx crate's canonical form as the expected baseline
    /// (insulates against crate version drift).
    #[test]
    fn try_canonical_mixed_operators_unchanged() {
        let input = "MIT OR Apache-2.0 AND MIT";
        let canonical_baseline = spdx::Expression::parse(input).unwrap().to_string();
        let our_output = SpdxExpression::try_canonical(input).unwrap().as_str().to_string();
        assert_eq!(
            our_output, canonical_baseline,
            "mixed-operator expression must not be deduped \
             (recursive dedup out of v1.0 scope per spec Out of Scope §1)"
        );
    }

    /// T011 [US2] (US2.1): OR-chain dedup, parallel to T004 for AND.
    #[test]
    fn try_canonical_dedupes_or_operands() {
        let l = SpdxExpression::try_canonical("MIT OR MIT").unwrap();
        assert_eq!(l.as_str(), "MIT");
    }

    /// T012 [US2] (US2.2): OR-chain with distinct operand preserved.
    #[test]
    fn try_canonical_dedupes_or_chain_distinct_preserved() {
        let l = SpdxExpression::try_canonical("MIT OR Apache-2.0 OR MIT").unwrap();
        assert_eq!(l.as_str(), "MIT OR Apache-2.0");
    }

    /// T013 [US2] (Invariant 7): second `try_canonical` call on the
    /// already-deduped output is a no-op.
    #[test]
    fn try_canonical_is_idempotent() {
        let e1 = SpdxExpression::try_canonical("MIT AND MIT").unwrap();
        assert_eq!(e1.as_str(), "MIT", "first pass dedupes");
        let e2 = SpdxExpression::try_canonical(e1.as_str()).unwrap();
        assert_eq!(
            e1.as_str(),
            e2.as_str(),
            "second pass on deduped output must be a no-op"
        );
    }

    /// L1 (per /speckit-analyze): defensive guard — the lenient
    /// `SpdxExpression::new` constructor MUST NOT apply the dedup
    /// pass (FR-006 — preserves best-effort raw-storage contract for
    /// non-SPDX-parseable inputs). Catches any future refactor that
    /// accidentally pulls the dedup helper into `new()`.
    #[test]
    fn lenient_new_constructor_does_not_apply_dedup() {
        let raw = SpdxExpression::new("MIT AND MIT").unwrap();
        assert_eq!(
            raw.as_str(),
            "MIT AND MIT",
            "SpdxExpression::new MUST preserve raw form (FR-006); \
             only try_canonical applies milestone-146 dedup"
        );
    }
}
