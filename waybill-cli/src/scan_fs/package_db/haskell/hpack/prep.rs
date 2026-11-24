//! Spec 1649 (#1673, #1674): read `!include` and YAML merge keys the way
//! hpack reads them, before the manifest reaches `serde_yaml`.
//!
//! # Why the text is prepared, not the parsed value
//!
//! Two measured facts force this (research R1, R3; hpack 0.38.3):
//!
//! - **Anchors cross include boundaries in both directions.** An included
//!   file may define `&lib` for the manifest to use, or use `*d` that the
//!   manifest defined before the include, and an anchor redefined later wins
//!   even when the redefinition comes from an include. hpack resolves
//!   `!include` on the YAML event stream, so the included document behaves
//!   as if it were written in place. Parsing each file separately cannot
//!   reproduce that: an alias to an anchor in another file fails to parse
//!   (`unknown anchor`). Splicing the included text in place, re-indented,
//!   reproduces it exactly.
//! - **`serde_yaml` refuses a repeated key**, and hpack's pattern repeats
//!   `<<` (two merges in one mapping). Each `<<` key is therefore renamed to
//!   a numbered unique key, parsed, and folded in afterwards under hpack's
//!   precedence. `serde_yaml`'s own `apply_merge` is not used: it rejects
//!   the repeated form this exists to read.
//!
//! # Precedence (measured)
//!
//! Explicit keys beat merged ones, whichever comes first in the file. Among
//! merges the first wins, and the sequence form `<<: [*a, *b]` behaves the
//! same. A merge whose value is not a mapping (such as a sequence anchor)
//! contributes nothing.
//!
//! # Bounds (FR-004)
//!
//! An include is read only from a file under the scanned root, never twice on
//! one chain, and no deeper than [`MAX_INCLUDE_DEPTH`]. One that cannot be
//! resolved is left in place as a tag. The existing `strip_tagged` then
//! removes it, and the manifest is reported `partial`, with the reason in
//! C200. Nothing is fetched (FR-006).

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde_yaml::{Mapping, Value};

/// The deepest include resolved, counting the manifest's own includes as
/// depth 1. The measured corpus maximum is 2 (research R2).
pub(crate) const MAX_INCLUDE_DEPTH: usize = 8;

/// The renamed form of a `<<` key, followed by its document-order number.
const MERGE_KEY_PREFIX: &str = "<<__waybill_merge_";

/// Why an `!include` was not read (FR-004, R6).
///
/// An enum matched exhaustively (Principle IV); rendered only in C200.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum UnresolvedReason {
    /// No such file.
    Missing,
    /// The file resolves outside the scanned root, and is not read.
    OutsideRoot,
    /// The file is already being included on this chain.
    Cycle,
    /// Deeper than [`MAX_INCLUDE_DEPTH`].
    TooDeep,
    /// Present but could not be read.
    Unreadable,
    /// In a position the splice does not cover, such as flow syntax.
    UnsupportedPosition,
}

impl UnresolvedReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::OutsideRoot => "outside-root",
            Self::Cycle => "cycle",
            Self::TooDeep => "too-deep",
            Self::Unreadable => "unreadable",
            Self::UnsupportedPosition => "unsupported-position",
        }
    }
}

/// What happened to one `!include`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IncludeOutcome {
    Resolved,
    Unresolved(UnresolvedReason),
}

/// One `!include`, attributed to its manifest (FR-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncludeResolution {
    /// The `package.yaml` the include belongs to.
    pub(crate) manifest: PathBuf,
    /// The file containing the `!include`: the manifest, or an included file.
    pub(crate) file: PathBuf,
    /// The path as written.
    pub(crate) include: String,
    pub(crate) outcome: IncludeOutcome,
}

