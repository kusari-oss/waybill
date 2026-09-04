# Feature Specification: Reduce `go mod why` subprocess-spawn amplification on Go monorepos

**Feature Branch**: `771-gomodwhy-subprocess-scale`
**Created**: 2026-09-04
**Status**: Draft
**Input**: Issue #745 empirical decomposition — see Motivation

## Motivation

Scanning the Kubernetes source tree with waybill takes 82.6s (v0.6.0) / 80.5s (v0.6.1). Empirical decomposition shows 63 seconds of that time comes from the m112 `go mod why` build-inclusion classification pass:

| Configuration | Wall time | Δ |
|---|---:|---:|
| A — default (`--offline --no-deep-hash`) | 80.5s | — |
| B — `+ --no-go-mod-why` | **19.3s** | **−76%** |
| C — `+ --no-binary-scan=all` | 18.2s | −6% |
| D — `+ --exclude-path vendor,third_party` | 18.3s | noise |

The bottleneck is subprocess-spawn amplification, **not** algorithmic complexity in the file walker or graph resolver. `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs` runs per-workspace preflights + chunked `go mod why -m` queries with `CHUNK_SIZE = 20`. For Kubernetes (39 `go.mod` files under `go.work`, 246 modules in root `go.sum`):

- 39 workspaces × (1 `go list all` preflight + ~13 `go mod why -m -vendor` chunks) = **546 subprocess spawns**
- Each Go toolchain invocation is ~50-100ms fork/exec plus ~500ms-1s of module-graph load
- Scan hits the 60-second wall-clock budget cap at `elapsed_ms=60005`; 22 sibling main modules under `staging/src/k8s.io/*` are skipped entirely with the `budget-exhausted` marker → **22/39 workspaces receive no classification data**

## Clarifications

### Session 2026-09-04

- Q: US3 shared preflight — which working directory should the shared `go list all` run from? → A: The `go.work` file's parent directory (workspace root). Deterministic union graph in workspace mode; per-member `replace` directives affect only version selection, not resolvability, and the preflight's sole job is to detect resolvability failures.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — CHUNK_SIZE amplification eliminated (Priority: P1)

An operator scans a Go monorepo (representative fixture: Kubernetes at `kusari-sandbox/test-kubernetes`) with default flags (`waybill --offline sbom scan --path <k8s> --no-deep-hash --format cyclonedx-json --output out.cdx.json`). Wall-time completes in a fraction of the v0.6.1 baseline **without** the operator having to reach for `--no-go-mod-why` or any other workaround flag. Every module the pre-fix scan would have classified receives its classification verdict in the emitted SBOM.

**Why this priority**: This tier alone is a one-line change with the highest impact-per-line ratio in the milestone. It is byte-identical to the pre-fix output (chunk size does not affect `go mod why -m` semantics), fully backwards compatible with every existing corpus fixture, and yields the majority of the wall-time saving. Ships first as the MVP.

**Independent Test**: Run the SC-001 command against the Kubernetes fixture with only US1 landed. Wall-time must fall to ≤ 30 seconds (from the 80-second baseline). Every module that was classified under v0.6.1 (`analyzed=421` per the FR-013 log) must still be classified, with identical verdicts, in the US1 output.

**Acceptance Scenarios**:

