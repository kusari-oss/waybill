//! The pkg-config `.pc` parser (spec 1531 FR-005, research R7).
//!
//! Follows pkg-config's own reading of a file:
//! - `#` starts a comment; `\#` is a literal `#`; `\` before a newline
//!   continues the line.
//! - `name=value` defines a variable; `Key: value` sets a field.
//! - `${var}` in either is expanded at the line where it appears, from the
//!   variables defined above it; `$$` is a literal `$`. An undefined
//!   variable is left as `${var}`, which makes a version unresolved — it is
//!   never guessed. Expanding at definition time cannot loop.

use std::collections::HashMap;

use super::SkipReason;

/// What a `.pc` file says, for this reader's purposes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PkgConfig {
    pub(crate) version: Result<String, SkipReason>,
    pub(crate) requires: Vec<String>,
    pub(crate) requires_private: Vec<String>,
}

pub(crate) fn parse(content: &str) -> PkgConfig {
    let mut vars: HashMap<String, String> = HashMap::new();
    let mut version: Option<String> = None;
    let mut requires = Vec::new();
    let mut requires_private = Vec::new();
    for line in logical_lines(content) {
        let line = line.trim();
        let ident_len = line
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(line.len());
        if ident_len == 0 {
            continue;
        }
        let (ident, rest) = line.split_at(ident_len);
        let rest = rest.trim_start();
        if let Some(value) = rest.strip_prefix(':') {
            let value = expand(value.trim(), &vars);
            match ident {
                "Version" => version = Some(value),
                "Requires" => requires = module_names(&value),
                "Requires.private" => requires_private = module_names(&value),
                _ => {}
            }
        } else if let Some(value) = rest.strip_prefix('=') {
            let value = expand(value.trim(), &vars);
            vars.insert(ident.to_string(), value);
        }
    }
    let version = match version {
        None => Err(SkipReason::NoVersion),
        Some(v) if v.is_empty() || v.contains("${") || is_autoconf_placeholder(&v) => {
            Err(SkipReason::UnresolvedVersion)
        }
        Some(v) => Ok(v),
    };
    PkgConfig { version, requires, requires_private }
}

