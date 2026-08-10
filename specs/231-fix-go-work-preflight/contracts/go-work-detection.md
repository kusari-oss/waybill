# Contract: Go workspace-mode detection semantics

**Feature**: 231-fix-go-work-preflight
**Phase**: 1
**Audience**: Future contributors extending the golang reader or adjacent readers that shell out to `go`.

waybill's Go reader shells out to the Go toolchain in multiple call sites (`mod_why.rs::run_bounded`, `go_mod_graph.rs`, follow-up milestones). This contract records the workspace-detection semantics they MUST all agree on so `go list` / `go mod why` / `go mod graph` invocations behave identically across the reader.

## Detection algorithm

For a given main-module directory `D`, the workspace state is determined by (in strict order):

1. **`GOWORK` env var — explicit override**. Read `std::env::var("GOWORK")`:
   - `"off"` (case-insensitive) → **Workspace inactive**. Do not walk. Set `GOFLAGS=-mod=mod` as usual for offline mode.
   - Empty string or unset or `"auto"` → fall through to step 2.
   - Any other non-empty value → treat as an explicit path to a `go.work` file. If `Path::new(&value).is_file()` → **Workspace active** (path = canonicalize'd value). If the file doesn't exist → fall through to step 2 (waybill degrades to on-disk detection; the actual `go` invocation will surface any error via its own stderr).

2. **On-disk ancestor walk**. From `D`, walk `.parent()` up to the filesystem root:
   - At each level `L`, check `L/go.work` via `Path::is_file`.
   - First match → **Workspace active** (path = canonicalize'd `L/go.work`).
   - No match at any level → **Workspace inactive**.
   - The walk is UNBOUNDED. It does NOT clamp to the scan root or any user-supplied boundary. This matches Go's own toolchain behavior.

## Consequences of the workspace-active decision

When workspace is **active**, offline invocations of `go` subprocesses inside the golang reader MUST NOT set `GOFLAGS=-mod=mod`. Preferred: omit `GOFLAGS` entirely; Go's workspace default `-mod=readonly` applies. Alternative equivalent: explicitly set `GOFLAGS=-mod=readonly`.

Other offline-env pins (`GOPROXY=off`, `GOTOOLCHAIN=local`) remain unchanged — they're workspace-compatible.

When workspace is **inactive**, offline invocations MUST preserve pre-231 behavior verbatim: `GOFLAGS=-mod=mod`, `GOPROXY=off`, `GOTOOLCHAIN=local`. FR-003 byte-parity guarantee.

## Diagnostic surface

Every `analyze_main_module` invocation records `workspace_active: bool` in its `MainModuleAnalysis` output. The scan-level aggregation emits an INFO log line:

```
INFO: go-mod-why classification: analyzed=<N> prod=<A> test=<B> not_needed=<C> unresolved=<D> unknown_marked=<E> workspace_modules=<F> elapsed_ms=<T>
```

`workspace_modules` is the sum of `workspace_active` across every analyzed main-module. Operators reading the log can correlate this counter with `unknown_marked` — before m231, workspace_modules > 0 correlated 1:1 with `unknown_marked ≈ total`; post-m231, the correlation vanishes because workspace scans now produce definitive verdicts.

The pre-m231 WARN log at `mod_why.rs:209-217` (`go-mod-why analysis skipped (unresolvable-packages)`) remains unchanged. It still fires on genuine `go list all` failures (malformed `go.work`, missing `go` binary, etc.) per FR-005.

## Cross-invocation consistency

If a future milestone adds new `go` subprocess call sites in the golang reader (or anywhere else), those call sites MUST invoke `detect_workspace_mode` and pass the result to their own offline-env-pinning logic. This contract exists so the golang reader's Go-toolchain interaction stays coherent as it grows.

`go_mod_graph.rs` currently does NOT set `GOFLAGS` at all, so it doesn't need the fix today (research §R3). But if a future contributor adds `-mod=mod` to `go_mod_graph.rs`, they MUST plumb through `detect_workspace_mode` at the same time.

## Behavior invariants (test contract)

1. **Given** `GOWORK=off` in the caller env AND a `go.work` file exists in the module's ancestor chain, **When** detection runs, **Then** it returns `Off`. Preflight uses `-mod=mod`.
2. **Given** `GOWORK` unset AND no `go.work` in any ancestor, **When** detection runs, **Then** it returns `Inactive`. Preflight uses `-mod=mod`.
3. **Given** `GOWORK` unset AND `go.work` at the immediate parent of the module, **When** detection runs, **Then** it returns `Active(<parent>/go.work)`. Preflight omits `GOFLAGS`.
4. **Given** `GOWORK` unset AND `go.work` at an ancestor two levels up (module is nested), **When** detection runs, **Then** it returns `Active(<ancestor>/go.work)`. Preflight omits `GOFLAGS`.
5. **Given** `GOWORK=/path/to/real.go.work` (existing file), **When** detection runs, **Then** it returns `Explicit(/path/to/real.go.work)`. Preflight omits `GOFLAGS`.
6. **Given** `GOWORK=/path/to/nonexistent.go.work` AND `go.work` at an ancestor, **When** detection runs, **Then** it returns `Active(<ancestor>/go.work)` (fell through to on-disk detection).

Tests MUST assert each invariant by inspecting the returned `WorkspaceMode` variant (unit test) OR by inspecting the effective child-process env after `apply_offline_env` (integration test).
