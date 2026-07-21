//! Shared RFC-822-style control-file stanza parser.
//!
//! Used by:
//! - `dpkg.rs` — `/var/lib/dpkg/status` and per-package `status.d/`
//! - `opkg.rs` — `/var/lib/opkg/status` (milestone 107)
//!
//! Both files use byte-identical syntax: `Field-Name: value` lines,
//! optional continuation lines (start with whitespace) extending the
//! preceding field, blank-line-separated stanzas. Field names are
//! compared case-insensitively (the parser lowercases at insert time).
//!
//! **Behavior contract** (preserved verbatim from the prior dpkg.rs
//! inline parser):
//! - First-occurrence wins on duplicate field names (rare in practice,
//!   but dpkg's parser uses `iter().find()` which finds the first; we
//!   match that semantics).
//! - Continuation lines (any line starting with space or tab) append a
//!   `\n` + the trimmed continuation text to the preceding field's
//!   value. If no field precedes (malformed stanza starting with a
//!   continuation), the continuation is silently dropped.
//! - Lines without a `:` separator are silently dropped (matches
//!   dpkg's `split_once(':')` short-circuit).
//! - Empty stanza strings (after blank-line splitting) yield an empty
//!   `ControlStanza` with no fields. Caller decides whether to drop
//!   these — `parse_stanzas` filters them.
//!
//! This module MUST stay net behavior-neutral relative to dpkg's prior
//! inline parser. The 33 byte-identity goldens are the regression
//! gate.

use std::collections::BTreeMap;

/// One parsed stanza. Field-name keys are lowercased at insert time;
/// values are stored verbatim (trimmed, with `\n` joining continuation
/// lines).
#[derive(Clone, Debug, Default)]
pub(super) struct ControlStanza {
    fields: BTreeMap<String, String>,
}

// Several named-accessor methods are added now for upcoming opkg.rs
// consumption (milestone 107 US1). dpkg.rs only uses `.get(...)` today,
// so the named accessors look unused — silence the warning until US1
// lands.
#[allow(dead_code)]
impl ControlStanza {
    /// Lookup a field by name (case-insensitive). Returns the verbatim
    /// stored value (trimmed at parse time, continuation-joined with
    /// `\n`). `None` when the field is absent.
    pub(super) fn get(&self, name: &str) -> Option<&str> {
        let key = name.to_ascii_lowercase();
        self.fields.get(&key).map(String::as_str)
    }

    pub(super) fn name(&self) -> Option<&str> {
        self.get("package")
    }

    pub(super) fn version(&self) -> Option<&str> {
        self.get("version")
    }

    pub(super) fn architecture(&self) -> Option<&str> {
        self.get("architecture")
    }

    pub(super) fn maintainer(&self) -> Option<&str> {
        self.get("maintainer")
    }

    pub(super) fn license(&self) -> Option<&str> {
        self.get("license")
    }

    pub(super) fn depends(&self) -> Option<&str> {
        self.get("depends")
    }

    pub(super) fn status(&self) -> Option<&str> {
        self.get("status")
    }

