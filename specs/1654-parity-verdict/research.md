# Research: a parity verdict per row, a test helper that can fail, and the closure parity gaps

Code references are at `main` `614e3c2`.

**How it was measured.** A temporary, uncommitted instrument in `cli/parity_cmd.rs` wrote each failing row's three value sets (CycloneDX, SPDX 2.3, SPDX 3) to a file. It was run:
- over the 28 corpus goldens (`measurements/golden-failing-rows-before.md`);
- over the eleven test files' own `parity-check` calls, single-threaded, because parallel appends corrupted 70 of 109 lines on the first attempt.

`parity_cmd.rs` was then restored with `cp`.

## R1. The verdict field (FR-002)

- **Decision:** `CoverageRowReport` gains `verdict: "pass" | "fail" | "not-checked"`.
  - It is computed in the same branch that increments `universal_pass` or `gaps`.
  - `not-checked` covers a row that is not universal, and a universal row that no format carries. Today the latter counts as passing, and it keeps counting that way in the summary.
  - Serialised as a new key, so every existing key, the summary and the table output are unchanged.
- **Rationale:** the only thing missing is exposing the verdict the rule already computes, so the two cannot disagree (SC-001).
- **Alternative:** a boolean `passes`. Rejected: it cannot say "not checked", and that is how the expectation came to be read as the result.

## R2. The shared helper (FR-003, FR-004)

- **Decision:** `tests/common/parity.rs::assert_parity(out_dir)`. It runs `parity-check --format json`, then asserts:
  - no row's verdict is `fail`;
  - `summary.parity_gaps == 0`;
  - the number of `fail` rows equals `summary.parity_gaps`, which guards the field against drift.

  Its failure message names each failing row with its three presences and counts.
- **Adoption:** the eleven files drop their own helper and call it. `tests/boot_library_edges.rs` loses its `PRE_EXISTING_GAPS` allowlist.
- **Proof that each file can fail:** one mutation per file, each removing one format's value of a row that file asserts on, in the emitter, from a `cp` backup.

## R3. What fails today, and why (FR-001)

**Corpus goldens: 80 failing rows, exactly.**

