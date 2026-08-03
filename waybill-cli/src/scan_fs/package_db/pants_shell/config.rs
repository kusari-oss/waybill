//! Milestone 225: minimal `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` parser.
//!
//! Parses ONLY `version = "..."` per subsystem section (per research
//! §R4). All other keys (`known_versions`, `install_from_resolve`,
//! etc.) are ignored via serde's default behavior.
//!
//! Fail-open contract (FR-004): missing file, missing keys, malformed
//! TOML, non-string values all fall through to `None` so the reader
//! emits zero tool components without aborting the scan.
//!
//! Distinct from `super::super::pants::config` (m223, `[python]`) and
//! `super::super::pants_jvm::config` (m224, `[jvm]`) by design — each
//! reader parses only its own subsystem sections so schema changes in
//! one don't cascade.

use serde::Deserialize;

/// Minimal `pants.toml` shape recognized by the shell reader.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ShellSetupConfig {
    #[serde(default)]
    pub(crate) shellcheck: Option<ExternalToolSection>,
    #[serde(default)]
    pub(crate) shfmt: Option<ExternalToolSection>,
    #[serde(default)]
    pub(crate) shunit2: Option<ExternalToolSection>,
}

/// One subsystem section's `[external_tool]` shape.
#[derive(Debug, Deserialize)]
pub(crate) struct ExternalToolSection {
    /// Operator-pinned version string. `None` means "operator relies
    /// on Pants default" — waybill emits NO component in that case.
    #[serde(default)]
    pub(crate) version: Option<String>,
}

/// Parse `pants.toml` bytes. Returns `None` on any parse error per
/// FR-004 fail-open. The caller should log a WARN naming the file
/// and continue without tool components.
pub(crate) fn parse(bytes: &[u8]) -> Option<ShellSetupConfig> {
    let text = std::str::from_utf8(bytes).ok()?;
    toml::from_str::<ShellSetupConfig>(text).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_all_three_sections_with_version() {
        let toml = br#"
[shellcheck]
version = "v0.9.0"

[shfmt]
version = "v3.7.0"

[shunit2]
version = "2.1.8"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert_eq!(
            cfg.shellcheck.as_ref().and_then(|s| s.version.as_deref()),
            Some("v0.9.0"),
        );
        assert_eq!(
            cfg.shfmt.as_ref().and_then(|s| s.version.as_deref()),
            Some("v3.7.0"),
        );
        assert_eq!(
            cfg.shunit2.as_ref().and_then(|s| s.version.as_deref()),
            Some("2.1.8"),
        );
    }

    #[test]
    fn parse_only_shellcheck() {
        let toml = br#"
[shellcheck]
version = "v0.9.0"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert!(cfg.shellcheck.is_some());
        assert!(cfg.shfmt.is_none());
        assert!(cfg.shunit2.is_none());
    }

    #[test]
    fn parse_shellcheck_without_version_key() {
        let toml = br#"
[shellcheck]
known_versions = ["v0.9.0|linux_x86_64|<sha>|<size>"]
"#;
        let cfg = parse(toml).expect("valid toml parses");
        let sc = cfg.shellcheck.expect("section present");
        assert!(sc.version.is_none());
    }

    #[test]
    fn parse_malformed_toml_returns_none() {
        let garbage = b"not = valid = toml =";
        assert!(parse(garbage).is_none());
    }
}
