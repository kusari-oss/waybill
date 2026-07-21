//! Milestone 114 — shared filesystem-walking helper for every ecosystem-
//! reader source-tree walker.
//!
//! Pre-114, each ecosystem reader (`cargo.rs`, `maven.rs`, `gem.rs`,
//! `golang/legacy.rs`, `go_binary.rs`, `rpm_file.rs`, `nuget/mod.rs`,
//! `yocto/recipe.rs`, `binary/discover.rs`, `binary/source_binding/
//! cmake_observer.rs`, `package_db/project_roots.rs`) carried its own
//! `fn walk_*` recursion implementing the same canonicalize-keyed
//! visited-set + depth-bound + skip-list machinery. The duplication was
//! a milestone-054 hazard: a contributor adding a new reader would copy-
//! paste the closest existing walker and risk dropping a loop-
//! protection invariant.
//!
//! Post-114 every standard ecosystem walker delegates to
//! [`safe_walk`] in this module. Contributors writing a new reader
//! configure a [`WalkConfig`] + visit callback once; the canonicalize-
//! keyed visited-set, depth bound, milestone-113 directory-exclusion
//! check, and `tracing::debug!` skip-cause logging all live here.
//!
//! See `specs/114-safe-walk-migration/` for the full design (spec.md +
//! plan.md + research.md + data-model.md + contracts/walk-api.md +
//! quickstart.md).
//!
//! # Audit pattern
//!
//! Run:
//!
//! ```sh
//! grep -rEn 'fn walk[_(]' waybill-cli/src/scan_fs/
//! ```
//!
//! Acceptable matches fall into two categories:
//!
//! ## A. Filesystem ecosystem-discovery walkers
//!
//! - `scan_fs/walk.rs` (this file) — [`safe_walk`] + internal helpers.
//! - `scan_fs/walker.rs` — **documented exception**: whole-filesystem
//!   file enumerator with `since` mtime filter + `size_cap` byte cap +
//!   content-hashing inside the walk. Has no skip list, no depth
//!   bound, and yields every matching file under the root. None of
//!   [`WalkConfig`]'s three fields fit; migrating would bloat the
//!   helper API. Stays hand-rolled.
//! - `scan_fs/package_db/npm/walk.rs` — **documented exception**:
//!   npm `@scope`-aware walker. Recurses one level only into
//!   directories whose name starts with `@`, and propagates an
//!   `in_npm_internals: bool` per-descent state through the recursive
//!   calls (npm-self-bundled-internals tagging — feature 005 US1).
//!   The generic [`WalkConfig::should_skip`] can't express either
//!   semantic. Stays hand-rolled.
//! - `scan_fs/binary/source_binding/cmake_observer.rs::walk_for_cmake_build_dirs`
//!   — **documented exception**: stop-at-match descent. When a
//!   directory IS a cmake project build root (`CMakeCache.txt` + non-
//!   empty `_deps/`), the walker records it and does NOT descend
//!   into its `_deps/<name>-build/` subdirectories (which are sub-
//!   builds, not independent cmake projects). [`safe_walk`]'s visit
//!   callback fires before descent is decided, leaving no way to
//!   "visit this dir AND suppress its child descent" in a single
//!   pass. Stays hand-rolled.
//! - `scan_fs/package_db/maven_sidecar.rs::walk` (impl method) —
//!   **documented exception**: M2 cache walker that intentionally
//!   uses `entry.file_type()` (lstat-equivalent — does NOT dereference
//!   symlinks) instead of canonicalize-keyed visited-set for symlink-
//!   loop protection. Per the explicit milestone-054 audit comment
//!   at maven_sidecar.rs:158–163, this is FR-001 audit rubric option
//!   (b): the lstat skip is the primary invariant, depth cap is
//!   defense-in-depth. Migrating to [`safe_walk`] would silently
//!   change the symlink-resolution semantics for cached .pom files
//!   in the M2 tree. Stays hand-rolled.
//!
//! ## B. Non-filesystem-walker false positives (catch the regex but aren't walkers)
//!
//! - `scan_fs/package_db/maven.rs::walk_m2_jars` — iterates a
//!   precomputed `Vec<PathBuf>` returned by `MavenRepoCache::discover`;
//!   no `read_dir` recursion.
//! - `scan_fs/package_db/maven.rs::walk_jar_maven_meta` — walks JAR
//!   archive internal content (via the `zip` crate), not the
//!   filesystem.
//! - `scan_fs/package_db/maven.rs::MavenRepoCache::walk_rootfs_poms`
//!   — multi-root cap-bounded stack-based walker for the M2 cache.
//!   Different semantics (multiple roots + cap-limit + no
//!   canonicalize-keyed dedup, by design — `seen` is keyed on
//!   `(group, artifact, version)` not on canonical paths). Migrating
//!   would require multi-root + cap-bounded support in
//!   [`WalkConfig`]; deferred to a future milestone if needed.
//! - `scan_fs/package_db/rpmdb_sqlite/schema.rs::walk_schema_page`
//!   — SQLite B-tree page walker, not filesystem.
//! - Test functions named `walks_*`, `walk_jar_*`, `walk_fat_jar_*`,
//!   `walk_rootfs_poms_*` (typically indented inside
//!   `#[cfg(test)] mod tests`) — tests OF walkers, not walkers.
//!
//! ## Review policy
//!
//! Any `fn walk[_(]` match in `waybill-cli/src/scan_fs/` OUTSIDE the
//! union of categories A and B above is a regression: a contributor
//! introduced a new hand-rolled filesystem walker bypassing this
//! helper. Reviewer action: reject the PR or push back to either
//! (a) migrate the new walker to [`safe_walk`], or (b) add a new
//! entry to this comment block with a one-sentence reason. The
//! exception list MUST stay short — three entries is plausible,
//! ten is the abstraction failing and we should rethink.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Per-call configuration for [`safe_walk`].
///
/// Three fields; no defaults; callers MUST supply every field. Future
/// expansion happens by adding fields (callers update at the call site)
/// rather than by builder patterns or markers.
pub(crate) struct WalkConfig<'a> {
    /// Max recursion depth. Defense-in-depth backstop for the
    /// canonicalize-keyed visited-set: if canonicalization is
    /// unavailable (sandboxed filesystem, missing `realpath` perms),
    /// `max_depth` guarantees bounded termination.
    ///
    /// Per the milestone-054 audit: existing walker constants pick 6
    /// or 8 or 10. The helper does not impose a default; pick the
    /// value appropriate for your use case.
    pub max_depth: usize,

    /// Predicate consulted before descent into each child directory.
    /// `(candidate, rootfs)` lets the closure extract the candidate's
    /// filename via `.file_name()` AND compute the candidate's path
    /// relative to the scan root (today required by the milestone-113
    /// directory-exclusion mechanism, tomorrow potentially by other
    /// path-relative skip rules). Returning `true` suppresses descent.
    pub should_skip: &'a dyn Fn(&Path, &Path) -> bool,

    /// Milestone-113 user-supplied directory exclusion. Consulted by
    /// the helper AFTER `should_skip`, as a separate fast-path step
    /// (the helper short-circuits when `exclude_set.is_empty()`
    /// rather than invoking the closure). Centralizing the exclusion
    /// check inside the helper rather than inside every per-walker
    /// `should_skip` closure means EVERY walker logs skip events
    /// uniformly via `tracing::debug!` post-migration; pre-migration
    /// only `project_roots.rs` did.
    pub exclude_set: &'a super::package_db::exclude_path::ExclusionSet,
}

