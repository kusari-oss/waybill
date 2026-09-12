//! Milestone 839 (#766) — on-disk cache for deps.dev version metadata.
//!
//! Mechanics ported from `clearly_defined_disk_cache`, which already
//! solved recorded negatives, corrupt-entry handling, hash-collision
//! refusal and best-effort writes. One thing differs, and it is the
//! reason this is a separate module rather than a reuse:
//!
//! **Freshness is per entry, not global.** The sibling cache applies
//! one 7-day TTL to everything. deps.dev publishes its own policy on
//! every response (`Cache-Control: max-age=3600`), so each entry
//! carries the bound that applied when it was fetched. If deps.dev
//! changes its policy, old entries keep the bound they were written
//! under and new ones pick up the new one, with no migration.
//!
//! Storing it per entry is also what makes the operator override
//! honest: `--enrich-cache-max-age` raises the bound written into new
//! entries rather than overriding the check at read time, so a
//! decision to accept staleness is recorded in the data instead of
//! being applied invisibly on every later read.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

use super::deps_dev_client::VersionInfo;
use super::request_key::EnrichmentKey;

const CACHE_ENV_OVERRIDE: &str = "WAYBILL_DEPS_DEV_CACHE_DIR";
const DISABLE_ENV: &str = "WAYBILL_DEPS_DEV_NO_CACHE";
const SCHEMA_VERSION: u32 = 1;

/// Applied when a response carries no parseable `Cache-Control`.
/// One hour matches what deps.dev actually sends today, so the
/// fallback behaves like the common case rather than inventing one.
pub const DEFAULT_MAX_AGE_SECS: u64 = 3600;

#[derive(Serialize, Deserialize)]
struct DiskEntry {
    v: u32,
    /// The full key, stored to catch a hash collision. Without it a
    /// truncated-digest filename collision would serve one package's
    /// licence as another's.
    key: String,
    /// Unix-epoch seconds.
    fetched_at: u64,
    /// Freshness bound that applied at fetch time.
    max_age_secs: u64,
    /// `None` records a confirmed absence — deps.dev has no data for
    /// this coordinate. Cached like a hit and expiring like one,
    /// because without it the long tail of packages deps.dev does not
    /// carry is re-requested every scan, and on a large repository
    /// that tail is most of the traffic this feature exists to remove.
    record: Option<VersionInfo>,
}

/// `root == None` means disabled: every read misses, every write is a
/// no-op, and no call site needs to special-case it.
#[derive(Clone)]
pub struct DepsDevDiskCache {
    root: Option<PathBuf>,
    /// Operator override (`--enrich-cache-max-age`). Applied at fetch
    /// time to the bound written into new entries — never at read
    /// time (contract C-7.5).
    override_max_age: Option<u64>,
}

impl DepsDevDiskCache {
    pub fn open(enabled: bool, override_max_age: Option<u64>) -> Arc<Self> {
        let root = if !enabled {
            debug!("deps.dev disk cache disabled for this scan");
            None
        } else if std::env::var_os(DISABLE_ENV).is_some_and(|v| !v.is_empty()) {
            debug!("deps.dev disk cache disabled via env var");
            None
        } else {
            resolve_cache_root()
        };
        Arc::new(Self {
            root,
            override_max_age,
        })
    }

    /// The bound to write into a new entry, given what the response
    /// asked for.
    pub fn effective_max_age(&self, response_max_age: Option<u64>) -> u64 {
        self.override_max_age
            .unwrap_or_else(|| response_max_age.unwrap_or(DEFAULT_MAX_AGE_SECS))
    }

    /// `None` ⇒ miss (absent, expired, corrupt, colliding or
    /// disabled). `Some(inner)` ⇒ hit, where `inner == None` is a
    /// recorded absence.
    pub fn get(&self, key: &EnrichmentKey) -> Option<Option<VersionInfo>> {
        let root = self.root.as_ref()?;
        let file = root.join(entry_filename(key));
        let bytes = std::fs::read(&file).ok()?;
        let entry: DiskEntry = match serde_json::from_slice(&bytes) {
            Ok(e) => e,
            Err(e) => {
                // C-8.1: corrupt is a miss, never an error. A cache
                // that can fail a scan is worse than no cache.
                debug!(file = %file.display(), error = %e,
                       "deps.dev disk-cache entry corrupted — treating as miss");
                return None;
            }
        };
        if entry.v != SCHEMA_VERSION {
            return None;
        }
        if entry.key != key.cache_key() {
            warn!("deps.dev disk-cache hash collision — refusing entry");
            return None;
        }
        let age = SystemTime::now()
            .duration_since(UNIX_EPOCH + Duration::from_secs(entry.fetched_at))
            .unwrap_or(Duration::ZERO);
        if age.as_secs() >= entry.max_age_secs {
            debug!(age_secs = age.as_secs(), max_age_secs = entry.max_age_secs,
                   "deps.dev disk-cache entry past its freshness bound");
            return None;
        }
        Some(entry.record)
    }

