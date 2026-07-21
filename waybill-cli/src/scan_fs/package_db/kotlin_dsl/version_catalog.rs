//! Gradle version-catalog (`libs.versions.toml`) parser.
//!
//! Reads the Gradle 7+ centralized version catalog convention:
//!
//! ```toml
//! [versions]
//! okhttp = "4.12.0"
//!
//! [libraries]
//! okhttp = { module = "com.squareup.okhttp3:okhttp", version.ref = "okhttp" }
//! retrofit = { group = "com.squareup.retrofit2", name = "retrofit", version = "2.11.0" }
//! ```
//!
//! Builds a lookup table `(alias → ResolvedRef)` consumed at component-
//! emission time when `build.gradle.kts` references `libs.<alias>`. Missing
//! `version.ref` entries warn + drop per FR-009. The catalog accepts BOTH
//! the `module = "g:n"` form AND the split `group/name/version` form.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Resolved view of a `[libraries]` entry — `group`, `name`, `version`
/// all populated. Failed resolutions are dropped at parse time so
/// downstream consumers never see partials.
#[derive(Debug, Clone)]
pub(super) struct ResolvedRef {
    pub(super) group: String,
    pub(super) name: String,
    pub(super) version: String,
}

#[derive(Debug, Clone)]
pub(super) struct VersionCatalog {
    pub(super) libraries: BTreeMap<String, ResolvedRef>,
    pub(super) source_path: PathBuf,
}

#[derive(Debug, Error)]
pub(super) enum CatalogError {
    #[error("libs.versions.toml at `{path}` unreadable: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// `toml::de::Error` is 128+ bytes — boxing keeps `CatalogError`
    /// small enough to satisfy `clippy::result-large-err` (a Windows
    /// CI lane lint that's pickier than the local lane).
    #[error("libs.versions.toml at `{path}` parse failure: {source}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
}

