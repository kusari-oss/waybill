//! SPDX 3.0.1 `simplelicensing_LicenseExpression` element builder
//! (milestone 011).
//!
//! Per `data-model.md` Element Catalog §`simplelicensing_License
//! Expression`: each declared and concluded license expression
//! becomes one element with `simplelicensing_licenseExpression`
//! carrying the canonical SPDX expression. The element is wired
//! to its owning Package by a `Relationship` with
//! `relationshipType: "hasDeclaredLicense"` or
//! `"hasConcludedLicense"`. Concluded-license element + edge are
//! omitted when the concluded expression equals the declared
//! expression.
//!
//! Canonicalization uses `spdx::Expression::try_canonical(&str)`
//! (the SPDX 2.3 path's helper); on failure, the raw string is
//! preserved verbatim per FR-008.

use std::collections::{BTreeMap, BTreeSet};

use data_encoding::BASE32_NOPAD;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::license::SpdxExpression;

use super::document::PLACEHOLDER_EXTRACTED_TEXT;
use super::v3_relationships::build_relationship;

/// Build `simplelicensing_LicenseExpression` elements + the
/// associated `hasDeclaredLicense` / `hasConcludedLicense`
/// `Relationship` elements.
///
/// LicenseExpression elements are deduplicated by `(kind, expr)`
/// across the scan: if 50 packages declare `MIT`, exactly one
/// declared-MIT LicenseExpression element is emitted; the 50
/// Relationships all point at it. Concluded-license element is
/// omitted when its canonical expression equals the declared
/// canonical expression for the same Package (no redundant edge).
pub fn build_license_elements_and_relationships(
    components: &[ResolvedComponent],
    package_iri_by_purl: &BTreeMap<String, String>,
    doc_iri: &str,
    creation_info_id: &str,
) -> (Vec<Value>, Vec<Value>) {
    // Dedup: (kind, canonical-or-raw expression) → element IRI.
    let mut elements_by_key: BTreeMap<(LicenseKind, String), String> =
        BTreeMap::new();
    let mut elements: Vec<Value> = Vec::new();
    let mut relationships: Vec<Value> = Vec::new();
    let mut seen_iris: BTreeSet<String> = BTreeSet::new();

    for c in components {
        let Some(pkg_iri) = package_iri_by_purl.get(c.purl.as_str()) else {
            continue;
        };

        let declared_expr = reduce_license_vec(&c.licenses);
        let concluded_expr = reduce_license_vec(&c.concluded_licenses);

        if let Some(expr) = &declared_expr {
            let iri = element_iri_for(LicenseKind::Declared, expr, doc_iri);
            elements_by_key
                .entry((LicenseKind::Declared, expr.clone()))
                .or_insert_with(|| {
                    let element = json!({
                        "type": "simplelicensing_LicenseExpression",
                        "spdxId": iri,
                        "creationInfo": creation_info_id,
                        "simplelicensing_licenseExpression": expr,
                    });
                    if seen_iris.insert(iri.clone()) {
                        elements.push(element);
                    }
                    iri.clone()
                });
            relationships.push(build_relationship(
                pkg_iri,
                "hasDeclaredLicense",
                &iri,
                doc_iri,
                creation_info_id,
            ));
        }

        if let Some(expr) = &concluded_expr {
            // Skip when concluded equals declared — no redundant edge.
            if Some(expr) == declared_expr.as_ref() {
                continue;
            }
            let iri = element_iri_for(LicenseKind::Concluded, expr, doc_iri);
            elements_by_key
                .entry((LicenseKind::Concluded, expr.clone()))
                .or_insert_with(|| {
                    let element = json!({
                        "type": "simplelicensing_LicenseExpression",
                        "spdxId": iri,
                        "creationInfo": creation_info_id,
                        "simplelicensing_licenseExpression": expr,
                    });
                    if seen_iris.insert(iri.clone()) {
                        elements.push(element);
                    }
                    iri.clone()
                });
            relationships.push(build_relationship(
                pkg_iri,
                "hasConcludedLicense",
                &iri,
                doc_iri,
                creation_info_id,
            ));
        }
    }

    sort_by_spdx_id(&mut elements);
    sort_by_spdx_id(&mut relationships);
    (elements, relationships)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LicenseKind {
    Declared,
    Concluded,
}

fn element_iri_for(kind: LicenseKind, expr: &str, doc_iri: &str) -> String {
    let prefix = match kind {
        LicenseKind::Declared => "license-decl",
        LicenseKind::Concluded => "license-conc",
    };
    let h = hash_prefix(expr.as_bytes(), 16);
    format!("{doc_iri}/{prefix}-{h}")
}

/// Reduce a `Vec<SpdxExpression>` to a single canonical-or-raw
/// expression string, or `None` when the list is empty. Multiple
/// declared expressions on a component (rare) are joined with
/// ` AND ` and re-canonicalized; canonicalization failure preserves
/// the raw joined string verbatim per FR-008 (no silent drop).
fn reduce_license_vec(items: &[SpdxExpression]) -> Option<String> {
    match items.len() {
        0 => None,
        1 => Some(canonicalize_or_raw(items[0].as_str())),
        _ => {
            let joined = items
                .iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join(" AND ");
            Some(canonicalize_or_raw(&joined))
        }
    }
}

fn canonicalize_or_raw(expr: &str) -> String {
    match SpdxExpression::try_canonical(expr) {
        Ok(canon) => canon.as_str().to_string(),
        Err(_) => {
            // Milestone 190 (#551): unknown/vendor licenses must hash to
            // a stable `LicenseRef-<idstring>` token so the m154
            // CustomLicense sweep at `sweep_custom_licenses` fires and
            // emits a matching `simplelicensing_CustomLicense` element.
            // Mirrors the SPDX 2.3 packages.rs:264 pattern verbatim.
            // Pre-m190: raw text preserved; the sweep found no
            // LicenseRef-* tokens and never emitted CustomLicense for
            // any ecosystem. Passthrough exception: if the raw text
            // already contains a LicenseRef-* token (e.g., an operand
            // authored as such by the reader), keep it verbatim — the
            // sweep will handle it directly.
            if expr.contains("LicenseRef-") {
                expr.to_string()
            } else {
                super::ids::SpdxId::for_license_ref(expr).as_str().to_string()
            }
        }
    }
}

fn sort_by_spdx_id(values: &mut [Value]) {
    values.sort_by(|a, b| {
        let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
        key(a).cmp(&key(b))
    });
}

fn hash_prefix(input: &[u8], chars: usize) -> String {
    let digest = Sha256::digest(input);
    let encoded = BASE32_NOPAD.encode(&digest);
    encoded[..chars].to_string()
}

/// Milestone 154 — compiled regex for extracting SPDX 3 LicenseRef-*
/// tokens from `simplelicensing_licenseExpression` strings.
///
/// Byte-identical pattern to milestone 153's `license_ref_regex()` in
/// `document.rs`. Duplicated inline per research §R3 (the pattern is a
/// 3-line construct + spec-defined constant; drift risk is nil;
/// promoting to a shared module would over-engineer for 3 lines).
///
/// **Lockstep invariant**: milestone-153's `document.rs::license_ref_regex()`
/// is the canonical reference. Any future change to that pattern MUST
/// be mirrored here (and vice versa). Grammar drift silently breaks
/// cross-format symmetry.
///
/// Pattern: `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)`
/// - Capture group 1 is the LicenseRef- token proper.
/// - The non-capturing prefix `(?:^|[^:])` excludes matches inside
///   `DocumentRef-<doc>:LicenseRef-<id>` compound tokens (SPDX 2.3 §10.1
///   / SPDX 3.0.1 same idstring grammar).
static LICENSE_REF_REGEX: std::sync::OnceLock<regex::Regex> =
    std::sync::OnceLock::new();

fn license_ref_regex() -> &'static regex::Regex {
    LICENSE_REF_REGEX.get_or_init(|| {
        regex::Regex::new(r"(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)")
            .expect("milestone-154 LicenseRef regex must compile")
    })
}

