//! Per-host cache layout management for the fingerprint corpus.
//!
//! Cache root resolution (in order):
//!   1. `WAYBILL_FINGERPRINTS_CACHE_DIR` env var (operator override —
//!      useful for CI sandboxes + Docker `COPY` scenarios)
//!   2. `$XDG_CACHE_HOME/waybill/fingerprints/` when `$XDG_CACHE_HOME`
//!      is set (Linux convention)
//!   3. `$HOME/.cache/waybill/fingerprints/` on Unix /
//!      `$USERPROFILE/.cache/waybill/fingerprints/` on Windows
//!
//! Per-SHA layout (full 40-hex SHA as directory key for collision
//! resistance; 12-hex truncation is reserved for the SBOM annotation):
//!
//! ```text
//! <cache-root>/
//!   <full-40-hex-sha-A>/
//!     corpus/
//!       index.json
//!       openssl.json
//!       ...
//!   <full-40-hex-sha-B>/
//!     corpus/...
//! ```
//!
//! See `specs/108-fingerprint-corpus/contracts/cache-layout.md`.

use std::path::PathBuf;

use super::source_config::CorpusSourceId;
use super::source_sha::CorpusSha;

const CACHE_ENV_OVERRIDE: &str = "WAYBILL_FINGERPRINTS_CACHE_DIR";

/// Resolve the cache root per the documented resolution order.
/// Returns the resolved path; does NOT create the directory.
#[allow(dead_code)]
pub(crate) fn cache_root() -> PathBuf {
    if let Some(override_path) = std::env::var_os(CACHE_ENV_OVERRIDE) {
        if !override_path.is_empty() {
            return PathBuf::from(override_path);
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("waybill").join("fingerprints");
        }
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home)
        .join(".cache")
        .join("waybill")
        .join("fingerprints")
}

/// Cache directory for a specific corpus SHA. Full 40-hex SHA in the
/// directory name (the 12-hex truncation is reserved for the SBOM
/// annotation per FR-005).
#[allow(dead_code)]
pub(crate) fn cache_dir_for_sha(sha: &CorpusSha) -> PathBuf {
    cache_root().join(sha.to_full_hex())
}

/// True when the cache directory for the given SHA exists AND contains
/// a `corpus/index.json` file (the minimum required for a load attempt).
/// Returns false on either missing directory OR missing index — the
/// loader will treat either case as a cache miss.
#[allow(dead_code)]
pub(crate) fn cache_hit(sha: &CorpusSha) -> bool {
    cache_dir_for_sha(sha)
        .join("corpus")
        .join("index.json")
        .is_file()
}

// =====================================================================
// Per-source cache layout (milestone 110 Phase 5-Slim PR 2)
// =====================================================================
//
// Milestone-110 sources beyond the milestone-108 default get nested
// `<source-id>/<content-sha>/corpus/` paths. The milestone-108 default
// keeps the flat `<content-sha>/corpus/` layout to preserve existing
// caches — no migration; no re-fetch on upgrade. See the PR-2 commit
// message for the decision rationale.
//
// `<source-id>` comes from `CorpusSourceId::from_url(source.url)`.
// `<content-sha>` is computed at fetch time over the response body
// (research R3 + the operator's locked Source-SHA-Model choice).
// Mixing the two layouts on disk is safe because the per-source prefix
// (a 16-char BASE32 hash) cannot collide with a 40-hex SHA used by
// the milestone-108 layout.

/// Cache directory for a per-source content SHA.
/// Layout: `<cache-root>/<source-id>/<content-sha>/`.
#[allow(dead_code)]
pub(crate) fn cache_dir_for_source(
    source_id: &CorpusSourceId,
    content_sha: &CorpusSha,
) -> PathBuf {
    cache_root()
        .join(source_id.as_str())
        .join(content_sha.to_full_hex())
}

/// True when the per-source cache contains a loadable corpus for the
/// given source + content SHA combination.
#[allow(dead_code)]
pub(crate) fn cache_hit_for_source(
    source_id: &CorpusSourceId,
    content_sha: &CorpusSha,
) -> bool {
    cache_dir_for_source(source_id, content_sha)
        .join("corpus")
        .join("index.json")
        .is_file()
}

/// Operator-explicit cache cleanup (FR-009).
#[allow(dead_code)]
pub(crate) enum KeepRev<'a> {
    All,
    Except(&'a CorpusSha),
}

