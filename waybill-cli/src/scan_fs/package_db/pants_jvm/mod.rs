//! Milestone 224: Pants coursier JVM lockfile reader — orchestrator entry.
//!
//! Discovers Pants-generated coursier lockfiles under
//! `3rdparty/jvm/*.lock` (default glob) plus any paths declared via
//! `pants.toml` `[jvm.resolves]`. Parses each, discriminates Pants-
//! generated files from standalone coursier via the FR-011 header
//! substring, and emits one `PackageDbEntry` per locked distribution.
//!
//! Fail-open per contract: per-file corruption logs WARN and is
//! skipped; standalone coursier lockfiles log INFO and are skipped;
//! the whole scan never aborts on lockfile issues.
//!
//! See `specs/224-pants-coursier-jvm/` for spec + plan + contracts.

pub mod config;
pub mod coordinate;
pub mod lockfile;
pub mod resolve_classifier;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::PackageDbEntry;
use lockfile::SkipReason;

// Milestone 664 US2 T045: shared-walker marker-detect registration.
use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext,
};

/// Discovered lockfile candidate — absolute path plus the resolve
/// name to tag its components with. The `resolve_name` comes from
/// either the filename stem (`3rdparty/jvm/junit.lock` → `junit`) or
/// the `[jvm.resolves]` config-declared key (config wins on tie).
struct DiscoveredLockfile {
    path: PathBuf,
    resolve_name: String,
}

/// Enumerate lockfile candidates: default `3rdparty/jvm/*.lock` glob
/// PLUS every `[jvm.resolves]`-declared path that exists on disk. A
/// malformed / unreadable `pants.toml` gracefully falls through per
/// FR-004 (the reader keeps the default glob's discoveries).
fn discover_lockfiles(scan_root: &Path) -> Vec<DiscoveredLockfile> {
    let mut out: Vec<DiscoveredLockfile> = Vec::new();

    // Default glob: 3rdparty/jvm/*.lock.
    let default_dir = scan_root.join("3rdparty").join("jvm");
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

    // pants.toml override: [jvm.resolves].
    let pants_toml = scan_root.join("pants.toml");
    if pants_toml.exists() {
        match std::fs::read(&pants_toml) {
            Ok(bytes) => match config::parse(&bytes) {
                Some(cfg) => {
                    for (name, rel_path) in &cfg.jvm.resolves {
                        let resolved = scan_root.join(rel_path);
                        if !resolved.exists() {
                            tracing::warn!(
                                pants_toml = %pants_toml.display(),
                                resolve = %name,
                                declared_path = %rel_path,
                                "pants-coursier-jvm reader: [jvm.resolves] path does not exist on disk; skipping that resolve"
                            );
                            continue;
                        }
                        // Config-declared name wins if the same path
                        // was already discovered by the default glob.
                        if let Some(existing) =
                            out.iter_mut().find(|d| d.path == resolved)
                        {
                            existing.resolve_name = name.clone();
                        } else {
                            out.push(DiscoveredLockfile {
                                path: resolved,
                                resolve_name: name.clone(),
                            });
                        }
                    }
                }
                None => {
                    tracing::warn!(
                        pants_toml = %pants_toml.display(),
                        "pants-coursier-jvm reader: pants.toml could not be parsed as TOML; falling back to default glob"
                    );
                }
            },
            Err(e) => {
                tracing::warn!(
                    pants_toml = %pants_toml.display(),
                    error = %e,
                    "pants-coursier-jvm reader: pants.toml could not be read; falling back to default glob"
                );
            }
        }
    }

    out
}

/// Milestone 664 US2 T045: marker-detect state. The pants_jvm reader
/// has no tree walker of its own (fixed-root scan), so its shared-walker
/// registration exists only to record whether ANY pants signal was seen
/// during the single-pass descent. Finalize gates the O(1) `read()` on
/// this flag (plus a defensive fs-existence fallback) so repos with no
/// pants presence save two fs syscalls per scan.
#[derive(Default, Debug)]
pub(crate) struct PantsJvmMarkerState {
    pub(crate) seen: bool,
}

/// Per-file callback. Marks `seen = true` iff either:
///   - basename == `pants.toml` (definitive pants signal), OR
///   - basename ends in `.lock` AND the immediate parent dir is `jvm`
///     with `3rdparty/` anywhere in the ancestor chain (US3 T024
///     fixture: `3rdparty/jvm/*.lock` without a `pants.toml`).
fn on_pants_jvm_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    let Some(state) = ctx.state::<Mutex<PantsJvmMarkerState>>(ReaderId::PANTS_JVM) else {
        return;
    };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let matches = if name == "pants.toml" {
        true
    } else if name.ends_with(".lock") {
        let parent_is_jvm = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("jvm");
        let under_3rdparty = path
            .ancestors()
            .filter_map(|a| a.file_name().and_then(|n| n.to_str()))
            .any(|n| n == "3rdparty");
        parent_is_jvm && under_3rdparty
    } else {
        false
    };
    if !matches {
        return;
    }
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.seen = true;
}

