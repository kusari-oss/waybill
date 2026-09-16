//! Milestone 223 + 672: minimal `pants.toml` parser.
//!
//! Parses `[python].lockfile` (m223 singular) AND `[python.resolves]`
//! (m672 Pants 2.x map, bare-string values only) per research.md §R4
//! and `specs/672-pants-reader-follow-up/research.md` §R3. Coupling to
//! the full Pants config schema is a maintenance burden we
//! intentionally avoid. Every other section + unknown value type is
//! gracefully ignored via `#[serde(default)]` on every field.
//!
//! Fail-open contract (FR-004): missing file, missing key, malformed
//! TOML, and non-string values all fall through to `None`/empty-map
//! so the reader can fall back to the default `3rdparty/python/*.lock`
//! glob. Non-bare-string `[python.resolves]` values are captured as
//! `toml::Value` here so the caller can name the observed TOML type
//! in its WARN log per m672 FR-007 (see
//! `specs/672-pants-reader-follow-up/contracts/python_resolves_map.md`
//! C3).

use std::collections::BTreeMap;

use serde::Deserialize;

/// Minimal `pants.toml` shape. Everything outside `[python]` is
/// ignored. If `[python]` is missing, `python` defaults to an empty
/// `PythonSection`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct PantsConfig {
    #[serde(default)]
    pub(crate) python: PythonSection,
    /// Milestone 868 (#887): every other top-level table, captured so
    /// tool sections can be inspected for `install_from_resolve`.
    ///
    /// Pants declares a tool's resolve from the tool's own section —
    /// `[black] install_from_resolve = "black"` — not from
    /// `[python.resolves]`, which is the full registry and lists
    /// application and tool lockfiles alike. That back-reference is the
    /// only DECLARATIVE statement that a resolve exists to serve
    /// tooling; the alternative, matching resolve names against a list
    /// of known tool names, is a guess that is already wrong in
    /// practice (see `resolve_classifier`).
    #[serde(flatten)]
    #[allow(dead_code)] // Read via `tool_declared_resolves` (T028 classifier).
    pub(crate) other_sections: BTreeMap<String, toml::Value>,
}

impl PantsConfig {
    /// Resolve names a tool section declares it installs from.
    ///
    /// Knowingly PARTIAL, and that is the point: on the measured target
    /// five of nine resolves carry the back-reference while `towncrier`
    /// and `pants-plugins` are tooling that nothing declares as such.
    /// Callers must treat absence as "undeclared" and fall back
    /// visibly (FR-003a-i / FR-003c) rather than inferring from the
    /// resolve's name or its lockfile path.
    #[allow(dead_code)] // Consumed by the T028 classifier precedence change.
    pub(crate) fn tool_declared_resolves(&self) -> std::collections::BTreeSet<String> {
        self.other_sections
            .values()
            .filter_map(|v| v.get("install_from_resolve"))
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect()
    }
}

