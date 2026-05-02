# Research: filesystem-walker symlink-loop hang fix

**Created**: 2026-05-02
**Purpose**: Audit findings + design rationale for the per-walker hardening + realistic-project regression suite. Resolves all NEEDS CLARIFICATION before Phase 1.

## Bug investigation

### Reproduction

```sh
git clone --depth 1 --branch knative-v1.22.0 https://github.com/knative/func.git /tmp/knative-func
HOME=$(mktemp -d) GOMODCACHE=$(mktemp -d)/empty \
  target/debug/mikebom --offline sbom scan --path /tmp/knative-func \
  --format spdx-2.3-json --output /tmp/out.json --no-deep-hash
# Hangs at 100% CPU. Last log line: "parsed Go source tree" with
# modules=418, production_imports=68. Process spins indefinitely.
```

### Stack-sample evidence

`sample <pid> 3` against the hung process reports 100% of stack samples in `mikebom::scan_fs::package_db::rpm_file::walk_dir` recursing through itself ~10+ levels deep.

The `parsed Go source tree` log line is the LAST output from any reader. The user's diagnosis ("hang during Go import analysis") was based on this log being last; the actual hang is in the **next** reader (`rpm_file::read` calling `discover_rpm_files` calling `walk_dir`).

### Root cause

`mikebom-cli/src/scan_fs/package_db/rpm_file.rs:147-168` follows symlinks via `path.is_dir()` (which dereferences) with no visited-set, no depth limit, no symlink-aware skip. Knative/func ships intentional symlink-loop fixtures at `pkg/oci/testdata/test-links/`:

```
linkToRoot -> .                   # self-loop
b/linkToRoot -> ..                # parent-child loop
b/linkToRootsParent -> ../..      # grandparent loop
b/linkOutsideRootsParent -> ../../..
b/c/linkToParent -> ...
...
```

`walk_dir` enters `linkToRoot`, calls itself recursively with `path = .../linkToRoot`, which `path.is_dir()` resolves to the parent dir, descends into `linkToRoot` again, ad infinitum.

Identical bug shape in `mikebom-cli/src/scan_fs/binary/discover.rs:24-43` — second instance.

## Walker audit

| Walker | File:line | Depth limit | Visited set | Status |
|---|---|---|---|---|
| `rpm_file::walk_dir` | `rpm_file.rs:147` | ❌ | ❌ | **Critical — main hang** |
| `binary::discover::walk_dir` | `discover.rs:24` | ❌ | ❌ | **Critical — second instance** |
| `cargo::walk_for_cargo_lockfiles` | `cargo.rs:714` | ✅ (6) | ❌ | Harden — add visited-set |
| `gem::walk_for_gemfile_locks` | `gem.rs:751` | ✅ (6) | ❌ | Harden — add visited-set |
| `gem::walk_for_gemspecs` | `gem.rs:810` | ✅ (6) | ❌ | Harden — add visited-set |
| `go_binary::walk_for_binaries` | `go_binary.rs:516` | ✅ (10) | ❌ | Harden — add visited-set |
| `maven::walk_for_maven` | `maven.rs:3030` | ✅ (6) | ❌ | Harden — add visited-set |
| `golang::walk_for_go_roots` | `golang.rs:1159` | ✅ (6) | ✅ (canonicalize) | Verify — already protected |
| `project_roots::walk_for_project_roots` | `project_roots.rs:49` | ✅ | ✅ (canonicalize) | Verify — already protected |

9 walkers total. 2 critical (zero protection), 5 to harden (depth-limit only), 2 already protected (verify only — add inline comment naming the protection mechanism per FR-001 audit requirement).

## Decisions

### Decision 1: Per-walker hardening (Option B from Q1) vs. shared `safe_walk` helper (Option C)

- **Decision**: Per-walker patches. Each walker adds its own visited-set following the `golang.rs:1159-1167` pattern.
- **Rationale**: Q1 clarification chose Option B explicitly. Smallest blast radius before alpha.10 ships. Migration to a shared helper is tracked in follow-up issue #108.
- **Alternatives considered**:
  - **Option A (depth-limit-only)**: Rejected per Q1 — leaves O(2^depth) explosion vector on cyclic inputs. Even depth=6 means up to 64 redundant traversals through a cyclic subtree.
  - **Option C (shared `safe_walk` helper)**: Rejected for milestone 054 scope; deferred to issue #108.

### Decision 2: `std::fs::canonicalize` + `HashSet<PathBuf>` for the visited-set keying

