//! Render declared + concluded licenses into the CycloneDX 1.6 `licenses` array.
//!
//! Issue #954 follow-up. One renderer, used by both the `metadata.component`
//! path and the `components[]` path, because they had diverged: the same declared
//! value came out two different ways in one document depending on where the
//! component sat.
//!
//! # What CycloneDX 1.6 says
//!
//! `licenses` is a `oneOf`:
//!
//! > EITHER (list of SPDX licenses and/or named licenses) OR (tuple of one SPDX
//! > License Expression)
//!
//! and the expression form is for the case where *"the relationship between
//! multiple licenses requires specific logical operators"*, its own example being
//! `Apache-2.0 AND (MIT OR GPL-2.0-only)`.
//!
//! Critically, the spec does **not** define whether multiple entries in the array
//! form are conjunctive or disjunctive.
//!
//! That cuts differently for the two operators, which is the crux and took three
//! attempts to settle:
//!
//! - **OR**: splitting erases a choice. Nothing in the array form says "either",
//!   so `Unlicense OR MIT` becomes indistinguishable from "both apply". The
//!   operator is load-bearing, so it needs the `expression` slot.
//! - **AND**: multiple entries are read as conjunctive by consumers, so splitting
//!   states the same thing — *and* keeps each listed operand in `license.id`,
//!   where compliance tooling can match it. Collapsing AND into an expression
//!   hides `GPL-2.0-only` from every id-matching consumer, which is a real loss
//!   for OS packages where most licensed components live.
//!
//! An earlier version of this module treated the operators symmetrically and
//! collapsed both. `tests/ipk_license_splitter_m202.rs` caught it on
//! `GPL-2.0-only AND bzip2-1.0.4` — an integration test scanning a real fixture,
//! where the unit tests had agreed with the mistake.
//!
//! # What was wrong
//!
//! Both paths mishandled a compound expression, differently:
//!
//! - `components[]` **split** `Unlicense OR MIT` into `{"id":"Unlicense"}` plus
//!   `{"id":"MIT"}`. Since the array form carries no operator, "either one" and
//!   "both apply" became indistinguishable. Observed across all ten licensed
//!   components of the `rust-ripgrep` corpus target.
//! - `metadata.component` put the whole compound in `license.name`. That keeps
//!   the operator textually but claims the value is a *named* license rather than
//!   an expression, so a consumer parsing `expression` never sees it.
//!
//! The split was introduced deliberately (milestone 202) to populate
//! `license.id`, which sbomqs's licensing checks read. Reversing it looked like a
//! trade — spec conformance against a quality score — so it was **measured**
//! rather than argued, with sbomqs v2.1.1 over the same document in both
//! shapes:
//!
//! | Shape | Total | Licensing | `comp_with_licenses` | `comp_with_valid_licenses` | `comp_spdx_listed_license` |
//! |---|---|---|---|---|---|
//! | split per-id | 7.4/10 | 8.8/10 | 10.0 | 10.0 | 10.0 |
//! | single `expression` | 7.4/10 | 8.8/10 | 10.0 | 10.0 | 10.0 |
//!
//! Identical. sbomqs scores the expression form exactly as it scores split ids, so
//! **there is no trade**: the split cost the operator and bought nothing.
//!
//! Two details that make the measurement worth repeating rather than trusting.
//! Those checks read **concluded** licenses, so a first attempt using an offline
//! scan (declared only) scored 0.0 for both shapes and proved nothing. And
//! Constitution Principle V would make the spec win regardless — but "the spec
//! says so" is a weaker argument to leave in a comment than a number.
//!
//! # The rule
//!
//! | Value | Slot |
//! |---|---|
//! | a single SPDX-listed identifier (`MIT`) | `license.id` |
//! | an **OR** compound (`Unlicense OR MIT`) | `expression` |
//! | an **AND** of single tokens (`GPL-2.0-only AND bzip2-1.0.4`) | split: one entry each, ids preserved |
//! | any other valid expression (parens, mixed operators, `WITH`) | `expression` |
//! | a **bare** `LicenseRef-…` / `DocumentRef-…`, with no operator | `license.name` |
//! | anything else (`The Apache Software License, Version 2.0`) | `license.name` |
//!
//! Note the second and third rows interact: the SPDX grammar permits
//! `LicenseRef-` as an *operand*, so `MIT AND LicenseRef-Acme` is a valid
//! expression and takes the `expression` slot. Only a bare reference with no
//! operator falls to `name`. A probe test records the parser's verdicts, because
//! the slot choice depends on them and they are not guessable.
//!
//! The expression form is a *tuple of one*, so it can only be used when the
//! component has exactly one license value in total. When a component carries
//! both a declared and a concluded license and either is compound, the array form
//! is used with the compound in `license.name`: that preserves both
//! acknowledgements and the operator text, which matters more than reaching the
//! ideal slot. It is never split.

