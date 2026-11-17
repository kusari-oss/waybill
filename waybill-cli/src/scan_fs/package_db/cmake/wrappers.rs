//! Spec 1539 (#1518): fetches made through user-defined wrappers.
//!
//! A wrapper is a `function()` or `macro()` whose body makes exactly one
//! call to `FetchContent_Declare`, `ExternalProject_Add` or another wrapper,
//! and passes the caller's arguments to it (research R1). A call to a
//! wrapper is bound as CMake binds it, measured with CMake 4.4.5
//! (`specs/1539-cmake-fetch-wrappers/measurements/cmake-semantics/`):
//! named parameters take `ARGV0…` and `ARGN` is the rest (W7);
//! `cmake_parse_arguments` keeps the last value of a one-value keyword (W4),
//! and a multi-value keyword takes every word up to the next keyword of the
//! same call, so a `URL` it does not parse can be swallowed (W1); a default
//! the body sets applies only when the call did not (W8). The bound
//! arguments go to the existing reader unchanged, so a repeated `URL` is
//! read as CMake reads it: the first entry is fetched (W2, W3).
//!
//! Nothing is evaluated beyond that. A variable the binder cannot know
//! stays written as `${…}`, and decides nothing unless the identity
//! depends on it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use super::blocks::BlockStack;
use super::fallback::FetchKind;

/// Wrappers followed through other wrappers, at most (FR-004).
pub(crate) const MAX_DEPTH: usize = 8;

/// How the caller's arguments reach the call in a wrapper's body (R1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ForwardingShape {
    /// `${ARGN}`.
    Argn,
    /// `${ARGV}`.
    Argv,
    /// `${<P>_UNPARSED_ARGUMENTS}` after `cmake_parse_arguments(<P> …)`.
    Unparsed,
    /// Any other `${<P>_<KW>}`: `URL ${ARG_URL}`, `DEPENDS ${ARG_DEPENDS}`.
    ParsedVariable,
    /// A named parameter or `${ARGV<n>}`, in any position.
    Parameter,
}

#[derive(Debug, Clone)]
struct BodyCommand {
    name: String,
    args: Vec<String>,
    /// The `if()` conditions enclosing it inside the body, outermost first.
    if_conditions: Vec<String>,
}

/// One `function()` or `macro()` definition.
#[derive(Debug, Clone)]
pub(crate) struct Definition {
    pub(crate) name: String,
    pub(crate) file: PathBuf,
    /// Its index among the definitions in `file`. Tokenised commands carry
    /// no line numbers, so `(file, ordinal)` identifies it.
    pub(crate) ordinal: usize,
    params: Vec<String>,
    body: Vec<BodyCommand>,
    /// The body's single fetch or wrapper call, when this is a wrapper.
    call: Option<usize>,
    pub(crate) shapes: BTreeSet<ForwardingShape>,
}

/// A call bound through its wrappers to a fetch built-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bound {
    pub(crate) kind: FetchKind,
    /// The built-in's arguments, its name first.
    pub(crate) args: Vec<String>,
    /// Conditions inside the wrapper bodies, outermost wrapper first.
    pub(crate) conditions: Vec<String>,
    /// Wrapper names as defined, outermost first.
    pub(crate) chain: Vec<String>,
}

/// Why a call to a wrapper names no fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unbound {
    NotAWrapper,
    TooFewArguments,
    Recursion,
    DepthExceeded,
    /// The definitions of a name disagree (FR-002, research R5).
    Ambiguous,
}

/// Every definition of every name, by lowercased name: CMake command names
/// are case-insensitive.
#[derive(Debug, Default)]
pub(crate) struct Definitions {
    by_name: BTreeMap<String, Vec<Definition>>,
    templates: BTreeSet<(PathBuf, usize)>,
}

fn fetch_kind(lower: &str) -> Option<FetchKind> {
    match lower {
        "fetchcontent_declare" => Some(FetchKind::FetchContent),
        "externalproject_add" => Some(FetchKind::ExternalProject),
        _ => None,
    }
}

fn var_re() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_.+-]*)\}").ok()).as_ref()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ParseArgs {
    prefix: String,
    parse_argv: Option<usize>,
    options: Vec<String>,
    one: Vec<String>,
    multi: Vec<String>,
    /// The trailing arguments of the non-`PARSE_ARGV` form.
    input: Vec<String>,
}

fn split_list(s: &str) -> Vec<String> {
    s.split(';').filter(|x| !x.is_empty()).map(str::to_string).collect()
}

