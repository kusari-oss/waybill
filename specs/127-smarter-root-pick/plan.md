# Implementation Plan: Smarter root component selection for polyglot + multi-module Go workspace scans

**Branch**: `127-smarter-root-pick` | **Date**: 2026-06-17 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/127-smarter-root-pick/spec.md`

## Summary

Source-tier scan paths today silently misroute the SBOM subject when multiple main-module-tagged components exist. Two reproducible bug classes drive this feature: polyglot repos (argo-workflows: 4 main-modules from Go + Maven + 2 npm; Maven wins) and multi-module Go workspaces (otel-collector: 55 nested `go.mod` files; alphabetic leaf wins). The fix is a deterministic root-selection ladder layered on top of the existing milestone-053 + milestone-064/066/068/069/070 main-module-tagging infrastructure: when the existing count==1 fast path fails, prefer the main-module whose manifest file sits at the scan's `--path` root, breaking residual ties by a fixed ecosystem-priority order, then by longest-common-prefix of manifest paths. Every fall-through past a detected main-module emits a `tracing::warn!` log listing the loser PURLs and recommending operator override. The new heuristic surfaces via a new `mikebom:root-selection-heuristic` document-scope annotation that carries both the heuristic name AND a numeric confidence value modeled on the existing CDX `evidence.identity.confidence` channel. The two reproducible bugs become SC-001 and SC-002; the 33 existing alpha.48 goldens MUST remain byte-identical (SC-003).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–126; no nightly required for this user-space-only selection logic).

**Primary Dependencies**: Existing only — `std::path::{Path, PathBuf}` + `std::fs::canonicalize` (already pervasive in `scan_fs/`), `tracing` (warn/info logs), `anyhow` (error propagation), `serde`/`serde_json` (annotation construction). **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM via the existing document-scope annotation channel. Mirrors every milestone since 002.

**Testing**: `cargo +stable test --workspace` integration tests using `tempfile::tempdir()`-isolated fixture trees + the existing milestone-090 `MIKEBOM_FIXTURES_DIR` cache. Two new vendored OSS fixtures (kubelb-shape Go single-module sanity, argo-workflows-shape polyglot multi-main-module, otel-collector-shape Go multi-module workspace) under `tests/fixtures/transitive_parity/` or a sibling directory. End-to-end smoke via `mikebom-cli/tests/identifiers_root_purl_control.rs`-style harness.

**Target Platform**: Linux + macOS + Windows (existing tri-platform CI lanes; this feature touches no OS-specific code paths).

**Project Type**: Existing three-crate workspace (`mikebom-cli`, `mikebom-common`, `mikebom-ebpf`). No new crates. All changes in `mikebom-cli` source-tier code (`scan_fs/package_db/` + `generate/cyclonedx/metadata.rs` + `generate/spdx/document.rs` + `generate/spdx/v3_document.rs`).

**Performance Goals**: The new canonicalization + longest-common-prefix pass runs once per scan at the metadata.component selection step, over a Vec of main-module-tagged components — at most ~100 entries on the largest known repo (otel-collector). Path canonicalization is one stat per main-module; LCP is O(n·m) where n=main-module count and m=longest path. Target: ≤1 ms overhead on a 55-main-module scan (otel-collector). Verified by re-running the existing milestone-094 perf benchmark.

**Constraints**: 33 byte-identity goldens (11 CDX + 11 SPDX 2.3 + 11 SPDX 3) under `mikebom-cli/tests/fixtures/golden/` MUST stay byte-identical (SC-003). Every fixture is a single-main-module project; the count==1 fast path is preserved exactly. The new heuristic annotation is emitted ONLY when a tiebreaker fires, so single-main-module fixtures see no diff. The pkg_alias_binding `image-baz.cdx.json` golden also stays unchanged (image-tier scope-out per Q4).

**Scale/Scope**: Per FR-001–FR-012. Adds one `bool` field to the main-module annotation contract; one new annotation key (`mikebom:root-selection-heuristic`); one new C-row entry in the milestone-005 catalog. Changes ~3 emission code paths (CDX `metadata.rs`, SPDX 2.3 `document.rs`, SPDX 3 `v3_document.rs`) — same single-source-of-truth pattern milestones 077/078/079 used. No new CLI flag, no new env var, no new binding semantics surface.

## Constitution Check

Evaluating against mikebom Constitution v1.4.0 (`.specify/memory/constitution.md`).

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | User-space Rust only; no C, no eBPF changes. |
| II. eBPF-Only Observation | ✓ N/A | This feature operates on already-discovered main-module-tagged components emitted by source-tier readers — it does not introduce new discovery. No eBPF interaction. |
| III. Fail Closed | ✓ | New code paths preserve fail-closed posture: when no `is_workspace_root` main-module is detected AND LCP yields no winner, the existing fallback ladder fires (Maven `scan_target_coord` → `pkg:generic/<target>@0.0.0`) AND FR-007 emits a warning. The scan itself never silently swallows an error; the heuristic's fall-through is a transparent annotation choice, not silent failure. |
| IV. Type-Driven Correctness | ✓ | The Root-selection heuristic entity is a typed `enum RootSelectionHeuristic { RepoRoot, EcosystemPriority, LongestCommonPrefix, MavenScanTargetCoord, SyntheticPlaceholder }` (the two implicit `single-main-module` and `operator-override` cases never produce the annotation, so they're handled out-of-band). Confidence is a `f64` constrained at compile time to a fixed table per variant — no string-typed lookup at runtime. The new `is_workspace_root: bool` field on `ResolvedComponent.extra_annotations` continues to use the existing `BTreeMap<String, serde_json::Value>` channel (Principle V parity-bridging — see V below). |
| V. Specification Compliance | ✓ + audit | **Native-field audit performed.** New `mikebom:root-selection-heuristic` annotation. CDX 1.6: `metadata.component` carries the BOM-subject identity; CDX has no native field naming the *heuristic* used to elect the subject (`evidence.identity.confidence` is per-component, not per-document — and only carries a confidence float, no heuristic name). SPDX 2.3: `documentDescribes` selects packages by ID, no native "selection method" field. SPDX 3.0.1: same, `rootElement` is a ref. Verdict: **no native construct exists in any of the three formats** for surfacing the document-scope heuristic-used signal. Per Principle V's parity-bridging clause, the `mikebom:root-selection-heuristic` annotation is justified; will be documented in `docs/reference/sbom-format-mapping.md` with the parity-gap justification (CDX has `evidence.identity.confidence` at component-scope only; SPDX has no comparable channel at all). The `is_workspace_root` bool is internal-only — never surfaced in emitted SBOMs — so no Principle V audit is needed for it. |
| VI. Three-Crate Architecture | ✓ | No new crates. All changes in `mikebom-cli`. |
| VII. Test Isolation | ✓ | Tests use `tempfile::tempdir()`-isolated fixture trees plus the milestone-090 `MIKEBOM_FIXTURES_DIR` cache for the two new vendored OSS shapes. No eBPF privilege requirement. |
| VIII. Completeness | ✓ | This feature only changes which already-discovered component is named as the SBOM subject — it does NOT change which components are discovered, emitted, or counted. The 129-component count on kubelb and the 378-component count on otel-collector remain identical. |
| IX. Accuracy | ✓ | This feature *improves* accuracy: today's root selection is wrong for both reproducible bug classes (#366 + #367). The new heuristic delivers the correct identity for the SBOM subject. The confidence value on the annotation lets downstream consumers gate on auto-pick quality (Principle X). |
| X. Transparency | ✓ | FR-006 emits a transparency annotation naming the heuristic + confidence for every non-fast-path selection. FR-007 emits a warning for every fall-through past a detected main-module. Both align with Principle X's "structured metadata in SBOM output" + "spec-native mechanisms" clauses (CDX `properties[]`, SPDX `annotations[]`). |
| XI. Enrichment | ✓ N/A | No enrichment-source interaction. |
| XII. External Data Source Enrichment | ✓ N/A | No external data source involvement. |

**Verdict**: No violations. No complexity tracking entries.

## Project Structure

### Documentation (this feature)

```text
specs/127-smarter-root-pick/
├── plan.md              # This file
├── spec.md              # Feature spec (with Clarifications)
├── research.md          # Phase 0: native-field audit + ecosystem-priority research + perf budget research
├── data-model.md        # Phase 1: ResolvedComponent extra_annotations contract + Root-selection heuristic enum
├── quickstart.md        # Phase 1: end-to-end repro recipes for SC-001 (otel) + SC-002 (argo) + SC-003 (zero-regression)
├── contracts/           # Phase 1: CLI behavior contract + annotation JSON schema contract
└── checklists/
    └── requirements.md  # /speckit-specify-time quality gate (all 16 items pass)
