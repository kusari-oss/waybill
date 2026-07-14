# Implementation Plan: Fix Graph-Completeness Over-Firing on Operator-Supplied Roots

**Branch**: `192-operator-root-completeness` | **Date**: 2026-07-14 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/192-operator-root-completeness/spec.md`

## Summary

Single-file fix inside the graph-completeness pass. When the operator supplies `--root-name X` (and optionally `--root-purl-type <eco>`), the emitted root PURL is `pkg:generic/X` (or `pkg:<eco>/X`) and its `ResolvedRootSubject` variant is NOT `MainModule`. Today `build_ecosystem_root_set` at `mikebom-cli/src/generate/graph_completeness/bfs.rs:73` builds an empty `per_ecosystem_root` for those scans, populates `ecosystems_without_root` with every ecosystem present in `components[]`, and the downstream `MultiEcosystemPartialRoot` classifier fires on any single orphan in any of those ecosystems — flipping `mikebom:graph-completeness` from `complete` to `partial`.

Post-fix: when the root's PURL is derivable AND the selection subject is non-`MainModule`, `build_ecosystem_root_set` also seeds the `per_ecosystem_root` map with `(ecosystem, target_ref)` for EVERY ecosystem present in `components[]`, EXCEPT the ecosystem that already matches the root PURL's own ecosystem segment (per Q2 answer A). Result: `ecosystems_without_root` is empty for operator-override scans, the `MultiEcosystemPartialRoot` classifier no longer fires as a false positive, and the emitted `mikebom:graph-completeness` value reflects real reachability instead of the operator-override artifact. Native-root path (`MainModule`) is a byte-identity no-op per FR-004.

One INFO-level tracing log line per scan reports the synthesis count per Q1 answer A.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–191; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `std::collections::{HashMap, HashSet}` (already pervasive in the graph-completeness module), `tracing::info!` (the INFO-level log line per FR-009), `mikebom_common::resolution::ResolvedComponent` (existing type), `mikebom_common::types::purl::Purl` (existing type; the fix reads `.ecosystem()` from a parsed PURL string built from `target_ref`). **Zero new Cargo dependencies.** No subprocess calls. No network access.

**Storage**: N/A — the fix is a pure in-memory transformation over the existing `EcosystemRootSet` struct built once per scan. No caches, no persistence.

**Testing**: `cargo +stable test --workspace` (workspace-wide unit + integration). New assertions:

- Unit tests co-located with `build_ecosystem_root_set` in `bfs.rs::tests` covering: operator-override with single-ecosystem components → synthesis fires, ecosystems_without_root is empty; operator-override with multi-ecosystem components → synthesis fires for every ecosystem present; operator-override with `--root-purl-type golang` on a Go+npm mixed component list → synthesis fires for npm ONLY (golang already matched by root PURL); native-root path (MainModule) → synthesis does NOT fire (byte-identity guard); empty components → synthesis is a no-op.
- Integration test scanning a Go source repo with `--root-name X --root-version Y`; assert `mikebom:graph-completeness == "complete"`.
- Integration test scanning a mixed Go+npm source repo with `--root-name X --root-version Y --root-purl-type golang`; assert `complete` and that the npm placeholder synthesis worked without duplicating a golang root.
- Regression: every existing golden byte-identical (fix is scoped to operator-override path; native-root goldens untouched).

**Target Platform**: Linux + macOS host builds; behavior is host-agnostic (pure in-memory transformation over emitted JSON).

**Project Type**: CLI (existing `mikebom sbom scan` subcommand path).

**Performance Goals**: The fix adds one HashMap iteration over `components[]` inside the existing `build_ecosystem_root_set` pass — same O(N) as the existing per-ecosystem-root loop. No perf regression concern.

**Constraints**: Byte-identity on the native-root path (SC-004) is a HARD gate. No new `mikebom:*` annotations (FR-008 / Principle V). No new Cargo dependencies.

**Scale/Scope**: ~30-50 LOC delta in `mikebom-cli/src/generate/graph_completeness/bfs.rs` + ~5-8 new unit tests + 2 new integration tests. Estimated 12-15 tasks.

## Constitution Check

Post-Phase-0 recheck below. Initial pass:

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | PASS | No new deps; no C code introduced. |
| II. eBPF-Only Observation | N/A | User-space classifier fix — orthogonal to eBPF trace. |
| III. Fail Closed | PASS | Fix only affects classifier VALUE, not component emission; failure modes (empty components, missing target_ref) fall through to existing safe defaults. |
| IV. Type-Driven Correctness | PASS | Fix reads existing `ResolvedRootSubject` enum + `Purl` newtype; no new types; no `.unwrap()` in production code (test-mod guarded per existing convention). |
| V. Specification Compliance | PASS + explicitly enforced by FR-008. The `mikebom:graph-completeness` + `mikebom:graph-completeness-reason` annotations are pre-existing (m158/m167/m177); this milestone changes only the VALUES emitted into those channels, not the annotation set. Zero new `mikebom:*` annotations. Audit result recorded: no new fields introduced. |
| VI. Three-Crate Architecture | PASS | Changes limited to `mikebom-cli/src/generate/graph_completeness/bfs.rs` + shared type reuse from `mikebom-common`. No new crate. |
| VII. Test Isolation | PASS | All new tests are pure-Rust unit + integration; no eBPF privilege required. |
| VIII. Completeness | PASS | No components dropped or added. The fix corrects a false-negative reachability signal — moves closer to Principle VIII's spirit ("consumers cannot act on data they cannot assess"). |
| IX. Accuracy | PASS | Fix REMOVES a false positive; the classifier will report `partial` ONLY when real orphans exist (FR-007). |
| X. Transparency | PASS | INFO-level log per FR-009 gives operators explicit visibility into when the synthesis path fired. Existing `mikebom:graph-completeness-reason` annotation continues to surface real gaps unchanged. |
| XI. Enrichment | N/A | No external data source touched. |
| XII. External Data Source Enrichment | N/A | No external data source touched. |
| Strict Boundary 1 (No lockfile discovery) | N/A | Not a discovery change. |
| Strict Boundary 2 (No MITM proxy) | N/A | Not a network change. |
| Strict Boundary 3 (No C code) | PASS | No C. |
| Strict Boundary 4 (No .unwrap() in production) | PASS | New logic uses `Option::map` / `if let`; test-mod guarded per convention. |
| Strict Boundary 5 (No file-tier duplicates in default mode) | N/A | Not a file-tier-emission change. |

**No violations. Proceed to Phase 0.**

## Project Structure

### Documentation (this feature)

```text
specs/192-operator-root-completeness/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output — classifier-input contract
├── checklists/
│   └── requirements.md  # Created by /speckit-specify
└── tasks.md             # Created by /speckit-tasks (NOT this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── generate/
│       └── graph_completeness/
│           ├── bfs.rs                    # PRIMARY: extend build_ecosystem_root_set
│                                          # signature to accept `target_ref: &str`;
│                                          # add operator-override synthesis pass.
│           └── mod.rs                    # WIRE: pass target_ref into
│                                          # build_ecosystem_root_set (existing call
│                                          # at line ~156). The generic-carve-out at
│                                          # line 171 stays as-is.
└── tests/
    ├── graph_completeness_operator_root.rs   # NEW: integration tests for US1
                                              # acceptance scenarios 1-5.
    └── existing goldens/                     # UNCHANGED: byte-identity gate (SC-004).
```

**Structure Decision**: Purely a classifier-input fix. All logic lives in the existing `build_ecosystem_root_set` function at `mikebom-cli/src/generate/graph_completeness/bfs.rs:73`. Signature gets one new parameter (`target_ref: &str`); the caller at `mod.rs:156` gets updated to pass it through. No new modules; the new integration-test file is the only file addition.

## Complexity Tracking

*No constitution violations — table intentionally empty.*
