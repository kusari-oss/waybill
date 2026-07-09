#![allow(dead_code)] // lifted by scan_cmd wiring at the bottom of this PR.

//! Milestone 133 US1 — orphan file-tier walker.
//!
//! Drives `safe_walk` over the rootfs, classifies each file via
//! [`super::content_shape::classify`], hashes survivors with
//! streaming SHA-256, dedupes against the [`super::dedupe::DedupeIndex`],
//! and accumulates per-unique-hash [`super::FileTierEntry`] records.
//!
//! **Walker is callable but not yet integrated**: this module is
//! reachable from inside `scan_fs::file_tier` but the production
//! scan pipeline does NOT invoke it yet — that integration ships
//! in US1.B alongside the new `--file-inventory` CLI flag, default
//! flip, multi-format SBOM emission, and the new C-rows.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::Digest;

use super::content_shape::{
    classify, sibling_lockfiles_for, ContentShape, POM_BUILD_OUTPUT_DIR,
};
use super::dedupe::DedupeIndex;
use super::FileTierEntry;

/// Milestone 174 — closed set of VCS metadata directory + file names
/// the file-tier walker skips at descent AND at file-visit time.
///
/// - **Exact base-name match**. `.git`, `.hg`, and `.svn` are excluded;
///   `.github`, `.githooks`, `.gitignore`, `.gitattributes`, `.gitmodules`
///   are NOT excluded (per FR-006 similar-name protection).
/// - **Case-sensitive**. `.GIT/` would not match. No known VCS tool
///   creates upper-case metadata directories; fold-safe comparison
///   would add complexity for zero real-world benefit and would risk
///   false-positive exclusion of unrelated operator content (spec
///   Assumptions #3).
/// - **Closed set**. Adding a fourth name (`.bzr`, `.fslckout`,
///   `_darcs`, `CVS`, `RCS`) requires a follow-up milestone per spec
///   Assumptions.
///
/// This const covers both the directory-form case (`should_skip`
/// closure at `walk_file_tier` line ~104) and the file-form case
/// (git submodule pointer file, checked at the top of the visit
/// callback). One const, two call sites — see [`is_vcs_metadata_name`].
const VCS_METADATA_NAMES: &[&str] = &[".git", ".hg", ".svn"];

/// Returns `true` when `candidate`'s base name exactly matches one of
/// [`VCS_METADATA_NAMES`]. Used by both the `should_skip` closure
/// (directory-descend gate) AND the visit callback (file-form gate
/// for the git-submodule `.git` pointer file case).
///
/// Non-UTF-8 filenames on Unix return `false` (fail-open per
/// Constitution Principle III — a non-UTF-8-named directory is
/// exceedingly unlikely to be VCS metadata since git/hg/svn all
/// create canonical ASCII names).
///
/// Emits a `tracing::debug!` line naming the candidate when returning
/// `true`. FR-009: MUST NOT appear at INFO or higher. Default log
/// level suppresses; `RUST_LOG=debug` surfaces the skip decisions.
///
/// Pure function — no I/O, no allocation, no mutable state. Safe to
/// call before `symlink_metadata` so that a symlink named `.git`
/// pointing at arbitrary content is also skipped without opening it.
fn is_vcs_metadata_name(candidate: &Path) -> bool {
    match candidate.file_name().and_then(|s| s.to_str()) {
        Some(name) if VCS_METADATA_NAMES.contains(&name) => {
            tracing::debug!(
                candidate = %candidate.display(),
                "file-tier walker: skipping VCS metadata"
            );
            true
        }
        _ => false,
    }
}

