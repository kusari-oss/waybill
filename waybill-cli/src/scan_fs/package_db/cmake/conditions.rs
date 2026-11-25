//! Spec 1641 (#1639): whether the CMake conditions enclosing a fetch or a
//! package search hold in the default configuration (research R2–R4).
//!
//! A condition evaluates to one of four values. `Platform` is true on some
//! supported platform: a platform check, or its negation, describes a build
//! for another platform, not an optional feature. At the top of a chain it
//! holds.

use std::collections::{BTreeSet, HashMap};

/// Sentinel tokens for an `if()`'s parentheses (research R3). The tokenizer
/// drops parentheses; `parse_commands` re-inserts these for `if`/`elseif`
/// so grouping survives, and C232's strings leave them out.
pub(crate) const OPEN: &str = "\u{1}(";
pub(crate) const CLOSE: &str = "\u{1})";

/// The arguments of an `if()`/`elseif()` with [`OPEN`]/[`CLOSE`] around
/// each parenthesised group, from the tokenizer's per-argument depth.
pub(crate) fn with_parens(args: Vec<crate::scan_fs::package_db::installed_lib::cmake_version::Arg>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut depth = 0usize;
    for a in args {
        while depth < a.depth {
            out.push(OPEN.to_string());
            depth += 1;
        }
        while depth > a.depth {
            out.push(CLOSE.to_string());
            depth -= 1;
        }
        out.push(a.text);
    }
    while depth > 0 {
        out.push(CLOSE.to_string());
        depth -= 1;
    }
    out
}

/// `args` without the sentinels: today's C232 form.
pub(crate) fn without_parens(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).filter(|a| *a != OPEN && *a != CLOSE).collect()
}

/// One enclosing `if` block: the conditions of the branches before the
/// current one, and the current one's (`None` in an `else`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Branch {
    pub(crate) earlier: Vec<Vec<String>>,
    pub(crate) current: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Eval {
    Holds,
    DoesNotHold,
    Unknown,
    Platform,
}

impl Eval {
    /// Whether a chain with this value runs by default (FR-001, FR-004):
    /// everything but `DoesNotHold`.
    pub(crate) fn in_scope(self) -> bool {
        self != Eval::DoesNotHold
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Eval::Holds => "holds",
            Eval::DoesNotHold => "does-not-hold",
            Eval::Unknown => "unknown",
            Eval::Platform => "platform",
        }
    }

    pub(crate) fn not(self) -> Eval {
        match self {
            Eval::Holds => Eval::DoesNotHold,
            Eval::DoesNotHold => Eval::Holds,
            other => other,
        }
    }

    pub(crate) fn and(self, o: Eval) -> Eval {
        use Eval::*;
        match (self, o) {
            (DoesNotHold, _) | (_, DoesNotHold) => DoesNotHold,
            (Unknown, _) | (_, Unknown) => Unknown,
            (Holds, _) | (_, Holds) => Holds,
            (Platform, Platform) => Platform,
        }
    }

    pub(crate) fn or(self, o: Eval) -> Eval {
        use Eval::*;
        match (self, o) {
            (Holds, _) | (_, Holds) => Holds,
            (Platform, _) | (_, Platform) => Platform,
            (Unknown, _) | (_, Unknown) => Unknown,
            (DoesNotHold, DoesNotHold) => DoesNotHold,
        }
    }
}

/// What kind of atom decided a value, for the census (research R6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Atom {
    Platform,
    Superbuild,
    OptionOn,
    OptionOff,
    Literal,
    /// Spec 1653 (US2): a value configured in a build's `CMakeCache.txt`.
    Configured,
    Unknown,
}

impl Atom {
    fn as_str(self) -> &'static str {
        match self {
            Atom::Platform => "platform",
            Atom::Superbuild => "superbuild",
            Atom::OptionOn => "option-on",
            Atom::OptionOff => "option-off",
            Atom::Literal => "literal",
            Atom::Configured => "configured",
            Atom::Unknown => "unknown",
        }
    }
}

/// The census category of the atoms a chain read: the one kind, or
/// `compound` when it read several.
pub(crate) fn category(atoms: &BTreeSet<Atom>) -> &'static str {
    match atoms.len() {
        0 => "unknown",
        1 => atoms.iter().next().map_or("unknown", |a| a.as_str()),
        _ => "compound",
    }
}

/// Where a default came from (data-model "Default").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Option,
    Literal,
}

