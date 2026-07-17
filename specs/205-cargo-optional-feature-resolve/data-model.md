# Data Model: Fix Cargo Optional-Dep Over-Exclusion

**Date**: 2026-07-17
**Purpose**: Document the 1 new enum + 1 new helper function + 2 modified sites in `cargo.rs`. No new types beyond those; no wire-format changes; no parity-catalog changes.

## E1: `CargoMetadataResolveFailure` enum (NEW)

**Location**: `mikebom-cli/src/scan_fs/package_db/cargo.rs` (new type, adjacent to the existing `CargoError` enum).

**Shape**:

```rust
#[derive(Debug, thiserror::Error)]
pub(super) enum CargoMetadataResolveFailure {
    #[error("`cargo` binary not found on $PATH")]
    BinaryNotFound,

    #[error("`cargo metadata` exited with code {code}; stderr head: {stderr_head}")]
    NonZeroExit { code: i32, stderr_head: String },

    #[error("`cargo metadata` exceeded {timeout_secs}s timeout")]
    Timeout { timeout_secs: u64 },

    #[error("`cargo metadata` JSON parse failed: {source}")]
    ParseError { source: serde_json::Error },

    #[error("`cargo metadata` I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
```

**Variant semantics**:

| Variant | Trigger | Payload | Fallback |
|---|---|---|---|
| `BinaryNotFound` | `Command::new("cargo").arg("--version").output()` returns `ErrorKind::NotFound` | (none) | → all optional deps → Runtime + WARN |
| `NonZeroExit` | `cargo metadata` exits with non-zero status | `code`: exit status (or `-1` if unset); `stderr_head`: first 20 lines of stderr per m203 FR-018 precedent | → all optional deps → Runtime + WARN including `stderr_head` |
| `Timeout` | `mpsc::Receiver::recv_timeout` returns `Err(Timeout)` | `timeout_secs`: effective timeout | → all optional deps → Runtime + WARN |
| `ParseError` | `serde_json::from_slice` fails on stdout | `source`: `serde_json::Error` | → all optional deps → Runtime + WARN |
| `IoError` | Any other `std::io::Error` from subprocess I/O | Underlying `io::Error` | → all optional deps → Runtime + WARN |

**Validation rules**:
- Every variant is safe to `format!()` via the `Display` impl — no panics.
- `stderr_head` truncated to 20 lines pre-construction (secrets-guard per m203 precedent).
- Not `pub` — internal-only enum, mikebom's public API surface unchanged.

## E2: `resolve_render_timeout` helper (NEW — mirrors m203 exactly)

**Location**: `mikebom-cli/src/scan_fs/package_db/cargo.rs` (new function).

**Signature**: `fn resolve_cargo_metadata_timeout() -> Duration`

**Behavior** (verbatim m203 pattern):
1. Read `MIKEBOM_CARGO_METADATA_TIMEOUT_SECS` env var.
2. Parse as `u64`. On parse failure, ignore the value.
3. Clamp to `[1, 3600]` seconds.
4. On absent env var or parse failure, default 60.
5. Return `Duration::from_secs(secs)`.

## E3: `resolve_activated_deps_via_cargo_metadata` helper (NEW)

**Location**: `mikebom-cli/src/scan_fs/package_db/cargo.rs` (new function).

**Signature**: `pub(super) fn resolve_activated_deps_via_cargo_metadata(workspace_root: &Path, timeout: Duration) -> Result<HashSet<String>, CargoMetadataResolveFailure>`

**Body structure** (per research R1's cargo metadata shape + R3's m055 subprocess pattern):

1. **Probe** `cargo` availability via `Command::new("cargo").arg("--version").output()`. If `ErrorKind::NotFound`, return `Err(BinaryNotFound)`.
2. **Spawn** worker thread that runs:
   ```rust
   Command::new("cargo")
       .args(["metadata", "--format-version", "1", "--offline", "--locked"])
       .current_dir(workspace_root)
       .output()
   ```
   Sends result via `mpsc::channel`.

   **Flag rationale**:
   - `--offline` is REQUIRED per FR-006. Blocks cargo from reaching over the wire to update the registry index. Without it, a workspace whose Cargo.toml declares a dep whose version isn't in the local index cache would trigger a network fetch (violating FR-006 and slowing the scan). With it, cargo exits non-zero on any operation that would need network → FR-004 NonZeroExit fallback path fires cleanly.
   - `--locked` is REQUIRED for determinism (FR-007). Requires Cargo.lock to exist AND be up-to-date; forbids cargo from mutating it. Without it, cargo may rewrite Cargo.lock as a side effect of `cargo metadata` (a real Cargo behavior when the manifest has been touched since the lockfile was last generated), which would (a) mutate the scanned workspace under the operator's feet, and (b) produce non-deterministic scan output. With it, any lockfile drift triggers non-zero exit → FR-004 fallback.

   Both flags failing produce the same outcome (`NonZeroExit` with stderr_head describing the cargo-side reason), which the FR-004 fallback handles uniformly.
3. **Wait** with `rx.recv_timeout(timeout)`. Return `Err(Timeout)` on channel timeout.
4. **Classify** subprocess result:
   - `Err` I/O → `Err(IoError(e))`.
   - Non-zero exit → `Err(NonZeroExit { code, stderr_head })`.
   - Zero exit → parse stdout as JSON.
