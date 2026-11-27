//! Spec 1664 (#1731) — Nix build references that make a Haskell package a
//! project member.
//!
//! Only `callCabal2nix "<name>" <path>` with an unquoted literal path starting
//! `./` or `../` is recognised: the one form the corpus measurement found
//! naming a directory #1032's rule rejects (research R1). Everything else —
//! `callCabal2nixWithOptions`, interpolated or quoted paths, variables,
//! fetched sources, absolute paths, comments — admits nothing (FR-003,
//! FR-006). No Nix is evaluated.

use std::path::{Component, Path, PathBuf};

/// `text` with `#` line comments and `/* … */` block comments blanked out.
/// Newlines are kept, so line numbers survive. A `#` or `/*` inside a
/// `"…"` or `''…''` string is not a comment.
pub(crate) fn strip_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != b'"' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            out.push_str(&text[start..i]);
        } else if c == b'\'' && b.get(i + 1) == Some(&b'\'') {
            let start = i;
            i += 2;
            while i < b.len() && !(b[i] == b'\'' && b.get(i + 1) == Some(&b'\'')) {
                i += 1;
            }
            i = (i + 2).min(b.len());
            out.push_str(&text[start..i]);
        } else if c == b'#' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                if b[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = (i + 2).min(b.len());
        } else {
            let ch = text[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

const TOKEN: &str = "callCabal2nix";

/// Every literal path passed to `callCabal2nix "<name>"` in `text`, with the
/// 1-based line of the `callCabal2nix` token. Comments are stripped first.
pub(crate) fn callcabal2nix_paths(text: &str) -> Vec<(String, usize)> {
    let s = strip_comments(text);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(off) = s[from..].find(TOKEN) {
        let at = from + off;
        from = at + TOKEN.len();
        // `fooCallCabal2nix` is another name. `callCabal2nixWithOptions` needs
        // no check of its own: the next token must be a quoted name, and
        // `WithOptions` is not (mutation M1, measurements/mutations.md).
        if s[..at].chars().next_back().is_some_and(is_ident_char) {
            continue;
        }
        let rest = s[from..].trim_start();
        let Some(rest) = rest.strip_prefix('"') else {
            continue;
        };
        let Some(close) = rest.find('"') else {
            continue;
        };
        let rest = rest[close + 1..].trim_start();
        let end = rest
            // Not `{` or `}`: `./${name}` must stay whole so the `${` check
            // below sees it.
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | ';' | '"'))
            .unwrap_or(rest.len());
        let tok = &rest[..end];
        let literal = (tok == "./." || tok.starts_with("./") || tok.starts_with("../") || tok == "..")
            && !tok.contains("${");
        if literal {
            let line = s[..at].matches('\n').count() + 1;
            out.push((tok.to_string(), line));
        }
    }
    out
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '\'' | '-')
}

/// `literal` resolved against `nix_file`'s directory, lexically (no
/// filesystem access). `None` when it leaves `rootfs`.
pub(crate) fn resolve(nix_file: &Path, literal: &str, rootfs: &Path) -> Option<PathBuf> {
    let mut out = nix_file.parent()?.to_path_buf();
    for comp in Path::new(literal).components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::Normal(p) => out.push(p),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    out.starts_with(rootfs).then_some(out)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn paths(t: &str) -> Vec<String> {
        callcabal2nix_paths(t).into_iter().map(|(p, _)| p).collect()
    }

    // ---- recognised forms (research R1) ----

    #[test]
    fn attribute_prefixed_call_on_a_relative_path() {
        let t = r#"{ haskellPackages }: haskellPackages.callCabal2nix "tix-to-markdown" ../../tools/tix-to-markdown { }"#;
        assert_eq!(paths(t), ["../../tools/tix-to-markdown"]);
        let t = r#"  x = hfinal.callCabal2nix "x" ../../../tools/x { };"#;
        assert_eq!(paths(t), ["../../../tools/x"]);
    }

    #[test]
    fn a_call_split_over_lines_is_recognised_with_its_line() {
        let t = "a\n  (pkgs.haskellPackages.callCabal2nix\n    \"hsec-sync\"\n    ./code/hsec-sync\n    { });\n";
        assert_eq!(callcabal2nix_paths(t), [("./code/hsec-sync".to_string(), 2)]);
    }

    #[test]
    fn dot_slash_dot_is_the_files_own_directory() {
        assert_eq!(paths(r#"hprev.callCabal2nix "slack-web" ./. { }"#), ["./."]);
        let root = Path::new("/r");
        assert_eq!(resolve(Path::new("/r/flake.nix"), "./.", root), Some(PathBuf::from("/r")));
    }

    #[test]
    fn resolution_is_relative_to_the_nix_file() {
        let root = Path::new("/r");
        assert_eq!(
            resolve(Path::new("/r/nix/packages/t.nix"), "../../tools/t", root),
            Some(PathBuf::from("/r/tools/t"))
        );
    }

    #[test]
    fn resolution_outside_the_root_is_refused() {
        let root = Path::new("/r");
        assert_eq!(resolve(Path::new("/r/a.nix"), "../elsewhere", root), None);
        assert_eq!(resolve(Path::new("/r/a.nix"), "/abs", root), None);
    }

    // ---- exclusions (FR-006, research R3) ----

    #[test]
    fn comments_admit_nothing() {
        assert!(paths("# x = callCabal2nix \"x\" ./x { };\n").is_empty());
        assert!(paths("/* callCabal2nix \"x\" ./x { } */").is_empty());
        assert!(paths("/*\n callCabal2nix \"x\" ./x\n*/").is_empty());
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        assert_eq!(paths("s = \"#\"; y = callCabal2nix \"y\" ./y { };"), ["./y"]);
    }

    #[test]
    fn non_literal_paths_admit_nothing() {
        assert!(paths(r#"callCabal2nix name "${src}/x" { }"#).is_empty());
        assert!(paths(r#"callCabal2nix "x" "${src}/x" { }"#).is_empty());
        assert!(paths(r#"mapAttrs (name: path: hfinal.callCabal2nix name path {})"#).is_empty());
        assert!(paths(r#"final.callCabal2nix "t" (builtins.fetchTarball { url = "u"; }) {}"#).is_empty());
        assert!(paths(r#"callCabal2nix "x" path { }"#).is_empty());
        assert!(paths(r#"callCabal2nix "x" /abs/x { }"#).is_empty());
        assert!(paths(r#"callCabal2nix "x" "./x" { }"#).is_empty());
        assert!(paths(r#"callCabal2nix "x" ./${name} { }"#).is_empty());
    }

    #[test]
    fn with_options_is_a_different_function() {
        assert!(paths(r#"callCabal2nixWithOptions "x" ./x "" { }"#).is_empty());
    }

    #[test]
    fn a_path_outside_a_call_admits_nothing() {
        assert!(paths("src = ./x;\n").is_empty());
    }
}
