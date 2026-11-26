# After: spec 1657

All figures are from macOS (18 cores). Builds are from the branch, with only
`waybill-cli/src/edit/spdx3.rs` differing between the before and after
binaries (built back to back).

## US1: SPDX 3 edits (#1711)

### Byte identity (FR-002): `edit_identity.sh`

The script makes 252 pairs: every corpus golden (28 targets × 3 formats) under
three recipes (`--drop scope=excluded`, `--drop scope=development,test`,
`--retier design=build`), edited with the before and after release binaries.

- **Control (before against itself):** an unmasked first run reported 13
  pairs differing, and before-against-before differs too.
  - The cause is the derivation record's `created` timestamp and the edited
    document's `-edit-<16 hex>` IRI suffix. Both are wall-clock dependent, and
    `sbom edit` has no `SOURCE_DATE_EPOCH`.
  - The script masks both. With the masks, the control reads
    **0 of 252 differing**.
- **Before against after:** **0 of 252 differing.** 243 edits succeeded; 9
  failed with the same exit status in both binaries.

### Time (FR-003): moat golden, `--drop scope=excluded`, 3 runs each

| binary | SPDX 2.3 | SPDX 3 |
|---|---|---|
| before, debug | 4.62 / 4.63 / 4.64 s | 76.91 / 77.30 / 85.83 s |
| after, debug | 4.69 / 4.74 / 4.94 s | **3.90 / 3.92 / 3.92 s** |
| before, release | 0.50 / 0.51 / 0.53 s | 9.15 / 9.34 / 9.59 s |
| after, release | 0.50 / 0.51 / 0.51 s | **0.55 / 0.56 / 0.56 s** |

The SPDX 3 median is now 0.83× the SPDX 2.3 median in debug and 1.10× in
release. The target was ≤ 2×.

### Suite effect

The `sbom_edit_filter` binary now runs in 37.44 / 37.66 / 38.92 s (18
passed). Before, the i1454 test alone took 150.8 s and the binary 154 s
(#1711's measurement). The remaining time is the other tests' own work.

## US2: each test runs once (#1712)

### Listing: `list_counts.py` over `cargo test -p waybill -- --list`

| | binaries | tests listed | binaries with `corpus_harness_195::` tests | `dual_format_perf::` in another binary |
|---|---|---|---|---|
| before (`list-before.txt`) | 421 | 5,358 | 17 (144 each) | 1 (`holistic_parity`, 2) |
| after (`list-after.txt`) | 422 | 3,052 | 0 | 0 |

- **The drop:** 5,358 − 3,052 = 2,306, which is 16 × 144 plus 2.
- **The new binary:** `corpus_harness_self_tests` holds the 144 harness tests, all passing (`test result: ok. 144 passed`), under new module names.
- **`dual_format_perf`:** its 2 tests now appear only in its own binary.

### Cost of one duplicate: `requirements_owner`, 3 runs

- **Harness tests alone:** 5.20 / 5.43 / 5.70 s.
- **The whole binary:** 5.70 / 5.93 / 5.99 s.
- **Where the time goes:** `invariants::mutation_tests` 5.06 s, `invariants::tests` 2.24 s, `invariants::checks` 1.93 s and `cache::run_with_deadline_tests` 2.01 s, which run in parallel.

So each of the 16 removed copies cost about 5.4 s of serial time, about 87 s
in total.

### How the move was done (`move_harness_tests.py`)

- **What moved:** 22 test modules, out of 17 files under `tests/corpus_harness_195/`. Research R2 first said 6 files; the recursive count is 17, including the `invariants/` subtree.
  - Inline `#[cfg(test)]` and `#[cfg(all(test, unix))]` modules.
  - Whole-file test modules declared in `invariants/mod.rs`.
  - `manifest.rs`'s trailing audit tests.
- **Paths:** each `super::` was rewritten to an absolute `crate::corpus_harness_195::…` path.
- **Imports:** the moved tests got their former parent's `use` statements, deduplicated.
- **Visibility:** 61 private top-level harness items were raised to `pub(crate)`, as were `RecallOutcome`'s fields and `ratio`.
- **Shared helpers:** `ImageFile`, `build_synthetic_image` and `build_benchmark_fixture` moved to `tests/common/synthetic_image.rs`.
- **Recovery:** a first run of the script failed partway, because it deleted a whole-file module the directory walk still listed. A second run mis-rewrote `super`. Each time the tree was restored from a backup with `cp -R` and checked with `diff -r`.

## US3: scan tests and the network (#1715)

`network_probe.sh` measured all 38 candidates (`network-probe-before.txt`); the per-file disposition is in `offline-review.md`.

- **Only two waited on live deps.dev:** `scan_split_basic` and `split_manifest_schema`. Both now pass `--offline`.

  | binary | before (normal) | after, 3 runs |
  |---|---|---|
  | `scan_split_basic` | 58.7 s | 0.19 / 0.11 / 0.11 s (9 passed) |
  | `split_manifest_schema` | 13.0 s | 0.05 / 0.05 / 0.07 s (1 passed) |

- **The other 36** take the same time blocked or not.
- **Every one of the 38** passes with the network blocked, so no assertion depends on live enrichment (FR-009).

### Enforcement point (FR-007): not adopted

A scan sent to an unreachable proxy does not hang. It also does not fail: it degrades the enrichment and passes, as the blocked probe runs show. Setting the proxy in the shared helpers would therefore hide a future online test rather than catch it. The reusable guard is `network_probe.sh` instead: a test that goes online shows up as a normal-against-blocked time gap.

## Mutation (T006)

M1: the index skips `waybill:sbom-tier` values. That fails 11 tests: `sbom_edit_parity::retiered_outputs_agree_and_record_the_mapping` and 10 in `sbom_edit_retier`. All 5 `sbom_edit*` binaries reported. Reverted with `cp`, checked with `cmp`.

The two `--offline` changes need no mutation: their tests already passed with the network blocked (above), which is the same input as `--offline` as far as enrichment is concerned.

## CI (T014): `Tests` step, run 45695514065 against `main`'s run 45613291165

| lane | before | after |
|---|---|---|
| macOS | 1,511 s | 1,135 s (−25%) |
| Linux | 903 s | 683 s (−24%) |
| Windows (non-blocking) | 935 s | 1,146 s (+23%, unexplained; one sample each) |
