//! Spec 1653 US3 (T028, T029): include directories from a compilation
//! database, and which component each one is.
//!
//! `compile_commands.json` lists one entry per translation unit, with a
//! `command` string or an `arguments` array, and a working `directory`
//! (research R6: every include path the probe saw was absolute). Only
//! include flags are read. A 0-byte file is *absent*, not unreadable:
//! TheRock touches an empty one when CMake writes none
//! (measurements/therock.md).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// One translation unit: its source file and include directories, both
/// absolute and lexically normalised. Never emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unit {
    pub(crate) file: PathBuf,
    pub(crate) includes: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Malformed;

/// The units of a compilation database. An empty text is no units.
pub(crate) fn units(json: &str) -> Result<Vec<Unit>, Malformed> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| Malformed)?;
    let entries = v.as_array().ok_or(Malformed)?;
    let mut out = Vec::new();
    for e in entries {
        let Some(dir) = e["directory"].as_str() else { continue };
        let dir = Path::new(dir);
        let args: Vec<String> = match (e["arguments"].as_array(), e["command"].as_str()) {
            (Some(a), _) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
            (None, Some(c)) if is_windows_dir(dir) => split_command_windows(c),
            (None, Some(c)) => split_command(c),
            (None, None) => continue,
        };
        let file = e["file"].as_str().map(|f| absolute(dir, f)).unwrap_or_default();
        let includes = include_dirs(&args).into_iter().map(|i| absolute(dir, &i)).collect();
        out.push(Unit { file, includes });
    }
    Ok(out)
}

/// Every include directory in `units`, deduplicated.
#[cfg(test)]
pub(crate) fn include_dirs_of(units: &[Unit]) -> std::collections::BTreeSet<PathBuf> {
    units.iter().flat_map(|u| u.includes.iter().cloned()).collect()
}

/// `-I`, `-isystem`, `-iquote`, `-idirafter`, joined or separate; `/I` and
/// `-I` for `cl` and `clang-cl`.
fn include_dirs(args: &[String]) -> Vec<String> {
    let msvc = args.first().is_some_and(|c| {
        let stem = Path::new(&c.replace('\\', "/")).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        stem == "cl" || stem == "clang-cl"
    });
    let mut flags: Vec<&str> = vec!["-isystem", "-iquote", "-idirafter", "-I"];
    if msvc {
        flags.push("/I");
    }
    let mut out = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        for f in &flags {
            if let Some(rest) = a.strip_prefix(f) {
                if rest.is_empty() {
                    if let Some(next) = it.next() {
                        out.push(next.clone());
                    }
                } else {
                    out.push(rest.to_string());
                }
                break;
            }
        }
    }
    out
}

/// A POSIX-shell-style split: whitespace separates, single and double
/// quotes group, a backslash escapes the next character outside single
/// quotes. Enough for the strings CMake writes.
fn split_command(c: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = c.chars();
    let (mut single, mut double) = (false, false);
    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !double => {
                single = !single;
                in_word = true;
            }
            '"' if !single => {
                double = !double;
                in_word = true;
            }
            '\\' if !single => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                in_word = true;
            }
            c if c.is_whitespace() && !single && !double => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

/// A `directory` that starts with a drive letter: the database was written
/// on Windows, where CMake quotes `command` by Windows rules (#1717 review).
fn is_windows_dir(dir: &Path) -> bool {
    let b = dir.as_os_str().as_encoded_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

/// A `command` split by Windows rules (`CommandLineToArgvW`): a backslash
/// is literal unless a run of them precedes `"`, where `2n` backslashes give
/// `n` and the quote toggles quoting, and `2n+1` give `n` and a literal `"`.
/// POSIX rules removed every backslash of `C:\src\include` (#1717 review).
fn split_command_windows(c: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_word, mut quoted) = (false, false);
    let mut chars = c.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let mut n = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    n += 1;
                }
                if chars.peek() == Some(&'"') {
                    cur.extend(std::iter::repeat_n('\\', n / 2));
                    if n % 2 == 1 {
                        chars.next();
                        cur.push('"');
                    }
                } else {
                    cur.extend(std::iter::repeat_n('\\', n));
                }
                in_word = true;
            }
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

fn absolute(dir: &Path, p: &str) -> PathBuf {
    let p = Path::new(p);
    normalise(&if p.is_absolute() { p.to_path_buf() } else { dir.join(p) })
}

