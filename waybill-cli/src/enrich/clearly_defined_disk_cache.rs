//! Persistent (cross-scan) cache for ClearlyDefined `/definitions/...`
//! responses (#137 part 2/2).
//!
//! Cache layout mirrors the milestone-108 fingerprint corpus pattern
//! at `binary/fingerprints/cache.rs::cache_root`:
//!
//! Cache root resolution order:
//!   1. `WAYBILL_CLEARLY_DEFINED_CACHE_DIR` env var (operator override)
//!   2. `$XDG_CACHE_HOME/waybill/clearly-defined/` (Linux convention)
//!   3. `$HOME/.cache/waybill/clearly-defined/` (Unix) /
//!      `$USERPROFILE/.cache/waybill/clearly-defined/` (Windows)
//!
//! Per-coord entries land in flat files keyed on the SHA-256 of the
//! coord's URL path, truncated to 32 hex chars (128 bits of collision
//! resistance is more than enough — the keyspace is small and the
//! consequence of a collision is one wrong license that gets corrected
//! on the next live fetch).
//!
//! File schema:
//!
//! ```json
//! {
//!   "v": 1,
//!   "coord_url_path": "npm/npmjs/-/express/4.18.2",
//!   "fetched_at": 1740000000,
//!   "definition": { "declared_license": "MIT" }
//! }
//! ```
//!
//! `definition` may be `null` — that records a confirmed CD miss
//! (404), avoiding a re-fetch on every scan until the TTL expires.
//!
//! `coord_url_path` is stored alongside the value so a human inspecting
//! the cache file can correlate the hash back to its source, and so a
//! tooling-side correctness check can detect a SHA-256 collision (the
//! reader compares the stored URL path against the current coord's URL
//! path; mismatch ⇒ treat as cache miss).
//!
//! Writes go through `tempfile::NamedTempFile::new_in` + atomic-rename
//! via `persist`. The milestone-100 `oci_pull/cache::insert` pattern
//! (idempotent retry on Windows `MoveFileExW` ACCESS_DENIED) is
//! transferable but unnecessary here — CD entries are content-derived
//! and overwriting a stale entry with a fresher one is fine.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use super::clearly_defined_client::CdDefinition;
use super::clearly_defined_coord::CdCoord;

const CACHE_ENV_OVERRIDE: &str = "WAYBILL_CLEARLY_DEFINED_CACHE_DIR";
const DISABLE_ENV: &str = "WAYBILL_CLEARLY_DEFINED_NO_CACHE";
/// 7-day TTL. CD definitions change rarely enough that a week
/// out-of-date entry is fine; tune later if license curation lag
/// observed in the field is higher than this.
const DEFAULT_TTL_SECS: u64 = 7 * 24 * 60 * 60;
const SCHEMA_VERSION: u32 = 1;

/// On-disk cache for CD `/definitions` responses. Cheap to clone — the
/// `root` `PathBuf` is the only owned state; the `Arc<Self>` wrapping
/// at the source layer keeps the cost of fan-out clones at O(1).
///
/// `root == None` means the cache is **disabled** — either the
/// operator opted out via `WAYBILL_CLEARLY_DEFINED_NO_CACHE`, or no
/// writable cache directory could be resolved. Disabled-cache reads
/// always miss and writes are no-ops.
#[derive(Clone)]
pub struct CdDiskCache {
    root: Option<PathBuf>,
    ttl: Duration,
}

impl CdDiskCache {
    /// Resolve the cache root per the documented order; honour the
    /// disable env var. Does not create the directory — `put` is
    /// responsible for that (lazy creation so a read-only home dir
    /// doesn't fail at startup).
    pub fn open() -> Arc<Self> {
        let root = if std::env::var_os(DISABLE_ENV).is_some_and(|v| !v.is_empty()) {
            debug!("ClearlyDefined disk cache disabled via env var");
            None
        } else {
            resolve_cache_root()
        };
        Arc::new(Self {
            root,
            ttl: Duration::from_secs(DEFAULT_TTL_SECS),
        })
    }