| Row | Goldens | Cause |
|---|---|---|
| G1 | 28 | **Golden masking.** `layer2_golden.rs` masks SPDX 2.3's whole `creationInfo` to `"<masked>"`, so the extractor finds no `Tool:` creator. A live scan has no G1 gap. Not a document defect. |
| A7 | 16 | Declared licence. Several shapes: CycloneDX lists individual IDs where SPDX keeps whole expressions (`GPL-2.0-only AND LGPL-2.1`), `LicenseRef` texts differ, and SPDX 3 lacks some values (lootbox's `MPL-2.0`). Traced per shape in T-tasks before any fix. |
| B1, B2 | 15 each | Two causes (R4, R5). |
| E1 | 5 (closures) | Extractor mismatch (R6). |
| A8 | 1 (postgres image) | Concluded licence; image-only. Out of scope (Clarification 1), to be filed. |

A12 (CPE) and D1 (identity evidence) differ in counts on many goldens but **fail on none**.

**The eleven test files' own scans: 109 failing-row records in 8 of 11 files.** `library_identity`, `merged_binary_evidence` and `sbom_edit_retier` are clean.

| Row | Files | Cause |
|---|---|---|
| B1 | `binary_kind_technique`, `version_string_{anchored,boundary,static}`, closure files | Binary tests: SPDX 2.3 lacks the synthetic root → bundled `.so` (file-tier) edge that CycloneDX and SPDX 3 carry. Closure: R4. |
| B2, E1 | `closure_resolved_tier`, `boot_library_edges` | R4, R6. |
| C1, C4, C182, C203 | `closure_resolved_tier`, `hpack_include_merge`, `project_discovery_haskell` | **The root, when it exists only as CycloneDX `metadata.component`, keeps 4 of its properties** (`component-role`, `sbom-tier`, `source-files`, `workspace-member`). It loses `source-type` (C1), `evidence-kind` (C4), the closure role (C182) and the scope source (C203), which both SPDX formats carry on the root package. When the root is also in `components[]` (f2 without `--root-name`), no gap shows. |

## R4. Closure B1/B2: runtime versus dev edges (FR-005)

**Measured on shellcheck:** CycloneDX has 596 runtime and 12,587 dev edges; SPDX has 13,182 and 1. The union is identical (**0 edges only in one format**). Every difference is an edge CycloneDX calls dev and SPDX calls runtime.

**The two rules differ:**
- CycloneDX has no edge types. Its extractor (`cdx_dependency_edges`) calls an edge dev when **either** endpoint has `scope: excluded`.
- SPDX types the edge from the **target**, through `apply_lifecycle_scope_to_edges`. It has a pass for lifecycle scope and one for `BuildInclusion::NotNeeded`, and **none for milestone 1277's artifact-absent closure members**, which CycloneDX marks `excluded` unconditionally.

**The differing edges, per target:**

| Target | Differing edges | Target excluded | Only the source excluded |
|---|---|---|---|
| moat | 6,699 | 5,442 | 1,257 |
| shellcheck | 12,586 | 10,040 | 2,546 |
| slack-web | 8,175 | 5,578 | 2,597 |
| openai | 8,379 | 5,826 | 2,553 |
| slacklinker | 12,417 | 9,225 | 3,192 |

**Decision, in two parts:**
- **(a) The extractor:** CycloneDX's extractor classifies by the **target** only, matching how SPDX types an edge and what the type means.
  - Measured as a temporary mutation over all 28 goldens: **no new failing target**, and haskell-security-advisories' B1 and B2 go from failing to passing.
  - Milestone 085's comment explains the target side (Maven puts scope on the target). The source side was milestone 052's original rule, kept by 085 out of caution, and no golden needs it.
- **(b) The emitter:** `apply_lifecycle_scope_to_edges` gains a third pass. A `DependsOn` into a component with `artifact_presence.is_absent_from_artifact()` becomes `BuildDependsOn` (SPDX `BUILD_DEPENDENCY_OF`).
  - The same fact makes CycloneDX mark the target `excluded`.
  - It runs after passes 1 and 2, so it never re-types an edge they typed.
- **Prediction (not measured):** (a) and (b) bring the closure goldens' B1/B2 to 0. The T-task measures it.

**Alternative:** SPDX typing by either endpoint. Rejected: an SPDX dependency type describes the target, so typing an edge into a runtime library as a build dependency, because its consumer is build-only, would be false.

## R5. Non-closure B1/B2: the dev-inclusion gate (out of scope)

**What differs:** on haskell-aeson, the 7 differing edges are test and bench dependencies (`tasty-bench`, `QuickCheck`). SPDX types them dev. CycloneDX gives the target no `scope` at all, because `builder.rs` emits `scope: excluded` for a lifecycle scope only when `include_dev` is set.

**Where it applies:** this cause accounts for the remaining non-closure B1/B2 failures (Go, Haskell, Maven and Python goldens).

**Scope:** none of the eleven test files' fixtures exercises it, so under Clarification 1 it is measured and filed, not fixed. The fix would change every such golden's CycloneDX.

## R6. E1: the completeness claim (FR-005)

**Measured on shellcheck:**
- CycloneDX: absent.
- Both SPDX formats: `{"complete_ecosystems":[],"target_aggregate":"incomplete_first_party_only"}`.

**Why they differ:**
- `e1_cdx` reports presence when **any** `compositions[]` record is `complete`. That includes the root's own dependency-list claim, which SPDX does not encode.
- `e1_spdx23` and `e1_spdx3` report presence whenever the annotation exists, **even when it claims no ecosystem complete**.

The two documents agree; the extractors don't.

**Decision:** E1 means "an ecosystem is claimed complete":
- **CycloneDX:** present iff some record has `aggregate: complete` and non-empty `assemblies`. That is an ecosystem record, not the root's dependency-only claim.
- **SPDX:** present iff `complete_ecosystems` is non-empty.

Measured in the T-task across all goldens, before and after.

## R7. The root in `metadata.component` (C1, C4, C182, C203)

- **Decision:** the CycloneDX `metadata.component` carries the same `waybill:*` properties the root would carry in `components[]`. This is the emitter fix, because CycloneDX is the only format that loses them.
- **The T-task first:**
  - finds where `metadata.component`'s properties are built and why only 4 survive;
  - measures which goldens change. Every golden whose root is only in `metadata.component`, including the closures under `--root-name`, gains properties.

## R8. The binary tests' B1

The binary tests' synthetic root → bundled `.so` edge is missing from SPDX 2.3 only. The T-task traces it in the SPDX 2.3 relationship emitter before choosing a fix:
- it may be emitted as another relationship type;
- or dropped because the target is file-tier.

## R9. Goldens and G1 (FR-006)

**No test parity-checks goldens.** G1 needs no product change. `measurements/golden_parity.sh` will:
- reconstruct the masked creator before checking;
- or report G1 separately as a masking artefact.

Its after-table records which.

**Changed goldens:**
- R4(b) changes the five closure goldens' SPDX documents;
- R7 changes CycloneDX documents whose root is only in `metadata.component`;
- R6 changes nothing in documents.

All are regenerated by CI dispatch and traced.

## R10. Out of scope, to be filed (Clarification 1)

- The R5 dev-inclusion gate (9 goldens' B1/B2).
- A8 on image-postgres16.
- Any A7 shape found only in non-closure goldens after the closure A7 trace.

Each will be filed with its per-row measurement.
