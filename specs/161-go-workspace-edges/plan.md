# Implementation Plan: Go workspace-mode false dep-graph edges (fix + regression guard)

**Branch**: `161-go-workspace-edges` | **Date**: 2026-07-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/161-go-workspace-edges/spec.md`

## Summary

Milestone 155–160 audit expansion surfaced that on Go repos using `go.work` workspace mode, mikebom emits **wrong `dependsOn` edges** — 30.8% of edges on `test-kubernetes` (26.6% DIVERGE + 4.2% EMITTED-SUPERSET) don't match `go mod graph` executed in each `use`d module's own directory. Base type libraries like `k8s.io/api` appear to depend on leaf applications like `kube-proxy` — a Constitution Principle IX (Accuracy) failure that amplifies vulnerability-scan false positives.

The fix is investigation-heavy (matches milestone-160 shape): FR-007a/b/c prescribe root-cause classes to look for during T014–T016 empirical work (multi-`go.mod` walker attribution, `v0.0.0-unknown` version-tell handling, workspace-root vs `use`d-module discrimination). The spec's 3 concrete false edges (`k8s.io/api → kube-proxy`, `k8s.io/apimachinery → endpointslice`, `k8s.io/cli-runtime → streaming`) are the load-bearing SC-002 spot-checks.

**Technical approach**:

1. **Add a `go.work` parser** at `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs` (new sibling file). Parses the standardized go.work syntax: `use ( ... )` clauses, `replace <old> => <new>` directives, `go <version>` line. Line-based parser using stdlib only — no new deps.
2. **Detect workspace mode at Go-scan entry**: `legacy::read` checks `<rootfs>/go.work` (and honors `GOWORK=off`). Populates a new `WorkspaceContext.workspace_mode: WorkspaceMode` enum before the `candidate_project_roots` walk.
3. **Per-`use`d-module isolated attribution (FR-002)**: When workspace-mode is detected, the multi-project-root loop in `legacy::read` runs each `use`d module's resolver invocation with `GOWORK=off` at the subprocess level (so step-1 `go mod graph` returns that module's isolated view, not the merged workspace view). Combined with the milestone-055 per-project-root fresh `ModuleGraphMap`, this eliminates cross-workspace-member edge leakage at source.
4. **Q1 hybrid disposition for `v0.0.0-unknown` edges**: A post-resolution sweep classifies each candidate edge whose target has version `v0.0.0-unknown`:
   - If target is workspace-internal (a `use`d module) AND source's own go.mod's require block names the target → RESOLVE the target's version from the sibling's `go.mod`
   - Else → SUPPRESS the edge as false-positive per FR-002
5. **Document-scope C112 annotation**: New `mikebom:go-workspace-mode` doc-scope annotation reporting `detected: <N> use-modules` (per Q2, including the empty-use case as `detected: 0 use-modules`) or `malformed: <reason>` on parse failure. Absent when no `go.work` file at scanned root.
6. **Fix the FR-007 root causes** discovered during T014–T016 empirical investigation.

Q1–Q3 clarifications (spec §Clarifications) lock: hybrid RESOLVE-or-SUPPRESS disposition, `detected: 0 use-modules` for empty-use case, per-module `GOWORK=off go mod graph` as SC-001 ground-truth.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–160; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `std::process::Command` for the `GOWORK=off go mod graph` subprocess invocations (same pattern as milestone-055 `run_go_mod_graph` at `go_mod_graph.rs:81`), `std::fs::exists` for `go.work` detection, `serde`/`serde_json` (annotation values), `tracing` (FR-011 log), `anyhow`/`thiserror` (error propagation). **Zero new Cargo dependencies.** The go.work parser is stdlib-only (line-based, mirrors the existing `parse_go_mod` structure at `legacy.rs:200`).

**Storage**: N/A — all state in-process per scan; matches every milestone since 002. The new `WorkspaceMode` enum + `GoWorkDocument` parsed struct live on the stack for the duration of a single Go-reader invocation.

**Testing**: `cargo +stable test --workspace --no-fail-fast` per Constitution Development Workflow. New tests live in three tiers per milestone-055/091/158/160 precedent:
- Unit tests in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs` (module-inline `#[cfg(test)]`) covering go.work parsing + `WorkspaceMode` classification.
- Unit tests in `graph_resolver.rs::wiremock_integration` covering the Q1 hybrid disposition logic.
- Integration test at `mikebom-cli/tests/go_workspace_edges.rs` (per SC-010) with a synthetic 3-module go.work fixture exercising the release binary end-to-end.
- SC-001 audit-fixture regression against a to-be-added `test-kubernetes` fixture in the milestone-090 fixture-cache repo. Gated behind `MIKEBOM_WORKSPACE_EDGES_AUDIT=1` env var (matches milestone-083/160 external-tool test convention).