/// `cmake_parse_arguments(<P> <opts> <one> <multi> <args…>)` or
/// `cmake_parse_arguments(PARSE_ARGV <n> <P> <opts> <one> <multi>)`.
fn parse_args(args: &[String]) -> Option<ParseArgs> {
    if args.first().map(String::as_str) == Some("PARSE_ARGV") {
        if args.len() < 6 {
            return None;
        }
        return Some(ParseArgs {
            prefix: args[2].clone(),
            parse_argv: args[1].parse().ok(),
            options: split_list(&args[3]),
            one: split_list(&args[4]),
            multi: split_list(&args[5]),
            input: Vec::new(),
        })
        .filter(|p| p.parse_argv.is_some());
    }
    if args.len() < 4 {
        return None;
    }
    Some(ParseArgs {
        prefix: args[0].clone(),
        parse_argv: None,
        options: split_list(&args[1]),
        one: split_list(&args[2]),
        multi: split_list(&args[3]),
        input: args[4..].to_vec(),
    })
}

fn shapes_of(call: &BodyCommand, params: &[String], prefixes: &[String]) -> BTreeSet<ForwardingShape> {
    let mut s = BTreeSet::new();
    let Some(re) = var_re() else { return s };
    for a in &call.args {
        for c in re.captures_iter(a) {
            let v = &c[1];
            if v == "ARGN" {
                s.insert(ForwardingShape::Argn);
            } else if v == "ARGV" {
                s.insert(ForwardingShape::Argv);
            } else if params.iter().any(|p| p == v)
                || v.strip_prefix("ARGV").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            {
                s.insert(ForwardingShape::Parameter);
            } else if let Some(p) = prefixes.iter().find(|p| v.strip_prefix(p.as_str()).is_some_and(|r| r.starts_with('_'))) {
                if v == format!("{p}_UNPARSED_ARGUMENTS") {
                    s.insert(ForwardingShape::Unparsed);
                } else {
                    s.insert(ForwardingShape::ParsedVariable);
                }
            }
        }
    }
    s
}