use serde_json::json;
use waybill_common::types::license::SpdxExpression;

/// Whether `s` is a valid SPDX expression that carries an operator.
///
/// Both halves matter. The operator check alone would send prose containing the
/// word "and" to the `expression` slot; the parse check alone would send a bare
/// `MIT` there, when `license.id` is the better slot for a listed identifier.
fn is_compound_expression(s: &str) -> bool {
    let has_operator = s
        .split_whitespace()
        .any(|t| t == "AND" || t == "OR" || t == "WITH");
    has_operator && spdx::Expression::parse(s).is_ok()
}

/// One `{license: {...}}` array entry for a value that is not being emitted as
/// an expression.
fn array_entry(value: &SpdxExpression, ack: &str) -> serde_json::Value {
    match value.as_spdx_id() {
        // A listed identifier belongs in `id`; that is also the form sbomqs
        // counts, so the common case is unaffected by this module's changes.
        Some(id) => json!({ "license": { "id": id, "acknowledgement": ack } }),
        // Everything else goes in `name`: a `LicenseRef-`, a compound that could
        // not take the expression slot, or free-form prose. `id` is restricted to
        // the SPDX list, so putting any of these there would be a schema error.
        None => json!({ "license": { "name": value.as_str(), "acknowledgement": ack } }),
    }
}

/// Render `declared` and `concluded` into the CDX 1.6 `licenses` array.
///
/// Returns an empty vec when there is nothing to emit, so callers can skip the
/// key entirely rather than writing `"licenses": []`.
pub(super) fn render_licenses(
    declared: &[SpdxExpression],
    concluded: &[SpdxExpression],
) -> Vec<serde_json::Value> {
    let mut pairs: Vec<(&SpdxExpression, &str)> = Vec::new();
    for l in declared {
        pairs.push((l, "declared"));
    }
    for l in concluded {
        pairs.push((l, "concluded"));
    }

    // AND-of-single-tokens splits into one entry per operand. That keeps each
    // listed operand in `license.id` where a compliance tool can match it, and
    // multiple entries read as conjunctive, so nothing is misstated. Applied
    // per value, so it composes with a second value from the other source.
    let mut split_entries: Vec<serde_json::Value> = Vec::new();
    let mut all_splittable = !pairs.is_empty();
    for (value, ack) in &pairs {
        match try_split_and_compound(value.as_str()) {
            Some(tokens) => {
                for tok in tokens {
                    let entry = license_entry_for_token(&tok, ack);
                    if !entry.is_null() {
                        split_entries.push(entry);
                    }
                }
            }
            None => {
                all_splittable = false;
                break;
            }
        }
    }
    if all_splittable && !split_entries.is_empty() {
        return split_entries;
    }

    match pairs.as_slice() {
        [] => Vec::new(),
        // The expression form is a tuple of ONE, so it is only reachable when the
        // component has a single license value overall. This is where an OR
        // compound lands: the choice cannot be represented by array entries.
        [(only, ack)] if is_compound_expression(only.as_str()) => {
            vec![json!({ "expression": only.as_str(), "acknowledgement": ack })]
        }
        _ => pairs
            .iter()
            .map(|(value, ack)| array_entry(value, ack))
            .collect(),
    }
}

fn try_split_and_compound(expr: &str) -> Option<Vec<String>> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('(') || trimmed.contains(')') {
        return None;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.contains(&"WITH") {
        return None;
    }
    // Pick a single top-level operator. Mixed operators (e.g.
    // `A AND B OR C`) require parens for unambiguous parsing, so
    // bail — let the single-expression fallback handle them.
    let has_or = tokens.contains(&"OR");
    let has_and = tokens.contains(&"AND");
    // Issue #954 follow-up: AND only. Splitting `A OR B` into array entries
    // erases the choice, because CDX defines no operator between entries — a
    // consumer cannot tell it from "both apply". OR therefore goes to the
    // `expression` slot instead (see `licenses.rs`). AND is different: multiple
    // entries read as conjunctive, so splitting preserves the meaning AND keeps
    // each listed operand matchable in `license.id`, which is what milestone 202
    // was for. Dropping that was an over-correction, caught by
    // `tests/ipk_license_splitter_m202.rs` on `GPL-2.0-only AND bzip2-1.0.4`.
    let separator = match (has_or, has_and) {
        (false, true) => " AND ",
        _ => return None,
    };
    let parts: Vec<&str> = trimmed.split(separator).map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    let mut tokens_out = Vec::with_capacity(parts.len());
    for p in parts {
        // Every operand must be a single token (SPDX id or
        // LicenseRef-*); whitespace inside an operand means the
        // expression has nested operators we can't flatten.
        if p.is_empty() || p.contains(char::is_whitespace) {
            return None;
        }
        tokens_out.push(p.to_string());
    }
    Some(tokens_out)
}

