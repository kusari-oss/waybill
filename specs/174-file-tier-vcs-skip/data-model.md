# Phase 1 Data Model: File-tier VCS metadata exclusion (m174)

**Feature**: 174-file-tier-vcs-skip
**Date**: 2026-07-08

Two entities. No new types. No new fields. No new enum variants. This is a bug fix, not a feature — the data model changes reflect that.

## Entity 1 — `VCS_METADATA_NAMES` const

**Location**: `mikebom-cli/src/scan_fs/file_tier/walker.rs`, module-scope (at the top, near existing imports).

**Shape**:

```rust
/// FR-001 / FR-006 closed set of VCS metadata directory + file names.
/// Exact base-name match, case-sensitive (Assumptions #3 fold-unsafe on
/// case-insensitive filesystems is deliberate — no VCS tool creates
/// upper-case metadata names).
///
/// Adding a fourth name (`.bzr`, `.fslckout`, `_darcs`, `CVS`, `RCS`)
/// requires a follow-up milestone per spec Assumptions.
const VCS_METADATA_NAMES: &[&str] = &[".git", ".hg", ".svn"];
```

**Invariants**:
- Closed set — no runtime mutation, no env-var override.
- Lower-case (case-sensitive match — see Edge Cases).
- Three entries — future additions require a spec-level milestone.

## Entity 2 — `is_vcs_metadata_name` helper function

**Location**: same file, adjacent to the `VCS_METADATA_NAMES` const.

**Shape**:

```rust
/// Returns `true` when `candidate`'s base name is exactly `.git`,
/// `.hg`, or `.svn`. Used by both the `should_skip` closure passed to
/// `safe_walk` (directory-descend gate) AND the visit callback
/// (file-form gate for the git-submodule `.git` pointer file case).
///
/// Non-UTF-8 filenames on Unix return `false` (fail-open per
/// Constitution Principle III — a non-UTF-8-named directory is
/// exceedingly unlikely to be VCS metadata; git/hg/svn all create
/// canonical ASCII names).
///
/// Emits a `tracing::debug!` line naming the candidate when returning
/// `true`. Default log level suppresses; `RUST_LOG=debug` surfaces the
/// skip decisions for troubleshooting.
fn is_vcs_metadata_name(candidate: &Path) -> bool {
    match candidate.file_name().and_then(|s| s.to_str()) {
        Some(name) if VCS_METADATA_NAMES.iter().any(|&n| n == name) => {
            tracing::debug!(
                candidate = %candidate.display(),
                "file-tier walker: skipping VCS metadata"
            );
            true
        }
        _ => false,
    }
}
```

**Contract**:
- **Pure function** — no I/O, no mutable state, no allocation (the `to_str()` returns a `&str` borrowing from the OsStr; the comparison is byte-equal via `str::eq`).
- **Safe to call before `symlink_metadata`** — never touches the filesystem.
- **Idempotent** — called twice with the same input returns the same value.
- **Thread-safe** — the const is a `'static` value; no lock needed.

## Entity 3 — modified `WalkConfig` construction site

**Location**: `walker.rs:92-96` — a single site.

**Pre-174 shape**:

```rust
let walk_cfg = crate::scan_fs::walk::WalkConfig {
    max_depth: 32,
    should_skip: &|_candidate, _root| false,
    exclude_set: cfg.exclude_set,
};
```

**Post-174 shape**:

```rust
let walk_cfg = crate::scan_fs::walk::WalkConfig {
    max_depth: 32,
    should_skip: &|candidate, _root| is_vcs_metadata_name(candidate),
    exclude_set: cfg.exclude_set,
};
```

Delta: 1 line change to the closure body. `should_skip`'s type contract at `walk.rs:127` is preserved verbatim.

## Entity 4 — modified visit-callback file-form check

**Location**: `walker.rs:98-190` (the visit callback body). Insert BEFORE `symlink_metadata` at line 102.

**Pre-174 shape** (top of visit callback):

```rust
crate::scan_fs::walk::safe_walk(rootfs, &walk_cfg, |abs_path| {
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
    // ... rest of visit callback
});
```

**Post-174 shape** (added file-form check):

```rust
crate::scan_fs::walk::safe_walk(rootfs, &walk_cfg, |abs_path| {
    // Milestone 174 FR-002: skip file-form VCS metadata (git
    // submodule pointer file). MUST run BEFORE symlink_metadata
    // so a symlink named `.git` is also skipped.
    if is_vcs_metadata_name(abs_path) {
        return;
    }
    // Only files are interesting. `safe_walk` invokes the
    // visit closure for both directories and files; we
    // discriminate here.
    let meta = match std::fs::symlink_metadata(abs_path) {
        // ... unchanged
```

Delta: 6 lines added (3 code, 3 comment). No existing lines modified.

**Note on counter semantics**: the added early-return skips ALL of the existing stats counters (`unreadable_skipped`, `shape_skipped`, `special_skipped`, `oversize_skipped`, `dedupe_skipped`, `emitted`). Per FR-005, no new counter category is added in this milestone. If future tooling needs to distinguish "VCS-skipped" from "unclassified-skipped", that's a follow-up.

## Cross-entity invariants (post-174)

1. **VCS_METADATA_NAMES membership is authoritative**: any name in the const IS excluded; no name outside it IS. FR-006 (protection for `.github`, `.githooks`, `.gitignore`, `.gitattributes`, etc.) is structurally guaranteed by the exact-name-match — no glob semantics involved.
2. **Directory-form and file-form use identical predicate**: the same `is_vcs_metadata_name(candidate)` runs in both `should_skip` (directory gate) and the visit callback (file gate). One helper, two call sites. If a future milestone adds a fourth VCS name, both gates get it automatically.
3. **No filesystem I/O in the exclusion path**: `is_vcs_metadata_name` reads only the filename bytes already in the `Path`; the file/directory content is never opened, matching FR-008 spirit ("never opens the subtree at all") extended to files.
4. **No SBOM wire-shape change**: emitted components are strictly a subset of pre-174 emitted components. All existing components either (a) survive unchanged, or (b) disappear because they were `.git/hooks/*.sample`-shaped noise.

## State transitions

None. This is a stateless walker-logic change. The exclusion decision is a pure function of the candidate path.
