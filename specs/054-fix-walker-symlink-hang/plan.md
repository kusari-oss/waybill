# Implementation Plan: Fix filesystem-walker symlink-loop hang + realistic-project regression suite

**Branch**: `054-fix-walker-symlink-hang` | **Date**: 2026-05-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/054-fix-walker-symlink-hang/spec.md`

## Summary

Two unprotected filesystem walkers (`rpm_file::walk_dir`, `binary::discover::walk_dir`) follow symlinks blindly with no visited-set or depth limit. On any input that contains a symlink loop (knative/func ships ~10 such loops as test fixtures in `pkg/oci/testdata/test-links/`), they recurse forever. Additionally, ~4 walkers have depth-limit-only protection — sufficient against the worst-case hang, but still O(2^depth) on cyclic inputs.

Fix in two parts:

1. **Audit + harden every walker** to have a canonicalize-keyed visited-set (mandatory) plus a max-depth backstop (defense-in-depth). Per-walker patches; full migration to a single `safe_walk` helper deferred to issue #108.
2. **Add a realistic-project CI regression job** that scans real-world OSS projects (knative/func + 1-2 others) at fixed git tags via live `git clone --depth 1 --branch <tag>` per CI run (with `actions/cache@v4` keyed by `<project>:<tag>`). Schema-validates the resulting SBOM. Catches the next "fairly basic issue" before merge, not after release.

Closes the user-reported knative/func hang. Unblocks the alpha.10 release (PR #107 paused pending this milestone per the user's instruction).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–053; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `std::fs::canonicalize`, `std::collections::HashSet`, `PathBuf`. **No new crates.** Per spec assumption: not pulling `walkdir` or `ignore` crates — std-only is the design intent matching the existing minimal-dependency Cargo.toml posture.
**Storage**: N/A — visited-set is per-walker-invocation in-memory state, cleared between scans.
**Testing**: `cargo +stable test --workspace`. New synthesized symlink-loop fixtures via `tempfile::tempdir() + std::os::unix::fs::symlink`. New realistic-project CI job clones knative/func at `knative-v1.22.0` per run.
**Target Platform**: Linux + macOS (matching existing CI lanes — linux-x86_64, linux-x86_64-ebpf, macos-latest). Windows: out of scope per existing project posture.
**Project Type**: CLI (workspace-rooted; bug fix touches `mikebom-cli/src/scan_fs/` walkers + adds a new GitHub Actions workflow file).
**Performance Goals**: SC-001 ≤60s for knative/func; SC-002 ≤5s for minimal symlink-loop fixture; SC-005 ≤15% variance on existing 9-ecosystem fixtures (no regression).
**Constraints**: `std::fs::canonicalize` adds a stat + readlink syscall per directory entry. For typical projects (≤100k files), aggregate cost is amortized via dedup (each unique canonical dir canonicalize'd once). Pathological inputs (millions of files) revisit during implementation if SC-005 fires.
**Scale/Scope**: 9 walkers in `mikebom-cli/src/scan_fs/` to audit. 2 require visited-set + depth-limit (zero protection today). 4 require visited-set added on top of existing depth-limit. 2 already have the canonicalize-keyed visited-set per `golang.rs::walk_for_go_roots` and `project_roots.rs::walk_for_project_roots` patterns — confirm and document via inline comment.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Per `.specify/memory/constitution.md` v1.4.0:

| Principle | Status | Notes |
|-----------|--------|-------|
| **I. Pure Rust, Zero C** | ✅ Pass | All changes in pure Rust; no C dependency. |
| **II. eBPF-Only Observation** | ✅ Pass | This is enrichment-of-observation infrastructure (the filesystem walkers feed package-DB readers, not eBPF discovery). No discovery-surface change. |
| **III. Fail Closed** | ✅ Pass | When the visited-set detects a cycle, the walker breaks the cycle and continues — this is bounded-completeness, not silent failure. The user gets a complete SBOM modulo the cycle's redundant traversal, not an aborted scan. The `tracing::debug!` breadcrumb on cycle-detection (FR-008) makes the behavior observable. |
| **IV. Type-Driven Correctness** | ✅ Pass | New code uses `HashSet<PathBuf>` + the existing `Path` newtype. Production code uses `Result` for `canonicalize` (which can fail on broken symlinks — handled via `unwrap_or_else(|_| candidate.to_path_buf())` matching the existing `golang.rs::walk_for_go_roots:1163` pattern). `.unwrap()` only inside `#[cfg(test)]` modules guarded by `#[cfg_attr(test, allow(clippy::unwrap_used))]`. |
| **V. Specification Compliance** | ✅ Pass | No new `mikebom:*` annotations or relationship types — this is a correctness fix to the scan pipeline, not an SBOM-output-shape change. The Principle V audit clause does not apply (no new field being introduced). |
| **VI. Three-Crate Architecture** | ✅ Pass | All changes within `mikebom-cli/`. No new crates. |
| **VII. Test Isolation** | ✅ Pass | New unit tests use `tempfile::tempdir()` + `std::os::unix::fs::symlink` — no elevated privileges, no eBPF involvement. The realistic-project CI job runs unprivileged `git clone` + scan. |
| **VIII. Completeness** | ✅ Pass | This feature *prevents* false negatives caused by silent walker termination. Pre-054 a hung walker meant ZERO components emitted (no SBOM at all); post-054 every directory the walker can reach is enumerated exactly once. Net: completeness improves. |
| **IX. Accuracy** | ✅ Pass | Visited-set dedup ensures the walker yields each canonical directory exactly once, regardless of how many symlinks point to it. No phantom doubled emissions; no dropped components. |
| **X. Transparency** | ✅ Pass | Cycle detection emits a `tracing::debug!` breadcrumb (FR-008) naming the canonical path so future bug reports have visible evidence of the cycle. Default-log scans of legitimate trees emit zero loop-detection chatter. |
| **XI. Enrichment** | ✅ Pass | No enrichment-source changes. |
| **XII. External Data Source Enrichment** | ✅ Pass | The realistic-project CI job clones upstream OSS at fixed tags — this is test fixture acquisition, not external-source enrichment of an SBOM at scan time. Constitution XII applies to runtime enrichment (deps.dev, lockfiles), not test fixtures. |
| **Strict Boundary #1 (No lockfile-based discovery)** | ✅ Pass | No discovery-source change. |
| **Pre-PR Verification (mandatory)** | ✅ Pass | T-final task in Phase 6 runs `./scripts/pre-pr.sh` per the constitution's Development Workflow table. |

