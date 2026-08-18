//! Milestone 663 US1 — Maven `~/.m2/repository/` cache probe.
//!
//! Cache path shape: `<root>/g1/g2/.../artifact/version/artifact-version.<ext>`.
//! Extraction: split segments after cache root into
//! `[g1, g2, ..., artifact, version, filename]` → PURL
//! `pkg:maven/g1.g2..../artifact@version`.
//!
//! Q1 clarification: malformed cache path → log warn + decline.

use std::path::{Path, PathBuf};

use waybill_common::types::purl::{encode_purl_segment, Purl};

/// Compute the Maven cache root: `$M2_HOME/repository` if set, else
/// `~/.m2/repository`.
fn cache_root() -> Option<PathBuf> {
    if let Some(m2_home) = std::env::var_os("M2_HOME") {
        return Some(PathBuf::from(m2_home).join("repository"));
    }
    super::home_dir().map(|h| h.join(".m2").join("repository"))
}

/// Try to extract a Maven PURL from a path under the Maven cache root.
/// Returns `None` for both "path outside cache" and "malformed path
/// structure" (Q1 decline behavior).
pub(super) fn try_match_maven(path: &Path) -> Option<Purl> {
    let root = cache_root()?;
    let rel = path.strip_prefix(&root).ok()?;

    let components: Vec<&std::ffi::OsStr> =
        rel.components().map(|c| c.as_os_str()).collect();
    if components.len() < 3 {
        tracing::warn!(
            path = %path.display(),
            "cache_probe/maven: path too shallow for Maven GAV extraction; declining",
        );
        return None;
    }

    let filename = components[components.len() - 1].to_str()?;
    let version = components[components.len() - 2].to_str()?;
    let artifact = components[components.len() - 3].to_str()?;

    let expected_prefix = format!("{artifact}-{version}");
    if !filename.starts_with(&expected_prefix) {
        tracing::warn!(
            path = %path.display(),
            "cache_probe/maven: filename {filename:?} doesn't match artifact-version prefix; declining",
        );
        return None;
    }

    let group_parts: Vec<&str> = components[..components.len() - 3]
        .iter()
        .filter_map(|c| c.to_str())
        .collect();
    if group_parts.is_empty() {
        tracing::warn!(
            path = %path.display(),
            "cache_probe/maven: no group segments; declining",
        );
        return None;
    }
    let group_id = group_parts.join(".");

    let purl_str = format!(
        "pkg:maven/{}/{}@{}",
        encode_purl_segment(&group_id),
        encode_purl_segment(artifact),
        encode_purl_segment(version),
    );
    match Purl::new(&purl_str) {
        Ok(p) => Some(p),
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                purl = %purl_str,
                error = %err,
                "cache_probe/maven: PURL construction failed; declining",
            );
            None
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn m663_maven_cache_hit_extracts_gav() {
        let td = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test; env var reset in teardown below.
        unsafe {
            std::env::set_var("M2_HOME", td.path());
        }

        let full_path = td
            .path()
            .join("repository")
            .join("com")
            .join("example")
            .join("waybillfixture")
            .join("waybill-fixture-lib")
            .join("1.0.0")
            .join("waybill-fixture-lib-1.0.0.jar");

        let purl = try_match_maven(&full_path).expect("extract");
        assert_eq!(
            purl.as_str(),
            "pkg:maven/com.example.waybillfixture/waybill-fixture-lib@1.0.0"
        );

        unsafe {
            std::env::remove_var("M2_HOME");
        }
    }

    #[test]
    fn m663_maven_non_cache_path_declines() {
        // Guarantee no M2_HOME set for this test.
        unsafe {
            std::env::remove_var("M2_HOME");
        }
        let purl = try_match_maven(std::path::Path::new("/tmp/random/file.jar"));
        assert!(purl.is_none());
    }
}