- **Decision**: Each walker maintains a `HashSet<PathBuf>` populated by `std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.to_path_buf())`. Insert before recursing; skip recursion if `insert` returns `false` (already visited).
- **Rationale**: Matches the existing pattern in `golang.rs:1163` and `project_roots.rs:51`. std-only (no new crate). `canonicalize` resolves symlinks + normalizes `.` / `..` so two paths reaching the same on-disk dir collapse to the same key. Sub-millisecond per call on typical filesystems; aggregate cost amortized via dedup (each unique canonical dir canonicalize'd once).
- **Alternatives considered**:
  - **Inode pair `(dev, ino)` keying**: Faster on Linux, but POSIX-only — Windows fallback would still need a path-keyed alternative. Rejected for cross-platform consistency.
  - **Path-equality keying without canonicalize**: Cheaper but doesn't dedup symlink-equivalent paths. Rejected — defeats the loop-protection purpose.
  - **`std::fs::read_link` + manual cycle detection**: More code, equivalent behavior. Rejected as needless complexity.

### Decision 3: `MAX_WALK_DEPTH` = 16 across all walkers

- **Decision**: `const MAX_WALK_DEPTH: usize = 16;` per-walker module (not a workspace-wide const).
- **Rationale**: Defense-in-depth backstop for the visited-set primary mechanism. 16 is deeper than any realistic monorepo's natural nesting (typical 6-10 levels). Per-walker definition matches the existing pattern (e.g., `cargo.rs:45 const MAX_PROJECT_ROOT_DEPTH: usize = 6;`); supports follow-up issue #108's helper extraction without forcing every walker to import the same const before the migration.
- **Alternatives considered**:
  - **8 / 10 / 12**: Rejected — some legitimate Rust workspaces have `target/<profile>/<package>/build/<dep>/<...>` nested 10+ deep.
  - **32**: Rejected — overkill; a 32-deep legitimate tree is implausible.
  - **Workspace-wide const in `mikebom-cli/src/scan_fs/mod.rs`**: Rejected — premature centralization; the migration to a shared helper (issue #108) is the natural place to introduce a single const.

### Decision 4: Live `git clone --depth 1 --branch <tag>` for realistic-project CI fixtures (Q2 Option A)

- **Decision**: New `.github/workflows/realistic-projects.yml` clones each project per CI run with `actions/cache@v4` keyed by `<project>:<tag>`.
- **Rationale**: Q2 clarification chose Option A. Smallest code change. Pinned-tag is source-of-truth for content — no manual tarball-regen chore. Network blips on github.com are rare; CI rerun fixes them.
- **Alternatives considered**:
  - **Option B (pre-built tarballs in repo)**: Rejected — 15-30 MB repo bloat per project; manual regen on tag updates.
  - **Option C (GitHub release artifacts)**: Rejected — adds dependency on artifact preservation; more moving parts.
  - **Option D (git submodules)**: Rejected — non-trivial to update; submodule state is checked into the repo, requiring a tag-bump PR every time.

### Decision 5: knative/func @ `knative-v1.22.0` as the headline fixture

- **Decision**: Initial realistic-project CI matrix entry: knative/func at `knative-v1.22.0`.
- **Rationale**: User's literal repro command. Ships 10+ symlink loops in `pkg/oci/testdata/test-links/`. Multi-module Go layout. ~15 MB cloned. Small enough to clone in CI.
- **Alternatives considered**:
  - **kubernetes/kubernetes**: Rejected as initial fixture — too big (~500 MB clone).
  - **helm/helm**: Considered as a 2nd fixture in task generation — small, well-known, no symlink loops (but exercises a different code path).
  - **homebrew-core**: Out of scope (not Go).

### Decision 6: Separate workflow file `realistic-projects.yml` (vs. extending `ci.yml`)

- **Decision**: New `.github/workflows/realistic-projects.yml` runs the realistic-project clone + scan + schema-validate job in parallel with the existing `ci.yml` 3-lane gate.
- **Rationale**: FR-010 — flake isolation. The new job's network-dependent clone + per-platform multipliers shouldn't couple to the main pre-PR gate's flakiness budget. Independently re-runnable.
- **Alternatives considered**:
  - **Extending `ci.yml` with a 4th job**: Rejected — couples flakes; harder to re-run in isolation.
  - **Manual-trigger-only workflow**: Rejected — defeats the point of catching regressions automatically.

## Out-of-scope, tracked elsewhere

- **Issue #108**: Full migration of every walker to a single shared `safe_walk` helper. Deferred so milestone 054 can ship before alpha.10.
- **Performance optimization for the existing depth-limited walkers**: Adding visited-set keeps them O(unique-dirs); pathological inputs (e.g., a tree with millions of files) would need separate analysis. Not in scope here.
- **Windows symlink semantics**: Windows uses `IO_REPARSE_POINT` which behaves differently from POSIX symlinks. mikebom doesn't support Windows; revisit when/if Windows enters scope.
