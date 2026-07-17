# Research: Helm `--helm-render` Subprocess Implementation

**Date**: 2026-07-17
**Purpose**: Resolve 4 mechanical unknowns before task decomposition.

## R1 — Subprocess pattern (reuse m055 verbatim)

**Investigation**: `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs:81-158` implements the canonical mikebom subprocess-with-timeout pattern. Verified structure:

1. **Probe binary availability first** (line 90-101): `Command::new("go").arg("version").output()`. If `ErrorKind::NotFound`, return `StepResult::Unavailable` early. Cheap check — fails fast on missing-binary case without needing to spawn the real subprocess with piped I/O.
2. **Spawn subprocess in a worker thread** (line 119-127): `thread::spawn(move || tx.send(Command::new(...).output()))`. Main thread doesn't block on subprocess I/O; worker sends result on `mpsc::channel`.
3. **Main thread waits with timeout** (line 129+): `rx.recv_timeout(timeout)`. On `Err(_)` (channel timeout), return `StepResult::Failed { class: Timeout, ... }`. Worker thread + subprocess continue in background but get reaped eventually (documented per line 117-118 comment).

Same pattern reused by:
- m053 `git describe` (mikebom-cli/src/scan_fs/package_db/golang/legacy.rs)
- m173 warm-go-cache (parallel version)

**Decision**: `extract_image_refs_rendered(chart_dir, timeout) -> Result<Vec<ImageRef>, HelmRenderError>` follows the m055 pattern verbatim:

```rust
pub(super) fn extract_image_refs_rendered(
    chart_dir: &Path,
    timeout: Duration,
) -> Result<Vec<ImageRef>, HelmRenderError> {
    use std::process::Command;
    use std::sync::mpsc;
    use std::thread;

    // Probe: `helm version` — fails fast on missing binary.
    match Command::new("helm").arg("version").arg("--short").output() {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(HelmRenderError::BinaryNotFound);
        }
        Err(e) => return Err(HelmRenderError::IoError(e)),
    }

    let (tx, rx) = mpsc::channel();
    let chart_dir = chart_dir.to_path_buf();
    thread::spawn(move || {
        let result = Command::new("helm")
            .args(["template", &chart_dir.to_string_lossy()])
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(HelmRenderError::IoError(e)),
        Err(_) => return Err(HelmRenderError::Timeout),
    };

    if !output.status.success() {
        return Err(HelmRenderError::NonZeroExit {
            code: output.status.code().unwrap_or(-1),
            stderr_head: cap_stderr_lines(&output.stderr, 20),
        });
    }

    // Success path: apply the existing IMAGE_REGEX from the
    // unrendered extraction to the rendered stdout.
    Ok(extract_image_refs_from_yaml_bytes(&output.stdout))
}
```

**Rationale**: 3 pre-existing sites (m053, m055, m173) prove the pattern is stable + reviewer-familiar. No new subprocess-runtime abstraction added. Worker-thread leak on timeout is acknowledged in m055 comments — same trade-off applies here.

**Alternatives considered + rejected**:
- `tokio::process::Command::spawn` + `tokio::time::timeout`: rejected — requires making the helm reader async, cascading into calling code. mikebom's readers are all synchronous today.
- `nix::sys::wait::waitpid` with WNOHANG polling loop: rejected — POSIX-only, extra dependency, no clear win over the stdlib pattern.
- Custom timeout wrapper crate (e.g., `wait-timeout`): rejected — one more dep to justify per Constitution I.

**References**:
- `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs:81-158` — canonical pattern.
- m188 `contracts/extraction-pipeline.md §Phase C` — pre-approved design.

## R2 — Branch site at helm.rs:300-301

**Investigation**: `mikebom-cli/src/scan_fs/package_db/helm.rs::read` at line 282+. Line 300-301:

```rust
// US3 (`HelmRenderMode::OptIn`) is a follow-up; for now, always
// unrendered.
let _ = render_mode; // future US3 hook
let image_refs = extract_image_refs_unrendered(rootfs);
```

The `let _ = render_mode;` is the exact spot m188 left for m203 to fill.

**Decision**: Replace the 3-line pre-existing block with the match:

```rust
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
diagnostics.helm_extraction_mode = Some(extraction_mode);
```

**Rationale**: The match preserves the existing `Off` path exactly (same call to `extract_image_refs_unrendered`) and adds the `OptIn` branch. `diagnostics.helm_extraction_mode` was already being set to `Unrendered` post-line-301 in m188 — the m203 wiring just updates that assignment to reflect the actual outcome (Rendered on success, Unrendered on fallback).

**Alternatives considered + rejected**:
- Inline the match into a helper `select_image_refs(rootfs, mode, diagnostics)` to reduce read()'s cyclomatic complexity: rejected — the match is 12 lines; extraction adds indirection without meaningful reuse.