/// Prepare a manifest's text for `serde_yaml`: splice its includes, then
/// rename its merge keys. Returns the text and every include met.
///
/// A manifest naming neither construct is returned unchanged, without being
/// scanned (FR-007).
pub(crate) fn prepare(text: &str, manifest: &Path, root: &Path) -> (String, Vec<IncludeResolution>) {
    if !text.contains("!include") && !text.contains("<<") {
        return (text.to_string(), Vec::new());
    }
    let mut splicer = Splicer {
        manifest: manifest.to_path_buf(),
        root: std::fs::canonicalize(root).ok(),
        resolutions: Vec::new(),
    };
    let mut chain: Vec<PathBuf> = std::fs::canonicalize(manifest).into_iter().collect();
    let spliced = if text.contains("!include") {
        splicer.splice(text, manifest, &mut chain)
    } else {
        text.to_string()
    };
    (rename_merge_keys(&spliced), splicer.resolutions)
}

// ───────────────────────────────────────────────────────────────────────
// Line structure
// ───────────────────────────────────────────────────────────────────────

/// One line, split into the parts the splice and the rename need.
struct LineParts<'a> {
    /// Leading spaces.
    indent: &'a str,
    /// Zero or more `- ` sequence markers after the indent.
    lead: &'a str,
    /// The mapping key, when the line has one, without its `:`.
    key: Option<&'a str>,
    /// What follows the key's `: `, or the whole rest when there is no key.
    value: &'a str,
}

impl<'a> LineParts<'a> {
    fn parse(line: &'a str) -> Self {
        let body = line.trim_start_matches(' ');
        let indent = &line[..line.len() - body.len()];
        let mut lead_len = 0;
        while let Some(after) = body[lead_len..].strip_prefix("- ") {
            lead_len += 2 + after.len() - after.trim_start_matches(' ').len();
        }
        let lead = &body[..lead_len];
        let rest = &body[lead_len..];
        match split_key(rest) {
            Some((key, value)) => Self { indent, lead, key: Some(key), value },
            None => Self { indent, lead, key: None, value: rest },
        }
    }

    /// The column of the node this line opens: the key's, or the item's.
    fn column(&self) -> usize {
        self.indent.len() + self.lead.len()
    }
}

/// Split `key: value` at the key's `:`. A key is any quoted scalar, or any
/// plain scalar ending at the first `: ` (slashes included: the corpus's
/// commonest include is `_common/lib: !include "../package-common.yaml"`).
fn split_key(rest: &str) -> Option<(&str, &str)> {
    let first = rest.chars().next()?;
    let key_end = match first {
        '"' | '\'' => {
            let close = quoted_end(rest, first)?;
            let after = rest[close..].trim_start_matches(' ');
            if !after.starts_with(':') {
                return None;
            }
            rest.len() - after.len()
        }
        '#' | '[' | ']' | '{' | '}' | '&' | '*' | '!' | '|' | '>' | '%' | '@' | '`' | ',' => {
            return None;
        }
        _ => {
            let bytes = rest.as_bytes();
            let mut found = None;
            for (i, b) in bytes.iter().enumerate() {
                if *b == b'#' && i > 0 && bytes[i - 1] == b' ' {
                    return None;
                }
                if *b == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ') {
                    found = Some(i);
                    break;
                }
            }
            found?
        }
    };
    let key = rest[..key_end].trim_end_matches(' ');
    let value = rest[key_end + 1..].trim_start_matches(' ');
    Some((key, value))
}

/// The byte index just past the closing quote of a scalar starting with
/// `q` at index 0, or `None` when it does not close on this line.
fn quoted_end(s: &str, q: char) -> Option<usize> {
    closing_quote(&s[1..], q).map(|i| i + 2)
}

/// The byte index of the quote that closes a `q`-quoted scalar continuing
/// in `s`, honouring `\` escapes in `"` and `''` in `'`.
fn closing_quote(s: &str, q: char) -> Option<usize> {
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if q == '"' && c == '\\' {
            it.next();
        } else if c == q {
            if q == '\'' && it.peek().is_some_and(|(_, n)| *n == '\'') {
                it.next();
            } else {
                return Some(i);
            }
        }
    }
    None
}

