# Feature Specification: Fix filesystem-walker symlink-loop hang + realistic-project regression suite

**Feature Branch**: `054-fix-walker-symlink-hang`
**Created**: 2026-05-02
**Status**: Draft
**Input**: User description: "knative/func hangs indefinitely on `mikebom sbom scan --path ./func`. 100% CPU spin, no output, killed after 10 min. Reproduced. We really need a reasonably working SBOM tool here, and I'm growing concerned about all these fairly basic issues. Again, we should follow standards and be testing against realistic [projects]."

## Clarifications

### Session 2026-05-02

- Q: Should depth-limit-only walkers (without a visited-set) count as protected under FR-001(b), or do they need to be hardened to visited-set protection too? → A: **Hardened audit (Option B)**. Every walker MUST have a canonicalize-keyed visited-set; depth-limit alone is insufficient. Adds a visited-set to ~4 existing depth-limited walkers in addition to fixing the 2 unprotected ones. Full migration to a single shared `safe_walk` helper (Option C) is deferred to follow-up issue #108 — milestone 054 keeps per-walker implementations to minimize blast radius before alpha.10 ships.
- Q: How should the realistic-project CI job acquire its fixtures (FR-006, FR-007)? → A: **Live `git clone --depth 1 --branch <tag>` per CI run (Option A)**. GitHub Actions caches between runs via `actions/cache@v4` keyed by `<project>:<tag>`. Smallest code change; determinism comes from the pinned tag itself. Network blips on github.com are rare and a CI rerun fixes them — the trade-off vs. checking in tarballs (15-30 MB repo bloat per project, manual regen on tag bumps) favors the live-clone approach.

## Investigation findings (recorded here so the spec is grounded, not aspirational)

The user's diagnosis ("hang during Go source-file import analysis, O(n²) in import → module resolution") was based on the last log line emitted before the hang. In practice the issue is downstream of Go scanning entirely:

- `tracing::info` "parsed Go source tree" emits at ~1s with `modules=418, production_imports=68, test_only_imports=8, main_modules=9`. Go scanning has FINISHED by this point.
- `sample <pid> 3` against the hung process shows 100% of stack samples in `mikebom::scan_fs::package_db::rpm_file::walk_dir` recursing through itself ~10+ levels deep, never returning.
- Knative/func ships `pkg/oci/testdata/test-links/` with intentional symlink loops as test fixtures: `linkToRoot -> .` (self-loop), `b/linkToRoot -> ..` (parent loop), `b/linkToRootsParent -> ../..` (grandparent loop), `b/linkOutsideRootsParent -> ../../..`, `b/c/linkToParent`. Plus the `templates/` tree has `f` symlinks across 5 scaffolding subdirs.
- `rpm_file::walk_dir` (`mikebom-cli/src/scan_fs/package_db/rpm_file.rs:147`) follows symlinks via `path.is_dir()` (which dereferences symlinks) with NO depth limit, NO visited-path set, NO symlink-loop detection. This is the root cause.
- Same shape in `binary/discover.rs::walk_dir` (line 24) — second instance of the same bug pattern.
- Other walkers in the codebase (cargo, gem, go_binary, golang, maven, project_roots) have varying degrees of protection: some have depth limits (6-10), some have canonicalize-keyed visited sets, some have neither. The codebase has no shared, audited walker abstraction.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - mikebom completes a scan against a realistic Go project containing symlink loops (Priority: P1)

A developer or CI pipeline runs `mikebom sbom scan --path ./<project>` against any Go project that ships test-fixture symlink loops (knative/func, kubernetes/kubernetes, helm/chart-testing, and many others — `git grep -r "linkToRoot" .` finds dozens). The scan completes in bounded wall-clock time and produces a valid SBOM. mikebom never hangs at 100% CPU regardless of the filesystem topology under the scan root.

**Why this priority**: This is a basic correctness bug that makes mikebom effectively unusable on a meaningful subset of real-world projects. The user reported "growing concerned about all these fairly basic issues" — this is the headline that needs to clear before any new release. A scanner that hangs forever on legitimate input is not a working SBOM tool.