impl Definitions {
    /// Every definition in `files` (relative path, lowercased commands),
    /// and which are wrappers, to a fixed point (R1, R2, FR-004).
    /// `has_identity(file, kind, args)` is spec 1538's reading of a
    /// built-in call on its own: a body call that already names a package
    /// is a direct fetch in a function body, never a wrapper.
    pub(crate) fn collect<'a>(
        files: impl IntoIterator<Item = (&'a Path, &'a [(String, Vec<String>)])>,
        has_identity: impl Fn(&Path, FetchKind, &[String]) -> bool,
    ) -> Definitions {
        let mut all: Vec<Definition> = Vec::new();
        for (file, cmds) in files {
            let mut open: Vec<(Definition, BlockStack)> = Vec::new();
            let mut ordinal = 0;
            for (name, args) in cmds {
                match name.as_str() {
                    "function" | "macro" => {
                        open.push((
                            Definition {
                                name: args.first().cloned().unwrap_or_default(),
                                file: file.to_path_buf(),
                                ordinal,
                                params: args.iter().skip(1).cloned().collect(),
                                body: Vec::new(),
                                call: None,
                                shapes: BTreeSet::new(),
                            },
                            BlockStack::default(),
                        ));
                        ordinal += 1;
                    }
                    "endfunction" | "endmacro" => {
                        if let Some((d, _)) = open.pop() {
                            all.push(d);
                        }
                    }
                    _ => {
                        if let Some((d, blocks)) = open.last_mut() {
                            if !blocks.observe(name, args) {
                                d.body.push(BodyCommand {
                                    name: name.clone(),
                                    args: args.clone(),
                                    if_conditions: blocks.if_conditions(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // A definition is a wrapper when its body makes exactly one call to
        // a built-in or to a wrapper, that call forwards the caller's
        // arguments, and (for a built-in) it has no identity of its own.
        let wrapper_call = |d: &Definition, wrappers: &BTreeSet<String>| -> Option<(usize, BTreeSet<ForwardingShape>)> {
            let calls: Vec<usize> = (0..d.body.len())
                .filter(|&i| fetch_kind(&d.body[i].name).is_some() || wrappers.contains(&d.body[i].name))
                .collect();
            let [i] = calls[..] else { return None };
            let prefixes: Vec<String> = d
                .body
                .iter()
                .filter(|c| c.name == "cmake_parse_arguments")
                .filter_map(|c| parse_args(&c.args))
                .map(|p| p.prefix)
                .collect();
            let shapes = shapes_of(&d.body[i], &d.params, &prefixes);
            if shapes.is_empty() {
                return None;
            }
            if let Some(kind) = fetch_kind(&d.body[i].name) {
                if has_identity(&d.file, kind, &d.body[i].args) {
                    return None;
                }
            }
            Some((i, shapes))
        };
        let mut wrappers: BTreeSet<String> = BTreeSet::new();
        for _ in 0..=all.len() {
            let next: BTreeSet<String> =
                all.iter().filter(|d| wrapper_call(d, &wrappers).is_some()).map(|d| d.name.to_ascii_lowercase()).collect();
            if next == wrappers {
                break;
            }
            wrappers = next;
        }
        let mut out = Definitions::default();
        for mut d in all {
            if let Some((i, shapes)) = wrapper_call(&d, &wrappers) {
                d.call = Some(i);
                d.shapes = shapes;
                out.templates.insert((d.file.clone(), d.ordinal));
            }
            out.by_name.entry(d.name.to_ascii_lowercase()).or_default().push(d);
        }
        out
    }

    /// A call of `lower` is a wrapper call site.
    pub(crate) fn is_wrapper(&self, lower: &str) -> bool {
        self.by_name.get(lower).is_some_and(|ds| ds.iter().any(|d| d.call.is_some()))
    }

    /// The body of definition `ordinal` in `file` is a wrapper's template.
    pub(crate) fn is_template(&self, file: &Path, ordinal: usize) -> bool {
        self.templates.contains(&(file.to_path_buf(), ordinal))
    }

    /// Wrapper definitions (C231 `wrappers.definitions`).
    pub(crate) fn wrapper_count(&self) -> usize {
        self.templates.len()
    }

    /// Bind a call of `name` with `args`, already expanded with the call
    /// site's literal `set()`s. A name defined more than once is bound
    /// against every definition, and only agreement on `identity` binds
    /// (FR-002, research R5).
    pub(crate) fn bind(&self, name: &str, args: &[String], identity: &dyn Fn(&Bound) -> Option<String>) -> Result<Bound, Unbound> {
        self.bind_at(name, args, &[], identity)
    }

    fn bind_at(
        &self,
        name: &str,
        args: &[String],
        chain: &[String],
        identity: &dyn Fn(&Bound) -> Option<String>,
    ) -> Result<Bound, Unbound> {
        let lower = name.to_ascii_lowercase();
        if chain.iter().any(|c| c.eq_ignore_ascii_case(&lower)) {
            return Err(Unbound::Recursion);
        }
        if chain.len() >= MAX_DEPTH {
            return Err(Unbound::DepthExceeded);
        }
        let defs = self.by_name.get(&lower).ok_or(Unbound::NotAWrapper)?;
        let mut results: Vec<Result<Bound, Unbound>> = defs.iter().map(|d| self.bind_one(d, args, chain, identity)).collect();
        if results.len() > 1 {
            let keys: Vec<Option<String>> = results.iter().map(|r| r.as_ref().ok().and_then(identity)).collect();
            if keys.windows(2).any(|w| w[0] != w[1]) {
                return Err(Unbound::Ambiguous);
            }
        }
        results.swap_remove(0)
    }

    fn bind_one(
        &self,
        d: &Definition,
        args: &[String],
        chain: &[String],
        identity: &dyn Fn(&Bound) -> Option<String>,
    ) -> Result<Bound, Unbound> {
        let call = d.call.ok_or(Unbound::NotAWrapper)?;
        let scope = Scope::evaluate(d, call, args)?;
        let body_call = &d.body[call];
        let bound_args = scope.substitute(&body_call.args);
        let mut chain = chain.to_vec();
        chain.push(d.name.clone());
        match fetch_kind(&body_call.name) {
            Some(kind) => Ok(Bound { kind, args: bound_args, conditions: body_call.if_conditions.clone(), chain }),
            None => {
                let mut inner = self.bind_at(&body_call.name, &bound_args, &chain, identity)?;
                let mut conditions = body_call.if_conditions.clone();
                conditions.append(&mut inner.conditions);
                inner.conditions = conditions;
                Ok(inner)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Val {
    Known(Vec<String>),
    /// Set by something the binder does not evaluate.
    Unknown,
}

/// A wrapper body's variables at its fetch call.
struct Scope {
    vars: BTreeMap<String, Val>,
    /// Names the body owns: unset, they expand to nothing, as in CMake.
    /// Any other unset name is the caller's or global, and stays as written.
    local: BTreeSet<String>,
}

fn falsy(v: Option<&Val>) -> Option<bool> {
    match v {
        None => Some(true),
        Some(Val::Unknown) => None,
        Some(Val::Known(l)) => Some(match l.as_slice() {
            [] => true,
            [s] => {
                let u = s.to_ascii_uppercase();
                s.is_empty()
                    || ["0", "OFF", "NO", "FALSE", "N", "IGNORE", "NOTFOUND"].contains(&u.as_str())
                    || u.ends_with("-NOTFOUND")
            }
            _ => false,
        }),
    }
}

impl Scope {
    fn evaluate(d: &Definition, call: usize, args: &[String]) -> Result<Scope, Unbound> {
        if args.len() < d.params.len() {
            return Err(Unbound::TooFewArguments);
        }
        let mut s = Scope { vars: BTreeMap::new(), local: BTreeSet::new() };
        let own = |s: &mut Scope, k: &str, v: Vec<String>| {
            s.local.insert(k.to_string());
            s.vars.insert(k.to_string(), Val::Known(v));
        };
        own(&mut s, "ARGC", vec![args.len().to_string()]);
        own(&mut s, "ARGV", args.to_vec());
        own(&mut s, "ARGN", args[d.params.len()..].to_vec());
        for (i, a) in args.iter().enumerate() {
            own(&mut s, &format!("ARGV{i}"), vec![a.clone()]);
        }
        for (p, a) in d.params.iter().zip(args) {
            own(&mut s, p, vec![a.clone()]);
        }
        let fixed: BTreeSet<String> = s.vars.keys().cloned().collect();
        for cmd in &d.body[..call] {
            match cmd.name.as_str() {
                "cmake_parse_arguments" => {
                    if let Some(p) = parse_args(&cmd.args) {
                        let input = match p.parse_argv {
                            Some(n) => args.get(n..).map(<[String]>::to_vec).unwrap_or_default(),
                            None => s.substitute(&p.input),
                        };
                        s.parse(&p, &input);
                    }
                }
                "set" => s.set(cmd),
                "unset" => {
                    if let Some(v) = cmd.args.first() {
                        if cmd.if_conditions.is_empty() {
                            s.vars.remove(v);
                        } else {
                            s.vars.insert(v.clone(), Val::Unknown);
                        }
                    }
                }
                _ => {
                    // `list(APPEND X …)`, `string(… X)` and the like change a
                    // variable the binder does not model.
                    for a in &cmd.args {
                        if s.vars.contains_key(a) && !fixed.contains(a) {
                            s.vars.insert(a.clone(), Val::Unknown);
                        }
                    }
                }
            }
        }
        Ok(s)
    }

    /// W1, W1b, W4: CMake's keyword rules.
    fn parse(&mut self, p: &ParseArgs, input: &[String]) {
        let key = |k: &str| format!("{}_{}", p.prefix, k);
        for k in p.options.iter().chain(&p.one).chain(&p.multi).map(String::as_str).chain(["UNPARSED_ARGUMENTS", "KEYWORDS_MISSING_VALUES"]) {
            self.local.insert(key(k));
            self.vars.remove(&key(k));
        }
        for k in &p.options {
            self.vars.insert(key(k), Val::Known(vec!["FALSE".to_string()]));
        }
        enum Cur<'a> {
            None,
            One(&'a str),
            Multi(&'a str),
        }
        let mut cur = Cur::None;
        let mut one: BTreeMap<&str, String> = BTreeMap::new();
        let mut multi: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        let mut unparsed: Vec<String> = Vec::new();
        for a in input {
            if let Some(k) = p.options.iter().find(|k| *k == a) {
                self.vars.insert(key(k), Val::Known(vec!["TRUE".to_string()]));
                cur = Cur::None;
            } else if let Some(k) = p.one.iter().find(|k| *k == a) {
                cur = Cur::One(k.as_str());
            } else if let Some(k) = p.multi.iter().find(|k| *k == a) {
                multi.entry(k.as_str()).or_default();
                cur = Cur::Multi(k.as_str());
            } else {
                match cur {
                    Cur::One(k) => {
                        one.insert(k, a.clone());
                        cur = Cur::None;
                    }
                    Cur::Multi(k) => multi.entry(k).or_default().push(a.clone()),
                    Cur::None => unparsed.push(a.clone()),
                }
            }
        }
        for (k, v) in one {
            self.vars.insert(key(k), Val::Known(vec![v]));
        }
        for (k, v) in multi.into_iter().filter(|(_, v)| !v.is_empty()) {
            self.vars.insert(key(k), Val::Known(v));
        }
        if !unparsed.is_empty() {
            self.vars.insert(key("UNPARSED_ARGUMENTS"), Val::Known(unparsed));
        }
    }

    /// `set(<var> <value>…)`. Unconditional, it sets; guarded by
    /// `if(NOT DEFINED <var>)` or `if(NOT <var>)` alone, it is a default
    /// (W8); under any other condition, the result is unknown.
    fn set(&mut self, cmd: &BodyCommand) {
        let Some(var) = cmd.args.first() else { return };
        if cmd.args.iter().any(|a| a == "PARENT_SCOPE" || a == "CACHE") {
            return;
        }
        let value = self.substitute(&cmd.args[1..]);
        let apply = match cmd.if_conditions.as_slice() {
            [] => Some(true),
            [c] => {
                let words: Vec<&str> = c.split_whitespace().collect();
                match words.as_slice() {
                    ["NOT", "DEFINED", v] if v == var => match self.vars.get(var) {
                        None => Some(true),
                        Some(Val::Unknown) => None,
                        Some(Val::Known(_)) => Some(false),
                    },
                    ["NOT", v] if v == var => falsy(self.vars.get(var)),
                    _ => None,
                }
            }
            _ => None,
        };
        match apply {
            Some(true) if value.is_empty() => {
                self.vars.remove(var);
            }
            Some(true) => {
                self.vars.insert(var.clone(), Val::Known(value));
            }
            Some(false) => {}
            None => {
                self.vars.insert(var.clone(), Val::Unknown);
            }
        }
    }

    /// Replace every variable the scope knows. A whole argument that is one
    /// known variable becomes one argument per list element, quoted or not
    /// (W6); an unset local expands to nothing; anything else stays as
    /// written.
    fn substitute(&self, args: &[String]) -> Vec<String> {
        let Some(re) = var_re() else { return args.to_vec() };
        let mut out = Vec::new();
        for a in args {
            if let Some(c) = re.captures(a).filter(|c| c.get(0).is_some_and(|m| m.as_str() == a)) {
                match self.vars.get(&c[1]) {
                    Some(Val::Known(v)) => {
                        out.extend(v.iter().cloned());
                        continue;
                    }
                    None if self.local.contains(&c[1]) => continue,
                    _ => {}
                }
            }
            let mut s = a.clone();
            for _ in 0..MAX_DEPTH {
                let next = re
                    .replace_all(&s, |c: &regex::Captures<'_>| match self.vars.get(&c[1]) {
                        Some(Val::Known(v)) => v.join(";"),
                        None if self.local.contains(&c[1]) => String::new(),
                        _ => c[0].to_string(),
                    })
                    .into_owned();
                if next == s {
                    break;
                }
                s = next;
            }
            out.push(s);
        }
        out
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    type Cmds = Vec<(String, Vec<String>)>;

    fn cmds(text: &str) -> Cmds {
        crate::scan_fs::package_db::installed_lib::cmake_version::commands(
            &crate::scan_fs::package_db::installed_lib::cmake_version::strip_comments(text),
        )
        .into_iter()
        .map(|(n, a)| (n.to_ascii_lowercase(), a.into_iter().map(|x| x.text).collect()))
        .collect()
    }

    /// A built-in call has an identity of its own when it names a literal
    /// URL with no `${…}` and a literal name: a stand-in for spec 1538's
    /// reading, which the reader supplies.
    fn literal_identity(_: &Path, _: FetchKind, args: &[String]) -> bool {
        let joined = args.join(" ");
        let repo = joined.contains("GIT_REPOSITORY https://github.com/") && !joined.contains("GIT_REPOSITORY ${");
        let url = args.iter().position(|a| a == "URL").and_then(|i| args.get(i + 1)).is_some_and(|u| !u.contains("${"));
        repo || (url && !args[0].contains("${"))
    }

    fn defs(files: &[(&str, &str)]) -> Definitions {
        let parsed: Vec<(PathBuf, Cmds)> =
            files.iter().map(|(f, t)| (PathBuf::from(f), cmds(t))).collect();
        Definitions::collect(parsed.iter().map(|(f, c)| (f.as_path(), c.as_slice())), literal_identity)
    }

    fn key(b: &Bound) -> Option<String> {
        let i = b.args.iter().position(|a| a == "URL" || a == "GIT_REPOSITORY")?;
        b.args.get(i + 1).cloned()
    }

    fn bind(d: &Definitions, call: &str) -> Result<Bound, Unbound> {
        let c = cmds(call);
        d.bind(&c[0].0, &c[0].1, &key)
    }

    fn shapes(d: &Definitions, name: &str) -> Vec<ForwardingShape> {
        d.by_name[name].iter().flat_map(|x| x.shapes.iter().copied()).collect()
    }

    const PARSE: &str = "function(wf_parse target_name)\n\
        cmake_parse_arguments(PARSE_ARGV 1 ARG \"CMAKE_PROJECT\" \"SOURCE_DIR;PREFIX;EXCLUDE_FROM_ALL\" \"TOUCH\")\n\
        if(NOT DEFINED ARG_PREFIX)\n set(ARG_PREFIX \"${CMAKE_CURRENT_BINARY_DIR}/${target_name}_fetch\")\nendif()\n\
        ExternalProject_Add(\"${target_name}\" PREFIX \"${ARG_PREFIX}\" SOURCE_DIR \"${ARG_SOURCE_DIR}\" CONFIGURE_COMMAND \"\" ${_extra} ${ARG_UNPARSED_ARGUMENTS})\n\
        endfunction()\n";

    #[test]
    fn each_forwarding_shape_is_detected() {
        let d = defs(&[(
            "w.cmake",
            "function(wf_argn name)\nExternalProject_Add(${name} ${ARGN})\nendfunction()\n\
             function(wf_argv)\nExternalProject_Add(${ARGV})\nendfunction()\n\
             macro(wf_named)\ncmake_parse_arguments(W \"\" \"NAME;SOURCE_URL\" \"\" ${ARGN})\nFetchContent_Declare(${W_NAME} URL ${W_SOURCE_URL})\nendmacro()\n\
             function(wf_pos)\nFetchContent_Declare(${ARGV0} URL ${ARGV1})\nendfunction()\n",
        )]);
        assert_eq!(shapes(&d, "wf_argn"), vec![ForwardingShape::Argn, ForwardingShape::Parameter]);
        assert_eq!(shapes(&d, "wf_argv"), vec![ForwardingShape::Argv]);
        assert_eq!(shapes(&d, "wf_named"), vec![ForwardingShape::ParsedVariable], "after a different keyword too");
        assert_eq!(shapes(&d, "wf_pos"), vec![ForwardingShape::Parameter]);
        let p = defs(&[("w.cmake", PARSE)]);
        assert_eq!(shapes(&p, "wf_parse"), vec![ForwardingShape::Unparsed, ForwardingShape::ParsedVariable, ForwardingShape::Parameter]);
        assert_eq!(p.wrapper_count(), 1);
    }

    #[test]
    fn bodies_that_are_not_wrappers() {
        let d = defs(&[(
            "w.cmake",
            "function(lit)\nFetchContent_Declare(x URL https://example.invalid/x-1.0.tar.gz)\nendfunction()\n\
             function(own dep)\nExternalProject_Add(${dep} GIT_REPOSITORY https://github.com/o/r.git GIT_TAG v1)\nendfunction()\n\
             function(two name)\nExternalProject_Add(${name} ${ARGN})\nFetchContent_Declare(${name} ${ARGN})\nendfunction()\n",
        )]);
        assert!(!d.is_wrapper("lit"), "literal-only: forwards nothing");
        assert!(!d.is_wrapper("own"), "R2: an identity of its own (spec 1538 C-13b)");
        assert!(!d.is_wrapper("two"), "two fetch calls");
        assert_eq!(d.wrapper_count(), 0);
    }

    #[test]
    fn a_wrapper_of_a_wrapper_is_found_on_the_second_pass() {
        let d = defs(&[(
            "w.cmake",
            "function(wf_outer n)\nwf_argn(${n} ${ARGN})\nendfunction()\nfunction(wf_argn name)\nExternalProject_Add(${name} ${ARGN})\nendfunction()\n",
        )]);
        assert!(d.is_wrapper("wf_outer"));
        let b = bind(&d, "WF_OUTER(x URL https://example.invalid/x-1.0.tar.gz)").unwrap();
        assert_eq!(b.chain, vec!["wf_outer".to_string(), "wf_argn".to_string()]);
        assert_eq!(b.args, vec!["x", "URL", "https://example.invalid/x-1.0.tar.gz"]);
    }

    #[test]
    fn a_wrapper_calling_a_wrapper_needs_no_identity_check() {
        // LLVM's builtin_default_target shape: keywords re-passed to another
        // wrapper whose body names no URL.
        let d = defs(&[(
            "w.cmake",
            "function(ep name source_dir)\ncmake_parse_arguments(ARG \"\" \"\" \"DEPENDS\" ${ARGN})\nExternalProject_Add(${name} SOURCE_DIR ${source_dir} DEPENDS ${ARG_DEPENDS})\nendfunction()\n\
             function(builtin_default_target p)\ncmake_parse_arguments(ARG \"\" \"\" \"DEPENDS\" ${ARGN})\nep(builtins ${p}/lib DEPENDS ${ARG_DEPENDS})\nendfunction()\n",
        )]);
        assert!(d.is_wrapper("builtin_default_target"));
        let b = bind(&d, "builtin_default_target(rt DEPENDS a b)").unwrap();
        assert_eq!(b.args, vec!["builtins", "SOURCE_DIR", "rt/lib", "DEPENDS", "a", "b"]);
    }

    #[test]
    fn same_named_definitions_keep_distinct_ordinals() {
        let d = defs(&[
            ("a/CMakeLists.txt", "function(x)\nendfunction()\nfunction(wf_dup n)\nExternalProject_Add(${n} ${ARGN})\nendfunction()\n"),
            ("b/CMakeLists.txt", "function(wf_dup)\nmessage(STATUS no)\nendfunction()\n"),
        ]);
        assert!(d.is_template(Path::new("a/CMakeLists.txt"), 1));
        assert!(!d.is_template(Path::new("b/CMakeLists.txt"), 0), "the same name, not a wrapper");
        assert_eq!(bind(&d, "wf_dup(q URL u)"), Err(Unbound::Ambiguous), "W-10c");
    }

    /// TheRock's shape, in TheRock's argument order (probe W1b).
    #[test]
    fn unparsed_arguments_reach_the_fetch_in_order() {
        let d = defs(&[("w.cmake", PARSE)]);
        let b = bind(&d, "wf_parse(z-sources SOURCE_DIR \"${_s}\" URL \"https://m.invalid/z-1.5.7.tar.gz\" URL_HASH \"SHA256=00\" TOUCH \"${_t}\")").unwrap();
        assert_eq!(b.kind, FetchKind::ExternalProject);
        assert_eq!(b.args[0], "z-sources");
        let tail = &b.args[b.args.len() - 4..];
        assert_eq!(tail, ["URL", "https://m.invalid/z-1.5.7.tar.gz", "URL_HASH", "SHA256=00"]);
        assert!(b.args.contains(&"${CMAKE_CURRENT_BINARY_DIR}/z-sources_fetch".to_string()), "the default applies: {:?}", b.args);
        assert!(b.args.contains(&"${_extra}".to_string()), "an unknown local stays as written");
    }

    /// Probe W1: a multi-value keyword before `URL` swallows it.
    #[test]
    fn a_multi_value_keyword_swallows_a_following_url() {
        let d = defs(&[("w.cmake", PARSE)]);
        let b = bind(&d, "wf_parse(z TOUCH t1 t2 URL https://m.invalid/z-1.0.tar.gz URL_HASH SHA256=00 EXTRA)").unwrap();
        assert!(!b.args.contains(&"URL".to_string()), "CMake fetches nothing: {:?}", b.args);
    }

    /// Probe W4: the last value of a one-value keyword wins.
    #[test]
    fn a_one_value_keyword_keeps_its_last_value() {
        let d = defs(&[(
            "w.cmake",
            "function(wf)\ncmake_parse_arguments(PARSE_ARGV 0 W \"\" \"URL\" \"\")\nFetchContent_Declare(n URL ${W_URL})\nendfunction()\n",
        )]);
        let b = bind(&d, "wf(URL first URL second)").unwrap();
        assert_eq!(b.args, vec!["n", "URL", "second"]);
    }

    /// Probe W8: a default applies only when the call did not set it, in
    /// both guard forms.
    #[test]
    fn a_default_applies_only_when_unset() {
        for guard in ["NOT DEFINED ARG_URL", "NOT ARG_URL"] {
            let d = defs(&[(
                "w.cmake",
                &format!(
                    "function(wf name)\ncmake_parse_arguments(PARSE_ARGV 1 ARG \"\" \"URL\" \"\")\nif({guard})\nset(ARG_URL https://d.invalid/a-1.0.tar.gz)\nendif()\nExternalProject_Add(${{name}} URL ${{ARG_URL}})\nendfunction()\n"
                ),
            )]);
            assert_eq!(bind(&d, "wf(e8a)").unwrap().args, vec!["e8a", "URL", "https://d.invalid/a-1.0.tar.gz"], "{guard}");
            assert_eq!(bind(&d, "wf(e8b URL b)").unwrap().args, vec!["e8b", "URL", "b"], "{guard}");
        }
    }

    /// A value set under any other condition is unknown, not guessed.
    #[test]
    fn a_conditional_set_is_unknown() {
        let d = defs(&[(
            "w.cmake",
            "function(wf name)\nif(WIN32)\nset(u https://w.invalid/w-1.0.zip)\nendif()\nExternalProject_Add(${name} URL ${u})\nendfunction()\n",
        )]);
        assert_eq!(bind(&d, "wf(x)").unwrap().args, vec!["x", "URL", "${u}"]);
    }

    /// A command the binder does not model, naming a variable, makes it
    /// unknown rather than leaving a stale value.
    #[test]
    fn a_list_command_makes_a_variable_unknown() {
        let d = defs(&[(
            "w.cmake",
            "function(wf name)\nset(u https://a.invalid/a-1.0.tar.gz)\nlist(APPEND u https://b.invalid/b-1.0.tar.gz)\nExternalProject_Add(${name} URL ${u})\nendfunction()\n",
        )]);
        assert_eq!(bind(&d, "wf(x)").unwrap().args, vec!["x", "URL", "${u}"]);
    }

    /// Probe W2: the body's own `URL` comes before the caller's, and CMake
    /// fetches the first.
    #[test]
    fn the_bodys_url_stays_first() {
        let d = defs(&[("w.cmake", "function(wf name)\nExternalProject_Add(${name} URL a ${ARGN})\nendfunction()\n")]);
        assert_eq!(bind(&d, "wf(x URL b)").unwrap().args, vec!["x", "URL", "a", "URL", "b"]);
    }

    #[test]
    fn positional_arguments() {
        let d = defs(&[("w.cmake", "macro(wf)\nFetchContent_Declare(${ARGV0} URL ${ARGV1})\nendmacro()\n")]);
        assert_eq!(bind(&d, "wf(n u)").unwrap().args, vec!["n", "URL", "u"]);
        let p = defs(&[("w.cmake", "function(wf a b)\nExternalProject_Add(${a} URL ${b})\nendfunction()\n")]);
        assert_eq!(bind(&p, "wf(only)"), Err(Unbound::TooFewArguments));
    }

    #[test]
    fn recursion_and_depth_are_unresolved() {
        let d = defs(&[(
            "w.cmake",
            "function(wa n)\nwb(${n} ${ARGN})\nendfunction()\nfunction(wb n)\nwa(${n} ${ARGN})\nendfunction()\n\
             function(w0 n)\nExternalProject_Add(${n} ${ARGN})\nendfunction()\n",
        )]);
        // Neither wa nor wb reaches a built-in, so neither becomes a wrapper.
        assert!(!d.is_wrapper("wa"));
        // A chain deeper than MAX_DEPTH: w9 → w8 → … → w0.
        let mut text = String::from("function(w0 n)\nExternalProject_Add(${n} ${ARGN})\nendfunction()\n");
        for i in 1..=MAX_DEPTH + 1 {
            text.push_str(&format!("function(w{i} n)\nw{}(${{n}} ${{ARGN}})\nendfunction()\n", i - 1));
        }
        let deep = defs(&[("w.cmake", &text)]);
        assert!(bind(&deep, &format!("w{}(x URL u)", MAX_DEPTH - 1)).is_ok());
        assert_eq!(bind(&deep, &format!("w{}(x URL u)", MAX_DEPTH)), Err(Unbound::DepthExceeded));
    }

    /// A cycle can only reach a built-in through a second definition: `wf`
    /// in `a/` fetches, `wr` calls `wf`, and `wf` in `b/` calls `wr`. The
    /// cycle never loops; the call is unresolved.
    #[test]
    fn a_cycle_through_a_second_definition_terminates_unresolved() {
        let d = defs(&[
            ("a.cmake", "function(wf n)\nExternalProject_Add(${n} ${ARGN})\nendfunction()\nfunction(wr n)\nwf(${n} ${ARGN})\nendfunction()\n"),
            ("b.cmake", "function(wf n)\nwr(${n} ${ARGN})\nendfunction()\n"),
        ]);
        assert!(d.is_wrapper("wr"));
        assert_eq!(bind(&d, "wr(x URL u)"), Err(Unbound::Ambiguous), "one definition fetches, the other recurses");
        assert_eq!(d.bind_at("wr", &["x".into()], &["wr".into()], &key), Err(Unbound::Recursion));
    }

    #[test]
    fn agreeing_definitions_bind_and_disagreeing_ones_do_not() {
        let body = "function(wf_dup n)\nExternalProject_Add(${n} ${ARGN})\nendfunction()\n";
        let same = defs(&[("a/CMakeLists.txt", body), ("b/CMakeLists.txt", body)]);
        assert!(bind(&same, "wf_dup(q URL u)").is_ok(), "W-10");
        let differ = defs(&[
            ("a/CMakeLists.txt", body),
            ("b/CMakeLists.txt", "function(wf_dup n)\nExternalProject_Add(${n} URL https://other.invalid/o-1.0.tar.gz)\nendfunction()\n"),
        ]);
        assert_eq!(bind(&differ, "wf_dup(q URL u)"), Err(Unbound::Ambiguous), "W-10b");
    }

    #[test]
    fn conditions_inside_bodies_follow_the_chain() {
        let d = defs(&[(
            "w.cmake",
            "function(wo n)\nif(A)\nwi(${n} ${ARGN})\nendif()\nendfunction()\nfunction(wi n)\nif(WIN32)\nExternalProject_Add(${n} ${ARGN})\nendif()\nendfunction()\n",
        )]);
        assert_eq!(bind(&d, "wo(x URL u)").unwrap().conditions, vec!["A".to_string(), "WIN32".to_string()]);
    }
}