```

### Source Code (repository root)

```text
mikebom-cli/src/
├── scan_fs/
│   └── package_db/
│       ├── golang/legacy.rs                  # FR-001 — set is_workspace_root on Go main-module
│       ├── cargo.rs                          # FR-001 — same for Cargo main-module
│       ├── npm/                              # FR-001 — same for npm main-module
│       ├── pip/                              # FR-001 — same for pip main-module
│       ├── gem.rs                            # FR-001 — same for gem main-module
│       ├── maven.rs                          # FR-001 — same for maven main-module + FR-012 dedup
│       └── mod.rs                            # FR-010 canonicalize + dedupe across readers; FR-012 scan_target_coord suppression
├── generate/
│   ├── root_selector.rs                      # NEW — implements the heuristic ladder; the single source of truth
│   ├── cyclonedx/
│   │   └── metadata.rs                       # FR-002–FR-005 wire-up — replace the existing 269-309 priority ladder with calls to root_selector
│   └── spdx/
│       ├── document.rs                       # FR-005 — same selector for SPDX 2.3 documentDescribes
│       └── v3_document.rs                    # FR-005 — same selector for SPDX 3 rootElement
└── cli/
    └── scan_cmd.rs                           # FR-011 — pipe the selected root into the --bind-to-source envelope subject