/// Per-scan configuration for [`walk_file_tier`]. No defaults; all
/// fields supplied by the caller.
pub(crate) struct WalkerConfig<'a> {
    /// Maximum file size (in bytes) to consider for file-tier
    /// emission. Files larger than this are skipped and counted in
    /// [`WalkerStats::oversize_skipped`]. Per FR-010 the production
    /// default is 100 MB; tests pass any value.
    pub size_limit_bytes: u64,
    /// Compiled `**`-pattern exclusion set from
    /// [`super::content_shape::build_orphan_exclusion_globs`]. Built
    /// once per scan by the caller.
    pub exclusion_globs: &'a globset::GlobSet,
    /// FR-011 hybrid dedupe index. Built once per scan after every
    /// reader completes.
    pub dedupe_index: &'a DedupeIndex,
    /// Milestone 133 US1.C: operator-supplied `--exclude-path` set
    /// (milestone 113). The file-tier walker honors the same
    /// exclusions as the package-DB readers so an operator who
    /// suppresses `tests/fixtures/**` doesn't get every test fixture
    /// emitted as file-tier components in its place.
    pub exclude_set: &'a crate::scan_fs::package_db::exclude_path::ExclusionSet,
}

/// Diagnostic skip-counters. Emitted as document-level annotations
/// per Principle X (`mikebom:file-inventory-skipped-oversize` etc.)
/// in US1.B. US1.A returns them but the SBOM emission code does
/// not yet read them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct WalkerStats {
    /// Files skipped because their size exceeded `size_limit_bytes`.
    pub oversize_skipped: usize,
    /// Files skipped because they were special files (devices,
    /// sockets, FIFOs) or otherwise non-regular.
    pub special_skipped: usize,
    /// Files skipped because mikebom couldn't open or read them
    /// (permissions, missing, mid-flight delete).
    pub unreadable_skipped: usize,
    /// Files skipped because the dedupe index reported them
    /// covered (path OR hash match against an already-emitted
    /// component).
    pub dedupe_skipped: usize,
    /// Files skipped because [`classify`] returned `None`
    /// (content shape didn't match the allowlist, OR path was on
    /// the exclusion list).
    pub shape_skipped: usize,
    /// Files hashed AND surviving every filter — converted to
    /// `FileTierEntry` records.
    pub emitted: usize,
}

