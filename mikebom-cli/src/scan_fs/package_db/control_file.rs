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
}