    /// Best-effort write. A read-only or full cache directory degrades
    /// speed, never correctness (C-8.2).
    pub fn put(&self, key: &EnrichmentKey, record: &Option<VersionInfo>, max_age_secs: u64) {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        if std::fs::create_dir_all(root).is_err() {
            return;
        }
        let entry = DiskEntry {
            v: SCHEMA_VERSION,
            key: key.cache_key(),
            fetched_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            max_age_secs,
            record: record.clone(),
        };
        let Ok(bytes) = serde_json::to_vec(&entry) else {
            return;
        };
        // C-8.3: write-then-rename. An interrupted scan must not leave
        // a half-written entry that a later scan would parse as valid
        // — the truncation could land mid-licence and still be
        // syntactically plausible JSON.
        let final_path = root.join(entry_filename(key));
        let tmp = final_path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, &bytes).is_ok() && std::fs::rename(&tmp, &final_path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.root.is_some()
    }
}

fn entry_filename(key: &EnrichmentKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.cache_key().as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(37);
    for byte in digest.iter().take(16) {
        out.push_str(&format!("{byte:02x}"));
    }
    out.push_str(".json");
    out
}

fn resolve_cache_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(CACHE_ENV_OVERRIDE) {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("waybill").join("deps-dev"));
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
            .join("deps-dev"),
    )
}

