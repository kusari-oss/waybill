# Data Model: Helm `--helm-render` Subprocess Implementation

**Date**: 2026-07-17
**Purpose**: Document the 1 new enum + 2 new helper functions + the extended branch in `helm::read`. No new wire-format constructs — m203 activates the internal `HelmExtractionMode::Rendered` variant that m188 already defined.

## E1: `HelmRenderError` enum (NEW)

**Location**: `mikebom-cli/src/scan_fs/package_db/helm.rs` (new type, adjacent to existing `HelmRenderMode`).

**Shape**:

```rust
#[derive(Debug, thiserror::Error)]
pub(super) enum HelmRenderError {
    #[error("`helm` binary not found on $PATH")]
    BinaryNotFound,

    #[error("`helm template` exited with code {code}; stderr head: {stderr_head}")]
    NonZeroExit { code: i32, stderr_head: String },

    #[error("`helm template` exceeded {timeout_secs}s timeout")]
    Timeout { timeout_secs: u64 },

    #[error("`helm template` I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
```

**Variant semantics**:

| Variant | Trigger | Payload | Fallback |
|---|---|---|---|
| `BinaryNotFound` | `Command::new("helm").arg("version").output()` returns `ErrorKind::NotFound` | (none) | → unrendered + WARN |
| `NonZeroExit` | Subprocess exits with non-zero status | `code`: exit status (or `-1` if unset); `stderr_head`: first 20 lines of stderr per m188 FR-018 | → unrendered + WARN including `stderr_head` |
| `Timeout` | `mpsc::Receiver::recv_timeout` returns `Err(Timeout)` | `timeout_secs`: the effective timeout value | → unrendered + WARN |
| `IoError` | Any other `std::io::Error` from subprocess I/O (spawn failure, channel send failure, etc.) | Underlying `io::Error` | → unrendered + WARN |

**Validation rules**:
- Every variant is safe to `format!()` via the `Display` impl — no panics.
- `stderr_head` truncated to 20 lines pre-construction (secrets-guard per m188 FR-018).
- Not `pub` — internal-only enum, mikebom's public API surface unchanged.

## E2: `resolve_render_timeout() -> Duration` (NEW helper)

**Location**: `mikebom-cli/src/scan_fs/package_db/helm.rs` (new function, adjacent to `HelmRenderError`).

**Signature**: `fn resolve_render_timeout() -> Duration`

**Behavior**:
1. Read `MIKEBOM_HELM_RENDER_TIMEOUT_SECS` env var.
2. Parse as `u64`. On parse failure, ignore the value.
3. Clamp to `[1, 3600]` seconds.
4. On absent env var or parse failure, default 60.
5. Return `Duration::from_secs(secs)`.

**Validation rules**:
- Total function is 4-8 lines including the const declarations.
- Silent parse-failure handling (no `Result` return type) — matches m173/m089 env-var handling posture.
- Bounds `[1, 3600]` prevent both "never times out" (`u64::MAX`) and "0-second timeout" (unusable) footguns.

## E3: `extract_image_refs_rendered(chart_dir, timeout) -> Result<Vec<ImageRef>, HelmRenderError>` (NEW)

**Location**: `mikebom-cli/src/scan_fs/package_db/helm.rs` (new function, sibling to existing `extract_image_refs_unrendered`).

**Signature**: `pub(super) fn extract_image_refs_rendered(chart_dir: &Path, timeout: Duration) -> Result<Vec<ImageRef>, HelmRenderError>`

**Body structure** (per research R1's verbatim m055 pattern):

1. **Probe** `helm` availability via `Command::new("helm").arg("version").arg("--short").output()`.
2. **Spawn** worker thread that runs `Command::new("helm").args(["template", chart_dir]).output()` and sends the result via `mpsc::channel`.
3. **Wait** with `rx.recv_timeout(timeout)`.
4. **Classify** the result into `HelmRenderError` variants OR `Ok(image_refs)`:
   - Timeout channel error → `Err(Timeout { timeout_secs })`.
   - Subprocess I/O error → `Err(IoError(e))`.
   - Non-zero exit → `Err(NonZeroExit { code, stderr_head })`.
   - Success + zero exit → apply existing `IMAGE_REGEX` to `output.stdout`, return `Ok(refs)`.

**Reused helpers** (both existing in helm.rs):
- `IMAGE_REGEX` — the regex m188 defined for image-ref extraction from YAML.
- `cap_stderr_lines(bytes, n)` OR equivalent inline helper — 20-line cap for the `stderr_head` field. If this helper doesn't already exist, m203 adds it as a small utility function.

**Post-condition on success**: returned `Vec<ImageRef>` is deduplicated + sorted per the m188 unrendered path's convention. The rendered stdout produces the same `ImageRef` structural shape as the unrendered path — no wire-format contract change.

## E4: `helm::read` branch extension (MODIFIED)

**Location**: `mikebom-cli/src/scan_fs/package_db/helm.rs:282+` (`read()` function).

**Pre-m203 state** (line 298-301):

```rust
// Phase B — line-based image-ref extraction from templates + crds.
// US3 (`HelmRenderMode::OptIn`) is a follow-up; for now, always
// unrendered.
let _ = render_mode; // future US3 hook
let image_refs = extract_image_refs_unrendered(rootfs);
```

**Post-m203 state**:

```rust
// Phase B or C — line-based OR rendered image-ref extraction, per
// m188 contracts §Phase C flow diagram. HelmRenderMode::Off + all
// HelmRenderError variants fall back to Phase B (unrendered).
let (image_refs, extraction_mode) = match render_mode {
    HelmRenderMode::OptIn => {
        let timeout = resolve_render_timeout();
        match extract_image_refs_rendered(rootfs, timeout) {
            Ok(refs) => (refs, HelmExtractionMode::Rendered),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    chart_dir = %rootfs.display(),
                    "helm-render failed; falling back to unrendered extraction"
                );
                (extract_image_refs_unrendered(rootfs), HelmExtractionMode::Unrendered)
            }
        }
    }
    HelmRenderMode::Off => {
        (extract_image_refs_unrendered(rootfs), HelmExtractionMode::Unrendered)
    }
};
```

Then the existing `diagnostics.helm_extraction_mode = Some(HelmExtractionMode::Unrendered);` line becomes:

```rust
diagnostics.helm_extraction_mode = Some(extraction_mode);
```

**Change semantics**:
- Default `HelmRenderMode::Off` path: byte-identical to pre-m203 (same call to `extract_image_refs_unrendered`, same diagnostic value).
- `HelmRenderMode::OptIn` path with successful subprocess: NEW code path — sets `Rendered` mode + returns rendered refs.
- `HelmRenderMode::OptIn` path with ANY error class: falls back to `extract_image_refs_unrendered` + `Unrendered` mode + WARN log. Diagnostic value ends up SAME as `Off` path.

## Cross-cutting: FR-009 non-Helm scan byte-identity

**Guarantee**: `helm::read` at helm.rs:282+ is only invoked when the rootfs has `Chart.yaml` at its top level (verified at line 288). Non-Helm scans skip the entire helm reader — including m203's new subprocess code.

**Consequence**: Every existing non-Helm-touching golden test's output is byte-identical pre-m203 vs post-m203. The `extract_image_refs_rendered` function is never called; the branch never fires.

**Enforcement**: FR-009 verified at implement time via `git diff --stat mikebom-cli/tests/fixtures/` — expected zero drift on non-Helm goldens.
