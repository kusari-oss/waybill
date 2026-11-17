//! Filesystem walker that finds candidate binary files.
//!
//! Walks `rootfs` looking for regular files whose first 16 bytes
//! match a known binary magic (ELF / Mach-O / PE). Skips hidden
//! and build directories; ignores files outside the size envelope.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Milestone 054 FR-003: max recursion depth for the `walk_dir`
/// filesystem traversal. Default ceiling per the spec; not tightened
/// because binaries can sit anywhere in a rootfs (no shallow-by-
/// convention structural constraint to justify a tighter bound).
/// Defense-in-depth backstop for the canonicalize-keyed visited-set
/// primary mechanism (FR-002).
const MAX_WALK_DEPTH: usize = 16;

/// Walk `rootfs` for regular files, probing the first 16 bytes of
/// each for a known binary magic. Skips hidden / build dirs. Ignores
/// files <1 KB or >500 MB (defense-in-depth).
///
/// `claimed_paths` is used as a fast-path filter — paths already
/// claimed by the OS-package readers (dpkg / apk / rpm / pip / etc.)
/// are skipped BEFORE the magic-byte probe. On OS-only container
/// scans (e.g., debian:12-slim, where dpkg claims every executable),
/// this drops the walker from 304ms to ~50ms by avoiding thousands
/// of no-op file opens. The check uses Layer-1 raw-path match only;
/// canonical + inode layers still run inside `binary::read` per
/// `is_claimed_binary` so correctness is preserved (a false-miss
/// here just means the file falls through to the full probe + the
/// downstream layered claim check).
///
/// #1524: what a directory scan does with test-input binaries.
pub struct TestInputs<'a> {
    /// Root-relative paths of the binaries not returned.
    pub skipped: &'a mut Vec<PathBuf>,
    /// Spec 1541 (#1593): absolute install prefixes of the installed
    /// libraries the tree holds. A test-input binary under one is an
    /// installed package's test, and is returned. One vendored prefix in a
    /// source tree no longer makes the whole tree "installed": on ROCm
    /// TheRock, a single committed `lib/cmake/<pkg>/` file did, and 550
    /// LLVM test inputs were reported.
    pub installed_prefixes: &'a [PathBuf],
}

/// `test_inputs` — #1524. `Some` in a directory scan: a binary whose
/// root-relative parent path contains a [`TEST_INPUT_DIRS`] component, and
/// that is under none of [`TestInputs::installed_prefixes`], is not
/// returned; its root-relative path is pushed to
/// [`TestInputs::skipped`] instead. `None` (image and rootfs scans, nix
/// outputs) returns every binary, because an installed system's
/// `/usr/lib/<pkg>/tests/` is not a fixture.
pub(super) fn discover_binaries(
    root: &Path,
    claimed_paths: &HashSet<PathBuf>,
    exclude_set: &crate::scan_fs::package_db::exclude_path::ExclusionSet,
    mut test_inputs: Option<TestInputs<'_>>,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_file() {
        if !claimed_paths.contains(root) && is_supported_binary(root) {
            out.push(root.to_path_buf());
        }
        return out;
    }
    // Milestone 114: delegates to scan_fs::walk::safe_walk.
    // Milestone 118 (issue #343 / FR-002): thread the operator's
    // ExclusionSet through so binary-tier discovery honors
    // `--exclude-path` the same way ecosystem-tier walkers do.
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let name = candidate.file_name().and_then(|s| s.to_str()).unwrap_or("");
            matches!(
                name,
                ".git" | "target" | "node_modules" | ".cargo" | "__pycache__" | ".venv"
            )
        },
        exclude_set,
    };
    // #1056: report each binary at its resolved location under `root`,
    // not the route the walk took — a usrmerged `lib/` alias would
    // otherwise flip the recorded path between runs of one image. Joined
    // back onto `root` (not canonicalized whole) so the later
    // root-relative normalisation still strips the prefix the caller
    // passed.
    let mut resolved = crate::scan_fs::walk::ResolvedPaths::new(root);
    crate::scan_fs::walk::safe_walk(root, &cfg, |path| {
        // Fast-path skip: claimed by an OS-package reader ⇒ definitely
        // not an unattributed binary. Saves the file-open + magic-byte
        // read on every already-claimed file.
        if claimed_paths.contains(path) {
            return;
        }
        if path.is_file() && is_supported_binary(path) {
            let rel = resolved.relative(path);
            let reported = rel
                .as_ref()
                .map(|rel| root.join(rel))
                .unwrap_or_else(|| path.to_path_buf());
            if claimed_paths.contains(&reported) {
                return;
            }
            if let (Some(t), Some(rel)) = (test_inputs.as_mut(), rel) {
                let installed = t
                    .installed_prefixes
                    .iter()
                    .any(|p| reported.starts_with(p) || path.starts_with(p));
                if is_under_test_input_dir(&rel) && !installed {
                    t.skipped.push(rel);
                    return;
                }
            }
            out.push(reported);
        }
    });
    out
}

