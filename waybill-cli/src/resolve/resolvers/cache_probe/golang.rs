//! Milestone 663 US1 — Go `$GOMODCACHE` cache probe.
//!
//! Cache path shape: `<root>/host/user/pkg@v1.2.3/...` where the
//! `<name>@<version>` segment appears at some level in the path.
//! Extraction: find the `@`-containing segment, split into name +
//! version, join pre-`@` segments into the namespace.
//!
//! Q1 clarification: no `@` found or version doesn't start with `v`
//! → log warn + decline.

use std::path::{Path, PathBuf};

use waybill_common::types::purl::{encode_purl_segment, Purl};

fn cache_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(gomodcache) = std::env::var_os("GOMODCACHE") {
        roots.push(PathBuf::from(gomodcache));
    }
    if let Some(gopath) = std::env::var_os("GOPATH") {
        roots.push(PathBuf::from(gopath).join("pkg").join("mod"));
    }
    if let Some(home) = super::home_dir() {
        roots.push(home.join("go").join("pkg").join("mod"));
    }
    roots
}

pub(super) fn try_match_golang(path: &Path) -> Option<Purl> {
    // Try each candidate cache root; use the first that strip_prefix's.
    let rel = cache_roots()
        .into_iter()
        .find_map(|root| path.strip_prefix(&root).ok().map(|r| r.to_path_buf()))?;

    // Find the first segment containing `@vX.Y.Z`. Segments after it are
    // "inside the module" and irrelevant to the coord.
    let mut namespace_parts: Vec<String> = Vec::new();
    for component in rel.components() {
        let seg = component.as_os_str().to_str()?;
        if let Some(at_idx) = seg.find('@') {
            let name = &seg[..at_idx];
            let version = &seg[at_idx + 1..];
            if !version.starts_with('v') {
                tracing::warn!(
                    path = %path.display(),
                    "cache_probe/golang: version {version:?} doesn't start with 'v'; declining",
                );
                return None;
            }
            let namespace = namespace_parts
                .iter()
                .map(|s| encode_purl_segment(s))
                .collect::<Vec<_>>()
                .join("/");
            let purl_str = if namespace.is_empty() {
                format!(
                    "pkg:golang/{}@{}",
                    encode_purl_segment(name),
                    encode_purl_segment(version),
                )
            } else {
                format!(
                    "pkg:golang/{}/{}@{}",
                    namespace,
                    encode_purl_segment(name),
                    encode_purl_segment(version),
                )
            };
            return match Purl::new(&purl_str) {
                Ok(p) => Some(p),
                Err(err) => {
                    tracing::warn!(
                        path = %path.display(),
                        purl = %purl_str,
                        error = %err,
                        "cache_probe/golang: PURL construction failed; declining",
                    );
                    None
                }
            };
        }
        namespace_parts.push(seg.to_string());
    }
    tracing::warn!(
        path = %path.display(),
        "cache_probe/golang: no @vX.Y.Z segment found; declining",
    );
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn m663_golang_cache_hit_extracts_module_coord() {
        let td = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test.
        unsafe {
            std::env::set_var("GOMODCACHE", td.path());
            std::env::remove_var("GOPATH");
        }
        let full_path = td
            .path()
            .join("example.com")
            .join("waybill")
            .join("fixture@v2.0.0")
            .join("main.go");

        let purl = try_match_golang(&full_path).expect("extract");
        assert_eq!(
            purl.as_str(),
            "pkg:golang/example.com/waybill/fixture@v2.0.0"
        );

        unsafe {
            std::env::remove_var("GOMODCACHE");
        }
    }

    #[test]
    fn m663_golang_non_cache_path_declines() {
        unsafe {
            std::env::remove_var("GOMODCACHE");
            std::env::remove_var("GOPATH");
        }
        let purl = try_match_golang(std::path::Path::new("/tmp/random/main.go"));
        assert!(purl.is_none());
    }
}
