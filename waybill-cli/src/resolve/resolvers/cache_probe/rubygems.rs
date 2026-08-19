//! Milestone 663 US2 — RubyGems cache probe.
//!
//! Two path variants:
//!   A. User gem cache: `$GEM_HOME/specs/rubygems.org%443/<name>-<version>.gemspec`
//!      or `~/.gem/specs/rubygems.org%443/<name>-<version>.gemspec`.
//!   B. Bundler bundle: any path containing `vendor/bundle/ruby/<x>/gems/<name>-<version>/...`.
//!
//! Extraction: same `<name>-<version>` split on last `-` before semver,
//! mirroring Cargo. Emit `pkg:gem/<name>@<version>`.
//!
//! Q1 clarification: no semver split → log warn + decline.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use waybill_common::types::purl::{encode_purl_segment, Purl};

fn user_gem_cache_root() -> Option<PathBuf> {
    if let Some(gem_home) = std::env::var_os("GEM_HOME") {
        return Some(PathBuf::from(gem_home).join("specs").join("rubygems.org%443"));
    }
    super::home_dir().map(|h| h.join(".gem").join("specs").join("rubygems.org%443"))
}

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

/// Try variant B: any path containing `vendor/bundle/ruby/<x>/gems/`.
/// The segment IMMEDIATELY after `gems/` is `<name>-<version>`.
fn try_bundler_variant(path: &Path) -> Option<String> {
    let components: Vec<&std::ffi::OsStr> =
        path.components().map(|c| c.as_os_str()).collect();
    if components.len() < 6 {
        return None;
    }
    for i in 0..=components.len() - 6 {
        if components[i].to_str()? == "vendor"
            && components[i + 1].to_str()? == "bundle"
            && components[i + 2].to_str()? == "ruby"
            && components[i + 4].to_str()? == "gems"
        {
            return Some(components[i + 5].to_str()?.to_string());
        }
    }
    None
}

pub(super) fn try_match_rubygems(path: &Path) -> Option<Purl> {
    // Try variant A (user gem cache).
    let stem_from_user_cache: Option<String> = user_gem_cache_root()
        .and_then(|r| {
            path.strip_prefix(&r).ok().and_then(|rel| {
                rel.file_name()
                    .and_then(|f| f.to_str())
                    .and_then(|f| f.strip_suffix(".gemspec").map(str::to_string))
            })
        });

    let stem = stem_from_user_cache
        .or_else(|| try_bundler_variant(path))?;

    let (name, version) = match extract_name_version(&stem) {
        Some(nv) => nv,
        None => {
            tracing::warn!(
                path = %path.display(),
                stem = %stem,
                "cache_probe/rubygems: stem doesn't match <name>-<semver>; declining",
            );
            return None;
        }
    };

    let purl_str = format!(
        "pkg:gem/{}@{}",
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
                "cache_probe/rubygems: PURL construction failed; declining",
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
    fn m663_rubygems_variant_a_user_cache() {
        let td = tempfile::tempdir().unwrap();
        let mut env = EnvGuard::acquire();
        env.set("GEM_HOME", td.path().to_str().unwrap());
        let full_path = td
            .path()
            .join("specs")
            .join("rubygems.org%443")
            .join("waybill-fixture-gem-1.2.3.gemspec");
        let purl = try_match_rubygems(&full_path).expect("extract");
        assert_eq!(purl.as_str(), "pkg:gem/waybill-fixture-gem@1.2.3");
    }

    #[test]
    fn m663_rubygems_variant_b_bundler() {
        // Bundler layout — doesn't use GEM_HOME; path-based match.
        let path = Path::new(
            "/proj/vendor/bundle/ruby/3.1.0/gems/waybill-fixture-gem-1.2.3/lib/foo.rb",
        );
        let purl = try_match_rubygems(path).expect("extract");
        assert_eq!(purl.as_str(), "pkg:gem/waybill-fixture-gem@1.2.3");
    }

    #[test]
    fn m663_rubygems_non_cache_path_declines() {
        let mut env = EnvGuard::acquire();
        env.remove("GEM_HOME");
        env.remove("HOME");
        env.remove("USERPROFILE");
        let purl = try_match_rubygems(Path::new("/tmp/random.gemspec"));
        assert!(purl.is_none());
    }
}