pub(super) fn parse(path: &Path) -> Result<VersionCatalog, CatalogError> {
    let text = std::fs::read_to_string(path).map_err(|source| CatalogError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let parsed: toml::Value = toml::from_str(&text).map_err(|source| CatalogError::ParseToml {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;
    let versions: BTreeMap<String, String> = parsed
        .get("versions")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| {
                    v.as_str()
                        .map(|s| (k.clone(), s.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut libraries: BTreeMap<String, ResolvedRef> = BTreeMap::new();
    if let Some(libs_table) = parsed.get("libraries").and_then(|v| v.as_table()) {
        for (alias, entry) in libs_table {
            match resolve_library(entry, &versions) {
                Ok(rref) => {
                    libraries.insert(alias.clone(), rref);
                }
                Err(reason) => {
                    tracing::warn!(
                        alias = %alias,
                        path = %path.display(),
                        reason = %reason,
                        "kotlin_dsl: libs.versions.toml entry dropped — see reason"
                    );
                }
            }
        }
    }
    Ok(VersionCatalog {
        libraries,
        source_path: path.to_path_buf(),
    })
}

fn resolve_library(
    entry: &toml::Value,
    versions: &BTreeMap<String, String>,
) -> Result<ResolvedRef, String> {
    let table = entry
        .as_table()
        .ok_or_else(|| "library entry is not a table".to_string())?;

    let (group, name) = if let Some(module_str) =
        table.get("module").and_then(|v| v.as_str())
    {
        let (g, n) = module_str.split_once(':').ok_or_else(|| {
            format!("module string `{module_str}` is not `group:name`")
        })?;
        (g.to_string(), n.to_string())
    } else {
        let group = table
            .get("group")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "neither `module` nor `group` declared".to_string())?
            .to_string();
        let name = table
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "split form is missing `name`".to_string())?
            .to_string();
        (group, name)
    };

    let version = resolve_version(table, versions)?;
    Ok(ResolvedRef {
        group,
        name,
        version,
    })
}

fn resolve_version(
    table: &toml::map::Map<String, toml::Value>,
    versions: &BTreeMap<String, String>,
) -> Result<String, String> {
    // Form A: `version = "1.2.3"` (inline literal).
    if let Some(v) = table.get("version").and_then(|v| v.as_str()) {
        return Ok(v.to_string());
    }
    // Form B: `version = { ref = "alias" }` (TOML dotted/sub-table forms).
    if let Some(v_tab) = table.get("version").and_then(|v| v.as_table()) {
        if let Some(ref_key) = v_tab.get("ref").and_then(|v| v.as_str()) {
            return versions
                .get(ref_key)
                .cloned()
                .ok_or_else(|| format!("version.ref `{ref_key}` not in [versions]"));
        }
    }
    Err("entry has no resolvable version".to_string())
}

/// Look up `<alias>` against the catalog. Kotlin DSL writes
/// `libs.foo.bar.baz` which references catalog key `foo-bar-baz` (the
/// Gradle convention substitutes `-` for `.` between segments). The
/// caller passes the dotted source form; we normalize here.
pub(super) fn lookup<'a>(catalog: &'a VersionCatalog, dotted_alias: &str) -> Option<&'a ResolvedRef> {
    let dashed = dotted_alias.replace('.', "-");
    catalog.libraries.get(&dashed)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_toml(text: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(text.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parses_module_form_with_version_ref() {
        let f = write_toml(
            r#"
[versions]
okhttp = "4.12.0"

[libraries]
okhttp = { module = "com.squareup.okhttp3:okhttp", version.ref = "okhttp" }
"#,
        );
        let cat = parse(f.path()).unwrap();
        assert_eq!(cat.libraries.len(), 1);
        let r = cat.libraries.get("okhttp").unwrap();
        assert_eq!(r.group, "com.squareup.okhttp3");
        assert_eq!(r.name, "okhttp");
        assert_eq!(r.version, "4.12.0");
    }

    #[test]
    fn parses_split_gav_form_with_inline_version() {
        let f = write_toml(
            r#"
[libraries]
retrofit = { group = "com.squareup.retrofit2", name = "retrofit", version = "2.11.0" }
"#,
        );
        let cat = parse(f.path()).unwrap();
        let r = cat.libraries.get("retrofit").unwrap();
        assert_eq!(r.group, "com.squareup.retrofit2");
        assert_eq!(r.name, "retrofit");
        assert_eq!(r.version, "2.11.0");
    }

    #[test]
    fn drops_entry_missing_version_ref_target() {
        let f = write_toml(
            r#"
[versions]
okhttp = "4.12.0"

[libraries]
okhttp = { module = "com.squareup.okhttp3:okhttp", version.ref = "okhttp" }
ghost = { module = "io.example:ghost", version.ref = "missing" }
"#,
        );
        let cat = parse(f.path()).unwrap();
        // Good entry survives; bad entry dropped.
        assert_eq!(cat.libraries.len(), 1);
        assert!(cat.libraries.contains_key("okhttp"));
        assert!(!cat.libraries.contains_key("ghost"));
    }

    #[test]
    fn drops_malformed_module_string() {
        let f = write_toml(
            r#"
[libraries]
bad = { module = "no-colon-here", version = "1.0.0" }
good = { module = "g:n", version = "1.0.0" }
"#,
        );
        let cat = parse(f.path()).unwrap();
        assert!(cat.libraries.contains_key("good"));
        assert!(!cat.libraries.contains_key("bad"));
    }

    #[test]
    fn parse_failure_returns_err() {
        let f = write_toml("[invalid toml syntax");
        let err = parse(f.path()).unwrap_err();
        assert!(matches!(err, CatalogError::ParseToml { .. }));
    }

    #[test]
    fn empty_catalog_returns_empty_libraries() {
        let f = write_toml("");
        let cat = parse(f.path()).unwrap();
        assert!(cat.libraries.is_empty());
    }

    #[test]
    fn lookup_normalizes_dotted_to_dashed() {
        let f = write_toml(
            r#"
[versions]
ktor = "2.3.7"

[libraries]
ktor-client-cio = { module = "io.ktor:ktor-client-cio-jvm", version.ref = "ktor" }
"#,
        );
        let cat = parse(f.path()).unwrap();
        // Kotlin DSL writes `libs.ktor.client.cio` → catalog `ktor-client-cio`.
        let r = lookup(&cat, "ktor.client.cio").unwrap();
        assert_eq!(r.group, "io.ktor");
    }
}
