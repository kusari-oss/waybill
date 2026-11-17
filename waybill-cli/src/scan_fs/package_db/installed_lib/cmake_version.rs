//! The CMake package version file reader (spec 1531 FR-007, research R7).
//!
//! Reads only a literal `set(PACKAGE_VERSION <literal>)` at the top level of
//! the file: outside every `if`/`foreach`/`while`/`function`/`macro` block.
//! Nothing is evaluated. A version computed from a variable, or set only
//! inside a block, is `computed-version`.

use super::SkipReason;

pub(crate) fn parse(content: &str) -> Result<String, SkipReason> {
    let text = strip_comments(content);
    let mut depth: usize = 0;
    for (command, args) in commands(&text) {
        let command = command.to_ascii_lowercase();
        match command.as_str() {
            "if" | "foreach" | "while" | "function" | "macro" | "block" => depth += 1,
            "endif" | "endforeach" | "endwhile" | "endfunction" | "endmacro" | "endblock" => {
                depth = depth.saturating_sub(1)
            }
            "set" if depth == 0 => {
                if args.first().map(|a| a.text.as_str()) != Some("PACKAGE_VERSION") {
                    continue;
                }
                return match args.get(1) {
                    Some(a) if !a.text.contains("${") && !a.text.is_empty() => Ok(a.text.clone()),
                    _ => Err(SkipReason::ComputedVersion),
                };
            }
            _ => {}
        }
    }
    Err(SkipReason::ComputedVersion)
}

pub(crate) struct Arg {
    pub(crate) text: String,
}

/// Remove `#` line comments and `#[[ … ]]` / `#[=[ … ]=]` bracket comments,
/// leaving quoted strings intact.
pub(crate) fn strip_comments(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    let mut in_quote = false;
    while i < chars.len() {
        let c = chars[i];
        if in_quote {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_quote = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_quote = true;
                out.push(c);
                i += 1;
            }
            '#' => {
                // Bracket comment: `#[` then `=`* then `[`.
                let mut j = i + 1;
                if j < chars.len() && chars[j] == '[' {
                    j += 1;
                    let mut eq = 0;
                    while j < chars.len() && chars[j] == '=' {
                        eq += 1;
                        j += 1;
                    }
                    if j < chars.len() && chars[j] == '[' {
                        let close: String = std::iter::once(']')
                            .chain(std::iter::repeat_n('=', eq))
                            .chain(std::iter::once(']'))
                            .collect();
                        let rest: String = chars[j + 1..].iter().collect();
                        match rest.find(&close) {
                            Some(end) => {
                                i = j + 1 + rest[..end].chars().count() + close.chars().count();
                            }
                            None => i = chars.len(),
                        }
                        continue;
                    }
                }
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// `name(args…)` invocations, in order. Arguments are whitespace-separated;
/// a quoted argument keeps its inner text with the quotes removed.
pub(crate) fn commands(text: &str) -> Vec<(String, Vec<Arg>)> {
    commands_checked(text).0
}

/// [`commands`], and whether the text ends inside a command whose `(` was
/// never closed (spec 1538: such a file is counted as unparseable, C231).
pub(crate) fn commands_checked(text: &str) -> (Vec<(String, Vec<Arg>)>, bool) {
    let mut unterminated = false;
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !(chars[i].is_ascii_alphabetic() || chars[i] == '_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        let mut j = i;
        while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
            j += 1;
        }
        if j >= chars.len() || chars[j] != '(' {
            continue;
        }
        let (args, end, closed) = arguments(&chars, j + 1);
        unterminated |= !closed;
        out.push((name, args));
        i = end;
    }
    (out, unterminated)
}

/// Parse arguments from just after `(` to the matching `)`. Returns the
/// arguments and the index after `)`.
fn arguments(chars: &[char], mut i: usize) -> (Vec<Arg>, usize, bool) {
    let mut args = Vec::new();
    let mut depth = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == ')' {
            if depth == 0 {
                return (args, i + 1, true);
            }
            depth -= 1;
            i += 1;
        } else if c == '(' {
            depth += 1;
            i += 1;
        } else if c == '"' {
            let mut text = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    text.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                text.push(chars[i]);
                i += 1;
            }
            i += 1;
            args.push(Arg { text });
        } else if let Some((text, end)) = bracket_argument(chars, i) {
            // `[=[ … ]=]`: CMake's generated CTest files name tests this way,
            // and the content may hold spaces (spec 1533 research R1).
            args.push(Arg { text });
            i = end;
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != ')' && chars[i] != '(' {
                i += 1;
            }
            args.push(Arg { text: chars[start..i].iter().collect() });
        }
    }
    (args, i, false)
}