**Independent Test**: Clone any project with a known symlink loop (or use a constructed fixture: a directory containing `link -> .`), run `mikebom sbom scan --path <project> --offline --format spdx-2.3-json --output out.json --no-deep-hash`, and verify the scan completes within 60 seconds with exit 0. Independently delivers the headline value (no hang).

**Acceptance Scenarios**:

1. **Given** the knative/func v1.22.0 fixture (which contains 5+ intentional symlink loops in `pkg/oci/testdata/test-links/` plus 5 `f` symlinks in `templates/`), **When** `mikebom sbom scan --path <fixture> --offline --no-deep-hash` runs, **Then** the scan completes within 60 seconds with exit 0 and emits a valid SBOM.
2. **Given** a synthesized minimal symlink-loop fixture (`tmpdir/loop/link -> .`), **When** mikebom scans it, **Then** the scan completes within 5 seconds (the loop is recognized and the walker doesn't descend through the cycle indefinitely).
3. **Given** a deep but acyclic symlink chain (`tmpdir/a -> b -> c -> d` ending at a real file), **When** mikebom scans it, **Then** the scan completes within 5 seconds and the file at the end of the chain is correctly identified.
4. **Given** a symlink that points outside the scan root (`tmpdir/escape -> /etc`), **When** mikebom scans it, **Then** the walker MAY follow it (existing behavior — the user explicitly invoked mikebom with that root) but MUST NOT recurse into the escapee's children indefinitely; canonicalize-keyed visited-set bounds the walk regardless.

---

### User Story 2 - Realistic-project regression suite (Priority: P2)

A maintainer working on mikebom (any future change) gets fast, automated feedback when their change regresses scan behavior on a real-world project. Today, mikebom's CI exercises hand-rolled minimal fixtures (single-file go.mod's, dpkg-status stubs, etc.) but does NOT scan any non-trivial open-source project end-to-end. Two consecutive milestones (053, this one) shipped with bugs that a realistic-project regression test would have caught immediately.

**Why this priority**: Without this, the next "fairly basic issue" is statistically inevitable. The user explicitly called this out: "we should follow standards and be testing against realistic [projects]." This story converts that concern into a concrete CI gate so future regressions surface in PR review, not in user-reported field bug reports.

**Independent Test**: Add a new CI job (or extension of an existing one) that clones 1-3 small-but-realistic open-source projects (knative/func, plus 1-2 from other ecosystems), scans each with mikebom in all three formats, and asserts the scan completes within a per-project wall-clock budget AND the resulting SBOMs validate against the SPDX schema. Independently testable: run the new CI job on a freshly-cloned repo; either the bug surfaces (job fails) or it doesn't (job passes within budget).

**Acceptance Scenarios**:

1. **Given** the new realistic-project CI job, **When** a PR introduces a regression that causes any of the project scans to hang, panic, or produce an invalid SBOM, **Then** the CI job fails within its bounded budget (e.g., 5 minutes per project) and reports which project regressed.
2. **Given** the new realistic-project CI job, **When** a PR makes no relevant change, **Then** the job passes within its bounded budget on every supported runner (linux-x86_64, macos-latest).
3. **Given** the user's original repro (`git clone --depth 1 --branch knative-v1.22.0 https://github.com/knative/func.git && mikebom sbom scan --path ./func ...`), **When** the post-054 binary runs against this exact command, **Then** the scan completes within 60 seconds and emits a valid SPDX 2.3 SBOM.

---

### User Story 3 - Audit + harden every walker (Priority: P3)

A maintainer reviewing mikebom for the same class of bug across all filesystem walkers gets a centralized, audited walker abstraction (or an equivalent shared discipline) that makes "did you remember to handle symlink loops?" a property of the codebase, not a per-walker question.

**Why this priority**: Two walkers (`rpm_file::walk_dir`, `binary::discover::walk_dir`) currently have zero loop protection. Several others have depth limits but no visited-set. A future contributor adding a new ecosystem reader has no obvious reference for "the right way to walk." Centralizing this prevents a third instance of this bug.

