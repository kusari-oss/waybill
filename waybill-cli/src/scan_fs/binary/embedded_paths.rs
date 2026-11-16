//! Absolute source paths embedded in a binary (spec 1534, #1532).
//!
//! Unstripped and assertion-carrying builds keep `__FILE__` strings and the
//! like in their read-only data. Each absolute path to a source file says
//! which tree compiled the binary. This module reads them from the string
//! region the scan already collects (`scan::collect_string_region`, at most
//! 16 MiB; research R1 measured that cap and the absence of DWARF to lose
//! nothing) and reduces each binary to a small finding:
//!
//! - the versioned packages its **compiled-source** paths name, through the
//!   **outermost** directory equal to a package name (research R2);
//! - the bundled third-party trees its paths name under a vendoring
//!   convention (research R4).
//!
//! Header paths never attribute: they name what was `#include`d, not what
//! was built. Toolchain paths never name a file's source.

use std::collections::BTreeSet;

/// Package names that collide with unrelated source trees (research R2:
/// SuiteSparse's `amd` matched Mesa's and the Linux kernel's `amd/`
/// directories). Grows only with a measurement.
pub(crate) const STOPLIST: &[&str] = &["amd"];

/// Vendoring directory names that are not projects (research R4: TheRock's
/// own `third_party/sysdeps/`). Grows only with a measurement.
pub(crate) const BUNDLED_EXCLUSIONS: &[&str] = &["sysdeps"];

const VENDOR_MARKERS: &[&str] = &["third_party", "third-party", "thirdparty", "3rdparty", "external", "extern", "vendor"];
const COMPILED: &[&str] = &["c", "cc", "cpp", "cxx", "c++", "hip", "cu"];
const HEADERS: &[&str] = &["h", "hh", "hpp", "hxx", "inl", "inc", "cuh", "ipp", "tcc"];
const TOOLCHAIN: &[&str] = &["/usr/", "/builddir/", "/opt/"];

/// What one binary's embedded paths say (data-model `EmbeddedPathFinding`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddedPathFinding {
    /// Versioned packages named by compiled-source paths.
    pub(crate) packages: BTreeSet<String>,
    /// A package was named only through header paths.
    pub(crate) header_only: bool,
    /// Bundled third-party trees, minus [`BUNDLED_EXCLUSIONS`].
    pub(crate) bundled: BTreeSet<String>,
    /// Bundled-tree names dropped by [`BUNDLED_EXCLUSIONS`].
    pub(crate) bundled_excluded: BTreeSet<String>,
    /// At least one absolute source path was found.
    pub(crate) has_paths: bool,
    /// Every source path found was a toolchain path.
    pub(crate) toolchain_only: bool,
}

/// What `binary::read` needs to run the extraction: the versioned package
/// names (possibly empty) and where to put each binary's finding, keyed by
/// the binary file component's `source_path`.
pub(crate) struct EmbeddedPathsCtx<'a> {
    pub(crate) names: &'a BTreeSet<String>,
    pub(crate) findings: &'a mut std::collections::BTreeMap<String, EmbeddedPathFinding>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Compiled,
    Header,
}

fn source_kind(path: &str) -> Option<Kind> {
    let ext = path.rsplit_once('.')?.1;
    if ext.contains('/') {
        return None;
    }
    if COMPILED.contains(&ext) {
        Some(Kind::Compiled)
    } else if HEADERS.contains(&ext) {
        Some(Kind::Header)
    } else {
        None
    }
}

fn path_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b'+' | b'@' | b'~')
}