/// A bracket argument starting at `i`: `[`, `=`×n, `[`, content, `]`, `=`×n,
/// `]`. Returns its content and the index after it, or `None` when `i` does
/// not open one or it is never closed.
fn bracket_argument(chars: &[char], i: usize) -> Option<(String, usize)> {
    if chars.get(i) != Some(&'[') {
        return None;
    }
    let mut j = i + 1;
    while chars.get(j) == Some(&'=') {
        j += 1;
    }
    if chars.get(j) != Some(&'[') {
        return None;
    }
    let eq = j - i - 1;
    let start = j + 1;
    let mut k = start;
    while k < chars.len() {
        if chars[k] == ']' && chars[k + 1..].iter().take(eq).all(|c| *c == '=') && chars.get(k + 1 + eq) == Some(&']') {
            return Some((chars[start..k].iter().collect(), k + eq + 2));
        }
        k += 1;
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const GENERATED: &str = include_str!("testdata/BasicConfigVersion-SameMajor.cmake.txt");

    #[test]
    fn a_real_generated_file() {
        assert_eq!(parse(GENERATED), Ok("4.5.6".into()));
    }

    #[test]
    fn quoted_and_bare_literals() {
        assert_eq!(parse("set(PACKAGE_VERSION \"1.0\")\n"), Ok("1.0".into()));
        assert_eq!(parse("SET(PACKAGE_VERSION 1.0)\n"), Ok("1.0".into()));
        assert_eq!(parse("set ( PACKAGE_VERSION   \"2.3.4.60200\" )\n"), Ok("2.3.4.60200".into()));
    }

    #[test]
    fn a_computed_version_is_not_read() {
        assert_eq!(parse("set(PACKAGE_VERSION ${FOO_VERSION})\n"), Err(SkipReason::ComputedVersion));
        assert_eq!(parse("set(PACKAGE_VERSION \"${FOO_VERSION}\")\n"), Err(SkipReason::ComputedVersion));
        assert_eq!(parse("set(PACKAGE_VERSION)\n"), Err(SkipReason::ComputedVersion));
    }

    #[test]
    fn no_package_version_is_computed() {
        assert_eq!(parse("set(PACKAGE_VERSION_COMPATIBLE TRUE)\n"), Err(SkipReason::ComputedVersion));
        assert_eq!(parse(""), Err(SkipReason::ComputedVersion));
    }

    #[test]
    fn a_version_set_only_inside_a_block_is_computed() {
        assert_eq!(
            parse("if(WIN32)\n  set(PACKAGE_VERSION \"1.0\")\nendif()\n"),
            Err(SkipReason::ComputedVersion)
        );
        assert_eq!(
            parse("if(A)\nendif()\nset(PACKAGE_VERSION \"2.0\")\n"),
            Ok("2.0".into()),
            "a top-level set after a closed block is read"
        );
    }

    #[test]
    fn comments_are_ignored() {
        assert_eq!(parse("# set(PACKAGE_VERSION \"9\")\nset(PACKAGE_VERSION \"1\")\n"), Ok("1".into()));
        assert_eq!(
            parse("#[[ set(PACKAGE_VERSION \"9\") ]]\nset(PACKAGE_VERSION \"1\")\n"),
            Ok("1".into())
        );
        assert_eq!(
            parse("#[==[ set(PACKAGE_VERSION \"9\") ]] ]==]\nset(PACKAGE_VERSION \"1\")\n"),
            Ok("1".into())
        );
    }

    #[test]
    fn a_hash_inside_a_quoted_string_is_not_a_comment() {
        assert_eq!(parse("set(PACKAGE_VERSION \"1#2\")\n"), Ok("1#2".into()));
    }

    /// Spec 1533 R1: CMake writes test names as bracket arguments, which may
    /// hold spaces and `]`; the arguments after them must not shift.
    #[test]
    fn bracket_arguments_are_one_argument() {
        let texts = |s: &str| -> Vec<String> {
            commands(s).into_iter().next().unwrap().1.into_iter().map(|a| a.text).collect()
        };
        assert_eq!(texts("add_test([=[a b]=] \"x\" y)"), vec!["a b", "x", "y"]);
        assert_eq!(texts("add_test([[a]] x)"), vec!["a", "x"]);
        assert_eq!(texts("add_test([==[a ]=] b]==] x)"), vec!["a ]=] b", "x"]);
        // Not a bracket opener, and an unclosed one: ordinary text.
        assert_eq!(texts("f([x] y)"), vec!["[x]", "y"]);
        assert_eq!(texts("f([=[x y)"), vec!["[=[x", "y"]);
    }

    #[test]
    fn s1538_an_open_command_at_the_end_of_the_file_is_reported() {
        let (cmds, open) = commands_checked("project(x)\nFetchContent_Declare(a URL u\n");
        assert!(open);
        assert_eq!(cmds.len(), 2);
        let (_, open) = commands_checked("project(x)\nset(a (b))\n");
        assert!(!open, "nested parentheses close normally");
        let (_, open) = commands_checked("message(\"a)b\")\n");
        assert!(!open, "a parenthesis inside a quoted argument does not close or open");
    }
}