/// Milestone 154 (closes issue #487) — sweep every emitted
/// `simplelicensing_LicenseExpression` element's expression string for
/// inline `LicenseRef-<idstring>` substrings and emit a matching
/// `simplelicensing_CustomLicense` graph element per distinct LicenseRef.
///
/// Paired follow-up to milestone 153 which added the SPDX 2.3
/// `hasExtractedLicensingInfos[]` sweep. The two milestones together
/// preserve cross-format symmetry (spec FR-009 / FR-010 / FR-011): the
/// SPDX 2.3 side's `hasExtractedLicensingInfos[]` entries and this
/// milestone's `simplelicensing_CustomLicense` elements describe the
/// same LicenseRef set with byte-identical placeholder text.
///
/// # Field shape (per SPDX 3.0.1 spec § licensing_CustomLicense)
///
/// ```json
/// {
///   "type": "simplelicensing_CustomLicense",
///   "spdxId": "{doc_iri}/licenseref/{idstring}",
///   "creationInfo": "{creation_info_id}",
///   "name": "{idstring}",
///   "simplelicensing_licenseText": "{PLACEHOLDER_EXTRACTED_TEXT byte-identical to milestone 153}"
/// }
/// ```
///
/// # Behavior
///
/// 1. Iterate over `license_expression_elements`; tolerate (no-op on)
///    entries where `type != "simplelicensing_LicenseExpression"`.
/// 2. For each matching entry, extract capture-group-1 matches of
///    `license_ref_regex()` from the `simplelicensing_licenseExpression`
///    string; strip the `LicenseRef-` prefix to derive the idstring.
/// 3. Dedup by idstring via `BTreeMap<String, Value>`; existing entry
///    wins on collision (first-seen semantics; all constructions
///    identical anyway).
/// 4. Return `map.into_values().collect()` — `BTreeMap`'s lex ordering
///    on the idstring key produces deterministic output. Since spdxId
///    suffix = idstring, this is equivalent to sort-by-spdxId.
///
/// # Empty-in, empty-out
///
/// Empty input → empty output. Combined with the caller's push-each-
/// element loop at `v3_document.rs`, this guarantees zero
/// `simplelicensing_CustomLicense` elements in `@graph` for happy-path
/// scans (byte-identity preserved).
pub(super) fn sweep_custom_licenses(
    license_expression_elements: &[Value],
    doc_iri: &str,
    creation_info_id: &str,
) -> Vec<Value> {
    let re = license_ref_regex();
    let mut by_idstring: BTreeMap<String, Value> = BTreeMap::new();

    for elem in license_expression_elements {
        // Tolerate non-matching entries (defensive: caller passes only
        // license_expression elements today, but the type-check keeps
        // the helper robust against future refactors).
        if elem["type"].as_str() != Some("simplelicensing_LicenseExpression") {
            continue;
        }
        let Some(expr) = elem["simplelicensing_licenseExpression"].as_str() else {
            continue;
        };
        for cap in re.captures_iter(expr) {
            if let Some(m) = cap.get(1) {
                let license_ref_token = m.as_str();
                // Regex guarantees the match starts with "LicenseRef-".
                let idstring = license_ref_token
                    .strip_prefix("LicenseRef-")
                    .unwrap_or(license_ref_token)
                    .to_string();
                by_idstring.entry(idstring.clone()).or_insert_with(|| {
                    json!({
                        "type": "simplelicensing_CustomLicense",
                        "spdxId": format!("{doc_iri}/licenseref/{idstring}"),
                        "creationInfo": creation_info_id,
                        "name": idstring,
                        "simplelicensing_licenseText": PLACEHOLDER_EXTRACTED_TEXT,
                    })
                });
            }
        }
    }

    by_idstring.into_values().collect()
}

