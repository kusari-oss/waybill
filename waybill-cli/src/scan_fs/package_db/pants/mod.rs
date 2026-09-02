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

/// Milestone 672 + 673: where a discovered lockfile came from.
/// Drives the FR-009 map-wins-on-dedup logic — when two
/// `DiscoveredLockfile`s share the same canonicalized path, the one
/// with `origin == PythonResolvesMap` REPLACES the sibling because
/// the pants.toml map key is authoritative over the file-stem-derived
/// name. `PythonLockfileSingular` wins over the auto-discovery peers.
/// The three auto-discovery peers (`DefaultGlob`, `RepoRootGlob`,
/// `LockfilesGlob` from m673) fall to a lex-min `resolve_name`
/// tie-break (see `dedup_by_canonical_path`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoverySource {
    /// Found via the `3rdparty/python/*.lock` default glob (m223).
    /// Resolve name derived from `path.file_stem()`.
    DefaultGlob,
    /// Found via `pants.toml` `[python].lockfile` singular key
    /// (legacy Pants shape). Resolve name derived from
    /// `path.file_stem()` (matches m223 behavior).
    PythonLockfileSingular,
    /// Found via `pants.toml` `[python.resolves]` map (m672).
    /// Resolve name is the map's KEY (authoritative over file-stem
    /// derivation per FR-009).
    PythonResolvesMap,
    /// Milestone 673 FR-001: found via `<scan_root>/*.lock`
    /// enumeration (Pants 2.31+ default layout — no `3rdparty/python/`).
    /// Gated by `is_pex_lockfile_content` content-detection per FR-003;
    /// files that fail the gate silent-skip per FR-004. Resolve name
    /// derived from `path.file_stem()`.
    RepoRootGlob,
    /// Milestone 673 FR-002: found via `<scan_root>/lockfiles/*.lock`
    /// enumeration (Pants convention for dedicated lockfile directories,
    /// e.g. `pantsbuild/example-django`). Non-recursive per FR-009.
    /// Gated by `is_pex_lockfile_content`; silent-skip on gate failure.
    /// Resolve name derived from `path.file_stem()`.
    LockfilesGlob,
}

/// Discovered lockfile: absolute path + resolve name (derived from
/// the filename stem, e.g., `3rdparty/python/mypy.lock` → `mypy`).
#[derive(Clone)]
struct DiscoveredLockfile {
    path: PathBuf,
    resolve_name: String,
    /// Milestone 672: source of the discovery. See [`DiscoverySource`].
    origin: DiscoverySource,
}

/// Milestone 672 FR-013: count how many lockfiles in this scan
/// carried the pre-Pants-2.30 `//`-comment metadata block (i.e., the
/// prefix stripper actually consumed at least one `//` line before
/// handing bytes to the JSON parser). Log-line only in v1 (per 2026-
/// 09-01 clarify Q1); a v2 milestone may promote this to a
/// document-scope annotation.
#[derive(Debug, Default)]
struct LegacyShapeCounter {
    count: usize,
}

impl LegacyShapeCounter {
    /// Record that a lockfile was parsed after the stripper consumed
    /// `stripped_bytes > 0` bytes of leading `//` comments. A zero
    /// value means the file was clean JSON (no increment).
    fn record_stripped(&mut self, stripped_bytes: usize) {
        if stripped_bytes > 0 {
            self.count += 1;
        }
    }

