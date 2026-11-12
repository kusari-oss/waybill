//! Issue #1457 — does a Cabal version range admit a version?
//!
//! The nixpkgs pass assigns versions from the pinned revision's package set.
//! That set does not see a flake's own overlays, so a project that pins a
//! newer release through one (`packagesFromDirectory { directory =
//! ./nix/deps; }`) was reported at nixpkgs' version while declaring a bound
//! that version cannot satisfy. Measured on one real project:
//! `slack-web >= 2.3.0.0` declared, 2.3.0.0 configured by Cabal, 2.2.0.0
//! reported.
//!
//! Only as much of the Cabal range language as the decision needs: `==`,
//! `>=`, `>`, `<=`, `<`, `^>=`, `== x.y.*`, version sets `{a, b}`, `&&`,
//! `||`, parentheses, `-any` and `-none`. Anything else is
//! [`Admits::Unknown`], which never contradicts a version: an evaluator that
//! cannot read a range must not be the reason a version is withdrawn.

/// What a range says about one version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Admits {
    Yes,
    No,
    /// The range or the version did not parse.
    Unknown,
}

/// Does the project's own declaration rule this version out?
///
/// `true` only when **no** declared range admits the version and every one
/// of them could be read. The ranges arrive unioned across stanzas and
/// manifests, with `if` conditions and build-tool qualification already
/// discarded (`haskell.rs`, FR-007), so a range that fails may belong to a
/// branch the build never takes. A version no declared range admits fails
/// every site that names the package; that is a contradiction whichever
/// branch was taken. The cost is a miss: `>= 2.3` and `< 3` declared in two
/// stanzas do not together contradict 2.2, because `< 3` admits it.
pub(crate) fn contradicts(ranges: &[String], version: &str) -> bool {
    if ranges.is_empty() {
        return false;
    }
    ranges.iter().all(|r| admits(r, version) == Admits::No)
}

/// Evaluate one range against one version.
pub(crate) fn admits(range: &str, version: &str) -> Admits {
    let Some(v) = parse_version(version) else {
        return Admits::Unknown;
    };
    let Some(tokens) = tokenize(range) else {
        return Admits::Unknown;
    };
    let mut p = Parser { tokens: &tokens, pos: 0, v: &v };
    match p.disjunction() {
        Some(b) if p.pos == tokens.len() => {
            if b {
                Admits::Yes
            } else {
                Admits::No
            }
        }
        _ => Admits::Unknown,
    }
}

type Version = Vec<u64>;

fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.split('.').map(|p| p.parse::<u64>().ok()).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Eq,
    Ge,
    Gt,
    Le,
    Lt,
    Caret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Op(Op),
    /// A version literal; `true` when it ended in `.*`.
    Ver(Version, bool),
    And,
    Or,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Any,
    None,
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let rest = &s[i..];
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let fixed: &[(&str, Tok)] = &[
            ("^>=", Tok::Op(Op::Caret)),
            ("==", Tok::Op(Op::Eq)),
            (">=", Tok::Op(Op::Ge)),
            ("<=", Tok::Op(Op::Le)),
            ("&&", Tok::And),
            ("||", Tok::Or),
            ("-any", Tok::Any),
            ("-none", Tok::None),
            (">", Tok::Op(Op::Gt)),
            ("<", Tok::Op(Op::Lt)),
            ("(", Tok::LParen),
            (")", Tok::RParen),
            ("{", Tok::LBrace),
            ("}", Tok::RBrace),
            (",", Tok::Comma),
        ];
        if let Some((lit, tok)) = fixed.iter().find(|(lit, _)| rest.starts_with(lit)) {
            out.push(tok.clone());
            i += lit.len();
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            let mut lit = &s[start..i];
            let mut wildcard = false;
            if lit.ends_with('.') && b.get(i) == Some(&b'*') {
                lit = &lit[..lit.len() - 1];
                wildcard = true;
                i += 1;
            }
            out.push(Tok::Ver(parse_version(lit)?, wildcard));
            continue;
        }
        return None;
    }
    Some(out)
}

