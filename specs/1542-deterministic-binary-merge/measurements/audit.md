# Determinism audit and measurement (T015, T016, T017)

**Instruments:**
- `probe_compare_documents`, which compares documents after the corpus masking, array order included;
- `probe_i7_on_document`, which reports I-A, I7 and C45/C104/C105 across the formats.

Both are ignored tests in `public_corpus.rs`. TheRock documents are CI artifacts, read locally.

## TheRock built SDK (`measure-full` 44212407420 and 44212477435; branch `455a6d1f`, base `main` `37de8e08`)

Each run scans each side four times (a warm-up and three timed runs), and now keeps every scan's documents. That gives eight scans per side.

| | `main` (8 scans) | branch (8 scans) |
|---|---|---|
| CycloneDX, masked, against the first scan | **5 of 7 differ** | **7 of 7 identical** |
| SPDX 2.3 | **7 of 7 differ** | **7 of 7 identical** |
| SPDX 3 | **7 of 7 differ** | **7 of 7 identical** |
| completeness reason | orphan count 0 or 1, varying | `transitive-edges-unresolvable: generic, pypi` in all 8 (no orphan) |
| I7 unexplained | 0 or 1 | 0 in all 8 |
| C45, C104, C105 across formats | — | agree in all 8 |
| top-level `pkg:generic/gotcha` | present | **absent**; nested under `libgotcha.so.3.0.0`, `libgotcha.so.3` and the rocprofiler-sdk `libgotcha.so` |
| `libgotcha.so` file components | 1 (both paths, hash `10e333b8…`) | **2**: `10e333b8…` (rocprofiler-systems) and `56af308b…` (rocprofiler-sdk) |
| median scan, interleaved | 35.2 s / 46.1 s | 29.0 s / 41.7 s (**0.825×** / **0.905×**) |

**What varies on `main`** (first differences):
- **CycloneDX:** the detached `gotcha`'s properties (cause 1).
- **SPDX 2.3:**
  - the completeness reason (orphan count 0 or 1; cause 2);
  - a same-PURL package's occurrences, for example `rocpd/libpyrocpd…so` (cause 3).

None occurs on the branch.

**SC-002, SC-003 and SC-004 hold**, at 8 scans against SC-002's minimum of 5. **SC-007 holds on the SDK.**

## Quality corpus (44212399837 at the branch, 44212403477 at `main`)

No violations at either ref. **No target moved:** across all 18 targets, every metric is identical (`pkgs`, `edges`, `files`, `flat`, `max_depth`, `nodes_with_out_edges`, `graph_completeness`, `sbomqs`, `status`).

## Public corpus

- **44212396088 (`main`), 44212392500 (branch, `repeat=2`):** both passed, with no golden drift at the branch (FR-007).
- **The branch's repeat pass is not counted as evidence.** The layer printed nothing on success, and the run took no longer than `main`'s single scan (1,029 s against 1,082 s), so a run in which the comparison never executed would look the same. It was rerun after the layer gained a success line (`4b1b65d8`): run 44215765635, pending.

## Source tree (`measure-source` 44212411228)

Pending.
