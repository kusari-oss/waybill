# Phase 0 Research: Fix `go list all` preflight failure in Go workspace mode

**Feature**: 231-fix-go-work-preflight
**Date**: 2026-08-09

All decisions grounded in code that already exists in the repo. No open NEEDS CLARIFICATION items — the one clarification session (`spec.md § Clarifications`) confirmed the strict-A behavior on the retry-fallback question.

## R1 — Is the bug scoped only to offline mode?

**Decision**: **Yes.** The bug only manifests when `apply_offline_env(offline=true)` runs. In non-offline mode, `mod_why.rs::run_bounded` does NOT set `GOFLAGS`, so Go inherits the process's default (typically unset → workspace default `-mod=readonly`). Only the offline path forces `-mod=mod`, which triggers the workspace-mode rejection.

**Evidence**: `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:134-140` guards every env-setter behind `if offline`. Line 137 is the specific offender.

**Rationale**: Narrows the fix surface. The change is scoped to `apply_offline_env`; the online path stays untouched. Also means the reporter's Grafana scan (`--offline`) hit the failure directly; a Grafana scan without `--offline` would have avoided the bug (though it would depend on `deps.dev` for enrichment).

**Alternatives considered**:
- *Broaden the fix to consult workspace mode in every subprocess invocation, offline or not.* Rejected: no failure signal exists in non-offline mode; adding detection cost with no observable benefit. If a future non-offline invocation ever grows a `GOFLAGS=-mod=mod` setter, that fix belongs to that milestone.

## R2 — Ancestor-walk boundary for `go.work` detection

**Decision**: Walk up from the main-module directory unbounded (up to the filesystem root or `/`). Do NOT clamp to the scan root.

**Rationale**: Mirrors Go's own toolchain behavior. `go build`, `go test`, `go list all` all walk up from `$PWD` searching for `go.work` without respecting any waybill-imposed boundary. If waybill clamped to the scan root but Go didn't, the two would disagree on workspace-active state: waybill would run `go list all` with `-mod=mod`, Go would still activate workspace mode from a `go.work` above the scan root, and the preflight would fail exactly as it does today.

The walk terminates on the first `go.work` found OR at the filesystem root. Termination-by-filesystem-root is a safe upper bound — no infinite walk possible.

**Alternatives considered**:
- *Clamp the walk to the scan root.* Rejected per above.
- *Cache detection results across sibling modules under the same workspace.* Rejected: complexity for negligible gain. Grafana has ~20 modules under one `go.work`; the walk cost per module is <1 ms of stdlib `fs::metadata` calls. Optimizing this is premature.

## R3 — Does `go_mod_graph.rs` (sibling subprocess runner) have the same bug?

**Decision**: **No.** Verified by grep — `go_mod_graph.rs` does NOT call `.env()` on its `Command`. It inherits the caller's env, so no forced `-mod=mod`. Workspace-mode invocations of `go mod graph` succeed naturally.

**Rationale**: Confirms this milestone is a single-file fix. `mod_why.rs` is the only offender. Follow-up milestones that add new `go`-subprocess call sites should audit for `-mod=mod` setters at introduction time — best captured as a code-review checklist item, not a spec deliverable.

**Alternatives considered**: N/A — the empirical answer is unambiguous.

## R4 — Synthetic workspace fixture shape (for SC-001)

**Decision**: Two nested modules under a workspace root, sharing one runtime dependency and one test-only dependency. Concretely:

```text
waybill-cli/tests/fixtures/golden_inputs/golang/workspace_mode/
├── go.work                  # go 1.22 + use ./module-a + use ./module-b
├── module-a/
│   ├── go.mod               # example.com/mikebomfixture/a; requires shared@v1
│   ├── go.sum               # populated for offline mode
│   └── main.go              # imports the shared runtime dep
└── module-b/
    ├── go.mod               # example.com/mikebomfixture/b; requires shared@v1 + test-only@v1
    ├── go.sum
    └── lib.go               # imports both deps; test-only guarded by _test.go
```

Fixture module paths use `example.com/mikebomfixture/*` synthetic names per memory `feedback_fixture_synthetic_package_names`.

**Rationale**: This shape exercises FR-001 (workspace detected), FR-002 (preflight succeeds without `-mod=mod`), FR-004 (`go mod why` produces a mix of `prod` and `test` verdicts), FR-006 (workspace-active counter increments). Two modules is the minimum that meaningfully tests workspace mode (a single-module workspace is degenerate — Go treats it identically to non-workspace). Adding a third module wouldn't add coverage.

**Alternatives considered**:
- *Vendor a real minimal open-source Go workspace repo as the fixture.* Rejected: memory `feedback_fixture_synthetic_package_names` explicitly forbids real coordinates; also unclear which repo to pick.
- *Have the fixture require an actual `go` toolchain at test time.* Deferred: the integration test invokes `waybill sbom scan` which internally shells out to `go` (per FR-005, missing `go` → warn-and-skip). CI runners have Go installed. Local dev may not — that's why the unit tests for workspace detection are separate from the integration test; the unit tests exercise the detector in isolation without needing `go`.

## R5 — Test infrastructure reuse

**Decision**: Reuse the existing `waybill-cli/tests/scan_nuget.rs` subprocess pattern verbatim for the integration test. Same `common::bin()`, `apply_fake_home_env`, `Command::new(bin())` scaffold at `waybill-cli/tests/nuget_main_module_parity.rs` (m230's precedent).

**Rationale**: The pattern is proven, cross-platform (Linux/macOS/Windows), and self-contained (no `common::` extensions needed). Zero new test-infrastructure code needed.

**Alternatives considered**:
- *Test `detect_workspace_mode` in isolation without an end-to-end scan.* Included as a unit test; but the integration test is still needed to prove the child-process env is set correctly and that Go actually accepts the invocation. Both together give confidence.

## R6 — Type-safety for workspace-mode state (Principle IV)

**Decision**: Introduce a small enum in `mod_why.rs`:

```rust
enum WorkspaceMode {
    /// GOWORK=off — workspace mode explicitly disabled.
    Off,
    /// GOWORK=auto or unset AND no go.work on disk.
    Inactive,
    /// GOWORK=auto or unset AND go.work found via ancestor walk.
    /// The variant carries the path for diagnostic logging.
    Active(PathBuf),
    /// GOWORK=<explicit-path> — explicit override.
    Explicit(PathBuf),
}
```

`apply_offline_env` matches on this enum and sets `GOFLAGS` conditionally.

**Rationale**: Type-driven — encodes every possible workspace state; exhaustive matching prevents "we forgot to handle GOWORK=auto with go.work missing" bugs. Aligns with Constitution Principle IV.

**Alternatives considered**:
- *Boolean `workspace_active: bool`.* Rejected: loses the diagnostic value of the enum's path variants; also can't distinguish `GOWORK=off` from `Inactive` (which matters for logging the operator's explicit choice).
- *Full-fidelity mirror of Go's own `runtime` workspace state machine.* Overkill for the fix surface.
