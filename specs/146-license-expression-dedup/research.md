# Research — milestone 146 (SPDX license expression operand dedup)

Phase 0 output. Resolves the substantive design questions for the dedup pass.

## §A — `spdx = "0.10"` crate tree-walking API (was spec Assumption 1)

**Decision**: Use `spdx::Expression::iter()` returning `&ExprNode` (postfix order). The crate's tree-walking API is sufficient and clean — the string-split fallback the spec hedged for is not needed.

**Verification** (read directly from `~/.cargo/registry/src/index.crates.io-*/spdx-0.10.9/src/expression.rs`):

```rust
pub enum Operator {
    And,
    Or,
}

pub enum ExprNode {
    Op(Operator),
    Req(ExpressionReq),
}

pub struct Expression {
    pub(crate) expr: SmallVec<[ExprNode; 5]>,  // postfix-stored
    pub(crate) original: String,
}

impl Expression {
    pub fn requirements(&self) -> impl Iterator<Item = &ExpressionReq>;
    pub fn iter(&self) -> impl Iterator<Item = &ExprNode>;  // postfix
}
```

`LicenseReq` (the parsed inner type of `ExpressionReq`) implements `Display` at `spdx-0.10.9/src/lib.rs:251`:

```rust
impl fmt::Display for LicenseReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        self.license.fmt(f)?;
        if let Some(ref exe) = self.exception {
            write!(f, " WITH {}", exe.name)?;
        }
        Ok(())
    }
}
```

This is the critical detail: `LicenseReq::Display` includes the ` WITH <exception>` suffix when present. So a `LicenseReq` representing `GPL-2.0-or-later WITH Classpath-exception-2.0` produces the full string `"GPL-2.0-or-later WITH Classpath-exception-2.0"` via `.to_string()`. Byte-comparing two such reqs as atomic operands naturally treats the WITH-clause as one unit, satisfying spec FR-003 without any special-case handling.

**Rationale**: Tree-walking via the crate's typed API is faster, more correct, and more maintainable than string-splitting. The crate already handles the SPDX 2.x grammar's precedence (`AND` binds tighter than `OR`); a string-split would have to re-implement parens-balance tracking.

**Alternatives considered**:
- **String-split on ` AND ` / ` OR ` with paren-depth tracking** — rejected: re-implements parser logic; brittle around `WITH` clauses if anyone writes `(GPL-2.0-only WITH X) AND ...`.
- **Walk `requirements()` directly** (skipping operators) — rejected: doesn't tell us whether the outermost connector is AND or OR, so we can't choose the right separator for rejoining.
- **Use `Expression::evaluate()` with a custom predicate** — rejected: `evaluate` returns a `bool`, not a re-emit; not designed for our use case.

## §B — Dedup algorithm

**Decision**: Walk the postfix `iter()` output. Determine the outermost connector by inspecting the last `ExprNode::Op`. If all operators in the expression are the same (homogeneous AND-chain or OR-chain — covers the dominant Yocto-shaped input), collect unique `LicenseReq` strings in order of first appearance and rejoin. If the expression has mixed AND/OR operators (e.g., `MIT OR Apache-2.0 AND MIT`), do NOT dedupe (top-level analysis only — recursive dedup deferred per spec Out of Scope §1).

**Sketch** (illustrative; not normative — for plan-phase use only):

```rust
pub fn try_canonical(raw: &str) -> Result<Self, LicenseError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(LicenseError::Empty);
    }
    let expr = spdx::Expression::parse(trimmed)
        .map_err(|e| LicenseError::Invalid(e.to_string()))?;
    let canonical = dedupe_top_level_operands(&expr);
    Ok(Self(canonical))
}

fn dedupe_top_level_operands(expr: &spdx::Expression) -> String {
    use spdx::expression::{ExprNode, Operator};

    let nodes: Vec<&ExprNode> = expr.iter().collect();
    if nodes.is_empty() {
        return expr.to_string();
    }

    // Single-operand expressions have no operator nodes — dedup is a no-op.
    let outermost_op = match nodes.last() {
        Some(ExprNode::Op(op)) => *op,
        _ => return expr.to_string(),
    };

    // If the expression has MIXED operators (e.g., AND and OR), the
    // outermost connector is `outermost_op` but inner operands are
    // sub-expressions we don't recursively examine in v1.0 (spec
    // Out of Scope §1). Skip dedup to avoid splitting structure
    // we can't safely reassemble.
    let all_same_op = nodes.iter().all(|n| match n {
        ExprNode::Op(op) => *op == outermost_op,
        _ => true,
    });
    if !all_same_op {
        return expr.to_string();
    }

    // Homogeneous chain — collect req strings (including WITH clauses)
    // and dedupe in order of first occurrence.
    let mut seen = std::collections::BTreeSet::new();
    let mut unique: Vec<String> = Vec::new();
    for n in &nodes {
        if let ExprNode::Req(req) = n {
            let s = req.req.to_string();  // includes WITH exception
            if seen.insert(s.clone()) {
                unique.push(s);
            }
        }
    }

    if unique.len() == 1 {
        return unique.into_iter().next().unwrap();
    }

    let sep = match outermost_op {
        Operator::And => " AND ",
        Operator::Or => " OR ",
    };
    unique.join(sep)
}
```

