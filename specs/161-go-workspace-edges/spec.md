# Feature Specification: Go workspace-mode false dep-graph edges (fix + regression guard)

**Feature Branch**: `161-go-workspace-edges`
**Created**: 2026-07-04
**Status**: Draft
**Input**: User description: "495" (implement fix for [issue #495](https://github.com/kusari-oss/mikebom/issues/495))

## Clarifications

### Session 2026-07-04

- Q: How should `v0.0.0-unknown` false edges to workspace-internal targets be disposed of? → A: Hybrid — SUPPRESS the edge iff the target is NOT in the source module's own require block (matches FR-002 per-module truthful attribution); RESOLVE the target's version from the sibling `use`d module's go.mod iff the target IS in the source's require block (preserves legitimate intra-workspace edges). Matches FR-002 exactly and avoids the false-leakage pattern that causes the 30.8% wrong-edge rate.
- Q: `mikebom:go-workspace-mode` value when `go.work` present but has zero `use` entries? → A: `detected: 0 use-modules`. File is syntactically valid, semantically empty — a legitimate Go workspace-mode scenario (e.g. scaffolding). Treating it as `malformed:` would false-positive consumer defect-detection heuristics. Transparency-first per Constitution Principle X — consumers see workspace-mode-detected + zero members, distinguishable from truly-absent `go.work`.
- Q: What ground-truth generator defines SC-001's ≤ 5% wrong-edge measurement? → A: Per-`use`d-module `cd <use-module-dir> && GOWORK=off go mod graph`. Executes `go mod graph` in each `use`d module's own directory with workspace mode explicitly disabled, so the result is that module's isolated declared-dep view (no cross-workspace edge merging). Matches FR-002 semantic exactly + milestone-160 Q3 per-module methodology. Reproducible by any auditor with `go` installed; deterministic single-command per module. Rejected whole-workspace `go mod graph` from root because Go's workspace-mode semantics merge edges across `use` members, diverging from FR-002 truthful per-module attribution.

## Motivation

Discovered during the milestone-157/158/159/160 audit expansion against `kusari-sandbox/test-*` repos: on Go repos using `go.work` workspace mode with multiple nested modules, mikebom emits **wrong `dependsOn` edges** — base type libraries appear to depend on unrelated leaf applications that don't actually declare them.

Empirical measurement 2026-07-03 on `kusari-sandbox/test-kubernetes` (Kubernetes v1.x with `go.work` + 40+ nested modules under `k8s.io/*`):

| shape | count | % |
|---|---|---|
| EXACT-MATCH (mikebom == `go mod graph`) | 107 | 50.0% |
| EMITTED-SUPERSET (mikebom emits MORE edges) | 9 | 4.2% |
| EMITTED-SUBSET (mikebom MISSING edges) | 27 | 12.6% |
| **DIVERGE** (both missing AND extra) | **57** | **26.6%** |
| Not emitted | 14 | 6.5% |

The **57 DIVERGE cases + 9 EMITTED-SUPERSET cases (66 total, 30.8% of edges)** are the load-bearing signature of this bug: mikebom emits edges that don't exist in reality. Three concrete examples from that data:

- `k8s.io/api` — a base type library everyone imports — emits an edge to `pkg:golang/k8s.io/kube-proxy@v0.0.0-unknown`. `go mod graph` has no such edge; kube-proxy is an APPLICATION built ON TOP OF k8s.io/api, not something the type library depends on.
- `k8s.io/apimachinery` emits an edge to `pkg:golang/k8s.io/endpointslice@v0.0.0-unknown`.
- `k8s.io/cli-runtime` emits an edge to `pkg:golang/k8s.io/streaming@v0.0.0-unknown`.

The `v0.0.0-unknown` version pattern on every false-edge target is the diagnostic tell: these are workspace-declared modules whose version wasn't resolved (because they're in-workspace, not proxy-fetched).

## Suspected root cause

Kubernetes-style `go.work`:

```text
go 1.34.0
use (
    .
    ./staging/src/k8s.io/api
    ./staging/src/k8s.io/apimachinery
    ./staging/src/k8s.io/kube-proxy
    ...
)
```

The `use` clause makes all these modules available under a single build unit. Hypothesis (verified during T014-T016 empirical investigation): mikebom's Go graph builder takes the workspace-root's declared deps and inadvertently attributes them to EACH `use`d module. Correct behavior: each `use`d module's edges come from its OWN `go.mod`, independent of siblings.

Consumer impact: **vulnerability scanners see falsely-connected components** — a CVE against `k8s.io/api` would appear to propagate through `kube-proxy` when it doesn't in reality, and vice versa. False-positive vulnerability signaling erodes SBOM trust (Constitution Principle IX — Accuracy).

## Distinction from #494 (milestone 160)

Milestone 160 addressed **missing** edges (52.2% edge coverage on `test-podman` — proxy-fetch failures dropping legitimate edges). This issue addresses **wrong** edges (26.6% DIVERGE + 4.2% EMITTED-SUPERSET on `test-kubernetes` — workspace-root deps leaking into siblings). The two are complementary but non-overlapping: fixing missing-edge resolution doesn't help if the emitted edges are wrong.

## User Scenarios & Testing

### User Story 1 - SBOM consumer gets accurate dep-graph edges on Go workspace-mode repos (Priority: P1)

An SBOM consumer (Kusari Inspector, a vulnerability scanner, an SBOM comparator) loads mikebom's Go SBOM for a `go.work`-based workspace repo and finds that per-module `dependsOn` edges match `go mod graph` executed in each module's own directory. Base type libraries (`k8s.io/api`, `k8s.io/apimachinery`) MUST NOT show outgoing edges to sibling application modules (`kube-proxy`, `endpointslice`) unless those apps really do appear in the type library's own `require` block.

**Why this priority**: This is the observed bug's user-visible symptom. Without this fix, mikebom's SBOMs on any go.work-based repo (Kubernetes, Docker's moby, containerd, etc.) contain 26-31% wrong edges. Vulnerability scans amplify each false edge into a spurious CVE finding, eroding operator trust in mikebom's output (Constitution Principle IX — Accuracy). The `v0.0.0-unknown` version tell on every false edge makes this bug visible + audit-detectable, but it also inflates the SBOM footprint with phantom module-version pairs.

**Independent Test**: Scan `kusari-sandbox/test-kubernetes` with mikebom. For each `use`d workspace module M in the emitted SBOM, execute `go mod graph` in M's directory (not the workspace root). Assert:

- The set of outgoing edges from M in the emitted SBOM MATCHES the direct edges reported by `go mod graph` executed in M's directory (modulo legitimate build-tag filtering).
- Zero false edges — every emitted edge whose source is M has a corresponding line in M's own `go.mod`'s require block.
- The 3 specific false edges from the milestone-160 audit (`k8s.io/api → kube-proxy`, `k8s.io/apimachinery → endpointslice`, `k8s.io/cli-runtime → streaming`) MUST NOT appear.

**Acceptance Scenarios**:

1. **Given** `test-kubernetes` scanned in online mode with `GOPROXY=https://proxy.golang.org`, **When** mikebom emits the CDX, **Then** the emitted DIVERGE-shape edge count MUST be ≤ 5% (compared to the pre-161 baseline of 26.6%).

2. **Given** the same scan, **When** enumerating `k8s.io/api`'s outgoing `dependsOn`, **Then** the list MUST NOT include `k8s.io/kube-proxy`, `k8s.io/endpointslice`, `k8s.io/cli-runtime`, or any sibling workspace application module.

3. **Given** the same scan, **When** enumerating any emitted edge's target, **Then** the target's version MUST NOT be `v0.0.0-unknown` for any workspace-internal module (workspace modules should be identified by their real declared version OR by a workspace-root component reference that never appears as an edge target).

4. **Given** a non-workspace Go repo (`test-podman` from milestone 160), **When** mikebom scans, **Then** the emitted dep-graph shape MUST be byte-identical to pre-161 (this fix is scoped to workspace-mode; non-workspace scans MUST NOT regress).

---

### User Story 2 - Document-scope go-workspace-mode transparency annotation (Priority: P2)

A compliance auditor loads a mikebom SBOM for a go.work workspace repo and wants to know at document scope: did mikebom detect `go.work` mode, how many `use`d modules were discovered, and were per-module edges attributed correctly? mikebom emits document-scope `mikebom:go-workspace-mode` when a `go.work` file is present, naming the workspace-mode detection outcome and use-count.

**Why this priority**: Constitution Principle X (Transparency). Consumers need to programmatically detect workspace-mode scans (which have different edge-attribution semantics than single-module scans) without inspecting the source tree.

**Independent Test**: For every emitted SBOM containing at least one Go component AND a `go.work` file in the scanned root, assert:

- `mikebom:go-workspace-mode` annotation is present exactly once at document scope.
- Value shape follows the milestone-158/160 grammar: `<detection>: <use-count> use-modules[; additional-fields]`.
- Detected values: `detected: N use-modules`, `absent` (no go.work), `malformed: <reason>` (go.work parse failure).

**Acceptance Scenarios**:

1. **Given** `test-kubernetes` scanned, **When** mikebom emits the SBOM, **Then** the metadata MUST contain `mikebom:go-workspace-mode = "detected: 47 use-modules"` (or however many `use` entries the fixture has at test time).

2. **Given** a non-workspace repo (`test-podman`), **When** mikebom scans, **Then** the `mikebom:go-workspace-mode` annotation MUST NOT appear (absent = default).

3. **Given** a repo with a syntactically malformed `go.work` file, **When** mikebom scans, **Then** the annotation value MUST start with `malformed:` and name the parse failure class.

---

### User Story 3 - Non-Go and non-workspace scans byte-identical to pre-161 (Priority: P3)

Users scanning repos with NO Go components OR with Go components but no `go.work` file see byte-identical SBOM output vs. pre-161 milestones.

**Why this priority**: Regression guard. The workspace-mode fix + new annotation MUST be dormant when `go.work` is not present. SC-003 dual-side byte-identity precedent (milestones 157–160).

**Independent Test**: Regenerate all 10 non-Go milestone-090 goldens + the `golang` (single-module, no go.work) fixture with the milestone-161 code. Diff against pre-161. Zero diff bytes on 10 non-Go goldens; zero diff bytes on the `golang` (non-workspace) fixture.

**Acceptance Scenarios**:

1. **Given** the milestone-090 npm fixture (no Go components), **When** mikebom scans, **Then** the emitted CDX diff vs. pre-161 is exactly ZERO bytes.

2. **Given** the milestone-090 `golang` fixture (Go, but no `go.work`), **When** mikebom scans, **Then** the emitted CDX diff vs. pre-161 is exactly ZERO bytes.

### Edge Cases

- **Repo with `go.work` but zero `use` entries**: technically valid but semantically empty. mikebom emits `mikebom:go-workspace-mode = "detected: 0 use-modules"` and treats the scan identically to a non-workspace scan (edges from any go.mod files come from those go.mods).

- **Repo with `go.work` `use .` only** (single-entry workspace pointing at repo root): equivalent semantic to non-workspace mode. mikebom emits `detected: 1 use-modules` for transparency but MUST NOT introduce workspace-attribution false edges (there's only one workspace member).

- **Nested `go.work` files**: A `go.work` file inside a `use`d module. Go itself treats the outer `go.work` as authoritative; mikebom MUST match this behavior. Inner `go.work` files under `use`d directories are ignored.

- **`go.work replace` directives**: The `go.work` file can carry its own `replace` block that overrides individual `use`d modules' `go.mod` replaces. mikebom MUST honor `go.work replace` at workspace-scope resolution.

- **Workspace-root has its own go.mod**: `use .` implies the workspace root IS itself a module. Edges from that go.mod come from that go.mod's require block, not from any child module's require block. The bug's symptom (workspace-root deps leaking into siblings) suggests this attribution is currently broken.

- **`v0.0.0-unknown` version tell**: emitted iff mikebom couldn't resolve a target module's version through any ladder step. In workspace-mode scans, this frequently indicates the false-edge bug — the target is actually a workspace-internal module that has a real declared version in its own go.mod. Fix: detect the workspace-internal target and either resolve to the real version from the sibling go.mod, OR (better) suppress the edge if the source module doesn't legitimately require the target.

- **Mixed workspace + `go.work` disabled at scan time** (`GOWORK=off`): Go treats each go.mod as an independent module. mikebom MUST honor `GOWORK=off` and skip go.work parsing when set.

## Requirements

### Functional Requirements

- **FR-001**: mikebom MUST detect `go.work` files at the scanned root during Go scans. Detection uses the presence of a `go.work` file (or `go.work.sum` companion) at the workspace root. When detected, mikebom transitions to workspace-mode dep-graph resolution.

- **FR-002**: In workspace-mode, mikebom's per-module edge emission MUST come from EACH `use`d module's own `go.mod` require block, NOT from the workspace-root or from sibling modules. Concretely: for `use`d modules M1, M2, M3 with respective require lists R1, R2, R3, the emitted `dependsOn(M_i)` is derived from R_i alone.

- **FR-003**: The workspace root's own `go.mod` (when `use .` is present) MUST be treated as ANOTHER `use`d module — its edges come from its OWN require block, not merged into sibling modules.

- **FR-004**: mikebom MUST emit a document-scope annotation `mikebom:go-workspace-mode` with value `detected: <N> use-modules`, `absent`, or `malformed: <reason>` per US2 semantics. Absent iff `go.work` not present in the scanned root (byte-identity guard).

- **FR-005**: mikebom MUST honor `go.work replace` directives at workspace resolution scope. Any `replace <old> => <new>` in `go.work` overrides equivalent-shape replaces in individual `use`d modules' `go.mod` files. Per Go MVS semantics: workspace-level replaces have precedence over module-level replaces.

- **FR-006**: mikebom MUST honor the `GOWORK=off` environment variable per Go semantics: when set, `go.work` files MUST be ignored and per-module edges attributed as if workspace-mode is disabled. mikebom's `--offline` flag does NOT imply `GOWORK=off`.

- **FR-007**: mikebom MUST fix the workspace-attribution root cause. Concrete required fixes (verified 2026-07-04 during specification via `test-kubernetes` failure inspection):

  - **FR-007a**: The multi-`go.mod` walker MUST identify the workspace-root vs `use`d modules explicitly. When the current implementation walks all discovered `go.mod` files as a single flat set + attributes discovered edges as if all deps belong to one main module, workspace-attribution breaks. Fix: per-`go.mod` scoped emission.

  - **FR-007b**: The `v0.0.0-unknown` version pattern MUST be treated as a resolution-failure diagnostic, not as a legitimate emitted version. When mikebom is about to emit an edge to a workspace-internal target with unknown version, mikebom MUST either (a) resolve the target's real version from the sibling `use`d module's `go.mod`, OR (b) suppress the edge as a false-positive per FR-002 (target not in source's require list) — the correct action depends on T014-T016 empirical findings.

  - **FR-007c**: mikebom MUST NOT emit workspace-internal modules with synthetic `v0.0.0-unknown` versions as SBOM components. Workspace-internal modules SHOULD be represented as ONE main-module component per `use`d directory, with the real version declared in that module's `go.mod` (or `v0.0.0` if the workspace declares no version — matches Go's workspace semantics).

- **FR-008**: mikebom MUST preserve milestone-055's proxy-fetch behavior + milestone-091's go.sum fallback for external module resolution. This milestone is about FIXING workspace-attribution, not changing the external-module fetch path.

- **FR-009**: Standards-native precedence per Constitution Principle V. If either CDX 1.6 or SPDX 3.0.1 introduces an official "workspace-mode detection" property, mikebom MUST prefer that property. As of 2026-07-04, no such standard property exists; the `mikebom:go-workspace-mode` prefix is used.

- **FR-010**: `mikebom:go-workspace-mode` MUST be registered as a new document-scope parity-catalog row (C112) with `Directionality::SymmetricEqual` — matching the milestone-158/160 doc-scope pattern.

- **FR-011**: When per-workspace-module resolution is invoked, mikebom MUST emit an info-level tracing log at scan-emission time: `"go workspace resolution summary"` with fields `use_module_count`, `workspace_replace_count`, `has_workspace_root_gomod`, plus per-module edge counts. Grep-friendly for CI-log analysis per the milestone-157/158/159/160 observability convention.

### Key Entities

- **Workspace mode**: Enum `{Detected(use_count: usize), Absent, Malformed(reason: String)}`. Determines document-scope C112 annotation value and whether workspace-attribution logic applies.

- **`mikebom:go-workspace-mode` (document-scope)**: Document-scope annotation carrying the workspace-mode detection outcome. Emitted iff a `go.work` file is present in the scanned root.

- **Use-module map**: Per-scan mapping from workspace root → `Vec<PathBuf>` of `use`d module directories. Consumed by the per-`use`d-module edge attribution loop (FR-002).

- **Workspace-replace map**: Per-scan `HashMap<ModuleId, ModuleId>` of `go.work replace` directives. Applied at workspace resolution scope (FR-005), overriding any equivalent-shape replaces in individual `use`d modules' `go.mod` files.

## Success Criteria

### Measurable Outcomes

- **SC-001 (test-kubernetes false-edge fix, online mode)**: After milestone 161 ships, running `mikebom sbom scan --path test-kubernetes --format cyclonedx-json` (online mode, `GOPROXY=https://proxy.golang.org`) and comparing per-`use`d-module edges against `go mod graph` executed in each module's directory MUST show ≤ 5% DIVERGE-shape edges (measured as `|mikebom_wrong_edges| / |go_mod_graph_edges|`). Pre-161 empirical baseline: **26.6% DIVERGE + 4.2% EMITTED-SUPERSET = 30.8% wrong edges**. Target: ≤ 5% wrong edges. This SC is empirically-locked to the concrete testbed named in issue #495.

- **SC-002 (test-kubernetes specific false-edge suppression)**: The 3 concrete false edges from the milestone-160 audit MUST NOT appear in the emitted SBOM: `k8s.io/api → kube-proxy`, `k8s.io/apimachinery → endpointslice`, `k8s.io/cli-runtime → streaming`. All 3 edges are load-bearing — they represent the class of "base library falsely depends on leaf application" that erodes vulnerability-scan trust.

- **SC-003 (dual-side byte-identity guard, mirrors milestones 158/159/160)**: For every milestone-090 non-Go golden fixture (10 of 11 ecosystems: apk, bazel, cargo, cmake, deb, gem, maven, npm, pip, rpm), the emitted CDX / SPDX 2.3 / SPDX 3 SBOMs MUST be byte-identical to pre-161. For the `golang` (single-module, no go.work) fixture, ALSO byte-identical (this milestone is scoped to workspace-mode). Zero diff bytes on the 10 non-Go × 3 + 1 golang × 3 = 33 goldens.

- **SC-004 (workspace-mode annotation presence)**: 100% of emitted SBOMs from repos with a `go.work` file at the scanned root MUST carry a `mikebom:go-workspace-mode` document-scope annotation. 100% of emitted SBOMs WITHOUT a `go.work` file MUST NOT carry the annotation (byte-identity guard).

- **SC-005 (workspace-mode annotation value correctness)**: The `detected: <N> use-modules` value's `<N>` MUST equal the count of `use` clause entries in the parsed `go.work` file. For test-kubernetes, this is >= 40 (exact number verified against fixture at test time).

- **SC-006 (workspace `v0.0.0-unknown` suppression)**: Zero emitted `dependsOn` targets in the CDX MUST reference a workspace-internal module with version `v0.0.0-unknown`. Workspace-internal modules MUST be represented at their real version (from `go.mod`) OR as workspace-root components with the workspace-declared version.

- **SC-007 (non-workspace repo regression guard)**: Scanning the milestone-090 `golang` (single-module) fixture MUST produce byte-identical output vs. pre-161. Verifies the workspace-mode code is dormant when no `go.work` is present.

- **SC-008 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST both pass with zero errors before the PR is opened. The mandatory `./scripts/pre-pr.sh` gate must be green.

- **SC-009 (unit test coverage)**: The new workspace-mode code paths + FR-007 attribution-fix MUST have at least 10 unit tests covering: (a) `go.work` detection when file present; (b) `go.work` detection when file absent; (c) `go.work replace` directive parsing; (d) `use` clause parsing with quoted + unquoted paths; (e) `GOWORK=off` disables workspace-mode; (f) empty `use ()` block → 0 use-modules; (g) per-`use`d-module edge attribution — source module M's edges come from M's own require block; (h) `k8s.io/api`-shape workspace member emits ZERO edges to sibling app modules; (i) `v0.0.0-unknown` version detection + suppression in workspace-mode; (j) workspace-root's own `use .` module is treated as a normal `use`d module.

- **SC-010 (integration test)**: A new integration test at `mikebom-cli/tests/go_workspace_edges.rs` MUST synthesize a Go workspace with a `go.work` file + 3+ nested modules (one base library, one middle library depending on base, one leaf application depending on middle), scan it via the release binary, and assert per-`use`d-module edges match ground-truth (base has no outgoing edges; middle depends on base; leaf depends on middle) with ZERO false edges.

- **SC-011 (CHANGELOG entry)**: `CHANGELOG.md` MUST document the workspace-mode fix + FR-004/007 annotation vocabulary + the SC-001 empirical numbers + a consumer jq recipe for detecting workspace-mode SBOMs.

- **SC-012 (parity catalog registration)**: The new annotation (C112 document-scope) MUST have a parity-catalog entry with `Directionality::SymmetricEqual`. Milestone-071 parity check MUST pass symmetrically across CDX / SPDX 2.3 / SPDX 3.

- **SC-013 (issue #495 closure)**: Issue #495 MUST reference this milestone (`closes #495` in the impl commit message) and the milestone MUST demonstrably resolve the reported symptom (30.8% wrong edges → ≤ 5% on test-kubernetes).

## Assumptions

- **Ground truth = per-module `go mod graph`**: The `go mod graph` command output executed in EACH `use`d module's directory (not the workspace root) is the authoritative source for what edges "should" be in the SBOM for that module. SC-001 measures against this. Consumers running SC-001 verification themselves need the `go` binary installed.

- **`test-kubernetes` is the empirical benchmark**: SC-001/SC-002 numbers are pinned to this repo. The 3 specific false edges from the milestone-160 audit are the load-bearing verification.

- **Online mode is the primary target**: SC-001's ≤ 5% wrong-edge target applies to online mode. Offline mode may have different behavior but is NOT the primary fix target; workspace-attribution correctness applies in both modes.

- **No new Cargo dependencies**: Following the milestone-157/158/159/160 precedent, this work uses existing crates only.

- **milestone-090 golang fixture unchanged**: The current `golang` fixture is a single-module Go repo (no `go.work`). Milestone 161 does NOT modify it — SC-007 verifies its output stays byte-identical.

- **New `golang-workspace` fixture needed**: SC-010 requires a new synthetic go.work fixture with 3+ nested modules. Fixture will be added to the milestone-090 fixture-cache repo under `go/workspace-multi-module/` — same pattern as existing fixtures.

- **`go.work` file format is well-specified**: The go.work grammar is documented at pkg.go.dev/cmd/go#hdr-Workspaces (Go 1.18+). Parsing is straightforward line-based (`use (`, `use "./path"`, `replace <old> => <new>`, comment handling). No new crates needed.

- **SC-001 target is empirically-adjustable**: If T014-T016 investigation reveals the FR-007 root causes are more complex than anticipated, SC-001 may be revised inline per the milestone-156/157/158/159/160 empirical-revision pattern. The floor is a demonstrable reduction from the 30.8% pre-161 baseline; the aspirational target is ≤ 5% wrong edges.

- **Workspace-mode may reveal downstream orphan-classification issues**: The pre-milestone-158 `mikebom:graph-completeness` signal may have been artificially inflated toward "complete" by workspace-mode false edges (a base-library orphan would fail completeness, but a false edge from a leaf module to that base library makes it artificially reachable). Post-161, the milestone-158 completeness signal on test-kubernetes may correctly report `partial` where it previously reported `complete`. This is a legitimate behavior change, not a regression.

## Out of Scope

- **The Ruby built-in gems edge fix (issue #496)** — separate milestone. Ruby scans are unaffected by this work.

- **The npm phantom empty-version edges fix (issue #498)** — separate milestone. npm scans are unaffected.

- **Extending milestone-160's transitive-coverage annotations to workspace-mode**: The C108/C109/C110/C111 annotations continue working as-is — they describe per-external-module ladder attribution, orthogonal to workspace-mode attribution. Milestone 161 does NOT add new per-component workspace-mode annotations; the doc-scope C112 alone is sufficient signal.

- **Cross-workspace attribution** across multiple `go.work` files in a monorepo: mikebom scans one workspace root at a time. Nested `go.work` files under `use`d directories are ignored per Go's own semantics.

- **`go.work.sum` verification**: mikebom does NOT verify `go.work.sum` integrity hashes. That's `go` toolchain scope. mikebom only READS `go.work` for `use` clause + `replace` directive extraction.

- **Filesystem walker changes**: The milestone-114 `safe_walk` helper + milestone-113 `--exclude-path` behavior is unchanged. Workspace-mode detection is a `std::fs::exists("<root>/go.work")` check at Go-scan entry, not a walker extension.
