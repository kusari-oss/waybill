//! Milestone 226: minimal `pants.toml` `[golang]` section parser.
//!
//! Parses ONLY `expected_version = "..."` per research §R4. All
//! other keys (`min_dot_version`, `subprocess_env_vars`, etc.)
//! are ignored via serde's default behavior.
//!
//! Fail-open contract (FR-007): missing file, missing key,
//! malformed TOML, non-string values all fall through to `None`
//! so the reader emits zero toolchain component without aborting
//! the scan.

use serde::Deserialize;

/// Minimal `pants.toml` shape recognized by the pants_go reader.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct GoSetupConfig {
    #[serde(default)]
    pub(crate) golang: Option<GolangSection>,
}

/// The one Pants config section we care about here.
#[derive(Debug, Deserialize)]
pub(crate) struct GolangSection {
    /// Operator-pinned minimum Go version. When present + non-empty,
    /// the reader emits a design-tier `pkg:generic/go@<version>`
    /// component. Absent means "operator relies on Pants default"
    /// — waybill emits NO component per FR-008 policy.
    #[serde(default)]
    pub(crate) expected_version: Option<String>,
    // `min_dot_version` is deliberately NOT parsed per spec Out-of-Scope.
}

/// Parse `pants.toml` bytes. Returns `None` on any parse error
/// per FR-007 fail-open.
pub(crate) fn parse(bytes: &[u8]) -> Option<GoSetupConfig> {
    let text = std::str::from_utf8(bytes).ok()?;
    toml::from_str::<GoSetupConfig>(text).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_golang_expected_version() {
        let toml = br#"
[golang]
expected_version = "1.21"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert_eq!(
            cfg.golang.and_then(|g| g.expected_version).as_deref(),
            Some("1.21"),
        );
    }

    #[test]
    fn parse_golang_section_without_expected_version() {
        let toml = br#"
[golang]
min_dot_version = "1.21"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert!(cfg.golang.and_then(|g| g.expected_version).is_none());
    }

    #[test]
    fn parse_missing_golang_section_returns_defaults() {
        let toml = br#"
[python]
lockfile = "unrelated.lock"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert!(cfg.golang.is_none());
    }

    #[test]
    fn parse_malformed_toml_returns_none() {
        let garbage = b"not = valid = toml =";
        assert!(parse(garbage).is_none());
    }
}
