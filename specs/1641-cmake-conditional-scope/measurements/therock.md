# T021: TheRock measure-source (SC-004, SC-005)

## Runs

| run | waybill branch | base (`main`) | result |
|---|---|---|---|
| 44953640982 | `8d76de26` | `886792b7` | found one untraced change (Catch2); fixed in `dce5cc36` |
| 44959627444 | `49c12ee8` (final) | `886792b7` | every change traced |

Both are runs of `measure-source` in the TheRock SDK fixture repository, dispatched `--ref main -f waybill_ref=<branch> -f base_ref=886792b7…`. Each has one job, `measure`, which succeeded. The final run's `refs.txt`:

```
base 886792b776a36116b1e8657335c3e89ccf203e75
branch 49c12ee89587bd61ca77c2c86a97caa3c7231224
therock b1cae390cc745283dce25581415353f9bc906162
```

`dce5cc36` was measured too, but its runs were cancelled when the gate (`gate-log.md`) needed one more source change. `49c12ee8` is that change plus the six fixture updates.

## Time (SC-005)

The submodule scan: a warm-up, then 3 timed runs per side (`times.txt`).

| side | run 0 | run 1 | run 2 | median |
|---|---|---|---|---|
| `main` | 31.249 s | 31.080 s | 31.300 s | 31.249 s |
| branch | 31.169 s | 32.243 s | 31.634 s | 31.634 s |

The ratio of the medians, branch over `main`, is **1.012**, within the 1.05 ceiling. At `8d76de26` it was 1.011 (40.471 s over 40.017 s, on a slower runner). The warm-ups (66.2 s for `main`, the cold cache, and 31.7 s for the branch) are excluded.

## Component by component (SC-004)

Each side's CycloneDX document was compared component by component, recursively including `metadata.component`. Every field and every property was compared, and so was the `dependencies` array (each `dependsOn` sorted).

| mode | components (`main` / branch) | same `bom-ref` set | dependencies | changed components |
|---|---|---|---|---|
| submodules | 4,215 / 4,215 | yes | identical | 87 |
| no submodules | 266 / 266 | yes | identical | 50 |

Every changed component has one of four shapes. The columns are lifecycle scope / C62 derivation / C122 / conditions row:

| change (`main` → branch) | submodules | no submodules | traced to |
|---|---|---|---|
| `optional / – / cmake-conditional / C232` → `none / – / – / C232` | 41 | 27 | FR-001, FR-004, FR-005: a fetch whose chain holds or is unknown, C232 kept |
| `optional / – / cmake-conditional / C236` → `none / – / – / C236` | 44 | 2 | FR-007: a package search, C236 kept |
| `optional / – / cmake-conditional / C236` → `development / transitive / – / C236` | 1 | 21 | FR-007, then spec 1269's transitive pass |
| `development / source-example-directory / – / –` → the same plus C236 | 1 | 0 | FR-007 via `record_scopes` |

- **The third shape.** Once a search component is no longer Optional, spec 1269's pass sees that only development edges reach it, and gives it `development` with C62 `transitive`. An example is `pkg:generic/hipblas` without submodules. CycloneDX writes both Optional and Development as `scope: excluded`, which is why that field does not change for these components.
- **The fourth shape** is `pkg:generic/benchmark@1.9.5`: a `cmake-project` survivor that absorbed a `cmake-find-package` member (C56). On `main`, `record_scopes` skipped the record because it already had a scope from the example-directory rule. On the branch its searches are kept in scope, so the pairing lands in `conditions_only` and writes C236 by itself. The scope is unchanged.
- **No other field moves.** That includes PURL, version, C55, C56, edges and the C232/C236 strings, except `benchmark`'s new C236.

**The formats agree** (FR-008). Counted in each document, CycloneDX / SPDX 2.3 / SPDX 3:

| row | submodules, `main` → branch | no submodules, `main` → branch |
|---|---|---|
| C122 | 188 → 102, all three formats | 51 → 1, all three formats |
| C232 | 83 → 83 | 28 → 28 |
| C236 | 98 → 99 | 23 → 23 |

C122 falls by exactly the components that changed shape: 41 + 44 + 1 = 86 with submodules, and 27 + 2 + 21 = 50 without.

