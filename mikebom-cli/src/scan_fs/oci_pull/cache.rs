//! Disk cache for OCI blobs (config + layers) keyed on SHA-256
//! digest (milestone 036 / 031.z — closes #68).
//!
//! OCI distribution-spec blobs are content-addressed: every blob
//! fetched from `/v2/<repo>/blobs/sha256:<hex>` has a hash that
//! matches its content. Caching is therefore correct-by-construction
//! — a cache hit on a digest is always identical to a fresh network
//! fetch of that digest. We re-verify the on-disk hash on every
//! cache read to catch silent corruption (truncation, bit-flip,
//! filesystem damage); on mismatch we delete the entry and fall
//! through to the network.
//!
//! Manifests are intentionally NOT cached: a floating tag like
//! `:latest` should re-fetch the manifest every time so updates are
//! detected. Once the manifest resolves to layer digests, those
//! digests are immutable and cacheable.
//!
//! Layout: `<cache_dir>/sha256/<64-hex>` per blob. Atomic writes
//! via `tempfile::NamedTempFile` + `persist` (intra-fs rename).
//! LRU eviction keyed on file mtime.
//!
//! Cache-dir resolution priority:
//!   1. `$MIKEBOM_OCI_CACHE_DIR`
//!   2. `$XDG_CACHE_HOME/mikebom/oci-layers`
//!   3. macOS: `$HOME/Library/Caches/mikebom/oci-layers`
//!   4. fallback: `$HOME/.cache/mikebom/oci-layers`
//!
//! The cache is "best-effort": any IO failure on open/get/insert
//! falls through to anonymous behavior (cache-miss-style) rather
//! than failing the scan. The user's scan runs to completion; the
//! cache is purely an optimization.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result};
use sha2::Digest as _;

/// SHA-256-content-addressed disk cache for OCI blobs.
pub(super) struct Cache {
    dir: PathBuf,
    size_cap: u64,
}

impl Cache {
    /// Construct a Cache rooted at an explicit directory. Used by
    /// tests in sibling modules (e.g. `registry::tests`) that need
    /// to bypass the env-var resolution chain. Production code goes
    /// through [`Self::open`].
    #[cfg(test)]
    pub(super) fn open_for_test(dir: &std::path::Path, size_cap: u64) -> Self {
        std::fs::create_dir_all(dir.join("sha256"))
            .expect("test cache dir should be creatable");
        Self {
            dir: dir.to_path_buf(),
            size_cap,
        }
    }

    /// Build a cache rooted at the resolved cache directory. Returns
    /// `None` if the directory can't be located, created, or written
    /// to — in which case the caller falls through to no-cache mode.
    pub(super) fn open(size_cap: u64) -> Option<Self> {
        let dir = resolve_cache_dir()?;
        let blob_dir = dir.join("sha256");
        if let Err(e) = fs::create_dir_all(&blob_dir) {
            tracing::warn!(
                cache_dir = %dir.display(),
                error = %e,
                "could not create OCI cache directory; cache disabled"
            );
            return None;
        }
        // Probe-write to verify the directory is writable. A read-only
        // mount or permission-denied path discovers itself here rather
        // than on the first real cache write.
        let probe = dir.join(".mikebom-cache-probe");
        match File::create(&probe).and_then(|mut f| f.write_all(b"ok")) {
            Ok(_) => {
                let _ = fs::remove_file(&probe);
            }
            Err(e) => {
                tracing::warn!(
                    cache_dir = %dir.display(),
                    error = %e,
                    "OCI cache directory is not writable; cache disabled"
                );
                return None;
            }
        }
        Some(Self { dir, size_cap })
    }