    /// Read the cached definition for a coord.
    ///
    /// Outer return: `None` ⇒ cache miss (either no entry, expired, or
    /// disabled / unreadable). `Some(inner)` ⇒ cache hit; the inner
    /// `Option<CdDefinition>` is the recorded answer (with `None`
    /// meaning "CD answered 404"; mirrors the in-memory cache's
    /// `HashMap<CdCoord, Option<CdDefinition>>` shape).
    pub fn get(&self, coord: &CdCoord) -> Option<Option<CdDefinition>> {
        let root = self.root.as_ref()?;
        let file = root.join(entry_filename(coord));
        let bytes = std::fs::read(&file).ok()?;
        let entry: DiskEntry = match serde_json::from_slice(&bytes) {
            Ok(e) => e,
            Err(e) => {
                debug!(
                    file = %file.display(),
                    error = %e,
                    "CD disk-cache entry corrupted — treating as miss"
                );
                return None;
            }
        };
        if entry.v != SCHEMA_VERSION {
            // Schema-version mismatch: future-versioned readers can
            // upgrade; today we just miss.
            return None;
        }
        // SHA-256 collision guard: if the stored coord URL path
        // doesn't match the current coord's, this entry was written
        // for a *different* coord that happened to hash to the same
        // 32-hex prefix. Don't return the wrong definition.
        if entry.coord_url_path != coord.url_path() {
            warn!(
                stored = %entry.coord_url_path,
                wanted = %coord.url_path(),
                "CD disk-cache hash collision — refusing stale entry"
            );
            return None;
        }
        let fetched_at = UNIX_EPOCH + Duration::from_secs(entry.fetched_at);
        let now = SystemTime::now();
        let age = now.duration_since(fetched_at).unwrap_or(Duration::ZERO);
        if age > self.ttl {
            debug!(
                age_secs = age.as_secs(),
                ttl_secs = self.ttl.as_secs(),
                "CD disk-cache entry expired"
            );
            return None;
        }
        Some(entry.definition)
    }

    /// Write a definition (or recorded miss) to disk. Best-effort —
    /// failures are logged and dropped. Caller has already updated
    /// the in-memory cache; the disk write only affects future scans.
    pub fn put(&self, coord: &CdCoord, definition: &Option<CdDefinition>) {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        if let Err(e) = std::fs::create_dir_all(root) {
            debug!(
                root = %root.display(),
                error = %e,
                "CD disk-cache mkdir failed — writes disabled this scan"
            );
            return;
        }
        let entry = DiskEntry {
            v: SCHEMA_VERSION,
            coord_url_path: coord.url_path(),
            fetched_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            definition: definition.clone(),
        };
        let serialized = match serde_json::to_vec(&entry) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "CD disk-cache serialize failed");
                return;
            }
        };
        let target = root.join(entry_filename(coord));
        if let Err(e) = atomic_write(&target, &serialized) {
            debug!(
                file = %target.display(),
                error = %e,
                "CD disk-cache atomic write failed"
            );
        }
    }
}

/// Hash the coord's URL path with SHA-256 and take the first 32 hex
/// chars (128 bits). The output is a stable, filesystem-safe filename.
fn entry_filename(coord: &CdCoord) -> String {
    let mut hasher = Sha256::new();
    hasher.update(coord.url_path().as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(32 + 5);
    for byte in digest.iter().take(16) {
        out.push_str(&format!("{byte:02x}"));
    }
    out.push_str(".json");
    out
}

/// Resolve the cache root per the documented fallback order. Returns
/// `None` only when no candidate path can be derived (no env vars and
/// no home dir).
fn resolve_cache_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(CACHE_ENV_OVERRIDE) {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("waybill").join("clearly-defined"));
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    if home.is_empty() {
        return None;
    }
    Some(
        PathBuf::from(home)
            .join(".cache")
            .join("waybill")
            .join("clearly-defined"),
    )
}

