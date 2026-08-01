//! Milestone 223: minimal `pants.toml` parser.
//!
//! Parses ONLY the `[python].lockfile` key per research.md §R4 —
//! coupling to the full Pants config schema is a maintenance burden
//! we intentionally avoid. Every other section + unknown value type
//! is gracefully ignored via `#[serde(default)]` on every field.
//!
//! Fail-open contract (FR-004): missing file, missing key, malformed
//! TOML, and non-string values all fall through to `None` so the
//! reader can fall back to the default `3rdparty/python/*.lock` glob.

use serde::Deserialize;

/// Minimal `pants.toml` shape. Everything outside `[python]` is
/// ignored. If `[python]` is missing, `python` defaults to an empty
/// `PythonSection`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct PantsConfig {
    #[serde(default)]
    pub(crate) python: PythonSection,
}

/// The one Pants config section we care about.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct PythonSection {
    /// Custom lockfile path override. Interpreted relative to the scan
    /// root. Absent → use FR-001 default glob.
    #[serde(default)]
    pub(crate) lockfile: Option<String>,
}

/// Parse `pants.toml` bytes. Returns `None` on any parse error (per
/// FR-004 fail-open) — the caller should log a WARN naming the file
/// and fall back to the default lockfile discovery glob.
pub(crate) fn parse(bytes: &[u8]) -> Option<PantsConfig> {
    let text = std::str::from_utf8(bytes).ok()?;
    toml::from_str::<PantsConfig>(text).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_python_lockfile_path() {
        let toml = br#"
[python]
lockfile = "build-support/py.lock"
"#;
        let cfg = parse(toml).expect("valid toml parses");
        assert_eq!(
            cfg.python.lockfile.as_deref(),
            Some("build-support/py.lock")
        );
    }

    #[test]
    fn parse_missing_python_section_returns_default() {
        let toml = b"[jvm]\nlockfile = \"unrelated.lock\"\n";
        let cfg = parse(toml).expect("valid toml parses");
        assert!(cfg.python.lockfile.is_none());
    }

    #[test]
    fn parse_malformed_toml_returns_none() {
        let garbage = b"not = valid = toml =";
        assert!(parse(garbage).is_none());
    }
}
