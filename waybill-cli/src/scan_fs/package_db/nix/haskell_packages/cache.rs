//! Per-revision cache for retrieved nixpkgs files (milestone 926, #947).
//!
//! Keyed by the exact pinned revision, mirroring the m090 fixture cache,
//! m108 fingerprint cache and m195 corpus cache. A revision is immutable, so
//! unlike m110 — whose source could move and therefore needed a 24-hour TTL —
//! this cache needs **no expiry and no invalidation**. FR-010 and SC-005 fall
//! out of the layout rather than needing mechanism.
//!
//! The retrieved artifact is large (16,634,427 bytes for one revision at the
//! measured rev), which is why caching is a requirement rather than an
//! optimisation.

use std::path::PathBuf;

/// Environment variable redirecting the cache root.
///
/// Exists so tests assert against isolated state instead of the developer's
/// real `$HOME` (T045). Without it, a test asserting "the second scan
/// performs no retrieval" would depend on whatever the developer's cache
/// already held, and would pass or fail for reasons unrelated to the code.
/// Mirrors `WAYBILL_FIXTURE_CACHE` from m090.
pub(crate) const CACHE_ENV: &str = "WAYBILL_NIXPKGS_CACHE";

/// A revision must be a plain hex commit id.
///
/// The revision is interpolated into a filesystem path, so a value containing
/// `..` or a separator would escape the cache directory. Rejecting anything
/// that is not hex is simpler than sanitising, and a non-hex revision is not
/// a git commit anyway.
fn is_valid_revision(rev: &str) -> bool {
    !rev.is_empty()
        && rev.len() <= 64
        && rev.chars().all(|c| c.is_ascii_hexdigit())
}

/// Root directory for the cache, honouring [`CACHE_ENV`].
pub(crate) fn cache_root() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(CACHE_ENV) {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".cache")
            .join("waybill")
            .join("nixpkgs"),
    )
}

/// Path a given file at a given revision occupies in the cache.
///
/// `None` when the revision is not a plain hex id, or no cache root can be
/// determined. A missing cache is not an error: it degrades to re-retrieval.
pub(crate) fn entry_path(rev: &str, file_key: &str) -> Option<PathBuf> {
    if !is_valid_revision(rev) {
        return None;
    }
    // `file_key` is a caller-chosen label, never an attacker-controlled path.
    // Flattened so the cache stays one directory deep per revision.
    let safe_key: String = file_key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    Some(cache_root()?.join(rev).join(safe_key))
}

/// Read a cached file, or `None` when it is not cached.
pub(crate) fn read(rev: &str, file_key: &str) -> Option<String> {
    let path = entry_path(rev, file_key)?;
    std::fs::read_to_string(&path).ok()
}

/// Store a retrieved file.
///
/// Write failures are logged and swallowed: a cache that cannot be written is
/// a slower scan, not a failed one.
pub(crate) fn write(rev: &str, file_key: &str, contents: &str) {
    let Some(path) = entry_path(rev, file_key) else {
        return;
    };
    let Some(parent) = path.parent() else { return };
    if let Err(e) = std::fs::create_dir_all(parent) {
        tracing::debug!(error = %e, dir = %parent.display(), "nixpkgs cache: could not create directory");
        return;
    }
    // Write to a sibling temporary file then rename, so a concurrent reader
    // never observes a half-written package set.
    //
    // The temporary name carries the process id and a counter. A fixed name
    // is not enough: two scans running at once compute the same path, and one
    // can rename a file the other is still writing, leaving a truncated
    // package set in the cache that every later scan then reads. Renames on
    // the same filesystem are atomic, so the last writer simply wins and
    // both observe a complete file.
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nonce = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("partial.{}.{nonce}", std::process::id()));
    if let Err(e) = std::fs::write(&tmp, contents) {
        tracing::debug!(error = %e, path = %tmp.display(), "nixpkgs cache: write failed");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        tracing::debug!(error = %e, path = %path.display(), "nixpkgs cache: rename failed");
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::testing::EnvGuard;

    const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

    #[test]
    fn m926_cache_root_honours_the_override() {
        let _g = EnvGuard::acquire();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var(CACHE_ENV, tmp.path());
        assert_eq!(cache_root().unwrap(), tmp.path());
        std::env::remove_var(CACHE_ENV);
    }

    #[test]
    fn m926_round_trips_through_the_cache() {
        let _g = EnvGuard::acquire();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var(CACHE_ENV, tmp.path());

        assert_eq!(read(REV, "hackage-packages.nix"), None);
        write(REV, "hackage-packages.nix", "contents");
        assert_eq!(read(REV, "hackage-packages.nix").as_deref(), Some("contents"));

        std::env::remove_var(CACHE_ENV);
    }

    /// Revisions are interpolated into a path, so a traversal attempt must
    /// yield no path rather than a path outside the cache.
    #[test]
    fn m926_a_non_hex_revision_is_refused() {
        let _g = EnvGuard::acquire();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var(CACHE_ENV, tmp.path());
        for bad in ["../../etc", "a/b", "", "zzzz", "rev with space"] {
            assert!(entry_path(bad, "f").is_none(), "{bad:?} should be refused");
        }
        std::env::remove_var(CACHE_ENV);
    }

    /// Two revisions of the same file never collide — the point of keying by
    /// revision.
    #[test]
    fn m926_distinct_revisions_do_not_collide() {
        let _g = EnvGuard::acquire();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var(CACHE_ENV, tmp.path());
        let other = "b799d3e3886da994fa307f817a6bc705ae538eeb";
        write(REV, "f", "first");
        write(other, "f", "second");
        assert_eq!(read(REV, "f").as_deref(), Some("first"));
        assert_eq!(read(other, "f").as_deref(), Some("second"));
        std::env::remove_var(CACHE_ENV);
    }
}
