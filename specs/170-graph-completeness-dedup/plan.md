# Implementation Plan: Dedup document-scope `mikebom:graph-completeness` annotation

**Branch**: `170-graph-completeness-dedup` | **Date**: 2026-07-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/170-graph-completeness-dedup/spec.md`

## Summary

**Primary requirement**: eliminate the duplicate emission of the document-scope `mikebom:graph-completeness` annotation across CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 outputs (US1 P1) while preserving the Go-transitive-coverage signal via the already-existing C110 annotation (US2 P1) and adding a CI-gating unit test that prevents future two-catalog-rows-same-label regressions (US3 P2).

**Technical approach**: Three-fold surgical change per the m170 spec's Clarifications Q1 → Option A:

1. **Delete the milestone-061 emission site at `mikebom-cli/src/generate/cyclonedx/metadata.rs:228-245`** — the `if let Some(gc) = go_graph_completeness { ... }` block that pushes the C44 annotation. Rip out the `go_graph_completeness` + `go_graph_completeness_reason` parameters on `build_metadata` and their upstream plumbing (call sites in `scan_cmd.rs` + `generate/mod.rs` + the `TraceEmission` struct that feeds it). No renaming, no migration into C110 — pure removal per Q1 decision. Follow-up issue [#516](https://github.com/kusari-oss/mikebom/issues/516) tracks the "should we re-home the information?" investigation as post-hoc analysis.
2. **Delete the C44 row from `mikebom-cli/src/parity/extractors/mod.rs`** + drop its `c44_cdx`/`c44_spdx23`/`c44_spdx3` extractor helpers from `cdx.rs` / `spdx2.rs` / `spdx3.rs`. Update `docs/reference/sbom-format-mapping.md` C4-row per FR-006 → use the milestone-052's C6-strikethrough precedent (`~~C44~~ ~~mikebom:graph-completeness (Go-scoped)~~ **REMOVED in milestone 170**` with a one-line justification pointing at C104 as the canonical universal home and C110 as the Go-transitive-coverage carrier).
3. **Add a duplicate-label integrity gate** — new unit test in `mikebom-cli/src/parity/extractors/mod.rs::tests` asserting that every `label` in the EXTRACTORS table is unique. Test walks the const slice, builds a `HashMap<&str, Vec<&str>>` from label → row_ids, panics if any entry has `.len() > 1` with a message naming the collision.

**Golden regeneration**: run `MIKEBOM_UPDATE_GOLDENS=1 cargo test` to refresh `tests/fixtures/golden/cyclonedx/golang.cdx.json` + the m090 sibling-repo's Go-ecosystem CDX/SPDX 2.3/SPDX 3.0.1 goldens. Verify SC-005: `git diff main -- 'mikebom-cli/tests/fixtures/golden/**'` shows ONLY the C44 removal in the golang golden — no other byte-changes. Sibling-repo diff verified via `MIKEBOM_FIXTURES_UPDATE=1` (m090 process).

**Blast radius**: ~40 lines removed from `metadata.rs` (the m061 block plus its two parameters and their upstream flow); ~5 lines added to `mod.rs` for the new test; ~10 lines removed across the 4 extractor files (`cdx.rs`, `mod.rs`, `spdx2.rs`, `spdx3.rs`); 1 row struck through in `sbom-format-mapping.md`; ~10 lines net removed from the golang golden (the duplicate emission + its trailing empty context). Total ~65 lines removed, ~10 lines added.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–169; no nightly required for this user-space-only fix).

**Primary Dependencies**: Existing only — `serde`/`serde_json` (JSON round-trip), `tracing`, `anyhow`, `thiserror`. Reuses milestone-158's `GraphCompletenessResult` struct + `join_reason_codes` helper. Reuses milestone-071's parity-extractor infrastructure verbatim. **No new Cargo dependencies.**

**Storage**: N/A — pure metadata-emission transform on the CDX/SPDX code paths; no caches, no persistence.

**Testing**: `cargo test` — 2 new unit tests (US1's post-removal single-emission assertion via existing byte-golden diff + US3's new duplicate-label integrity gate). Sibling-repo goldens (Go ecosystem) update in lockstep. Pre-PR gate covers m071 catalog integrity + m078 SPDX 3 conformance validator (`spdx3-validate==0.0.5`).

**Target Platform**: All hosts mikebom builds on — Linux (CI), macOS (dev), Windows (m100-experimental). No host-specific code paths touched.

**Project Type**: cli (mikebom sbom-generation CLI + parity-extractor test infrastructure).

**Performance Goals**: N/A — this is a net reduction in emitted bytes (one fewer property per Go SBOM). No perf targets to hit.

**Constraints**: SC-005 byte-identity gate — golden diff MUST show only the C44 removal, no unrelated deltas. SC-006 pre-PR gate must stay green including the m071 parity catalog test that surfaced C116 during milestone 169.

**Scale/Scope**: Small — 3 code files (`metadata.rs`, `mod.rs`, and the three extractor files) + 1 docs file + goldens. No new source files.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No new C-transitive deps. Pure Rust refactor within existing crates.
- **II. eBPF-Only Observation**: ✅ `mikebom-ebpf` untouched. This is a metadata-emission fix in `mikebom-cli`.
- **III. Fail Closed**: ✅ Refactor is emission-code cleanup, no new external-input paths.
- **IV. Type-Driven Correctness**: ✅ Removing dead-code parameters tightens the type signature (`build_metadata` becomes clearer without the two obsolete `Option`-typed parameters).
- **V. Specification Compliance**: ✅ The fix makes emission MORE spec-compliant. CDX 1.6 permits duplicate `properties[]` entries with the same name but the semantic is undefined; consumers expect uniqueness in practice. Reducing to a single entry aligns with the SPDX-side annotation model (SPDX 2.3 + 3.0.1 formally allow duplicates but their consumer ecosystem treats them as data-quality bugs). The reading guide's §3.3 documentation of `mikebom:graph-completeness` as a singular signal becomes true post-fix. **Standards-native-precedence audit**: this fix does NOT introduce any new `mikebom:*` annotations; it retires one. The standards-native carrier for graph-completeness (CDX has no native equivalent; SPDX 2.3 has no native equivalent; SPDX 3.0.1 has no native equivalent — the milestone-158 catalog row C104 audit already documented this) remains the same. FR-002 preserves the annotation for the universal-semantic emission.
- **VI. Three-Crate Architecture**: ✅ Change is contained to `mikebom-cli`. `mikebom-common` untouched. `mikebom-ebpf` untouched.
- **VII. Test Isolation**: ✅ New unit test uses only the const `EXTRACTORS` slice (no I/O, no shared state, no `env` mutation). Trivially parallelizable.
- **VIII. Completeness**: ✅ No completeness signal removed. Universal graph-completeness (m158) preserved; Go-transitive-coverage (m160) preserved. Only the redundant DUPLICATE emission goes away.
- **IX. Accuracy**: ✅ Post-fix accuracy is strictly better. Pre-fix consumer got two values with no ordering guarantee — undefined output. Post-fix consumer gets exactly one value with defined semantic per FR-002.
- **X. Transparency**: ⚠️ This is the ONE principle worth scrutinizing. Q1's Option A choice accepts information loss (Site 1's `partial` value doesn't survive as-is; C110's `unknown` verdict replaces it). Mitigated by (a) issue [#516](https://github.com/kusari-oss/mikebom/issues/516) tracking the reconstructability question; (b) the fact that C110's more nuanced reason-code vocabulary (`offline-mode` / `proxy-fetch-degraded` / etc.) is arguably MORE transparent than C44's coarse three-way enum. If issue #516 concludes the information IS lost and reconstructable is worth it, a follow-up milestone re-homes it — but that's not m170's scope. **Watch item for the m170 reviewer**: if issue #516's investigation concludes the information is truly lost with no reconstruction recipe from remaining signals (universal `mikebom:graph-completeness` + per-component `mikebom:source-type` + PURL scheme), a fast-follow milestone (`171-c44-info-rehome` or similar) SHOULD be filed within one release cadence so consumers relying on the pre-m170 Go-scoped semantic don't remain broken indefinitely. This is a `SHOULD` not a `MUST` because m170's post-fix state is strictly better than pre-fix (the pre-fix ambiguous-duplicate emission was itself a Principle X violation), so even the worst-case Transparency-loss scenario is a net improvement.
- **XI. Enrichment**: N/A — no enrichment path touched.
- **XII. External Data Source Enrichment**: N/A — no external data source.

**Strict Boundaries check**:
- No new subprocess calls, no new network access, no new filesystem writes outside goldens.
- No new `mikebom:*` annotations added.
- File-tier emission untouched (no §5 override marker changes).

**Verdict**: All principles pass. Principle X's ⚠️ is acknowledged and tracked as issue #516; the m170 spec's Clarifications Q1 explicitly captured the user's endorsement of Option A even after being surfaced this concern.

## Project Structure

### Documentation (this feature)

```text
specs/170-graph-completeness-dedup/
├── plan.md              # This file (/speckit.plan output)
├── research.md          # Phase 0 output — locations catalog + reconstruction-recipe sketch
├── data-model.md        # Phase 1 output — the three affected entities + their before/after shapes
├── quickstart.md        # Phase 1 output — manual verification steps
├── contracts/           # Phase 1 output — thin, this feature has no new external contracts
├── checklists/          # Requirements checklist (spec-phase output)
└── tasks.md             # Phase 2 output (/speckit.tasks command — NOT created by /speckit.plan)
```

### Source Code (repository root)

Files touched by this feature (existing files only — zero new files):

```text
mikebom-cli/
├── src/
│   ├── generate/
│   │   ├── mod.rs                          # Remove go_graph_completeness fields from TraceEmission / SbomEmission
│   │   └── cyclonedx/
│   │       └── metadata.rs                 # Delete m061 emission site (lines 228-245) + drop 2 params from build_metadata
│   ├── cli/
│   │   └── scan_cmd.rs                     # Drop upstream plumbing that fed go_graph_completeness (line 2612 comment + surrounding lines)
│   └── parity/
│       └── extractors/
│           ├── mod.rs                      # Delete C44 ParityExtractor row + drop c44_* imports + ADD new duplicate-label unit test
│           ├── cdx.rs                      # Delete c44_cdx helper
│           ├── spdx2.rs                    # Delete c44_spdx23 helper
│           └── spdx3.rs                    # Delete c44_spdx3 helper
├── tests/
│   └── fixtures/
│       └── golden/
│           └── cyclonedx/
│               └── golang.cdx.json         # Golden regen: remove duplicate mikebom:graph-completeness entry
docs/
└── reference/
    └── sbom-format-mapping.md              # Strikethrough C44 row + note pointing at C104 + C110