struct Parser<'a> {
    tokens: &'a [Tok],
    pos: usize,
    v: &'a Version,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    // `&&` binds tighter than `||`, as in Cabal.
    fn disjunction(&mut self) -> Option<bool> {
        let mut acc = self.conjunction()?;
        while self.peek() == Some(&Tok::Or) {
            self.pos += 1;
            acc |= self.conjunction()?;
        }
        Some(acc)
    }

    fn conjunction(&mut self) -> Option<bool> {
        let mut acc = self.atom()?;
        while self.peek() == Some(&Tok::And) {
            self.pos += 1;
            acc &= self.atom()?;
        }
        Some(acc)
    }

    fn atom(&mut self) -> Option<bool> {
        match self.next()? {
            Tok::Any => Some(true),
            Tok::None => Some(false),
            Tok::LParen => {
                let b = self.disjunction()?;
                (self.next()? == Tok::RParen).then_some(b)
            }
            Tok::Op(op) => match self.next()? {
                Tok::Ver(bound, wildcard) => compare(&op, self.v, &bound, wildcard),
                Tok::LBrace => {
                    // A version set: `== {1.2, 1.3}`, `^>= {1.2, 1.3}`.
                    if !matches!(op, Op::Eq | Op::Caret) {
                        return None;
                    }
                    let mut any = false;
                    loop {
                        let Tok::Ver(bound, false) = self.next()? else {
                            return None;
                        };
                        any |= compare(&op, self.v, &bound, false)?;
                        match self.next()? {
                            Tok::Comma => continue,
                            Tok::RBrace => break,
                            _ => return None,
                        }
                    }
                    Some(any)
                }
                _ => None,
            },
            _ => None,
        }
    }
}

fn compare(op: &Op, v: &Version, bound: &Version, wildcard: bool) -> Option<bool> {
    if wildcard {
        // Only `== x.y.*` is valid Cabal; it means `>= x.y && < x.(y+1)`.
        if *op != Op::Eq {
            return None;
        }
        let upper = bump_last(bound)?;
        return Some(v >= bound && *v < upper);
    }
    // `Vec<u64>` orders component-wise with a shorter prefix first, which is
    // Cabal's version order: 1.2 < 1.2.0 < 1.2.1.
    Some(match op {
        Op::Eq => v == bound,
        Op::Ge => v >= bound,
        Op::Gt => v > bound,
        Op::Le => v <= bound,
        Op::Lt => v < bound,
        Op::Caret => v >= bound && *v < major_upper_bound(bound),
    })
}

/// `^>= x.y.z` means `>= x.y.z && < x.(y+1)`; `^>= x` means `< x.1`.
fn major_upper_bound(v: &Version) -> Version {
    match v.as_slice() {
        [] => vec![0, 1],
        [m1] => vec![*m1, 1],
        [m1, m2, ..] => vec![*m1, m2.saturating_add(1)],
    }
}