/// Parse `max-age` out of a `Cache-Control` header value.
pub fn parse_max_age(header: Option<&str>) -> Option<u64> {
    let h = header?;
    for part in h.split(',') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("max-age=") {
            return v.trim().parse().ok();
        }
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn key(n: &str) -> EnrichmentKey {
        EnrichmentKey::from_purl_parts("cargo", None, n, "1.0.0").unwrap()
    }
    fn info(l: &str) -> Option<VersionInfo> {
        Some(VersionInfo {
            licenses: vec![l.to_string()],
            links: vec![],
        })
    }
    /// C-8.5 / Constitution VII. Every test points at its own temp
    /// directory. A test touching the real `$HOME` shares mutable
    /// state with the developer's own scans and with every other test.
    fn cache_in(dir: &std::path::Path, override_max_age: Option<u64>) -> DepsDevDiskCache {
        DepsDevDiskCache {
            root: Some(dir.to_path_buf()),
            override_max_age,
        }
    }

    #[test]
    fn roundtrips_a_hit() {
        let d = tempfile::tempdir().unwrap();
        let c = cache_in(d.path(), None);
        c.put(&key("serde"), &info("MIT"), 3600);
        assert_eq!(c.get(&key("serde")).unwrap().unwrap().licenses, vec!["MIT"]);
    }

    #[test]
    fn roundtrips_a_recorded_absence() {
        // C-7.6. Without this the long tail of packages deps.dev does
        // not carry is re-requested on every scan.
        let d = tempfile::tempdir().unwrap();
        let c = cache_in(d.path(), None);
        c.put(&key("nonexistent"), &None, 3600);
        let hit = c.get(&key("nonexistent"));
        assert!(hit.is_some(), "the absence itself is a cache hit");
        assert!(hit.unwrap().is_none(), "and the recorded answer is 'no data'");
    }

    #[test]
    fn entry_past_its_bound_is_a_miss() {
        // C-7.1/C-7.4. Written with a one-hour bound, two hours ago.
        let d = tempfile::tempdir().unwrap();
        let c = cache_in(d.path(), None);
        let k = key("stale");
        let entry = DiskEntry {
            v: SCHEMA_VERSION,
            key: k.cache_key(),
            fetched_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 7200,
            max_age_secs: 3600,
            record: info("MIT"),
        };
        std::fs::write(
            d.path().join(entry_filename(&k)),
            serde_json::to_vec(&entry).unwrap(),
        )
        .unwrap();
        assert!(c.get(&k).is_none(), "two hours old under a one-hour bound");
    }

    #[test]
    fn entry_inside_its_bound_is_a_hit() {
        let d = tempfile::tempdir().unwrap();
        let c = cache_in(d.path(), None);
        let k = key("fresh");
        let entry = DiskEntry {
            v: SCHEMA_VERSION,
            key: k.cache_key(),
            fetched_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 1800,
            max_age_secs: 3600,
            record: info("MIT"),
        };
        std::fs::write(
            d.path().join(entry_filename(&k)),
            serde_json::to_vec(&entry).unwrap(),
        )
        .unwrap();
        assert!(c.get(&k).is_some(), "thirty minutes old under a one-hour bound");
    }

    #[test]
    fn per_entry_bound_means_a_policy_change_needs_no_migration() {
        // C-7.3. Two entries written under different bounds; each is
        // judged by its own.
        let d = tempfile::tempdir().unwrap();
        let c = cache_in(d.path(), None);
        for (name, max_age, age) in [("short", 60u64, 120u64), ("long", 86400, 120)] {
            let k = key(name);
            let entry = DiskEntry {
                v: SCHEMA_VERSION,
                key: k.cache_key(),
                fetched_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    - age,
                max_age_secs: max_age,
                record: info("MIT"),
            };
            std::fs::write(
                d.path().join(entry_filename(&k)),
                serde_json::to_vec(&entry).unwrap(),
            )
            .unwrap();
        }
        assert!(c.get(&key("short")).is_none(), "60s bound, 120s old");
        assert!(c.get(&key("long")).is_some(), "86400s bound, 120s old");
    }

    #[test]
    fn override_raises_the_bound_at_write_time_only() {
        // C-7.5. The override changes what gets WRITTEN, not how
        // existing entries are judged — so an operator's choice to
        // accept staleness is recorded in the data rather than applied
        // invisibly to entries written under a different intent.
        let d = tempfile::tempdir().unwrap();
        let strict = cache_in(d.path(), None);
        let lenient = cache_in(d.path(), Some(86_400));
        assert_eq!(strict.effective_max_age(Some(3600)), 3600);
        assert_eq!(lenient.effective_max_age(Some(3600)), 86_400);

        // An entry already on disk under the strict bound stays
        // expired even when read through the lenient cache.
        let k = key("written-strict");
        let entry = DiskEntry {
            v: SCHEMA_VERSION,
            key: k.cache_key(),
            fetched_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 7200,
            max_age_secs: 3600,
            record: info("MIT"),
        };
        std::fs::write(d.path().join(entry_filename(&k)), serde_json::to_vec(&entry).unwrap())
            .unwrap();
        assert!(
            lenient.get(&k).is_none(),
            "the override must not resurrect an entry written under a stricter bound",
        );
    }

    #[test]
    fn corrupt_entry_is_a_miss_not_an_error() {
        let d = tempfile::tempdir().unwrap();
        let c = cache_in(d.path(), None);
        let k = key("corrupt");
        std::fs::write(d.path().join(entry_filename(&k)), b"{not json").unwrap();
        assert!(c.get(&k).is_none());
    }

    #[test]
    fn collision_is_refused_rather_than_served() {
        // Fabricate an entry whose stored key disagrees with the one
        // being looked up — what a truncated-digest collision would
        // produce. Serving it would hand one package another's licence.
        let d = tempfile::tempdir().unwrap();
        let c = cache_in(d.path(), None);
        let k = key("wanted");
        let entry = DiskEntry {
            v: SCHEMA_VERSION,
            key: key("other").cache_key(),
            fetched_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            max_age_secs: 3600,
            record: info("GPL-3.0"),
        };
        std::fs::write(d.path().join(entry_filename(&k)), serde_json::to_vec(&entry).unwrap())
            .unwrap();
        assert!(c.get(&k).is_none());
    }

    #[test]
    fn disabled_cache_misses_and_drops_writes() {
        let c = DepsDevDiskCache {
            root: None,
            override_max_age: None,
        };
        c.put(&key("x"), &info("MIT"), 3600);
        assert!(c.get(&key("x")).is_none());
        assert!(!c.is_enabled());
    }

    #[test]
    fn writes_leave_no_temp_files_behind() {
        // C-8.3. A rename-based write must not litter, or a long-lived
        // cache directory accumulates partial entries forever.
        let d = tempfile::tempdir().unwrap();
        let c = cache_in(d.path(), None);
        c.put(&key("a"), &info("MIT"), 3600);
        let leftovers: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp file left behind");
    }

    #[test]
    fn cache_control_parsing() {
        assert_eq!(parse_max_age(Some("public, max-age=3600")), Some(3600));
        assert_eq!(parse_max_age(Some("max-age=60")), Some(60));
        assert_eq!(parse_max_age(Some("no-store")), None);
        assert_eq!(parse_max_age(Some("max-age=notanumber")), None);
        assert_eq!(parse_max_age(None), None);
    }
}
