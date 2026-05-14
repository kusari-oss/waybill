# Data Model — milestone 101

Per-file shape of every deliverable. The milestone has three deliverable streams: (a) the new Windows smoke test, (b) CI YAML split, (c) docs experimental callouts.

## File inventory

| File | State | Owner FRs |
|------|-------|-----------|
| `mikebom-cli/tests/scan_windows_smoke.rs` | NEW | FR-001, FR-002, FR-003, FR-004, FR-009, FR-011, FR-012 |
| `.github/workflows/ci.yml` | MODIFY | FR-008 |
| `README.md` | MODIFY | FR-005, FR-007 |
| `docs/user-guide/installation.md` | MODIFY | FR-006 |

Total: 1 new + 3 modified = 4 files. Matches FR-009 + SC-008 scope.

## `scan_windows_smoke.rs` — NEW

```rust
//! Milestone 101: Windows-host integration smoke test. Validates that
//! the locally-built `mikebom.exe` (a) exits 0 against two cross-
//! platform fixtures, (b) emits well-formed CycloneDX 1.6 JSON,
//! (c) emits ≥1 component per expected ecosystem, (d) forward-slash-
//! normalizes path-shaped fields per milestone 100 Contract 3, and
//! (e) completes within 60 seconds per scan (hang regression guard).
//!
//! `#[cfg(windows)]`-gated: on Linux/macOS the existing goldens
//! regression suite covers Unix forward-slash behavior; this file
//! compiles to an empty integration-test binary on non-Windows.
//!
//! Failure-diagnostic policy (FR-012): on assertion failure, print
//! the first 10 emitted component PURLs + the offending path-field
//! value(s) inline, AND write the full emitted SBOM to a per-test
//! tempdir as `actual.cdx.json` with the absolute path printed.

#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCAN_TIMEOUT_SECS: u64 = 60;
const TIMEOUT_DETECTION_THRESHOLD_SECS: u64 = 58;

/// Canonical list of path-shaped property/field names emitted by
/// milestone-100's normalization chokepoint + the 3 defensive
/// emission sites. Used by `walk_for_backslash_in_path_fields`
/// to scope the backslash check (FR-003) — broader scoping would
/// false-positive on CPE 2.3 escape sequences (the iteration-6
/// lesson from milestone 100).
const PATH_FIELD_NAMES: &[&str] = &[
    "mikebom:source-files",
    "mikebom:source-path",
    "location",
];

/// Run mikebom.exe sbom scan with a hard 60-second timeout.
/// Returns (exit_status, elapsed). On timeout, kills the subprocess
/// and panics with a clear hang-regression message.
fn run_scan_with_timeout(input_path: &Path, output_path: &Path) -> (std::process::ExitStatus, Duration) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mikebom"))
        .args([
            "sbom",
            "scan",
            "--path",
            input_path.to_str().expect("path utf-8"),
            "--output",
            output_path.to_str().expect("output utf-8"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mikebom.exe");

    let child_id = child.id();
    let timeout_handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(SCAN_TIMEOUT_SECS));
        // Best-effort kill. If the main thread already wait()ed, this
        // taskkill becomes a no-op (PID gone).
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &child_id.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    });

    let start = Instant::now();
    let status = child.wait().expect("wait mikebom.exe");
    let elapsed = start.elapsed();
    let _ = timeout_handle.join();

    if elapsed > Duration::from_secs(TIMEOUT_DETECTION_THRESHOLD_SECS) {
        panic!(
            "mikebom.exe sbom scan timed out — likely hang regression \
             (elapsed: {elapsed:?}, fixture: {})",
            input_path.display()
        );
    }
    (status, elapsed)
}

/// Recursive walk over the SBOM JSON. Returns every (field-name,
/// offending-value) pair where a path-shaped field value contains
/// a backslash. Scoped to CDX `properties[]` shape (`{"name": ...,
/// "value": ...}`) AND direct-field shapes (`location` on
/// `evidence.occurrences[]`). Per FR-003 + research §4 + §7.
fn walk_for_backslash_in_path_fields(val: &serde_json::Value) -> Vec<(String, String)> {
    fn inner(val: &serde_json::Value, hits: &mut Vec<(String, String)>) {
        match val {
            serde_json::Value::Object(map) => {
                // CDX properties[] shape: { "name": "mikebom:source-files", "value": "..." }
                if let (Some(name_str), Some(value)) =
                    (map.get("name").and_then(|n| n.as_str()), map.get("value"))
                {
                    if PATH_FIELD_NAMES.contains(&name_str) {
                        if let Some(value_str) = value.as_str() {
                            if value_str.contains('\\') {
                                hits.push((name_str.to_string(), value_str.to_string()));
                            }
                        }
                    }
                }
                // Direct-field shapes (e.g., "location" on CDX evidence.occurrences[]).
                for (k, v) in map {
                    if PATH_FIELD_NAMES.contains(&k.as_str()) {
                        if let Some(value_str) = v.as_str() {
                            if value_str.contains('\\') {
                                hits.push((k.clone(), value_str.to_string()));
                            }
                        }
                    }
                    inner(v, hits);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    inner(v, hits);
                }
            }
            _ => {}
        }
    }
    let mut hits = Vec::new();
    inner(val, &mut hits);
    hits
}

