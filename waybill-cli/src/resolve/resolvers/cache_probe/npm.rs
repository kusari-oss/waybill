//! Milestone 663 US3 — npm cache probe (node_modules variant).
//!
//! Path shape: `.../node_modules/<name>/package.json`
//! or `.../node_modules/@<scope>/<name>/package.json`.
//!
//! Extraction: `<name>` (or `@<scope>/<name>`) from the path,
//! `<version>` from a bounded metadata read of `package.json`
//! (max 64 KiB, parse via `serde_json`).
//!
//! Q1 clarification: metadata unreadable / missing / malformed →
//! log warn + decline. Never emits at reduced confidence.
//!
//! Pnpm content-addressed store (`~/.local/share/pnpm/store/v3/files/...`)
//! is deferred per plan R3 — requires `.package-lock.json`
//! cross-reference that MVP doesn't ship.

use std::path::Path;

use waybill_common::types::purl::{encode_purl_segment, Purl};

const MAX_PACKAGE_JSON_BYTES: u64 = 64 * 1024;

/// Locate the `node_modules/<name>[/@scope]/package.json` pattern.
/// Returns `(name, scope_or_none)` where `name` is the package name
/// (without scope prefix) and `scope_or_none` is `Some("scope")` for
/// scoped packages.
fn extract_name_from_path(path: &Path) -> Option<(String, Option<String>)> {
    let components: Vec<&std::ffi::OsStr> =
        path.components().map(|c| c.as_os_str()).collect();
    // Path must end in `package.json`.
    let last = components.last()?.to_str()?;
    if last != "package.json" {
        return None;
    }
    // Search for `node_modules` segment. The segments AFTER it are
    // either `[<name>, package.json]` (unscoped, 2 tail segments) or
    // `[@<scope>, <name>, package.json]` (scoped, 3 tail segments).
    for i in 0..components.len() {
        if components[i].to_str()? == "node_modules" && i + 2 < components.len() {
            let a = components[i + 1].to_str()?;
            if a.starts_with('@') {
                // Scoped: [@scope, name, package.json].
                if i + 3 >= components.len() {
                    return None;
                }
                let scope = a.trim_start_matches('@').to_string();
                let name = components[i + 2].to_str()?.to_string();
                return Some((name, Some(scope)));
            } else {
                // Unscoped: [name, package.json].
                return Some((a.to_string(), None));
            }
        }
    }
    None
}

/// Bounded read of `package.json`. Returns the `"version"` field
/// or `None` on any failure per Q1 clarification.
fn read_version_from_package_json(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_PACKAGE_JSON_BYTES {
        tracing::warn!(
            path = %path.display(),
            size = meta.len(),
            "cache_probe/npm: package.json exceeds 64 KiB; declining",
        );
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let json: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "cache_probe/npm: package.json parse failed; declining",
            );
            return None;
        }
    };
    let version = json.get("version").and_then(|v| v.as_str())?;
    Some(version.to_string())
}

pub(super) fn try_match_npm_pnpm(path: &Path) -> Option<Purl> {
    let (name, scope) = extract_name_from_path(path)?;
    let version = match read_version_from_package_json(path) {
        Some(v) => v,
        None => {
            tracing::warn!(
                path = %path.display(),
                "cache_probe/npm: version extraction failed; declining",
            );
            return None;
        }
    };

    let purl_str = match scope {
        Some(s) => format!(
            "pkg:npm/%40{}/{}@{}",
            encode_purl_segment(&s),
            encode_purl_segment(&name),
            encode_purl_segment(&version),
        ),
        None => format!(
            "pkg:npm/{}@{}",
            encode_purl_segment(&name),
            encode_purl_segment(&version),
        ),
    };
    match Purl::new(&purl_str) {
        Ok(p) => Some(p),
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                purl = %purl_str,
                error = %err,
                "cache_probe/npm: PURL construction failed; declining",
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
    fn m663_npm_unscoped_package_json_extracts_purl() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("node_modules").join("waybill-fixture-npm");
        std::fs::create_dir_all(&dir).unwrap();
        let pj = dir.join("package.json");
        std::fs::write(&pj, r#"{"name":"waybill-fixture-npm","version":"1.0.0"}"#).unwrap();

        let purl = try_match_npm_pnpm(&pj).expect("extract");
        assert_eq!(purl.as_str(), "pkg:npm/waybill-fixture-npm@1.0.0");
    }

    #[test]
    fn m663_npm_scoped_package_json_extracts_url_encoded_purl() {
        let td = tempfile::tempdir().unwrap();
        let dir = td
            .path()
            .join("node_modules")
            .join("@waybillfixture")
            .join("scoped-lib");
        std::fs::create_dir_all(&dir).unwrap();
        let pj = dir.join("package.json");
        std::fs::write(&pj, r#"{"name":"@waybillfixture/scoped-lib","version":"2.0.0"}"#)
            .unwrap();

        let purl = try_match_npm_pnpm(&pj).expect("extract");
        assert_eq!(purl.as_str(), "pkg:npm/%40waybillfixture/scoped-lib@2.0.0");
    }

    #[test]
    fn m663_npm_malformed_json_declines() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("node_modules").join("waybill-fixture-npm");
        std::fs::create_dir_all(&dir).unwrap();
        let pj = dir.join("package.json");
        std::fs::write(&pj, "not valid json").unwrap();

        assert!(try_match_npm_pnpm(&pj).is_none());
    }

    #[test]
    fn m663_npm_missing_version_declines() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("node_modules").join("waybill-fixture-npm");
        std::fs::create_dir_all(&dir).unwrap();
        let pj = dir.join("package.json");
        std::fs::write(&pj, r#"{"name":"waybill-fixture-npm"}"#).unwrap();

        assert!(try_match_npm_pnpm(&pj).is_none());
    }

    #[test]
    fn m663_npm_non_node_modules_path_declines() {
        let purl = try_match_npm_pnpm(Path::new("/tmp/random/package.json"));
        assert!(purl.is_none());
    }
}
