# Implementation Plan: Exclude VCS metadata directories from the file-tier walker

**Branch**: `174-file-tier-vcs-skip` | **Date**: 2026-07-08 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/174-file-tier-vcs-skip/spec.md`

## Summary

**Primary requirement**: the m133 file-tier walker at `mikebom-cli/src/scan_fs/file_tier/walker.rs:94` currently passes `should_skip: &|_candidate, _root| false` to `safe_walk`, which means NO directories are skipped at walker-level. As a result `.git/hooks/*.sample` files (git's default hook templates that populate every cloned repo) leak into emitted SBOMs as `pkg:generic/file-tier?content-sha256=...` components. Per-ecosystem walkers already exclude `.git`+`.hg`+`.svn` (verified across 10 readers: dart, cocoapods, composer, erlang, haskell, scala, rpm_file, ipk_file, etc.); the m133 file-tier walker missed the treatment. Two audits (langflow, test-tensorflow-models) surfaced the bug empirically.

**Technical approach**: single-line change to the `should_skip` closure at `walker.rs:94`. Replace the always-false closure with an exact-name check against a module-local `const VCS_METADATA_NAMES: &[&str] = &[".git", ".hg", ".svn"]`. The `safe_walk` contract at `walk.rs:127` already documents that `should_skip` returns `true` to suppress descent — perfect fit for the FR-008 requirement that the walker "never opens `.git/`, `.hg/`, or `.svn/` subtrees at all." Plus the FR-002 file-form case: the callback at `walker.rs:98-190` also needs to skip individual FILES named `.git` (submodule pointer file); one additional `if` check inside the visit callback covers it.

Five surgical changes:

1. **Add `const VCS_METADATA_NAMES: &[&str] = &[".git", ".hg", ".svn"];`** at the top of `walker.rs` (module scope) with a doc comment naming the FR-001 / FR-006 closed-set semantics + the "exact base-name match, case-sensitive" contract.

2. **Add helper `fn is_vcs_metadata_name(candidate: &Path) -> bool`** in `walker.rs` that returns `true` when `candidate.file_name().and_then(|s| s.to_str())` matches any name in `VCS_METADATA_NAMES` exactly (byte-equal, no case-fold). Used by BOTH the `should_skip` closure (directory-descent gate) AND the visit callback (file-emission gate).

3. **Replace `should_skip: &|_candidate, _root| false`** at `walker.rs:94` with `should_skip: &|candidate, _root| is_vcs_metadata_name(candidate)`. This kills directory-descent into `.git/`, `.hg/`, `.svn/` at any depth — FR-001 + FR-008 satisfied.

4. **Add file-form check inside the visit callback** at `walker.rs:98` (before `symlink_metadata`). If `is_vcs_metadata_name(abs_path)` returns `true`, return early (also skip counter incrementing). Covers FR-002 (git-submodule `.git` FILE case). Symlink-safe: `symlink_metadata` runs after the check, so a symlink named `.git` pointing to something malicious is also skipped.

5. **Debug-level trace log per FR-009** — `tracing::debug!(candidate = %candidate.display(), "file-tier walker: skipping VCS metadata")` inside `is_vcs_metadata_name` when returning `true`. INFO+ MUST NOT fire per FR-009. Emits at debug level (`tracing::debug!`) so operators with `RUST_LOG=debug` can see the skip decisions; default log level suppresses.

**Tests**: 5 unit tests in `walker.rs::tests` (added inline; the existing `#[cfg(test)] mod tests` block already exists at `walker.rs:285`):
1. `walker_skips_dot_git_directory` — construct a tempdir with `<root>/.git/hooks/pre-commit.sample`; assert zero file-tier entries emitted.
2. `walker_skips_dot_hg_directory` — same shape with `.hg/`.
3. `walker_skips_dot_svn_directory` — same shape with `.svn/`.
4. `walker_skips_dot_git_submodule_file` — tempdir with `<root>/.git` (FILE, not directory) containing `gitdir: ../.git/modules/foo`; assert zero file-tier entries.
5. `walker_preserves_similar_names` — tempdir with `<root>/.github/workflows/ci.yml` + `<root>/.githooks/pre-commit` + `<root>/.gitignore` — assert all three files ARE emitted (the exact-name-match protects them per FR-006).

Plus 1 integration test: `mikebom-cli/tests/file_tier_vcs_skip.rs` that scans a synthesized 3-file repo (`.git/hooks/pre-commit.sample` + `ci.sh` + `.gitignore`) via the release binary, asserts the emitted CDX SBOM contains a component for `ci.sh` AND a component for `.gitignore` AND NO component whose `mikebom:source-files` value contains any path starting with `.git/`.

**Docs**: none required — internal walker behavior change. The reading-guide's file-tier subsection (m133) does not currently mention `.git`, so nothing to update there. If we wanted operator-facing docs, that's a separate polish task; the fix itself is source-only.

**Golden regeneration**: NONE. Verified by inspection — no existing golden fixture at `mikebom-cli/tests/fixtures/golden/` contains a `.git/` subtree (goldens are byte-identity SBOMs from synthesized fixtures, not from real git-cloned repos). SC-003 is trivially satisfied.

**Blast radius**: ~30 lines added in `walker.rs` (const + helper + closure change + visit-callback check + trace log) + ~80 lines of unit tests + ~50 lines of integration test. Total < 200 lines. No new Cargo dependencies. No new CLI flags. No new annotations. No new fields. No golden changes. Extremely low-risk surgical fix.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–173; no nightly required for this user-space-only bug fix).

**Primary Dependencies**: Existing only — `std::path::Path` + `std::path::PathBuf` (base-name extraction), `tracing` (debug-level skip logs), existing `scan_fs::walk::safe_walk` (unchanged), existing `scan_fs::walk::WalkConfig` (unchanged). **Zero new Cargo dependencies.** Zero new subprocess calls. Zero new network access. Zero new filesystem writes.

**Storage**: N/A — pure walker logic change; no persistence.

**Testing**: `cargo test` — 5 new unit tests in the existing `walker.rs::tests` block + 1 new integration test at `mikebom-cli/tests/file_tier_vcs_skip.rs`. Existing byte-identity golden regression suite (33 fixtures across CDX / SPDX 2.3 / SPDX 3) MUST remain unchanged per SC-003; verified by inspection (no golden fixture contains a `.git/` subtree).

**Target Platform**: All hosts mikebom builds on — Linux (CI), macOS (dev), Windows (m100-experimental). No host-specific code paths touched. Case-sensitive exact-name comparison per Assumptions is deliberately fold-unsafe on case-insensitive filesystems (HFS+, NTFS default), documented behavior.

**Project Type**: cli (mikebom sbom-generation CLI).

**Performance Goals**: SC-004 signals a ≥25% wall-clock improvement for repos with heavy `.git/objects/pack/` subtrees; not a hard bound. Primary performance benefit: `safe_walk` never opens `.git/` at all, saving both `readdir` calls AND `open()` on pack files.

**Constraints**: SC-003 byte-identity gate — every existing non-VCS-containing golden fixture MUST remain byte-identical. Enforced by the existing regression suite; no golden regeneration needed since no golden currently exercises a `.git/` scenario.

**Scale/Scope**: Extremely small. 1 source file edited (`walker.rs`), 1 new integration test file, 0 docs changes, 0 golden updates.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No new Cargo dependencies. Pure Rust addition (const + helper fn + closure change).
- **II. eBPF-Only Observation**: ✅ `mikebom-ebpf` untouched.
- **III. Fail Closed**: ✅ The exclusion is a positive rule (skip specific directories); no failure mode introduced. `is_vcs_metadata_name` returns `false` for any name it can't decode (non-UTF-8 filenames on Unix), which is the fail-open direction — the walker treats the directory as normal content. This is correct — a non-UTF-8-named directory is exceedingly unlikely to be VCS metadata (git/hg/svn all create canonical ASCII names).
- **IV. Type-Driven Correctness**: ✅ Closed 3-name set encoded as a `&'static [&'static str]` const. Exact-match comparison via `str::eq`. No new types introduced; behavior expressible as a small const table.
- **V. Specification Compliance**: ✅ No new `mikebom:*` annotation, property, or relationship type. Zero SBOM wire-shape changes. The fix REMOVES spurious components from emitted SBOMs — the wire contract for the removed components was `pkg:generic/file-tier?content-sha256=...` which is standards-compliant, but their semantic meaning (git hook templates) never belonged in the SBOM. Post-174 the SBOM contains FEWER components; every removed component was noise, never signal.
- **VI. Three-Crate Architecture**: ✅ Change contained to `mikebom-cli` (walker + tests). No `mikebom-common` or `mikebom-ebpf` changes.
- **VII. Test Isolation**: ✅ All new unit + integration tests use per-test `tempfile::tempdir()`; no shared state.
- **VIII. Completeness**: ✅ **Improved**. The removed components were negative signal (noise); their absence doesn't reduce Completeness. In fact, Completeness IMPROVES because the ratio of signal-to-noise in emitted SBOMs rises.
- **IX. Accuracy**: ✅ **Improved**. The removed components had zero attribution value (git hooks aren't part of the operator's application, its dependencies, or its build output). Removing them makes the SBOM more accurately reflect what the scanned repository IS.
- **X. Transparency**: ✅ Debug-level trace logs (FR-009) allow operators to see the skip decisions when they need to. Default log level is quiet — this is unremarkable operator-invisible plumbing, appropriate for a built-in exclusion.
- **XI. Enrichment**: N/A — no enrichment path touched.
- **XII. External Data Source Enrichment**: N/A.

**Strict Boundaries check**:
- **New subprocess**: ✅ None.
- **New network access**: ✅ None.
- **New filesystem writes**: ✅ None. The exclusion PREVENTS reads of `.git/objects/` — strictly negative in the I/O direction.
- **New `mikebom:*` annotation namespaces**: ✅ None.
- **New Cargo dependencies**: ✅ Zero.
- **Strict Boundary §5 (file-tier no-duplicates in default mode)**: ✅ Preserved — the exclusion REMOVES components; it never introduces duplicates.

**Verdict**: All principles pass. Zero violations. Milestone improves Principles VIII/IX/X by increasing the signal-to-noise ratio of emitted SBOMs.

## Project Structure

### Documentation (this feature)

```text
specs/174-file-tier-vcs-skip/
├── plan.md              # This file
├── research.md          # Phase 0 — walker touchpoint audit + per-ecosystem exclusion cross-reference
├── data-model.md        # Phase 1 — VCS_METADATA_NAMES const + is_vcs_metadata_name helper contract
├── quickstart.md        # Phase 1 — 3-scenario manual verification recipe
├── contracts/           # Phase 1 — walker exclusion contract + safe_walk should_skip interaction
├── checklists/          # Requirements checklist (spec-phase output)
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── file_tier/
│           └── walker.rs                  # +30 lines: const + helper + closure change + visit-cb check + 5 unit tests
└── tests/
    └── file_tier_vcs_skip.rs              # NEW ~50 lines: 3-file synthesized-fixture integration test
```

**Structure Decision**: single-file source change plus one integration test file. No new modules, no new CLI flags, no docs updates. The fix lives inside the m133 walker's existing structure — a `const` at module scope + a helper function + a modified closure. The scope is deliberately contained per Assumptions #4 (do not centralize to `scan_fs::walk::safe_walk` — leave each walker's exclusion policy in its own hands so per-ecosystem readers can express ecosystem-specific rules independently).

## Complexity Tracking

No constitution violations to justify. The plan is a surgical 1-source-file bug fix that removes noise from emitted SBOMs.