/// On assertion failure, write the emitted SBOM to a per-test
/// tempdir as `actual.cdx.json` and print first 10 component PURLs
/// + offending fields inline. FR-012.
fn diagnose_and_panic(label: &str, sbom: &serde_json::Value, raw: &str, msg: String) -> ! {
    let tmp = std::env::temp_dir().join(format!("mikebom-smoke-{label}-{}.cdx.json", std::process::id()));
    let _ = std::fs::write(&tmp, raw);
    eprintln!("\n--- SMOKE FAILURE [{label}] ---");
    eprintln!("{msg}");
    if let Some(comps) = sbom.get("components").and_then(|c| c.as_array()) {
        eprintln!("First 10 component PURLs:");
        for c in comps.iter().take(10) {
            let p = c.get("purl").and_then(|p| p.as_str()).unwrap_or("<missing>");
            eprintln!("  {p}");
        }
    }
    eprintln!("Full SBOM written to: {}", tmp.display());
    eprintln!("--- end smoke failure ---\n");
    panic!("smoke test [{label}] failed");
}

fn run_smoke_case(label: &str, fixture_subpath: &str, expected_purl_prefixes: &[&str]) {
    let fixtures_root = PathBuf::from(env!("MIKEBOM_FIXTURES_DIR"));
    let input = fixtures_root.join(fixture_subpath);
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("out.cdx.json");

    let (status, elapsed) = run_scan_with_timeout(&input, &output);
    eprintln!("[smoke:{label}] scan completed in {elapsed:?}");
    assert!(
        status.success(),
        "smoke [{label}]: mikebom.exe exited non-zero ({status:?})"
    );

    let raw = std::fs::read_to_string(&output).expect("read emitted SBOM");
    let sbom: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => diagnose_and_panic(label, &serde_json::Value::Null, &raw, format!("malformed JSON: {e}")),
    };

    // Envelope: CycloneDX 1.6.
    if sbom.get("bomFormat").and_then(|v| v.as_str()) != Some("CycloneDX") {
        diagnose_and_panic(label, &sbom, &raw, format!("bomFormat != CycloneDX"));
    }
    if sbom.get("specVersion").and_then(|v| v.as_str()) != Some("1.6") {
        diagnose_and_panic(label, &sbom, &raw, format!("specVersion != 1.6"));
    }

    // ≥1 component per expected prefix.
    let comps = sbom
        .get("components")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    if comps.is_empty() {
        diagnose_and_panic(label, &sbom, &raw, "components[] empty".to_string());
    }
    for prefix in expected_purl_prefixes {
        let matched = comps.iter().any(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .map(|p| p.starts_with(prefix))
                .unwrap_or(false)
        });
        if !matched {
            diagnose_and_panic(label, &sbom, &raw, format!("no component with PURL prefix {prefix}"));
        }
    }

    // FR-003: no backslashes in path-shaped fields.
    let bs_hits = walk_for_backslash_in_path_fields(&sbom);
    if !bs_hits.is_empty() {
        let summary = bs_hits
            .iter()
            .take(5)
            .map(|(name, value)| format!("    {name} = {value}"))
            .collect::<Vec<_>>()
            .join("\n");
        diagnose_and_panic(
            label,
            &sbom,
            &raw,
            format!(
                "found {} path-shaped field value(s) with backslash separators \
                 (milestone-100 normalization regression):\n{summary}",
                bs_hits.len()
            ),
        );
    }
}

#[test]
fn smoke_cargo_fixture() {
    run_smoke_case(
        "cargo",
        "cargo/lockfile-v3",
        &["pkg:cargo/"],
    );
}