/// Walk the rootfs, classify + hash + dedupe each file, and return
/// per-unique-hash [`FileTierEntry`] records plus diagnostic stats.
///
/// The returned Vec is sorted by SHA-256 hex for deterministic
/// downstream ordering. Each entry's `paths` Vec is sorted
/// lex-ascending per FR-007. Caller is responsible for the final
/// `FileTierEntry → ResolvedComponent` conversion (US1.B).
pub(crate) fn walk_file_tier(
    rootfs: &Path,
    cfg: &WalkerConfig<'_>,
) -> (Vec<FileTierEntry>, WalkerStats) {
    let mut entries: HashMap<String, FileTierEntry> = HashMap::new();
    let mut stats = WalkerStats::default();

    let walk_cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 32,
        should_skip: &|candidate, _root| is_vcs_metadata_name(candidate),
        exclude_set: cfg.exclude_set,
    };

    crate::scan_fs::walk::safe_walk(rootfs, &walk_cfg, |abs_path| {
        // Milestone 174 FR-002: skip file-form VCS metadata (git
        // submodule pointer file — `.git` FILE contains a `gitdir:`
        // line pointing at the real metadata). MUST run BEFORE
        // symlink_metadata so a symlink named `.git` pointing at
        // arbitrary content is also skipped (per contracts/walker-
        // exclusion.md ordering constraint). Skipped files do NOT
        // increment any counter category per FR-005.
        if is_vcs_metadata_name(abs_path) {
            return;
        }
        // Only files are interesting. `safe_walk` invokes the
        // visit closure for both directories and files; we
        // discriminate here.
        let meta = match std::fs::symlink_metadata(abs_path) {
            Ok(m) => m,
            Err(_) => {
                stats.unreadable_skipped += 1;
                return;
            }
        };
        if meta.is_dir() || meta.file_type().is_symlink() {
            return;
        }
        if !meta.is_file() {
            // Devices, sockets, FIFOs, char/block specials.
            stats.special_skipped += 1;
            return;
        }
        if meta.len() > cfg.size_limit_bytes {
            stats.oversize_skipped += 1;
            return;
        }

        // Build the rootfs-relative path. `safe_walk` hands us the
        // absolute path; strip the rootfs prefix and clear any
        // leading `/`.
        let Ok(rel_abs) = abs_path.strip_prefix(rootfs) else {
            // Not under rootfs (shouldn't happen given safe_walk's
            // contract, but defense-in-depth).
            return;
        };
        let rel_path: PathBuf = rel_abs.to_path_buf();

        // Read first 8 bytes for the magic-number probe. 8 covers
        // every magic we check (ELF=4, PE=2, Mach-O=4, shebang=2).
        let mut head_bytes = [0u8; 8];
        let head_len = match std::fs::File::open(abs_path)
            .and_then(|mut f| f.read(&mut head_bytes))
        {
            Ok(n) => n,
            Err(_) => {
                stats.unreadable_skipped += 1;
                return;
            }
        };
        let head_slice = &head_bytes[..head_len];

        // Adjacent-lockfile probe — pulled into a closure so the
        // classifier stays I/O-free.
        let manifest_name = rel_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let abs_path_owned = abs_path.to_path_buf();
        let lockfile_check = move || lockfile_present_for(&abs_path_owned, manifest_name.as_deref());

        let shape = match classify(&rel_path, head_slice, cfg.exclusion_globs, lockfile_check) {
            Some(s) => s,
            None => {
                stats.shape_skipped += 1;
                return;
            }
        };

        // Stream-hash the file's bytes.
        let sha = match streaming_sha256_hex(abs_path) {
            Some(s) => s,
            None => {
                stats.unreadable_skipped += 1;
                return;
            }
        };

        // FR-011 hybrid dedupe.
        if cfg.dedupe_index.is_covered(&rel_path, &sha) {
            stats.dedupe_skipped += 1;
            return;
        }

        // Accumulate per-unique-hash. Multiple paths with identical
        // content collapse to one entry via `push_path`.
        match entries.get_mut(&sha) {
            Some(existing) => existing.push_path(rel_path),
            None => {
                entries.insert(
                    sha.clone(),
                    FileTierEntry::new(sha, rel_path, shape, meta.len()),
                );
            }
        }
        stats.emitted += 1;
    });

    // Finalize: sort each entry's paths per FR-007, then sort
    // entries by SHA-256 hex for deterministic output ordering.
    let mut out: Vec<FileTierEntry> = entries
        .into_values()
        .map(|mut e| {
            e.finalize();
            e
        })
        .collect();
    out.sort_by(|a, b| a.sha256_hex.cmp(&b.sha256_hex));
    (out, stats)
}

/// FR-005 adjacent-lockfile probe for a candidate lone manifest.
/// Returns `true` when a disqualifying lockfile is present nearby.
///
/// Per FR-005:
/// - `package.json` checks siblings for the npm/yarn/pnpm lockfiles.
/// - `Cargo.toml` walks parents up to 8 levels looking for `Cargo.lock`.
/// - `pom.xml` checks for a sibling `target/` build-output directory.
/// - `requirements.txt` / `Gemfile` / `go.mod` check siblings for
///   their respective lockfiles.
fn lockfile_present_for(abs_manifest: &Path, manifest_filename: Option<&str>) -> bool {
    let Some(name) = manifest_filename else {
        return false;
    };
    let Some(parent) = abs_manifest.parent() else {
        return false;
    };

    // pom.xml: sibling `target/` dir is the signal.
    if name == "pom.xml" {
        return parent.join(POM_BUILD_OUTPUT_DIR).is_dir();
    }

    // Cargo.toml: walk up to 8 ancestors looking for Cargo.lock.
    if name == "Cargo.toml" {
        let mut current: Option<&Path> = Some(parent);
        for _ in 0..=8 {
            let Some(dir) = current else {
                break;
            };
            if dir.join("Cargo.lock").is_file() {
                return true;
            }
            current = dir.parent();
        }
        return false;
    }

    // Every other manifest: sibling lockfile match.
    let siblings = sibling_lockfiles_for(name);
    siblings.iter().any(|lock| parent.join(lock).is_file())
}