/// Walk every directory + file under `rootfs` (depth-bounded by
/// `cfg.max_depth`), invoking `visit` once per visited path. Files and
/// directories both flow through the same callback; the caller's
/// closure discriminates via `path.is_file()` / `path.is_dir()` and
/// any name/extension check it needs.
///
/// # Guarantees
///
/// - **Symlink-loop bounded termination**: canonicalize-keyed
///   `HashSet<PathBuf>` visited-set; a symlink loop's second arrival
///   re-inserts the same canonical key and the helper returns early.
/// - **Depth-bounded recursion**: descent stops below
///   `cfg.max_depth` even if directories exist underneath.
/// - **Tolerant of unreadable directories**: `read_dir().ok()` early-
///   returns; the helper continues processing peer directories.
/// - **Single `visit` per canonical directory**: the visited-set
///   guard fires before `visit`. Files visited multiple times
///   (e.g., via different symlinks) would be visited multiple times
///   because the visited-set keys on directories, not files — this
///   matches the pre-migration per-walker behavior across the
///   codebase.
/// - **Skip cause logged**: `tracing::debug!` emitted at every skip
///   decision with `cause` field (`"built-in"` from `should_skip`
///   vs `"exclude-path"` from the milestone-113 mechanism).
///
/// # Caller contract
///
/// - `cfg.should_skip` MUST be a pure function — no I/O, no mutable
///   state. The helper invokes it once per child directory.
/// - `visit` may be any `FnMut`; closures with captured `&mut`
///   state are supported.
/// - The helper does NOT return errors. I/O errors are silently
///   swallowed (matches every pre-migration walker's tolerance
///   posture).
pub(crate) fn safe_walk<F: FnMut(&Path)>(rootfs: &Path, cfg: &WalkConfig, mut visit: F) {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    // Canonicalize once. Used by `walk_inner` to enforce the rootfs
    // sandbox: any child dir whose canonical form falls outside this
    // prefix is rejected, preventing absolute symlinks inside an
    // extracted image rootfs (e.g. alpine's `/var/run -> /run`) from
    // pulling host filesystem content into the SBOM. Issue #396.
    let canonical_rootfs = std::fs::canonicalize(rootfs).unwrap_or_else(|_| rootfs.to_path_buf());
    walk_inner(rootfs, rootfs, &canonical_rootfs, 0, cfg, &mut visit, &mut visited);
}