**Properties guaranteed by this design**:
- **FR-002** (preserves first-occurrence order): `seen.insert()` returns true only on first sight; later identical operands are dropped without disturbing earlier ones.
- **FR-003** (WITH clauses atomic): `req.req.to_string()` includes the ` WITH <exception>` suffix, so two reqs differing only in WITH-exception produce different strings → not deduped.
- **FR-004** (no-op for single-operand / already-deduped / mixed-operator inputs): single-operand expressions return early (no `Op` in last position); mixed-operator expressions return `expr.to_string()` unchanged; already-deduped expressions have all-distinct req strings → `unique` mirrors input order.
- Edge case `(GPL-2.0-only) AND GPL-2.0-only` (parenthesized vs bare): `spdx::Expression::parse` normalizes to remove redundant parens before storage (verified empirically — `parse("(MIT)").to_string()` → `"MIT"`), so both operands surface as `GPL-2.0-only` and dedupe.

**Rationale**:
- Tree-walking via the crate's API is faster and more correct than string-walking.
- The "outermost op + all-same-op" check is a 5-line guard that cleanly defers mixed-operator and parenthesized-sub-expression cases to a future milestone (spec Out of Scope §1).
- The `BTreeSet`-tracked-Vec idiom is the canonical Rust pattern for "dedupe preserving first occurrence."

**Alternatives considered**:
- **HashSet instead of BTreeSet** — equivalent semantics for tracking-only-seen; BTreeSet's ordered iteration is unused here, so either works. BTreeSet has a small allocation advantage on tiny inputs (typical: 2-5 operands). Either is fine for v1.0.
- **Use `Expression::requirements()` + reconstruct** — rejected: see §A alternative.
- **Recurse into parenthesized sub-expressions** — out of scope per spec Out of Scope §1; would require building an AST representation the crate doesn't expose directly.

## §C — Test strategy

**Decision**: 7-8 unit tests in `mikebom-common/src/types/license.rs#mod tests` covering each acceptance scenario, plus one integration test in `mikebom-cli/tests/license_dedup_integration_md146.rs` that exercises the end-to-end CDX + SPDX 2.3 + SPDX 3 emission via a synthetic RPM (built at runtime via `rpm::PackageBuilder` — the same pattern milestone 144 used for its rpm_file tests).

