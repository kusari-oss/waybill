//! Milestone 224: minimal `pants.toml` `[jvm]` section parser.
//!
//! Parses ONLY `[jvm].default_resolve` + `[jvm.resolves]` per
//! research.md §R4. All other sections + unknown value types are
//! gracefully ignored via `#[serde(default)]` on every field.
//!
//! Fail-open contract (FR-004): missing file, missing keys, malformed
//! TOML, and non-string values all fall through to `None` / empty map
//! so the reader can fall back to the default `3rdparty/jvm/*.lock`
//! glob.
//!
//! Distinct from `super::super::pants::config` (the pex reader's
//! parser for `[python].lockfile`) by design — the two Pants sections
//! evolve independently, and separate types prevent one reader's
//! schema changes from breaking the other.

use std::collections::HashMap;

use serde::Deserialize;

/// Minimal `pants.toml` shape. Everything outside `[jvm]` is ignored.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct PantsConfig {
    #[serde(default)]
    pub(crate) jvm: JvmSection,
}

/// The one Pants config section we care about here. Both fields
/// optional — an empty `[jvm]` table (or missing section) is valid.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct JvmSection {
    /// Default resolve name. Currently unused by the reader (each
    /// discovered lockfile's resolve name comes from its own filename
    /// stem or its `[jvm.resolves]` map key). Retained on the struct
    /// so parse round-trip is faithful — a future feature might
    /// promote it to a scope-tagging fallback.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) default_resolve: Option<String>,
    /// Map of resolve name → lockfile path (relative to scan root).
    /// Empty when the operator relies on the default glob only.
    #[serde(default)]
    pub(crate) resolves: HashMap<String, String>,
}

/// Parse `pants.toml` bytes. Returns `None` on any parse error (per
/// FR-004 fail-open). The caller should log a WARN naming the file
/// and fall back to the default lockfile discovery glob.
pub(crate) fn parse(bytes: &[u8]) -> Option<PantsConfig> {
    let text = std::str::from_utf8(bytes).ok()?;
    toml::from_str::<PantsConfig>(text).ok()
}

/// Milestone 1064 (#924, research R5) — every `[<scope>].lockfile` path
/// outside `[jvm]` and `[python]`, as `(scope, path)` sorted by scope.
///
/// Pants JVM tools declare their lockfile this way rather than as a
/// `[jvm.resolves]` key. Matching is by path, so no scope list is kept here.
/// `<default>` is Pants's built-in lockfile, which has no file in the
/// repository. Unparseable input declares nothing (FR-004).
pub(crate) fn tool_lockfiles(bytes: &[u8]) -> Vec<(String, String)> {
    let Some(doc) = std::str::from_utf8(bytes)
        .ok()
        .and_then(|t| toml::from_str::<toml::Table>(t).ok())
    else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = doc
        .iter()
        .filter(|(scope, _)| !matches!(scope.as_str(), "jvm" | "python"))
        .filter_map(|(scope, section)| {
            let path = section.get("lockfile")?.as_str()?;
            (path != "<default>" && !path.trim().is_empty())
                .then(|| (scope.clone(), path.to_string()))
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn tool_lockfiles_reads_every_other_tables_lockfile_path() {
        let toml = br#"
[jvm.resolves]
main = "3rdparty/jvm/main.lock"

[scalatest]
lockfile = "3rdparty/jvm/scalatest.lock"

[junit]
lockfile = "3rdparty/jvm/testing.lock"

[python]
lockfile = "ignored.lock"

[GLOBAL]
pants_version = "2.31.0"
"#;
        assert_eq!(
            tool_lockfiles(toml),
            vec![
                ("junit".to_string(), "3rdparty/jvm/testing.lock".to_string()),
                ("scalatest".to_string(), "3rdparty/jvm/scalatest.lock".to_string()),
            ]
        );
    }

    #[test]
    fn builtin_tool_lockfile_declares_nothing() {
        assert!(tool_lockfiles(b"[junit]\nlockfile = \"<default>\"\n").is_empty());
    }

    #[test]
    fn non_string_or_missing_lockfile_declares_nothing() {
        assert!(tool_lockfiles(b"[junit]\nlockfile = 3\n[ktlint]\nversion = \"1\"\n").is_empty());
        assert!(tool_lockfiles(b"not = valid = toml =").is_empty());
    }

    #[test]
    fn parse_valid_jvm_section_with_default_resolve_and_resolves() {
        let toml = br#"
[jvm]
default_resolve = "prod"

[jvm.resolves]
prod = "build-support/jvm/prod.lock"
junit = "3rdparty/jvm/junit.lock"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert_eq!(cfg.jvm.default_resolve.as_deref(), Some("prod"));
        assert_eq!(cfg.jvm.resolves.len(), 2);
        assert_eq!(
            cfg.jvm.resolves.get("prod").map(String::as_str),
            Some("build-support/jvm/prod.lock"),
        );
        assert_eq!(
            cfg.jvm.resolves.get("junit").map(String::as_str),
            Some("3rdparty/jvm/junit.lock"),
        );
    }

    #[test]
    fn parse_missing_jvm_section_returns_defaults() {
        let toml = br#"
[python]
lockfile = "unrelated.lock"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert!(cfg.jvm.default_resolve.is_none());
        assert!(cfg.jvm.resolves.is_empty());
    }

    #[test]
    fn parse_jvm_section_without_resolves_returns_empty_map() {
        let toml = br#"
[jvm]
default_resolve = "prod"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert_eq!(cfg.jvm.default_resolve.as_deref(), Some("prod"));
        assert!(cfg.jvm.resolves.is_empty());
    }

    #[test]
    fn parse_malformed_toml_returns_none() {
        let garbage = b"not = valid = toml =";
        assert!(parse(garbage).is_none());
    }
}