/// Absolute paths in one NUL-terminated string: each maximal run of path
/// bytes that starts with `/` and holds at least two components. A `:line`
/// suffix ends the run.
fn paths_in(s: &[u8], out: &mut Vec<String>) {
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'/' && (i == 0 || !path_byte(s[i - 1])) {
            let start = i;
            while i < s.len() && path_byte(s[i]) {
                i += 1;
            }
            let run = &s[start..i];
            if run.iter().filter(|b| **b == b'/').count() >= 2 {
                if let Ok(p) = std::str::from_utf8(run) {
                    out.push(p.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
}

/// Read `region` (NUL-separated strings) against `names`, the versioned
/// package names (lowercase; may be empty: bundled detection needs none).
pub(crate) fn extract(region: &[u8], names: &BTreeSet<String>) -> EmbeddedPathFinding {
    let mut f = EmbeddedPathFinding::default();
    let mut header_names: BTreeSet<String> = BTreeSet::new();
    let mut any_source = false;
    let mut any_non_toolchain = false;
    let mut found = Vec::new();
    for s in region.split(|b| *b == 0) {
        found.clear();
        paths_in(s, &mut found);
        for p in &found {
            let Some(kind) = source_kind(p) else { continue };
            any_source = true;
            if TOOLCHAIN.iter().any(|t| p.starts_with(t)) {
                continue;
            }
            any_non_toolchain = true;
            let parts: Vec<&str> = p.trim_start_matches('/').split('/').collect();
            let dirs = &parts[..parts.len().saturating_sub(1)];
            let mut matched: Option<String> = None;
            let mut bundled: Option<String> = None;
            for (i, seg) in dirs.iter().enumerate() {
                let low = seg.to_ascii_lowercase();
                if VENDOR_MARKERS.contains(&low.as_str()) {
                    bundled = dirs.get(i + 1).map(|n| n.to_ascii_lowercase());
                    break;
                }
                if i > 0 && dirs[i - 1] == "_deps" {
                    if let Some(name) = low.strip_suffix("-src") {
                        bundled = Some(name.to_string());
                        break;
                    }
                }
                if matched.is_none() && names.contains(&low) && !STOPLIST.contains(&low.as_str()) {
                    matched = Some(low);
                }
            }
            if let Some(b) = bundled {
                if BUNDLED_EXCLUSIONS.contains(&b.as_str()) {
                    f.bundled_excluded.insert(b);
                } else if !b.is_empty() {
                    f.bundled.insert(b);
                }
                continue;
            }
            if let Some(m) = matched {
                match kind {
                    Kind::Compiled => {
                        f.packages.insert(m);
                    }
                    Kind::Header => {
                        header_names.insert(m);
                    }
                }
            }
        }
    }
    f.has_paths = any_source;
    f.toolchain_only = any_source && !any_non_toolchain;
    f.header_only = f.packages.is_empty() && !header_names.is_empty();
    f
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn region(paths: &[&str]) -> Vec<u8> {
        let mut r = Vec::new();
        for p in paths {
            r.extend_from_slice(p.as_bytes());
            r.push(0);
        }
        r
    }

    fn set(v: &[&str]) -> BTreeSet<String> {
        names(v)
    }

    #[test]
    fn a_compiled_source_names_its_package() {
        let f = extract(&region(&["/src/waybill-fixture-p/lib/x.cc"]), &names(&["waybill-fixture-p"]));
        assert_eq!(f.packages, set(&["waybill-fixture-p"]));
        assert!(f.has_paths && !f.toolchain_only && !f.header_only);
    }

    #[test]
    fn a_header_never_attributes() {
        let f = extract(&region(&["/src/waybill-fixture-p/include/x.hpp"]), &names(&["waybill-fixture-p"]));
        assert!(f.packages.is_empty());
        assert!(f.header_only);
        // A compiled source elsewhere still attributes; then it is not header-only.
        let f = extract(
            &region(&["/src/waybill-fixture-q/include/q.h", "/src/waybill-fixture-p/a.cpp"]),
            &names(&["waybill-fixture-p", "waybill-fixture-q"]),
        );
        assert_eq!(f.packages, set(&["waybill-fixture-p"]));
        assert!(!f.header_only);
    }

    /// Research R2: `hip/` inside `rocthrust/` names rocthrust, not hip.
    #[test]
    fn the_outermost_directory_wins() {
        let n = names(&["hip", "waybill-fixture-thrust"]);
        let f = extract(&region(&["/w/projects/waybill-fixture-thrust/testing/unittest/hip/t.cu"]), &n);
        assert_eq!(f.packages, set(&["waybill-fixture-thrust"]));
        let f = extract(&region(&["/w/projects/hip/src/a.cpp", "/w/projects/waybill-fixture-thrust/b.cu"]), &n);
        assert_eq!(f.packages, set(&["hip", "waybill-fixture-thrust"]));
    }

    #[test]
    fn the_stoplist_and_the_toolchain() {
        let f = extract(&region(&["/patch_source/src/amd/common/x.c"]), &names(&["amd"]));
        assert!(f.packages.is_empty());
        let f = extract(
            &region(&["/builddir/build/BUILD/gcc-13/libstdc++-v3/src/a.cc", "/usr/include/c++/13/x.h", "/opt/rh/x/y.c"]),
            &names(&["gcc-13", "libstdc++-v3"]),
        );
        assert!(f.packages.is_empty());
        assert!(f.has_paths && f.toolchain_only);
    }

    #[test]
    fn vendoring_markers_name_bundled_trees() {
        let f = extract(
            &region(&[
                "/src/p/third_party/waybill-fixture-fmt/src/format.cc",
                "/src/p/external/waybill-fixture-json/x.cpp",
                "/src/p/3rdparty/Waybill-Fixture-Abseil/a.cc",
                "/b/_deps/waybill-fixture-gtest-src/src/gtest.cc",
                "/src/p/third_party/sysdeps/zlib/inflate.c",
            ]),
            &names(&["p"]),
        );
        assert_eq!(f.bundled, set(&["waybill-fixture-fmt", "waybill-fixture-json", "waybill-fixture-abseil", "waybill-fixture-gtest"]));
        assert_eq!(f.bundled_excluded, set(&["sysdeps"]));
        // A path inside a bundled tree never attributes, even though `p` is above it.
        assert!(f.packages.is_empty());
    }

    #[test]
    fn a_vendored_copy_of_a_named_package_is_bundled_not_attributed() {
        let f = extract(&region(&["/src/app/third_party/waybill-fixture-p/x.cc"]), &names(&["waybill-fixture-p"]));
        assert!(f.packages.is_empty());
        assert_eq!(f.bundled, set(&["waybill-fixture-p"]));
    }

    #[test]
    fn path_shapes() {
        let n = names(&["waybill-fixture-p"]);
        // A `:line` suffix, surrounding text, an assertion message.
        let f = extract(b"Assertion failed at /src/waybill-fixture-p/a.cpp:123: x != 0\0", &n);
        assert_eq!(f.packages, set(&["waybill-fixture-p"]));
        // Not a source extension; a relative path; a bare word.
        let f = extract(&region(&["/src/waybill-fixture-p/a.txt", "waybill-fixture-p/a.cpp", "a.cpp"]), &n);
        assert!(f.packages.is_empty() && !f.has_paths);
        // Non-UTF-8 bytes and an empty region.
        let mut r = b"\xff\xfe/src/waybill-fixture-p/b.c\0".to_vec();
        r.extend_from_slice(b"\0\0");
        assert_eq!(extract(&r, &n).packages, set(&["waybill-fixture-p"]));
        assert_eq!(extract(&[], &n), EmbeddedPathFinding::default());
    }

    #[test]
    fn no_names_still_finds_bundled_trees() {
        let f = extract(&region(&["/src/p/third_party/waybill-fixture-fmt/f.cc"]), &BTreeSet::new());
        assert_eq!(f.bundled, set(&["waybill-fixture-fmt"]));
        assert!(f.packages.is_empty());
    }
}
