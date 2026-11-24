# Data model: spec 1654

## `CoverageRowReport.verdict` (new field)

| value | when |
|---|---|
| `pass` | the row is universal, at least one format carries it, and its directionality invariant holds |
| `fail` | the row is universal, at least one format carries it, and the invariant does not hold. These are exactly the rows `summary.parity_gaps` counts. |
| `not-checked` | the row is not universal (format-restricted), or it is universal and no format carries it |

**Invariant:** `count(verdict == "fail") == summary.parity_gaps`. `summary.universal_parity_rows_passing` keeps counting unexercised universal rows as passing, as today.

## The B1/B2 classification (extractors)

An edge `A -> B` is non-runtime (B2) when **B**:
- in CycloneDX, has `scope: "excluded"`;
- in SPDX, is the edge's typed target: `DEV_`, `BUILD_` or `TEST_DEPENDENCY_OF`, read with source and target swapped.

Otherwise the edge is runtime (B1).

## SPDX edge typing, pass 3 (`apply_lifecycle_scope_to_edges`)

| pass | target condition | `DependsOn` becomes |
|---|---|---|
| 1 (milestone 052) | `lifecycle_scope` non-runtime | Dev / Build / Test / Optional |
| 2 (milestone 179) | `build_inclusion == NotNeeded` | Test |
| **3 (new)** | `artifact_presence.is_absent_from_artifact()` | **Build** |

Each pass rewrites only edges still `DependsOn`, so the passes take precedence in order.

Pass 3 only has an effect when the function runs after `closure_role_scope::assign_role_scope` has set `artifact_presence`, so it is called again there (research R4).

## E1, an ecosystem completeness claim

| format | present iff |
|---|---|
| CycloneDX | a `compositions[]` record with `aggregate == "complete"` and non-empty `assemblies` |
| SPDX 2.3 and SPDX 3 | the `compositions` annotation's `complete_ecosystems` is non-empty |

## `tests/common/parity.rs::assert_parity(out_dir: &Path)`

**Runs:** `waybill sbom parity-check --format json --scan-dir <out_dir>`.

**Asserts:**
- no row has `verdict == "fail"`;
- `summary.parity_gaps == 0`;
- the number of `fail` rows equals `summary.parity_gaps`, which guards the verdict against drifting from the count (SC-001).

**On failure:** it panics with each failing row's ID, label, and the three formats' kind and count.