# Sibling repo (mikebom-test-fixtures, m090):
tests/fixtures/spdx/golang/*.spdx.json      # Golden regen: remove duplicate annotation envelope
tests/fixtures/spdx3/golang/*.spdx3.json    # Golden regen: remove duplicate typed Annotation
```

**Structure Decision**: Standard three-crate workspace layout preserved. Change is entirely within `mikebom-cli` (per Constitution VI). Sibling `mikebom-test-fixtures` repo needs a companion PR for the Go-ecosystem SPDX goldens; that PR is opened together with the mikebom PR per the m090 process.

## Complexity Tracking

No Constitution violations; no complexity to track. This is a pure-deletion refactor plus one small new integrity test.

## Phase 0 — Outline & Research

Research questions this feature raises:

1. **Where exactly does `go_graph_completeness` flow through the emission pipeline?** — trace the upstream call chain to identify every touchpoint that needs cleanup.
2. **Do any other catalog rows in EXTRACTORS share a `label`?** — critical for FR-004's absolute-vs-allowlist choice. Spec's Assumptions section anticipates this needs planning-phase confirmation.
3. **What's the current shape of the affected goldens?** — need to identify the exact line ranges being removed to keep SC-005's diff-restriction assertion precise.
4. **What does the SPDX 3 output currently look like for `mikebom:graph-completeness`?** — the SPDX 3 Annotation element form is distinct from CDX properties; need to verify the dedup is achievable at the same site (yes — the CDX metadata builder emits the property; the SPDX 3 emission path converts via the same milestone-158 pathway, so a single upstream fix cascades correctly).
5. **What's the m090 sibling-repo golden regen workflow specifically?** — reference milestone 090's process doc.

`research.md` will consolidate.

## Phase 1 — Design & Contracts

Design outputs for this feature:

- **data-model.md** — three affected entities:
  1. `build_metadata()` function signature (before: 20 params including 2 obsolete `Option` types; after: 18 params).
  2. `EXTRACTORS` slice (before: 116 rows with C44 + C104 both labeled `"mikebom:graph-completeness"`; after: 115 rows with C104 unique).
  3. Emitted CDX 1.6 metadata `properties[]` shape (before: 2 duplicate `mikebom:graph-completeness` entries in Go scans; after: 1 entry unconditionally).

- **contracts/** — thin folder. This feature adds no new external CLI-flag / API contracts, so `contracts/README.md` documents:
  - The `properties[]` uniqueness invariant for `mikebom:graph-completeness` post-m170 (informal — no schema-level constraint enforceable, but documented for consumers reading via the reading guide).
  - The `EXTRACTORS.label` uniqueness invariant (structural — enforced by new unit test).

- **quickstart.md** — three-step manual verification recipe: (1) run pre-m170 mikebom on a Go project, count the duplicate; (2) apply the m170 branch, re-run, verify single emission; (3) run the parity-extractor unit test on a synthesized duplicate-label scenario, verify failure.

- Agent context update via `.specify/scripts/bash/update-agent-context.sh claude` — appends the m170 no-new-dependencies note to `CLAUDE.md`'s tech-list.

Post-design Constitution re-check: no drift from Phase 0 verdict. All principles remain green.