/// For each line, whether it is inside a block scalar or a multi-line
/// quoted scalar, where nothing that looks like a key or a tag is one
/// (research R4).
fn opaque_lines(lines: &[&str]) -> Vec<bool> {
    let mut out = vec![false; lines.len()];
    let mut block_owner: Option<usize> = None;
    let mut open_quote: Option<char> = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(q) = open_quote {
            out[i] = true;
            if closing_quote(line, q).is_some() {
                open_quote = None;
            }
            continue;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        if let Some(owner) = block_owner {
            if line.trim().is_empty() || indent > owner {
                out[i] = true;
                continue;
            }
            block_owner = None;
        }
        let parts = LineParts::parse(line);
        let value = without_comment(parts.value);
        if is_block_indicator(value) {
            block_owner = Some(if parts.key.is_some() { parts.column() } else { indent });
        } else if let Some(q @ ('"' | '\'')) = value.chars().next() {
            if quoted_end(value, q).is_none() {
                open_quote = Some(q);
            }
        }
    }
    out
}

/// `|` or `>`, with optional chomping and indentation indicators.
fn is_block_indicator(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('|' | '>'))
        && chars.as_str().len() <= 2
        && chars.all(|c| c == '+' || c == '-' || c.is_ascii_digit())
}

/// A value with a trailing ` # comment` removed. Quoted values are left
/// alone; a `#` inside one is not a comment.
fn without_comment(value: &str) -> &str {
    if value.starts_with('#') {
        return "";
    }
    if value.starts_with('"') || value.starts_with('\'') {
        return value.trim_end();
    }
    match value.find(" #") {
        Some(i) => value[..i].trim_end(),
        None => value.trim_end(),
    }
}

// ───────────────────────────────────────────────────────────────────────
// T013, T016 — the splice
// ───────────────────────────────────────────────────────────────────────

