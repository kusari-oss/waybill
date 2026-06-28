# Contract — `SpdxExpression::try_canonical` operand-dedup contract

Phase 1 output. Defines the dedup pass contract that `try_canonical` MUST satisfy.

## Contract

`SpdxExpression::try_canonical(raw: &str) -> Result<Self, LicenseError>` MUST return a value whose internal canonical string satisfies ALL the following invariants:

### Invariant 1 — Homogeneous AND-chain dedup

For any input `raw` that parses successfully as a SPDX 2.x expression whose ONLY top-level connector is `AND` (i.e., a homogeneous AND-chain `A AND B AND C ...`), the stored string MUST contain each distinct top-level operand exactly once, in the order of first occurrence.

**Examples**:
- `"MIT AND MIT"` → stored as `"MIT"`
- `"MIT AND Apache-2.0 AND MIT"` → stored as `"MIT AND Apache-2.0"`
- `"GPL-2.0-only AND GPL-2.0-only AND LGPL-2.1-or-later AND GPL-2.0-only"` → stored as `"GPL-2.0-only AND LGPL-2.1-or-later"`

### Invariant 2 — Homogeneous OR-chain dedup

Same as Invariant 1, but for `OR`-chain expressions.

**Examples**:
- `"MIT OR MIT"` → stored as `"MIT"`
- `"MIT OR Apache-2.0 OR MIT"` → stored as `"MIT OR Apache-2.0"`

### Invariant 3 — WITH-clauses treated as atomic operands

For byte-comparison purposes, a `<license-id> WITH <exception-id>` clause is ONE operand. The dedup MUST NOT split across the `WITH` boundary.

**Examples**:
- `"GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later WITH Classpath-exception-2.0"` → stored as `"GPL-2.0-or-later WITH Classpath-exception-2.0"` (both operands byte-identical post-canonical)
- `"GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later"` → stored as `"GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later"` (two operands differ — one has WITH, the other doesn't; no dedup)

### Invariant 4 — Single-operand and already-deduped inputs preserved

For any input that, after canonicalization by `spdx::Expression::parse + to_string()`, has either zero `Op` nodes (single operand) or all-distinct operands, the stored value MUST equal the canonical string verbatim (no transformation).

**Examples**:
- `"MIT"` → stored as `"MIT"`
- `"MIT AND Apache-2.0"` → stored as `"MIT AND Apache-2.0"`

### Invariant 5 — Mixed-operator and parenthesized sub-expressions deferred

For any input whose canonical form contains BOTH `AND` and `OR` operators at the (post-spdx-crate-parse) tree level, OR contains parenthesized sub-expressions whose top-level wrappers aren't byte-identical, the stored value MUST equal the canonical string verbatim (recursive dedup is out of v1.0 scope per spec Out of Scope §1).

**Examples**:
- `"MIT OR Apache-2.0 AND MIT"` (parses as `MIT OR (Apache-2.0 AND MIT)`) → stored as canonical form, unchanged
- `"(MIT AND MIT) OR Apache-2.0"` → if `spdx::Expression::parse + to_string()` produces `"MIT AND MIT OR Apache-2.0"` (or similar), the stored form is the canonical output unchanged. The inner `MIT AND MIT` is NOT recursively deduped in v1.0.

### Invariant 6 — Parse-failure path unchanged

For any input that fails `spdx::Expression::parse`, the returned `Err(LicenseError::Invalid)` is unchanged from pre-146 behavior. The dedup pass MUST NOT run on parse-failed inputs. (`SpdxExpression::new`, the lenient constructor, ALSO MUST NOT apply dedup — preserving its best-effort raw-storage contract.)

### Invariant 7 — Idempotence

Applying `try_canonical` to its own output is a no-op:

```rust
let e1 = SpdxExpression::try_canonical("MIT AND MIT").unwrap();
let e2 = SpdxExpression::try_canonical(e1.as_str()).unwrap();
assert_eq!(e1.as_str(), e2.as_str()); // both "MIT"
```

## Test surface (covers spec SC-002 + SC-004 + SC-005)

| Test | Asserts |
|---|---|
| `try_canonical_dedupes_two_identical_and_operands` | Invariant 1 (US1.1, SC-002) |
| `try_canonical_dedupes_with_distinct_operand_preserved` | Invariant 1 (US1.2) |
| `try_canonical_dedupes_multiple_occurrences_preserves_first_order` | Invariant 1 (US1.3) — order preservation |
| `try_canonical_already_deduped_unchanged` | Invariant 4 (US1.4) |
| `try_canonical_dedupes_or_operands` | Invariant 2 (US2.1) |
| `try_canonical_dedupes_or_chain_distinct_preserved` | Invariant 2 (US2.2) |
| `try_canonical_with_clauses_preserved_atomic` | Invariant 3 (SC-005) — both directions: matching WITH dedupes; differing WITH does not |
| `try_canonical_single_operand_unchanged` | Invariant 4 (single-operand no-op) |
| `try_canonical_mixed_operators_unchanged` | Invariant 5 (mixed-operator no-op; out-of-scope guard) |
| `try_canonical_is_idempotent` | Invariant 7 |
| `try_canonical_empty_returns_error` | Invariant 6 + existing behavior |
| `try_canonical_invalid_returns_error` | Invariant 6 |

## Non-contract (explicitly out of scope)

- Recursive dedup into parenthesized sub-expressions.
- Algebraic simplification (`(MIT AND Apache-2.0) OR (Apache-2.0 AND MIT)` → `MIT AND Apache-2.0`).
- Commutativity / associativity rewrites.
- Operator-precedence-aware re-grouping.
- Cross-`SpdxExpression`-instance dedup at downstream emitters (e.g., merging two Vec<SpdxExpression> at `reduce_license_vec` time). The Vec-level dedup is `reduce_license_vec`'s job; this milestone fixes WITHIN-expression dedup only.