// ----------------------------------------------------------------------
// Milestone 154 / Issue #487: SPDX 3 CustomLicense sweep tests
// ----------------------------------------------------------------------
//
// Tests for `sweep_custom_licenses` + the placeholder-const import
// invariant. Paired follow-up to milestone 153's SPDX 2.3
// hasExtractedLicensingInfos sweep tests in `document.rs`.
//
// See `specs/154-spdx3-custom-licenses/` for the spec/plan/tasks.

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Constant used by every test's `doc_iri` argument. Kept short
    /// for readability; the actual doc IRI shape doesn't matter for
    /// the sweep behavior — the sweep just concatenates the base with
    /// `/licenseref/<idstring>`.
    const DOC_IRI: &str = "https://example.com/doc";
    const CREATION_INFO_ID: &str = "_:creation-info";

    /// Helper: build a synthetic simplelicensing_LicenseExpression
    /// element with the given expression string.
    fn mk_license_expr(expr: &str) -> Value {
        json!({
            "type": "simplelicensing_LicenseExpression",
            "spdxId": format!("{DOC_IRI}/license-decl-mock"),
            "creationInfo": CREATION_INFO_ID,
            "simplelicensing_licenseExpression": expr,
        })
    }

    #[test]
    fn sweep_custom_licenses_single_expression_single_licenseref() {
        // US1 A2 (liblzma5 case): single-operand LicenseRef- becomes
        // one CustomLicense element with correct IRI + name + text.
        let inputs = vec![mk_license_expr("LicenseRef-PD")];
        let out = sweep_custom_licenses(&inputs, DOC_IRI, CREATION_INFO_ID);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["type"], "simplelicensing_CustomLicense");
        assert_eq!(
            out[0]["spdxId"],
            format!("{DOC_IRI}/licenseref/PD")
        );
        assert_eq!(out[0]["creationInfo"], CREATION_INFO_ID);
        assert_eq!(out[0]["name"], "PD");
        assert_eq!(
            out[0]["simplelicensing_licenseText"],
            PLACEHOLDER_EXTRACTED_TEXT
        );
    }

    #[test]
    fn sweep_custom_licenses_compound_expression() {
        // US1 A1 (busybox case): compound expression yields ONE element
        // for the LicenseRef- (GPL-2.0-only is a bare SPDX id, not
        // extracted).
        let inputs = vec![mk_license_expr(
            "GPL-2.0-only AND LicenseRef-bzip2-1.0.4",
        )];
        let out = sweep_custom_licenses(&inputs, DOC_IRI, CREATION_INFO_ID);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0]["spdxId"],
            format!("{DOC_IRI}/licenseref/bzip2-1.0.4")
        );
        assert_eq!(out[0]["name"], "bzip2-1.0.4");
    }

    #[test]
    fn sweep_custom_licenses_dedup_across_expressions() {
        // US1 A3: 4 busybox-family expressions all referencing the
        // same LicenseRef → exactly ONE element (dedup by idstring).
        let inputs: Vec<Value> = (0..4)
            .map(|_| mk_license_expr("GPL-2.0-only AND LicenseRef-bzip2-1.0.4"))
            .collect();
        let out = sweep_custom_licenses(&inputs, DOC_IRI, CREATION_INFO_ID);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["name"], "bzip2-1.0.4");
    }

    #[test]
    fn sweep_custom_licenses_nested_compound_structure() {
        // FR-005 per analysis remediation A1: nested compound with
        // multiple distinct LicenseRefs. Both extracted regardless of
        // operator (AND/OR) surroundings + paren nesting. Returned Vec
        // sorted lex by idstring via BTreeMap ordering.
        let inputs = vec![mk_license_expr(
            "MIT AND LicenseRef-foo OR (LicenseRef-bar AND Apache-2.0)",
        )];
        let out = sweep_custom_licenses(&inputs, DOC_IRI, CREATION_INFO_ID);
        assert_eq!(out.len(), 2);
        // Sorted: `bar` before `foo`.
        assert_eq!(out[0]["name"], "bar");
        assert_eq!(
            out[0]["spdxId"],
            format!("{DOC_IRI}/licenseref/bar")
        );
        assert_eq!(out[1]["name"], "foo");
        assert_eq!(
            out[1]["spdxId"],
            format!("{DOC_IRI}/licenseref/foo")
        );
    }

    #[test]
    fn sweep_custom_licenses_ignores_document_ref_prefixed() {
        // Edge Case: DocumentRef-<doc>:LicenseRef-<id> refers to a
        // LicenseRef defined in ANOTHER document; must NOT emit an
        // element for it here.
        let inputs = vec![mk_license_expr(
            "MIT AND DocumentRef-external:LicenseRef-foo",
        )];
        let out = sweep_custom_licenses(&inputs, DOC_IRI, CREATION_INFO_ID);
        assert_eq!(
            out.len(),
            0,
            "DocumentRef-prefixed LicenseRef must not emit a CustomLicense"
        );
    }

    #[test]
    fn cross_format_placeholder_identity() {
        // Bonus test (T014 / research §R6): milestone 154 imports the
        // milestone-153 placeholder const via `pub(crate)` visibility
        // promotion. This test asserts the imported const value starts
        // with the byte-exact prefix documented in the milestone-153
        // Clarifications Q1 wire contract — mechanically locks
        // FR-010 (cross-format placeholder identity) at compile time.
        //
        // Any accidental future edit to milestone-153's const value
        // trips this test AND milestone-153's own
        // `placeholder_text_matches_wire_contract` test simultaneously.
        assert!(
            PLACEHOLDER_EXTRACTED_TEXT
                .starts_with("License text not extracted by mikebom."),
            "placeholder wire contract (Clarifications Q1) must not drift; \
             milestone 153 + 154 share the same const via `pub(crate)`"
        );
        // Verify the pointer / consult-source pattern is present too.
        assert!(
            PLACEHOLDER_EXTRACTED_TEXT.contains("/usr/share/licenses/<name>/"),
            "placeholder must include the /usr/share/licenses/<name>/ pointer"
        );
    }

    #[test]
    fn sweep_custom_licenses_no_licenserefs_returns_empty() {
        // US2 A2 + FR-007: no LicenseRef-* anywhere → returned Vec is
        // empty. Combined with the wiring at v3_document.rs (push each
        // returned element onto @graph), an empty return means zero
        // simplelicensing_CustomLicense elements appear in @graph —
        // byte-identity preserved for happy-path scans.
        let inputs = vec![
            mk_license_expr("MIT"),
            mk_license_expr("Apache-2.0 AND GPL-2.0-only"),
        ];
        let out = sweep_custom_licenses(&inputs, DOC_IRI, CREATION_INFO_ID);
        assert!(out.is_empty(), "no LicenseRefs → empty Vec");
    }
}
