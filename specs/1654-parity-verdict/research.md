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
- **Implemented (I1):** the rule lives in one function, `parity::verdict::row_verdict`. `parity-check` uses it for both the field and the count. `tests/holistic_parity.rs` keeps its own per-directionality checks for their detailed messages, and asserts per row that its outcome agrees with `row_verdict`. A mutation making `row_verdict` fail every `SymmetricEqual` row failed `holistic_parity` on every fixture.

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
  - **Ordering:** `artifact_presence` is set by `closure_role_scope::assign_role_scope` (`scan_cmd.rs` around line 5602), **after** the function's last call (around 5559). So it is called once more after `withdraw_scope_runtime_evidence_contradicts`.
  - **The type:** `BUILD_DEPENDENCY_OF` is the nearest SPDX type for every artifact-absent member, including milestone 1277's `unreferenced` ones.
  - The same fact makes CycloneDX mark the target `excluded`.
  - It runs after passes 1 and 2, so it never re-types an edge they typed.
- **Prediction (not measured):** (a) and (b) bring the closure goldens' B1/B2 to 0. The T-task measures it.
- **Implemented (T008, T009):**
  - **(a)** `cdx_dependency_edges` classifies by the target only. Unit tests cover a source-excluded edge, which is now runtime, a target-excluded edge, and a plain edge. All 44 test targets that touch B1/B2 or the extractors pass (`holistic_parity`, `spdx_cdx_parity`, `spdx3_cdx_parity`, every `transitive_parity_*`, …).
  - **(b)** Pass 3 is its own function, `scan_fs::type_artifact_absent_edges`, rather than a second call of the whole typing function. Called again in full, passes 1 and 2 would type edges by any lifecycle scope the closure set in between, which CycloneDX excludes only with `--include-dev`.
    - It runs inside `apply_lifecycle_scope_to_edges`, where it is a no-op, and again in `scan_cmd.rs` after the boot-library edges (spec 1653), which also draws edges into closure members.
    - Withdrawal (#1465) applies only to members with no `artifact_presence`, so it and pass 3 never touch the same edge.
    - `is_absent_from_artifact()` is true only for `NotInArtifact`, which covers both build tooling and members the artifact doesn't reference (U2). It is the same predicate that makes CycloneDX emit `scope: excluded`.
  - **Re-measured:** `closure_resolved_tier` and `boot_library_edges` keep only E1 (R6). The new integration test `edges_into_excluded_members_are_build_dependencies_in_spdx23` fails when the late call is removed (a `DEPENDS_ON` into `waybill-fixture-boottool`).

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
- **Traced (T006).**
  - **Where:** `cyclonedx/metadata.rs` builds a promoted main module's `properties` from a hand-picked list: `component-role`, `sbom-tier`, `source-files`, `detected-go`, `workspace-member`, `produces-binaries`, `syft:cpe23` and an operator C212. Each was added by a milestone that noticed it missing. The subject has no `components[]` entry (milestone 084), so nothing else supplies the rest.
  - **The three files' failing scans:** kept via a temporary copy hook, restored with `cp`. In each, the root is a main module (`pkg:hackage/…` or `pkg:pub/…`) in `metadata.component` with **no twin** in `components[]`, carrying 4 properties. Both SPDX formats carry `source-type`, `evidence-kind`, `closure-role` and the scope source on that package.
  - **Goldens:** none of the 28 takes this path. No golden's `metadata.component` carries `waybill:component-role`, and the closures use `--root-name` (an override), which skips the list. **The fix changes no golden.**
- **Implemented (T011):**
  - `builder.rs`'s inline `components[]` property block (about 450 lines) is now a method, `component_properties`. `components[]` calls it, with the `component.cpe` assignment left at the call site. The three byte-identity suites (`cdx_regression`, `spdx_regression`, `spdx3_regression`) are unchanged.
  - For the subject main module, `metadata.component` gains each property that method yields and the list did not already name, appended after the list.
  - `--component-id` identifiers are still counted and attached only for `components[]` (`match_counts: None`).
  - After the fix, `hpack_include_merge` and `project_discovery_haskell` have 0 failing rows, and `closure_resolved_tier` keeps only B1, B2 and E1 (R4, R6).
  - A unit test pins the four properties. Removing the merge fails it.

## R8. The binary tests' B1

**Traced (T005).** Neither guess was right. The edge is not dropped or retyped: SPDX 2.3 has no root to start it from.

- **Measured:** all 11 `version_string_static` cases were kept via a temporary, uncommitted copy hook in the test, restored with `cp`. The 6 failing cases (S3, S4, S5, S7, S8, and the both-tables importer) each have **exactly one component**, the binary. The 3 passing cases (S1, S2, S6) also have `liblzma`, so two components.
- **On S7:**
  - CycloneDX: `metadata.component` is the synthetic `waybill-fixture-root@0.0.0`, which depends on the binary.
  - SPDX 3: the synthetic `pkg-root-…` `dependsOn` the binary.
  - SPDX 2.3: `SPDXRef-DOCUMENT DESCRIBES` the binary's own package. There is no root package and no `DEPENDS_ON`.
  - B1 is therefore 1 / absent / 1.
- **Cause:** `generate/spdx/document.rs`'s root fallback (after `select_root` returns neither an override nor a main module) has a `1 =>` arm: a lone top-level component *becomes* the root. CycloneDX (`cyclonedx/metadata.rs`) and SPDX 3 (`v3_document.rs`) have no such arm. After a main module and a target-name match, both synthesize.
- **Decision:** remove SPDX 2.3's `1 =>` arm, so the lone component goes through the same target-name match, then synthesis, as two or more. This is the SPDX 2.3 side moving to the rule the other two formats already follow.
- **Golden impact, measured:** all 28 corpus goldens already have a synthetic SPDX 2.3 root (`SPDXRef-DocumentRoot-…`), so none changes.
- **Implemented (T012):**
  - The `1 =>` arm is removed. A unit test written first failed on the old code: a lone component not named after the target gets a synthetic root that `DEPENDS_ON` it. A control keeps a lone target-named component as the root.
  - The unit suite and the three byte-identity regressions are unchanged.
  - Re-measured: `binary_kind_technique` (10 scans), `version_string_anchored` (28), `version_string_boundary` (24) and `version_string_static` (9) report **0 failing rows**.

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