**References**:
- `mikebom-cli/src/scan_fs/package_db/helm.rs:282-310` — read() body.
- m188 `contracts/extraction-pipeline.md` — Phase B/C pseudocode.

## R3 — Test strategy for the 4 fallback classes

**Investigation**: US2 acceptance scenarios require exercising `BinaryNotFound`, `NonZeroExit`, `Timeout` fallback classes without depending on a real `helm` install in default CI. Options:

| Approach | Coverage | CI Dep | Test Speed |
|---|---|---|---|
| A: `PATH=""` for BinaryNotFound; stub shell scripts for NonZeroExit + Timeout | Full 3 classes | POSIX shell only | Fast (subprocess spawn + small wait) |
| B: Mock the subprocess boundary via a trait injected into `helm::read` | Full 3 classes | None | Fastest (no subprocess) |
| C: Skip US2 fallback tests in default CI; gate all behind `MIKEBOM_HELM_INTEGRATION=1` | 0 in default CI | Real helm binary | Slow (only in nightly) |

**Decision**: Approach A. Set `PATH=""` in the test's environment to exercise `BinaryNotFound`. Create stub shell scripts (`helm-exit1.sh`, `helm-sleep-*.sh`) checked into the fixture directory; add their parent dir to `PATH` in the test's env to make them the resolved `helm` binary. Each stub is 1-2 lines of `sh`.

**Rationale**:
- Full 3-class fallback coverage in default CI (matches Constitution VII isolation posture).
- Zero mocking-abstraction added (trait injection in Option B would leak into production).
- Subprocess overhead is minor — each test runs ~100ms including stub spawn + timeout wait.
- Windows: bash scripts don't work; mark the shell-script tests `#[cfg(unix)]` and add a separate Windows-only variant using `.bat`/PowerShell if needed (deferred to nightly / not required for the fix's landing).

**Alternatives considered + rejected**:
- Approach B: rejected — mocking abstraction unnecessarily complicates helm.rs production surface.
- Approach C: rejected — leaves the fallback classes untested in default CI, which is the very code we're introducing.

**Success path (US1)** stays gated behind `MIKEBOM_HELM_INTEGRATION=1` per m188 precedent — nightly-only, requires real `helm` binary + real chart fixture.

**References**:
- `mikebom-cli/tests/helm_reader.rs` — existing m188 test file to extend.
- Memory `reference_spdx3_validator` — precedent for env-var-gated external-binary tests.

## R4 — Timeout resolution + validation

**Investigation**: `MIKEBOM_HELM_RENDER_TIMEOUT_SECS` env var per FR-002. Needs bounds check (positive integer) + default.

**Decision**: `resolve_render_timeout() -> Duration` helper:

```rust
fn resolve_render_timeout() -> Duration {
    const DEFAULT_SECS: u64 = 60;
    const MIN_SECS: u64 = 1;
    const MAX_SECS: u64 = 3600;
    let secs = std::env::var("MIKEBOM_HELM_RENDER_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|n| n.clamp(MIN_SECS, MAX_SECS))
        .unwrap_or(DEFAULT_SECS);
    Duration::from_secs(secs)
}
```

**Rationale**:
- Default 60s per m188 documentation.
- Clamp `[1, 3600]` (1 second min, 1 hour max) matches the existing Go warm-cache timeout convention (m173).
- Silently clamps rather than erroring on out-of-range — matches mikebom's other env-var handlers (per m089+ pattern) which prefer degraded-mode-over-abort for operator-supplied overrides.
- Non-numeric input silently falls back to the default. Documented in the doc comment.

**Alternatives considered + rejected**:
- Return `Result<Duration, ParseError>` and abort scan on parse failure: rejected — env-var validation errors would break scans in unrelated ways for operators experimenting with the flag.
- No clamp (accept any u64): rejected — a value of `u64::MAX` seconds would functionally never time out; defeats the purpose.

**References**:
- m173 warm-go-cache timeout resolution pattern (env-var + default + clamp).

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Subprocess pattern | m055 `run_go_mod_graph` verbatim (thread + `mpsc::channel` + `recv_timeout`) | tokio::process, custom crate | 3-site precedent; no new deps |
| Branch site | In-place at helm.rs:300-301 (`let _ = render_mode` replaced with match) | Extract to helper `select_image_refs` | Match is 12 lines; extraction adds indirection |
| Fallback test strategy | Approach A: `PATH=""` + stub shell scripts | Mock injection / gate-behind-integration | Full 3-class coverage in default CI, zero mocking abstraction |
| Timeout resolution | Env-var + default 60s + clamp `[1, 3600]` + silent fallback on parse-fail | Error on out-of-range / no clamp | Matches m173 + m089 env-var handling posture |
| New Cargo deps | Zero | (n/a) | Nothing needed |
