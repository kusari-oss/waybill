//! Milestone 663 US3 — Python cache probe.
//!
//! Two path variants:
//!   A. Installed dist-info: `.../site-packages/<name>-<version>.dist-info/METADATA`
//!      Uses the METADATA `Version:` header as authoritative
//!      (the filename split gives us the name).
//!   B. Wheel cache: `.../wheels/.../<name>-<version>-<pyver>-<abi>-<platform>.whl`
//!      Filename stem split only; no metadata read.
//!
//! Extraction: name normalization per PEP 503 (underscores → hyphens,
//! lowercase). Emit `pkg:pypi/<name>@<version>`.
//!
//! Q1 clarification: METADATA unreadable / missing Version: header
//! (variant A) or filename stem malformed (variant B) → log warn +
//! decline.

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use waybill_common::types::purl::{encode_purl_segment, Purl};

const MAX_METADATA_BYTES: u64 = 64 * 1024;

fn normalize_pypi_name(raw: &str) -> String {
    raw.to_lowercase().replace(['_', '.'], "-")
}

fn name_version_split() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<name>.+?)-(?P<version>\d+\.\d+.*)$").expect("valid regex")
    })
}

fn extract_name_version(stem: &str) -> Option<(String, String)> {
    let caps = name_version_split().captures(stem)?;
    Some((caps["name"].to_string(), caps["version"].to_string()))
}

fn read_version_from_metadata(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_METADATA_BYTES {
        tracing::warn!(
            path = %path.display(),
            size = meta.len(),
            "cache_probe/pypi: METADATA exceeds 64 KiB; declining",
        );
        return None;
    }
    let contents = std::fs::read_to_string(path).ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("Version:") {
            let v = rest.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    tracing::warn!(
        path = %path.display(),
        "cache_probe/pypi: no Version: header in METADATA; declining",
    );
    None
}

fn try_dist_info(path: &Path) -> Option<Purl> {
    if path.file_name()?.to_str()? != "METADATA" {
        return None;
    }
    let parent = path.parent()?;
    let dir_name = parent.file_name()?.to_str()?;
    let stem = dir_name.strip_suffix(".dist-info")?;
    let (raw_name, _stem_version) = extract_name_version(stem)?;
    let version = read_version_from_metadata(path)?;
    let name = normalize_pypi_name(&raw_name);
    let purl_str = format!(
        "pkg:pypi/{}@{}",
        encode_purl_segment(&name),
        encode_purl_segment(&version),
    );
    Purl::new(&purl_str).ok()
}

fn try_wheel_filename(path: &Path) -> Option<Purl> {
    let filename = path.file_name()?.to_str()?;
    let stem = filename.strip_suffix(".whl")?;
    static WHEEL_RE: OnceLock<Regex> = OnceLock::new();
    let re = WHEEL_RE.get_or_init(|| {
        // Wheel PEP 427: {name}-{version}(-{build tag})?-{pyver}-{abi}-{platform}.whl
        // MVP regex: name-version-pyver-abi-platform. Version regex is
        // enough for common cases (\d+.\d+ + optional suffix).
        Regex::new(
            r"^(?P<name>.+?)-(?P<version>\d+\.\d+(?:\.\d+)?[^\-]*)-[^\-]+-[^\-]+-[^\-]+$",
        )
        .expect("valid regex")
    });
    let caps = re.captures(stem)?;
    let raw_name = &caps["name"];
    let version = &caps["version"];
    let name = normalize_pypi_name(raw_name);
    let purl_str = format!(
        "pkg:pypi/{}@{}",
        encode_purl_segment(&name),
        encode_purl_segment(version),
    );
    Purl::new(&purl_str).ok()
}

pub(super) fn try_match_pypi(path: &Path) -> Option<Purl> {
    try_dist_info(path).or_else(|| try_wheel_filename(path))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn m663_pypi_dist_info_extracts_purl() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("waybill_fixture_pip-1.0.0.dist-info");
        std::fs::create_dir_all(&dir).unwrap();
        let meta = dir.join("METADATA");
        std::fs::write(
            &meta,
            "Metadata-Version: 2.1\nName: waybill-fixture-pip\nVersion: 1.0.0\n",
        )
        .unwrap();

        let purl = try_match_pypi(&meta).expect("extract");
        // Note: underscore→hyphen normalization on the name.
        assert_eq!(purl.as_str(), "pkg:pypi/waybill-fixture-pip@1.0.0");
    }

    #[test]
    fn m663_pypi_wheel_cache_extracts_purl() {
        let path = Path::new(
            "/home/user/.cache/pip/wheels/ab/cd/ef/waybill_fixture_pip-1.0.0-py3-none-any.whl",
        );
        let purl = try_match_pypi(path).expect("extract");
        assert_eq!(purl.as_str(), "pkg:pypi/waybill-fixture-pip@1.0.0");
    }

    #[test]
    fn m663_pypi_missing_version_header_declines() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("waybill_fixture_pip-1.0.0.dist-info");
        std::fs::create_dir_all(&dir).unwrap();
        let meta = dir.join("METADATA");
        std::fs::write(&meta, "Metadata-Version: 2.1\nName: waybill-fixture-pip\n").unwrap();

        assert!(try_match_pypi(&meta).is_none());
    }

    #[test]
    fn m663_pypi_non_cache_path_declines() {
        let purl = try_match_pypi(Path::new("/tmp/random.txt"));
        assert!(purl.is_none());
    }
}