#[test]
fn smoke_polyglot_monorepo() {
    run_smoke_case(
        "polyglot",
        "polyglot-monorepo",
        &["pkg:pypi/", "pkg:npm/"],
    );
}
```

Total: ~150 lines including doc-comments. Per FR-009 only existing deps: `std::process`, `std::path`, `std::time`, `std::thread`, `std::fs`, `tempfile` (already in dev-deps), `serde_json` (already in workspace), `env!()` macro.

## `.github/workflows/ci.yml` — MODIFY

Current Windows lane (post-milestone-100, lines ~283-303 inclusive of the `continue-on-error` block):

```yaml
      - name: Clippy
        run: cargo +stable clippy --workspace --all-targets -- -D warnings

      # Milestone 100 ships build correctness ... (long comment block)
      - name: Tests (non-blocking, see issue #210)
        continue-on-error: true
        run: cargo +stable test --workspace
```

Replace with:

```yaml
      - name: Clippy
        run: cargo +stable clippy --workspace --all-targets -- -D warnings

      # Milestone 101 — Windows smoke test (blocking gate).
      # Runs FIRST so the blocking gate fails fast on a runtime
      # regression in `mikebom.exe sbom scan` before paying for
      # the full workspace test compile + run. See specs/101-
      # windows-smoke-experimental/.
      - name: Smoke test (blocking — milestone 101)
        run: cargo +stable test --test scan_windows_smoke

      # Milestone 100 ships build correctness (clippy `-D warnings`
      # above) + SBOM emission with forward-slash paths + a Windows
      # release artifact. The smoke step (above) gates the runtime
      # regression class. Full `cargo test --workspace` parity is
      # tracked in #210; this step's per-test failures stay visible
      # in CI logs but don't block the merge until #210 closes.
      - name: Tests (non-blocking, see issue #210)
        continue-on-error: true
        run: cargo +stable test --workspace
```

Diff: replace the existing `Tests` block (lines ~289-303) with the new two-step layout. ~16 lines added.

## `README.md` — MODIFY

Two changes:

### Change 1: Platform-support table cell

Current (post-milestone-100, line ~272):
```markdown
| Windows x86_64    | ✅ supported (milestone 100)         | ❌                          |
```

Replace with:
```markdown
| Windows x86_64    | 🧪 experimental (milestone 100, [#210](https://github.com/kusari-sandbox/mikebom/issues/210)) | ❌ |
```

### Change 2: Insert experimental callout into "Windows install" subsection

Current Windows install subsection begins (line ~280-ish post-milestone-100):
```markdown
### Windows install

Download `mikebom-v<version>-x86_64-pc-windows-msvc.zip` from the
[latest release](https://github.com/kusari-sandbox/mikebom/releases),
extract `mikebom.exe`, and place it on your `PATH`.
```

Replace with:
```markdown
### Windows install

> 🧪 **Experimental.** Windows builds are available as of milestone
> 100, but are not feature-equivalent to Linux/macOS yet. Known gaps:
> Linux-only OS package readers (dpkg/rpm/apk), HOME-env-var-derived
> cache paths, OCI image cache atomic-rename, path-resolver pattern
> matcher, and Python stdlib collapse. Full Windows runtime test
> parity + production-code fixes are tracked in
> [#210](https://github.com/kusari-sandbox/mikebom/issues/210).
> Do not rely on the Windows binary for production SBOM workflows
> until #210 closes.

Download `mikebom-v<version>-x86_64-pc-windows-msvc.zip` from the
[latest release](https://github.com/kusari-sandbox/mikebom/releases),
extract `mikebom.exe`, and place it on your `PATH`.
```

## `docs/user-guide/installation.md` — MODIFY

Verified at plan-time: the file exists, opens with a platform-support table at line ~5 (currently lists `Windows (WSL2)` as the scanning platform — pre-milestone-100 wording, doesn't reflect native Windows), and has no dedicated `### Windows install` subsection. Two concrete edits:

### Edit 1: Platform-support table

Current (line ~7):
```markdown
| **Scanning** | `mikebom sbom scan`, ... | Any OS Rust runs on. No privilege. No eBPF. |
```

The mention of Windows in the prose immediately below the table (`Windows (WSL2)`) is the line to update. Replace:
```markdown
If you only need the scanning surface, mikebom runs natively on macOS,
Windows (WSL2), or Linux. `trace` requires Linux with eBPF.
```
with:
```markdown
If you only need the scanning surface, mikebom runs natively on macOS,
Linux, or Windows (the Windows binary is 🧪 [experimental](https://github.com/kusari-sandbox/mikebom/issues/210); WSL2 also works for both scanning and tracing). `trace` requires Linux with eBPF.
```

### Edit 2: Add a new "Windows install (experimental)" subsection

Inserted between `## Pre-built binaries (recommended)` and `## Build from source`. Contains the canonical experimental callout (byte-identical to README's callout block per Contract 8) PLUS a one-line reference back to the README for the canonical download instructions:

```markdown
## Windows install (experimental)

> 🧪 **Experimental.** Windows builds are available as of milestone
> 100, but are not feature-equivalent to Linux/macOS yet. Known gaps:
> Linux-only OS package readers (dpkg/rpm/apk), HOME-env-var-derived
> cache paths, OCI image cache atomic-rename, path-resolver pattern
> matcher, and Python stdlib collapse. Full Windows runtime test
> parity + production-code fixes are tracked in
> [#210](https://github.com/kusari-sandbox/mikebom/issues/210).
> Do not rely on the Windows binary for production SBOM workflows
> until #210 closes.

For the latest Windows x86_64 binary, follow the [Windows install
instructions in the README](../../README.md#windows-install).
```

## Compatibility

- **No `Cargo.lock` change** — pure test + docs + CI YAML.
- **No production-code change** — `mikebom-cli/src/` unchanged.
- **No new crate deps** — `tempfile` + `serde_json` already in dev-deps / workspace.
- **No Linux/macOS CI behavior change** — `#[cfg(windows)]` gate means the new test compiles to empty on Unix; the workflow YAML edits only the Windows job.

## No JSON / no YAML schema additions

Zero new fields. The smoke test READS existing fields; it does not introduce new ones.