fn walk_inner<F: FnMut(&Path)>(
    dir: &Path,
    rootfs: &Path,
    canonical_rootfs: &Path,
    depth: usize,
    cfg: &WalkConfig,
    visit: &mut F,
    visited: &mut HashSet<PathBuf>,
) {
    // Guard against symlink loops and duplicate enumeration. Use the
    // canonical path when available; fall back to `dir` as-is so a
    // missing dir doesn't silently swallow the scan.
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
        return;
    }

    visit(dir);

    if depth >= cfg.max_depth {
        return;
    }

    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            // Files are yielded through `visit` immediately. The
            // caller's closure decides whether to act on them (the
            // typical per-ecosystem reader matches an extension or
            // a literal filename and pushes into a Vec).
            visit(&path);
            continue;
        }
        // Directory child: gate descent on `should_skip` first, then
        // on the milestone-113 exclusion set.
        if (cfg.should_skip)(&path, rootfs) {
            tracing::debug!(
                candidate = %path.display(),
                cause = "built-in",
                "safe_walk: skipping directory matched by user-supplied skip predicate"
            );
            continue;
        }
        if !cfg.exclude_set.is_empty() {
            if let Ok(rel) = path.strip_prefix(rootfs) {
                let rel_str = rel.to_string_lossy();
                if cfg.exclude_set.matches(&rel_str) {
                    // Milestone 118 (#343 / FR-010) — counter feeds the
                    // scan-end `tracing::info!` summary at scan_cmd.rs.
                    // `Relaxed` ordering per research.md § Decision 1.
                    cfg.exclude_set.suppressed_dirs.fetch_add(
                        1,
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    tracing::debug!(
                        candidate = %rel.display(),
                        cause = "exclude-path",
                        "safe_walk: skipping directory matched by milestone-113 ExclusionSet"
                    );
                    continue;
                }
            }
        }
        // Issue #396 — sandbox enforcement: refuse to follow a directory
        // whose canonical form escapes the rootfs. Without this check
        // an absolute symlink inside the extracted image (e.g.
        // alpine's `var/run -> /run`) silently pulls the host
        // filesystem's content into the SBOM. The canonicalize call
        // resolves the FULL symlink chain on every descent; if it
        // returns an error (broken / dangling symlink, permission
        // denied), the entry is rejected — safer than the fail-open
        // tolerance policy elsewhere in this function because the
        // alternative is leaking host content.
        match std::fs::canonicalize(&path) {
            Ok(canonical_child) => {
                if !canonical_child.starts_with(canonical_rootfs) {
                    tracing::debug!(
                        candidate = %path.display(),
                        canonical = %canonical_child.display(),
                        rootfs = %canonical_rootfs.display(),
                        cause = "rootfs-sandbox-escape",
                        "safe_walk: refusing to follow symlink that escapes the rootfs"
                    );
                    continue;
                }
            }
            Err(_) => {
                tracing::debug!(
                    candidate = %path.display(),
                    cause = "canonicalize-failed",
                    "safe_walk: cannot canonicalize directory; skipping"
                );
                continue;
            }
        }
        walk_inner(&path, rootfs, canonical_rootfs, depth + 1, cfg, visit, visited);
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::exclude_path::ExclusionSet;
    use std::cell::RefCell;

    fn no_skip() -> impl Fn(&Path, &Path) -> bool {
        |_, _| false
    }

    #[test]
    fn bare_minimum_walk_yields_rootfs_once() {
        let dir = tempfile::tempdir().unwrap();
        let empty = ExclusionSet::default();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &empty,
        };
        let calls = RefCell::new(0usize);
        safe_walk(dir.path(), &cfg, |_p| {
            *calls.borrow_mut() += 1;
        });
        assert_eq!(*calls.borrow(), 1, "empty dir visits rootfs once and stops");
    }

    #[test]
    #[cfg(unix)]
    fn canonicalize_keyed_dedup_breaks_symlink_loop() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("loop");
        std::fs::create_dir_all(&sub).unwrap();
        // a/loop/link → a/loop (canonical-equal to a/loop)
        std::os::unix::fs::symlink(&sub, sub.join("link")).unwrap();
        let empty = ExclusionSet::default();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &empty,
        };
        let dir_calls = RefCell::new(0usize);
        safe_walk(dir.path(), &cfg, |p| {
            if p.is_dir() {
                *dir_calls.borrow_mut() += 1;
            }
        });
        // 2 unique canonical dirs: dir.path() + sub. The link
        // resolves to sub (already visited) so it does NOT trigger
        // a third visit.
        assert_eq!(
            *dir_calls.borrow(),
            2,
            "symlink loop must not produce duplicate dir visits"
        );
    }

    #[test]
    fn depth_bound_stops_descent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b/c/d/e")).unwrap();
        let empty = ExclusionSet::default();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 2,
            should_skip: &skip,
            exclude_set: &empty,
        };
        let dir_calls = RefCell::new(0usize);
        safe_walk(dir.path(), &cfg, |p| {
            if p.is_dir() {
                *dir_calls.borrow_mut() += 1;
            }
        });
        // Depth 0 = root; depth 1 = a; depth 2 = a/b. Below
        // max_depth=2 we don't enter a/b's children. So visit gets:
        // root, a, b — 3 dirs.
        assert_eq!(*dir_calls.borrow(), 3, "max_depth=2 must cap descent");
    }

    #[test]
    fn skip_predicate_suppresses_descent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("keep")).unwrap();
        std::fs::create_dir_all(dir.path().join("drop")).unwrap();
        std::fs::write(dir.path().join("keep/file"), b"").unwrap();
        std::fs::write(dir.path().join("drop/file"), b"").unwrap();
        let empty = ExclusionSet::default();
        let skip = |p: &Path, _: &Path| p.file_name().and_then(|s| s.to_str()) == Some("drop");
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &empty,
        };
        let seen: RefCell<Vec<String>> = RefCell::new(Vec::new());
        safe_walk(dir.path(), &cfg, |p| {
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                seen.borrow_mut().push(name.to_string());
            }
        });
        let s = seen.borrow();
        assert!(s.iter().any(|n| n == "keep"));
        assert!(!s.iter().any(|n| n == "drop"));
        assert!(!s.iter().any(|n| n == "file" && s.iter().filter(|x| *x == "file").count() > 1));
    }

    #[test]
    fn skip_predicate_receives_candidate_and_rootfs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("child")).unwrap();
        let rootfs_captured: RefCell<Option<PathBuf>> = RefCell::new(None);
        let candidate_captured: RefCell<Option<PathBuf>> = RefCell::new(None);
        let skip = |candidate: &Path, rootfs: &Path| {
            *candidate_captured.borrow_mut() = Some(candidate.to_path_buf());
            *rootfs_captured.borrow_mut() = Some(rootfs.to_path_buf());
            false
        };
        let empty = ExclusionSet::default();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &empty,
        };
        safe_walk(dir.path(), &cfg, |_| {});
        assert_eq!(rootfs_captured.borrow().as_ref().unwrap(), dir.path());
        assert!(candidate_captured
            .borrow()
            .as_ref()
            .unwrap()
            .ends_with("child"));
    }

    #[test]
    fn empty_exclude_set_short_circuits() {
        // Empty exclude set never invokes ExclusionSet::matches; verify
        // by trying to walk into a dir named e.g. "tests/fixtures" with
        // an empty set — descent proceeds and we see the dir.
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("tests/fixtures/child");
        std::fs::create_dir_all(&nested).unwrap();
        let empty = ExclusionSet::default();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &empty,
        };
        let saw_fixtures = RefCell::new(false);
        safe_walk(dir.path(), &cfg, |p| {
            if p.file_name().and_then(|s| s.to_str()) == Some("fixtures") {
                *saw_fixtures.borrow_mut() = true;
            }
        });
        assert!(*saw_fixtures.borrow());
    }

    #[test]
    fn non_empty_exclude_set_match_suppresses_descent() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        let fixture = dir.path().join("tests/fixtures/something");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(&fixture).unwrap();
        let set = ExclusionSet::from_iter(["tests/fixtures"]).unwrap();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &set,
        };
        let saw_something = RefCell::new(false);
        let saw_real = RefCell::new(false);
        safe_walk(dir.path(), &cfg, |p| {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "something" {
                *saw_something.borrow_mut() = true;
            }
            if name == "real" {
                *saw_real.borrow_mut() = true;
            }
        });
        assert!(*saw_real.borrow(), "real dir must still be visited");
        assert!(
            !*saw_something.borrow(),
            "excluded subtree must not be entered"
        );
    }

    #[test]
    fn files_and_directories_both_visit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("a.txt"), b"").unwrap();
        std::fs::write(dir.path().join("sub/b.txt"), b"").unwrap();
        let empty = ExclusionSet::default();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &empty,
        };
        let files: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let dirs: RefCell<Vec<String>> = RefCell::new(Vec::new());
        safe_walk(dir.path(), &cfg, |p| {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if p.is_file() {
                files.borrow_mut().push(name.to_string());
            } else if p.is_dir() {
                dirs.borrow_mut().push(name.to_string());
            }
        });
        assert!(files.borrow().contains(&"a.txt".to_string()));
        assert!(files.borrow().contains(&"b.txt".to_string()));
        assert!(dirs.borrow().contains(&"sub".to_string()));
    }

    #[test]
    fn unreadable_directory_does_not_abort_peer_descent() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let restricted = dir.path().join("restricted");
            let peer = dir.path().join("peer");
            std::fs::create_dir_all(&restricted).unwrap();
            std::fs::create_dir_all(&peer).unwrap();
            std::fs::write(peer.join("file"), b"").unwrap();
            // Make restricted unreadable.
            std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o000))
                .unwrap();
            let empty = ExclusionSet::default();
            let skip = no_skip();
            let cfg = WalkConfig {
                max_depth: 6,
                should_skip: &skip,
                exclude_set: &empty,
            };
            let saw_peer_file = RefCell::new(false);
            safe_walk(dir.path(), &cfg, |p| {
                if p.file_name().and_then(|s| s.to_str()) == Some("file") {
                    *saw_peer_file.borrow_mut() = true;
                }
            });
            // Restore permissions so tempdir can clean up.
            std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o755))
                .unwrap();
            assert!(
                *saw_peer_file.borrow(),
                "unreadable peer must not abort the walk"
            );
        }
    }

    #[test]
    fn fnmut_callback_with_captured_mut_state_works() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), b"").unwrap();
        std::fs::write(dir.path().join("b"), b"").unwrap();
        let empty = ExclusionSet::default();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &empty,
        };
        let mut collected: Vec<String> = Vec::new();
        safe_walk(dir.path(), &cfg, |p| {
            if p.is_file() {
                collected.push(p.file_name().unwrap().to_string_lossy().into_owned());
            }
        });
        collected.sort();
        assert_eq!(collected, vec!["a", "b"]);
    }

    #[test]
    fn multiple_walks_do_not_share_state() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("child")).unwrap();
        let empty = ExclusionSet::default();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &empty,
        };
        let calls1 = RefCell::new(0usize);
        let calls2 = RefCell::new(0usize);
        safe_walk(dir.path(), &cfg, |_| *calls1.borrow_mut() += 1);
        safe_walk(dir.path(), &cfg, |_| *calls2.borrow_mut() += 1);
        // Second call sees the same tree as the first — visited-set
        // is per-call, not shared across invocations.
        assert_eq!(*calls1.borrow(), *calls2.borrow());
        assert!(*calls1.borrow() >= 2);
    }

    #[test]
    fn exclude_set_uses_path_relative_to_rootfs() {
        // A literal entry "tests/fixtures" must NOT match a candidate
        // at "services/a/tests/fixtures" — the matcher works
        // path-relative-to-rootfs.
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("services/a/tests/fixtures/inside");
        std::fs::create_dir_all(&nested).unwrap();
        let set = ExclusionSet::from_iter(["tests/fixtures"]).unwrap();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 8,
            should_skip: &skip,
            exclude_set: &set,
        };
        let saw_inside = RefCell::new(false);
        safe_walk(dir.path(), &cfg, |p| {
            if p.file_name().and_then(|s| s.to_str()) == Some("inside") {
                *saw_inside.borrow_mut() = true;
            }
        });
        // The nested tests/fixtures is at services/a/tests/fixtures,
        // which is NOT == "tests/fixtures" nor prefixed by it. So
        // descent into "inside" proceeds.
        assert!(*saw_inside.borrow(), "ExclusionSet matches relative paths");
    }

    #[test]
    fn pattern_exclude_set_matches_at_any_depth() {
        // Pattern "**/testdata" anchors at any depth.
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("services/a/testdata/inside");
        std::fs::create_dir_all(&nested).unwrap();
        let set = ExclusionSet::from_iter(["**/testdata"]).unwrap();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 8,
            should_skip: &skip,
            exclude_set: &set,
        };
        let saw_inside = RefCell::new(false);
        safe_walk(dir.path(), &cfg, |p| {
            if p.file_name().and_then(|s| s.to_str()) == Some("inside") {
                *saw_inside.borrow_mut() = true;
            }
        });
        assert!(
            !*saw_inside.borrow(),
            "pattern match suppresses descent into the matched subtree"
        );
    }

    /// Issue #396 — `safe_walk` MUST NOT follow an absolute symlink
    /// that escapes the rootfs sandbox. Reproduces the alpine
    /// `/var/run -> /run` case that pulled the operator's host
    /// filesystem (Nix-managed `/var/run/current-system/sw/bin/...`)
    /// into every image-scan SBOM.
    #[cfg(unix)]
    #[test]
    fn rootfs_sandbox_blocks_absolute_symlink_escape() {
        // Build a tempdir tree:
        //   <tmp>/rootfs/in_sandbox/   (a real dir inside the sandbox)
        //   <tmp>/rootfs/escape   ->   <tmp>/host/sentinel  (an absolute symlink that points outside)
        //   <tmp>/host/sentinel/poison  (a child file the walker MUST NOT visit)
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path().join("rootfs");
        let host_target = tmp.path().join("host/sentinel");
        std::fs::create_dir_all(rootfs.join("in_sandbox")).unwrap();
        std::fs::create_dir_all(&host_target).unwrap();
        std::fs::write(host_target.join("poison"), b"host content").unwrap();
        // Canonicalize the target path so the symlink uses an absolute
        // path the way image rootfs symlinks do (`var/run -> /run`).
        let host_target_canonical = std::fs::canonicalize(&host_target).unwrap();
        std::os::unix::fs::symlink(&host_target_canonical, rootfs.join("escape")).unwrap();

        let set = ExclusionSet::new_empty();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &set,
        };
        let saw_in_sandbox = RefCell::new(false);
        let saw_poison = RefCell::new(false);
        safe_walk(&rootfs, &cfg, |p| {
            if p.file_name().and_then(|s| s.to_str()) == Some("in_sandbox") {
                *saw_in_sandbox.borrow_mut() = true;
            }
            if p.file_name().and_then(|s| s.to_str()) == Some("poison") {
                *saw_poison.borrow_mut() = true;
            }
        });
        assert!(
            *saw_in_sandbox.borrow(),
            "real directory inside the sandbox must still be visited"
        );
        assert!(
            !*saw_poison.borrow(),
            "safe_walk MUST NOT follow an absolute symlink out of the rootfs (issue #396 regression)"
        );
    }

    /// Issue #396 follow-up — relative symlinks that resolve to a
    /// path STILL UNDER the rootfs are followed normally. The escape
    /// guard only fires on absolute / parent-traversal symlinks that
    /// land outside the rootfs canonical prefix.
    #[cfg(unix)]
    #[test]
    fn rootfs_sandbox_allows_in_sandbox_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path().join("rootfs");
        std::fs::create_dir_all(rootfs.join("real")).unwrap();
        std::fs::write(rootfs.join("real/inside-file"), b"inside").unwrap();
        // Relative symlink that stays under the rootfs.
        std::os::unix::fs::symlink("real", rootfs.join("alias")).unwrap();

        let set = ExclusionSet::new_empty();
        let skip = no_skip();
        let cfg = WalkConfig {
            max_depth: 6,
            should_skip: &skip,
            exclude_set: &set,
        };
        let saw_inside = RefCell::new(false);
        safe_walk(&rootfs, &cfg, |p| {
            if p.file_name().and_then(|s| s.to_str()) == Some("inside-file") {
                *saw_inside.borrow_mut() = true;
            }
        });
        assert!(
            *saw_inside.borrow(),
            "in-sandbox relative symlink must be followed normally"
        );
    }
}
