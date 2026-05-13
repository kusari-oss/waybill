# Data Model — milestone 100

Per-file shape of every deliverable. The milestone has three deliverable streams: (a) path-normalization code + helper, (b) CI YAML, (c) release pipeline YAML + docs.

## File inventory

| File | State | Owner FRs |
|------|-------|-----------|
| `mikebom-cli/src/scan_fs/sbom_path.rs` | NEW | FR-004, SC-003 |
| `mikebom-cli/src/scan_fs/mod.rs` | MODIFY | wire chokepoint at ResolvedComponent builder |
| `mikebom-cli/src/generate/cyclonedx/evidence.rs` | MODIFY | defensive normalization at `location` emission |
| `mikebom-cli/src/generate/spdx/annotations.rs` | MODIFY | defensive normalization at SPDX 2.3 occurrences |
| `mikebom-cli/src/generate/spdx/v3_annotations.rs` | MODIFY | defensive normalization at SPDX 3 occurrences |
| `.github/workflows/ci.yml` | MODIFY | add `lint-and-test-windows` job |
| `.github/workflows/release.yml` | MODIFY | add `build-windows-x86_64` job + update aggregation `needs:` |
| `README.md` | MODIFY | Windows install + usage section per FR-011 |
| (optional) `tests/filesystem_walker_*.rs` | MODIFY | add `#[cfg(unix)]` gate to symlink-creating tests if not already gated |

## `sbom_path.rs` — NEW

```rust
//! Path normalization for SBOM JSON emission per milestone-100 spec §
//! Clarifications + research §2.
//!
//! On Windows, replaces backslash separators with forward-slash; on
//! Unix, returns the native string unchanged. SBOM JSON is a
//! cross-platform artifact; forward-slash everywhere matches the de
//! facto industry convention (syft + trivy) and the CDX 1.6 / SPDX 2.3
//! / SPDX 3 schema example conventions.
//!
//! Only the separator character is normalized. Drive-letter prefixes
//! (`C:`) are preserved verbatim — a Windows path like
//! `C:\Users\dev\Cargo.toml` becomes `C:/Users/dev/Cargo.toml`.

use std::path::Path;

/// Normalize a filesystem path for SBOM JSON emission.
pub fn normalize_sbom_path(path: &Path) -> String {
    let raw = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        raw.replace('\\', '/')
    } else {
        raw
    }
}

/// Convenience variant for `&str` callers where the path has already
/// been converted to a `String` (e.g., `PackageDbEntry.source_path`).
pub fn normalize_sbom_path_str(s: &str) -> String {
    if cfg!(windows) {
        s.replace('\\', '/')
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn unix_path_unchanged() {
        // On Unix hosts, native paths use forward-slash already.
        // On Windows hosts running this test, no backslashes present
        // → still unchanged.
        let p = PathBuf::from("/home/dev/project/Cargo.toml");
        assert_eq!(normalize_sbom_path(&p), "/home/dev/project/Cargo.toml");
    }

    #[test]
    fn windows_backslash_normalized_on_windows() {
        // This test only meaningfully exercises on Windows hosts; on
        // Unix the path stays backslash-separated (because Unix doesn't
        // treat `\` as a separator). Still asserts the function
        // doesn't crash on either host.
        let p = PathBuf::from(r"C:\Users\dev\project\Cargo.toml");
        let out = normalize_sbom_path(&p);
        #[cfg(windows)]
        assert_eq!(out, "C:/Users/dev/project/Cargo.toml");
        #[cfg(not(windows))]
        assert_eq!(out, r"C:\Users\dev\project\Cargo.toml");
    }

    #[test]
    fn str_variant_normalizes_str_input() {
        #[cfg(windows)]
        assert_eq!(normalize_sbom_path_str(r"C:\a\b"), "C:/a/b");
        #[cfg(not(windows))]
        assert_eq!(normalize_sbom_path_str("/a/b"), "/a/b");
    }
}
```

## `scan_fs/mod.rs` chokepoint — MODIFY

Two sites: the main deduplicator-builder at `scan_fs/mod.rs:534` (which builds `ResolvedComponent` from a `PackageDbEntry`) + the dedup entry-point at `scan_fs/mod.rs:167`. Both populate `source_path` + `evidence.source_file_paths`:

```rust
// Before:
evidence: ResolutionEvidence {
    technique: ResolutionTechnique::PackageDatabase,
    confidence: PACKAGE_DB_CONFIDENCE,
    source_connection_ids: vec![],
    source_file_paths: vec![entry.source_path.clone()],
    deps_dev_match: None,
},

// After (milestone 100):
evidence: ResolutionEvidence {
    technique: ResolutionTechnique::PackageDatabase,
    confidence: PACKAGE_DB_CONFIDENCE,
    source_connection_ids: vec![],
    source_file_paths: vec![
        crate::scan_fs::sbom_path::normalize_sbom_path_str(&entry.source_path),
    ],
    deps_dev_match: None,
},
```

The `entry.source_path` itself remains native-OS format internally (used for logging, file opens, etc.). Only the SBOM-bound `source_file_paths` field is normalized.

## CDX / SPDX 2.3 / SPDX 3 emission sites — MODIFY

3 defensive-normalization sites:

### `cyclonedx/evidence.rs:84`

```rust
// Before:
let occ_entries: Vec<serde_json::Value> = occurrences
    .iter()
    .map(|o| {
        // ...
        json!({
            "location": o.location,
            "additionalContext": serde_json::to_string(&ctx)
                .unwrap_or_default(),
        })
    })
    .collect();

// After (milestone 100):
let occ_entries: Vec<serde_json::Value> = occurrences
    .iter()
    .map(|o| {
        // ...
        json!({
            "location": crate::scan_fs::sbom_path::normalize_sbom_path_str(&o.location),
            "additionalContext": serde_json::to_string(&ctx)
                .unwrap_or_default(),
        })
    })
    .collect();
```

### `spdx/annotations.rs:244` (around the D2 evidence.occurrences block)

Same pattern: wrap the path-string emission with `normalize_sbom_path_str(...)`.

### `spdx/v3_annotations.rs:257` (same block, SPDX 3)

Same pattern.

## `.github/workflows/ci.yml` — MODIFY

Add the new job AFTER `lint-and-test-macos` (currently ends at line ~245). Job body per `research.md §3`. Total addition: ~30 lines of YAML.

## `.github/workflows/release.yml` — MODIFY

Two changes:
1. Add `build-windows-x86_64` job after `build-macos-aarch64` (at line ~250). Job body per `research.md §4`. Total addition: ~40 lines of YAML.
2. Update `release` job's `needs:` array:
   ```yaml
   # Before:
   needs: [build-linux-x86_64, build-linux-aarch64, build-macos-aarch64]
   # After:
   needs: [build-linux-x86_64, build-linux-aarch64, build-macos-aarch64, build-windows-x86_64]
   ```

## `README.md` — MODIFY

Add a Windows section to the install/usage docs. Target ~30 lines:

```markdown
### Windows install

Download `mikebom-v<version>-x86_64-pc-windows-msvc.zip` from the
latest [release](https://github.com/kusari-sandbox/mikebom/releases),
extract `mikebom.exe`, and place it on your PATH.

### Windows usage

mikebom on Windows supports the same cross-platform ecosystem readers
(cargo / npm / pip / gem / maven / go) as Linux/macOS:

```powershell
mikebom.exe sbom scan --path C:\Users\dev\my-project --output out.cdx.json
```

Linux-specific readers (dpkg, rpm, apk) and eBPF tracing are not
applicable on Windows hosts and produce no output.

Path strings in emitted SBOMs are forward-slash-normalized regardless
of host OS to preserve cross-host SBOM portability.
```

## (Optional) Symlink-creating tests — MODIFY

Audit `tests/filesystem_walker_*.rs` (milestone 054) for unconditional `std::os::unix::fs::symlink` calls. If any exist without a `#[cfg(unix)]` gate on the enclosing test function, add the gate. The Windows CI lane will surface these failures during T002 bring-up; implementer fixes inline.

## Compatibility

- **No `Cargo.lock` change** — pure in-source addition + JSON-emission edits.
- **Goldens regen on first Windows CI lane run**: the path-normalization decision means the workspace-path-stripping helper in the goldens harness needs to be aware of the new forward-slash format. If the existing helper uses raw `path.to_string_lossy()` for the workspace-root prefix it strips, the strip may not match on Windows. **Verify at implementation time**: extend the cross-host normalize helper if needed. Goldens themselves don't regenerate (they're already forward-slash on Linux/macOS).
- **Backward compatibility** — 100% additive. The Linux + macOS hosts continue to emit identical SBOM bytes (forward-slash was already their native format). Windows is a new host whose emissions match the Linux/macOS convention.

## No JSON / no YAML schema additions

Zero new fields. The path-normalization is value-level (string content) not shape-level.