**The SPDX documents, structurally.** Masked: `annotationDate`, `created`, `documentNamespace`.
- **SPDX 2.3.** Packages are identical once annotations are set aside. Each removed `OPTIONAL_DEPENDENCY_OF` comes back as the same parent→child edge:
  - with submodules: 142 edges over the 86 changed packages, as 141 `DEPENDS_ON` and 1 `DEV_DEPENDENCY_OF`;
  - without: 50 edges, as 29 `DEPENDS_ON` and 21 `DEV_DEPENDENCY_OF`.

  The annotation changes are those of the CycloneDX properties above. At document level only the two coverage counts change.
- **SPDX 3.** Packages and files are identical, and so is the `(from, to)` set of `dependsOn` edges (6,694 and 284). SPDX 3 has no Optional lifecycle scope, so Optional is carried by annotations alone. The only edge change is to `development` scope: on 1 edge with submodules and 21 without, the `development/transitive` components. The annotation count falls by 172 − 3 = 169 and by 100 − 42 = 58, the per-key changes above.

**C231 and the find coverage** (document-scope `conditional`):

| | submodules | no submodules |
|---|---|---|
| `waybill:cmake-fetch-coverage` | 74 → 32 | 28 → 1 |
| `waybill:cmake-find-coverage` | 63 → 18 | 23 → 0 |

The fetch count falls by 42 with submodules: the 41 visible fetches, plus `pkg:github/rocm/rocm-libraries@rocm-7.2.4`. That fetch is Optional at the reader on `main` and kept on the branch (`NOT DEFINED ROCPRIM_ROOT`, unknown). Spec 1538 R9 then merges it into the `rocm-libraries` submodule on both sides, so it is not a component of either document (FR-006, case R-1). Without submodules, 28 − 1 = 27. The find count falls by 45 = 44 + 1, and by 23 = 2 + 21.

## Against the census

All 41 changed fetch components with submodules are among the census's 48 changing fetch subjects (`census.md`). No change falls outside them. The other 7 subjects:
- **`googletest@v1.14.0`, `@v1.16.0`:** Test (`source-test-directory`) on both sides.
- **`googletest@v1.17.0`:** Optional with the `cmake-not-target` survivor, unchanged. Its kept conditions (`HIPTENSOR_BUILD_TESTS`, `else: HIPTENSOR_USE_SYSTEM_GOOGLETEST`) belong to `rocm-libraries/projects/hiptensor/test/CMakeLists.txt`. That is a test directory, so the declaration is Test, which ranks behind the Optional fallback declaration.
- **`GTest@1.17.0`:** Optional with the `cmake-function-body` survivor, unchanged. *Inference, not traced:* its kept declaration (`else: GTest_FOUND`) must carry Test, since a kept declaration with no scope would outrank the survivor. The artifacts do not name its file.
- **`lapack`, `Catch2@3.8.1`:** an unconditional `ExternalProject` declaration already puts each in scope on `main`.
- **`rocm-libraries@rocm-7.2.4`:** merged into its submodule, as above.

The census's 41 versioned changing fetches match the 41 components. Searches change 45 components of the census's upper bound of 156 names. The bound counts names, and a search becomes a component only when it is paired with a record or the tree builds the project.

## What the first run found: Catch2

At `8d76de26`, one change matched no FR. `pkg:generic/Catch2@3.8.1` swapped its C55 and C56:
- `main`: `cmake-externalproject`, with C56 `["cmake-fetchcontent-url"]`;
- branch: the reverse.

Its unconditional `ExternalProject` declaration and its `FetchContent` declaration under `else: Catch2_FOUND` (unknown, so now kept) both ranked 0 in `one_per_purl`, and the tie went to file order. `dce5cc36` ranks an unconditional declaration first (research R5). Case M-2 reproduces it: it fails before the fix, and passes on `main`'s binary and after the fix. Mutations M23 and M24 are recorded in `mutations.md`. In the final run Catch2 is identical to `main`.

## The function-body change (`49c12ee8`)

`49c12ee8` leaves a search in a `function()` body Optional (`gate-log.md`). On TheRock it changes no component. Three components carry a `function: …` condition in C236:
- `rocshmem_static`: Optional on both sides;
- `rocprofiler_sdk_pytest`: Test on both sides;
- `blas`: in scope through its other searches (`ROCBLAS_ENABLE_CLIENT` holds; one chain is unknown).

The final run's per-component result is therefore that of `8d76de26`, without the Catch2 change.

## Orphans

From `report.md`, unreachable components without an orphan reason: 0 on both sides, in both modes. They do not increase. All four rows of the report are identical between sides: 69 own-declared PyPI packages, all reachable, and root targets 46 / 630 with the formats agreeing.