/// Atomic write via tempfile in the same directory + persist (rename).
/// Same pattern as `oci_pull/cache::insert`; CD entries are content
/// derived so overwriting a stale entry with a fresher one is fine.
fn atomic_write(target: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cache file path has no parent dir"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    tmp.persist(target)
        .map_err(|e| anyhow::anyhow!("persist failed: {}", e.error))?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct DiskEntry {
    v: u32,
    coord_url_path: String,
    /// Unix-epoch seconds.
    fetched_at: u64,
    definition: Option<CdDefinition>,
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn coord(name: &str) -> CdCoord {
        CdCoord {
            cd_type: "npm".to_string(),
            provider: "npmjs".to_string(),
            namespace: "-".to_string(),
            name: name.to_string(),
            revision: "1.0.0".to_string(),
        }
    }

    fn cache_in(dir: &std::path::Path) -> CdDiskCache {
        CdDiskCache {
            root: Some(dir.to_path_buf()),
            ttl: Duration::from_secs(DEFAULT_TTL_SECS),
        }
    }

    #[test]
    fn put_then_get_roundtrips_definition() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());
        let c = coord("express");
        let def = Some(CdDefinition {
            declared_license: Some("MIT".to_string()),
        });
        cache.put(&c, &def);
        assert_eq!(cache.get(&c), Some(def));
    }

    #[test]
    fn put_then_get_roundtrips_recorded_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());
        let c = coord("never-existed");
        cache.put(&c, &None);
        // Outer Some(...) = cache hit; inner None = recorded miss
        assert_eq!(cache.get(&c), Some(None));
    }

    #[test]
    fn get_returns_none_for_unknown_coord() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());
        assert_eq!(cache.get(&coord("unknown")), None);
    }

    #[test]
    fn distinct_coords_produce_distinct_files() {
        // Smoke test the collision-resistance assumption: two
        // distinct coords land in two distinct cache files.
        assert_ne!(
            entry_filename(&coord("express")),
            entry_filename(&coord("lodash"))
        );
    }

    #[test]
    fn corrupted_entry_returns_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());
        let c = coord("express");
        let path = dir.path().join(entry_filename(&c));
        std::fs::write(&path, b"{not valid json").unwrap();
        assert_eq!(cache.get(&c), None);
    }

    #[test]
    fn disabled_cache_misses_and_drops_writes() {
        let cache = CdDiskCache {
            root: None,
            ttl: Duration::from_secs(DEFAULT_TTL_SECS),
        };
        let c = coord("express");
        let def = Some(CdDefinition {
            declared_license: Some("MIT".to_string()),
        });
        cache.put(&c, &def);
        assert_eq!(cache.get(&c), None);
    }

    #[test]
    fn expired_entry_is_treated_as_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CdDiskCache {
            root: Some(dir.path().to_path_buf()),
            ttl: Duration::from_secs(60), // 60-sec TTL for the test
        };
        let c = coord("express");
        // Hand-write an entry with fetched_at = 2 hours ago.
        let old_entry = DiskEntry {
            v: SCHEMA_VERSION,
            coord_url_path: c.url_path(),
            fetched_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(2 * 60 * 60),
            definition: Some(CdDefinition {
                declared_license: Some("MIT".to_string()),
            }),
        };
        let path = dir.path().join(entry_filename(&c));
        std::fs::write(&path, serde_json::to_vec(&old_entry).unwrap()).unwrap();
        assert_eq!(cache.get(&c), None);
    }

    #[test]
    fn schema_version_mismatch_returns_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());
        let c = coord("express");
        // Write an entry with v=999 (future version we don't know).
        let entry = serde_json::json!({
            "v": 999,
            "coord_url_path": c.url_path(),
            "fetched_at": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            "definition": null,
        });
        let path = dir.path().join(entry_filename(&c));
        std::fs::write(&path, serde_json::to_vec(&entry).unwrap()).unwrap();
        assert_eq!(cache.get(&c), None);
    }
}