5. **Parse** JSON:
   - Navigate to `resolve.nodes[]` array.
   - For each node, iterate `deps[]`.
   - Insert each `deps[i].name` into a `HashSet<String>`.
   - Return `Ok(activated_names)`.
6. **Parse failure** at step 5 → `Err(ParseError { source })`.

**Post-condition on success**: returned `HashSet<String>` contains every dep NAME (Cargo package name) that appears in ANY workspace-member node's `deps[]` — i.e., the union of all activated deps across the resolved workspace.

## E4: Classifier delta at `parse_lockfile` (MODIFIED)

**Location**: `mikebom-cli/src/scan_fs/package_db/cargo.rs::parse_lockfile` (currently at line 1057+).

**Signature change**: add `activated_names: &HashSet<String>` as a new parameter (between the existing `optional_names` and `root_names` params for locality).

**Classifier check change** at `cargo.rs:1155`:

Pre-m205:
```rust
} else if prod_set.contains(&key) && optional_names.contains(&pkg.name) {
    entry.lifecycle_scope = Some(LifecycleScope::Optional);
    entry.extra_annotations.insert(
        "mikebom:optional-derivation".to_string(),
        serde_json::Value::String("cargo-optional-true".to_string()),
    );
```

Post-m205:
```rust
} else if prod_set.contains(&key)
    && optional_names.contains(&pkg.name)
    && !activated_names.contains(&pkg.name)
{
    // Milestone 205 (#593): dep is TRULY Optional iff declared
    // `optional = true` in some workspace manifest AND NOT activated
    // by the resolved feature set (per `cargo metadata --format-
    // version 1` resolve.nodes[].deps[]). When cargo metadata failed,
    // `activated_names` is empty — the classifier still gates on
    // `optional_names` alone, but the CALLER at line 1259 has already
    // WARNed and switched to safe-over-inclusion mode by populating
    // `activated_names` with all `optional_names` (making this branch
    // unreachable — every optional dep flips to Runtime).
    entry.lifecycle_scope = Some(LifecycleScope::Optional);
    entry.extra_annotations.insert(
        "mikebom:optional-derivation".to_string(),
        serde_json::Value::String("cargo-optional-true".to_string()),
    );
```

## E5: Caller wiring at `read` (MODIFIED)

**Location**: `mikebom-cli/src/scan_fs/package_db/cargo.rs::read` around line 1259.

**Pre-m205 state**:

```rust
let entries = parse_lockfile(
    &lock_path,
    &prod_set,
    &build_set,
    &workspace_sections.optional_deps,
    &workspace_sections.root_names,
)?;
```

**Post-m205 state**:

```rust
// Milestone 205 (#593): resolve Cargo's actual feature-activation
// set via `cargo metadata --format-version 1`. Fallback per FR-004:
// on any failure, populate `activated_names` with ALL manifest-
// declared optional deps → every one flips to Runtime (safe over-
// inclusion; vuln-scanners never miss shipped deps). WARN log
// naming the workspace + failure class.
let workspace_root = lock_path.parent().unwrap_or(&lock_path);
let timeout = resolve_cargo_metadata_timeout();
let activated_names: HashSet<String> = match resolve_activated_deps_via_cargo_metadata(
    workspace_root, timeout,
) {
    Ok(names) => names,
    Err(e) => {
        tracing::warn!(
            workspace = %workspace_root.display(),
            reason = %e,
            "cargo metadata failed; falling back to name-only optional \
             classification (safe over-inclusion — deps marked Runtime \
             instead of Optional so vuln-scanners never miss shipped deps)"
        );
        // Safe over-inclusion: populate with all optional names so the
        // classifier's `!activated_names.contains(&pkg.name)` check at
        // line 1155 always evaluates false → no dep reaches the
        // Optional branch → every one flows through to Runtime/Build/
        // Development classification per the pre-m179 default flow.
        workspace_sections.optional_deps.clone()
    }
};
let entries = parse_lockfile(
    &lock_path,
    &prod_set,
    &build_set,
    &workspace_sections.optional_deps,
    &activated_names,
    &workspace_sections.root_names,
)?;
```

**Change semantics**:
- Successful cargo metadata → `activated_names` contains actually-pulled-in dep names → classifier at E4 gates optional classification on `!activated_names.contains(&pkg.name)`.
- Failed cargo metadata → `activated_names = optional_deps.clone()` → classifier's `!activated_names.contains(&pkg.name)` always false → the Optional branch is dead → all optional deps flow to Runtime (or Build / Development per the manifest section they appear in).
- Non-Cargo scans do not enter the cargo reader → no cargo metadata invocation → no WARN log → byte-identity guaranteed.

## Cross-cutting: FR-005 non-Cargo scan byte-identity

**Guarantee**: `cargo::read` at `cargo.rs:1178+` is only invoked when the scan touches a Cargo workspace (Cargo.lock file detected). Non-Cargo scans skip the entire cargo reader — including m205's new resolver code.

**Consequence**: Every existing non-Cargo-touching golden test's output is byte-identical pre-m205 vs post-m205. `resolve_activated_deps_via_cargo_metadata` is never called; the WARN log never fires; the classifier check never runs.

**Enforcement**: FR-005 verified at implement time via `git diff --stat mikebom-cli/tests/fixtures/public_corpus/{go-cobra,npm-express,maven-guice,python-flask}/` — expected zero drift.
