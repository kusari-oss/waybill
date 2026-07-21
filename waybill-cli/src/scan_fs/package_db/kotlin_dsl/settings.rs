//! Kotlin DSL `settings.gradle.kts` parser.
//!
//! Extracts the workspace topology from `settings.gradle.kts`:
//!
//! - `rootProject.name = "..."` → workspace-root component name
//! - `include(":module")` / `include(":m1", ":m2")` → workspace-member
//!   module declarations
//!
//! Per spec FR-007 + contracts/kotlin-dsl-extraction.md § "`settings.gradle.kts`
//! parsing". Missing fields are non-fatal (workspace root falls back to
//! the directory name when `rootProject.name` is absent).

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Clone)]
pub(super) struct SettingsScript {
    /// `rootProject.name = "..."` value, or `None` when the file
    /// doesn't declare one. Workspace-root component falls back to the
    /// containing directory name in that case.
    pub(super) root_project_name: Option<String>,
    /// Module names declared via `include(...)`. Leading colon stripped
    /// (`:app` → `app`). Reserved for a future per-module main-module
    /// emission pass; v0.1 synthesizes only the workspace-root, not the
    /// individual main-module entries.
    #[allow(dead_code)]
    pub(super) includes: Vec<String>,
    /// Path to the `settings.gradle.kts` file. Drives the workspace-
    /// root component's `waybill:source-files` annotation.
    pub(super) source_path: PathBuf,
}

#[derive(Debug, Error)]
pub(super) enum SettingsError {
    #[error("settings.gradle.kts at `{path}` unreadable: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

static ROOT_PROJECT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*rootProject\.name\s*=\s*"([^"]+)"\s*$"#)
        .expect("rootProject.name regex compiles")
});

static INCLUDE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*include\s*\(\s*((?:"[^"]+"\s*,?\s*)+)\)"#)
        .expect("include regex compiles")
});

static QUOTED_ARG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""([^"]+)""#).expect("quoted-arg regex compiles")
});

pub(super) fn parse(path: &Path) -> Result<SettingsScript, SettingsError> {
    let content = std::fs::read_to_string(path).map_err(|source| SettingsError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let root_project_name = ROOT_PROJECT_NAME_RE
        .captures(&content)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
    let mut includes: Vec<String> = Vec::new();
    for outer in INCLUDE_RE.captures_iter(&content) {
        let Some(args) = outer.get(1) else { continue };
        for inner in QUOTED_ARG_RE.captures_iter(args.as_str()) {
            let Some(raw) = inner.get(1) else { continue };
            let cleaned = raw.as_str().trim_start_matches(':').to_string();
            if !cleaned.is_empty() {
                includes.push(cleaned);
            }
        }
    }
    Ok(SettingsScript {
        root_project_name,
        includes,
        source_path: path.to_path_buf(),
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_settings(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parses_root_project_name_and_includes() {
        let f = write_settings(
            r#"rootProject.name = "my-kmp-lib"
include(":app", ":shared")
"#,
        );
        let s = parse(f.path()).unwrap();
        assert_eq!(s.root_project_name.as_deref(), Some("my-kmp-lib"));
        assert_eq!(s.includes, vec!["app".to_string(), "shared".to_string()]);
    }

    #[test]
    fn handles_missing_root_project_name() {
        let f = write_settings(r#"include(":app")"#);
        let s = parse(f.path()).unwrap();
        assert!(s.root_project_name.is_none());
        assert_eq!(s.includes, vec!["app".to_string()]);
    }

    #[test]
    fn handles_multi_include_blocks() {
        let f = write_settings(
            r#"rootProject.name = "demo"
include(":app")
include(":lib", ":web")
"#,
        );
        let s = parse(f.path()).unwrap();
        assert_eq!(s.includes, vec!["app".to_string(), "lib".to_string(), "web".to_string()]);
    }

    #[test]
    fn strips_leading_colon_from_module_names() {
        let f = write_settings(r#"include(":nested:sub-module")"#);
        let s = parse(f.path()).unwrap();
        assert_eq!(s.includes, vec!["nested:sub-module".to_string()]);
    }

    #[test]
    fn empty_settings_file_returns_empty_settings_script() {
        let f = write_settings("");
        let s = parse(f.path()).unwrap();
        assert!(s.root_project_name.is_none());
        assert!(s.includes.is_empty());
    }

    #[test]
    fn missing_file_returns_io_err() {
        let err = parse(Path::new("/no/such/path/settings.gradle.kts")).unwrap_err();
        assert!(matches!(err, SettingsError::Io { .. }));
    }
}