mikebom-cli/tests/
├── root_selection_us1_multi_module_workspace.rs    # NEW — SC-001 (otel-collector)
├── root_selection_us2_polyglot.rs                  # NEW — SC-002 (argo-workflows)
├── root_selection_us3_heuristic_annotation.rs      # NEW — SC-004 + SC-005 cross-format consistency
├── root_selection_byte_identity.rs                 # NEW — SC-003 zero-regression on all 33 alpha.48 goldens
└── fixtures/
    └── root_selection/
        ├── multi_module_go_workspace/              # NEW — synthesized otel-shape fixture (3 nested go.mod, one at root)
        ├── polyglot_go_maven_npm/                  # NEW — synthesized argo-shape fixture (1 go.mod at root, 1 pom.xml subdir, 1 package.json subdir)
        ├── go_subdir_no_root_module/               # NEW — edge case: all go.mod files in subdirs (LCP tiebreaker)
        └── cargo_workspace/                        # NEW — cargo workspace fast-path uniformity check
```

**Structure Decision**: Existing three-crate workspace (`mikebom-cli` + `mikebom-common` + `mikebom-ebpf`); zero new crates per Constitution Principle VI. The new `generate/root_selector.rs` module is the single source of truth for the heuristic ladder — all three format emitters call into it identically, satisfying FR-005's cross-format consistency requirement. Per-ecosystem reader files gain a single line each that sets `is_workspace_root: bool` on the main-module annotation based on whether the manifest file's parent canonicalizes to `--path`. The four new fixture trees live in-tree (~5 KB each — small enough not to need the milestone-090 fixture repo) so the integration tests are hermetic and CI-friendly.

## Complexity Tracking

> No Constitution violations. No entries.
