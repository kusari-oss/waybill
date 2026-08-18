//! Milestone 663 US2 — Cargo `~/.cargo/registry/` cache probe.
//!
//! Two path variants:
//!   A. `<root>/registry/cache/<registry-hash>/<name>-<version>.crate`
//!   B. `<root>/registry/src/<registry-hash>/<name>-<version>/...`
//!
//! Extraction: filename stem (variant A) or last-cache-relative-dir
//! (variant B) equals `<name>-<version>`. Split on the LAST `-`
//! before a semver-shaped suffix (`\d+\.\d+\.\d+`).
//!
//! Q1 clarification: no semver suffix → log warn + decline.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use waybill_common::types::purl::{encode_purl_segment, Purl};

/// Compute the Cargo registry root: `$CARGO_HOME/registry` if set,
/// else `~/.cargo/registry`.
fn cache_root() -> Option<PathBuf> {
    if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
        return Some(PathBuf::from(cargo_home).join("registry"));
    }
    super::home_dir().map(|h| h.join(".cargo").join("registry"))
}

/// Regex to find the `-<semver>` boundary in a `<name>-<version>`
/// stem. Uses the LAST match since names may contain hyphens
/// (`serde-json-1.0.100` → name=`serde-json`, version=`1.0.100`).
fn name_version_split() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<name>.+?)-(?P<version>\d+\.\d+\.\d+.*)$").expect("valid regex")
    })
}

fn extract_name_version(stem: &str) -> Option<(String, String)> {
    let caps = name_version_split().captures(stem)?;
    Some((caps["name"].to_string(), caps["version"].to_string()))
}

pub(super) fn try_match_cargo(path: &Path) -> Option<Purl> {
    let root = cache_root()?;
    let rel = path.strip_prefix(&root).ok()?;

    let components: Vec<&std::ffi::OsStr> =
        rel.components().map(|c| c.as_os_str()).collect();
    if components.len() < 3 {
        return None;
    }

    // First component must be "cache" (variant A) or "src" (variant B).
    let stem = match components[0].to_str()? {
        "cache" => {
            // Variant A: cache/<registry-hash>/<name>-<version>.crate
            let filename = components[components.len() - 1].to_str()?;
            let stem = filename.strip_suffix(".crate")?;
            stem.to_string()
        }
        "src" => {
            // Variant B: src/<registry-hash>/<name>-<version>/...
            // Locate the `<registry-hash>` at [1]; the next segment is the
            // `<name>-<version>` dir. Its position depends on how deep
            // the path is; we take the segment IMMEDIATELY AFTER the
            // registry-hash (index 2).
            if components.len() < 3 {
                return None;
            }
            components[2].to_str()?.to_string()
        }
        _ => return None,
    };

    let (name, version) = match extract_name_version(&stem) {
        Some(nv) => nv,
        None => {
            tracing::warn!(
                path = %path.display(),
                stem = %stem,
                "cache_probe/cargo: stem doesn't match <name>-<semver>; declining",
            );
            return None;
        }
    };

    let purl_str = format!(
        "pkg:cargo/{}@{}",
        encode_purl_segment(&name),
        encode_purl_segment(&version),
    );
    match Purl::new(&purl_str) {
        Ok(p) => Some(p),
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                purl = %purl_str,
                error = %err,
                "cache_probe/cargo: PURL construction failed; declining",
            );
            None
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    use crate::testing::EnvGuard;

    #[test]
    fn m663_cargo_cache_variant_a_crate_file() {
        let td = tempfile::tempdir().unwrap();
        let mut env = EnvGuard::acquire();
        env.set("CARGO_HOME", td.path().to_str().unwrap());
        let full_path = td
            .path()
            .join("registry")
            .join("cache")
            .join("github.com-1ecc6299db9ec823")
            .join("waybill-fixture-crate-1.2.3.crate");
        let purl = try_match_cargo(&full_path).expect("extract");
        assert_eq!(purl.as_str(), "pkg:cargo/waybill-fixture-crate@1.2.3");
    }

    #[test]
    fn m663_cargo_cache_variant_b_src_dir() {
        let td = tempfile::tempdir().unwrap();
        let mut env = EnvGuard::acquire();
        env.set("CARGO_HOME", td.path().to_str().unwrap());
        let full_path = td
            .path()
            .join("registry")
            .join("src")
            .join("github.com-1ecc6299db9ec823")
            .join("waybill-fixture-crate-1.2.3")
            .join("Cargo.toml");
        let purl = try_match_cargo(&full_path).expect("extract");
        assert_eq!(purl.as_str(), "pkg:cargo/waybill-fixture-crate@1.2.3");
    }

    #[test]
    fn m663_cargo_non_cache_path_declines() {
        let mut env = EnvGuard::acquire();
        env.remove("CARGO_HOME");
        env.remove("HOME");
        env.remove("USERPROFILE");
        let purl = try_match_cargo(Path::new("/tmp/random.crate"));
        assert!(purl.is_none());
    }

    #[test]
    fn m663_cargo_malformed_stem_declines() {
        let td = tempfile::tempdir().unwrap();
        let mut env = EnvGuard::acquire();
        env.set("CARGO_HOME", td.path().to_str().unwrap());
        // Missing semver in stem → decline.
        let full_path = td
            .path()
            .join("registry")
            .join("cache")
            .join("github.com-1ecc")
            .join("no-version-here.crate");
        let purl = try_match_cargo(&full_path);
        assert!(purl.is_none());
    }
}