**Unit tests** (covers spec SC-002 + SC-005):
1. `try_canonical_dedupes_two_identical_and_operands` — `"MIT AND MIT"` → `"MIT"` (US1.1, SC-002 anchor)
2. `try_canonical_dedupes_with_distinct_operand_preserved` — `"MIT AND Apache-2.0 AND MIT"` → `"MIT AND Apache-2.0"` (US1.2)
3. `try_canonical_dedupes_multiple_occurrences_preserves_first_order` — `"GPL-2.0-only AND GPL-2.0-only AND LGPL-2.1-or-later AND GPL-2.0-only"` → `"GPL-2.0-only AND LGPL-2.1-or-later"` (US1.3)
4. `try_canonical_already_deduped_unchanged` — `"MIT AND Apache-2.0"` → `"MIT AND Apache-2.0"` (US1.4)
5. `try_canonical_dedupes_or_operands` — `"MIT OR MIT"` → `"MIT"` (US2.1)
6. `try_canonical_dedupes_or_chain_distinct_preserved` — `"MIT OR Apache-2.0 OR MIT"` → `"MIT OR Apache-2.0"` (US2.2)
7. `try_canonical_with_clauses_preserved_atomic` — `"GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later"` does NOT dedupe (the two operands differ — one has WITH, the other doesn't); the SAME WITH-clause on both sides DOES dedupe (SC-005)
8. `try_canonical_single_operand_unchanged` — `"MIT"` → `"MIT"` (FR-004 no-op guard)

**Integration test** (covers SC-004):
- `tests/license_dedup_integration_md146.rs::license_dedup_end_to_end_via_synthetic_rpm`
  - Build a synthetic RPM with `License: MIT AND MIT` via `rpm::PackageBuilder::license("MIT AND MIT")`.
  - Scan the tempdir with `mikebom sbom scan --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json`.
  - Assert the emitted CDX `licenses[0].license.id == "MIT"` (NOT `expression: "MIT AND MIT"`), SPDX 2.3 `licenseDeclared == "MIT"`, SPDX 3 `software_declaredLicense == "MIT"`.

**Rationale**: Unit tests provide the CI-binding signal; the integration test gives end-to-end byte-equivalence confirmation that CDX/SPDX 2.3/SPDX 3 all benefit from the single fix.

## §D — Golden refresh scope

**Decision**: Run all three golden-refresh env vars (`MIKEBOM_UPDATE_CDX_GOLDENS=1`, `MIKEBOM_UPDATE_SPDX_GOLDENS=1`, `MIKEBOM_UPDATE_SPDX3_GOLDENS=1`) and inspect the diffs. Expectation: most fixtures' license strings are single SPDX ids (no AND) and won't change. Any fixture whose pre-145 emission carried an `X AND X` shape will see a one-line diff per affected component (license string collapses).

**Rationale**:
- Predicting the exact golden-diff set requires scanning every fixture — easier to just run the refresh and inspect the diff.
- The accept-criteria for the refresh: every diff must be a license-string simplification (single grep-able pattern: `"<X> AND <X>"` → `"<X>"`); reject any unrelated drift.

**Verification command** (after refresh):

```bash
git diff --stat -- mikebom-cli/tests/fixtures/golden/
# Inspect each file's diff for the expected pattern only.
```

## §E — Risk of breaking the dpkg copyright reader

**Decision**: No risk. The dpkg copyright reader at `mikebom-cli/src/scan_fs/package_db/copyright.rs:120-148` ALREADY dedupes its own extracted candidates via a `BTreeSet<String>` keyed by the canonical-form string (per `extract_licenses` lines 132-145). So a dpkg `copyright` file declaring `License: MIT` in three different `Files:` stanzas produces ONE `SpdxExpression("MIT")` — no `X AND X` shape ever reaches `try_canonical` from the dpkg path. The 146 fix is orthogonal.

**Verification**: Read `mikebom-cli/src/scan_fs/package_db/copyright.rs:132-147` — the `BTreeSet`-keyed dedup is already in place for the per-file-license extraction. Confirmed; no change needed in this reader.

## §F — Other reader-side license-stamping audit

**Decision**: No other reader is known to produce `X AND X` shape internally. The only path that introduces the shape is upstream RPM `License:` header concatenation (Yocto bitbake behavior; out of mikebom scope). All other readers either:
- Stamp from a single source string per package (RPM, apk, gem, etc.) — single license per Vec entry, no internal duplication.
- Already dedupe at the reader level (dpkg copyright, per §E above).
- Don't stamp licenses at all (Go, Cargo lockfile-based readers).

**Verification**: `grep -rn 'licenses\|License' mikebom-cli/src/scan_fs/package_db/` (already done during /speckit-specify investigation) shows single-license stamping at rpm.rs:608, rpm_file.rs:455, yocto/recipe.rs:600. None concatenate.

## Summary of decisions feeding Phase 1

- **§A**: Use `spdx::Expression::iter()` + `LicenseReq::Display` (provides WITH-atomic comparison). No string-walking fallback.
- **§B**: Postfix-walk algorithm with "outermost op + all-same-op" guard. Mixed-operator and parenthesized-sub-expression cases are deferred to a future milestone per spec Out of Scope §1.
- **§C**: 8 unit tests + 1 integration test.
- **§D**: All three golden-refresh env vars; expect small narrow diffs (most fixtures unaffected).
- **§E**: dpkg copyright reader already dedupes; no risk.
- **§F**: No other reader-side dedup work needed.
- **No new Cargo dependencies.**