**Gate result**: Pass. No constitution violations; no Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/054-fix-walker-symlink-hang/
├── plan.md              # This file
├── spec.md              # Feature specification (Q1 + Q2 clarifications recorded)
├── research.md          # Phase 0 output (audit findings + canonicalize rationale)
├── data-model.md        # Phase 1 output: VisitedPathSet, RealisticProjectFixture entities
├── quickstart.md        # Phase 1 output: 3-step verification recipe
├── contracts/
│   └── walker-protection.md   # Phase 1: per-walker contract (mandatory visited-set + depth-limit)
├── checklists/
│   └── requirements.md  # Spec-quality checklist (all items pass)
└── tasks.md             # Phase 2 output (created by /speckit.tasks)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       ├── package_db/
│       │   ├── rpm_file.rs       # ⬅️ MAIN HANG — add visited-set + depth-limit (currently neither)
│       │   ├── cargo.rs          # ⬅️ HARDEN — add visited-set on top of existing depth-limit
│       │   ├── gem.rs            # ⬅️ HARDEN — add visited-set to walk_for_gemfile_locks +
│       │   │                     #     walk_for_gemspecs (currently depth-limit only)
│       │   ├── go_binary.rs      # ⬅️ HARDEN — add visited-set to walk_for_binaries
│       │   ├── golang.rs         # (verify only — already has canonicalize-keyed visited-set
│       │   │                     #  via walk_for_go_roots; add inline comment naming the
│       │   │                     #  protection mechanism per FR-001 audit requirement)
│       │   ├── maven.rs          # ⬅️ HARDEN — add visited-set to walk_for_maven (depth-limit only)
│       │   └── project_roots.rs  # (verify only — already has the cleanest visited-set
│       │                         #  implementation; reference target for follow-up issue #108)
│       └── binary/
│           └── discover.rs       # ⬅️ MAIN HANG #2 — add visited-set + depth-limit
│                                 #     (currently neither, same shape as rpm_file::walk_dir)

