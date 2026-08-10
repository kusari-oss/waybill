# Feature Specification: Fix `go list all` preflight failure in Go workspace mode

**Feature Branch**: `231-fix-go-work-preflight`
**Created**: 2026-08-09
**Status**: Draft
**Input**: User description: "golang reader: fix go.work workspace-mode preflight failure — currently 469 modules in Grafana workspace scan degrade to build-inclusion: unknown because go list all is invoked with -mod=mod which workspace mode rejects. Detect a go.work file in the module's ancestor chain; when present, invoke go list all WITHOUT -mod=mod so the workspace's default -mod=readonly applies. Add regression test using a minimal workspace fixture. Verify against Grafana: unknown_marked should drop from 469 to 0 (or near-0)." (Closes #671.)

## Background

The Go reader's `mod_why` preflight (`waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`) currently runs `go list all` inside a child process with `GOFLAGS=-mod=mod` (line 137). Go rejects `-mod=mod` whenever workspace mode is active — that is, whenever a `go.work` file exists in the target module's ancestor chain — and returns:

```
go: -mod may only be set to readonly or vendor when in workspace mode,
but it is set to "mod"
```

The preflight fails, `go mod why` is skipped for every module in the workspace, and the build-inclusion classifier falls back to marking every Go component `waybill:build-inclusion: unknown` instead of producing the `prod` / `test` / `not-needed` classification CISA 2026 downstream consumers rely on.

Verified end-to-end scan of `github.com/grafana/grafana` (2026-08-07, waybill built from `229-release-flow-impl`): **469 Go modules degraded to `unknown`.** That's the entire Grafana dependency graph losing its build-inclusion signal — a real CISA 2026 quality regression for every workspace-mode Go project (which includes every project that opted into the workspace pattern for local development or polyrepo migration).

## Clarifications

### Session 2026-08-09

- Q: When the fix (detect + drop `-mod=mod`) is applied but `go list all` still fails, should the reader retry with `GOWORK=off`? → A: No — strict-A. Respect operator intent; emit the WARN path (FR-005) and let operators investigate. Rationale: silently forcing `GOWORK=off` produces `go list all` output that doesn't match what the operator sees running the same command themselves. The m228 release-flow survey established "respect operator's build configuration" as project posture. Option C (retry-with-GOWORK=off) was explicitly considered and rejected.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Grafana-shape workspace scan preserves build-inclusion classification (Priority: P1)

A developer or CI system scans a Go project that has a `go.work` file at the repository root (multi-module workspace). The scan completes without emitting any `go-mod-why analysis skipped (unresolvable-packages)` warnings, and every Go component in the emitted SBOM carries a definitive `waybill:build-inclusion` value (`prod`, `test`, `not-needed`, or `unresolved` — never the generic `unknown` fallback that indicates the preflight failed silently).

**Why this priority**: This is the exact failure the reporter surfaced and the only failure mode this milestone is scoped to close. Fixing it restores build-inclusion signal to the largest class of real-world Go projects currently affected — any workspace-mode project, single-module workspace projects included.

**Independent Test**: Scan a minimal workspace fixture (two nested modules + a shared dependency + `go.work` at the root). Assert (a) the scan emits zero `go-mod-why analysis skipped` warnings; (b) every Go component in the emitted SBOM has a `waybill:build-inclusion` value that is NOT the generic fallback; (c) at least one module gets a definitive `prod` or `test` classification.

**Acceptance Scenarios**:

1. **Given** a Go project with a `go.work` file listing modules `./a` and `./b`, plus a shared runtime dependency `example.com/shared`, **When** `waybill sbom scan` runs against the project root, **Then** the emitted SBOM contains a `pkg:golang/example.com/shared@<version>` component whose annotations include `waybill:build-inclusion: prod` (not `unknown`).
2. **Given** the same project, **When** the scan finishes, **Then** stderr / trace output contains zero `go-mod-why analysis skipped (unresolvable-packages)` warnings AND the diagnostic summary reports `unknown_marked` of 0 (or a small residual — see Assumptions).
3. **Given** a Go project WITHOUT a `go.work` file (single-module), **When** the scan runs, **Then** existing behavior is preserved verbatim — same command invocation, same `GOFLAGS=-mod=mod` env, same classifier output. No regressions on the pre-231 single-module code path.
4. **Given** the Grafana repository at HEAD (real-world reference target — 20+ modules in `go.work`), **When** waybill scans it with the milestone-231 binary, **Then** the diagnostic summary reports `unknown_marked` of 0 (allowing a small residual per Assumptions), a dramatic drop from the 469 observed pre-fix.

---

### Edge Cases

- **`go.work` present but empty or malformed**: The Go toolchain still activates workspace mode even for a nearly-empty `go.work`. The fix must detect `go.work` by presence, not by content validity. If the file is malformed, `go list all` itself will fail with a different error — that failure preserves the existing "skip with warning" behavior; no regression.
- **`go.work` in an ancestor OUTSIDE the scan root**: A developer may run waybill against a subdirectory of a larger workspace repo. Go's `go.work` discovery walks up the filesystem, not just the scan root. The detection MUST mirror the Go toolchain's behavior — walk up from each Go-module directory until either a `go.work` or the filesystem root is reached. Bounded by the scan root when the scan root is above the module; unbounded otherwise (mirrors the Go compiler's own behavior).
- **`GOWORK` environment variable override**: A user may set `GOWORK=off` in the shell before invoking waybill. In that case workspace mode is off regardless of any `go.work` on disk. The fix should respect the environment variable — if `GOWORK=off` is set in the caller's env, keep the pre-231 behavior (retain `-mod=mod`). If unset or set to `auto`, do the on-disk detection. If pointed at a specific `go.work` path, treat that as workspace-active.
- **Multiple concurrent `go list all` invocations across nested modules**: If a workspace has both an outer `go.work` and a nested module that could be scanned independently, the preflight is called once per main-module. Each invocation independently detects its own workspace state — no cross-invocation coordination needed.
- **`go` binary not present on `$PATH`**: Existing behavior — the preflight fails with a different `command not found` diagnostic; the classifier falls back to `unknown`. Unchanged by this milestone.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Before shelling out to `go list all`, the reader MUST detect whether Go workspace mode is active for the target module. Detection MUST match the Go toolchain's behavior: walk up from the module's directory looking for a `go.work` file; also honor a `GOWORK` env var set in the caller's environment (`off` disables detection; `auto` or unset defers to on-disk detection; an explicit path treats that path as the active workspace).
- **FR-002**: When workspace mode is active for a target module, the reader MUST invoke `go list all` WITHOUT setting `GOFLAGS=-mod=mod` (either by omitting the env var entirely for that child process OR by setting it to an empty value / a workspace-compatible value like `-mod=readonly`). Go's workspace default is `-mod=readonly`, which the fix inherits when the flag is not overridden.
- **FR-003**: When workspace mode is NOT active for a target module, the reader MUST preserve the pre-231 behavior verbatim — same `GOFLAGS=-mod=mod` env, same argv, same child-process configuration. Single-module Go projects (the historical majority case) MUST see zero behavior change.
- **FR-004**: When `go list all` succeeds for a workspace-mode module, the reader MUST run `go mod why` for every Go component in that module's dependency graph and produce definitive `prod` / `test` / `not-needed` / `unresolved` classifications, NOT the generic `unknown` fallback.
- **FR-005**: When `go list all` still fails after the FR-002 fix (e.g., malformed `go.work`, missing `go` binary, network-required-but-offline), the reader MUST preserve the existing warn-and-skip behavior — emit the existing `go-mod-why analysis skipped` WARN log, fall back to `unknown` markers, and continue scanning.
- **FR-006**: The reader MUST log a single INFO-level diagnostic per scan indicating how many Go modules the preflight ran against and how many of those had workspace mode active. The counter matches the shape of the existing `build-inclusion pass` INFO log so operators can correlate the two.

### Key Entities

- **Go module (main-module scope)**: A directory containing a `go.mod` file. Each main-module gets its own `go list all` preflight invocation.
- **Go workspace (`go.work`)**: A file at any ancestor of a Go module that switches the toolchain into workspace mode. Its presence toggles the toolchain's `-mod` default from `mod` to `readonly` and rejects `-mod=mod`.
- **Preflight classification bucket**: Each Go module's build-inclusion classification result — one of `prod`, `test`, `not-needed`, `unresolved`, or `unknown`. The last (`unknown`) is the fallback that this milestone is specifically avoiding.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a synthetic workspace fixture (a `go.work` at the root + 2 nested Go modules + a shared runtime dependency), `waybill sbom scan` produces zero `go-mod-why analysis skipped (unresolvable-packages)` warnings, and the emitted SBOM's `waybill:build-inclusion` values for Go components are non-`unknown` on ≥1 component. Measured by scanning the fixture and grepping stderr / annotations.
- **SC-002**: On the Grafana repository at HEAD (`github.com/grafana/grafana`, 20+ modules in `go.work`), a milestone-231 scan reports `unknown_marked` of 0 in the `build-inclusion pass` INFO summary line (a small residual — see Assumptions — is acceptable but must be documented). Pre-fix baseline: 469. Measured by parsing the diagnostic summary from the scan's stderr.
- **SC-003**: On a single-module Go project (no `go.work` present), a milestone-231 scan produces byte-identical `waybill:build-inclusion` output compared to the pre-231 baseline for the same project. Measured by diffing the emitted SBOM (with content-addressed IDs and timestamps masked per `feedback_verify_golden_churn_normalized`) across the two builds.
- **SC-004**: The `go-mod-why classification: analyzed=N ...` INFO summary line reports `analyzed` ≥ 1 (i.e., the preflight actually ran the analysis rather than skipping) on every workspace-mode scan of the synthetic fixture. Pre-fix, this line reports `analyzed=0 ... skipped=unresolvable-packages`.
- **SC-005**: When `GOWORK=off` is set in the caller's environment while scanning a project that has a `go.work` file on disk, the reader falls back to the pre-231 behavior (invokes with `-mod=mod`), preserving the operator's explicit override. Measured by scanning the synthetic fixture twice — once with `GOWORK` unset, once with `GOWORK=off` — and asserting the two invocations produce different child-process env values (via a hook or inspection point) but both complete successfully.

## Assumptions

- The Go toolchain is available at `go` on `$PATH` during the scan. This is a pre-existing assumption of the golang reader; not new to milestone 231.
- A small residual of `unknown` markers may persist even post-fix due to unrelated causes: dependencies that Go can't resolve (renamed modules, deleted proxy entries, replaced modules pointing to missing paths). SC-002 allows a small residual as long as the number is a dramatic drop from 469 (e.g., ≤ 5 rather than ≥ 100).
- The synthetic workspace fixture (SC-001) uses `MikebomFixture.*`-style synthetic module names per memory `feedback_fixture_synthetic_package_names` — no real-world Go module paths (avoiding Kusari Inspector advisory-scan collisions).
- The Grafana verification (SC-002) is a manual step performed once by the implementer; it is not automated into CI because pulling Grafana at HEAD is out of scope for the corpus policy in memory `feedback_release_process`. The CI regression signal is the synthetic-fixture test from SC-001.
- Workspace detection walking up from the module directory does NOT need to respect the scan root's boundary. Go's own toolchain walks up unbounded — waybill mirrors that. Rationale: an operator scanning `~/repo/subdir` where `~/repo/go.work` exists still gets workspace mode from Go's perspective, so waybill must too.
- The existing `mod_why` module already computes the per-main-module directory needed for FR-001's ancestor walk. This milestone does NOT need to alter the module-discovery pass.
- The existing `INFO: build-inclusion pass:` and `INFO: go-mod-why classification:` diagnostic log lines are the correct place to add the FR-006 workspace-active counter — extending the existing log is preferred over adding a new one.