    /// Value emitted into the reader-complete INFO log's
    /// `legacy_shape_lockfiles` field.
    fn as_log_value(&self) -> usize {
        self.count
    }
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
                    origin: DiscoverySource::DefaultGlob,
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
                                    origin: DiscoverySource::PythonLockfileSingular,
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
                    // Milestone 672 T012 (US2): walk `[python.resolves]`
                    // bare-string entries per FR-005 + contract C1/C2/C3.
                    // Non-bare-string entries WARN + skip (FR-007);
                    // missing-path entries WARN + skip (FR-008). Other
                    // entries in the same map remain honored.
                    for (resolve_name, value) in &cfg.python.resolves {
                        let Some(path_str) = value.as_str() else {
                            tracing::warn!(
                                pants_toml = %pants_toml.display(),
                                resolve = %resolve_name,
                                observed_type = value.type_str(),
                                "pants-pex reader: `[python.resolves]` entry has non-string value; skipping. m672 v1 supports bare-string values only. File a follow-up issue if table-shape parsing is needed."
                            );
                            continue;
                        };
                        let joined = scan_root.join(path_str);
                        if !joined.exists() {
                            tracing::warn!(
                                pants_toml = %pants_toml.display(),
                                resolve = %resolve_name,
                                declared_path = %path_str,
                                "pants-pex reader: `[python.resolves]` entry names a path that does not exist on disk; skipping"
                            );
                            continue;
                        }
                        out.push(DiscoveredLockfile {
                            path: joined,
                            resolve_name: resolve_name.clone(),
                            origin: DiscoverySource::PythonResolvesMap,
                        });
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

    // Milestone 673 T004 (US1, FR-001): enumerate `<scan_root>/*.lock`
    // (Pants 2.31+ default layout — `<resolve>.lock` at repo root).
    // Non-recursive. Files gated by `is_pex_lockfile_content`
    // (contract C3); files that FAIL the gate silent-skip per FR-004
    // (no WARN, no counter). Files that PASS append as candidates
    // with `origin: RepoRootGlob` and resolve name from file stem.
    if let Ok(read_dir) = std::fs::read_dir(scan_root) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("lock") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                // Read failure is silent-skip on wide-scope paths
                // (may be a directory named `*.lock`, permission
                // denied on an unrelated file, etc. — FR-004).
                continue;
            };
            if !lockfile::is_pex_lockfile_content(&bytes) {
                // FR-004 silent-skip — this is a non-PEX `.lock` file
                // (Cargo, Poetry, bun, etc.) or a corrupted PEX shape.
                continue;
            }
            let resolve_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("default")
                .to_string();
            out.push(DiscoveredLockfile {
                path,
                resolve_name,
                origin: DiscoverySource::RepoRootGlob,
            });
        }
    }

    // Milestone 673 T008 (US2, FR-002): enumerate
    // `<scan_root>/lockfiles/*.lock` (Pants convention for dedicated
    // multi-resolve lockfile directories, e.g. `pantsbuild/example-django`).
    // Non-recursive per FR-009 — only immediate children of the
    // `lockfiles/` directory are candidates. Gate + silent-skip
    // semantics identical to the repo-root loop above.
    let lockfiles_dir = scan_root.join("lockfiles");
    if let Ok(read_dir) = std::fs::read_dir(&lockfiles_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("lock") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if !lockfile::is_pex_lockfile_content(&bytes) {
                continue;
            }
            let resolve_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("default")
                .to_string();
            out.push(DiscoveredLockfile {
                path,
                resolve_name,
                origin: DiscoverySource::LockfilesGlob,
            });
        }
    }

    // Milestone 672 T013 (US2): canonicalize + dedup per FR-009.
    // Two candidates that resolve to the same file on disk are
    // parsed exactly once. When dedup fires, the `PythonResolvesMap`
    // entry wins (the pants.toml map key is authoritative over
    // file-stem-derived names). Ties within the same origin fall to
    // the lexically-first `resolve_name`.
    dedup_by_canonical_path(out)
}