tests/
└── fixtures/
    └── walker-symlink-loops/     # ⬅️ NEW — synthesized minimal symlink-loop fixtures
                                  #     for unit + integration tests. Tarball-style
                                  #     (no .git) so determinism is preserved.

.github/workflows/
└── realistic-projects.yml        # ⬅️ NEW — separate workflow file (per FR-010) for the
                                  #     clone + scan + schema-validate job. Triggered on PR
                                  #     + push-to-main. Runs in parallel with the existing
                                  #     ci.yml lanes; doesn't block them on its own flakes.

docs/
└── design-notes.md               # ⬅️ DOC UPDATE — new section: "Filesystem walking pattern."
                                  #     Documents the per-walker visited-set + depth-limit
                                  #     contract; points at follow-up issue #108 for the
                                  #     eventual single-helper migration.

CHANGELOG.md                      # ⬅️ DOC UPDATE — `[Unreleased]` → `### Fixed` entry:
                                  #     "Fix filesystem-walker symlink-loop hang on
                                  #     real-world projects (knative/func)."
```

**Structure Decision**: Single-crate (`mikebom-cli`) feature touching ~6 walker files in `scan_fs/package_db/` + 1 in `scan_fs/binary/`. Adds 1 new GitHub Actions workflow file (`realistic-projects.yml`) per FR-010 (separate workflow so its flakes don't block the main pre-PR gate). Adds 1 new fixture directory for unit-test consumption. No new crates, no new top-level modules, no API surface change at the CLI layer. The migration to a single shared `safe_walk` helper (issue #108) is OUT of scope; per-walker patches keep blast radius small ahead of the alpha.10 release.

## Phase 0: Outline & Research

`research.md` consolidates audit findings + rationale for the canonicalize-based design. Generated this run.

Key decisions recorded:

- **Decision**: per-walker visited-set hardening (vs. shared `safe_walk` helper). **Rationale**: Q1 clarification chose Option B; minimizes blast radius before alpha.10. **Alternatives**: Option A (depth-limit-only is sufficient) rejected per Q1 — leaves O(2^depth) explosion vector; Option C (full migration to shared helper) deferred to #108.
- **Decision**: `std::fs::canonicalize` + `HashSet<PathBuf>` for the visited-set keying. **Rationale**: matches the existing pattern in `golang.rs::walk_for_go_roots:1163` and `project_roots.rs::walk_for_project_roots:51`; std-only; sub-millisecond per call; deduplicates by on-disk identity (multiple symlinks → same canonical path → one entry). **Alternatives**: keying by inode pair (faster on Linux, but POSIX-only and adds platform-specific code paths) rejected; keying by `path.to_path_buf()` directly (cheaper but doesn't dedup symlink-equivalent paths) rejected.
- **Decision**: max-depth = 16 across all walkers. **Rationale**: deeper than any realistic monorepo's natural nesting (typical: 6-10); surfaces pathological inputs without false-positives. Defense-in-depth backstop for the visited-set primary mechanism. **Alternatives**: 32 rejected (overkill); 8 rejected (some legitimate Rust workspaces with `target/<profile>/<package>/<dep>/<...>` go deeper); per-walker tuning rejected (uniform value simpler).
- **Decision**: live `git clone --depth 1 --branch <tag>` per CI run for realistic-project fixtures, with `actions/cache@v4` keyed by `<project>:<tag>`. **Rationale**: Q2 clarification chose Option A; smallest code change; pinned-tag is source-of-truth for content. **Alternatives**: pre-built tarballs in repo (B), GitHub release artifacts (C), git submodules (D) — all rejected per the Q2 trade-off analysis.
- **Decision**: knative/func @ `knative-v1.22.0` is the headline fixture. **Rationale**: user's literal repro command; ~15 MB; ships 10+ symlink loops; multi-module Go layout; small enough to clone in CI. Adding 1-2 more fixtures from other ecosystems happens in Phase 2 task generation; the headline pass focuses on the user's reported case.
- **Decision**: separate workflow file `realistic-projects.yml` (vs. extending existing `ci.yml`). **Rationale**: FR-010 — flake isolation. The new job's network-dependent clone + per-platform multipliers shouldn't gate the main pre-PR validation. Re-runnable in isolation when a flake bites. **Alternatives**: extending `ci.yml` rejected — couples the new lane's flakiness to the existing 3-lane gate.

No NEEDS CLARIFICATION markers in Technical Context. Phase 0 complete.

## Phase 1: Design & Contracts

### 1. Data model

`data-model.md` (this run) — captures:

- **`VisitedPathSet`**: a `HashSet<PathBuf>` keyed by `std::fs::canonicalize` output, scoped to a single walker invocation. Insert returns `bool` indicating "newly seen vs. already visited." On `canonicalize` failure (broken symlink, EACCES on a parent component), the walker MUST fall back to `path.to_path_buf()` as the key — preserves dedup correctness on the happy path while not blocking the walker on transient lookup failures. Pattern matches `golang.rs:1163`.
- **`MAX_WALK_DEPTH`** const: `usize = 16`. Defined per-walker (not as a workspace const) to keep each walker's protections self-contained — supports issue #108's eventual extraction without forcing every walker to import the same const before the migration.
- **`RealisticProjectFixture`** (CI-only, not Rust): a per-project record consisting of `(name, upstream_url, tag, expected_min_components, schema_to_validate)`. Drives the matrix in the new `.github/workflows/realistic-projects.yml`. Initially: knative/func + 1-2 others picked during task generation. The `expected_min_components` is a smoke-test gate: if a regression makes the SBOM emit fewer components than the floor, the CI job fails clearly.

### 2. Contracts

`contracts/walker-protection.md` (this run) — captures the per-walker protection contract:

- **Mandatory invariants**: every `fn walk*` in `mikebom-cli/src/scan_fs/` MUST (a) maintain a canonicalize-keyed visited-set across the entire walk; (b) honor a max-depth bound (default 16); (c) emit `tracing::debug!` on cycle detection naming the canonical path; (d) return successfully on broken-symlink targets (preserve existing behavior); (e) return successfully on EACCES (preserve existing behavior — `let Ok(entries) = read_dir else { return; }` pattern).
- **Audit rubric**: PR-review time check: `grep -rn "fn walk" mikebom-cli/src/scan_fs/` MUST find every match either delegating to the post-054 protected pattern OR carrying an inline comment justifying the deviation (e.g., "bounded-by-construction: only iterates a finite, hardcoded list").
- **Realistic-project CI contract**: the new workflow MUST clone at a pinned tag, scan with `--offline --no-deep-hash`, schema-validate the resulting SBOM, and fail with the regressing project's name + scan duration on any deviation. Per-project budget: 5 min linux / 10 min macos. `actions/cache@v4` keyed by `<project>:<tag>` to avoid re-cloning across CI runs against the same tag.

### 3. Quickstart

`quickstart.md` (this run) — gives reviewers a 3-step verification recipe:

1. Clone knative/func at `knative-v1.22.0`.
2. Run `mikebom sbom scan --path ./func --format spdx-2.3-json --output out.json --no-deep-hash --offline`.
3. **Expect**: scan completes within 60s with exit 0; output validates against the SPDX 2.3 schema; emits ≥ 200 `pkg:golang` components.

### 4. Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after this plan is committed — adds milestone 054 entry to `CLAUDE.md`'s Active Technologies list. No new technologies (no new crates, no new languages) — the script will record "054-fix-walker-symlink-hang: Existing only".

### 5. Re-evaluate Constitution Check

Re-checked above table after Phase 1 design — no new violations introduced. The `tracing::debug!` cycle-detection breadcrumb (FR-008) actively *strengthens* Principle X (Transparency) compliance. The new realistic-project CI job actively strengthens Principle VIII (Completeness) — regressions that silently degrade a real-world scan's edge count get caught at PR-review time.

**Phase 1 outputs**: `data-model.md`, `contracts/walker-protection.md`, `quickstart.md`, agent-context update. All feed into `/speckit.tasks` next.

## Complexity Tracking

*No constitution violations to justify. Section intentionally empty.*