    /// Read a cached blob if present. Verifies SHA-256 on read; on
    /// mismatch, deletes the corrupted entry and returns `None`.
    /// Touches the file's mtime so LRU eviction reflects actual use.
    pub(super) fn get(&self, digest: &str) -> Option<Vec<u8>> {
        let path = self.path_for(digest)?;
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::warn!(
                    digest = %digest,
                    error = %e,
                    "could not read OCI cache entry; falling through to network"
                );
                return None;
            }
        };
        let expected_hex = digest.strip_prefix("sha256:")?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        let actual_hex = format!("{:x}", hasher.finalize());
        if !actual_hex.eq_ignore_ascii_case(expected_hex) {
            tracing::warn!(
                digest = %digest,
                "OCI cache entry corrupted (sha256 mismatch); evicting and re-fetching"
            );
            let _ = fs::remove_file(&path);
            return None;
        }
        // Update mtime for LRU. Best-effort; if the FS doesn't
        // support it, the eviction order is just slightly off.
        if let Ok(file) = File::open(&path) {
            let _ = file.set_modified(SystemTime::now());
        }
        tracing::debug!(digest = %digest, bytes = bytes.len(), "OCI blob cache hit");
        Some(bytes)
    }

    /// Insert a blob into the cache. Atomic rename via tempfile.
    /// Triggers eviction if the post-insert total exceeds the cap.
    /// Caller has already verified the bytes match the digest, so
    /// we don't re-hash here.
    pub(super) fn insert(&self, digest: &str, bytes: &[u8]) -> Result<()> {
        let Some(path) = self.path_for(digest) else {
            // Non-sha256 digest — silently no-op. The blob is still
            // returned to the caller; we just don't cache it.
            return Ok(());
        };
        let blob_dir = path
            .parent()
            .expect("path_for always returns <dir>/sha256/<hex>");
        // tempfile-in-same-dir guarantees rename(2) is intra-fs and
        // atomic. Without this, a tempdir on a different mount would
        // fail with EXDEV on persist().
        let mut tmp = tempfile::NamedTempFile::new_in(blob_dir)
            .with_context(|| format!("creating tempfile in {}", blob_dir.display()))?;
        tmp.write_all(bytes)
            .with_context(|| format!("writing tempfile for {digest}"))?;
        tmp.flush().context("flushing tempfile")?;
        tmp.persist(&path).map_err(|e| {
            // PersistError carries the underlying io::Error.
            anyhow::anyhow!(
                "atomic-rename to {} failed: {}",
                path.display(),
                e.error
            )
        })?;
        // Eviction is best-effort: failures here just mean the cache
        // grows past the cap until the next successful insert. Don't
        // propagate.
        if let Err(e) = self.evict_to_cap() {
            tracing::warn!(
                cache_dir = %self.dir.display(),
                error = %e,
                "OCI cache eviction failed; cap may be exceeded until next insert"
            );
        }
        Ok(())
    }

    /// Compute the on-disk path for a digest. Returns `None` for any
    /// non-`sha256:<64-hex>` value — this is both a digest-format
    /// check and a path-traversal guard (rejects `..`, `/`, etc.
    /// that wouldn't pass the hex grammar).
    fn path_for(&self, digest: &str) -> Option<PathBuf> {
        let hex = digest.strip_prefix("sha256:")?;
        if hex.len() != 64 {
            return None;
        }
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(self.dir.join("sha256").join(hex.to_ascii_lowercase()))
    }

    /// Walk `<dir>/sha256/`, sum file sizes, and remove
    /// oldest-mtime entries until the total is at or below the cap.
    fn evict_to_cap(&self) -> Result<()> {
        let blob_dir = self.dir.join("sha256");
        // Collect (path, mtime, size) triples.
        let mut entries: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
        let read_dir = match fs::read_dir(&blob_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "reading cache dir {}: {e}",
                    blob_dir.display()
                ));
            }
        };
        for entry in read_dir.flatten() {
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !metadata.is_file() {
                continue;
            }
            let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            entries.push((entry.path(), mtime, metadata.len()));
        }
        let total: u64 = entries.iter().map(|(_, _, sz)| *sz).sum();
        if total <= self.size_cap {
            return Ok(());
        }
        // Oldest first.
        entries.sort_by_key(|(_, mtime, _)| *mtime);
        let mut current = total;
        for (path, _, size) in &entries {
            if current <= self.size_cap {
                break;
            }
            // Treat "already gone" as success — a concurrent eviction
            // beat us to this file, which is fine.
            match fs::remove_file(path) {
                Ok(_) => {
                    tracing::debug!(path = %path.display(), bytes = *size, "evicted OCI cache entry");
                    current = current.saturating_sub(*size);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    current = current.saturating_sub(*size);
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "could not evict OCI cache entry"
                    );
                }
            }
        }
        Ok(())
    }
}

