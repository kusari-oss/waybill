//! Cache-first JSON loader for the fingerprint corpus.
//!
//! Reads `<cache-dir>/<sha>/corpus/index.json` to enumerate per-library
//! files, then loads each `corpus/<library>.json` into either a
//! v1 `FingerprintRecord` (milestone 108) or a v2 `CorpusRecordV2`
//! (milestone 110). Detection is record-level via the JSON's
//! `schema_version` field (presence → v2; absence → v1) — NOT via an
//! archive-level VERSION sentinel (existing milestone-108 archives have
//! none and adding one would be a breaking change to the public corpus
//! contract).
//!
//! Records that fail individual parsing or validation are skipped with
//! `tracing::warn!` per FR-010; other records still load. Missing or
//! corrupt index returns a typed error so the caller can decide whether
//! to trigger a fetch (Phase 4) or fall back to bundled.
//!
//! The two loader entry points (`load_corpus_from_cache` for v1,
//! `load_v2_records_from_cache` for v2) are independent: a caller may
//! invoke one or both. Within a single cache directory, v1 and v2
//! records MAY coexist (each JSON file is either v1 or v2 — the
//! loader peeks at the file before parsing). Existing milestone-108
//! consumers continue to call the v1-only loader and are unaffected.

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use super::cache;
use super::record::{CorpusRecordV2, FingerprintRecord};
use super::source_config::CorpusSourceId;
use super::source_sha::CorpusSha;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum LoaderError {
    #[error("cache miss: no `corpus/index.json` at {path}")]
    CacheNotFound { path: String },
    #[error("cache corrupt: {reason} (at {path})")]
    CacheCorrupt { path: String, reason: String },
}

#[derive(Debug, Deserialize)]
struct CorpusIndex {
    version: u32,
    entries: Vec<IndexEntry>,
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    library: String,
    path: String,
    #[serde(default)]
    #[allow(dead_code)]
    digest: Option<String>,
}

/// Load the corpus from the per-SHA cache directory. Returns a vector
/// of validated records or a typed error for the caller to handle.
#[allow(dead_code)]
pub(crate) fn load_corpus_from_cache(
    sha: &CorpusSha,
) -> Result<Vec<FingerprintRecord>, LoaderError> {
    let dir = cache::cache_dir_for_sha(sha);
    let corpus_dir = dir.join("corpus");
    let index_path = corpus_dir.join("index.json");

    let index_text = std::fs::read_to_string(&index_path).map_err(|_| {
        LoaderError::CacheNotFound {
            path: index_path.display().to_string(),
        }
    })?;

    let index: CorpusIndex = serde_json::from_str(&index_text).map_err(|e| {
        LoaderError::CacheCorrupt {
            path: index_path.display().to_string(),
            reason: format!("index.json parse failed: {e}"),
        }
    })?;

    if index.version != 1 {
        return Err(LoaderError::CacheCorrupt {
            path: index_path.display().to_string(),
            reason: format!("unsupported index version: {}", index.version),
        });
    }

    let records = load_per_library_records(&corpus_dir, &index);
    Ok(records)
}

fn load_per_library_records(
    corpus_dir: &Path,
    index: &CorpusIndex,
) -> Vec<FingerprintRecord> {
    let mut out = Vec::with_capacity(index.entries.len());
    for entry in &index.entries {
        let path = corpus_dir.join(&entry.path);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    library = %entry.library,
                    path = %path.display(),
                    error = %e,
                    "fingerprint corpus record file unreadable; skipping",
                );
                continue;
            }
        };
        let record: FingerprintRecord = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    library = %entry.library,
                    path = %path.display(),
                    error = %e,
                    "fingerprint corpus record JSON malformed; skipping",
                );
                continue;
            }
        };
        if let Err(e) = record.validate() {
            tracing::warn!(
                library = %entry.library,
                path = %path.display(),
                error = %e,
                "fingerprint corpus record failed validation; skipping",
            );
            continue;
        }
        out.push(record);
    }
    out
}

// =====================================================================
// v2 loader (milestone 110, Phase 4 Slice B-1)
// =====================================================================

