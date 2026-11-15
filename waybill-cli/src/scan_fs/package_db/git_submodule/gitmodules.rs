//! Parse `.gitmodules` (spec 1530, research R7).
//!
//! A hand-written subset of git-config syntax: `[submodule "<name>"]`
//! sections with `path` and `url` keys. `git config -f` is not used: it
//! would be one more subprocess, and git-config files can carry `include`
//! directives that read other files.
//!
//! Values that could be abused once handed to `git`, or that point outside
//! the repository, are rejected here (FR-012) and counted as
//! `unsafe-path-or-url`.

use std::path::{Component, Path, PathBuf};

/// One `[submodule "<name>"]` section (data-model.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubmoduleDeclaration {
    pub(crate) name: String,
    /// Relative to the directory holding the `.gitmodules` file.
    pub(crate) path: PathBuf,
    pub(crate) url: Option<String>,
    pub(crate) declared_in: PathBuf,
}

/// What a `.gitmodules` file yielded.
#[derive(Debug, Default)]
pub(crate) struct Parsed {
    pub(crate) declarations: Vec<SubmoduleDeclaration>,
    /// Sections rejected as unsafe (absolute path, `..`, control
    /// characters, or a URL that reads as a command-line option).
    pub(crate) unsafe_entries: usize,
}

/// Parse `.gitmodules` text. Never fails: unreadable sections are skipped
/// with a WARN, unsafe ones are counted.
pub(crate) fn parse(text: &str, declared_in: &Path) -> Parsed {
    let mut out = Parsed::default();
    let mut current: Option<(String, Option<String>, Option<String>)> = None;
    let flush = |cur: Option<(String, Option<String>, Option<String>)>, out: &mut Parsed| {
        let Some((name, path, url)) = cur else { return };
        let Some(path) = path else {
            tracing::warn!(
                reader = "git-submodule",
                file = %declared_in.display(),
                submodule = %name,
                "submodule section has no `path`; ignored",
            );
            return;
        };
        if !path_is_safe(&path) || url.as_deref().is_some_and(|u| !url_is_safe(u)) {
            tracing::warn!(
                reader = "git-submodule",
                file = %declared_in.display(),
                submodule = %name,
                "submodule path or URL is unsafe (absolute, `..`, control character, or an option-like URL); skipped",
            );
            out.unsafe_entries += 1;
            return;
        }
        out.declarations.push(SubmoduleDeclaration {
            name,
            path: PathBuf::from(path.trim_end_matches('/')),
            url,
            declared_in: declared_in.to_path_buf(),
        });
    };

    for line in logical_lines(text) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            flush(current.take(), &mut out);
            current = parse_section_header(header).map(|name| (name, None, None));
            continue;
        }
        let Some(cur) = current.as_mut() else { continue };
        let Some((key, raw)) = line.split_once('=') else { continue };
        let value = parse_value(raw);
        match key.trim().to_ascii_lowercase().as_str() {
            "path" => cur.1 = Some(value),
            "url" => cur.2 = Some(value),
            _ => {}
        }
    }
    flush(current.take(), &mut out);
    out
}

/// Join backslash-continued lines.
fn logical_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for raw in text.lines() {
        if let Some(stripped) = raw.strip_suffix('\\') {
            acc.push_str(stripped);
            continue;
        }
        acc.push_str(raw);
        out.push(std::mem::take(&mut acc));
    }
    if !acc.is_empty() {
        out.push(acc);
    }
    out
}

/// `submodule "<name>"]` → `<name>`. Any other section is not ours.
fn parse_section_header(rest: &str) -> Option<String> {
    let inner = rest.trim_end().strip_suffix(']')?.trim();
    let (kind, name) = inner.split_once(char::is_whitespace)?;
    if !kind.eq_ignore_ascii_case("submodule") {
        return None;
    }
    let name = name.trim();
    let name = name.strip_prefix('"')?.strip_suffix('"')?;
    Some(name.replace("\\\"", "\"").replace("\\\\", "\\"))
}