/// Milestone 672 T013 (US2, FR-009) + m673 T002 (extended for
/// `RepoRootGlob`/`LockfilesGlob`): canonicalize every candidate's
/// path via `std::fs::canonicalize` (follows symlinks per m672
/// research.md §R4) then group by canonical form. For each collision
/// group, apply the precedence rule:
///
/// 1. Any entry with `origin == PythonResolvesMap` wins
///    (pants.toml map key is authoritative — m672 FR-009).
/// 2. Else any entry with `origin == PythonLockfileSingular` wins
///    (m223 explicit `[python].lockfile` beats auto-discovery).
/// 3. Else the entry with the lexically-first `resolve_name` wins
///    (deterministic tie-break among the three auto-discovery
///    peers: `DefaultGlob`, `RepoRootGlob`, `LockfilesGlob`).
///
/// Paths that fail `canonicalize` (rare — e.g. race with a delete
/// between discovery and here) are dropped with a WARN.
fn dedup_by_canonical_path(candidates: Vec<DiscoveredLockfile>) -> Vec<DiscoveredLockfile> {
    use std::collections::BTreeMap;

    // Group by canonical path. Values are candidate entries that
    // share the same canonical path.
    let mut buckets: BTreeMap<PathBuf, Vec<DiscoveredLockfile>> = BTreeMap::new();
    for candidate in candidates {
        let canonical = match std::fs::canonicalize(&candidate.path) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    path = %candidate.path.display(),
                    error = %e,
                    "pants-pex reader: could not canonicalize discovered lockfile path; skipping"
                );
                continue;
            }
        };
        buckets.entry(canonical).or_default().push(candidate);
    }

    let mut out: Vec<DiscoveredLockfile> = Vec::with_capacity(buckets.len());
    for (canonical, group) in buckets {
        let winner = if let Some(map_entry) = group
            .iter()
            .find(|d| d.origin == DiscoverySource::PythonResolvesMap)
        {
            // Precedence tier 1: `[python.resolves]` map key.
            map_entry.clone()
        } else if let Some(singular_entry) = group
            .iter()
            .find(|d| d.origin == DiscoverySource::PythonLockfileSingular)
        {
            // Precedence tier 2: `[python].lockfile` singular
            // (m673 extension per research.md §R4).
            singular_entry.clone()
        } else {
            // Precedence tier 3: lex-min `resolve_name` among the
            // three auto-discovery peers (`DefaultGlob`,
            // `RepoRootGlob`, `LockfilesGlob`).
            group
                .iter()
                .min_by(|a, b| a.resolve_name.cmp(&b.resolve_name))
                .cloned()
                .expect("group is non-empty by construction")
        };
        // Replace the path with the canonicalized form so downstream
        // reads / logs / dedup operate on the same address space.
        out.push(DiscoveredLockfile {
            path: canonical,
            ..winner
        });
    }
    out
}

