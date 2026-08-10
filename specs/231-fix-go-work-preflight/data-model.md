# Phase 1 Data Model: Fix `go list all` preflight failure in Go workspace mode

**Feature**: 231-fix-go-work-preflight
**Date**: 2026-08-09

Everything below is scoped to one new enum and one new helper. No changes to existing types.

## New enum: `WorkspaceMode`

Encodes the workspace-active state for a single main-module directory. Lives in `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs` as a module-private type.

| Variant | Meaning | Effect on `GOFLAGS` |
|---|---|---|
| `Off` | `GOWORK=off` in caller env. Workspace mode explicitly disabled by operator. | `-mod=mod` retained (pre-231 behavior). |
| `Inactive` | `GOWORK=auto` or unset AND no `go.work` found via ancestor walk. Non-workspace project. | `-mod=mod` retained (pre-231 behavior; FR-003 byte-parity guarantee). |
| `Active(PathBuf)` | `GOWORK=auto` or unset AND `go.work` found. Path is the discovered file's absolute path (for diagnostic logging). | `-mod=mod` omitted — inherits Go's workspace default `-mod=readonly`. |
| `Explicit(PathBuf)` | `GOWORK` points to a specific `go.work` file. Same effect as `Active`; distinct variant for logging clarity. | `-mod=mod` omitted. |

The enum is not `pub`; it's an implementation detail of `mod_why.rs`.

## New helper: `detect_workspace_mode`

```rust
fn detect_workspace_mode(main_module_dir: &Path) -> WorkspaceMode
```

Determines the workspace state for a single main-module. Called from `apply_offline_env`'s new caller side (which now takes `main_module_dir: &Path` as an additional parameter).

Logic:

1. Read `GOWORK` from `std::env::var("GOWORK")`:
   - `"off"` (case-insensitive after `to_lowercase()`) → return `Off`.
   - `""` (empty) or unset (`Err(_)`) or `"auto"` → fall through to on-disk detection.
   - Any other value → treat as an explicit path; `Path::new(&val).is_file()` → return `Explicit(canonicalize'd path)`. If the file doesn't exist, fall through to on-disk detection (Go's own behavior: an invalid `GOWORK` path fails the build, but for waybill's preflight purposes we degrade to inactive rather than propagating the error — the actual `go list all` invocation will surface any real error via the existing warn-and-skip path).
2. On-disk detection: walk up from `main_module_dir` looking for `<ancestor>/go.work`:
   - Start at `main_module_dir`.
   - At each level, check `<level>/go.work` via `Path::is_file`.
   - If found, return `Active(<level>/go.work canonicalized)`.
   - Otherwise, move to `<level>.parent()`.
   - Stop when `.parent()` returns `None` (filesystem root reached).
3. If nothing found, return `Inactive`.

Validation rules:
- The walk MUST be unbounded (up to the filesystem root). No clamping to scan root, per research §R2.
- No IO errors propagated to the caller — a `fs::metadata` failure at any level is treated as "no `go.work` here, keep walking." The eventual `go list all` invocation is the source of truth on any filesystem-actual failure.

## Modified helper: `apply_offline_env`

**Existing signature** (`mod_why.rs:134`):

```rust
fn apply_offline_env(cmd: &mut Command, offline: bool)
```

**New signature**:

```rust
fn apply_offline_env(cmd: &mut Command, offline: bool, workspace_mode: &WorkspaceMode)
```

Behavior:
- If `!offline`: no-op (unchanged).
- If `offline` + `workspace_mode` in `{Off, Inactive}`: set `GOPROXY=off`, `GOFLAGS=-mod=mod`, `GOTOOLCHAIN=local` (pre-231 verbatim).
- If `offline` + `workspace_mode` in `{Active(_), Explicit(_)}`: set `GOPROXY=off`, `GOTOOLCHAIN=local`; **do NOT set `GOFLAGS`** (or explicitly set `GOFLAGS=-mod=readonly` — semantically equivalent since that's Go's workspace default; the plan picks "do NOT set" to keep the child-process env minimal).

## Call-site update: `run_bounded`

The existing `run_bounded` function at `mod_why.rs:154` currently calls `apply_offline_env(&mut cmd, offline)`. It needs to pass the workspace mode too. Since it already receives `cwd: &Path` (which IS the main-module directory for the preflight invocation), the change is:

```rust
let workspace_mode = detect_workspace_mode(&cwd);
apply_offline_env(&mut cmd, offline, &workspace_mode);
```

Detection runs once per subprocess invocation — cheap (single-digit-ms stdlib calls) and correct (each main-module gets its own detection, matching Go's own semantics).

## Counter for FR-006: workspace-active count

`MainModuleAnalysis` (existing struct in `mod_why.rs`, referenced at line 187) already tracks per-module diagnostic state. This milestone adds one field:

```rust
pub struct MainModuleAnalysis {
    // ... existing fields ...
    /// Milestone 231: workspace mode detected for this main-module's
    /// preflight. Feeds the FR-006 diagnostic counter.
    pub workspace_active: bool,
}
```

`analyze_main_module` sets `workspace_active` to `matches!(workspace_mode, WorkspaceMode::Active(_) | WorkspaceMode::Explicit(_))` immediately after detection. The scan-level aggregation loop (existing code that emits the `INFO: go-mod-why classification:` log line) sums this across all analyses and adds the count to the log line's key-value pairs.

## Out-of-scope

- Extending workspace-mode detection to `go_mod_graph.rs`. Research §R3 confirmed that file doesn't have the bug.
- Adding a CLI flag to override workspace-mode detection. `GOWORK` (the Go toolchain's own env var) is already the industry-standard override.
- Caching detection results across sibling modules. Research §R2 rejected as premature optimization.
