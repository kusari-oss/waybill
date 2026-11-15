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

struct Arg {
    text: String,
}

/// Remove `#` line comments and `#[[ … ]]` / `#[=[ … ]=]` bracket comments,
/// leaving quoted strings intact.
fn strip_comments(content: &str) -> String {
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
fn commands(text: &str) -> Vec<(String, Vec<Arg>)> {
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
        let (args, end) = arguments(&chars, j + 1);
        out.push((name, args));
        i = end;
    }
    out
}

/// Parse arguments from just after `(` to the matching `)`. Returns the
/// arguments and the index after `)`.
fn arguments(chars: &[char], mut i: usize) -> (Vec<Arg>, usize) {
    let mut args = Vec::new();
    let mut depth = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == ')' {
            if depth == 0 {
                return (args, i + 1);
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
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != ')' && chars[i] != '(' {
                i += 1;
            }
            args.push(Arg { text: chars[start..i].iter().collect() });
        }
    }
    (args, i)
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
}