/// Map one split-expression token to the right CDX `license` shape.
///
/// Three-branch classifier post-milestone 202 (closes #579):
///
/// 1. **Pre-formed reference** (`LicenseRef-*` / `DocumentRef-*`): route to
///    `license.name` verbatim (schema-legal free-text label; sbomqs counts
///    it via `comp_with_licenses`).
/// 2. **SPDX-list-canonical identifier** (member of the SPDX License List
///    per `spdx::license_id`, or an SPDX exception per `spdx::exception_id`):
///    route to `license.id` — the canonical CDX 1.6 §5.4.4.1 slot. Value
///    preserved verbatim (no `try_canonical` normalization — that would
///    silently rewrite legacy long-form names like `GPL-2.0` → `GPL-2.0-only`
///    and drift emitted goldens).
/// 3. **Non-canonical operand** (compound-expression operand that isn't on
///    the SPDX List, e.g. `bzip2-1.0.4` from a Yocto recipe License field):
///    route to `license.name = "LicenseRef-<sanitized>"` per CDX 1.6
///    §5.4.4.2 escape-hatch convention. Uses the shared
///    `waybill_common::types::license::sanitize_license_operand_to_ref`
///    helper — same function the SPDX 2.3 emitter uses (m152) — so both
///    formats produce byte-identical `LicenseRef-*` identifiers for the
///    same input token (FR-002 CDX/SPDX 2.3 parity).
///
/// Defensive fallback: if the sanitizer returns `None` (all-invalid-chars
/// input after filtering), emit `serde_json::Value::Null` so the caller's
/// filter drops the entry rather than producing schema-invalid output.
fn license_entry_for_token(token: &str, acknowledgement: &str) -> serde_json::Value {
    if token.starts_with("LicenseRef-") || token.starts_with("DocumentRef-") {
        return json!({
            "license": {
                "name": token,
                "acknowledgement": acknowledgement,
            }
        });
    }
    let is_spdx_list_id =
        spdx::license_id(token).is_some() || spdx::exception_id(token).is_some();
    if is_spdx_list_id {
        return json!({
            "license": {
                "id": token,
                "acknowledgement": acknowledgement,
            }
        });
    }
    match waybill_common::types::license::sanitize_license_operand_to_ref(token) {
        Some(sanitized) => json!({
            "license": {
                "name": format!("LicenseRef-{sanitized}"),
                "acknowledgement": acknowledgement,
            }
        }),
        None => serde_json::Value::Null,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn e(s: &str) -> SpdxExpression {
        SpdxExpression::new(s).unwrap()
    }

    #[test]
    fn a_single_listed_identifier_uses_the_id_slot() {
        let got = render_licenses(&[e("MIT")], &[]);
        assert_eq!(got, vec![json!({"license":{"id":"MIT","acknowledgement":"declared"}})]);
    }

    #[test]
    fn a_compound_expression_uses_the_expression_slot_and_is_not_split() {
        // The rust-ripgrep case. Previously two `id` entries, which drops the OR.
        let got = render_licenses(&[e("Unlicense OR MIT")], &[]);
        assert_eq!(
            got,
            vec![json!({"expression":"Unlicense OR MIT","acknowledgement":"declared"})],
            "an operator has nowhere to live in the array form, so it must use \
             the expression slot"
        );
        assert_eq!(got.len(), 1, "never split into one entry per operand");
    }

    #[test]
    fn a_parenthesised_compound_also_uses_the_expression_slot() {
        let got = render_licenses(&[e("MIT AND (Apache-2.0 OR BSD-2-Clause)")], &[]);
        assert_eq!(
            got,
            vec![json!({
                "expression":"MIT AND (Apache-2.0 OR BSD-2-Clause)",
                "acknowledgement":"declared"
            })]
        );
    }

    #[test]
    fn free_form_prose_uses_the_name_slot_not_the_expression_slot() {
        // The maven-guice case: `<name>The Apache Software License, Version 2.0</name>`.
        // `expression` is defined as a valid SPDX expression, and this is not one.
        let got = render_licenses(&[e("The Apache Software License, Version 2.0")], &[]);
        assert_eq!(
            got,
            vec![json!({
                "license":{
                    "name":"The Apache Software License, Version 2.0",
                    "acknowledgement":"declared"
                }
            })]
        );
    }

    #[test]
    fn prose_containing_the_word_and_is_not_treated_as_an_expression() {
        // Guards the operator check being done alone: this contains "AND" as a
        // token but is not a parseable expression.
        let got = render_licenses(&[e("Terms AND Conditions of Acme Corp")], &[]);
        assert!(
            got[0]["license"]["name"].is_string(),
            "unparseable prose must not reach the expression slot: {got:?}"
        );
    }

    #[test]
    fn a_license_ref_uses_the_name_slot() {
        let got = render_licenses(&[e("LicenseRef-Acme")], &[]);
        assert_eq!(
            got,
            vec![json!({"license":{"name":"LicenseRef-Acme","acknowledgement":"declared"}})]
        );
    }

    #[test]
    fn declared_and_concluded_both_survive_as_array_entries() {
        let got = render_licenses(&[e("MIT")], &[e("Apache-2.0")]);
        assert_eq!(
            got,
            vec![
                json!({"license":{"id":"MIT","acknowledgement":"declared"}}),
                json!({"license":{"id":"Apache-2.0","acknowledgement":"concluded"}}),
            ]
        );
    }

    #[test]
    fn a_compound_beside_a_second_value_keeps_both_and_still_does_not_split() {
        // `expression` is a tuple of one, so the compound cannot take that slot
        // here. It goes to `name` — preserving the operator text and both
        // acknowledgements, which beats dropping either.
        let got = render_licenses(&[e("Unlicense OR MIT")], &[e("MIT")]);
        assert_eq!(got.len(), 2, "both values must survive");
        assert_eq!(got[0]["license"]["name"], "Unlicense OR MIT");
        assert_eq!(got[0]["license"]["acknowledgement"], "declared");
        assert_eq!(got[1]["license"]["id"], "MIT");
        assert_eq!(got[1]["license"]["acknowledgement"], "concluded");
        assert!(
            got.iter().all(|v| v.get("expression").is_none()),
            "mixing array and expression shapes is a schema error: {got:?}"
        );
    }

    #[test]
    fn probe_which_strings_the_spdx_parser_accepts() {
        // Recording observed parser behaviour, because the slot choice depends on
        // it and it is not obvious: SPDX expressions permit `LicenseRef-` as an
        // operand, so a compound containing one IS a valid expression.
        for (s, expect) in [
            ("MIT AND LicenseRef-Acme", true),
            ("MIT AND Apache-2.0", true),
            ("Unlicense OR MIT", true),
            ("The Apache Software License, Version 2.0", false),
            ("Custom-In-House-License", false),
            ("bzip2-1.0.4", false),
        ] {
            assert_eq!(
                spdx::Expression::parse(s).is_ok(),
                expect,
                "parser verdict changed for {s:?}"
            );
        }
    }

    #[test]
    fn an_and_of_single_tokens_splits_and_keeps_ids_matchable() {
        // The ipk case. Collapsing this into `expression` would hide
        // `GPL-2.0-only` from any consumer matching on `license.id`.
        let got = render_licenses(&[e("GPL-2.0-only AND bzip2-1.0.4")], &[]);
        assert_eq!(got.len(), 2, "AND splits: {got:?}");
        assert_eq!(got[0]["license"]["id"], "GPL-2.0-only");
        assert!(
            got[1]["license"]["name"].as_str().is_some_and(|n| n.contains("bzip2")),
            "the non-listed operand keeps its identity in `name`: {got:?}"
        );
    }

    #[test]
    fn or_and_and_are_not_treated_the_same_way() {
        // The asymmetry, asserted directly so it cannot be "simplified" away.
        let or = render_licenses(&[e("Apache-2.0 OR MIT")], &[]);
        let and = render_licenses(&[e("Apache-2.0 AND MIT")], &[]);
        assert_eq!(or.len(), 1, "OR must stay one expression: {or:?}");
        assert!(or[0].get("expression").is_some());
        assert_eq!(and.len(), 2, "AND must split: {and:?}");
        assert!(and.iter().all(|e| e.get("expression").is_none()));
    }

    #[test]
    fn nothing_in_nothing_out() {
        assert!(render_licenses(&[], &[]).is_empty());
    }

    #[test]
    fn the_two_shapes_are_never_mixed() {
        // CDX 1.6 `licenses` is a oneOf; an array containing both an
        // `{expression}` entry and a `{license}` entry validates against neither.
        for (d, c) in [
            (vec![e("MIT OR Apache-2.0")], vec![e("MIT")]),
            (vec![e("MIT")], vec![e("MIT OR Apache-2.0")]),
            (vec![e("MIT OR Apache-2.0")], vec![e("GPL-2.0-only OR MIT")]),
        ] {
            let got = render_licenses(&d, &c);
            let exprs = got.iter().filter(|v| v.get("expression").is_some()).count();
            let objs = got.iter().filter(|v| v.get("license").is_some()).count();
            assert!(
                exprs == 0 || objs == 0,
                "shapes mixed for declared={d:?} concluded={c:?}: {got:?}"
            );
        }
    }
}