/// Public entry — orchestrates lockfile discovery + parsing + entry
/// emission.
///
/// Milestone 672 T019 (US3, FR-010/FR-011/FR-012): the empty-candidates
/// path is Pants-signal-gated. When AT LEAST ONE Pants signal is
/// present (either `3rdparty/python/` OR `pants.toml`) but discovery
/// found zero lockfiles, emit a single-line INFO diagnostic naming the
/// outcome + the two supported override keys so operators can self-
/// diagnose. When NO Pants signal is present, remain silent — this
/// preserves byte-identity for non-Pants repos per m223 SC-003.
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry> {
    let default_dir_exists = scan_root.join("3rdparty").join("python").exists();
    let pants_toml_exists = scan_root.join("pants.toml").exists();
    // Milestone 673 T009 (US2, FR-006): the `<scan_root>/lockfiles/`
    // directory counts as a Pants signal so the m672 US3 zero-
    // discovered diagnostic INFO log fires for repos that use ONLY
    // the `lockfiles/` convention (no `pants.toml`, no
    // `3rdparty/python/`). Directory-existence check only — its
    // contents don't matter for the signal.
    let lockfiles_dir_exists = scan_root.join("lockfiles").exists();
    let pants_signal_present =
        default_dir_exists || pants_toml_exists || lockfiles_dir_exists;

    let candidates = discover_lockfiles(scan_root);
    if candidates.is_empty() {
        if pants_signal_present {
            tracing::info!(
                lockfiles_discovered = 0_usize,
                hint = "supply lockfile paths via `[python.resolves]` or `[python].lockfile` in pants.toml",
                "pants-pex reader complete"
            );
        }
        return Vec::new();
    }

    let lockfiles_discovered = candidates.len();
    let mut lockfiles_parsed_ok: usize = 0;
    let mut lockfiles_skipped_corrupt: usize = 0;
    // Milestone 672 T007 FR-013: count how many parsed lockfiles
    // carried the `//`-comment legacy shape.
    let mut legacy_counter = LegacyShapeCounter::default();
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
        let Some((lock, was_legacy_shape)) = lockfile::parse(&bytes) else {
            // Milestone 674 FR-002 (Pants uv-backend fallback): if the
            // PEX parse failed, try to parse as a uv.lock. Modern
            // Pants can use `uv` as its Python resolver backend and
            // generates uv-shape TOML lockfiles at Pants-declared
            // paths. When uv.lock parse succeeds, emit its components
            // with the Pants resolve-name annotation preserved.
            if let Some(uv_entries) =
                crate::scan_fs::package_db::pip::uv_lock::parse_uv_lock_bytes(
                    &bytes,
                    &candidate.path.display().to_string(),
                    &candidate.resolve_name,
                )
            {
                let n = uv_entries.len();
                tracing::info!(
                    lockfile = %candidate.path.display(),
                    packages = n,
                    resolve = %candidate.resolve_name,
                    "uv-lock reader: recognized as uv.lock format after Pex parse rejection"
                );
                components.extend(uv_entries);
                lockfiles_parsed_ok += 1;
                continue;
            }
            // Neither PEX nor uv — genuine parse failure. lockfile::parse
            // already emitted a WARN with the PEX reason;
            // parse_uv_lock_bytes emitted its own WARN (or silent None
            // on non-JSON/TOML). Skip the file.
            tracing::warn!(
                lockfile = %candidate.path.display(),
                "pants-pex reader: parse failed for the file above; skipping"
            );
            lockfiles_skipped_corrupt += 1;
            continue;
        };
        lockfiles_parsed_ok += 1;
        // T007: record legacy-shape lockfiles. The stripper returns a
        // (potentially shortened) slice; `was_legacy_shape` is true
        // iff at least one leading `//` line was consumed. Pass a
        // non-zero sentinel so `record_stripped` increments the count
        // (the counter tracks files, not bytes — see T005 tests).
        if was_legacy_shape {
            legacy_counter.record_stripped(1);
        }
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
        legacy_shape_lockfiles = legacy_counter.as_log_value(),
        components_emitted,
        "pants-pex reader complete"
    );

    components
}

// -------------------------------------------------------------------
// Milestone 672 T005 (foundational): `LegacyShapeCounter` unit tests.
// See data-model.md §"Struct 3" for the contract.
// -------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn legacy_shape_counter_starts_at_zero() {
        let counter = LegacyShapeCounter::default();
        assert_eq!(counter.as_log_value(), 0);
    }

    #[test]
    fn legacy_shape_counter_record_zero_stripped_is_noop() {
        // A clean-JSON file (stripper returns the input slice
        // unchanged) contributes zero bytes stripped → counter
        // stays at zero (contract C4 idempotence guarantee at the
        // stripper level; the counter mirrors that).
        let mut counter = LegacyShapeCounter::default();
        counter.record_stripped(0);
        assert_eq!(counter.as_log_value(), 0);
        counter.record_stripped(0);
        counter.record_stripped(0);
        assert_eq!(counter.as_log_value(), 0);
    }

    #[test]
    fn legacy_shape_counter_record_positive_increments_by_one_not_by_bytes() {
        // The counter tracks lockfile COUNT, not stripped-byte
        // count. Any `record_stripped(N > 0)` call increments by
        // exactly 1 regardless of N.
        let mut counter = LegacyShapeCounter::default();
        counter.record_stripped(4096); // large legacy block
        counter.record_stripped(1); // minimum non-zero
        counter.record_stripped(0); // clean file — skip
        counter.record_stripped(500);
        assert_eq!(
            counter.as_log_value(),
            3,
            "counter must count files (3 non-zero calls), not bytes"
        );
    }
}