/// Stream SHA-256 the file's bytes in 8 KB chunks. Returns
/// lowercase-hex. `None` on any I/O failure.
fn streaming_sha256_hex(abs_path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(abs_path).ok()?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 8 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

// Re-export for test helpers below. Unused-marker silencer:
// `WalkerStats` and `WalkerConfig` are referenced by US1.B
// integration code that hasn't landed yet; the `dead_code` allow
// keeps US1.A's lint gate green without suppressing the warning
// crate-wide.
#[allow(dead_code)]
fn _unused_lints_silencer() {
    let _ = ContentShape::ElfBinary;
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::file_tier::content_shape::build_orphan_exclusion_globs;
    use sha2::Digest;
    use tempfile::TempDir;

    fn empty_dedupe() -> DedupeIndex {
        DedupeIndex::default()
    }

    fn empty_exclude() -> crate::scan_fs::package_db::exclude_path::ExclusionSet {
        crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty()
    }

    fn make_globs() -> globset::GlobSet {
        build_orphan_exclusion_globs()
    }

    fn write_file(dir: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn sha256_of(bytes: &[u8]) -> String {
        let mut h = sha2::Sha256::new();
        h.update(bytes);
        hex_encode(&h.finalize())
    }

    #[test]
    fn empty_rootfs_returns_no_entries() {
        let tmp = TempDir::new().unwrap();
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert!(entries.is_empty());
        assert_eq!(stats.emitted, 0);
    }

    #[test]
    fn elf_binary_emits_one_entry() {
        let tmp = TempDir::new().unwrap();
        let payload = b"\x7FELF\x02\x01\x01\x00rest-of-file";
        write_file(tmp.path(), "opt/custom-tool", payload);
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].shape, ContentShape::ElfBinary);
        assert_eq!(entries[0].sha256_hex, sha256_of(payload));
        assert_eq!(entries[0].paths.len(), 1);
        assert_eq!(entries[0].paths[0], PathBuf::from("opt/custom-tool"));
        assert_eq!(stats.emitted, 1);
    }

    #[test]
    fn source_file_skipped_by_classifier() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "app/src/main.rs", b"fn main() {}");
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert!(entries.is_empty());
        assert_eq!(stats.shape_skipped, 1);
        assert_eq!(stats.emitted, 0);
    }

    #[test]
    fn duplicate_content_at_two_paths_collapses_to_one_entry() {
        let tmp = TempDir::new().unwrap();
        let payload = b"\x7FELF\x02\x01\x01\x00duplicated";
        write_file(tmp.path(), "opt/a", payload);
        write_file(tmp.path(), "opt/b", payload);
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, _stats) = walk_file_tier(tmp.path(), &cfg);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].paths.len(), 2);
        // FR-007: paths sorted lex-ascending.
        assert_eq!(
            entries[0].paths,
            vec![PathBuf::from("opt/a"), PathBuf::from("opt/b")]
        );
    }

    #[test]
    fn dedupe_index_path_match_skips_file() {
        let tmp = TempDir::new().unwrap();
        let payload = b"\x7FELF\x02\x01\x01\x00owned-by-pkg";
        write_file(tmp.path(), "usr/bin/jq", payload);
        let g = make_globs();
        // Forge a DedupeIndex that already claims `usr/bin/jq`.
        use mikebom_common::resolution::{
            FileOccurrence, ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
        };
        use mikebom_common::types::purl::Purl;
        let claim = ResolvedComponent {
            name: "jq".to_string(),
            version: "1.6".to_string(),
            purl: Purl::new("pkg:deb/debian/jq@1.6").unwrap(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 1.0,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![FileOccurrence {
                location: "usr/bin/jq".to_string(),
                sha256: "x".to_string(),
                md5_legacy: None,
                apk_sha1: None,
                rpm_file_digest: None,
            }],
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_range: None,
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: vec![],
            extra_annotations: std::collections::BTreeMap::new(),
            binary_role: None,
        };
        let d = DedupeIndex::build(&[claim]);
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert!(entries.is_empty());
        assert_eq!(stats.dedupe_skipped, 1);
        assert_eq!(stats.emitted, 0);
    }

    #[test]
    fn oversize_file_skipped_with_counter_increment() {
        let tmp = TempDir::new().unwrap();
        // Two ELF magic + 10 bytes; size_limit_bytes=8 is below.
        let payload = b"\x7FELF\x02\x01\x01\x00aaaaaaaaaa";
        write_file(tmp.path(), "opt/big", payload);
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 8,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert!(entries.is_empty());
        assert_eq!(stats.oversize_skipped, 1);
        assert_eq!(stats.emitted, 0);
    }

    #[test]
    fn lone_cargo_toml_emits_when_no_cargo_lock() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "app/Cargo.toml", b"[package]\nname=\"x\"\n");
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].shape, ContentShape::LoneManifest);
        assert_eq!(stats.emitted, 1);
    }

    #[test]
    fn cargo_toml_with_sibling_cargo_lock_is_skipped() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "app/Cargo.toml", b"[package]\nname=\"x\"\n");
        write_file(tmp.path(), "app/Cargo.lock", b"# locked");
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        // Cargo.lock is the locked manifest, ends with .lock → excluded.
        // Cargo.toml has sibling .lock → not lone → skipped.
        assert!(entries.is_empty());
        assert_eq!(stats.shape_skipped, 2);
    }

    #[test]
    fn entries_returned_sorted_by_sha256() {
        let tmp = TempDir::new().unwrap();
        let p1 = b"\x7FELF\x02\x01\x01\x00aaa";
        let p2 = b"\x7FELF\x02\x01\x01\x00bbb";
        write_file(tmp.path(), "opt/a", p1);
        write_file(tmp.path(), "opt/b", p2);
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, _stats) = walk_file_tier(tmp.path(), &cfg);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].sha256_hex < entries[1].sha256_hex);
    }

    // ================================================================
    // Milestone 174 — VCS metadata directory + file exclusion tests.
    //
    // T005 (US1 P1): FR-001 directory-descend gate for `.git`, `.hg`,
    //                `.svn`.
    // T008 (US2 P1): FR-002 file-form `.git` submodule pointer +
    //                FR-006 similar-name protection.
    // T008b (US2):   FR-009 debug-level-only log guarantee.
    // T008c (US2):   FR-007 bare-repo scan completes without panic.
    // ================================================================

    /// T005 (FR-001): descent into `<root>/.git/` is skipped at any
    /// depth. `walk_file_tier` returns an empty entry vec on a rootfs
    /// whose only content is inside `.git/`.
    #[test]
    fn walker_skips_dot_git_directory() {
        let tmp = TempDir::new().unwrap();
        write_file(
            tmp.path(),
            ".git/hooks/pre-commit.sample",
            b"#!/bin/sh\n# sample hook - should not appear in SBOM\n",
        );
        write_file(tmp.path(), ".git/HEAD", b"ref: refs/heads/main\n");
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert!(
            entries.is_empty(),
            "expected zero file-tier entries under .git/; got {:?}",
            entries.iter().map(|e| &e.paths).collect::<Vec<_>>()
        );
        assert_eq!(stats.emitted, 0);
    }

    /// T005 (FR-001 + SC-005): same guarantee for `.hg/`.
    #[test]
    fn walker_skips_dot_hg_directory() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), ".hg/store/data/foo.i", b"mercurial internals");
        write_file(tmp.path(), ".hg/dirstate", b"whatever");
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert!(entries.is_empty(), "expected zero entries under .hg/");
        assert_eq!(stats.emitted, 0);
    }

    /// T005 (FR-001 + SC-005): same guarantee for `.svn/`.
    #[test]
    fn walker_skips_dot_svn_directory() {
        let tmp = TempDir::new().unwrap();
        write_file(
            tmp.path(),
            ".svn/pristine/aa/aabbccdd.svn-base",
            b"pristine base",
        );
        write_file(tmp.path(), ".svn/wc.db", b"sqlite? whatever");
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, stats) = walk_file_tier(tmp.path(), &cfg);
        assert!(entries.is_empty(), "expected zero entries under .svn/");
        assert_eq!(stats.emitted, 0);
    }

    /// T008 (FR-002): file-form `.git` (git submodule pointer) is
    /// skipped. The visit callback's file-form check catches this
    /// case; the directory-descend gate can't (it only fires on
    /// directories).
    #[test]
    fn walker_skips_dot_git_submodule_file() {
        let tmp = TempDir::new().unwrap();
        // Submodule root has a `.git` FILE (not directory) containing
        // a `gitdir:` pointer. Git's canonical shape.
        write_file(
            tmp.path(),
            "submodule/.git",
            b"gitdir: ../.git/modules/submodule\n",
        );
        // A first-party file inside the same submodule dir survives.
        write_file(
            tmp.path(),
            "submodule/README.sh",
            b"#!/bin/sh\necho hello\n",
        );
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, _stats) = walk_file_tier(tmp.path(), &cfg);
        // No entry represents `.git` (file-form).
        let has_dot_git_file = entries.iter().any(|e| {
            e.paths
                .iter()
                .any(|p| p.file_name().and_then(|n| n.to_str()) == Some(".git"))
        });
        assert!(
            !has_dot_git_file,
            "FR-002: expected no entry for file-form `.git`; got entries={:?}",
            entries.iter().map(|e| &e.paths).collect::<Vec<_>>()
        );
    }

    /// T008 (FR-006): similar-name protection. `.github`, `.githooks`,
    /// `.gitignore`, `.gitattributes`, `.gitmodules` are NOT VCS
    /// metadata — exact base-name match protects them from exclusion.
    /// These files still go through the content-shape classifier —
    /// some may still be dropped by that filter, but the m174 walker
    /// gate does NOT drop them.
    ///
    /// Verified by: walking a tempdir with the 5 similar-name files
    /// AND a trivially-classifiable `.sh` script. Assert the walker
    /// returns a non-empty entry set including the `.sh` script,
    /// confirming the walker did descend into (not skip) directories
    /// like `.github/workflows/`. Also assert no entry paths start
    /// with `.git/` (which would mean the .git-form exclusion
    /// accidentally fired on the .github/ path).
    #[test]
    fn walker_preserves_similar_names() {
        let tmp = TempDir::new().unwrap();
        // The 5 similar-name files.
        write_file(
            tmp.path(),
            ".github/workflows/ci.yml",
            b"name: CI\non: push\njobs: {}\n",
        );
        write_file(
            tmp.path(),
            ".githooks/pre-commit",
            b"#!/bin/sh\necho custom hook\n",
        );
        write_file(tmp.path(), ".gitignore", b"target/\n*.log\n");
        write_file(tmp.path(), ".gitattributes", b"*.rs eol=lf\n");
        write_file(
            tmp.path(),
            ".gitmodules",
            b"[submodule \"foo\"]\n\tpath = foo\n\turl = ../foo\n",
        );
        // A trivially-classifiable script inside `.githooks/` proves
        // the walker DID descend into similar-name directories.
        write_file(
            tmp.path(),
            ".githooks/deploy.sh",
            b"#!/bin/bash\necho deploying\n",
        );
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        let (entries, _stats) = walk_file_tier(tmp.path(), &cfg);
        // The walker DID descend into `.githooks/` (proven by the
        // presence of at least one entry). The classifier decides
        // final emission; the m174 gate does not intercept.
        let has_similar_name_content = entries.iter().any(|e| {
            e.paths.iter().any(|p| {
                let s = p.to_string_lossy();
                s.starts_with(".githooks/")
                    || s.starts_with(".github/")
                    || s == ".gitignore"
                    || s == ".gitattributes"
                    || s == ".gitmodules"
            })
        });
        assert!(
            has_similar_name_content,
            "FR-006: expected at least one similar-name file to reach the walker; \
             got entries={:?}",
            entries.iter().map(|e| &e.paths).collect::<Vec<_>>()
        );
        // No entry path should start with `.git/` (the exact-name
        // match must not accidentally consume `.github/`).
        let dot_git_paths: Vec<&std::path::PathBuf> = entries
            .iter()
            .flat_map(|e| e.paths.iter())
            .filter(|p| p.to_string_lossy().starts_with(".git/"))
            .collect();
        assert!(
            dot_git_paths.is_empty(),
            "FR-006: exact-name match should not match `.github/*` etc.; \
             got .git/ paths={dot_git_paths:?}"
        );
    }

    /// T008b (FR-009): the VCS-skip log MUST NOT fire at INFO or
    /// higher. Structural guarantee (the helper uses `tracing::debug!`
    /// which cannot produce INFO-level output at the macro level) —
    /// this is a belt-and-braces gate against a future change that
    /// accidentally upgrades the log level.
    #[test]
    fn walker_vcs_skip_does_not_emit_info_log() {
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        // In-memory capture writer.
        #[derive(Clone, Default)]
        struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for CaptureWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> MakeWriter<'a> for CaptureWriter {
            type Writer = CaptureWriter;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer = CaptureWriter(captured.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(writer)
            .finish();

        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), ".git/HEAD", b"ref: refs/heads/main\n");
        write_file(
            tmp.path(),
            ".git/hooks/pre-commit.sample",
            b"template\n",
        );

        // Run the walker under the INFO-max subscriber.
        tracing::subscriber::with_default(subscriber, || {
            let g = make_globs();
            let d = empty_dedupe();
            let cfg = WalkerConfig {
                size_limit_bytes: 100 * 1024 * 1024,
                exclusion_globs: &g,
                dedupe_index: &d,
                exclude_set: &empty_exclude(),
            };
            let (_entries, _stats) = walk_file_tier(tmp.path(), &cfg);
        });

        let output = captured.lock().unwrap();
        let output_str = String::from_utf8_lossy(&output);
        assert!(
            !output_str.contains("skipping VCS metadata"),
            "FR-009: VCS-skip message MUST NOT appear at INFO level; \
             captured stderr contained the substring: {output_str}"
        );
    }

    /// T008c (FR-007 post-remediation): scanning a bare-repo shape
    /// (top-level HEAD + refs/ + config, no `.git/` subdirectory)
    /// completes without panicking. This test documents the deliberate
    /// scope limit — m174's exclusion is scoped to descendants NAMED
    /// `.git`/`.hg`/`.svn`, not to bare-repo internal-layout detection.
    /// The tool MAY still emit file-tier components for readable text
    /// files at the bare repo's own root; a follow-up milestone MAY
    /// add bare-repo detection if operator demand surfaces.
    #[test]
    fn walker_bare_repo_completes_successfully() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "HEAD", b"ref: refs/heads/main\n");
        write_file(
            tmp.path(),
            "refs/heads/main",
            b"0123456789abcdef0123456789abcdef01234567\n",
        );
        write_file(
            tmp.path(),
            "config",
            b"[core]\n\tbare = true\n\trepositoryformatversion = 0\n",
        );
        write_file(tmp.path(), "objects/pack/.gitkeep", b"");
        let g = make_globs();
        let d = empty_dedupe();
        let cfg = WalkerConfig {
            size_limit_bytes: 100 * 1024 * 1024,
            exclusion_globs: &g,
            dedupe_index: &d,
            exclude_set: &empty_exclude(),
        };
        // The call returns without panic. Component count is NOT
        // asserted — per FR-007 post-remediation, m174 does NOT
        // guarantee zero components on bare-repo scans (the exclusion
        // gates on directory base names being one of `.git`/`.hg`/
        // `.svn`, and a bare repo's own root is not named that).
        let (_entries, _stats) = walk_file_tier(tmp.path(), &cfg);
    }
}