/// Load only the v2 records from the per-SHA cache directory.
///
/// Returns an empty Vec when no v2 records exist in the cache (the
/// milestone-108 default state, where every JSON file is v1). v1
/// records are silently skipped — callers needing v1 records call
/// `load_corpus_from_cache` instead.
///
/// Detection: peeks at each JSON file's `schema_version` field via a
/// lightweight pre-parse (the `RecordSchemaProbe` struct). Files with
/// `schema_version: 2` deserialize to `CorpusRecordV2`; files without
/// the field (v1 shape) are skipped.
///
/// Validation: post-deserialization `validate_v2()` runs per record.
/// Records failing validation skip with a warning (matches v1 behavior).
#[allow(dead_code)] // Slice B-2 wires this into the production scan path.
pub(crate) fn load_v2_records_from_cache(
    sha: &CorpusSha,
) -> Result<Vec<CorpusRecordV2>, LoaderError> {
    let dir = cache::cache_dir_for_sha(sha);
    let corpus_dir = dir.join("corpus");
    let index_path = corpus_dir.join("index.json");

    let index_text = std::fs::read_to_string(&index_path).map_err(|_| {
        LoaderError::CacheNotFound {
            path: index_path.display().to_string(),
        }
    })?;

    let index: CorpusIndex = serde_json::from_str(&index_text).map_err(|e| {
        LoaderError::CacheCorrupt {
            path: index_path.display().to_string(),
            reason: format!("index.json parse failed: {e}"),
        }
    })?;

    if index.version != 1 {
        return Err(LoaderError::CacheCorrupt {
            path: index_path.display().to_string(),
            reason: format!("unsupported index version: {}", index.version),
        });
    }

    Ok(load_per_library_v2_records(&corpus_dir, &index))
}

/// Load v2 records from a per-source cache directory (milestone 110
/// Phase 5-Slim PR 2). Same parsing logic as `load_v2_records_from_cache`
/// but reads from `<cache-root>/<source-id>/<content-sha>/corpus/`
/// instead of `<cache-root>/<content-sha>/corpus/`.
///
/// Returns an empty Vec for a per-source cache that happens to contain
/// only v1 records — the multi-source orchestrator (PR-2 multi_source.rs)
/// treats that as "this source has no v2 contribution" and continues.
#[allow(dead_code)]
pub(crate) fn load_v2_records_from_source_cache(
    source_id: &CorpusSourceId,
    content_sha: &CorpusSha,
) -> Result<Vec<CorpusRecordV2>, LoaderError> {
    let dir = cache::cache_dir_for_source(source_id, content_sha);
    let corpus_dir = dir.join("corpus");
    let index_path = corpus_dir.join("index.json");

    let index_text = std::fs::read_to_string(&index_path).map_err(|_| {
        LoaderError::CacheNotFound {
            path: index_path.display().to_string(),
        }
    })?;

    let index: CorpusIndex = serde_json::from_str(&index_text).map_err(|e| {
        LoaderError::CacheCorrupt {
            path: index_path.display().to_string(),
            reason: format!("index.json parse failed: {e}"),
        }
    })?;

    if index.version != 1 {
        return Err(LoaderError::CacheCorrupt {
            path: index_path.display().to_string(),
            reason: format!("unsupported index version: {}", index.version),
        });
    }

    Ok(load_per_library_v2_records(&corpus_dir, &index))
}

#[derive(Debug, Deserialize)]
struct RecordSchemaProbe {
    #[serde(default)]
    schema_version: Option<u8>,
}