    pub(super) fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Insert a field (first-wins on duplicates, matching the prior
    /// dpkg parser's `iter().find()` semantics).
    fn insert_first_wins(&mut self, name: &str, value: String) {
        let key = name.to_ascii_lowercase();
        self.fields.entry(key).or_insert(value);
    }

    /// Append a continuation line to the most-recently-inserted field's
    /// value. Tracking "most-recently-inserted" is done by the parser
    /// loop, not the map — the parser remembers the last-inserted key
    /// in a separate variable.
    fn append_continuation_to(&mut self, last_key: &str, line: &str) {
        if let Some(value) = self.fields.get_mut(last_key) {
            value.push('\n');
            value.push_str(line.trim_start());
        }
    }
}

/// Parse a full control file (multi-stanza). Stanzas are blank-line-
/// separated; empty stanzas are filtered out.
pub(super) fn parse_stanzas(text: &str) -> Vec<ControlStanza> {
    let mut out = Vec::new();
    let mut cur = ControlStanza::default();
    let mut cur_has_content = false;
    let mut last_key: Option<String> = None;

    for line in text.lines() {
        if line.trim().is_empty() {
            // Stanza boundary.
            if cur_has_content {
                out.push(std::mem::take(&mut cur));
                cur_has_content = false;
                last_key = None;
            }
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line — extend the last field.
            if let Some(key) = &last_key {
                cur.append_continuation_to(key, line);
            }
            // If no field has been opened yet, silently drop (matches
            // prior behavior — dpkg's loop ignored continuation lines
            // before the first field via `if let Some(last) = ...`).
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let value = v.trim().to_string();
            cur.insert_first_wins(&key, value);
            // Even if first-wins kept the prior value, the
            // continuation-attribution last_key still moves forward
            // to whatever field the current line names. This matches
            // dpkg's prior behavior where `fields.last_mut()` after
            // pushing always pointed at the most-recently-PARSED
            // field, regardless of whether `Vec::push` actually
            // changed semantics. (In practice duplicate fields are
            // ~never present so this distinction is academic.)
            last_key = Some(key);
            cur_has_content = true;
        }
        // Lines without `:` are silently dropped (preserves prior
        // dpkg parser behavior).
    }
    if cur_has_content {
        out.push(cur);
    }
    out
}

/// Milestone 169 (T005, closes #500 US2 Q2 clarification) — parse a
/// Debian-style `Depends:` field with alternative-list syntax.
///
/// Input: raw `Depends:` field text (may contain commas separating
/// multiple deps + `|` separating alternatives within each dep).
///
/// Semantics (per m169 Q2 clarification 2026-07-06):
///
/// - `Depends: pkg-a, pkg-b` → `resolved = ["pkg-a", "pkg-b"]` (2 edges; no alternatives).
/// - `Depends: pkg-a | pkg-b, pkg-c` → `resolved = ["pkg-a", "pkg-c"]` + `alternates_by_source["pkg-a"] = ["pkg-b"]` (first-wins matches opkg runtime default; fallback preserved for downstream consumers via `waybill:dep-alternative-alternates` annotation).
/// - Version constraints in parens are ignored for the resolved list per existing dpkg/opkg reader precedent.
///
/// Consumed by `ipk_file.rs` (m169 US1 T011, wired up 2026-07-06) and
/// scheduled for `opkg.rs` (m169 US2 hardening T021).
pub(super) fn parse_depends_field_with_alternatives(
    raw: &str,
) -> DepsWithAlternatives {
    let mut resolved: Vec<String> = Vec::new();
    let mut alternates_by_source: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for chunk in raw.split(',') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        // Split into `|`-separated alternatives (opkg alt-list syntax).
        let alts: Vec<String> = chunk
            .split('|')
            .map(|s| strip_version_constraint(s.trim()))
            .filter(|s| !s.is_empty())
            .collect();
        if alts.is_empty() {
            continue;
        }
        let first = alts[0].clone();
        resolved.push(first.clone());
        if alts.len() > 1 {
            alternates_by_source.insert(first, alts[1..].to_vec());
        }
    }

    DepsWithAlternatives {
        resolved,
        alternates_by_source,
    }
}

/// Trim trailing version constraints like `(>= 1.2)` from a package
/// name — matches the existing dpkg/opkg reader convention where the
/// dep target is name-only.
fn strip_version_constraint(s: &str) -> String {
    match s.find('(') {
        Some(i) => s[..i].trim().to_string(),
        None => s.to_string(),
    }
}