/// Build the `ReaderRegistration`. Matches `pants.toml` + `*.lock` at
/// basename level; the callback's path filter narrows `.lock` matches
/// to the `3rdparty/jvm/` subtree so unrelated lockfiles (Cargo.lock,
/// Gemfile.lock, uv.lock, ...) don't trip the flag.
pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&["**/pants.toml", "**/*.lock"])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::PANTS_JVM,
        state: Some(Arc::new(Mutex::new(PantsJvmMarkerState::default()))),
        patterns,
        on_file: Some(on_pants_jvm_file),
        on_dir: None,
        descend_into: None,
    })
}

/// Extract the marker flag out of the registration's state slot.
pub(crate) fn extract_marker(registration: &ReaderRegistration) -> PantsJvmMarkerState {
    let Some(state_arc) = registration.state.as_ref() else {
        return PantsJvmMarkerState::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<PantsJvmMarkerState>>() else {
        return PantsJvmMarkerState::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Post-walker entry — gates the O(1) read on marker presence with a
/// defensive fs-existence fallback for pathological layouts (e.g. a
/// walker exclusion that skipped `3rdparty/`). Preserves FR-006
/// byte-identity: any repo that previously emitted pants-jvm components
/// will still emit them via at least one signal path.
pub(crate) fn finalize(
    marker: PantsJvmMarkerState,
    scan_root: &Path,
) -> Vec<PackageDbEntry> {
    if !marker.seen && !scan_root.join("3rdparty").join("jvm").is_dir() {
        return Vec::new();
    }
    read(scan_root)
}

/// Public entry — orchestrates lockfile discovery + parsing + entry
/// emission. Returns `Vec::new()` when no lockfiles are found (and
/// emits no log line — preserves byte-identity for non-Pants-JVM
/// repos per FR-007 / SC-003).
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry> {
    let candidates = discover_lockfiles(scan_root);
    if candidates.is_empty() {
        return Vec::new();
    }

    let lockfiles_discovered = candidates.len();
    let mut lockfiles_parsed_ok: usize = 0;
    let mut lockfiles_skipped_corrupt: usize = 0;
    let mut lockfiles_skipped_non_pants: usize = 0;
    let mut components: Vec<PackageDbEntry> = Vec::new();

    for candidate in &candidates {
        let bytes = match std::fs::read(&candidate.path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    lockfile = %candidate.path.display(),
                    error = %e,
                    "pants-coursier-jvm reader: could not read lockfile bytes; skipping"
                );
                lockfiles_skipped_corrupt += 1;
                continue;
            }
        };
        let lock = match lockfile::parse(&bytes) {
            Ok(l) => l,
            Err(SkipReason::NotPants) => {
                tracing::info!(
                    lockfile = %candidate.path.display(),
                    "pants-coursier-jvm reader: not a Pants-generated coursier lockfile; skipping"
                );
                lockfiles_skipped_non_pants += 1;
                continue;
            }
            Err(SkipReason::MetadataInvalid(msg)) => {
                tracing::warn!(
                    lockfile = %candidate.path.display(),
                    error = %msg,
                    "pants-coursier-jvm reader: Pants metadata invalid; skipping"
                );
                lockfiles_skipped_corrupt += 1;
                continue;
            }
            Err(SkipReason::TomlParseError(msg)) => {
                tracing::warn!(
                    lockfile = %candidate.path.display(),
                    error = %msg,
                    "pants-coursier-jvm reader: coursier TOML body parse error; skipping"
                );
                lockfiles_skipped_corrupt += 1;
                continue;
            }
        };
        lockfiles_parsed_ok += 1;
        for entry in &lock.entries {
            if let Some(pkg) = lockfile::entry_to_package_db_entry(
                entry,
                &candidate.path,
                &candidate.resolve_name,
            ) {
                components.push(pkg);
            }
        }
    }

    let components_emitted = components.len();
    tracing::info!(
        lockfiles_discovered,
        lockfiles_parsed_ok,
        lockfiles_skipped_corrupt,
        lockfiles_skipped_non_pants,
        components_emitted,
        "pants-coursier-jvm reader complete"
    );

    components
}