/// #1524 — directory names that hold toolchain test inputs: core dumps,
/// deliberately corrupt ELF, cross-platform objects. On the llvm-project
/// fork #1524 reports, every one of the 546 binary-analyzed components sat
/// under one of these. Exact, case-sensitive component equality.
pub(crate) const TEST_INPUT_DIRS: [&str; 4] = ["test", "tests", "unittests", "Inputs"];

/// Whether a root-relative path lies under a [`TEST_INPUT_DIRS`] directory.
/// Only the parent components count: a binary that is itself named `test`
/// is not a test input.
pub(crate) fn is_under_test_input_dir(rel: &Path) -> bool {
    rel.parent().is_some_and(|parent| {
        parent.components().any(|c| match c {
            std::path::Component::Normal(name) => {
                name.to_str().is_some_and(|n| TEST_INPUT_DIRS.contains(&n))
            }
            _ => false,
        })
    })
}

fn is_supported_binary(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    match f.read_exact(&mut magic) {
        Ok(()) => detect_format(&magic).is_some(),
        Err(_) => false,
    }
}

/// Detect binary format by first-4-bytes magic. Returns the
/// canonical `binary-class` string per FR-021.
pub(crate) fn detect_format(magic: &[u8]) -> Option<&'static str> {
    if magic.len() < 4 {
        return None;
    }
    // ELF: 0x7F 'E' 'L' 'F'
    if magic == [0x7F, b'E', b'L', b'F'] {
        return Some("elf");
    }
    // Mach-O: MH_MAGIC (0xFEEDFACE), MH_CIGAM (0xCEFAEDFE), MH_MAGIC_64
    // (0xFEEDFACF), MH_CIGAM_64 (0xCFFAEDFE), fat-binary variants
    // (0xCAFEBABE / 0xBEBAFECA).
    if matches!(
        magic,
        [0xFE, 0xED, 0xFA, 0xCE]
            | [0xCE, 0xFA, 0xED, 0xFE]
            | [0xFE, 0xED, 0xFA, 0xCF]
            | [0xCF, 0xFA, 0xED, 0xFE]
            | [0xCA, 0xFE, 0xBA, 0xBE]
            | [0xBE, 0xBA, 0xFE, 0xCA]
    ) {
        return Some("macho");
    }
    // PE: starts with "MZ" (0x4D 0x5A) in the DOS header; a real PE
    // also has a PE\0\0 signature at the offset stored at 0x3C.
    // First-4-bytes probe is necessarily optimistic — full PE
    // validation happens at parse time via `object::read::File::parse`.
    if &magic[..2] == b"MZ" {
        return Some("pe");
    }
    None
}

