//! Where these files are read, and how a path inside the scan root is
//! resolved (spec 1531 FR-001, FR-006, FR-012; research R3, R7).

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use super::RecordKind;

/// Directories pkg-config and `find_package` treat as library directories.
const LIB_DIRS: [&str; 5] = ["lib", "lib64", "lib32", "libx32", "share"];

/// Symlink hops followed before a path is treated as escaping (a cycle).
const MAX_HOPS: usize = 40;

/// `path` relative to `root`, tolerating a canonical/non-canonical
/// mismatch between the two (macOS `/var` → `/private/var`).
pub(crate) fn relative_to_root(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(rel) = path.strip_prefix(root) {
        return Some(rel.to_path_buf());
    }
    let canon_root = std::fs::canonicalize(root).ok()?;
    path.strip_prefix(&canon_root).ok().map(Path::to_path_buf)
}

/// A Debian/GNU multiarch triplet such as `x86_64-linux-gnu`.
pub(crate) fn is_multiarch_triplet(name: &str) -> bool {
    let parts: Vec<&str> = name.split('-').collect();
    (3..=4).contains(&parts.len())
        && parts.iter().all(|p| {
            !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

/// The install prefix (relative to the scan root) when `rel` is in a
/// recognised location for `kind`, else `None`.
pub(crate) fn prefix_of(rel: &Path, kind: RecordKind, name: &str) -> Option<PathBuf> {
    let comps: Vec<&str> = rel.components().map(|c| c.as_os_str().to_str()).collect::<Option<_>>()?;
    let n = comps.len();
    if n < 2 {
        return None;
    }
    let dirs = &comps[..n - 1];
    match kind {
        RecordKind::PkgConfig => {
            let d = dirs.len();
            if dirs.last() != Some(&"pkgconfig") || d < 2 {
                return None;
            }
            libdir_prefix(dirs, d - 2)
        }
        RecordKind::CmakeVersion => cmake_prefix(dirs, name),
    }
}

/// When `dirs[i]` is a library directory, the prefix above it:
/// `<prefix>/lib*` or `<prefix>/share` → `<prefix>`; `<prefix>/lib/<triplet>`
/// → `<prefix>`.
fn libdir_prefix(dirs: &[&str], i: usize) -> Option<PathBuf> {
    if LIB_DIRS.contains(&dirs[i]) {
        return Some(dirs[..i].iter().collect());
    }
    if i >= 1 && is_multiarch_triplet(dirs[i]) && dirs[i - 1] == "lib" {
        return Some(dirs[..i - 1].iter().collect());
    }
    None
}

/// The locations `find_package` searches for a package's config files:
/// - `<prefix>/(lib*|share|lib/<triplet>)/cmake/[<name>*/]`
/// - `<prefix>/(lib*|share|lib/<triplet>)/<name>*/(cmake|CMake)/`
/// - `<prefix>/<name>*/(cmake|CMake)/`
fn cmake_prefix(dirs: &[&str], name: &str) -> Option<PathBuf> {
    let d = dirs.len();
    let lname = name.to_ascii_lowercase();
    let starts_with_name = |s: &str| s.to_ascii_lowercase().starts_with(&lname);
    let is_cmake = |s: &str| s == "cmake" || s == "CMake";
    // (lib*|share)/cmake/  or  (lib*|share)/cmake/<name>*/
    for depth in [1usize, 2] {
        if d < depth + 1 {
            continue;
        }
        let cm = d - depth;
        if dirs[cm] != "cmake" {
            continue;
        }
        if depth == 2 && !starts_with_name(dirs[d - 1]) {
            continue;
        }
        if let Some(p) = libdir_prefix(dirs, cm - 1) {
            return Some(p);
        }
    }
    if d >= 2 && is_cmake(dirs[d - 1]) && starts_with_name(dirs[d - 2]) {
        // (lib*|share)/<name>*/cmake/
        if d >= 3 {
            if let Some(p) = libdir_prefix(dirs, d - 3) {
                return Some(p);
            }
        }
        // <prefix>/<name>*/cmake/
        return Some(dirs[..d - 2].iter().collect());
    }
    None
}

/// Resolve `rel` (relative to `root`) the way a process chrooted at `root`
/// would: an absolute symlink target is taken relative to the root, `..`
/// never climbs above it. Returns the absolute path under `root`, or `None`
/// when the path escapes the root, a link cycles, or a component cannot be
/// read. No host path outside `root` is opened or canonicalised.
pub(crate) fn resolve_in_root(root: &Path, rel: &Path) -> Option<PathBuf> {
    let mut queue: Vec<OsString> = Vec::new();
    push_front(&mut queue, rel)?;
    let mut cur: Vec<OsString> = Vec::new();
    let mut hops = 0usize;
    while let Some(c) = queue.pop() {
        if c == ".." {
            cur.pop()?;
            continue;
        }
        if c == "." || c.is_empty() {
            continue;
        }
        let mut candidate = root.to_path_buf();
        candidate.extend(&cur);
        candidate.push(&c);
        let meta = std::fs::symlink_metadata(&candidate).ok()?;
        if meta.file_type().is_symlink() {
            hops += 1;
            if hops > MAX_HOPS {
                return None;
            }
            let target = std::fs::read_link(&candidate).ok()?;
            if target.has_root() {
                cur.clear();
            }
            push_front(&mut queue, &target)?;
        } else {
            cur.push(c);
        }
    }
    let mut out = root.to_path_buf();
    out.extend(&cur);
    Some(out)
}

/// Push `p`'s normal components onto the front of `queue` (a stack whose
/// top is the next component). Root and prefix components are dropped: the
/// caller has already reset to the root for an absolute target.
fn push_front(queue: &mut Vec<OsString>, p: &Path) -> Option<()> {
    let comps: Vec<OsString> = p
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_os_string()),
            Component::ParentDir => Some(OsString::from("..")),
            Component::CurDir => Some(OsString::from(".")),
            Component::RootDir | Component::Prefix(_) => None,
        })
        .collect();
    queue.extend(comps.into_iter().rev());
    Some(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn pc(rel: &str) -> Option<PathBuf> {
        let name = Path::new(rel).file_stem().unwrap().to_str().unwrap().to_string();
        prefix_of(Path::new(rel), RecordKind::PkgConfig, &name)
    }

    fn cm(rel: &str, name: &str) -> Option<PathBuf> {
        prefix_of(Path::new(rel), RecordKind::CmakeVersion, name)
    }

    #[test]
    fn pkgconfig_locations() {
        assert_eq!(pc("opt/sdk/lib/pkgconfig/x.pc"), Some(PathBuf::from("opt/sdk")));
        assert_eq!(pc("usr/lib/x86_64-linux-gnu/pkgconfig/x.pc"), Some(PathBuf::from("usr")));
        assert_eq!(pc("usr/share/pkgconfig/x.pc"), Some(PathBuf::from("usr")));
        assert_eq!(pc("usr/lib64/pkgconfig/x.pc"), Some(PathBuf::from("usr")));
        assert_eq!(pc("lib/pkgconfig/x.pc"), Some(PathBuf::new()), "a prefix at the scan root");
        assert_eq!(
            pc(".venv/lib/python3.12/site-packages/sdk/lib/pkgconfig/x.pc"),
            Some(PathBuf::from(".venv/lib/python3.12/site-packages/sdk"))
        );
    }

    #[test]
    fn pkgconfig_rejected_locations() {
        assert_eq!(pc("x.pc"), None, "scan root");
        assert_eq!(pc("src/tests/x.pc"), None);
        assert_eq!(pc("pkgconfig/x.pc"), None, "no library directory above");
        assert_eq!(pc("src/pkgconfig/x.pc"), None);
        assert_eq!(pc("usr/x86_64-linux-gnu/pkgconfig/x.pc"), None, "triplet not under lib");
        assert_eq!(pc("usr/lib/pkgconfig/sub/x.pc"), None, "not directly in pkgconfig");
    }

    #[test]
    fn cmake_locations() {
        assert_eq!(cm("opt/sdk/lib64/cmake/Foo/FooConfigVersion.cmake", "Foo"), Some(PathBuf::from("opt/sdk")));
        assert_eq!(cm("opt/sdk/lib/cmake/FooConfigVersion.cmake", "Foo"), Some(PathBuf::from("opt/sdk")));
        assert_eq!(
            cm("usr/lib/x86_64-linux-gnu/cmake/foo/foo-config-version.cmake", "foo"),
            Some(PathBuf::from("usr"))
        );
        assert_eq!(cm("opt/sdk/share/foo-1.0/cmake/foo-config-version.cmake", "foo"), Some(PathBuf::from("opt/sdk")));
        assert_eq!(cm("opt/sdk/Foo-1.0/cmake/foo-config-version.cmake", "foo"), Some(PathBuf::from("opt/sdk")));
        assert_eq!(cm("opt/Foo/CMake/FooConfigVersion.cmake", "Foo"), Some(PathBuf::from("opt")));
    }

    #[test]
    fn cmake_rejected_locations() {
        assert_eq!(cm("cmake/FooConfigVersion.cmake", "Foo"), None, "repository root cmake/");
        assert_eq!(cm("FooConfigVersion.cmake", "Foo"), None);
        assert_eq!(cm("src/cmake/FooConfigVersion.cmake", "Foo"), None, "src is not lib* and not Foo*");
        assert_eq!(cm("opt/sdk/lib/cmake/Bar/FooConfigVersion.cmake", "Foo"), None, "directory names another package");
        assert_eq!(cm("opt/sdk/lib/cmake/a/b/FooConfigVersion.cmake", "Foo"), None, "too deep");
    }

    #[test]
    fn triplets() {
        assert!(is_multiarch_triplet("x86_64-linux-gnu"));
        assert!(is_multiarch_triplet("aarch64-linux-gnu"));
        assert!(is_multiarch_triplet("arm-linux-gnueabihf"));
        assert!(is_multiarch_triplet("x86_64-pc-linux-gnu"));
        assert!(!is_multiarch_triplet("python3.12"));
        assert!(!is_multiarch_triplet("foo-bar"));
        assert!(!is_multiarch_triplet("X86_64-linux-gnu"));
    }

    // T041: root-relative resolution.
    #[cfg(unix)]
    #[test]
    fn resolution_inside_the_root() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        std::fs::create_dir_all(root.join("opt/sdk/lib/pkgconfig")).unwrap();
        std::fs::write(root.join("opt/sdk/lib/pkgconfig/x.pc"), "Version: 1\n").unwrap();
        std::fs::create_dir_all(root.join("usr/lib/pkgconfig")).unwrap();
        std::fs::create_dir_all(tmp.path().join("outside")).unwrap();
        std::fs::write(tmp.path().join("outside/x.pc"), "Version: 9\n").unwrap();

        // Absolute target: taken relative to the root, not the host.
        symlink("/opt/sdk/lib/pkgconfig/x.pc", root.join("usr/lib/pkgconfig/abs.pc")).unwrap();
        assert_eq!(
            resolve_in_root(&root, Path::new("usr/lib/pkgconfig/abs.pc")),
            Some(root.join("opt/sdk/lib/pkgconfig/x.pc"))
        );
        // Relative target inside the root.
        symlink("../../../opt/sdk/lib/pkgconfig/x.pc", root.join("usr/lib/pkgconfig/rel.pc")).unwrap();
        assert_eq!(
            resolve_in_root(&root, Path::new("usr/lib/pkgconfig/rel.pc")),
            Some(root.join("opt/sdk/lib/pkgconfig/x.pc"))
        );
        // Relative target that climbs out of the root.
        symlink("../../../../outside/x.pc", root.join("usr/lib/pkgconfig/esc.pc")).unwrap();
        assert_eq!(resolve_in_root(&root, Path::new("usr/lib/pkgconfig/esc.pc")), None);
        // Absolute target naming a host path: resolved under the root, where
        // it does not exist.
        let host = tmp.path().join("outside/x.pc");
        symlink(&host, root.join("usr/lib/pkgconfig/host.pc")).unwrap();
        assert_eq!(resolve_in_root(&root, Path::new("usr/lib/pkgconfig/host.pc")), None);
        // A directory symlink in the middle of the path.
        symlink("/opt/sdk/lib", root.join("usr/lib/sdklib")).unwrap();
        assert_eq!(
            resolve_in_root(&root, Path::new("usr/lib/sdklib/pkgconfig/x.pc")),
            Some(root.join("opt/sdk/lib/pkgconfig/x.pc"))
        );
        // A cycle.
        symlink("b.pc", root.join("usr/lib/pkgconfig/a.pc")).unwrap();
        symlink("a.pc", root.join("usr/lib/pkgconfig/b.pc")).unwrap();
        assert_eq!(resolve_in_root(&root, Path::new("usr/lib/pkgconfig/a.pc")), None);
        // A plain file.
        assert_eq!(
            resolve_in_root(&root, Path::new("opt/sdk/lib/pkgconfig/x.pc")),
            Some(root.join("opt/sdk/lib/pkgconfig/x.pc"))
        );
    }
}