/// The one Pants config section we care about.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct PythonSection {
    /// Milestone 223: pre-2.x singular lockfile path override.
    /// Interpreted relative to the scan root. Absent → use FR-001
    /// default glob. Preserved for backward compatibility; a repo can
    /// declare both this AND [`resolves`] and both are honored
    /// (superset union per m672 FR-006).
    #[serde(default)]
    pub(crate) lockfile: Option<String>,
    /// Milestone 672: Pants 2.x `[python.resolves]` map — key is the
    /// operator-supplied resolve name (e.g. `mypy`, `internal-libs`);
    /// value is the filesystem path (bare TOML string).
    ///
    /// Value type is `toml::Value` (not `String`) so non-bare-string
    /// entries don't fail the whole `pants.toml` parse — the caller
    /// checks each entry's `as_str()` and WARNs+skips shape-drift
    /// entries per m672 FR-007. `BTreeMap` preserves lexical key
    /// order for deterministic dedup + WARN log ordering.
    ///
    /// Empty map (`resolves = {}` or absent) is equivalent to the
    /// pre-m672 no-op case — no discovery additions, no WARN.
    #[serde(default)]
    #[allow(dead_code)] // Read by T012 map-walk in `discover_lockfiles`.
    pub(crate) resolves: BTreeMap<String, toml::Value>,
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
    fn m868_tool_sections_declare_their_resolves() {
        // #887 / T005. The declarative signal is a tool's own section
        // back-referencing a resolve, NOT membership of [python.resolves] —
        // that map is the full registry and lists application and tool
        // lockfiles alike, so it cannot discriminate.
        let cfg = parse(
            br#"
[python]
interpreter_constraints = ["CPython==3.13.7"]

[python.resolves]
app-default = "python.lock"
waybill-fixture-linter = "tools/waybill-fixture-linter.lock"
waybill-fixture-undeclared = "tools/waybill-fixture-undeclared.lock"

[waybill-fixture-linter]
install_from_resolve = "waybill-fixture-linter"
"#,
        )
        .expect("pants.toml parses");

        // The registry lists all three...
        assert_eq!(cfg.python.resolves.len(), 3);

        // ...but only one is DECLARED as serving a tool.
        let declared = cfg.tool_declared_resolves();
        assert!(declared.contains("waybill-fixture-linter"));
        assert!(
            !declared.contains("app-default"),
            "an application resolve must not be picked up as tool-declared"
        );
        assert!(
            !declared.contains("waybill-fixture-undeclared"),
            "a resolve under tools/ that nothing declares must stay undeclared — \
             the path is a convention, not a declaration (FR-003a-i)"
        );
        assert_eq!(declared.len(), 1);
    }

    #[test]
    fn m868_no_tool_sections_yields_no_declarations() {
        let cfg = parse(
            br#"
[python.resolves]
app-default = "python.lock"
"#,
        )
        .expect("parses");
        assert!(cfg.tool_declared_resolves().is_empty());
    }

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

    /// Milestone 672 T003 (US2): the `[python.resolves]` map deserializes
    /// with bare-string values; mixed bare-string + table entries also
    /// deserialize (the caller-side WARN+skip on table entries is
    /// covered by T012/T016, not this unit test).
    #[test]
    fn parse_python_resolves_map_bare_strings_only() {
        // (a) empty map → empty `resolves`.
        let empty_toml = br#"
[python]
lockfile = "legacy.lock"
"#;
        let cfg = parse(empty_toml).expect("valid toml parses");
        assert!(
            cfg.python.resolves.is_empty(),
            "resolves must default to empty when absent"
        );

        // (b) all-bare-string map → correct key/value population.
        let bare_toml = br#"
[python.resolves]
mypy = "build-support/py/mypy.lock"
internal-libs = "3rdparty/python/internal-libs.lock"
user_reqs = "3rdparty/python/user_reqs.lock"
"#;
        let cfg = parse(bare_toml).expect("valid toml parses");
        assert_eq!(cfg.python.resolves.len(), 3);
        assert_eq!(
            cfg.python.resolves.get("mypy").and_then(|v| v.as_str()),
            Some("build-support/py/mypy.lock")
        );
        assert_eq!(
            cfg.python
                .resolves
                .get("internal-libs")
                .and_then(|v| v.as_str()),
            Some("3rdparty/python/internal-libs.lock")
        );
        // BTreeMap preserves lex order.
        let keys: Vec<&str> =
            cfg.python.resolves.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["internal-libs", "mypy", "user_reqs"]);

        // (c) mixed bare-string + table entries: `toml::from_str`
        // succeeds — table entry lands as `Value::Table`. Caller-side
        // WARN+skip is exercised in the integration tests.
        let mixed_toml = br#"
[python.resolves]
bare-string = "3rdparty/python/bare.lock"
[python.resolves.table-shape]
path = "3rdparty/python/table.lock"
"#;
        let cfg = parse(mixed_toml).expect("mixed toml parses");
        assert_eq!(cfg.python.resolves.len(), 2);
        assert_eq!(
            cfg.python
                .resolves
                .get("bare-string")
                .and_then(|v| v.as_str()),
            Some("3rdparty/python/bare.lock")
        );
        // `table-shape` lands as a Table — `.as_str()` returns None.
        let table_entry = cfg
            .python
            .resolves
            .get("table-shape")
            .expect("table entry present");
        assert!(table_entry.as_str().is_none());
        assert!(table_entry.is_table());
    }
}