// Milestone 100: the `tests` module is gated `#[cfg(unix)]` because
// its only test (`walks_symlink_loop_without_hanging`) uses
// `std::os::unix::fs::symlink`, which is POSIX-only. Without the
// module-level gate, `use super::*;` would be unused on Windows and
// trip `-D unused-imports`. If future Windows-portable tests are
// added here, drop the module gate and reapply `#[cfg(unix)]` to
// individual POSIX-only test functions instead.
#[cfg(all(test, unix))]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Milestone 054 SC-002 + FR-009: walker terminates promptly on
    /// a synthesized minimal symlink-loop fixture instead of hanging
    /// indefinitely. Same shape as rpm_file's regression guard.
    #[test]
    fn walks_symlink_loop_without_hanging() {
        let tmp = tempfile::tempdir().unwrap();
        let loop_dir = tmp.path().join("loop");
        std::fs::create_dir_all(&loop_dir).unwrap();
        std::os::unix::fs::symlink(&loop_dir, loop_dir.join("link")).unwrap();
        let result = discover_binaries(
            tmp.path(),
            &std::collections::HashSet::new(),
            &crate::scan_fs::package_db::exclude_path::ExclusionSet::default(),
            None,
        );
        assert!(result.is_empty());
    }

    /// #1056: a binary reachable through a symlinked directory is
    /// reported at its resolved location under the root, not via the
    /// route `safe_walk` happened to take. Sixteen aliases so at least one
    /// is listed ahead of `usr` under lexical, creation, and APFS hash
    /// order (see `file_tier::walker::m958_path_is_reported_at_its_resolved_location`).
    #[test]
    fn binary_is_reported_at_its_resolved_location() {
        let tmp = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("usr/lib", tmp.path().join("lib")).unwrap();
        for i in 0..16 {
            std::os::unix::fs::symlink("usr/lib", tmp.path().join(format!("alias{i:02}")))
                .unwrap();
        }
        let real = tmp.path().join("usr/lib/app");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("tool"), b"\x7FELF\x02\x01\x01\x00binary").unwrap();

        let result = discover_binaries(
            tmp.path(),
            &std::collections::HashSet::new(),
            &crate::scan_fs::package_db::exclude_path::ExclusionSet::default(),
            None,
        );
        assert_eq!(result, vec![tmp.path().join("usr/lib/app/tool")]);
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod test_input_tests {
    //! #1524 — binaries under test-input directories in a directory scan.

    use super::*;

    fn elf(tag: u8) -> Vec<u8> {
        let mut b = b"\x7FELF\x02\x01\x01\x00".to_vec();
        b.resize(64, tag);
        b
    }

    fn plant(root: &Path, rels: &[&str]) {
        for (i, rel) in rels.iter().enumerate() {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, elf(i as u8)).unwrap();
        }
    }

    fn discover(root: &Path, skip: bool) -> (Vec<PathBuf>, Vec<PathBuf>) {
        discover_keeping(root, skip, &[])
    }

    fn discover_keeping(root: &Path, skip: bool, installed_prefixes: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let mut skipped = Vec::new();
        let mut found = discover_binaries(
            root,
            &HashSet::new(),
            &crate::scan_fs::package_db::exclude_path::ExclusionSet::default(),
            skip.then_some(TestInputs { skipped: &mut skipped, installed_prefixes }),
        );
        found.sort();
        skipped.sort();
        (found, skipped)
    }

    /// Exact component equality on the parent path. Names that only contain
    /// or resemble a test-input name, and a binary that is itself named
    /// `test`, are not test inputs.
    #[test]
    fn matching_is_exact_on_parent_components() {
        for rel in [
            "test/x",
            "tests/x",
            "a/unittests/b/x",
            "clang/test/Inputs/x",
            "Inputs/x",
        ] {
            assert!(is_under_test_input_dir(Path::new(rel)), "{rel}");
        }
        for rel in [
            "bin/test",
            "tests",
            "testing/x",
            "contest/x",
            "test-data/x",
            "Tests/x",
            "inputs/x",
            "unittest/x",
            "bin/x",
        ] {
            assert!(!is_under_test_input_dir(Path::new(rel)), "{rel}");
        }
    }

    /// The fix, and its positive control in the same walk: the binary under
    /// `tests/` is recorded and not returned; the one under `bin/` is
    /// returned.
    #[test]
    fn a_directory_scan_records_test_inputs_instead_of_returning_them() {
        let tmp = tempfile::tempdir().unwrap();
        plant(tmp.path(), &["bin/tool", "tests/Inputs/core", "src/unittests/obj"]);
        let (found, skipped) = discover(tmp.path(), true);
        assert_eq!(found, vec![tmp.path().join("bin/tool")]);
        assert_eq!(
            skipped,
            vec![PathBuf::from("src/unittests/obj"), PathBuf::from("tests/Inputs/core")]
        );
    }

    /// `None` is the image, rootfs and nix-output path: every binary is
    /// returned and nothing is recorded.
    #[test]
    fn without_a_sink_every_binary_is_returned() {
        let tmp = tempfile::tempdir().unwrap();
        plant(tmp.path(), &["bin/tool", "tests/Inputs/core"]);
        let (found, skipped) = discover(tmp.path(), false);
        assert_eq!(found, vec![tmp.path().join("bin/tool"), tmp.path().join("tests/Inputs/core")]);
        assert!(skipped.is_empty());
    }

    /// Matching is relative to the scan root: scanning a directory that is
    /// itself named `tests` reads what is in it.
    #[test]
    fn the_scan_root_s_own_name_does_not_count() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("tests");
        plant(&root, &["tool", "sub/other"]);
        let (found, skipped) = discover(&root, true);
        assert_eq!(found, vec![root.join("sub/other"), root.join("tool")]);
        assert!(skipped.is_empty());
    }

    /// Spec 1541 (#1593): a test-input binary under an installed library's
    /// prefix is that package's test and is reported; one elsewhere in the
    /// same tree is still skipped.
    #[test]
    fn s1541_only_test_inputs_under_an_installed_prefix_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        plant(
            dir.path(),
            &["vendor/sdk/lib/pkg/tests/a", "llvm/test/Object/Inputs/b", "prebuilt/c"],
        );
        let prefix = dir.path().join("vendor/sdk");
        let (found, skipped) = discover_keeping(dir.path(), true, std::slice::from_ref(&prefix));
        let rel: Vec<PathBuf> =
            found.iter().map(|p| p.strip_prefix(dir.path()).unwrap().to_path_buf()).collect();
        assert_eq!(rel, vec![PathBuf::from("prebuilt/c"), PathBuf::from("vendor/sdk/lib/pkg/tests/a")]);
        assert_eq!(skipped, vec![PathBuf::from("llvm/test/Object/Inputs/b")]);
    }
}