1. **Given** a Go monorepo with a large per-workspace module set (≥100 modules in a single main-module's `go.sum`), **When** the operator runs a default scan, **Then** the number of `go mod why -m` subprocess invocations per workspace falls from `ceil(N/20)` to `ceil(N/large_chunk_size)` where `large_chunk_size` is at least 500.
2. **Given** the Kubernetes fixture, **When** the operator runs the SC-001 command, **Then** wall-time is ≤ 30 seconds and the FR-013 summary log shows an `analyzed=` count at least as high as the v0.6.1 baseline.
3. **Given** any Go fixture in `waybill-cli/tests/fixtures/golang/`, **When** the CDX/SPDX output is compared byte-for-byte against the v0.6.1 golden (modulo version-string cascades), **Then** the outputs are identical.

---

### User Story 2 — Parallel workspace analysis (Priority: P2)

The same operator's default scan of the same Go monorepo completes even faster because independent workspace analyses run concurrently on the operator's machine, bounded by CPU count. The scan's log lines per workspace annotate the workspace path so operators can correlate diagnostics when concurrent output interleaves.

**Why this priority**: Independent value on top of US1 — cuts remaining wall-time by a factor near the operator's core count. Requires more care (shared budget accounting, subprocess concurrency cap) but the shape mirrors the existing golang resolver's parallel-proxy-fetch pattern (`graph_resolver.rs:1001`). Ships after US1 is validated.

**Independent Test**: Run the SC-001 command against the Kubernetes fixture with US1 + US2 landed on a machine reporting ≥ 4 logical CPUs. Wall-time must fall to ≤ 15 seconds. Concurrent per-workspace warn/info log lines each carry an explicit workspace identifier so an operator can attribute any degradation to a specific workspace.

**Acceptance Scenarios**:

1. **Given** a Go monorepo with ≥ 4 workspaces, **When** a default scan runs on a machine with ≥ 4 logical CPUs, **Then** at least 4 workspaces are analyzed concurrently as measured by overlapping child-process wall-time.
2. **Given** the same monorepo, **When** the number of running child `go` processes at any point is sampled, **Then** it never exceeds the operator's logical-CPU count.
3. **Given** a scan where one workspace's preflight fails, **When** the log is inspected, **Then** the warn line naming the failing workspace's absolute path is present exactly once and does not block classification of any sibling workspace.
4. **Given** the shared wall-clock budget of 60 seconds, **When** two workspaces run concurrently and one consumes 40 seconds, **Then** the other workspace stops taking new chunks once total elapsed time reaches 60 seconds (not per-workspace 60s).

---

### User Story 3 — Shared preflight across sibling workspaces (Priority: P3)

The same operator scans a `go.work`-managed monorepo. Every workspace member listed in the `go.work` file shares a single `go list all` reliability preflight rather than each member running its own. Non-workspace main modules keep running their own preflight, unchanged.

**Why this priority**: Removes redundant preflight cost that scales linearly with workspace-member count. Complementary to US1 and US2 — US1 cut chunk count, US2 parallelized workspaces, US3 eliminates duplicated preflight work altogether. Requires grouping detected main-modules by their `go.work` root and a single-scan cache with a clear fallback for non-workspace roots. Highest complexity of the three tiers; largest incremental win when the monorepo has many `go.work` members.

**Independent Test**: Run the SC-001 command against the Kubernetes fixture with all three US landed. Wall-time must fall to ≤ 10 seconds. The count of `go list all` invocations executed during the scan must equal the number of distinct `go.work` roots in the tree plus the count of main-modules that are NOT `go.work` members (i.e., zero duplicated preflights within any single `go.work` scope).

**Acceptance Scenarios**:

1. **Given** a monorepo with a single `go.work` file listing N members, **When** a default scan runs, **Then** exactly one `go list all` preflight is executed for that `go.work` scope regardless of N.
2. **Given** a monorepo with a single `go.work` file listing N members plus one out-of-workspace `go.mod`, **When** a default scan runs, **Then** exactly two `go list all` preflights are executed (one for the workspace, one for the out-of-workspace main-module).
3. **Given** a monorepo where the shared preflight succeeds but one workspace member's `go mod why` chunk fails, **When** the log is inspected, **Then** the failing member reports `subprocess-error` for its own chunk while sibling members whose chunks succeeded still produce verdicts.
4. **Given** the shared preflight fails (e.g., `go list all` exits non-zero), **When** the classifier is invoked, **Then** all members of that `go.work` scope are marked with `skip_reason: UnresolvablePackages` and no `go mod why -m` chunks are attempted for any of them.

### Edge Cases

- **Zero Go main-modules present**: the classifier is not invoked; no subprocess is spawned; no diagnostic is emitted.
- **Go toolchain absent from `$PATH`**: existing degrade path is preserved — the `toolchain_available` probe fails, the classifier reports its existing toolchain-missing skip for every main-module, and the scan continues.
- **Single-workspace repo (1 `go.mod`, no `go.work`)**: US1 applies (larger chunk size), US2 has nothing to parallelize (no-op), US3 has no sibling to share preflight with (no-op). Wall-time is dominated by preflight + a single chunk.
- **`go.work` present but `go` binary too old to support workspaces**: existing m231 `WorkspaceMode::Off` path returns; US3 falls back to per-workspace preflight; no attempt to use `go.work` semantics.
- **Argv length limit exceeded by a single chunk**: a defensive cap in the chunk-size selection MUST keep every invocation below the operating system's ARG_MAX (POSIX minimum: 128 KiB). Empirically 500-1000 module paths at ~40 chars per path fits with wide margin; the cap SHOULD be documented in code but SHOULD NOT require operator-facing configuration.
- **Concurrent workspaces contending for shared budget**: workspace A consumes 55s and workspace B has just started chunk 1; B's chunk starts under the 5s remaining and either completes or times out via the existing `Invocation::TimedOut` path. No workspace is deadlocked.
- **Concurrent workspaces sharing preflight but one member has diverged replace directives**: divergence within a `go.work` is possible in principle. When the shared preflight succeeds, `go mod why -m` is still run per-member (unchanged), so per-member replace directives still surface. When per-member `go mod why -m` output diverges from the shared preflight's module set, the divergent modules receive `Unresolved` verdicts per the existing tier-2 fallback — no silent misclassification.
- **Log-line ordering under concurrent execution**: warn/info emitted by concurrent workspaces MAY interleave. Each per-workspace log line MUST carry the workspace's absolute path in the structured `main_module` field so operators can grep-reconstruct per-workspace timelines.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The build-inclusion classifier MUST batch module paths into `go mod why -m` invocations of size ≥ 500 by default, subject to a defensive argv-length guard (below).
- **FR-002**: The build-inclusion classifier MUST NOT construct any single subprocess argument list whose total byte length exceeds the operating system's ARG_MAX limit. When honoring the batch size would exceed that limit, the classifier MUST split the batch into the largest chunks that fit and continue.
- **FR-003**: The build-inclusion classifier MUST analyze detected main-modules concurrently, bounded by a subprocess-concurrency cap equal to the operator's reported logical-CPU count (`std::thread::available_parallelism()`).
- **FR-004**: The build-inclusion classifier MUST share the shared 60-second wall-clock budget across concurrent workspace analyses (no per-workspace 60s budget).
- **FR-005**: Every warn/info log line emitted by a concurrent workspace analysis MUST carry the workspace's absolute path in the structured `main_module` field so operators can correlate interleaved output.
- **FR-006**: When a `go.work` file is detected at a scope that contains multiple main-modules, the classifier MUST execute the `go list all` reliability preflight exactly once per `go.work` scope, running the invocation from the `go.work` file's parent directory, and reuse its result for every member of that scope.
- **FR-007**: When the shared preflight fails, every member of that `go.work` scope MUST be marked with the existing `SkipReason::UnresolvablePackages` and no `go mod why -m` chunks MUST be executed for any of them.
- **FR-008**: Main-modules that are NOT members of any detected `go.work` scope MUST continue to execute their own `go list all` preflight (fallback path — unchanged from the pre-fix behavior).
- **FR-009**: The FR-013 summary log emitted at classifier completion MUST retain its existing wire-shape (fields `analyzed=`, `prod=`, `test=`, `not_needed=`, `unresolved=`, `unknown_marked=`, `workspace_modules=`, `skipped=`, `elapsed_ms=`) so downstream tooling that parses the log continues to work.
- **FR-010**: The `--no-go-mod-why` operator flag, its `WAYBILL_NO_GO_MOD_WHY` env-var equivalent, and the existing skip semantics (classifier not invoked at all when set) MUST NOT change.
- **FR-011**: The `apply_offline_env` env-var pinning (`GOPROXY=off`, `GOFLAGS=-mod=mod`, `GOTOOLCHAIN=local`) triggered by `--offline` / `WAYBILL_OFFLINE` MUST apply identically to every subprocess spawned under the new concurrent path.
- **FR-012**: Every existing regression fixture under `waybill-cli/tests/fixtures/golang/` MUST produce identical CDX/SPDX output (modulo version-string cascades) with the milestone landed.
- **FR-013**: The milestone MUST NOT introduce any new Cargo dependencies. The concurrent execution path MUST reuse the existing `std::thread`, `std::sync::mpsc`, and `std::sync::atomic` primitives already present in `mod_why.rs` and `graph_resolver.rs`.
- **FR-014**: The milestone MUST preserve the m231 workspace-mode detection contract (`WorkspaceMode::Active/Explicit/Inactive/Off`). This milestone extends its use — it does not change its semantics.
- **FR-015**: The verdict-classification rules (`ProdNeeded`, `TestOnly`, `NotNeeded`, `Unresolved`) MUST NOT change. Every module that receives a verdict under the pre-fix classifier MUST receive the same verdict under the post-fix classifier.

### Key Entities

**Terminology note**: This milestone inherits the m112 code's variable naming where `workspaces: Vec<PathBuf>` refers to a list of main-module directories (each containing a `go.mod`). In this spec, **"main-module" and "workspace" refer to the same thing** — a directory with a `go.mod`. The Go 1.18+ `go.work` construct is called a **"go.work scope"** or **"scope"** throughout, never a "workspace", to avoid overloading the term.

- **Main-module** (a.k.a. **workspace** in code): A directory containing a `go.mod` file discovered by the walker. Each main-module produces its own set of module-classification verdicts.
- **`go.work` scope**: A directory containing a `go.work` file. Zero or more main-modules can be members of a single `go.work` scope. Membership is derived from the `use` directives in the `go.work` file.
- **Preflight result**: The stdout/exit-status of a single `go list all` invocation, keyed by the directory it was run from. A preflight is required before `go mod why -m` verdicts from the same directory can be trusted (`go mod why` returns silent false-not-needed verdicts if module resolution fails).
- **`go mod why -m` chunk**: A single subprocess invocation querying a subset of module paths against a specific main-module's build graph. Chunk size is bounded by the argv-length guard.
- **Wall-clock budget**: A single `Arc<AtomicU64>` tracking elapsed time across every subprocess invocation in the classifier. Shared across concurrent workspaces per FR-004.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the Kubernetes fixture (`kusari-sandbox/test-kubernetes`, 39 main-modules, 246 root-module `go.sum` entries), a default scan completes in ≤ **10 seconds** wall-time on a reference class of machine (macOS aarch64, ≥ 8 logical CPUs, warm cache). Interim milestones: US1 alone ≤ 30 s; US1 + US2 ≤ 15 s; all three ≤ 10 s.
- **SC-002**: On the same Kubernetes fixture, every detected main-module MUST be analyzed (i.e., zero workspaces skipped due to `budget-exhausted`). The FR-013 summary log MUST show an `analyzed=` count equal to or greater than the v0.6.1 baseline (`analyzed=421`).
- **SC-003**: On every existing Go fixture in `waybill-cli/tests/fixtures/golang/`, the emitted CDX / SPDX 2.3 / SPDX 3 outputs are byte-identical to the v0.6.1 goldens after the version-string cascade is masked (per the release-bump normalized-diff protocol in memory `feedback_verify_golden_churn_normalized`).
- **SC-004**: The milestone introduces zero new Cargo dependencies at the workspace `Cargo.lock` level.
- **SC-005**: Every warn/info log line emitted by the classifier during a concurrent-execution scan carries the workspace's absolute path in a structured `main_module` field. Verified by grepping the emitted log for a scan against the Kubernetes fixture.
- **SC-006**: The `--no-go-mod-why` flag, when set, produces byte-identical CDX / SPDX 2.3 / SPDX 3 output vs. the v0.6.1 pre-milestone binary against every Go fixture in `waybill-cli/tests/fixtures/golang/`. Regression pin for FR-010.

## Assumptions

- Reference-class benchmark host is macOS aarch64 with ≥ 8 logical CPUs and a warm module cache. Other host classes (older Linux CI runners with 2-4 vCPUs, Windows hosts) benefit proportionally but their exact wall-times are not pinned by SC-001.
- The `kusari-sandbox/test-kubernetes` fixture (v0.6.1 shape: 39 `go.mod` files, `go.work` present, 246 modules in root `go.sum`) is the canonical performance regression corpus for this milestone. Public reproducibility uses `git clone --depth 1`.
- The 60-second shared wall-clock budget is sufficient for the Kubernetes fixture once the three tiers ship. Larger Go monorepos (hypothetical Kubernetes-of-Kubernetes) MAY still hit the budget; changing the budget default is a separate milestone.
- The operator's `go` toolchain supports `go.work` (Go ≥ 1.18). Older toolchains fall back to the m231 `WorkspaceMode::Off` path per FR-014.
- Empirical `go mod why -m` invocations against Kubernetes-scale module lists succeed within the argv-length guard at chunk size 500. If future evidence shows individual invocations approaching ARG_MAX at that size, the chunk size is retuned in a follow-up patch; the guard itself is defensive and does not change.
- Concurrent `go` subprocesses do not require serialization on shared filesystem state. The Go toolchain's module cache is designed for concurrent readers (multiple `go build` processes routinely run concurrently in CI systems).
- Every existing `waybill-cli/tests/fixtures/golang/` fixture is small enough that the milestone's changes are byte-identical for it (SC-003 will confirm empirically at implementation time).

## Non-Goals

- No change to `go mod why` output-parsing semantics (`parse_go_mod_why`).
- No change to verdict-classification rules.
- No change to the `--no-go-mod-why` operator flag or `WAYBILL_NO_GO_MOD_WHY` env var.
- No change to the shared 60-second wall-clock budget default. The goal is to fit under the budget, not to raise it.
- No change to the FR-013 summary log's wire shape.
- No change to the `WorkspaceMode` enum's variants or the `detect_workspace_mode` behavior. This milestone extends the enum's use, not its semantics.
- No introduction of new operator-facing flags. Every improvement is default-on.
- No cross-scan caching. All state is per-scan, matching every reader-tier milestone since m002.

## Dependencies

- The milestone builds on m112 (`go mod why` build-inclusion classification) and m231 (workspace-mode detection). Neither is modified — this milestone extends how they are invoked.
- The milestone is compatible with the m669 benchmark harness. New per-milestone timings will surface in the existing `xtask bench` output.