/// Remove cache directories per the operator's `--keep-rev` selection.
/// Returns the paths of every directory removed (for stdout reporting
/// by the `waybill fingerprints cache-clear` subcommand).
#[allow(dead_code)]
pub(crate) fn cache_clear(keep: KeepRev<'_>) -> std::io::Result<Vec<PathBuf>> {
    let root = cache_root();
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let keep_dir_name = match keep {
        KeepRev::All => None,
        KeepRev::Except(sha) => Some(sha.to_full_hex()),
    };
    let mut removed = Vec::new();
    let read_dir = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(_) => return Ok(Vec::new()),
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(keep_name) = &keep_dir_name {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name == keep_name {
                    continue;
                }
            }
        }
        std::fs::remove_dir_all(&path)?;
        removed.push(path);
    }
    Ok(removed)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const SAMPLE_SHA: &str = "fff39c6ad22ce8420b506323ce1d5cce4b628d5c";
    const ALT_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    // Shared with loader::tests via super::super::test_env_lock so
    // BOTH modules' tests serialize against the same MutexGuard. Per-
    // module Mutex instances would still race across module boundaries.
    use super::super::test_env_lock as env_lock;

    #[test]
    fn cache_root_honors_env_override() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        // SAFETY: env vars are process-global; the env_lock guard
        // serializes against other env-mutating tests in this module.
        unsafe {
            std::env::set_var(CACHE_ENV_OVERRIDE, &path_str);
        }
        assert_eq!(cache_root(), tmp.path());
        unsafe {
            std::env::remove_var(CACHE_ENV_OVERRIDE);
        }
    }

    #[test]
    fn cache_dir_for_sha_uses_full_40_hex() {
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let dir = cache_dir_for_sha(&sha);
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name, SAMPLE_SHA);
        assert_eq!(name.len(), 40);
    }

    #[test]
    fn cache_hit_false_when_directory_absent() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var(CACHE_ENV_OVERRIDE, &path_str);
        }
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        assert!(!cache_hit(&sha));
        unsafe {
            std::env::remove_var(CACHE_ENV_OVERRIDE);
        }
    }

    #[test]
    fn cache_clear_removes_all_when_no_keep() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var(CACHE_ENV_OVERRIDE, &path_str);
        }
        // Populate two SHA dirs.
        std::fs::create_dir_all(tmp.path().join(SAMPLE_SHA)).unwrap();
        std::fs::create_dir_all(tmp.path().join(ALT_SHA)).unwrap();
        let removed = cache_clear(KeepRev::All).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!tmp.path().join(SAMPLE_SHA).exists());
        assert!(!tmp.path().join(ALT_SHA).exists());
        unsafe {
            std::env::remove_var(CACHE_ENV_OVERRIDE);
        }
    }

    // ============================================================
    // Per-source layout tests (milestone 110 Phase 5-Slim PR 2)
    // ============================================================

    #[test]
    fn cache_dir_for_source_nests_source_id_then_sha() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var(CACHE_ENV_OVERRIDE, &path_str);
        }
        let source_id = CorpusSourceId::from_url("https://corpus.example/x.tar.gz");
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let dir = cache_dir_for_source(&source_id, &sha);
        // <root>/<source-id>/<full-sha>/
        let expected = tmp.path().join(source_id.as_str()).join(SAMPLE_SHA);
        assert_eq!(dir, expected);
        unsafe {
            std::env::remove_var(CACHE_ENV_OVERRIDE);
        }
    }

    #[test]
    fn cache_hit_for_source_requires_index_json() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var(CACHE_ENV_OVERRIDE, &path_str);
        }
        let source_id = CorpusSourceId::from_url("https://corpus.example/x.tar.gz");
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        // Missing dir → no hit.
        assert!(!cache_hit_for_source(&source_id, &sha));
        // Dir present but no index.json → still no hit.
        let dir = cache_dir_for_source(&source_id, &sha).join("corpus");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!cache_hit_for_source(&source_id, &sha));
        // index.json present → hit.
        std::fs::write(dir.join("index.json"), r#"{"version":1,"entries":[]}"#).unwrap();
        assert!(cache_hit_for_source(&source_id, &sha));
        unsafe {
            std::env::remove_var(CACHE_ENV_OVERRIDE);
        }
    }

    #[test]
    fn cache_clear_preserves_kept_sha() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var(CACHE_ENV_OVERRIDE, &path_str);
        }
        std::fs::create_dir_all(tmp.path().join(SAMPLE_SHA)).unwrap();
        std::fs::create_dir_all(tmp.path().join(ALT_SHA)).unwrap();
        let keep = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let removed = cache_clear(KeepRev::Except(&keep)).unwrap();
        assert_eq!(removed.len(), 1);
        assert!(tmp.path().join(SAMPLE_SHA).exists());
        assert!(!tmp.path().join(ALT_SHA).exists());
        unsafe {
            std::env::remove_var(CACHE_ENV_OVERRIDE);
        }
    }
}
