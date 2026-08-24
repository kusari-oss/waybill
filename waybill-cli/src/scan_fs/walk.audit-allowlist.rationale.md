# walk.audit-allowlist.txt — per-entry rationale

Milestone 664 US3 T064 audit close-out.

The CI walker-audit gate (see `.github/workflows/ci.yml` §"Walker-audit
allow-list check", introduced in milestone 115) blocks new `fn walk_*`
functions in `scan_fs/` unless they appear in
`walk.audit-allowlist.txt`. This file documents WHY each of the current
12 entries is retained after the T063 US1 + US2 audit sweep and the
T064 shrink-attempt.

The T064 aspirational target ("shrink to only FR-005 + m133 + FR-007
fixed-path entries; every other line MUST be gone") assumed T026-T059
would delete the legacy per-reader shim walker functions. Per FR-004
coexistence, those shims stay under `#[allow(dead_code)]` until the
coexistence period ends — a follow-up task deletes them, at which
point their allowlist entries can drop too.

## Categorization

### A. FR-005 permanent escape hatches

These walkers CANNOT migrate to `walk_registry` because they are
per-project-anchored (not scan-root-anchored) OR require descend-into
overrides the shared walker does not currently expose.

| File:Entry | Milestone | Reason |
|---|---|---|
| `package_db/npm/walk.rs:fn walk_node_modules(` | 664 US2 T043 | Inner `node_modules/**` walk needs content-driven bounded descent (nested `node_modules/` under transitive deps). Shared walker traverses once; this walker runs per-project-root within an already-discovered `node_modules/`. |

### B. Legacy shim retentions (all originally-deferred readers now resolved)

**All three originally-deferred m664 readers migrated to the shared
walker as of 2026-08-23**:
- T029 yocto/recipe — resolved via the `Option<Vec<PathBuf>>`
  precomputed-paths pattern (same shape as T047 pants_go / T058
  golang / T059 yocto_layers).
- T039 maven — resolved via the `descend_into` API extension (contract
  C10, landed same day).
- T057 go_binary — resolved via the two-phase pilot pattern
  (candidate-collection pilot + post-pilot finalize) plus C10
  `descend_into: [build, dist, out, coverage, venv]`.

Their legacy walker functions remain under `#[allow(dead_code)]` per
FR-004 coexistence; their allowlist entries stay for that reason
(deletion happens when the coexistence period ends).

| File:Entry | Original Task | Status |
|---|---|---|
| `package_db/maven.rs:pub(crate) fn walk_rootfs_poms(` | T039 (resolved 2026-08-23) | `find_top_level_poms` and other maven walkers retained under `#[allow(dead_code)]` for FR-004 coexistence. Production path uses shared walker via `descend_into: [target]` + ancestor-path filter in `finalize()`. |

### C. Non-scan-tree walkers (infrastructure, not package-reader discovery)

These walkers operate on caches, archive-internal structures, or trace
mode's post-exit artifact scan — NOT the scan tree. Migrating them to
`walk_registry` would be architecturally wrong.

| File:Entry | Milestone | Scope |
|---|---|---|
| `binary/source_binding/cmake_observer.rs:fn walk_for_cmake_build_dirs(` | 155/156 | Binary source-binding infra; walks compiler build dirs for `.dep-v0` / linker-map sections, not scan tree. |
| `package_db/gradle/cache_reader.rs:pub(super) fn walk_transitives(` | 235 US2 | Gradle m235 ladder US2 tier (subprocess output cache reader), not scan-tree. |
| `package_db/maven.rs:fn walk_m2_jars(` | 003/144 | Iterates `~/.m2/` cache (Maven's user-scope local repository), not scan tree. |
| `package_db/maven_sidecar.rs:fn walk(` | 144 | Walks JAR archive internal content via the `zip` crate, not the filesystem. |
| `file_tier/walker.rs:pub(crate) fn walk_file_tier(` | 133 | m133 file-tier walker — OUT OF SCOPE per FR-013. |
| `walker.rs:fn walk(` | 001 (trace mode) | Trace-mode post-exit artifact walker (eBPF trace mode's `.deb` / `.crate` / `.whl` / `.jar` / `.tar.gz` etc. hashing). Separate from `sbom scan` package_db readers. |
| `walker.rs:pub fn walk_and_hash(` | 001 (trace mode) | Trace-mode public entry to the artifact walker. |

### D. Shared-walker infrastructure (the walker functions themselves)

The walker implementations themselves match the `fn walk_` regex.
These are the m064 primary infrastructure and cannot be removed.

| File:Entry | Milestone | Scope |
|---|---|---|
| `walk.rs://! \`fn walk_*\` recursion implementing the same canonicalize-keyed` | m054/m114 | Module-level doc comment on `safe_walk`. False-positive on the regex; kept for now (rewording is out of scope for T064). |
| `walk.rs:fn walk_inner<F: FnMut(&Path)>(` | m054/m114 | `safe_walk`'s own recursive helper. |
| `walk_registry/walker.rs:fn walk_inner(&mut self, current: &Path, depth_remaining: usize) {` | m664 Phase 2 | `SharedWalker`'s own recursive helper. |

## FR-005 promotion rule (for T065)

The T065 CI gate's failure message should point contributors to this
rationale document. New `fn walk_*` functions should:

1. First check whether the reader can migrate to `walk_registry`
   (see `specs/664-single-pass-walker/quickstart.md`).
2. If it truly cannot (e.g., needs descend-into override, is per-
   project-anchored, or is non-scan-tree infra), add BOTH the allowlist
   entry AND a rationale row in the appropriate category above.
3. Never add an entry to bypass CI temporarily — either migrate to
   `walk_registry` or classify explicitly.

Author: Milestone 664 US3 T064 close-out.