/// A git-config value: unquoted text up to a `#`/`;` comment, with quoted
/// spans (`"…"`) kept verbatim apart from `\"`, `\\`, `\n`, `\t` escapes.
fn parse_value(raw: &str) -> String {
    let mut out = String::new();
    let mut in_quotes = false;
    let mut chars = raw.trim().chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => in_quotes = !in_quotes,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => {}
            },
            '#' | ';' if !in_quotes => break,
            _ => out.push(c),
        }
    }
    if in_quotes {
        out
    } else {
        out.trim_end().to_string()
    }
}

/// A submodule path is relative, stays inside its repository, and carries
/// no control characters.
fn path_is_safe(path: &str) -> bool {
    if path.is_empty() || path.chars().any(|c| c.is_control()) {
        return false;
    }
    let p = Path::new(path);
    if p.is_absolute() || path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// A URL that begins with `-` would be read as an option if it ever
/// reached a command line (the CVE-2018-17456 class).
fn url_is_safe(url: &str) -> bool {
    !url.trim_start().starts_with('-') && !url.chars().any(|c| c == '\0' || c == '\n')
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn parse_str(s: &str) -> Parsed {
        parse(s, Path::new("/r/.gitmodules"))
    }

    #[test]
    fn s1530_two_sections_with_quotes_comments_and_case() {
        let p = parse_str(
            "# leading comment\n\
             [submodule \"third_party/a\"]\n\
             \tpath = third_party/a\n\
             \tURL = \"https://github.com/o/a.git\" ; trailing\n\
             [submodule \"b\"]\n\
             \tPath = deps/b # trailing\n\
             \turl = git@github.com:o/b.git\n",
        );
        assert_eq!(p.unsafe_entries, 0);
        let got: Vec<(&str, &Path, Option<&str>)> = p
            .declarations
            .iter()
            .map(|d| (d.name.as_str(), d.path.as_path(), d.url.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("third_party/a", Path::new("third_party/a"), Some("https://github.com/o/a.git")),
                ("b", Path::new("deps/b"), Some("git@github.com:o/b.git")),
            ]
        );
    }

    #[test]
    fn s1530_escaped_quote_in_a_quoted_value() {
        let p = parse_str("[submodule \"x\"]\n\tpath = \"dir with \\\"quote\\\"\"\n");
        assert_eq!(p.declarations[0].path, Path::new("dir with \"quote\""));
    }

    #[test]
    fn s1530_unsafe_paths_and_urls_are_rejected_and_counted() {
        let p = parse_str(
            "[submodule \"abs\"]\n\tpath = /etc/x\n\
             [submodule \"up\"]\n\tpath = ../outside\n\
             [submodule \"mid\"]\n\tpath = a/../../b\n\
             [submodule \"opt\"]\n\tpath = ok\n\turl = --upload-pack=touch /tmp/x\n\
             [submodule \"fine\"]\n\tpath = third_party/fine\n\turl = https://example.invalid/fine.git\n",
        );
        assert_eq!(p.unsafe_entries, 4);
        let names: Vec<&str> = p.declarations.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["fine"]);
    }

    #[test]
    fn s1530_section_without_path_is_ignored_not_counted() {
        let p = parse_str("[submodule \"nopath\"]\n\turl = https://example.invalid/x.git\n[core]\n\tbare = false\n");
        assert!(p.declarations.is_empty());
        assert_eq!(p.unsafe_entries, 0);
    }

    #[test]
    fn s1530_comment_only_file_yields_nothing() {
        // haskell-language-server's .gitmodules at its corpus pin (research R13).
        let p = parse_str("# To remove a submodule you need to:\n#\n#     Delete the relevant section\n");
        assert!(p.declarations.is_empty());
        assert_eq!(p.unsafe_entries, 0);
    }
}