/// The boolean defaults declared in one scope (research R4). Several
/// definitions with different values make a name `Unknown`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Defaults {
    values: HashMap<String, (Eval, Source)>,
    /// Spec 1653 (FR-005): values a build configured, which replace source
    /// defaults of the same name. Empty without `--cmake-build-tree`, so every
    /// evaluation is then exactly spec 1641's.
    configured: HashMap<String, Configured>,
}

/// One configured value (data-model `ConfiguredOverlay`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Configured {
    Known(String),
    /// Build directories owning the file configured different values.
    Conflicting,
}

/// The configured values for one declaring file: the condition inputs of
/// every build directory that owns it (contracts/settlement.md, configured
/// values), merged. Only `BOOL`, `STRING`, `UNINITIALIZED` and untyped cache
/// entries are inputs; paths and internals never are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConfiguredOverlay {
    pub(crate) values: HashMap<String, Configured>,
}

impl ConfiguredOverlay {
    pub(crate) fn add(&mut self, name: &str, value: &str) {
        self.values
            .entry(name.to_string())
            .and_modify(|v| {
                if *v != Configured::Known(value.to_string()) {
                    *v = Configured::Conflicting;
                }
            })
            .or_insert_with(|| Configured::Known(value.to_string()));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// `cmake_dependent_option` before its dependencies are evaluated.
#[derive(Debug, Clone)]
pub(crate) struct Dependent {
    name: String,
    default: Eval,
    depends: Vec<Vec<String>>,
    fallback: Eval,
}

/// One file's declarations, collected once and merged per scope.
#[derive(Debug, Clone, Default)]
pub(crate) struct FileDefaults {
    plain: Vec<(String, Eval, Source)>,
    dependent: Vec<Dependent>,
}

impl FileDefaults {
    /// Read `option()`, `cmake_dependent_option()`, `set(… CACHE BOOL …)` and
    /// literal `set()` from one file's commands.
    pub(crate) fn collect(cmds: &[(String, Vec<String>)]) -> FileDefaults {
        let mut out = FileDefaults::default();
        for (name, args) in cmds {
            let Some(var) = args.first() else { continue };
            match name.as_str() {
                "option" => {
                    let v = args.get(2).map_or(Eval::DoesNotHold, |v| constant(v).unwrap_or(Eval::Unknown));
                    out.plain.push((var.clone(), v, Source::Option));
                }
                "cmake_dependent_option" => {
                    if let (Some(d), Some(deps), Some(f)) = (args.get(2), args.get(3), args.get(4)) {
                        out.dependent.push(Dependent {
                            name: var.clone(),
                            default: constant(d).unwrap_or(Eval::Unknown),
                            depends: deps.split(';').map(|c| c.split_whitespace().map(str::to_string).collect()).collect(),
                            fallback: constant(f).unwrap_or(Eval::Unknown),
                        });
                    }
                }
                "set" if args.len() >= 2 => {
                    let cache = args.iter().position(|a| a == "CACHE");
                    match cache {
                        Some(i) if args.get(i + 1).is_some_and(|t| t == "BOOL") && i == 2 => {
                            out.plain.push((var.clone(), constant(&args[1]).unwrap_or(Eval::Unknown), Source::Option));
                        }
                        None if args.len() == 2 => {
                            let v = &args[1];
                            let e = if v.contains("${") { Eval::Unknown } else { constant(v).unwrap_or(Eval::Holds) };
                            out.plain.push((var.clone(), e, Source::Literal));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        out
    }
}

impl Defaults {
    /// The defaults in reach: every file of a scope, merged.
    pub(crate) fn merge<'a>(files: impl IntoIterator<Item = &'a FileDefaults>) -> Defaults {
        let mut values: HashMap<String, (Eval, Source)> = HashMap::new();
        let mut dependents: Vec<&Dependent> = Vec::new();
        let put = |values: &mut HashMap<String, (Eval, Source)>, name: &str, v: Eval, s: Source| {
            values
                .entry(name.to_string())
                .and_modify(|seen| {
                    if seen.0 != v {
                        seen.0 = Eval::Unknown;
                    }
                })
                .or_insert((v, s));
        };
        for f in files {
            for (n, v, s) in &f.plain {
                put(&mut values, n, *v, *s);
            }
            dependents.extend(f.dependent.iter());
        }
        let plain = Defaults { values: values.clone(), configured: HashMap::new() };
        for d in dependents {
            let deps = d.depends.iter().fold(Eval::Holds, |acc, c| acc.and(plain.evaluate(c, &mut BTreeSet::new())));
            let v = match deps {
                Eval::DoesNotHold => d.fallback,
                Eval::Unknown => Eval::Unknown,
                _ => d.default,
            };
            put(&mut values, &d.name, v, Source::Option);
        }
        Defaults { values, configured: HashMap::new() }
    }

    /// These defaults with a build's configured values over them (FR-005).
    pub(crate) fn with_overlay(mut self, overlay: &ConfiguredOverlay) -> Defaults {
        self.configured = overlay.values.clone();
        self
    }

    /// Whether any configured value is in force.
    pub(crate) fn has_overlay(&self) -> bool {
        !self.configured.is_empty()
    }

    /// A configured value, when one is known for `name`.
    fn configured_value(&self, name: &str) -> Option<&str> {
        match self.configured.get(name)? {
            Configured::Known(v) => Some(v.as_str()),
            Configured::Conflicting => None,
        }
    }

    fn lookup(&self, name: &str, atoms: &mut BTreeSet<Atom>) -> Eval {
        // A configured value is CMake's `if(<variable>)`: a false constant
        // does not hold, any other defined value does.
        match self.configured.get(name) {
            Some(Configured::Known(v)) => {
                atoms.insert(Atom::Configured);
                return constant(v).unwrap_or(Eval::Holds);
            }
            Some(Configured::Conflicting) => {
                atoms.insert(Atom::Unknown);
                return Eval::Unknown;
            }
            None => {}
        }
        match self.values.get(name) {
            Some((v, s)) => {
                atoms.insert(match (s, v) {
                    (Source::Literal, _) => Atom::Literal,
                    (Source::Option, Eval::Holds) => Atom::OptionOn,
                    (Source::Option, Eval::DoesNotHold) => Atom::OptionOff,
                    _ => Atom::Unknown,
                });
                *v
            }
            None => {
                atoms.insert(Atom::Unknown);
                Eval::Unknown
            }
        }
    }

    /// One condition's tokens (with [`OPEN`]/[`CLOSE`]), recording the atom
    /// kinds read.
    pub(crate) fn evaluate(&self, tokens: &[String], atoms: &mut BTreeSet<Atom>) -> Eval {
        let toks: Vec<&str> = tokens.iter().map(String::as_str).collect();
        if toks.is_empty() {
            return Eval::Holds;
        }
        let mut p = Parser { toks: &toks, pos: 0, d: self, atoms };
        let v = p.or();
        if p.pos != toks.len() {
            p.atoms.insert(Atom::Unknown);
            return Eval::Unknown;
        }
        v.eval()
    }

    /// A whole chain: the reach value, then every enclosing branch, then
    /// extra condition strings (a wrapper body's), all conjoined (FR-003).
    pub(crate) fn chain(&self, reach: Eval, branches: &[Branch], extra: &[String], atoms: &mut BTreeSet<Atom>) -> Eval {
        let mut v = reach;
        for b in branches {
            for e in &b.earlier {
                v = v.and(self.evaluate(e, atoms).not());
            }
            if let Some(c) = &b.current {
                v = v.and(self.evaluate(c, atoms));
            }
        }
        for s in extra {
            v = v.and(self.condition_string(s, atoms));
        }
        v
    }

    /// A condition in C232's string form, where `else: ` negates and
    /// grouping is lost: a string with no `AND`/`OR`/`NOT` reads exactly,
    /// anything else is `Unknown`.
    pub(crate) fn condition_string(&self, s: &str, atoms: &mut BTreeSet<Atom>) -> Eval {
        if s.starts_with("function: ") {
            return Eval::Holds;
        }
        let (neg, body) = match s.strip_prefix("else: ") {
            Some(b) => (true, b),
            None => (false, s),
        };
        let toks: Vec<String> = body.split_whitespace().map(str::to_string).collect();
        if neg && toks.iter().any(|t| matches!(t.to_ascii_uppercase().as_str(), "AND" | "OR")) {
            atoms.insert(Atom::Unknown);
            return Eval::Unknown;
        }
        let v = self.evaluate(&toks, atoms);
        if neg {
            v.not()
        } else {
            v
        }
    }
}

/// A CMake boolean constant (case-insensitive), or `None` for any other
/// string.
fn constant(s: &str) -> Option<Eval> {
    let u = s.to_ascii_uppercase();
    match u.as_str() {
        "ON" | "YES" | "TRUE" | "Y" => Some(Eval::Holds),
        "OFF" | "NO" | "FALSE" | "N" | "IGNORE" | "NOTFOUND" | "" => Some(Eval::DoesNotHold),
        _ if u.ends_with("-NOTFOUND") => Some(Eval::DoesNotHold),
        _ => u.parse::<f64>().ok().map(|n| if n == 0.0 { Eval::DoesNotHold } else { Eval::Holds }),
    }
}

fn is_platform_var(name: &str) -> bool {
    matches!(
        name,
        "WIN32" | "UNIX" | "APPLE" | "LINUX" | "MSVC" | "CYGWIN" | "MINGW" | "MSYS" | "ANDROID" | "IOS" | "BSD" | "XCODE"
            | "CMAKE_SYSTEM_NAME" | "CMAKE_SYSTEM_PROCESSOR" | "CMAKE_SYSTEM_VERSION"
            | "CMAKE_CXX_COMPILER_ID" | "CMAKE_C_COMPILER_ID" | "CMAKE_GENERATOR" | "CMAKE_SIZEOF_VOID_P"
            | "CMAKE_CROSSCOMPILING" | "EMSCRIPTEN"
    ) || name.starts_with("CMAKE_HOST_")
        || name.starts_with("MSVC_")
}

/// `X`, or `X` out of `${X}`.
fn var_name(t: &str) -> &str {
    t.strip_prefix("${").and_then(|r| r.strip_suffix('}')).unwrap_or(t)
}

/// Spec 1653 (FR-005): one decided comparison, or `None` for an operator
/// this does not evaluate (`STRLESS`, `IN_LIST`, paths, a regex the `regex`
/// crate does not accept, a non-numeric `EQUAL`).
fn compare_values(l: &str, op: &str, r: &str) -> Option<Eval> {
    let b = |x: bool| if x { Eval::Holds } else { Eval::DoesNotHold };
    let version = |s: &str| -> Option<Vec<u64>> {
        s.split('.').map(|c| c.parse::<u64>().ok()).collect::<Option<Vec<u64>>>()
    };
    let cmp_versions = || -> Option<std::cmp::Ordering> {
        let (mut a, mut c) = (version(l)?, version(r)?);
        let n = a.len().max(c.len());
        a.resize(n, 0);
        c.resize(n, 0);
        Some(a.cmp(&c))
    };
    use std::cmp::Ordering::*;
    Some(match op {
        "STREQUAL" => b(l == r),
        "EQUAL" => b(l.trim().parse::<i64>().ok()? == r.trim().parse::<i64>().ok()?),
        "MATCHES" => b(regex::Regex::new(r).ok()?.is_match(l)),
        "VERSION_EQUAL" => b(cmp_versions()? == Equal),
        "VERSION_LESS" => b(cmp_versions()? == Less),
        "VERSION_GREATER" => b(cmp_versions()? == Greater),
        "VERSION_LESS_EQUAL" => b(cmp_versions()? != Greater),
        "VERSION_GREATER_EQUAL" => b(cmp_versions()? != Less),
        _ => return None,
    })
}

fn is_dir_var(t: &str) -> Option<&'static str> {
    match var_name(t) {
        "CMAKE_SOURCE_DIR" => Some("root"),
        "CMAKE_CURRENT_SOURCE_DIR" | "PROJECT_SOURCE_DIR" | "CMAKE_CURRENT_LIST_DIR" => Some("current"),
        "CMAKE_PROJECT_NAME" => Some("root-name"),
        "PROJECT_NAME" => Some("current-name"),
        _ => None,
    }
}

const UNARY: &[&str] = &[
    "EXISTS", "COMMAND", "DEFINED", "POLICY", "TARGET", "TEST", "IS_DIRECTORY", "IS_SYMLINK", "IS_ABSOLUTE",
    "IS_READABLE", "IS_WRITABLE", "IS_EXECUTABLE",
];
const BINARY: &[&str] = &[
    "EQUAL", "LESS", "LESS_EQUAL", "GREATER", "GREATER_EQUAL", "STREQUAL", "STRLESS", "STRLESS_EQUAL", "STRGREATER",
    "STRGREATER_EQUAL", "VERSION_EQUAL", "VERSION_LESS", "VERSION_LESS_EQUAL", "VERSION_GREATER",
    "VERSION_GREATER_EQUAL", "MATCHES", "IN_LIST", "PATH_EQUAL", "IS_NEWER_THAN",
];

/// A value during parsing. `Standalone` is "this is the top-level project"
/// (the positive super-build test): unknown itself, but its negation is the
/// super-build guard, which holds (research R2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Val {
    E(Eval),
    Standalone,
}

impl Val {
    fn eval(self) -> Eval {
        match self {
            Val::E(e) => e,
            Val::Standalone => Eval::Unknown,
        }
    }
}

struct Parser<'a, 'b> {
    toks: &'a [&'a str],
    pos: usize,
    d: &'b Defaults,
    atoms: &'b mut BTreeSet<Atom>,
}

impl<'a> Parser<'a, '_> {
    fn peek(&self) -> Option<&'a str> {
        self.toks.get(self.pos).copied()
    }

    fn keyword(&self, k: &str) -> bool {
        self.peek().is_some_and(|t| t == k)
    }

    fn or(&mut self) -> Val {
        let mut v = self.and();
        while self.keyword("OR") {
            self.pos += 1;
            let r = self.and();
            v = Val::E(v.eval().or(r.eval()));
        }
        v
    }

    fn and(&mut self) -> Val {
        let mut v = self.not();
        while self.keyword("AND") {
            self.pos += 1;
            let r = self.not();
            v = Val::E(v.eval().and(r.eval()));
        }
        v
    }

    fn not(&mut self) -> Val {
        if self.keyword("NOT") {
            self.pos += 1;
            return match self.not() {
                Val::Standalone => {
                    self.atoms.insert(Atom::Superbuild);
                    Val::E(Eval::Holds)
                }
                Val::E(e) => Val::E(e.not()),
            };
        }
        self.binary()
    }

    fn binary(&mut self) -> Val {
        if self.keyword(OPEN) {
            self.pos += 1;
            let v = self.or();
            if self.keyword(CLOSE) {
                self.pos += 1;
            }
            return v;
        }
        if self.peek().is_some_and(|t| UNARY.contains(&t)) {
            self.pos += 2;
            self.atoms.insert(Atom::Unknown);
            return Val::E(Eval::Unknown);
        }
        let Some(left) = self.peek() else {
            return Val::E(Eval::Unknown);
        };
        if left == CLOSE || left == "AND" || left == "OR" {
            return Val::E(Eval::Unknown);
        }
        self.pos += 1;
        if let Some(op) = self.peek().filter(|t| BINARY.contains(t)) {
            self.pos += 1;
            let right = self.peek().unwrap_or("");
            self.pos += 1;
            return self.compare(left, op, right);
        }
        self.atom(left)
    }

    fn compare(&mut self, left: &str, op: &str, right: &str) -> Val {
        let (l, r) = (var_name(left), var_name(right));
        if is_platform_var(l) || is_platform_var(r) {
            self.atoms.insert(Atom::Platform);
            return Val::E(Eval::Platform);
        }
        if op == "STREQUAL" {
            match (is_dir_var(left), is_dir_var(right)) {
                (Some("root"), Some("current")) | (Some("current"), Some("root")) => return Val::Standalone,
                (Some("root-name"), Some("current-name")) | (Some("current-name"), Some("root-name")) => return Val::Standalone,
                _ => {}
            }
        }
        // Spec 1653 (FR-005): with a configured value on at least one side,
        // the comparison is decided. Without one it stays unknown, exactly as
        // spec 1641 has it, so a scan without a build tree is unchanged.
        if let (Some((lv, lc)), Some((rv, rc))) = (self.operand(left), self.operand(right)) {
            if lc || rc {
                if let Some(v) = compare_values(&lv, op, &rv) {
                    self.atoms.insert(Atom::Configured);
                    return Val::E(v);
                }
            }
        }
        self.atoms.insert(Atom::Unknown);
        Val::E(Eval::Unknown)
    }

    /// An operand's string value, and whether it came from a configured
    /// value. `${X}` and a bare name with a configured value read it; a
    /// configured name that conflicts, or `${X}` without one, is unknown; any
    /// other token is the literal itself.
    fn operand(&self, t: &str) -> Option<(String, bool)> {
        let name = var_name(t);
        if self.d.configured.contains_key(name) {
            return self.d.configured_value(name).map(|v| (v.to_string(), true));
        }
        if t.starts_with("${") || t.contains("${") {
            return None;
        }
        Some((t.to_string(), false))
    }

    fn atom(&mut self, t: &str) -> Val {
        if let Some(c) = constant(t) {
            return Val::E(c);
        }
        let name = var_name(t);
        if is_platform_var(name) {
            self.atoms.insert(Atom::Platform);
            return Val::E(Eval::Platform);
        }
        if name == "PROJECT_IS_TOP_LEVEL" || name.ends_with("_IS_TOP_LEVEL") {
            return Val::Standalone;
        }
        Val::E(self.d.lookup(name, self.atoms))
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::installed_lib::cmake_version::commands;

    fn toks(cmake: &str) -> Vec<String> {
        let (_, args) = commands(&format!("if({cmake})")).into_iter().next().unwrap();
        with_parens(args)
    }

    fn defaults(cmake: &str) -> Defaults {
        let cmds: Vec<(String, Vec<String>)> =
            commands(cmake).into_iter().map(|(n, a)| (n.to_ascii_lowercase(), a.into_iter().map(|x| x.text).collect())).collect();
        Defaults::merge([&FileDefaults::collect(&cmds)])
    }

    fn eval(d: &Defaults, cmake: &str) -> Eval {
        d.evaluate(&toks(cmake), &mut BTreeSet::new())
    }

    fn top(e: Eval) -> bool {
        e.in_scope()
    }

    fn overlay(pairs: &[(&str, &str)]) -> ConfiguredOverlay {
        let mut o = ConfiguredOverlay::default();
        for (k, v) in pairs {
            o.add(k, v);
        }
        o
    }

    #[test]
    fn spec1653_configured_values_replace_source_defaults() {
        let d = defaults("option(WITH_EXTRA \"\" OFF)\nset(BACKEND A CACHE STRING \"\")");
        assert_eq!(eval(&d, "WITH_EXTRA"), Eval::DoesNotHold);
        let c = d.clone().with_overlay(&overlay(&[("WITH_EXTRA", "ON"), ("BACKEND", "B")]));
        assert_eq!(eval(&c, "WITH_EXTRA"), Eval::Holds, "configured ON beats the OFF default");
        assert_eq!(eval(&c, "BACKEND"), Eval::Holds, "a defined non-false string holds");
        for falsy in ["OFF", "0", "NO", "FALSE", "N", "IGNORE", "NOTFOUND", "X-NOTFOUND", ""] {
            let c = Defaults::default().with_overlay(&overlay(&[("V", falsy)]));
            assert_eq!(eval(&c, "V"), Eval::DoesNotHold, "{falsy:?}");
        }
    }

    #[test]
    fn spec1653_comparisons_need_a_configured_operand() {
        // #1656: without a build, a cache-string comparison stays unknown.
        assert_eq!(eval(&Defaults::default(), "BACKEND STREQUAL \"B\""), Eval::Unknown);
        assert_eq!(eval(&Defaults::default(), "\"B\" STREQUAL \"B\""), Eval::Unknown, "two literals: unchanged from spec 1641");
        let c = Defaults::default().with_overlay(&overlay(&[("BACKEND", "B"), ("N", "3"), ("V", "1.10.2")]));
        assert_eq!(eval(&c, "BACKEND STREQUAL \"B\""), Eval::Holds);
        assert_eq!(eval(&c, "${BACKEND} STREQUAL \"A\""), Eval::DoesNotHold);
        assert_eq!(eval(&c, "NOT BACKEND STREQUAL \"A\""), Eval::Holds);
        assert_eq!(eval(&c, "N EQUAL 3"), Eval::Holds);
        assert_eq!(eval(&c, "BACKEND MATCHES \"^[AB]$\""), Eval::Holds);
        assert_eq!(eval(&c, "V VERSION_GREATER 1.9"), Eval::Holds, "component-wise, not lexical");
        assert_eq!(eval(&c, "V VERSION_LESS 1.10.2"), Eval::DoesNotHold);
        assert_eq!(eval(&c, "V VERSION_EQUAL 1.10.2.0"), Eval::Holds);
        assert_eq!(eval(&c, "V VERSION_LESS_EQUAL 1.10.2"), Eval::Holds);
        assert_eq!(eval(&c, "V VERSION_GREATER_EQUAL 2"), Eval::DoesNotHold);
        assert_eq!(eval(&c, "BACKEND STRLESS C"), Eval::Unknown, "an operator not evaluated stays unknown");
        assert_eq!(eval(&c, "BACKEND STREQUAL ${OTHER}"), Eval::Unknown, "an unconfigured variable is unknown");
        assert_eq!(eval(&c, "CMAKE_SYSTEM_NAME STREQUAL Linux"), Eval::Platform, "platform checks are unchanged");
    }

    #[test]
    fn spec1653_conflicting_builds_are_unknown() {
        let c = Defaults::default().with_overlay(&overlay(&[("BACKEND", "A"), ("BACKEND", "B")]));
        assert_eq!(eval(&c, "BACKEND STREQUAL \"B\""), Eval::Unknown);
        assert_eq!(eval(&c, "BACKEND"), Eval::Unknown);
    }

    #[test]
    fn platform_predicates_and_their_negations_are_platform() {
        let d = Defaults::default();
        for c in ["WIN32", "NOT WIN32", "CMAKE_SYSTEM_NAME STREQUAL \"Linux\"", "CMAKE_SYSTEM_NAME MATCHES \"Linux\"", "NOT APPLE", "CMAKE_HOST_UNIX", "${CMAKE_SYSTEM_PROCESSOR} STREQUAL x86_64", "CMAKE_CXX_COMPILER_ID VERSION_GREATER 9"] {
            assert_eq!(eval(&d, c), Eval::Platform, "{c}");
        }
    }

    #[test]
    fn the_superbuild_guard_holds_in_either_order_and_its_positive_form_is_unknown() {
        let d = Defaults::default();
        assert_eq!(eval(&d, "NOT CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR"), Eval::Holds);
        assert_eq!(eval(&d, "NOT CMAKE_CURRENT_SOURCE_DIR STREQUAL CMAKE_SOURCE_DIR"), Eval::Holds);
        assert_eq!(eval(&d, "NOT \"${CMAKE_SOURCE_DIR}\" STREQUAL \"${PROJECT_SOURCE_DIR}\""), Eval::Holds);
        assert_eq!(eval(&d, "NOT PROJECT_IS_TOP_LEVEL"), Eval::Holds);
        assert_eq!(eval(&d, "CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR"), Eval::Unknown);
        assert_eq!(eval(&d, "PROJECT_IS_TOP_LEVEL"), Eval::Unknown);
    }

    #[test]
    fn options_take_their_declared_default() {
        let d = defaults("option(A \"\" ON)\noption(B \"\" OFF)\noption(C \"doc\")\nset(D ON CACHE BOOL \"\")\nset(E OFF)\nset(F foo)\nset(G ${H})\n");
        assert_eq!(eval(&d, "A"), Eval::Holds);
        assert_eq!(eval(&d, "B"), Eval::DoesNotHold);
        assert_eq!(eval(&d, "C"), Eval::DoesNotHold, "a missing value is OFF");
        assert_eq!(eval(&d, "D"), Eval::Holds);
        assert_eq!(eval(&d, "E"), Eval::DoesNotHold);
        assert_eq!(eval(&d, "F"), Eval::Holds, "a non-false string is true");
        assert_eq!(eval(&d, "G"), Eval::Unknown);
        assert_eq!(eval(&d, "Z"), Eval::Unknown, "no default visible");
        assert_eq!(eval(&d, "${A}"), Eval::Holds);
    }

    #[test]
    fn conflicting_defaults_are_unknown() {
        let d = defaults("option(A \"\" ON)\noption(A \"\" OFF)\noption(B \"\" ON)\nset(B ON CACHE BOOL \"\")\n");
        assert_eq!(eval(&d, "A"), Eval::Unknown);
        assert_eq!(eval(&d, "B"), Eval::Holds, "agreeing definitions agree");
    }

    #[test]
    fn a_dependent_option_follows_its_dependencies() {
        let d = defaults("option(B \"\" OFF)\ncmake_dependent_option(X \"\" ON \"UNIX\" OFF)\ncmake_dependent_option(DEPY \"\" ON \"B\" OFF)\ncmake_dependent_option(W \"\" ON \"Q\" OFF)\n");
        assert_eq!(eval(&d, "X"), Eval::Holds);
        assert_eq!(eval(&d, "DEPY"), Eval::DoesNotHold, "`Y` itself is a CMake true constant");
        assert_eq!(eval(&d, "W"), Eval::Unknown, "an unknown dependency is unknown");
    }

    #[test]
    fn operators_follow_cmake_precedence_and_three_valued_logic() {
        let d = defaults("option(ON1 \"\" ON)\noption(OFF1 \"\" OFF)\n");
        assert_eq!(eval(&d, "ON1 AND OFF1"), Eval::DoesNotHold);
        assert_eq!(eval(&d, "ON1 OR OFF1"), Eval::Holds);
        assert_eq!(eval(&d, "NOT OFF1"), Eval::Holds);
        assert_eq!(eval(&d, "OFF1 OR ON1 AND OFF1"), Eval::DoesNotHold, "AND binds tighter");
        assert_eq!(eval(&d, "(OFF1 OR ON1) AND ON1"), Eval::Holds);
        assert_eq!(eval(&d, "NOT (ON1 AND OFF1)"), Eval::Holds);
        assert_eq!(eval(&d, "Z AND OFF1"), Eval::DoesNotHold);
        assert_eq!(eval(&d, "Z OR ON1"), Eval::Holds);
        assert_eq!(eval(&d, "Z AND ON1"), Eval::Unknown);
        assert_eq!(eval(&d, "WIN32 AND OFF1"), Eval::DoesNotHold);
        assert_eq!(eval(&d, "WIN32 AND ON1"), Eval::Holds);
        assert_eq!(eval(&d, "WIN32 OR OFF1"), Eval::Platform);
        assert!(top(eval(&d, "NOT (WIN32 OR APPLE)")));
    }

    #[test]
    fn unreadable_expressions_are_unknown() {
        let d = Defaults::default();
        for c in ["EXISTS ${CMAKE_SOURCE_DIR}/x", "DEFINED X", "TARGET foo", "X STREQUAL y", "X IN_LIST L", "$<CONFIG:Debug>"] {
            assert_eq!(eval(&d, c), Eval::Unknown, "{c}");
        }
    }

    #[test]
    fn branches_read_else_and_elseif() {
        let d = defaults("option(ON1 \"\" ON)\noption(OFF1 \"\" OFF)\n");
        let b = |earlier: &[&str], current: Option<&str>| Branch {
            earlier: earlier.iter().map(|c| toks(c)).collect(),
            current: current.map(toks),
        };
        let chain = |bs: &[Branch]| d.chain(Eval::Holds, bs, &[], &mut BTreeSet::new());
        assert!(chain(&[b(&["WIN32"], None)]).in_scope(), "the else of a platform check is another platform");
        assert!(!chain(&[b(&["ON1"], None)]).in_scope(), "the else of an ON option runs only when it is OFF");
        assert!(chain(&[b(&["OFF1"], None)]).in_scope());
        assert!(!chain(&[b(&["ON1"], Some("ON1"))]).in_scope(), "an elseif after a branch that holds");
        assert!(chain(&[b(&["OFF1"], Some("ON1"))]).in_scope());
        assert!(chain(&[b(&[], Some("UNIX")), b(&[], Some("ON1"))]).in_scope(), "nesting conjoins");
        assert!(!chain(&[b(&[], Some("UNIX")), b(&[], Some("OFF1"))]).in_scope());
        assert!(!d.chain(Eval::DoesNotHold, &[], &[], &mut BTreeSet::new()).in_scope(), "the reach value conjoins");
    }

    #[test]
    fn a_condition_string_reads_else_and_falls_back_to_unknown() {
        let d = defaults("option(ON1 \"\" ON)\n");
        let mut a = BTreeSet::new();
        assert_eq!(d.condition_string("else: ON1", &mut a), Eval::DoesNotHold);
        assert_eq!(d.condition_string("else: WIN32", &mut a), Eval::Platform);
        assert_eq!(d.condition_string("else: A AND B", &mut a), Eval::Unknown);
        assert_eq!(d.condition_string("function: f", &mut a), Eval::Holds);
    }

    #[test]
    fn parentheses_survive_tokenizing_and_leave_c232_unchanged() {
        let t = toks("(A OR B) AND NOT (C)");
        assert_eq!(without_parens(&t).join(" "), "A OR B AND NOT C");
        assert!(t.contains(&OPEN.to_string()) && t.contains(&CLOSE.to_string()));
    }

    #[test]
    fn the_census_category_names_one_kind_or_compound() {
        let d = defaults("option(ON1 \"\" ON)\noption(OFF1 \"\" OFF)\nset(L 1)\n");
        let cat = |c: &str| {
            let mut a = BTreeSet::new();
            d.evaluate(&toks(c), &mut a);
            category(&a)
        };
        assert_eq!(cat("WIN32"), "platform");
        assert_eq!(cat("NOT CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR"), "superbuild");
        assert_eq!(cat("ON1"), "option-on");
        assert_eq!(cat("OFF1"), "option-off");
        assert_eq!(cat("L"), "literal");
        assert_eq!(cat("Z"), "unknown");
        assert_eq!(cat("WIN32 AND OFF1"), "compound");
    }
}