fn bump_last(v: &Version) -> Option<Version> {
    let mut u = v.clone();
    let last = u.last_mut()?;
    *last = last.checked_add(1)?;
    Some(u)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn r(s: &str) -> Vec<String> {
        vec![s.to_string()]
    }

    #[test]
    fn the_issue_1457_range_rules_out_the_nixpkgs_version() {
        assert_eq!(admits(">= 2.3.0.0", "2.2.0.0"), Admits::No);
        assert_eq!(admits(">= 2.3.0.0", "2.3.0.0"), Admits::Yes);
        assert!(contradicts(&r(">= 2.3.0.0"), "2.2.0.0"));
        assert!(!contradicts(&r(">= 2.3.0.0"), "2.3.0.0"));
    }

    #[test]
    fn operators() {
        assert_eq!(admits(">=1.0 && <2", "1.2.3"), Admits::Yes);
        assert_eq!(admits(">=1.0 && <2", "2"), Admits::No);
        assert_eq!(admits(">1.0", "1.0"), Admits::No);
        assert_eq!(admits("<=1.0", "1.0"), Admits::Yes);
        assert_eq!(admits("==1.0", "1.0"), Admits::Yes);
        assert_eq!(admits("==1.0", "1.0.0"), Admits::No);
        assert_eq!(admits("< 1 || >= 1.4", "1.2"), Admits::No);
        assert_eq!(admits("< 1 || >= 1.4", "1.4.1"), Admits::Yes);
        assert_eq!(admits("(>= 1 && < 2) || == 3.1", "3.1"), Admits::Yes);
        assert_eq!(admits("-any", "9"), Admits::Yes);
        assert_eq!(admits("-none", "9"), Admits::No);
    }

    #[test]
    fn shorter_versions_sort_first() {
        assert_eq!(admits(">= 1.2.0", "1.2"), Admits::No);
        assert_eq!(admits("< 1.2.0", "1.2"), Admits::Yes);
    }

    #[test]
    fn caret_and_wildcard() {
        assert_eq!(admits("^>=2.8", "2.8.5"), Admits::Yes);
        assert_eq!(admits("^>=2.8", "2.9"), Admits::No);
        assert_eq!(admits("^>=2.8", "2.7.9"), Admits::No);
        assert_eq!(admits("^>= 1", "1.0.9"), Admits::Yes);
        assert_eq!(admits("^>= 1", "1.1"), Admits::No);
        assert_eq!(admits("^>=0.5 || ^>=1.0.0", "1.0.4"), Admits::Yes);
        assert_eq!(admits("== 4.*", "4.18.2"), Admits::Yes);
        assert_eq!(admits("== 4.*", "5.0"), Admits::No);
        assert_eq!(admits("==3.1.*", "3.1"), Admits::Yes);
        assert_eq!(admits("==3.1.*", "3.2"), Admits::No);
    }

    #[test]
    fn version_sets() {
        assert_eq!(admits("== { 1.2, 1.3 }", "1.3"), Admits::Yes);
        assert_eq!(admits("== {1.2, 1.3}", "1.4"), Admits::No);
        assert_eq!(admits("^>= {1.2, 2.0}", "2.0.7"), Admits::Yes);
        assert_eq!(admits("^>= {1.2, 2.0}", "2.1"), Admits::No);
    }

    /// An unreadable range or version never contradicts. A range from
    /// another ecosystem's syntax lands here, as does a malformed one.
    #[test]
    fn what_cannot_be_read_is_unknown() {
        assert_eq!(admits("docutils~=0.21.2", "0.1"), Admits::Unknown);
        assert_eq!(admits(">= 1.0 &&", "1.0"), Admits::Unknown);
        assert_eq!(admits(">= 1.0 )", "1.0"), Admits::Unknown);
        assert_eq!(admits(">= 1.*", "1.0"), Admits::Unknown);
        assert_eq!(admits(">= 1.0", "1.0-rc1"), Admits::Unknown);
        assert_eq!(admits("", "1.0"), Admits::Unknown);
        assert!(!contradicts(&r("docutils~=0.21.2"), "0.1"));
    }

    /// Contradiction needs every declared range to rule the version out.
    #[test]
    fn one_admitting_or_unreadable_range_is_enough_to_keep_the_version() {
        let ranges = vec![">= 2.3".to_string(), "< 3".to_string()];
        assert!(!contradicts(&ranges, "2.2"));
        let ranges = vec![">= 2.3".to_string(), "garbage".to_string()];
        assert!(!contradicts(&ranges, "2.2"));
        let ranges = vec![">= 2.3".to_string(), "^>= 2.4".to_string()];
        assert!(contradicts(&ranges, "2.2"));
        assert!(!contradicts(&[], "2.2"));
    }
}