/// `!include <path>`, the path quoted or plain, as a whole value.
fn include_value() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    #[allow(clippy::unwrap_used)]
    RE.get_or_init(|| Regex::new(r#"^!include\s+("([^"]*)"|'([^']*)'|([^\s"'#]+))\s*$"#).unwrap())
}

/// An `!include` anywhere, for reporting one the splice cannot place.
fn include_anywhere() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    #[allow(clippy::unwrap_used)]
    RE.get_or_init(|| Regex::new(r#"!include\s+("([^"]*)"|'([^']*)'|([^\s,}\]"'#]+))"#).unwrap())
}

fn captured_path(c: &regex::Captures<'_>) -> String {
    c.get(2).or_else(|| c.get(3)).or_else(|| c.get(4)).map(|m| m.as_str().to_string()).unwrap_or_default()
}

struct Splicer {
    manifest: PathBuf,
    /// The canonical scanned root; `None` when it cannot be canonicalised,
    /// in which case no include is read (fail closed).
    root: Option<PathBuf>,
    resolutions: Vec<IncludeResolution>,
}

impl Splicer {
    /// `text` with every block-context `!include` replaced by its file's
    /// spliced text, re-indented under the key or item that named it.
    /// `chain` holds the canonical paths of the files being included, the
    /// manifest first.
    fn splice(&mut self, text: &str, file: &Path, chain: &mut Vec<PathBuf>) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let opaque = opaque_lines(&lines);
        let mut out = String::with_capacity(text.len());
        for (line, opaque) in lines.iter().zip(opaque) {
            if opaque || !line.contains("!include") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let parts = LineParts::parse(line);
            let value = without_comment(parts.value);
            let target = include_value().captures(value).map(|c| captured_path(&c));
            let Some(include) = target.filter(|_| parts.key.is_some() || !parts.lead.is_empty()) else {
                self.unsupported(value, file);
                out.push_str(line);
                out.push('\n');
                continue;
            };
            match self.resolve(&include, file, chain) {
                Ok((display, canonical, body)) => {
                    chain.push(canonical);
                    let spliced = self.splice(&body, &display, chain);
                    chain.pop();
                    let prefix = " ".repeat(parts.column() + if parts.key.is_some() { 2 } else { 0 });
                    out.push_str(parts.indent);
                    out.push_str(parts.lead);
                    if let Some(key) = parts.key {
                        out.push_str(key);
                        out.push(':');
                    }
                    let opener = out.trim_end_matches(' ').len();
                    out.truncate(opener);
                    out.push('\n');
                    for l in spliced.lines() {
                        if !l.trim().is_empty() {
                            out.push_str(&prefix);
                            out.push_str(l);
                        }
                        out.push('\n');
                    }
                }
                Err(reason) => {
                    self.record(file, &include, IncludeOutcome::Unresolved(reason));
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        if !text.ends_with('\n') && out.ends_with('\n') {
            out.pop();
        }
        out
    }

    /// Read one include named in `file`, applying FR-004's bounds. On
    /// success, its display path, canonical path and prepared text.
    fn resolve(
        &mut self,
        include: &str,
        file: &Path,
        chain: &[PathBuf],
    ) -> Result<(PathBuf, PathBuf, String), UnresolvedReason> {
        let display = lexical_join(file.parent().unwrap_or(Path::new("")), include);
        if chain.len() > MAX_INCLUDE_DEPTH {
            return Err(UnresolvedReason::TooDeep);
        }
        let canonical = match std::fs::canonicalize(&display) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // A missing file above the root is still outside it.
                let inside = match (&self.root, nearest_canonical(&display)) {
                    (Some(root), Some(c)) => c.starts_with(root),
                    _ => false,
                };
                return Err(if inside { UnresolvedReason::Missing } else { UnresolvedReason::OutsideRoot });
            }
            Err(_) => return Err(UnresolvedReason::Unreadable),
        };
        match &self.root {
            Some(root) if canonical.starts_with(root) => {}
            _ => return Err(UnresolvedReason::OutsideRoot),
        }
        if chain.contains(&canonical) {
            return Err(UnresolvedReason::Cycle);
        }
        let body = std::fs::read_to_string(&canonical).map_err(|_| UnresolvedReason::Unreadable)?;
        self.record(file, include, IncludeOutcome::Resolved);
        Ok((display, canonical, included_document(&body)))
    }

    /// Record each `!include` in `value` that the splice cannot place. A
    /// comment or a quoted string names no include.
    fn unsupported(&mut self, value: &str, file: &Path) {
        if value.starts_with('"') || value.starts_with('\'') {
            return;
        }
        for c in include_anywhere().captures_iter(value) {
            let include = captured_path(&c);
            self.record(file, &include, IncludeOutcome::Unresolved(UnresolvedReason::UnsupportedPosition));
        }
    }

    fn record(&mut self, file: &Path, include: &str, outcome: IncludeOutcome) {
        self.resolutions.push(IncludeResolution {
            manifest: self.manifest.clone(),
            file: file.to_path_buf(),
            include: include.to_string(),
            outcome,
        });
    }
}

/// An included file's text as one document's content: no BOM, and no
/// `---` or `...` markers, which would end the including document.
fn included_document(body: &str) -> String {
    let body = body.strip_prefix('\u{feff}').unwrap_or(body);
    let mut out = String::with_capacity(body.len());
    for line in body.lines() {
        let t = line.trim_end();
        if t == "---" || t == "..." {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// `dir` joined with `rel`, `.` and `..` resolved without touching the
/// filesystem, so a reported path has the same form as the manifest's.
fn lexical_join(dir: &Path, rel: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for c in dir.join(rel).components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The canonical form of a path that may not exist: its nearest existing
/// ancestor canonicalised, with the missing tail appended.
fn nearest_canonical(p: &Path) -> Option<PathBuf> {
    let mut tail = Vec::new();
    let mut cur = p;
    loop {
        if let Ok(mut c) = std::fs::canonicalize(cur) {
            c.extend(tail.iter().rev());
            return Some(c);
        }
        tail.push(cur.file_name()?.to_os_string());
        cur = cur.parent()?;
    }
}

// ───────────────────────────────────────────────────────────────────────
// T009 — the rename (R4)
// ───────────────────────────────────────────────────────────────────────

/// Rename each line-key `<<` to `<<__waybill_merge_<n>`, numbered in
/// document order, so a repeated one parses. Nothing else changes: block
/// scalar bodies and multi-line quoted scalars are skipped, and every other
/// line is copied byte for byte.
pub(crate) fn rename_merge_keys(text: &str) -> String {
    if !text.contains("<<") {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    let opaque = opaque_lines(&lines);
    let mut n = 0usize;
    let mut out = String::with_capacity(text.len() + 32);
    for (line, opaque) in lines.iter().zip(opaque) {
        let parts = LineParts::parse(line);
        if !opaque && parts.key == Some("<<") {
            let at = parts.column();
            out.push_str(&line[..at]);
            out.push_str(MERGE_KEY_PREFIX);
            out.push_str(&n.to_string());
            out.push_str(&line[at + 2..]);
            n += 1;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !text.ends_with('\n') {
        out.pop();
    }
    out
}

// ───────────────────────────────────────────────────────────────────────
// T010 — the fold (R3)
// ───────────────────────────────────────────────────────────────────────

fn is_merge_key(k: &Value) -> bool {
    k.as_str().is_some_and(|s| s == "<<" || s.starts_with(MERGE_KEY_PREFIX))
}

/// Apply every merge key, bottom-up, with hpack's precedence: explicit keys
/// beat merged ones, and among merges the first wins. A merge whose value is
/// still a tag (an include that was not resolved) is left for
/// `strip_tagged`, so the manifest is reported partial.
pub(crate) fn fold_merges(v: &mut Value) {
    match v {
        Value::Mapping(map) => {
            for (_, val) in map.iter_mut() {
                fold_merges(val);
            }
            let keys: Vec<Value> = map
                .iter()
                .filter(|(k, val)| is_merge_key(k) && !matches!(val, Value::Tagged(_)))
                .map(|(k, _)| k.clone())
                .collect();
            let mut sources = Vec::with_capacity(keys.len());
            for k in keys {
                if let Some(src) = map.shift_remove(&k) {
                    sources.push(src);
                }
            }
            for src in sources {
                match src {
                    Value::Mapping(m) => merge_absent(map, m),
                    Value::Sequence(items) => {
                        for item in items {
                            if let Value::Mapping(m) = item {
                                merge_absent(map, m);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Value::Sequence(items) => items.iter_mut().for_each(fold_merges),
        Value::Tagged(t) => fold_merges(&mut t.value),
        _ => {}
    }
}

fn merge_absent(into: &mut Mapping, from: Mapping) {
    for (k, v) in from {
        if !into.contains_key(&k) {
            into.insert(k, v);
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn parse(text: &str) -> Value {
        let mut v: Value = serde_yaml::from_str(&rename_merge_keys(text)).unwrap();
        fold_merges(&mut v);
        v
    }

    fn deps(v: &Value) -> Vec<String> {
        v["library"]["dependencies"]
            .as_sequence()
            .into_iter()
            .flatten()
            .filter_map(|d| d.as_str().map(str::to_string))
            .collect()
    }

    // ── T005: serde_yaml behaviour this module relies on (R8) ──────────

    #[test]
    fn s1649_serde_yaml_later_anchor_definition_wins() {
        let v: Value = serde_yaml::from_str("a: &d [base]\nb: &d [text]\nc: *d\n").unwrap();
        assert_eq!(v["c"][0], "text");
    }

    #[test]
    fn s1649_serde_yaml_renamed_key_is_ordinary() {
        let v: Value = serde_yaml::from_str("<<__waybill_merge_0: {a: 1}\n<<__waybill_merge_1: {b: 2}\n").unwrap();
        assert_eq!(v["<<__waybill_merge_0"]["a"], 1);
        assert_eq!(v["<<__waybill_merge_1"]["b"], 2);
    }

    /// The corpus's most-aliased manifest, with its includes, has 7 aliases
    /// (`measurements/corpus-counts.md`); 40 covers it with room.
    #[test]
    fn s1649_serde_yaml_many_aliases_parse() {
        let mut t = String::from("a: &a\n  dependencies: [base, text, containers]\n");
        for i in 0..40 {
            t.push_str(&format!("k{i}:\n  <<: *a\n"));
        }
        let v = parse(&t);
        assert_eq!(v["k39"]["dependencies"][2], "containers");
    }

    // ── T009: the rename ───────────────────────────────────────────────

    const RENAME_INPUT: &str = "\
name: x
_a: &a {dependencies: [base]}
library:
  <<: *a
  << : *a
  - <<: *a
  description: |
    <<: not a key
    still <<: text
  other: >-
    <<: folded
  quoted: '<<: single
    <<: continued'
  dq: \"<<: double
    <<: continued \\\" escaped\"
  key-with-<<: kept
  \"<<\": quoted key kept
x: y";

    #[test]
    fn s1649_rename_touches_only_line_keys() {
        let out = rename_merge_keys(RENAME_INPUT);
        let changed: Vec<(&str, &str)> =
            RENAME_INPUT.lines().zip(out.lines()).filter(|(a, b)| a != b).collect();
        assert_eq!(
            changed,
            vec![
                ("  <<: *a", "  <<__waybill_merge_0: *a"),
                ("  << : *a", "  <<__waybill_merge_1 : *a"),
                ("  - <<: *a", "  - <<__waybill_merge_2: *a"),
            ]
        );
        assert_eq!(RENAME_INPUT.lines().count(), out.lines().count());
    }

    #[test]
    fn s1649_rename_reverses_exactly() {
        let out = rename_merge_keys(RENAME_INPUT);
        let re = Regex::new(r"<<__waybill_merge_\d+").unwrap();
        assert_eq!(re.replace_all(&out, "<<"), RENAME_INPUT);
        let with_newline = format!("{RENAME_INPUT}\n");
        assert_eq!(re.replace_all(&rename_merge_keys(&with_newline), "<<"), with_newline);
    }

    // ── T010: the fold ─────────────────────────────────────────────────

    #[test]
    fn s1649_fold_explicit_beats_merge_in_either_order() {
        let first = parse("a: &a {dependencies: [base]}\nlibrary:\n  dependencies: [text]\n  <<: *a\n");
        let last = parse("a: &a {dependencies: [base]}\nlibrary:\n  <<: *a\n  dependencies: [text]\n");
        assert_eq!(deps(&first), vec!["text"]);
        assert_eq!(deps(&last), vec!["text"]);
    }

    #[test]
    fn s1649_fold_first_merge_wins() {
        let v = parse("a: &a {dependencies: [base]}\nb: &b {dependencies: [text], x: 1}\nlibrary:\n  <<: *a\n  <<: *b\n");
        assert_eq!(deps(&v), vec!["base"]);
        assert_eq!(v["library"]["x"], 1, "the second merge still contributes keys the first lacks");
    }

    #[test]
    fn s1649_fold_sequence_form() {
        let v = parse("a: &a {dependencies: [base]}\nb: &b {dependencies: [text]}\nlibrary:\n  <<: [*a, *b]\n");
        assert_eq!(deps(&v), vec!["base"]);
    }

    #[test]
    fn s1649_fold_non_mapping_source_is_ignored() {
        let v = parse("s: &s [base]\nlibrary:\n  <<: *s\n  dependencies: [text]\n");
        assert_eq!(deps(&v), vec!["text"]);
        assert!(v["library"].as_mapping().unwrap().keys().all(|k| !is_merge_key(k)));
    }

    #[test]
    fn s1649_fold_nested_merge_inside_merged_mapping() {
        let v = parse("base: &base {dependencies: [base]}\nmid: &mid\n  <<: *base\n  ghc-options: -Wall\nlibrary:\n  <<: *mid\n");
        assert_eq!(deps(&v), vec!["base"]);
        assert_eq!(v["library"]["ghc-options"], "-Wall");
    }

    #[test]
    fn s1649_fold_leaves_a_tagged_merge_for_strip_tagged() {
        let mut v: Value = serde_yaml::from_str(&rename_merge_keys("<<: !include \"x.yaml\"\nname: y\n")).unwrap();
        fold_merges(&mut v);
        assert!(matches!(v["<<__waybill_merge_0"], Value::Tagged(_)));
    }

    // ── T013: the splice ───────────────────────────────────────────────

    fn root_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (rel, text) in files {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        dir
    }

    fn prepared(dir: &tempfile::TempDir, manifest: &str) -> (String, Vec<IncludeResolution>) {
        let path = dir.path().join(manifest);
        let text = std::fs::read_to_string(&path).unwrap();
        prepare(&text, &path, dir.path())
    }

    #[test]
    fn s1649_splice_indents_under_the_key_at_each_column() {
        let dir = root_with(&[
            ("package.yaml", "a: !include inc.yaml\nb:\n  c: !include inc.yaml\n  d:\n    e: !include inc.yaml\n"),
            ("inc.yaml", "x: 1\ny:\n  - z\n"),
        ]);
        let (text, res) = prepared(&dir, "package.yaml");
        assert_eq!(
            text,
            "a:\n  x: 1\n  y:\n    - z\nb:\n  c:\n    x: 1\n    y:\n      - z\n  d:\n    e:\n      x: 1\n      y:\n        - z\n"
        );
        assert_eq!(res.len(), 3);
        assert!(res.iter().all(|r| r.outcome == IncludeOutcome::Resolved));
    }

    #[test]
    fn s1649_splice_sequence_item() {
        let dir = root_with(&[("package.yaml", "xs:\n  - !include inc.yaml\n"), ("inc.yaml", "a: 1\nb: 2\n")]);
        let (text, _) = prepared(&dir, "package.yaml");
        let v: Value = serde_yaml::from_str(&text).unwrap();
        assert_eq!(v["xs"][0]["b"], 2);
    }

    #[test]
    fn s1649_splice_nested_include_is_relative_to_the_including_file() {
        let dir = root_with(&[
            ("package.yaml", "<<: !include shared/a.yaml\n"),
            ("shared/a.yaml", "<<: !include b.yaml\nx: 1\n"),
            ("shared/b.yaml", "y: 2\n"),
            ("b.yaml", "y: wrong\n"),
        ]);
        let (text, res) = prepared(&dir, "package.yaml");
        let mut v: Value = serde_yaml::from_str(&text).unwrap();
        fold_merges(&mut v);
        assert_eq!(v["y"], 2);
        assert_eq!(res[1].file, dir.path().join("shared/a.yaml"));
        assert_eq!(res[1].manifest, dir.path().join("package.yaml"));
    }

    #[test]
    fn s1649_splice_block_scalar_keeps_relative_indentation() {
        let dir = root_with(&[
            ("package.yaml", "lib: !include inc.yaml\n"),
            ("inc.yaml", "description: |\n  line one\n    indented two\nz: 1\n"),
        ]);
        let (text, _) = prepared(&dir, "package.yaml");
        let v: Value = serde_yaml::from_str(&text).unwrap();
        assert_eq!(v["lib"]["description"], "line one\n  indented two\n");
        assert_eq!(v["lib"]["z"], 1);
    }

    #[test]
    fn s1649_splice_key_forms_and_unquoted_path() {
        let dir = root_with(&[
            ("pkg/package.yaml", "_common/lib: !include \"../c.yaml\"\n\"quoted key\": !include '../c.yaml'\nplain: !include ../c.yaml # trailing\n"),
            ("c.yaml", "---\nk: v\n...\n"),
        ]);
        let (text, res) = prepared(&dir, "pkg/package.yaml");
        let v: Value = serde_yaml::from_str(&text).unwrap();
        assert_eq!(v["_common/lib"]["k"], "v");
        assert_eq!(v["quoted key"]["k"], "v");
        assert_eq!(v["plain"]["k"], "v");
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].include, "../c.yaml");
    }

    #[test]
    fn s1649_splice_skips_an_include_in_a_block_scalar() {
        let dir = root_with(&[("package.yaml", "d: |\n  see: !include x.yaml\n")]);
        let (text, res) = prepared(&dir, "package.yaml");
        assert_eq!(text, "d: |\n  see: !include x.yaml\n");
        assert!(res.is_empty());
    }

    #[test]
    fn s1649_prepare_leaves_plain_text_untouched() {
        let t = "name: x\nlibrary:\n  dependencies: [base]";
        let (out, res) = prepare(t, Path::new("/nonexistent/package.yaml"), Path::new("/nonexistent"));
        assert_eq!(out, t);
        assert!(res.is_empty());
    }

    // ── T016: the bounds ───────────────────────────────────────────────

    fn reasons(res: &[IncludeResolution]) -> Vec<IncludeOutcome> {
        res.iter().map(|r| r.outcome).collect()
    }

    #[test]
    fn s1649_bound_missing() {
        let dir = root_with(&[("package.yaml", "a: !include nope.yaml\n")]);
        let (text, res) = prepared(&dir, "package.yaml");
        assert_eq!(reasons(&res), vec![IncludeOutcome::Unresolved(UnresolvedReason::Missing)]);
        assert_eq!(text, "a: !include nope.yaml\n");
    }

    #[test]
    fn s1649_bound_outside_root_existing_and_missing() {
        let dir = root_with(&[
            ("root/p/package.yaml", "a: !include ../../outside.yaml\nb: !include ../../../gone.yaml\n"),
            ("outside.yaml", "x: 1\n"),
        ]);
        let path = dir.path().join("root/p/package.yaml");
        let text = std::fs::read_to_string(&path).unwrap();
        let (_, res) = prepare(&text, &path, &dir.path().join("root"));
        assert_eq!(
            reasons(&res),
            vec![
                IncludeOutcome::Unresolved(UnresolvedReason::OutsideRoot),
                IncludeOutcome::Unresolved(UnresolvedReason::OutsideRoot),
            ]
        );
    }

    #[test]
    fn s1649_bound_cycle_terminates() {
        let dir = root_with(&[
            ("package.yaml", "<<: !include a.yaml\n"),
            ("a.yaml", "<<: !include b.yaml\n"),
            ("b.yaml", "<<: !include a.yaml\n"),
        ]);
        let (_, res) = prepared(&dir, "package.yaml");
        assert_eq!(
            reasons(&res),
            vec![
                IncludeOutcome::Resolved,
                IncludeOutcome::Resolved,
                IncludeOutcome::Unresolved(UnresolvedReason::Cycle),
            ]
        );
    }

    #[test]
    fn s1649_bound_depth() {
        let mut files = vec![("package.yaml".to_string(), "<<: !include c1.yaml\n".to_string())];
        for i in 1..=9 {
            files.push((format!("c{i}.yaml"), format!("<<: !include c{}.yaml\n", i + 1)));
        }
        files.push(("c10.yaml".into(), "x: 1\n".into()));
        let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let dir = root_with(&refs);
        let (_, res) = prepared(&dir, "package.yaml");
        let resolved = res.iter().filter(|r| r.outcome == IncludeOutcome::Resolved).count();
        assert_eq!(resolved, MAX_INCLUDE_DEPTH);
        assert_eq!(res.last().unwrap().outcome, IncludeOutcome::Unresolved(UnresolvedReason::TooDeep));
    }

    #[test]
    fn s1649_bound_flow_position_is_unsupported() {
        let dir = root_with(&[("package.yaml", "a: {x: !include \"s.yaml\"}\n"), ("s.yaml", "y: 1\n")]);
        let (_, res) = prepared(&dir, "package.yaml");
        assert_eq!(reasons(&res), vec![IncludeOutcome::Unresolved(UnresolvedReason::UnsupportedPosition)]);
        assert_eq!(res[0].include, "s.yaml");
    }
}