/// Drop `.` and resolve `..` lexically.
fn normalise(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// The canonical form of `p`'s longest existing ancestor, with the rest
/// appended: a symlinked root (`/var` → `/private/var`) resolves even for a
/// directory not on disk.
pub(crate) fn canonical_lenient(p: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = p;
    loop {
        if let Ok(c) = std::fs::canonicalize(cur) {
            return rest.iter().rev().fold(c, |acc, r| acc.join(r));
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                cur = parent;
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// What a path inside a build was resolved to (T029).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// A declaration's name (rules 1–3): `_deps/<lc>-src`, an
    /// ExternalProject source directory, or a `<name>_DIR` install prefix.
    Name(String),
    /// A component the scan already has (rule 4): a git submodule.
    Purl(String),
}

/// Where a build's dependencies live, for [`Resolver::resolve`].
pub(crate) struct Resolver<'a> {
    /// `<build>/_deps`, canonical.
    pub(crate) deps: PathBuf,
    /// ExternalProject source directory (sibling of its stamp directory) → name.
    pub(crate) ep_sources: &'a BTreeMap<PathBuf, String>,
    /// `<name>_DIR` → its value, canonical.
    pub(crate) package_dirs: &'a BTreeMap<String, PathBuf>,
    /// Git submodule directories, canonical → PURL.
    pub(crate) submodules: &'a BTreeMap<PathBuf, String>,
}

/// How far above a `<name>_DIR` its install prefix may be:
/// `<prefix>/lib/cmake/<name>`, `<prefix>/share/<name>/cmake`.
const PREFIX_LEVELS: usize = 3;

impl Resolver<'_> {
    /// T029: the component an include directory (or a unit's source file)
    /// lies in, trying the rules in order. `None`: unresolved.
    pub(crate) fn resolve(&self, p: &Path) -> Option<Target> {
        // 1. A FetchContent population.
        if let Ok(rel) = p.strip_prefix(&self.deps) {
            if let Some(Component::Normal(first)) = rel.components().next() {
                if let Some(n) = first.to_str().and_then(|s| s.strip_suffix("-src")).filter(|n| !n.is_empty()) {
                    return Some(Target::Name(n.to_string()));
                }
            }
        }
        // 2. An ExternalProject source directory (the deepest).
        if let Some((_, n)) = self.ep_sources.iter().filter(|(d, _)| p.starts_with(d)).max_by_key(|(d, _)| d.components().count()) {
            return Some(Target::Name(n.clone()));
        }
        // 3. `<prefix>/include` of a `<name>_DIR`: the deepest prefix, and
        //    only when exactly one package names it.
        let mut best: Option<(usize, Vec<&String>)> = None;
        for (name, dir) in self.package_dirs {
            for prefix in dir.ancestors().skip(1).take(PREFIX_LEVELS) {
                if p.starts_with(prefix.join("include")) {
                    let depth = prefix.components().count();
                    match &mut best {
                        Some((d, names)) if *d == depth => names.push(name),
                        Some((d, _)) if *d > depth => {}
                        _ => best = Some((depth, vec![name])),
                    }
                    break;
                }
            }
        }
        if let Some((_, names)) = best {
            return match names.as_slice() {
                [one] => Some(Target::Name((*one).clone())),
                _ => None,
            };
        }
        // 4. A git submodule (the deepest).
        self.submodules
            .iter()
            .filter(|(d, _)| p.starts_with(d))
            .max_by_key(|(d, _)| d.components().count())
            .map(|(_, purl)| Target::Purl(purl.clone()))
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    /// #1717 review, finding 1: the shape CMake's Ninja generator writes for
    /// MSVC on Windows. POSIX splitting removed every backslash, so `cl` was
    /// not recognised and `/I` was skipped.
    #[test]
    fn a_windows_command_keeps_its_backslashes() {
        let args = split_command_windows(r#"C:\PROGRA~1\MICROS~1\cl.exe  /nologo /TP -IC:\src\include /I"C:\dep dir\include" /Fo x.obj /c C:\src\a.cpp"#);
        assert_eq!(args[0], r"C:\PROGRA~1\MICROS~1\cl.exe");
        assert_eq!(include_dirs(&args), [r"C:\src\include", r"C:\dep dir\include"]);
    }

    /// `CommandLineToArgvW`'s backslash-before-quote rules.
    #[test]
    fn windows_backslashes_before_a_quote() {
        assert_eq!(split_command_windows(r#"a\\"b c"\" d"#), [r#"a\b c""#, "d"]);
        assert_eq!(split_command_windows(r#"x\\\"y"#), [r#"x\"y"#]);
        assert_eq!(split_command_windows(r"\\server\share\x"), [r"\\server\share\x"]);
    }

    #[test]
    fn a_drive_letter_directory_selects_windows_rules() {
        assert!(is_windows_dir(Path::new(r"C:\b")));
        assert!(is_windows_dir(Path::new("d:/b")));
        assert!(!is_windows_dir(Path::new("/home/b")));
        assert!(!is_windows_dir(Path::new("C")));
        let json = r#"[{"directory":"C:\\b","command":"C:\\PROGRA~1\\cl.exe /IC:\\dep\\include /c a.cpp","file":"a.cpp"}]"#;
        let u = units(json).unwrap();
        assert!(u[0].includes.iter().any(|i| i.to_string_lossy().contains(r"C:\dep\include")), "{:?}", u[0].includes);
    }

    use super::*;

    /// The probe's `fetch` database (measurements/probe-results.md), at /b.
    const PROBE: &str = r#"[
{"directory": "/b", "command": "/usr/bin/c++ -I/b/_deps/hdronly-src/include -I/b/_deps/urldep-src -o CMakeFiles/app.dir/main.cpp.o -c /s/main.cpp", "file": "/s/main.cpp"},
{"directory": "/b", "command": "/usr/bin/c++ -I/b/_deps/urldep-src -o x.o -c /b/_deps/urldep-src/urldep.cpp", "file": "/b/_deps/urldep-src/urldep.cpp"}
]"#;

    #[test]
    fn the_probe_database() {
        let u = units(PROBE).unwrap();
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].file, PathBuf::from("/s/main.cpp"));
        assert_eq!(
            include_dirs_of(&u).into_iter().collect::<Vec<_>>(),
            [PathBuf::from("/b/_deps/hdronly-src/include"), PathBuf::from("/b/_deps/urldep-src")]
        );
    }

    #[test]
    fn every_flag_form_and_both_entry_forms() {
        let json = r#"[
{"directory": "/b/sub", "arguments": ["c++", "-I", "rel/inc", "-isystem", "/sys", "-iquote/q", "-idirafter", "/late", "-Iquoted dir", "-c", "x.cpp"], "file": "x.cpp"},
{"directory": "/b", "command": "c++ \"-I/with space\" -I'/single q' -I/esc\\ aped -I../up -c y.cpp", "file": "y.cpp"},
{"directory": "C:/b", "command": "cl.exe /IC:/inc /I C:/inc2 -IC:/inc3 /c z.cpp", "file": "z.cpp"},
{"directory": "/b", "command": "c++ /Include/not/a/flag.cpp -c w.cpp", "file": "w.cpp"}
]"#;
        let u = units(json).unwrap();
        let s = |v: &Vec<PathBuf>| v.iter().map(|p| p.to_string_lossy().replace('\\', "/")).collect::<Vec<_>>();
        assert_eq!(s(&u[0].includes), ["/b/sub/rel/inc", "/sys", "/q", "/late", "/b/sub/quoted dir"]);
        assert_eq!(u[0].file, PathBuf::from("/b/sub/x.cpp"));
        assert_eq!(s(&u[1].includes), ["/with space", "/single q", "/esc aped", "/up"]);
        assert_eq!(u[2].includes.len(), 3, "cl: /I, /I <dir>, -I: {:?}", u[2].includes);
        assert!(u[3].includes.is_empty(), "/I is a flag only for cl: {:?}", u[3].includes);
    }

    #[test]
    fn empty_is_absent_and_garbage_is_malformed() {
        assert_eq!(units("").unwrap(), Vec::new());
        assert_eq!(units("  \n").unwrap(), Vec::new());
        assert_eq!(units("[{\"directory\": ").unwrap_err(), Malformed);
        assert_eq!(units("{}").unwrap_err(), Malformed);
    }

    #[test]
    fn resolution_rules_in_order() {
        let ep = BTreeMap::from([(PathBuf::from("/b/ep-prefix/src/ep"), "ep".to_string())]);
        let pkgs = BTreeMap::from([
            ("hdronly".to_string(), PathBuf::from("/p/lib/cmake/hdronly")),
            ("a".to_string(), PathBuf::from("/shared/lib/cmake/a")),
            ("b".to_string(), PathBuf::from("/shared/lib/cmake/b")),
        ]);
        let subs = BTreeMap::from([(PathBuf::from("/s/vendor/x"), "pkg:github/o/x@1".to_string())]);
        let r = Resolver { deps: PathBuf::from("/b/_deps"), ep_sources: &ep, package_dirs: &pkgs, submodules: &subs };
        let name = |n: &str| Some(Target::Name(n.to_string()));
        assert_eq!(r.resolve(Path::new("/b/_deps/hdronly-src/include")), name("hdronly"));
        assert_eq!(r.resolve(Path::new("/b/_deps/x-build")), None);
        assert_eq!(r.resolve(Path::new("/b/ep-prefix/src/ep/include")), name("ep"));
        assert_eq!(r.resolve(Path::new("/p/include")), name("hdronly"));
        assert_eq!(r.resolve(Path::new("/p/include/hdronly")), name("hdronly"));
        assert_eq!(r.resolve(Path::new("/shared/include")), None, "two packages share the prefix");
        assert_eq!(r.resolve(Path::new("/s/vendor/x/include")), Some(Target::Purl("pkg:github/o/x@1".into())));
        assert_eq!(r.resolve(Path::new("/usr/include")), None);
    }

    #[test]
    fn a_missing_directory_canonicalises_through_its_ancestors() {
        let d = tempfile::tempdir().unwrap();
        let c = canonical_lenient(&d.path().join("no/such"));
        assert_eq!(c, std::fs::canonicalize(d.path()).unwrap().join("no/such"));
    }
}