**Target Platform**: Linux + macOS + Windows dev hosts (per milestones 100/101). The `GOWORK=off` env-var pattern works cross-platform via `Command::env`.

**Project Type**: CLI (Rust workspace with 3 crates per Constitution Principle VI).

**Performance Goals**: Preserve milestone-055 posture — 16-way concurrent proxy fetches at `graph_resolver.rs:344`. Adding a `go.work` parse at Go-scan entry is O(single file read) and adds negligible overhead. Per-`use`d-module `GOWORK=off go mod graph` invocations run once per `use`d module (typically 40–100 per workspace); total added subprocess time is bounded by the milestone-055 30-second per-invocation timeout × use-module count. For `test-kubernetes` (47 use-modules), worst-case adds ~40 seconds if every subprocess times out; typical case adds ~2 seconds total. Acceptable.

**Constraints**: **No new Cargo dependencies** (FR spec assumption). **No new subprocess-invocation patterns** beyond `Command::env("GOWORK", "off")` on the existing `go mod graph` invocation. **No `.unwrap()` in production** per Constitution Principle IV. **Standards-native precedence** per Principle V — FR-009 documents the audit result (no CDX/SPDX-native workspace-mode field as of 2026-07-04).

**Scale/Scope**: `test-kubernetes` has 47 `use`d modules × ~4–8 direct requires each = ~250 candidate workspace-internal edges. Q1's hybrid disposition sweep is O(edges) with a per-edge HashMap lookup (source's own require set) — sub-second even on large workspaces. Golden regeneration impact: 0 (this milestone doesn't touch the single-module milestone-090 `golang` fixture); a new synthetic `golang-workspace` fixture will be added to the milestone-090 fixture-cache repo for the SC-010 integration test.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle-by-principle assessment

**I. Pure Rust, Zero C** — ✅ PASS. All work is user-space Rust in `mikebom-cli`. No FFI. No C dependencies added. `mikebom-ebpf` untouched.

**II. eBPF-Only Observation** — ✅ N/A. Milestone 161 does not touch discovery — it's a parity/emission-layer correctness fix. Discovery of `go.mod` files continues via the existing `candidate_project_roots` walker.

**III. Fail Closed** — ✅ PASS. `go.work` parse failures emit `malformed: <reason>` in the C112 annotation (fail-transparent per Constitution Principle X). Runtime doesn't fall back to guessing workspace membership from filesystem structure alone.

**IV. Type-Driven Correctness** — ✅ PASS. New `WorkspaceMode` enum with 3 variants + `GoWorkDocument` parsed struct + `EdgeDisposition` classifier per data-model.md. All new annotation values are enum-backed with a single `as_str()` serializer, matching the milestone-055 `ErrorClass` at `graph_resolver.rs:292` shape. No `.unwrap()` in production paths.

**V. Specification Compliance** — ⚠️ GATE. Two audit checks required:
- **Native-first check**: FR-009 explicitly documents the audit — no CDX 1.6 or SPDX 3.0.1 native field for "workspace-mode detection" as of 2026-07-04. The `mikebom:go-workspace-mode` prefix is compliant per Principle V's "parity-bridging" clause.
- **Existing-mikebom-annotation check**: `mikebom:go-transitive-source` (C108 from milestone 160) is per-component ladder-attribution and orthogonal to workspace-mode detection. `mikebom:graph-completeness` (C104 from milestone 158) is graph-existence at document scope, also orthogonal. C112 is genuinely new semantic territory.

**VI. Three-Crate Architecture** — ✅ PASS. All changes are in `mikebom-cli`; no new crates. `mikebom-common` gains no new types.

**VII. Test Isolation** — ✅ PASS. New unit tests are pure logic (go.work parser + hybrid disposition classifier). Integration test uses a synthetic fixture — no eBPF privilege. SC-001 audit test is gated behind `MIKEBOM_WORKSPACE_EDGES_AUDIT=1` (matches milestone-083 pattern).

**VIII. Completeness** — ✅ PASS. Milestone 161 does not remove any legitimate components or edges. Suppression per Q1 hybrid applies ONLY to workspace-internal targets NOT in the source's own require block — genuine false positives.

**IX. Accuracy** — ✅ CENTRAL. This milestone directly addresses the accuracy gap discovered in the milestone-155–160 audit expansion (30.8% wrong edges on test-kubernetes). Every false edge suppressed reduces vulnerability-scan false positives.

**X. Transparency** — ✅ PASS. The new C112 document-scope annotation surfaces workspace-mode detection to consumers. Suppression of workspace-internal `v0.0.0-unknown` edges is signaled implicitly through their absence (the SBOM was never claiming edges that don't exist — no consumer-visible loss of information).

**XI. Enrichment** — ✅ N/A. Milestone 161 does not fetch new external data.

**XII. External Data Source Enrichment** — ✅ PASS. `GOWORK=off go mod graph` shell-out is milestone-055's existing enrichment source with an added env var; unchanged. No new external source introduced.

### Strict Boundary compliance

**§1 (No lockfile discovery)** — ✅ N/A. `go.work` is used for workspace-membership detection, NOT for component discovery. Components continue to come from `go.mod` + `go.sum` per milestone-002 semantics.

**§2 (No MITM proxy)** — ✅ PASS. HTTP fetches remain via `reqwest::blocking::Client` per milestone 055.

**§3 (No C code)** — ✅ PASS.

**§4 (No `.unwrap()` in production)** — ✅ PASS. New code follows the milestone-055/091/160 pattern with `anyhow::Result` + `?` propagation.

**§5 (No file-tier duplicates in default mode)** — ✅ N/A. File-tier emission not touched.

### Gate result

Constitution Check **PASSES** — no violations. All principles + boundaries compliant.

## Project Structure

### Documentation (this feature)

```text
specs/161-go-workspace-edges/
├── plan.md              # This file
├── research.md          # Phase 0 output (R1–R7 below)
├── data-model.md        # Phase 1 output (entities: WorkspaceMode enum, GoWorkDocument, EdgeDisposition)
├── quickstart.md        # Phase 1 output (contributor path: build+test+audit)
├── contracts/
│   └── annotations.md   # Phase 1 output (per-format wire shape for C112)
├── checklists/
│   └── requirements.md  # Already exists from /speckit-specify
└── tasks.md             # /speckit-tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       └── golang/
│   │           ├── gowork.rs             # NEW: go.work parser + WorkspaceMode detection
│   │           ├── legacy.rs             # EDIT: read() branches on workspace-mode; per-use'd-module GOWORK=off attribution; Q1 hybrid disposition sweep
│   │           ├── graph_resolver.rs     # EDIT: WorkspaceContext gains use_modules_map + workspace_replaces; step1_go_mod_graph passes GOWORK=off when workspace-mode is detected
│   │           ├── go_mod_graph.rs       # EDIT: run_go_mod_graph accepts a `gowork_off: bool` flag
│   │           └── mod.rs                # unchanged
│   ├── scan_fs/
│   │   └── mod.rs                        # EDIT: ScanDiagnostics.go_workspace_mode field; population from GoScanSignals
│   ├── cli/
│   │   └── scan_cmd.rs                   # EDIT: doc-scope C112 annotation emission wiring; new go_workspace_mode field on ScanArtifacts
│   ├── generate/
│   │   ├── mod.rs                        # EDIT: ScanArtifacts.go_workspace_mode field
│   │   ├── cyclonedx/
│   │   │   ├── builder.rs                # EDIT: with_go_workspace_mode setter + threading into build_metadata
│   │   │   └── metadata.rs               # EDIT: C112 emission alongside C104/C110
│   │   └── spdx/
│   │       ├── annotations.rs            # EDIT: C112 emission at document scope (SPDX 2.3)
│   │       └── v3_annotations.rs         # EDIT: C112 emission at document scope (SPDX 3)
│   └── parity/
│       └── extractors/
│           ├── mod.rs                    # EDIT: register C112 row
│           ├── cdx.rs                    # EDIT: cdx_anno!(c112_cdx, "mikebom:go-workspace-mode", document)
│           ├── spdx2.rs                  # EDIT: spdx23_anno!() invocation
│           └── spdx3.rs                  # EDIT: spdx3_anno!() invocation
└── tests/
    └── go_workspace_edges.rs             # NEW: SC-010 integration test (synthetic 3-module go.work fixture)
```

**Structure Decision**: Milestone 161 is a targeted extension of milestone 055's Go transitive-edge resolver plus a new `gowork.rs` sibling file. No new crates. No new source-tree directories. The two edit hot-spots are `mikebom-cli/src/scan_fs/package_db/golang/{gowork,legacy,graph_resolver}.rs` (detection + attribution) and the standard doc-scope emission plumbing (mirrors milestone 160's C110/C111 pattern).

## Complexity Tracking

*No Constitution violations. Section not applicable.*

## Phase completion status

- ✅ **Phase 0 (research)** — see `research.md` for R1–R7 resolutions.
- ✅ **Phase 1 (design & contracts)** — see `data-model.md`, `contracts/annotations.md`, `quickstart.md`.
- 🔲 **Phase 2 (task decomposition)** — deferred to `/speckit-tasks`.

## Post-design constitution re-check

Post-design re-check passes. R1 confirms the semantic distinction between C112 and existing doc-scope annotations (C104, C110). R2 pins the `go.work` parser as line-based-stdlib-only (no new deps, no complex grammar). R3 confirms per-`use`d-module `GOWORK=off go mod graph` is Go's documented behavior for workspace-member isolation.

## Notes

- The plan preserves the milestone-055 concurrency + timeout posture unchanged (FR-008 explicit).
- FR-007a/b/c investigation-heavy tasks (T014–T016 in the forthcoming tasks.md) will need concrete `test-kubernetes` fixture access via the milestone-090 fixture cache; the empirical work is expected to take 3–5 iterations of scan-diff-hypothesize-fix.
- The SC-001 ≤ 5% target is empirically-adjustable per Assumptions §7 in the spec — if T014–T016 investigation reveals the FR-007 root causes are more complex than anticipated, revising SC-001 to a demonstrated-achievable floor is a legitimate outcome (milestone-156/157/158/159/160 precedent).
- One follow-on possibility that this plan does NOT prescribe: if the empirical investigation reveals workspace-mode false edges come from Go's own `go mod graph` output (i.e. even `GOWORK=off go mod graph` in a `use`d module produces wrong edges when the surrounding filesystem has a `go.work`), an alternative fix path would parse go.mod require blocks directly instead of shelling out. That's a bigger design change and would be its own milestone.
