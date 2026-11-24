//! Spec 1652 T007: `CMakeCache.txt`, as CMake writes it.
//!
//! One entry per line, `NAME:TYPE=VALUE`; a line without a type is
//! `NAME=VALUE`. `#` and `//` lines are comments. Split at the first `:`
//! before the first `=`, then at that `=`, so a value may contain either.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheKind {
    Bool,
    String,
    Path,
    FilePath,
    Internal,
    Static,
    Uninitialized,
    Untyped,
    Other,
}

impl CacheKind {
    fn parse(s: &str) -> Self {
        match s {
            "BOOL" => Self::Bool,
            "STRING" => Self::String,
            "PATH" => Self::Path,
            "FILEPATH" => Self::FilePath,
            "INTERNAL" => Self::Internal,
            "STATIC" => Self::Static,
            "UNINITIALIZED" => Self::Uninitialized,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheEntry {
    pub(crate) kind: CacheKind,
    pub(crate) value: String,
}

/// A parsed cache. Names are case-sensitive, as CMake stores them.
#[derive(Debug, Clone, Default)]
pub(crate) struct CacheView {
    entries: HashMap<String, CacheEntry>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CacheError {
    /// No line parsed as an entry: not a CMake cache.
    NoEntries,
}

impl CacheView {
    pub(crate) fn parse(text: &str) -> Result<Self, CacheError> {
        let mut entries = HashMap::new();
        for raw in text.lines() {
            let line = raw.trim_end_matches('\r');
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let Some(eq) = line.find('=') else { continue };
            let (key, value) = (&line[..eq], &line[eq + 1..]);
            let (name, kind) = match key.find(':') {
                Some(c) => (&key[..c], CacheKind::parse(&key[c + 1..])),
                None => (key, CacheKind::Untyped),
            };
            if name.is_empty() {
                continue;
            }
            entries.insert(name.to_string(), CacheEntry { kind, value: value.to_string() });
        }
        if entries.is_empty() {
            return Err(CacheError::NoEntries);
        }
        Ok(Self { entries })
    }

    pub(crate) fn get(&self, name: &str) -> Option<&CacheEntry> {
        self.entries.get(name)
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Values of every entry whose name equals `name` ignoring ASCII case:
    /// `find_package(HdrOnly)` writes `HdrOnly_DIR`.
    pub(crate) fn entries_ci<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.entries.iter().filter(move |(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, e)| e.value.as_str())
    }

    /// `CMAKE_HOME_DIRECTORY`: the source directory this build was configured from.
    pub(crate) fn home(&self) -> Option<&str> {
        self.get("CMAKE_HOME_DIRECTORY").map(|e| e.value.as_str()).filter(|v| !v.is_empty())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const PROBE: &str = "# This is the CMakeCache file.\n\
        //Path to a program.\n\
        BACKEND:STRING=A\n\
        FETCHCONTENT_SOURCE_DIR_URLDEP:PATH=\n\
        WITH_EXTRA:BOOL=OFF\n\
        hdronly_DIR:INTERNAL=/b/CMakeFiles/pkgRedirects\n\
        urldep_SOURCE_DIR:STATIC=/b/_deps/urldep-src\n\
        CMAKE_HOME_DIRECTORY:INTERNAL=/src\n";

    #[test]
    fn probe_lines_parse_with_their_types() {
        let c = CacheView::parse(PROBE).unwrap();
        assert_eq!(c.get("BACKEND").unwrap(), &CacheEntry { kind: CacheKind::String, value: "A".into() });
        assert_eq!(c.get("FETCHCONTENT_SOURCE_DIR_URLDEP").unwrap().value, "");
        assert_eq!(c.get("FETCHCONTENT_SOURCE_DIR_URLDEP").unwrap().kind, CacheKind::Path);
        assert_eq!(c.get("urldep_SOURCE_DIR").unwrap().kind, CacheKind::Static);
        assert_eq!(c.home(), Some("/src"));
        assert!(c.get("This is the CMakeCache file.").is_none());
    }

    #[test]
    fn a_value_may_contain_colons_and_equals() {
        let c = CacheView::parse("X:STRING=a:b=c\nY=1\n").unwrap();
        assert_eq!(c.get("X").unwrap().value, "a:b=c");
        assert_eq!(c.get("Y").unwrap(), &CacheEntry { kind: CacheKind::Untyped, value: "1".into() });
    }

    #[test]
    fn crlf_lines_and_an_empty_file() {
        let c = CacheView::parse("A:BOOL=ON\r\nB:STRING=x\r\n").unwrap();
        assert_eq!(c.get("A").unwrap().value, "ON");
        assert_eq!(c.get("B").unwrap().value, "x");
        assert_eq!(CacheView::parse("").unwrap_err(), CacheError::NoEntries);
        assert_eq!(CacheView::parse("# only a comment\n").unwrap_err(), CacheError::NoEntries);
    }
}
