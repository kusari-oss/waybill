// Milestone 055 — `ModuleId` newtype for Go (module-path, version) pairs.
//
// Module-level `#[allow(dead_code)]`: the foundational scaffold (T003)
// lands the type ahead of the US1 wiring tasks (T024–T025) that
// actually consume it. The allow is removed in T025 once `legacy::read()`
// builds `ModuleId`s from `go.sum` entries.
#![allow(dead_code)]

//
// Per Constitution Principle IV (Type-Driven Correctness), the resolver
// MUST NOT pass raw `(String, String)` tuples across function boundaries
// for module identity. This newtype carries that semantic and provides
// a stable `Display` form (`<path>@<version>`) matching the format
// emitted by `go mod graph` and the `go.sum` lockfile.
//
// See specs/055-go-transitive-edges/data-model.md for the wider design
// context and specs/055-go-transitive-edges/spec.md FR-001 / FR-003 for
// the role of `ModuleId` as the canonical key set anchored on go.sum.

use std::fmt;

/// A Go (module-path, version) pair.
///
/// `path` is a module path string like `github.com/Azure/azure-sdk-for-go`.
/// `version` is either an SemVer-style tag (`v1.2.3`,
/// `v2.0.0+incompatible`) or a Go pseudo-version
/// (`v0.0.0-20211123-abcd1234abcd`).
///
/// `ModuleId` carries no validation beyond non-emptiness — inputs come from
/// already-parsed `go.sum` and `go.mod` files where structural invariants
/// are upstream-enforced. The fields are hidden behind accessors so that
/// raw construction is forced through `new()`, which preserves the
/// "newtype, not a tuple" contract per Constitution Principle IV.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModuleId {
    path: String,
    version: String,
}

impl ModuleId {
    /// Construct a new `ModuleId`. Empty `path` or `version` are accepted
    /// (downstream code may treat them as the main-module sentinel) but a
    /// `debug_assert!` flags non-trivially malformed inputs in debug
    /// builds without affecting release behavior.
    pub fn new(path: impl Into<String>, version: impl Into<String>) -> Self {
        let path = path.into();
        let version = version.into();
        debug_assert!(
            !path.contains(char::is_whitespace),
            "ModuleId.path must not contain whitespace: {path:?}",
        );
        debug_assert!(
            !version.contains(char::is_whitespace),
            "ModuleId.version must not contain whitespace: {version:?}",
        );
        Self { path, version }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

impl fmt::Display for ModuleId {
    /// Renders `<path>@<version>` matching `go mod graph` output and
    /// `go.sum` line format. The main module (no version) is rendered
    /// without the `@` separator.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.version.is_empty() {
            f.write_str(&self.path)
        } else {
            write!(f, "{}@{}", self.path, self.version)
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn display_renders_path_at_version() {
        let m = ModuleId::new("github.com/foo/bar", "v1.2.3");
        assert_eq!(m.to_string(), "github.com/foo/bar@v1.2.3");
    }

    #[test]
    fn display_omits_at_when_version_empty() {
        // Main-module sentinel: no version on the parent in `go mod graph`
        // output.
        let m = ModuleId::new("example.com/main", "");
        assert_eq!(m.to_string(), "example.com/main");
    }

    #[test]
    fn equality_is_value_based() {
        let a = ModuleId::new("github.com/foo/bar", "v1.0.0");
        let b = ModuleId::new("github.com/foo/bar", "v1.0.0");
        let c = ModuleId::new("github.com/foo/bar", "v2.0.0");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn ordering_is_lexicographic_on_path_then_version() {
        let a = ModuleId::new("a/foo", "v1.0.0");
        let b = ModuleId::new("a/foo", "v2.0.0");
        let c = ModuleId::new("b/foo", "v1.0.0");
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn hash_lookup_in_hashset() {
        let mut set = HashSet::new();
        set.insert(ModuleId::new("github.com/foo/bar", "v1.0.0"));
        assert!(set.contains(&ModuleId::new("github.com/foo/bar", "v1.0.0")));
        assert!(!set.contains(&ModuleId::new("github.com/foo/bar", "v2.0.0")));
    }

    #[test]
    fn accessors_return_underlying_strings() {
        let m = ModuleId::new("github.com/Azure/azure-sdk-for-go", "v1.2.3");
        assert_eq!(m.path(), "github.com/Azure/azure-sdk-for-go");
        assert_eq!(m.version(), "v1.2.3");
    }
}
