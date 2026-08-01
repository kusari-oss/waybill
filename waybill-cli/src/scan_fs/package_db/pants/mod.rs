//! Milestone 223: Pants pex-lockfile reader — orchestrator entry.
//!
//! Discovers Pex lockfiles (default glob `3rdparty/python/*.lock` +
//! optional `pants.toml`-declared path per FR-004), parses each,
//! and emits one `PackageDbEntry` per locked distribution.
//!
//! Fail-open per contract: any per-file corruption logs a WARN and
//! is skipped; the whole scan never aborts on Pex-lockfile issues.
//!
//! See `specs/223-pants-pex-reader/` for spec + plan + contracts.

pub mod config;
pub mod lockfile;
pub mod resolve_classifier;

use std::path::{Path, PathBuf};

use super::PackageDbEntry;

/// Discovered lockfile: absolute path + resolve name (derived from
/// the filename stem, e.g., `3rdparty/python/mypy.lock` → `mypy`).
struct DiscoveredLockfile {
    path: PathBuf,
    resolve_name: String,
}

/// Enumerate lockfile candidates: the default `3rdparty/python/*.lock`
/// glob PLUS, if `pants.toml` declares a `[python].lockfile` path,
/// that path (relative to `scan_root`).
///
/// Missing / malformed `pants.toml` gracefully falls through per
/// FR-004; the caller does not need to distinguish "no pants.toml"
/// from "invalid pants.toml".
fn discover_lockfiles(scan_root: &Path) -> Vec<DiscoveredLockfile> {
    let mut out = Vec::new();

    // Default glob: 3rdparty/python/*.lock.
    let default_dir = scan_root.join("3rdparty").join("python");
    if let Ok(read_dir) = std::fs::read_dir(&default_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lock") {
                let resolve_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("default")
                    .to_string();
                out.push(DiscoveredLockfile {
                    path,
                    resolve_name,
                });
            }
        }
    }

    // pants.toml override: [python].lockfile.
    let pants_toml = scan_root.join("pants.toml");
    if pants_toml.exists() {
        match std::fs::read(&pants_toml) {
            Ok(bytes) => {
                if let Some(cfg) = config::parse(&bytes) {
                    if let Some(custom_path) = cfg.python.lockfile {
                        let resolved = scan_root.join(&custom_path);
                        if resolved.exists() {
                            let resolve_name = resolved
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("default")
                                .to_string();
                            // Avoid duplicating a path we already found via glob.
                            if !out.iter().any(|d| d.path == resolved) {
                                out.push(DiscoveredLockfile {
                                    path: resolved,
                                    resolve_name,
                                });
                            }
                        } else {
                            tracing::warn!(
                                pants_toml = %pants_toml.display(),
                                declared_path = %custom_path,
                                "pants-pex reader: pants.toml declares [python].lockfile path that does not exist on disk; falling back to default glob"
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        pants_toml = %pants_toml.display(),
                        "pants-pex reader: pants.toml could not be parsed as TOML; falling back to default glob"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    pants_toml = %pants_toml.display(),
                    error = %e,
                    "pants-pex reader: pants.toml could not be read; falling back to default glob"
                );
            }
        }
    }

    out
}

/// Public entry — orchestrates lockfile discovery + parsing + entry
/// emission. Returns `Vec::new()` when no lockfiles are found (and
/// emits no log line — preserves byte-identity for non-Pants repos
/// per FR-007 / SC-003).
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry> {
    let candidates = discover_lockfiles(scan_root);
    if candidates.is_empty() {
        return Vec::new();
    }

    let lockfiles_discovered = candidates.len();
    let mut lockfiles_parsed_ok: usize = 0;
    let mut lockfiles_skipped_corrupt: usize = 0;
    let mut components: Vec<PackageDbEntry> = Vec::new();

    for candidate in &candidates {
        let bytes = match std::fs::read(&candidate.path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    lockfile = %candidate.path.display(),
                    error = %e,
                    "pants-pex reader: could not read lockfile bytes; skipping"
                );
                lockfiles_skipped_corrupt += 1;
                continue;
            }
        };
        let Some(lock) = lockfile::parse(&bytes) else {
            // lockfile::parse already emitted a WARN with the reason.
            tracing::warn!(
                lockfile = %candidate.path.display(),
                "pants-pex reader: parse failed for the file above; skipping"
            );
            lockfiles_skipped_corrupt += 1;
            continue;
        };
        lockfiles_parsed_ok += 1;
        for resolve in &lock.locked_resolves {
            for req in &resolve.locked_requirements {
                if let Some(entry) = lockfile::locked_req_to_entry(
                    req,
                    &candidate.path,
                    &candidate.resolve_name,
                ) {
                    components.push(entry);
                }
            }
        }
    }

    let components_emitted = components.len();
    tracing::info!(
        lockfiles_discovered,
        lockfiles_parsed_ok,
        lockfiles_skipped_corrupt,
        components_emitted,
        "pants-pex reader complete"
    );

    components
}