/// Return value of [`parse_depends_field_with_alternatives`].
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct DepsWithAlternatives {
    /// First-wins dep names — feeds `PackageDbEntry.depends` +
    /// dependsOn edges.
    pub(super) resolved: Vec<String>,
    /// Fallback alternatives per source-dep name — feeds the
    /// `waybill:dep-alternative-alternates` annotation on the SOURCE
    /// component. Key = the first-wins name in `resolved`; value =
    /// non-empty list of fallback names.
    pub(super) alternates_by_source:
        std::collections::HashMap<String, Vec<String>>,
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parses_single_stanza() {
        let text = "Package: foo\nVersion: 1.0\nArchitecture: amd64\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas.len(), 1);
        assert_eq!(stanzas[0].name(), Some("foo"));
        assert_eq!(stanzas[0].version(), Some("1.0"));
        assert_eq!(stanzas[0].architecture(), Some("amd64"));
    }

    #[test]
    fn parses_multi_stanza() {
        let text = "Package: foo\nVersion: 1.0\n\nPackage: bar\nVersion: 2.0\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas.len(), 2);
        assert_eq!(stanzas[0].name(), Some("foo"));
        assert_eq!(stanzas[1].name(), Some("bar"));
    }

    #[test]
    fn merges_multiline_continuation() {
        let text = "Package: foo\nDescription: first line\n second line\n third line\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas.len(), 1);
        assert_eq!(
            stanzas[0].get("description"),
            Some("first line\nsecond line\nthird line")
        );
    }

    #[test]
    fn tolerates_unknown_fields() {
        let text = "Package: foo\nVersion: 1.0\nVendor-Extension-Field: hello\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas.len(), 1);
        assert_eq!(
            stanzas[0].get("vendor-extension-field"),
            Some("hello")
        );
    }

    #[test]
    fn skips_malformed_lines_silently() {
        // Lines without `:` are dropped without warning (preserves
        // prior dpkg parser behavior).
        let text = "Package: foo\nthis-line-has-no-colon\nVersion: 1.0\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas.len(), 1);
        assert_eq!(stanzas[0].name(), Some("foo"));
        assert_eq!(stanzas[0].version(), Some("1.0"));
    }

    #[test]
    fn handles_empty_input() {
        assert!(parse_stanzas("").is_empty());
    }

    #[test]
    fn handles_blank_line_at_eof() {
        let text = "Package: foo\nVersion: 1.0\n\n\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas.len(), 1);
    }

    #[test]
    fn case_insensitive_field_names() {
        let text = "PACKAGE: foo\nversion: 1.0\nArChItEcTuRe: amd64\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas[0].name(), Some("foo"));
        assert_eq!(stanzas[0].version(), Some("1.0"));
        assert_eq!(stanzas[0].architecture(), Some("amd64"));
    }

    #[test]
    fn first_wins_on_duplicate_field_names() {
        // Matches the prior dpkg parser's `iter().find()` semantics.
        let text = "Package: first\nPackage: second\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas[0].name(), Some("first"));
    }

    #[test]
    fn description_continuation_correctly_merged() {
        // Exercises the multi-line `Description:` field that's the
        // most common continuation case in real-world dpkg DBs.
        let text = "Package: foo\nDescription: Short summary\n A longer paragraph that\n spans multiple lines.\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(
            stanzas[0].get("description"),
            Some("Short summary\nA longer paragraph that\nspans multiple lines.")
        );
    }

    #[test]
    fn continuation_before_any_field_silently_dropped() {
        // Malformed: stanza starts with a continuation line.
        let text = " orphan continuation\nPackage: foo\n";
        let stanzas = parse_stanzas(text);
        assert_eq!(stanzas[0].name(), Some("foo"));
    }

    // ------------------------------------------------------------
    // Milestone 169 T005 — parse_depends_field_with_alternatives
    // (Q2 clarification: first-wins + alternates recorded)
    // ------------------------------------------------------------

    #[test]
    fn depends_simple_comma_separated_no_alternatives() {
        let d = parse_depends_field_with_alternatives("libc, zlib");
        assert_eq!(d.resolved, vec!["libc".to_string(), "zlib".to_string()]);
        assert!(d.alternates_by_source.is_empty());
    }

    #[test]
    fn depends_alternative_list_first_wins() {
        // `pkg-a | pkg-b` per Q2: first-alt goes to resolved; fallbacks
        // to alternates_by_source keyed by the first-alt name.
        let d = parse_depends_field_with_alternatives("libmbedtls-12 | libssl3");
        assert_eq!(d.resolved, vec!["libmbedtls-12".to_string()]);
        assert_eq!(
            d.alternates_by_source.get("libmbedtls-12"),
            Some(&vec!["libssl3".to_string()])
        );
    }

    #[test]
    fn depends_mixed_alternatives_and_simple_deps() {
        let d = parse_depends_field_with_alternatives(
            "libc, libmbedtls-12 | libssl3 | libssl1.1, zlib",
        );
        assert_eq!(
            d.resolved,
            vec![
                "libc".to_string(),
                "libmbedtls-12".to_string(),
                "zlib".to_string(),
            ]
        );
        assert_eq!(
            d.alternates_by_source.get("libmbedtls-12"),
            Some(&vec!["libssl3".to_string(), "libssl1.1".to_string()])
        );
        assert!(!d.alternates_by_source.contains_key("libc"));
        assert!(!d.alternates_by_source.contains_key("zlib"));
    }

    #[test]
    fn depends_strips_version_constraints() {
        // Existing dpkg/opkg reader convention: dep target is
        // name-only; `(>= 1.2)` constraints are ignored.
        let d = parse_depends_field_with_alternatives("libc (>= 1.2), zlib (>= 1.0)");
        assert_eq!(d.resolved, vec!["libc".to_string(), "zlib".to_string()]);
    }

    #[test]
    fn depends_empty_input_returns_empty() {
        let d = parse_depends_field_with_alternatives("");
        assert!(d.resolved.is_empty());
        assert!(d.alternates_by_source.is_empty());
    }

    #[test]
    fn depends_trailing_pipe_ignored_gracefully() {
        // `pkg-a | ` — trailing pipe with empty alternative.
        let d = parse_depends_field_with_alternatives("libc | ");
        assert_eq!(d.resolved, vec!["libc".to_string()]);
        assert!(d.alternates_by_source.is_empty());
    }
}