/// `@VERSION@` and similar: a configure-time placeholder that was never
/// substituted.
fn is_autoconf_placeholder(v: &str) -> bool {
    let mut parts = v.split('@');
    let _ = parts.next();
    parts.any(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        && v.matches('@').count() >= 2
}

/// Join `\`-continued lines, drop comments, and honour `\#`.
fn logical_lines(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = content.chars().peekable();
    let mut in_comment = false;
    while let Some(c) = chars.next() {
        match c {
            '\n' => {
                out.push(std::mem::take(&mut cur));
                in_comment = false;
            }
            _ if in_comment => {}
            '\r' if chars.peek() == Some(&'\n') => {}
            '\\' => match chars.peek() {
                Some('\n') => {
                    chars.next();
                }
                Some('\r') => {
                    chars.next();
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                }
                Some('#') => {
                    chars.next();
                    cur.push('#');
                }
                _ => cur.push('\\'),
            },
            '#' => in_comment = true,
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Expand `${var}` from `vars`, and `$$` to `$`. Undefined variables stay
/// as written.
fn expand(value: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(i) = rest.find('$') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        if let Some(stripped) = after.strip_prefix('$') {
            out.push('$');
            rest = stripped;
        } else if let Some(body) = after.strip_prefix('{') {
            match body.find('}') {
                Some(end) => {
                    let name = &body[..end];
                    match vars.get(name) {
                        Some(v) => out.push_str(v),
                        None => {
                            out.push_str("${");
                            out.push_str(name);
                            out.push('}');
                        }
                    }
                    rest = &body[end + 1..];
                }
                None => {
                    out.push('$');
                    rest = after;
                }
            }
        } else {
            out.push('$');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Module names from a `Requires` value. Entries are separated by commas
/// or whitespace; a comparison operator (`=`, `!=`, `<`, `<=`, `>`, `>=`),
/// written with or without spaces, consumes the version after it.
pub(crate) fn module_names(value: &str) -> Vec<String> {
    #[derive(PartialEq)]
    enum Tok {
        Name(String),
        Op,
    }
    let is_sep = |c: char| c == ',' || c.is_whitespace();
    let is_op = |c: char| matches!(c, '<' | '>' | '=' | '!');
    let mut toks = Vec::new();
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_sep(c) {
            i += 1;
        } else if is_op(c) {
            while i < chars.len() && is_op(chars[i]) {
                i += 1;
            }
            toks.push(Tok::Op);
        } else {
            let start = i;
            while i < chars.len() && !is_sep(chars[i]) && !is_op(chars[i]) {
                i += 1;
            }
            toks.push(Tok::Name(chars[start..i].iter().collect()));
        }
    }
    let mut out = Vec::new();
    let mut skip_next = false;
    for t in toks {
        match t {
            Tok::Op => skip_next = true,
            Tok::Name(n) if skip_next => {
                let _ = n;
                skip_next = false;
            }
            Tok::Name(n) => out.push(n),
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn version(s: &str) -> Result<String, SkipReason> {
        parse(s).version
    }

    #[test]
    fn plain_version() {
        assert_eq!(version("Name: a\nVersion: 1.2.3\n"), Ok("1.2.3".into()));
    }

    #[test]
    fn version_from_a_variable() {
        assert_eq!(version("version=2.0.1\nVersion: ${version}\n"), Ok("2.0.1".into()));
    }

    #[test]
    fn two_level_variable_chain() {
        assert_eq!(version("major=3\nfull=${major}.1\nVersion: ${full}\n"), Ok("3.1".into()));
    }

    #[test]
    fn undefined_variable_is_unresolved() {
        assert_eq!(version("Version: ${version}\n"), Err(SkipReason::UnresolvedVersion));
    }

    #[test]
    fn a_variable_used_before_its_definition_is_unresolved() {
        // pkg-config expands at the line; `version` is not yet defined.
        assert_eq!(version("Version: ${version}\nversion=1.0\n"), Err(SkipReason::UnresolvedVersion));
    }

    #[test]
    fn a_variable_cycle_is_unresolved_and_terminates() {
        assert_eq!(version("a=${b}\nb=${a}\nVersion: ${a}\n"), Err(SkipReason::UnresolvedVersion));
    }

    #[test]
    fn an_autoconf_placeholder_is_unresolved() {
        assert_eq!(version("Version: @VERSION@\n"), Err(SkipReason::UnresolvedVersion));
        assert_eq!(version("Version: @PACKAGE_VERSION@\n"), Err(SkipReason::UnresolvedVersion));
    }

    #[test]
    fn an_email_like_version_is_not_a_placeholder() {
        assert_eq!(version("Version: 1.0@a\n"), Ok("1.0@a".into()));
    }

    #[test]
    fn missing_version_is_no_version() {
        assert_eq!(version("Name: a\n"), Err(SkipReason::NoVersion));
        assert_eq!(version("Version:\n"), Err(SkipReason::UnresolvedVersion));
    }

    #[test]
    fn double_dollar_is_a_literal_dollar() {
        assert_eq!(version("Version: 1$$2\n"), Ok("1$2".into()));
    }

    #[test]
    fn comments_are_dropped_and_escaped_hash_is_kept() {
        assert_eq!(version("# Version: 9\nVersion: 1.0 # trailing\n"), Ok("1.0".into()));
        assert_eq!(version("Version: 1.0\\#2\n"), Ok("1.0#2".into()));
    }

    #[test]
    fn backslash_newline_continues_the_line() {
        assert_eq!(version("Version: 1.\\\n0\n"), Ok("1.0".into()));
        let pc = parse("Requires: a, \\\n  b\nVersion: 1\n");
        assert_eq!(pc.requires, vec!["a", "b"]);
    }

    #[test]
    fn crlf_line_endings() {
        let pc = parse("version=4.0\r\nVersion: ${version}\r\nRequires: a\r\n");
        assert_eq!(pc.version, Ok("4.0".into()));
        assert_eq!(pc.requires, vec!["a"]);
    }

    #[test]
    fn field_names_are_case_sensitive() {
        assert_eq!(version("version: 1.0\n"), Err(SkipReason::NoVersion));
    }

    #[test]
    fn requires_and_private_are_separate() {
        let pc = parse("Version: 1\nRequires: a\nRequires.private: b, c\n");
        assert_eq!(pc.requires, vec!["a"]);
        assert_eq!(pc.requires_private, vec!["b", "c"]);
    }

    // T021: the `Requires` grammar.
    #[test]
    fn requires_grammar() {
        assert_eq!(module_names("a, b"), vec!["a", "b"]);
        assert_eq!(module_names("a b"), vec!["a", "b"]);
        assert_eq!(module_names("a >= 1.0, b"), vec!["a", "b"]);
        assert_eq!(module_names("a>=1.0"), vec!["a"]);
        assert_eq!(module_names("a != 2 b < 3"), vec!["a", "b"]);
        assert_eq!(module_names("a = 1.0 b"), vec!["a", "b"]);
        assert_eq!(module_names(""), Vec::<String>::new());
        assert_eq!(module_names("a, a"), vec!["a", "a"]);
        assert_eq!(module_names("glib-2.0 >= 2.50, gobject-2.0"), vec!["glib-2.0", "gobject-2.0"]);
    }

    #[test]
    fn requires_with_variables() {
        let pc = parse("dep=waybill-fixture-c\nVersion: 1\nRequires: ${dep} >= 1.0\n");
        assert_eq!(pc.requires, vec!["waybill-fixture-c"]);
    }
}