fn load_per_library_v2_records(
    corpus_dir: &Path,
    index: &CorpusIndex,
) -> Vec<CorpusRecordV2> {
    let mut out = Vec::new();
    for entry in &index.entries {
        let path = corpus_dir.join(&entry.path);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    library = %entry.library,
                    path = %path.display(),
                    error = %e,
                    "fingerprint corpus record file unreadable; skipping",
                );
                continue;
            }
        };
        // Peek at schema_version before committing to a full parse.
        // A failed probe = malformed JSON; skip with a warning. A probe
        // returning `schema_version: None` is a v1 record; skip silently
        // (the v1 loader handles those).
        let probe: RecordSchemaProbe = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    library = %entry.library,
                    path = %path.display(),
                    error = %e,
                    "fingerprint corpus record JSON malformed (probe failed); skipping",
                );
                continue;
            }
        };
        match probe.schema_version {
            Some(2) => { /* fall through to full v2 parse */ }
            Some(other) => {
                tracing::warn!(
                    library = %entry.library,
                    path = %path.display(),
                    schema_version = other,
                    "fingerprint corpus record has unsupported schema_version; skipping",
                );
                continue;
            }
            None => continue, // v1 record; not our concern here.
        }
        let record: CorpusRecordV2 = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    library = %entry.library,
                    path = %path.display(),
                    error = %e,
                    "v2 corpus record JSON malformed; skipping",
                );
                continue;
            }
        };
        if let Err(e) = record.validate_v2() {
            tracing::warn!(
                library = %entry.library,
                path = %path.display(),
                error = %e,
                "v2 corpus record failed validation; skipping",
            );
            continue;
        }
        out.push(record);
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    // Shared env-mutation lock with cache::tests — see fingerprints/mod.rs.
    use super::super::test_env_lock as env_lock;

    const SAMPLE_SHA: &str = "fff39c6ad22ce8420b506323ce1d5cce4b628d5c";

    fn write_valid_record(corpus_dir: &Path, library: &str) -> std::path::PathBuf {
        let path = corpus_dir.join(format!("{library}.json"));
        std::fs::write(
            &path,
            format!(
                r#"{{
                    "library": "{library}",
                    "target_purl": "pkg:generic/{library}",
                    "symbols": ["sym1", "sym2", "sym3"],
                    "min_symbols": 2
                }}"#
            ),
        )
        .unwrap();
        path
    }

    fn write_index(corpus_dir: &Path, libraries: &[&str]) {
        let entries: Vec<String> = libraries
            .iter()
            .map(|l| format!(r#"{{"library":"{l}","path":"{l}.json"}}"#))
            .collect();
        let json = format!(
            r#"{{"version":1,"entries":[{}]}}"#,
            entries.join(",")
        );
        std::fs::write(corpus_dir.join("index.json"), json).unwrap();
    }

    fn setup_cache(libraries: &[&str]) -> (tempfile::TempDir, CorpusSha) {
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let corpus_dir = tmp.path().join(sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        for lib in libraries {
            write_valid_record(&corpus_dir, lib);
        }
        write_index(&corpus_dir, libraries);
        (tmp, sha)
    }

    #[test]
    fn loads_valid_cache_to_corpus() {
        let _g = env_lock();
        let (tmp, sha) = setup_cache(&["openssl", "zlib"]);
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let records = load_corpus_from_cache(&sha).unwrap();
        assert_eq!(records.len(), 2);
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn returns_cache_not_found_when_index_absent() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        assert!(matches!(
            load_corpus_from_cache(&sha),
            Err(LoaderError::CacheNotFound { .. })
        ));
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn returns_cache_corrupt_on_malformed_index_json() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let corpus_dir = tmp.path().join(sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        std::fs::write(corpus_dir.join("index.json"), "{ not valid json").unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        assert!(matches!(
            load_corpus_from_cache(&sha),
            Err(LoaderError::CacheCorrupt { .. })
        ));
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn skips_malformed_records_warns_continues() {
        let _g = env_lock();
        let (tmp, sha) = setup_cache(&["openssl"]);
        let corpus_dir = tmp.path().join(sha.to_full_hex()).join("corpus");
        // Add a malformed record file to the corpus dir + index.
        std::fs::write(corpus_dir.join("broken.json"), "{ invalid").unwrap();
        write_index(&corpus_dir, &["openssl", "broken"]);
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let records = load_corpus_from_cache(&sha).unwrap();
        // Only the valid record loaded; the broken one was skipped.
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].library, "openssl");
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn parses_index_with_optional_digest_field() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let corpus_dir = tmp.path().join(sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        write_valid_record(&corpus_dir, "openssl");
        std::fs::write(
            corpus_dir.join("index.json"),
            r#"{"version":1,"entries":[{"library":"openssl","path":"openssl.json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}]}"#,
        )
        .unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let records = load_corpus_from_cache(&sha).unwrap();
        assert_eq!(records.len(), 1);
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    // ============================================================
    // v2 loader tests (milestone 110 Phase 4 Slice B-1)
    // ============================================================

    fn write_v2_record(corpus_dir: &Path, id: &str, name: &str) -> std::path::PathBuf {
        let path = corpus_dir.join(format!("{name}.json"));
        let json = format!(
            r#"{{
              "id": "{id}",
              "purl": "pkg:github/example/{name}@1.0.0",
              "version_range": "1.0",
              "indicators": {{
                "exported_symbols": {{
                  "type": "symbol-set",
                  "required": ["sym1", "sym2", "sym3"],
                  "min_match": 2,
                  "confidence_baseline": 0.70
                }}
              }},
              "provenance": {{
                "tier": "manual-curation",
                "extracted_from": "https://example.com/{name}",
                "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "extraction_toolchain": "test-fixture",
                "extracted_at": "2026-06-01T12:00:00Z"
              }},
              "schema_version": 2
            }}"#
        );
        std::fs::write(&path, json).unwrap();
        path
    }

    fn setup_mixed_cache(v1_libs: &[&str], v2_libs: &[&str]) -> (tempfile::TempDir, CorpusSha) {
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let corpus_dir = tmp.path().join(sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        for lib in v1_libs {
            write_valid_record(&corpus_dir, lib);
        }
        for lib in v2_libs {
            write_v2_record(&corpus_dir, &format!("{lib}-1.0"), lib);
        }
        let all_libs: Vec<&str> = v1_libs.iter().chain(v2_libs.iter()).copied().collect();
        write_index(&corpus_dir, &all_libs);
        (tmp, sha)
    }

    #[test]
    fn v2_loader_returns_empty_when_only_v1_records_present() {
        let _g = env_lock();
        let (tmp, sha) = setup_cache(&["openssl", "zlib"]); // v1-only cache
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let records = load_v2_records_from_cache(&sha).unwrap();
        assert_eq!(
            records.len(),
            0,
            "v2 loader MUST silently skip v1 records (milestone-108 backward compat)"
        );
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn v2_loader_loads_only_v2_records_from_mixed_cache() {
        let _g = env_lock();
        let (tmp, sha) = setup_mixed_cache(&["openssl"], &["newlib", "anotherv2"]);
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let v2_records = load_v2_records_from_cache(&sha).unwrap();
        assert_eq!(v2_records.len(), 2, "expected 2 v2 records; v1 entry skipped");
        let mut ids: Vec<String> = v2_records.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        assert_eq!(ids, vec!["anotherv2-1.0".to_string(), "newlib-1.0".to_string()]);
        // And the v1 loader still works alongside.
        let v1_records = load_corpus_from_cache(&sha).unwrap();
        assert_eq!(v1_records.len(), 1, "expected 1 v1 record alongside the 2 v2 ones");
        assert_eq!(v1_records[0].library, "openssl");
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn v2_loader_skips_records_with_unsupported_schema_version() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let corpus_dir = tmp.path().join(sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        // schema_version: 99 — not supported.
        std::fs::write(
            corpus_dir.join("futurelib.json"),
            r#"{"id":"futurelib","schema_version":99}"#,
        )
        .unwrap();
        write_v2_record(&corpus_dir, "validlib-1.0", "validlib");
        write_index(&corpus_dir, &["futurelib", "validlib"]);
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let records = load_v2_records_from_cache(&sha).unwrap();
        assert_eq!(
            records.len(),
            1,
            "futurelib should be skipped (unsupported schema_version); validlib loads"
        );
        assert_eq!(records[0].id, "validlib-1.0");
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn v2_loader_skips_malformed_record_but_loads_remainder() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let corpus_dir = tmp.path().join(sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        // Claims schema_version: 2 but is missing required fields (no `purl`,
        // no `indicators`, etc.) — full deserialization fails.
        std::fs::write(
            corpus_dir.join("brokenlib.json"),
            r#"{"id":"broken","schema_version":2}"#,
        )
        .unwrap();
        write_v2_record(&corpus_dir, "good-1.0", "good");
        write_index(&corpus_dir, &["brokenlib", "good"]);
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let records = load_v2_records_from_cache(&sha).unwrap();
        assert_eq!(
            records.len(),
            1,
            "broken record skipped; good record still loads (FR-010 graceful degradation)"
        );
        assert_eq!(records[0].id, "good-1.0");
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn v2_loader_returns_cache_not_found_when_index_missing() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let err = load_v2_records_from_cache(&sha).unwrap_err();
        assert!(matches!(err, LoaderError::CacheNotFound { .. }));
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    // ============================================================
    // Per-source loader tests (milestone 110 Phase 5-Slim PR 2)
    // ============================================================

    fn setup_per_source_cache(
        source_id: &CorpusSourceId,
        content_sha: &CorpusSha,
        v2_libs: &[&str],
    ) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let corpus_dir = tmp
            .path()
            .join(source_id.as_str())
            .join(content_sha.to_full_hex())
            .join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        for lib in v2_libs {
            write_v2_record(&corpus_dir, &format!("{lib}-1.0"), lib);
        }
        write_index(&corpus_dir, v2_libs);
        tmp
    }

    #[test]
    fn per_source_loader_reads_nested_layout() {
        let _g = env_lock();
        let source_id = CorpusSourceId::from_url("https://corpus.example/extras.tar.gz");
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let tmp = setup_per_source_cache(&source_id, &sha, &["libfoo", "libbar"]);
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let records = load_v2_records_from_source_cache(&source_id, &sha).unwrap();
        assert_eq!(records.len(), 2);
        let mut ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        assert_eq!(ids, vec!["libbar-1.0".to_string(), "libfoo-1.0".to_string()]);
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn per_source_loader_returns_not_found_for_missing_layout() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let source_id = CorpusSourceId::from_url("https://corpus.example/nope.tar.gz");
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let err = load_v2_records_from_source_cache(&source_id, &sha).unwrap_err();
        assert!(matches!(err, LoaderError::CacheNotFound { .. }));
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn per_source_and_flat_layouts_coexist() {
        // The milestone-108 default uses the flat `<sha>/` layout;
        // arbitrary sources nest under `<source-id>/<sha>/`. Both
        // must load cleanly from the same cache root in one scan.
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        // Flat layout for the milestone-108 default.
        let default_sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let default_dir = tmp.path().join(default_sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&default_dir).unwrap();
        write_v2_record(&default_dir, "openssl-1.0", "openssl");
        write_index(&default_dir, &["openssl"]);
        // Nested layout for an arbitrary source.
        let arb_source_id = CorpusSourceId::from_url("https://corpus.example/extra.tar.gz");
        let arb_sha =
            CorpusSha::from_hex("0123456789abcdef0123456789abcdef01234567").unwrap();
        let arb_dir = tmp
            .path()
            .join(arb_source_id.as_str())
            .join(arb_sha.to_full_hex())
            .join("corpus");
        std::fs::create_dir_all(&arb_dir).unwrap();
        write_v2_record(&arb_dir, "libxyz-1.0", "libxyz");
        write_index(&arb_dir, &["libxyz"]);

        let default_recs = load_v2_records_from_cache(&default_sha).unwrap();
        assert_eq!(default_recs.len(), 1);
        assert_eq!(default_recs[0].id, "openssl-1.0");
        let extra_recs =
            load_v2_records_from_source_cache(&arb_source_id, &arb_sha).unwrap();
        assert_eq!(extra_recs.len(), 1);
        assert_eq!(extra_recs[0].id, "libxyz-1.0");
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn v1_and_v2_loaders_can_be_called_independently_on_the_same_cache() {
        // Confirms the spec's invariant: existing v1 callers see no change
        // even when v2 records exist alongside. New code paths consuming
        // load_v2_records_from_cache see only the v2 records.
        let _g = env_lock();
        let (tmp, sha) = setup_mixed_cache(&["openssl", "zlib"], &["newlib"]);
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        // v1 path returns ONLY v1 records (the v2 entry is malformed-as-v1 +
        // skipped by the v1 loader's `FingerprintRecord` deserialize).
        let v1_records = load_corpus_from_cache(&sha).unwrap();
        assert_eq!(v1_records.len(), 2);
        // v2 path returns ONLY v2 records.
        let v2_records = load_v2_records_from_cache(&sha).unwrap();
        assert_eq!(v2_records.len(), 1);
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }
}