**Independent Test**: After implementation, every filesystem-walking function in `mikebom-cli/src/scan_fs/` either uses the shared safe-walk abstraction OR has a documented opt-out with a justification. A `cargo deny` or grep-based audit script confirms zero `walk_dir`-style functions outside the shared abstraction lack the required protections.

**Acceptance Scenarios**:

1. **Given** the post-054 codebase, **When** a developer writes `grep -rn "fn walk" mikebom-cli/src/scan_fs/`, **Then** every match either delegates to the shared `safe_walk` (or equivalent) helper OR has an explicit comment naming why it's a special case (e.g., bounded-depth + acyclic-by-construction subtree).
2. **Given** a developer writes a new ecosystem reader, **When** they need to walk the filesystem, **Then** they reach for the shared helper as the obvious path-of-least-resistance, without inventing a fourth `walk_dir` variant.

---

### Edge Cases

- **Symlink loop entirely within the scan root** (knative/func's `pkg/oci/testdata/test-links/linkToRoot -> .`): the walker visits the directory once, recognizes the `.` resolves to a path it already canonicalized, and skips re-descending. Doesn't fail the scan.
- **Symlink loop that escapes the scan root** (`b/linkToRootsParent -> ../..`, `b/linkOutsideRootsParent -> ../../..`): canonicalize the target and check against the visited set. The escapee's canonical path may be outside the scan root entirely (e.g., points at the user's `$HOME`), in which case it's still bounded by the visited set + a configurable max-walk-depth.
- **Broken symlink (target doesn't exist)**: skipped silently. `path.is_file()` and `path.is_dir()` return false on broken symlinks today; behavior preserved.
- **Symlink cycle of length > 1** (`a -> b -> c -> a`): canonicalize-keyed visited set detects on second visit; walker breaks the cycle.
- **Hard links**: not symlinks; walker treats them as regular files. No change in behavior.
- **Permission-denied subdirectory**: existing behavior preserved (`read_dir` returns an Err which the `let Ok(entries) = ... else { return; }` swallows). The walker doesn't crash on EACCES.
- **Realistic project that scans correctly today** (cargo, npm, maven projects with no symlink loops): scan times must NOT regress. The fix is additive (loop detection); the happy path is unchanged in observable behavior.
- **Test fixture with a deliberately-symlinked directory used for actual testing** (e.g., a project's `tests/` dir mirroring another tree via symlinks): canonicalize logic must NOT silently double-count files reached via two paths. Visited-set keyed by canonical path means the file is processed exactly once, not zero, not twice.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every filesystem walker in `mikebom-cli/src/scan_fs/` MUST detect symlink loops and bound recursion via a **canonicalize-keyed visited-path set** (mandatory) plus a **configurable max-depth** (defense-in-depth backstop). Per the Q1 clarification (Hardened audit, Option B): depth-limit alone is INSUFFICIENT — every walker MUST have a visited-set. The audit covers `rpm_file::walk_dir` and `binary::discover::walk_dir` (currently zero protection), plus the ~4 walkers in cargo / gem / go_binary / maven (currently depth-limit-only — must add visited-set). Full migration to a single shared `safe_walk` helper is deferred to follow-up issue #108; milestone 054 keeps per-walker implementations.

- **FR-002**: A walker MUST NOT visit the same on-disk directory more than once, regardless of how many symlinks point to it. Detection: when `entry.path()` is a directory, canonicalize it (via `std::fs::canonicalize`) before deciding to descend; if the canonical path is already in the visited set, skip.

- **FR-003**: Every walker MUST honor a max-walk-depth ceiling. **Default: 16** (deeper than any realistic monorepo's natural nesting; surfaces pathological inputs without false-positive on legitimate deep trees). Per-walker tighter bounds (e.g., `cargo.rs::MAX_PROJECT_ROOT_DEPTH = 6`) are PERMITTED, but ONLY when carrying an inline justification comment naming the specific structural reason the tighter bound holds (e.g., `// cargo workspaces are shallow by convention; 6 covers any realistic layout`). The audit grep at SC-003 verifies every per-walker const < 16 has the comment. The max-depth is a defense-in-depth backstop, not the primary loop-prevention mechanism (which is FR-002).

- **FR-004**: The shared safe-walk abstraction (or equivalent shared discipline) MUST be the path-of-least-resistance for new ecosystem readers. Either a callable `pub fn safe_walk(...)` in a single module, OR a documented walker-construction pattern that future code review can enforce via grep audit. The abstraction's signature and rationale MUST be documented in `docs/design-notes.md` so future contributors find it without reading existing readers.

- **FR-005**: The fix MUST NOT regress scan times on the existing fixture suite. CI pre-PR gate enforces no measurable slowdown on the existing 9-ecosystem fixtures (acceptable variance: ±15% per fixture, motivated by macOS-latest runner-contention noise per `tests/dual_format_perf.rs`).

- **FR-006**: A new CI job (or extension of an existing one) MUST scan **knative/func @ `knative-v1.22.0`** (the user's reported repro case). The scan MUST complete within 5 minutes on linux / 10 minutes on macos with offline + no-deep-hash flags, AND the resulting SBOM MUST validate against its target schema. The job MUST fail clearly with the project name + scan duration when a regression is introduced. **Per-ecosystem expansion** (one realistic project per cargo / npm / maven / pip / gem / dpkg / apk / rpm + polyglot scans) is out of scope for milestone 054 and is tracked in follow-up issue #109 — milestone 054 focuses on closing the user's reported case while leaving the matrix structurally extensible for later additions.

- **FR-007**: The realistic-project CI job MUST acquire its fixtures via live `git clone --depth 1 --branch <tag> <upstream-url>` per CI run (per Q2 clarification, Option A). Tag updates require an explicit PR; the pinned tag is the source-of-truth for fixture content. The job MUST use `actions/cache@v4` keyed by `<project>:<tag>` to avoid re-cloning across CI runs against the same tag set. Network failures during clone are tolerated via standard CI rerun; the job MUST NOT silently degrade to skipping a fixture on clone failure (skipping would mask real regressions).

- **FR-008**: When a walker detects a symlink loop, it MUST emit a `tracing::debug!` (NOT info or warn) breadcrumb naming the loop's canonical path. Useful for debugging future reports; not noisy in default-log scans of legitimate trees.

- **FR-009**: The synthesized minimal symlink-loop fixture from US1 AS#2 (`tmpdir/loop/link -> .`) MUST be added as a unit test under each affected walker's `#[cfg(test)] mod tests`. Verifies the loop-protection works at the unit level, independent of the realistic-project CI job.

- **FR-010**: The realistic-project CI job MUST be a separate workflow file (or job within an existing workflow) that can be re-run independently when investigating a flake without re-running the entire pre-PR gate. macOS-latest is currently flake-prone for performance-bounded tests (see `tests/dual_format_perf.rs` history); the new job's per-project budget MUST account for this with a documented per-platform multiplier (e.g., 1.0× linux, 2.0× macos).

### Key Entities

- **Safe-walk abstraction**: a shared function (or pattern) that takes a root path + per-walker filter callback and returns matching paths. Internal state: canonicalize-keyed visited set, max-depth counter, optional skip-list of directory names. Replaces the ad-hoc `walk_dir` functions in `rpm_file.rs` and `binary/discover.rs` outright; existing protected walkers (cargo, gem, etc.) MAY adopt it incrementally.
- **Realistic-project fixture clone**: a git-clone-at-tag step in CI (or a checked-in tarball, depending on size constraints) that produces a known-frozen filesystem topology against which mikebom is exercised end-to-end. NOT bundled in `tests/fixtures/` (size); cloned fresh per CI run from a frozen tag.
- **Visited-path set**: a `HashSet<PathBuf>` keyed by `std::fs::canonicalize` output, scoped to a single walker invocation. Cleared between scans (no cross-scan persistence).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Reproducing the user's exact original case (`git clone --depth 1 --branch knative-v1.22.0 https://github.com/knative/func.git && mikebom sbom scan --path ./func --format spdx-2.3-json --output out.json`) MUST complete in under 60 seconds with exit 0 on both linux-x86_64 and macos-latest CI runners.

- **SC-002**: A synthesized minimal symlink-loop fixture (`tmpdir/loop/link -> .`) scans in under 5 seconds, with the loop-detection logic exercised exactly once (visited-set hit count ≥ 1).

- **SC-003**: Every filesystem walker in `mikebom-cli/src/scan_fs/` either uses the shared safe-walk abstraction OR carries an inline comment naming its loop-protection mechanism (canonicalize-keyed visited-set, depth limit, acyclic-by-construction subtree). A `grep -rn "fn walk_" mikebom-cli/src/scan_fs/` audit at PR-review time finds zero unannotated, unprotected walkers.

- **SC-004**: The new realistic-project CI job runs ≤ 5 min per project on linux-x86_64, ≤ 10 min on macos-latest, and validates the resulting SBOM against the SPDX 2.3 / 3.0.1 / CDX 1.6 schemas using the existing schema-validator tooling (`tests/spdx3_schema_validation.rs` and friends).

- **SC-005**: Existing 9-ecosystem fixture scans show NO regression (≤ 15% variance, accounting for runner noise) in wall-clock time after the fix. Measured via the existing `tests/dual_format_perf.rs` hot-path or an equivalent quick benchmark.

- **SC-006**: The pre-054 baseline behavior (hang on knative/func at 100% CPU after ~1s of normal scan output) is documented in the spec's investigation findings AND explicitly verified-as-fixed by the SC-001 acceptance test.

- **SC-007**: SBOM consumers running mikebom against any of the new realistic-project fixtures get a non-empty, schema-valid SBOM. Specifically: the post-054 SBOM for knative/func contains ≥ 200 `pkg:golang` components (mirroring its real go.sum closure) with at least one `DEPENDS_ON` edge from the main-module per milestone 053 FR-001.

## Assumptions

- **Hang root-cause is symlink-loop, not Go-import-resolution**: The user's report attributed the hang to Go import analysis. Stack-sample evidence (recorded in the Investigation findings section above) shows the hang is in `rpm_file::walk_dir`. The spec scope reflects the actual root cause; if a separate Go import-resolution perf issue exists, it's a follow-up not addressed here.
- **canonicalize is fast enough**: `std::fs::canonicalize` does a stat + readlink chain; for typical project trees (≤ 100k files) the per-call cost is sub-millisecond and the aggregate cost is amortized by deduplication (only called once per unique directory). If this assumption breaks on pathological inputs (e.g., a tree with millions of files), revisit during implementation.
- **No new crate**: the standard library has `std::fs::canonicalize` + `HashSet` + `PathBuf`. The shared safe-walk abstraction can be implemented entirely in std without pulling `walkdir` or `ignore` crates as dependencies. This matches the existing `mikebom-cli/Cargo.toml` minimal-dependency posture.
- **knative/func is a representative-enough fixture**: its 425 `.go` files + 20 `go.mod` files + 5+ symlink loops + ~15 MB total exercise the failure shape without being too big to clone in CI. If knative/func ever drops the test-links fixture, swap to another project that ships symlink loops (kubernetes, helm, etc.).
- **macOS-latest noise budget**: the new CI job's per-project budget assumes the same 2× macOS multiplier already documented for `dual_format_perf`. If macOS runners get faster (or slower) the multiplier is adjusted in a follow-up PR, not silenced.
- **Out-of-scope**: rewriting walkers that ALREADY have protection (cargo, gem, golang, maven, project_roots) is NOT mandatory if their existing protections satisfy FR-001's "(a) shared OR (b) per-walker visited-set + depth-limit" requirement. The audit task at FR-001 will catalog them; ones with adequate protection get an inline comment naming their mechanism, not a rewrite.
- **Out-of-scope: full bug-class audit**: this milestone fixes the specific hang AND adds the realistic-project regression suite. It does NOT do a full bug-class audit (e.g., enumerate every input that could plausibly hang mikebom for unrelated reasons — large-binary scanning, infinite recursion in archive readers, etc.). Future milestones tackle those if they surface; this one closes the symlink-loop class.
