# Data Model — milestone 146

Phase 1 output. Defines the contract change to `SpdxExpression::try_canonical` + the dedup behavior on the stored canonical string.

## Modified type

### `SpdxExpression` (in `mikebom-common/src/types/license.rs`)

Pre-146: stored canonical string from `spdx::Expression::parse + to_string()` round-trip. Preserved duplicate operands verbatim — `MIT AND MIT` parsed and re-displayed as `MIT AND MIT`.

Post-146: same parse step, then a NEW dedup pass on the parsed expression's top-level operands before storing the canonical string.

**Public API surface**: UNCHANGED. The `pub fn try_canonical(raw: &str) -> Result<Self, LicenseError>` signature is identical. Callers see no API churn. Only the VALUE returned for inputs containing duplicate top-level operands changes (collapses).

**Private helper added** (illustrative; not part of the public API contract):

```rust
fn dedupe_top_level_operands(expr: &spdx::Expression) -> String
```

Called inside `try_canonical` between `spdx::Expression::parse` and the `Ok(Self(...))` return.

## Pre/post behavior table

| Input (`try_canonical(input)`) | Pre-146 stored value | Post-146 stored value | Notes |
|---|---|---|---|
| `"MIT"` | `"MIT"` | `"MIT"` | unchanged (no ops) |
| `"MIT AND Apache-2.0"` | `"MIT AND Apache-2.0"` | `"MIT AND Apache-2.0"` | unchanged (already deduped) |
| `"MIT AND MIT"` | `"MIT AND MIT"` | `"MIT"` | **CHANGED** (US1.1) |
| `"GPL-2.0-only AND GPL-2.0-only AND LGPL-2.1-or-later"` | `"GPL-2.0-only AND GPL-2.0-only AND LGPL-2.1-or-later"` | `"GPL-2.0-only AND LGPL-2.1-or-later"` | **CHANGED** (US1.3) |
| `"MIT OR MIT"` | `"MIT OR MIT"` | `"MIT"` | **CHANGED** (US2.1) |
| `"MIT OR Apache-2.0 AND MIT"` | `"MIT OR Apache-2.0 AND MIT"` | `"MIT OR Apache-2.0 AND MIT"` | unchanged (mixed-operator; recursive dedup out of v1.0 scope) |
| `"GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later WITH Classpath-exception-2.0"` | same | `"GPL-2.0-or-later WITH Classpath-exception-2.0"` | **CHANGED** (WITH atomic per FR-003) |
| `"GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later"` | same | same | unchanged (two operands differ — one has WITH, the other doesn't) |
| `""` (empty) | `Err(LicenseError::Empty)` | `Err(LicenseError::Empty)` | unchanged (errors out before dedup) |
| `"Some Random Free Text"` (invalid SPDX) | `Err(LicenseError::Invalid)` | `Err(LicenseError::Invalid)` | unchanged (parse fails before dedup) |

## Validation rules (consolidated from spec FRs)

| Input | Rule | Source |
|---|---|---|
| Homogeneous AND-chain with duplicates | MUST collapse byte-identical operands; preserve first-occurrence order | FR-001 + FR-002 |
| Homogeneous OR-chain with duplicates | Same — `X OR X` ≡ `X` is idempotent | FR-001 (`OR` operator) |
| `WITH` clauses | MUST treat `<license> WITH <exception>` as one atomic operand for byte-comparison | FR-003 |
| Single-operand expression | No-op | FR-004 |
| Already-deduped expression | No-op | FR-004 |
| Mixed AND/OR at top level | No-op (recursive dedup deferred per spec Out of Scope §1) | FR-004 |
| Parenthesized sub-expressions with non-identical top-level wrappers | No-op (recursive dedup deferred) | FR-004 + spec Out of Scope §1 |
| Inputs that fail SPDX 2.x parse | Returns `Err(LicenseError::Invalid)` BEFORE dedup; dedup never runs | FR-006 (raw-storage contract preserved) |
| Empty inputs | Returns `Err(LicenseError::Empty)` | existing behavior unchanged |

## Out of model

- No new types (no structs / enums / newtypes introduced).
- No changes to `LicenseError` variants.
- No changes to `SpdxExpression::new` (the lenient constructor) — preserves its existing best-effort raw-storage contract per FR-006.
- No changes to `SpdxExpression::as_str`, `SpdxExpression::as_single_identifier`, or any other accessor.
- No changes to the public API of any downstream consumer (`mikebom-cli`'s emitters).
