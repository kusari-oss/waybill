# Phase 0 Research: File-tier VCS metadata exclusion (m174)

**Feature**: 174-file-tier-vcs-skip
**Date**: 2026-07-08

Four research questions resolved to unblock the surgical fix. Every question was already answered by prior code inspection during spec authorship; this file consolidates the answers so plan.md and tasks.md have single-point references.

---

## R1 — Where exactly in the walker does the fix inject?

**Decision**: at the `should_skip` closure passed to `safe_walk` at `mikebom-cli/src/scan_fs/file_tier/walker.rs:94`.

**Verified**: current source at that line reads:
```rust
let walk_cfg = crate::scan_fs::walk::WalkConfig {
    max_depth: 32,
    should_skip: &|_candidate, _root| false,   // <-- HERE
    exclude_set: cfg.exclude_set,
};
```

The `safe_walk` contract at `mikebom-cli/src/scan_fs/walk.rs:127` documents:
> `pub should_skip: &'a dyn Fn(&Path, &Path) -> bool`
>
> Predicate consulted before descent into each child directory.
> `(candidate, rootfs)` lets the closure extract the candidate's filename
> via `.file_name()` AND compute the candidate's path relative to the
> scan root. Returning `true` suppresses descent.

This is a perfect fit for FR-001 (exclude descent into `.git/`, `.hg/`, `.svn/`) + FR-008 (never open the subtree). The closure runs at pre-descend time, so `readdir()` on `.git/objects/pack/` never fires.

**Alternatives considered**:
- **Centralize in `scan_fs::walk::safe_walk` itself**: rejected per spec Assumptions #4 — per-ecosystem readers have divergent exclusion policies (cocoapods excludes `.git`+`.svn`+`.hg`+`Pods`+`node_modules`+`build`+`DerivedData`; dart just `.git`; the file-tier walker needs the 3-name set) and centralizing would require encoding all their permutations. Surgical per-walker fix wins.
- **Post-hoc filter at emission time**: rejected — the walker would still open every `.git/objects/pack/*.pack` file, defeating SC-004 (perf win). Pre-descend skip is strictly better.

---

## R2 — How do per-ecosystem walkers already exclude `.git`?

**Decision**: they don't share a mechanism; each walker inlines its own name check. Verified via `grep -rn '"\\.git"' mikebom-cli/src/scan_fs/`:

| Walker | Names excluded | Location |
|---|---|---|
| `dart.rs` | `.git` (implicit — via `is_vendor_dir`) | line 59 |
| `cocoapods.rs` | `.git`, `.svn`, `.hg`, `Pods`, `node_modules`, `build`, `DerivedData` | lines 54, 59 |
| `composer.rs` | `.git`, `.svn`, `.hg`, `vendor`, `node_modules` | lines 64, 72 |
| `erlang.rs` | `.git` | line 124 |
| `haskell.rs` | `.git` | line 122 |
| `scala.rs` | `.git` | line 87 |
| `rpm_file.rs` | `.git`, `target`, `node_modules`, `.cargo`, `__pycache__`, `.venv` | line 259 |
| `ipk_file.rs` | same as rpm_file | line 284 |

None share a common helper. The file-tier walker adding its own `is_vcs_metadata_name()` helper is consistent with the existing pattern (each walker owns its exclusion policy). Future refactor to centralize could happen in a separate cleanup milestone.

**Alternatives considered**:
- **Extract to a shared `is_vcs_metadata_name()` in `scan_fs::walk`**: rejected per Assumptions #4 — same argument as R1. The file-tier walker's 3-name set is intentional; per-ecosystem walkers add ecosystem-specific names on top. A shared helper would need every walker to opt in, which is a separate cross-cutting change beyond m174's scope.

---

## R3 — How does the file-form `.git` submodule pointer case work?

**Decision**: add a file-form check inside the visit callback at `walker.rs:98`, BEFORE `symlink_metadata` is called.

**Verified**: git submodules use a `.git` FILE (not directory) at the submodule's root containing a single line like `gitdir: ../.git/modules/<name>`. This is spec'd by git — `git submodule` creates this pointer file. Every submodule inside a repo has one at its own root.

Under the pre-174 walker, this file would:
1. Pass the `should_skip` closure (it's a file, not a directory being descended into)
2. Enter the visit callback
3. Have `symlink_metadata` called (returns file metadata, not directory)
4. Skip the directory check at line 109
5. Reach the content-shape classifier
6. Might be `shape_skipped` OR emit as a file-tier component (depends on content-shape rules for tiny text files)

Post-174 the file-form check at the top of the visit callback returns early before any of steps 3-6 fire, matching FR-002.

**Ordering**: the file-form check must run BEFORE `symlink_metadata` so a SYMLINK named `.git` (e.g., a malicious symlink to `/etc/passwd`) is also skipped. `is_vcs_metadata_name()` only inspects the filename — never touches the filesystem — so it's safe to call first.

**Alternatives considered**:
- **Skip only in `should_skip`**: rejected — the closure only fires at directory-descend time, not at file-visit time. The `.git` FILE would slip through.
- **Add to content-shape classifier**: rejected — that would suppress emission but still incur the file-open + first-8-bytes read + hash cost. FR-008 says "never opens the subtree at all"; extending that spirit to files too, we skip before any I/O.

---

## R4 — Are any existing golden fixtures affected by this change?

**Decision**: NO — verified via `find mikebom-cli/tests/fixtures/golden -type d -name ".git"` returning empty.

**Verified**: mikebom's golden fixtures are synthesized directory trees (rpm/deb/apk databases, cargo/npm/pip lockfile projects, Go modules, etc.) — none contain a `.git/` subtree because the fixtures are BUILT, not CLONED. The scan-time exclusion of `.git/` has zero effect on any golden regeneration.

SC-003 (byte-identity of all ~30 golden fixtures pre-174 vs post-174) is trivially satisfied. The existing `cargo test -p mikebom --test cdx_regression` / `spdx_regression` / `spdx3_regression` suites gate this at CI time.

**Alternatives considered**:
- **Add a new golden fixture WITH `.git/`**: rejected as unnecessary. The unit tests in `walker.rs::tests` already cover the exclusion behavior at the walker level; the integration test at `tests/file_tier_vcs_skip.rs` covers end-to-end. A dedicated golden would duplicate coverage without adding signal.

---

## Summary table

| ID | Question | Decision |
|---|---|---|
| R1 | Where does the fix inject? | `should_skip` closure at `walker.rs:94` (directory-descend gate) + visit-callback file-form check |
| R2 | How do per-ecosystem walkers exclude `.git`? | Each inlines its own check; no shared helper. File-tier walker follows the same per-walker-owns-policy pattern. |
| R3 | How does the file-form `.git` submodule pointer case work? | File-form check inside visit callback, BEFORE `symlink_metadata`, so symlink form is also skipped. |
| R4 | Are golden fixtures affected? | No — no golden contains a `.git/` subtree. SC-003 trivially satisfied. |