/// Resolve the OCI blob cache directory per the FR-002 priority
/// chain. Returns `None` only if `$HOME` is unset and no override
/// env var is set — in that case the cache is disabled.
pub(super) fn resolve_cache_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MIKEBOM_OCI_CACHE_DIR") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("mikebom").join("oci-layers"));
        }
    }
    let home = std::env::var("HOME").ok()?;
    if home.is_empty() {
        return None;
    }
    let base = if cfg!(target_os = "macos") {
        PathBuf::from(&home).join("Library").join("Caches")
    } else {
        PathBuf::from(&home).join(".cache")
    };
    Some(base.join("mikebom").join("oci-layers"))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use std::path::Path;
    use std::sync::Mutex;

    use super::*;

    /// Tests that mutate process-wide env vars (`MIKEBOM_OCI_CACHE_DIR`,
    /// `XDG_CACHE_HOME`, `HOME`) must hold this lock — cargo runs
    /// tests in parallel by default and concurrent env-mutation
    /// produces flaky failures.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Build a Cache directly on a tempdir, bypassing the
    /// resolve_cache_dir env-var lookup. Exists for test isolation
    /// — production code goes through `Cache::open`.
    fn cache_in(dir: &Path, size_cap: u64) -> Cache {
        fs::create_dir_all(dir.join("sha256")).unwrap();
        Cache {
            dir: dir.to_path_buf(),
            size_cap,
        }
    }

    fn sha256_of(bytes: &[u8]) -> String {
        let mut hasher = sha2::Sha256::new();
        hasher.update(bytes);
        format!("sha256:{:x}", hasher.finalize())
    }

    #[test]
    fn cold_cache_get_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_in(tmp.path(), 1 << 30);
        let digest = sha256_of(b"hello");
        assert!(cache.get(&digest).is_none());
    }

    #[test]
    fn insert_then_get_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_in(tmp.path(), 1 << 30);
        let bytes = b"hello world".to_vec();
        let digest = sha256_of(&bytes);
        cache.insert(&digest, &bytes).unwrap();
        let got = cache.get(&digest).unwrap();
        assert_eq!(got, bytes);
    }

    #[test]
    fn get_with_corrupt_file_returns_none_and_evicts() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_in(tmp.path(), 1 << 30);
        let bytes = b"hello".to_vec();
        let digest = sha256_of(&bytes);
        cache.insert(&digest, &bytes).unwrap();
        // Corrupt the on-disk bytes.
        let path = cache.path_for(&digest).unwrap();
        fs::write(&path, b"corrupted").unwrap();
        assert!(cache.get(&digest).is_none());
        // The file should have been evicted.
        assert!(!path.exists(), "corrupt entry should be removed");
    }

    #[test]
    fn get_with_non_sha256_digest_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_in(tmp.path(), 1 << 30);
        assert!(cache.get("sha512:0000000000000000000000000000000000000000000000000000000000000000").is_none());
        assert!(cache.get("not-a-digest").is_none());
        assert!(cache.get("sha256:tooshort").is_none());
        // Wrong length but valid hex.
        assert!(cache.get("sha256:abcdef").is_none());
    }

    #[test]
    fn insert_with_non_sha256_digest_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_in(tmp.path(), 1 << 30);
        // Should succeed (Ok(())) without writing anything to disk.
        cache.insert("sha512:00", b"bytes").unwrap();
        cache.insert("not-a-digest", b"bytes").unwrap();
        // No files in the sha256 subdir.
        let count = fs::read_dir(tmp.path().join("sha256"))
            .unwrap()
            .count();
        assert_eq!(count, 0);
    }

    #[test]
    fn path_for_rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_in(tmp.path(), 1 << 30);
        // Hex grammar check rejects `..`, `/`, etc.
        assert!(cache
            .path_for("sha256:../../../etc/passwd")
            .is_none());
        assert!(cache.path_for("sha256:/etc/passwd").is_none());
    }

    #[test]
    fn eviction_removes_oldest_when_over_cap() {
        let tmp = tempfile::tempdir().unwrap();
        // Cap = 600 bytes. Insert three 250-byte blobs in sequence.
        // After the third, total = 750; oldest must be evicted.
        let cache = cache_in(tmp.path(), 600);

        let bytes_a = vec![b'a'; 250];
        let bytes_b = vec![b'b'; 250];
        let bytes_c = vec![b'c'; 250];
        let digest_a = sha256_of(&bytes_a);
        let digest_b = sha256_of(&bytes_b);
        let digest_c = sha256_of(&bytes_c);

        cache.insert(&digest_a, &bytes_a).unwrap();
        // Sleep enough that mtimes are distinguishable on filesystems
        // with 1-second mtime resolution (ext4-default, APFS).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        cache.insert(&digest_b, &bytes_b).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        cache.insert(&digest_c, &bytes_c).unwrap();

        // a was inserted first → oldest mtime → evicted.
        assert!(cache.get(&digest_a).is_none(), "oldest should be evicted");
        assert!(cache.get(&digest_b).is_some(), "newer should remain");
        assert!(cache.get(&digest_c).is_some(), "newest should remain");
    }

    #[test]
    fn concurrent_inserts_do_not_corrupt_final_file() {
        // 8 threads each insert the same blob 40 times. Final file
        // must be intact (correct size, hash matches).
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache_in(tmp.path(), 1 << 30);
        let bytes = vec![b'x'; 4096];
        let digest = sha256_of(&bytes);

        std::thread::scope(|scope| {
            for _ in 0..8 {
                let bytes = bytes.clone();
                let digest = digest.clone();
                let cache_ref = &cache;
                scope.spawn(move || {
                    for _ in 0..40 {
                        cache_ref.insert(&digest, &bytes).unwrap();
                    }
                });
            }
        });

        let got = cache.get(&digest).unwrap();
        assert_eq!(got, bytes, "concurrent writes should not corrupt the final blob");
    }

    #[test]
    fn open_returns_some_for_writable_dir() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let saved = std::env::var("MIKEBOM_OCI_CACHE_DIR").ok();
        std::env::set_var("MIKEBOM_OCI_CACHE_DIR", tmp.path());
        let cache = Cache::open(1 << 30);
        if let Some(v) = saved {
            std::env::set_var("MIKEBOM_OCI_CACHE_DIR", v);
        } else {
            std::env::remove_var("MIKEBOM_OCI_CACHE_DIR");
        }
        assert!(cache.is_some());
    }

    #[test]
    fn open_returns_none_when_dir_is_unresolvable() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved_override = std::env::var("MIKEBOM_OCI_CACHE_DIR").ok();
        let saved_xdg = std::env::var("XDG_CACHE_HOME").ok();
        let saved_home = std::env::var("HOME").ok();
        std::env::remove_var("MIKEBOM_OCI_CACHE_DIR");
        std::env::remove_var("XDG_CACHE_HOME");
        std::env::remove_var("HOME");
        let result = resolve_cache_dir();
        if let Some(v) = saved_override {
            std::env::set_var("MIKEBOM_OCI_CACHE_DIR", v);
        }
        if let Some(v) = saved_xdg {
            std::env::set_var("XDG_CACHE_HOME", v);
        }
        if let Some(v) = saved_home {
            std::env::set_var("HOME", v);
        }
        assert!(result.is_none());
    }

    #[test]
    fn resolve_cache_dir_uses_override_first() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("MIKEBOM_OCI_CACHE_DIR").ok();
        std::env::set_var("MIKEBOM_OCI_CACHE_DIR", "/custom/cache/path");
        let resolved = resolve_cache_dir().unwrap();
        if let Some(v) = saved {
            std::env::set_var("MIKEBOM_OCI_CACHE_DIR", v);
        } else {
            std::env::remove_var("MIKEBOM_OCI_CACHE_DIR");
        }
        assert_eq!(resolved, PathBuf::from("/custom/cache/path"));
    }
}
