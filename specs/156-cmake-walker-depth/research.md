# Research — milestone 156

Phase 0 outputs for the CMake walker depth extension.

## R1 — Reuse milestone-054 `safe_walk` vs replicate the pattern inline

**Decision**: **Reuse `safe_walk`** from `mikebom-cli/src/scan_fs/walk.rs:174`. Its `WalkConfig` shape matches milestone 156's needs exactly:
- `max_depth: usize` — defensive cap. Pick 16 (well above any realistic project's depth; Kamailio's deepest `cmake/modules/` is 2).
- `should_skip: &dyn Fn(&Path, &Path) -> bool` — pass a no-op closure returning `false` for every candidate. Milestone 156 has no per-directory-name skip policy beyond what `exclude_set` handles.
- `exclude_set: &ExclusionSet` — passed through from the caller. Milestone-113 exclude-path integration comes for free.

Guarantees `safe_walk` already provides:
- **Symlink cycle protection**: canonicalize-keyed `HashSet<PathBuf>` visited-set. Second arrival at the same canonical dir returns early (matches milestone-156 FR-003).
- **Cross-rootfs sandbox**: symlinks whose canonical target escapes `rootfs` are refused (matches milestone-156 FR-004).
- **Unreadable-dir tolerance**: `read_dir().ok()` early-returns; peer directories continue processing (matches existing cmake.rs behavior).
- **Milestone-113 exclude-path**: consulted inside `safe_walk` per-descent (matches milestone-156 FR-005).
- **Deterministic skip logging**: `tracing::debug!` at every skip decision (visibility for operators).

**Rationale**: Constitution Principle I compliance (Pure Rust, Zero C — std-only implementation). Zero new dependencies. Zero code duplication. The `safe_walk` helper was purpose-built as the reader-wide recursion abstraction; using it for cmake.rs's extended walker is the exact use case the milestone-054 docstring anticipated.

**Alternatives considered**:
- **Replicate the pattern inline in cmake.rs**: rejected — copy-pastes safe_walk's ~150 LOC into cmake.rs. Doubles the surface area for symlink-cycle bugs. If safe_walk's sandbox rules get tightened in a future milestone (e.g., cross-mount detection), the cmake replica silently misses it.
- **Add a `walkdir` or `ignore` crate dep**: rejected — spec FR-016 + Constitution Principle I forbid new Cargo dependencies. Also unnecessary given safe_walk's coverage.
- **Recursive `read_dir` without cycle protection**: rejected — a `cmake/loop -> cmake/` symlink infinite-loops. Fails SC-003.

## R2 — CLI flag wiring for `--cmake-third-party-recursive`

**Decision**: **Add a `pub cmake_third_party_recursive: bool` field to the existing `ScanArgs` struct** at `mikebom-cli/src/cli/scan_cmd.rs:365` (immediately after `pub include_vendored: bool`), using clap's `#[arg(long)]` derive. Follow milestone-102's `include_vendored` pattern for env-var propagation:

```rust
/// (docstring) Extend the CMake reader's recursive descent to
/// third_party/. By default (unset) third_party/ is walked at depth-1
/// only (matching milestone-102 behavior); recursive descent applies
/// only to cmake/ and Modules/. Setting this flag treats third_party/
/// the same way. Useful when the parent project has vendored a
/// large dep tree (LLVM, Chromium, WebRTC, etc.) whose transitive
/// find_package declarations should surface in the SBOM.
///
/// Also accepts MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1 env var, read
/// directly by the milestone-156 cmake reader (mirrors the
/// MIKEBOM_INCLUDE_VENDORED env-var propagation pattern).
#[arg(long)]
pub cmake_third_party_recursive: bool,
```

Propagation to the reader mirrors milestone-102's env-var trick at `scan_cmd.rs:1703`. After parsing:

```rust
if args.cmake_third_party_recursive {
    // SAFETY: single-threaded at this point in the scan-cmd lifecycle
    // (same as MIKEBOM_INCLUDE_VENDORED at line 1703).
    unsafe {
        std::env::set_var("MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE", "1");
    }
}
```

**Rationale**: Zero-plumbing propagation — the CLI flag setter (scan_cmd.rs) writes an env var; `cmake::read` reads the env var directly. Avoids adding a bool parameter to the 75-callsite chain that motivated milestone 102's original env-var trick. Matches the existing convention verbatim.

**Alternatives considered**:
- **Plumb through the `read_all` signature**: rejected — 8-parameter `read_all` already, adding `include_third_party_recursive: bool` bloats it. The env-var pattern is the milestone-102 shipped design specifically to avoid this.
- **Boolean via clap `env = "..."` derive**: rejected — clap's env-var handling is strict about "1"/"true"/"0"/"false"; the milestone-102 shipped pattern uses a permissive `v == "1" || v.eq_ignore_ascii_case("true")` check. Keeping the two flags consistent (both permissive) prevents operator confusion.
- **Sub-command like `mikebom sbom scan-cmake --third-party-recursive`**: rejected — over-engineering for a single flag.

## R3 — `cmake::read` signature extension

**Decision**: Extend `cmake::read` at `mikebom-cli/src/scan_fs/package_db/cmake.rs:35`:

```rust
// Before (post-milestone-155):
pub fn read(scan_root: &Path, include_vendored: bool) -> Vec<PackageDbEntry>

// After (milestone 156):
pub fn read(
    scan_root: &Path,
    include_vendored: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry>
```

`include_third_party_recursive` is NOT a parameter — it's read from `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE` env var inside `read` (mirrors `include_vendored`'s env-fallback pattern at read_all:1193).

Both call sites already have `exclude_set` in scope:
- `mikebom-cli/src/scan_fs/package_db/mod.rs:1533` — inside `read_all`, which receives `exclude_set: &ExclusionSet` as its 8th parameter. Update to `cmake::read(rootfs, include_vendored, exclude_set)`.
- `mikebom-cli/src/scan_fs/binary/mod.rs:198` — inside the milestone-109 binding pass, which already has `exclude_set` in local scope. Update to `cmake::read(rootfs, false, exclude_set)` (unchanged `include_vendored = false` per milestone-109 intent).

**Rationale**: `discover_cmake_files` needs `exclude_set` for milestone-113 integration (per FR-005). Adding it as a required parameter follows the dart/composer/cocoapods reader convention. Both callers already have it — no upstream plumbing changes.

**Alternatives considered**:
- **Read `MIKEBOM_EXCLUDE_PATH` env var inside cmake.rs**: rejected — no such env var exists. The exclude-path CLI flag flows through the `ExclusionSet` type; env-var backdoor would be a new invention.
- **Take `Option<&ExclusionSet>`**: rejected — makes the caller's intent ambiguous. Both callers HAVE an exclude_set; there's no ambiguity to model.

## R4 — Extended `discover_cmake_files` implementation

**Decision**: Refactor `discover_cmake_files` at `cmake.rs:195-223`. New signature:

```rust
fn discover_cmake_files(
    scan_root: &Path,
    include_third_party_recursive: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf>
```

Implementation shape:

```rust
fn discover_cmake_files(
    scan_root: &Path,
    include_third_party_recursive: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();

    // Top-level CMakeLists.txt (unchanged).
    let top = scan_root.join("CMakeLists.txt");
    if top.is_file() {
        out.push(top);
    }

    // Recursive-by-default: cmake/ + Modules/.
    for subdir in &["cmake", "Modules"] {
        let dir = scan_root.join(subdir);
        if dir.is_dir() {
            collect_cmake_files_recursive(&dir, exclude_set, &mut out);
        }
    }

    // third_party/: depth-1 by default; recursive only if opt-in.
    let third_party = scan_root.join("third_party");
    if third_party.is_dir() {
        if include_third_party_recursive {
            collect_cmake_files_recursive(&third_party, exclude_set, &mut out);
        } else {
            collect_cmake_files_depth1(&third_party, &mut out);
        }
    }

    out
}

fn collect_cmake_files_recursive(
    dir: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
    out: &mut Vec<PathBuf>,
) {
    use crate::scan_fs::walk::{safe_walk, WalkConfig};
    let cfg = WalkConfig {
        max_depth: 16, // Defensive cap; realistic projects have depth <5.
        should_skip: &|_candidate: &Path, _rootfs: &Path| false, // No name-based skip.
        exclude_set,
    };
    safe_walk(dir, &cfg, |path: &Path| {
        if !path.is_file() {
            return;
        }
        if is_cmake_file(path) {
            out.push(path.to_path_buf());
        }
    });
}

fn collect_cmake_files_depth1(dir: &Path, out: &mut Vec<PathBuf>) {
    // Existing milestone-102 behavior for third_party/ when the flag
    // is not set. Preserves byte-identity for pre-156 fixtures.
    let Ok(read_dir) = std::fs::read_dir(dir) else { return };
    for entry in read_dir.flatten() {
        let p = entry.path();
        if p.is_file() && is_cmake_file(&p) {
            out.push(p);
        }
    }
}

fn is_cmake_file(p: &Path) -> bool {
    let is_cmake_module = p
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("cmake"))
        .unwrap_or(false);
    let is_cmakelists = p
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("CMakeLists.txt"))
        .unwrap_or(false);
    is_cmake_module || is_cmakelists
}
```

**Key design points**:

1. **`safe_walk` invoked with each top-level subdir as its own root** — so canonicalization sandbox anchors on `scan_root/cmake/` (or `scan_root/Modules/`, `scan_root/third_party/`). Symlink loops inside `cmake/` are caught by safe_walk's visited-set; a symlink pointing to `/etc/` (outside `scan_root/cmake/`) is refused by safe_walk's cross-rootfs check because `/etc/` doesn't start with `canonicalize(scan_root/cmake/)`.

   **Correction**: `safe_walk`'s sandbox anchors on the passed rootfs argument. If we pass `scan_root/cmake/`, then a symlink from `scan_root/cmake/link -> scan_root/other-legit-dir/` gets refused because `other-legit-dir` doesn't start with `scan_root/cmake/`. That's TIGHTER than we want — we don't want to refuse legit intra-project symlinks that leave `cmake/` but stay inside `scan_root/`.

   **Solution**: Pass `scan_root` as the sandbox root, but only visit files that are `.cmake` or `CMakeLists.txt`. safe_walk's visited-set + max_depth still catches loops; the sandbox check now only refuses TRULY external targets (e.g., `/etc/passwd`).

   Revised implementation calls `safe_walk(scan_root, &cfg, |path| { ... only process paths under scan_root/cmake/ or Modules/ or third_party/ per opt-in ... })`. But that walks the whole scan_root — too expensive.

   **Better solution**: Keep the per-subdir `safe_walk` calls but relax the sandbox behavior by pre-canonicalizing the scan_root separately. Actually simpler: use `safe_walk(scan_root/cmake, ...)` and accept the tight sandbox — mikebom's existing walkers use this pattern; intra-project symlinks crossing `cmake/` boundary are unusual and generally NOT the operator's intent.

   **Final decision**: `safe_walk(scan_root/cmake, ...)`. Intra-project symlinks that leave `cmake/` get skipped with a `tracing::debug!` log; operators can inspect the log if they hit that edge case. Constitution Principle X (transparency) satisfied.

2. **`max_depth: 16`** — defensive cap. Kamailio's `cmake/modules/` is depth 2 relative to `scan_root/cmake/` (so depth 3 total). 16 accommodates any legitimate project + prevents pathological deep hierarchies from consuming unbounded stack. Aligns with the milestone-054 audit note that "existing walker constants pick 6 or 8 or 10."

3. **`should_skip` returns false for every candidate** — no name-based skip policy. milestone-113's `--exclude-path` handles operator-requested exclusions.

4. **Directory candidates are passed through safe_walk's visit callback too** — the `path.is_file()` early-return filters them out. Directories don't produce output but their descent is what matters.

**Rationale**: preserves all milestone-102 behavior (top-level CMakeLists.txt discovered; `cmake/*.cmake` at depth-1 discovered; `Modules/*.cmake` at depth-1 discovered; `third_party/*.cmake` at depth-1 discovered when flag off) while adding depth-N discovery under `cmake/` + `Modules/` unconditionally + under `third_party/` when flag set.

**Alternatives considered**:
- **Unbounded recursion (`max_depth: usize::MAX`)**: rejected per Assumption 8's "practical performance bounded by file count." A defensive cap costs nothing on realistic projects.
- **Recursive walk of top-level scan_root (matching every subdir dynamically)**: rejected — over-broad. Would walk `src/`, `docs/`, `.git/`, etc. Explicit-per-top-level-subdir keeps the scope bounded per FR-017.
- **`walkdir` crate**: rejected per Constitution Principle I + FR-016.

## R5 — Byte-identity guard (SC-002) verification approach

**Decision**: The extended walker MUST produce byte-identical output for any pre-existing fixture that only has depth-1 `.cmake` files. Verification:

1. **milestone-090 cmake fixture** (`~/.cache/mikebom/fixtures/<sha>/cmake/`): all `.cmake` files at depth-1. Recursive walk of `cmake/` (which contains only `cmake/third_party.cmake` per the milestone-090 stay-set) finds the same 1 file that depth-1 iteration finds. No new emissions. Golden fixtures across CDX / SPDX 2.3 / SPDX 3 stay byte-identical.
2. **milestone-155 Kamailio-shape fixture** (`mikebom-cli/tests/fixtures/cmake-find-package/kamailio-shape/`): contains `cmake/defs.cmake` (depth-1) AND `cmake/modules/FindLibev.cmake` (depth-2). Post-156 the depth-2 file becomes discoverable. Its ONLY `find_package` call is `find_package_handle_standard_args(Libev DEFAULT_MSG ...)` which is intentionally the FR-009 NON-emitting pattern. Post-156 emission count stays 5 cmake-find-package + 1 cmake-pkg-check-modules — matches milestone-155 counts exactly.
3. **milestone-102/103 fetchcontent + vendored fixtures**: existing FetchContent + ExternalProject + add_subdirectory paths unchanged (FR-011). Emissions unchanged.

**Automated verification**: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression` MUST NOT produce a diff for the cmake fixture — running without that env var (the default gate posture) MUST pass all 11 golden tests. If ANY golden diff surfaces, that's an SC-002 violation.

**Rationale**: SC-002 is the load-bearing backward-compat guarantee. Any golden diff indicates surprise emission from a newly-walked file that shouldn't have been discovered OR that changes parse output.

## R6 — Test inventory (SC-008 requires ≥6)

**Decision**: 8 new tests. 6 unit tests inline in `cmake.rs`'s `#[cfg(test)] mod tests` block + 2 integration tests in `mikebom-cli/tests/`.

| # | Test | Type | Covers |
|---|------|------|--------|
| 1 | `discover_cmake_files_walks_cmake_recursively` | unit | FR-001 recursive descent under `cmake/`. Fixture: `cmake/modules/FindFoo.cmake` (depth-2). Assert discovered. |
| 2 | `discover_cmake_files_walks_modules_recursively` | unit | FR-001 recursive descent under `Modules/`. Fixture: `Modules/utils/Extra.cmake` (depth-2). Assert discovered. |
| 3 | `discover_cmake_files_depth1_third_party_by_default` | unit | FR-019 default behavior. Fixture: `third_party/depth1.cmake` (depth-1) + `third_party/subdir/depth2.cmake` (depth-2). No env var set. Assert depth-1 discovered, depth-2 NOT discovered. |
| 4 | `discover_cmake_files_recursive_third_party_when_opt_in` | unit | FR-019 opt-in behavior. Same fixture as #3. Set `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1`. Assert BOTH depth-1 and depth-2 discovered. |
| 5 | `discover_cmake_files_respects_exclude_set` | unit | FR-005 exclude-path integration. Fixture: `cmake/modules/FindFoo.cmake`. `ExclusionSet` contains `cmake/modules`. Assert file NOT discovered. |
| 6 | `find_package_at_depth2_emits_via_read` | unit | US1 A1 end-to-end. Fixture: `cmake/modules/FindLibev.cmake` containing `find_package(Libev 1.4.0)`. Assert `read()` emits `pkg:generic/libev@1.4.0` with mechanism `cmake-find-package` and source path preserved. |
| 7 | `cmake_walker_depth_symlink_cycle_bounded` | integration | SC-003 symlink cycle safety. Fixture: `cmake/loop -> cmake/`. Assert scan completes in <5s; each `.cmake` file read at most once. |
| 8 | `cmake_walker_depth_kamailio_shape_still_5_plus_1` | integration | SC-002 byte-identity for milestone-155 Kamailio-shape fixture. Post-156 emission count stays 5 cmake-find-package + 1 cmake-pkg-check-modules; the newly-discoverable depth-2 `FindLibev.cmake` (which only contains `find_package_handle_standard_args`) does NOT emit. |

**Plus** SC-005 (cross-depth version consolidation), SC-006 (--exclude-path integration), SC-011 (opt-in flag off/on) — all as new integration tests.

Full integration test count: 5. Full unit test count: 6. Total: 11 (well above SC-008's ≥6 floor).

**Rationale**: covers each FR + SC with a dedicated test. safe_walk's own visited-set + sandbox behaviors are tested in `walk.rs`'s existing tests (`safe_walk_bounded_by_max_depth`, `safe_walk_skips_symlink_loops`, `safe_walk_refuses_rootfs_escape`) so we don't duplicate that here.

## R7 — CHANGELOG entry shape (SC-009)

**Decision**: Single subsection under `## [Unreleased]` in `CHANGELOG.md`, above whatever milestone-155's entry currently sits. Content documents:

- Extension of `discover_cmake_files` recursive descent (FR-001).
- Kamailio testbed impact: **1 → ≥10** identified components (SC-001).
- New CLI flag `--cmake-third-party-recursive` + env var alias (FR-019 + Q1 clarification).
- Default behavior for `third_party/` unchanged (depth-1) — SBOMs for existing scan targets stay identical.
- Recommendation for build-tree contamination: `--exclude-path build,cmake-build-*,out` (FR-018).
- Reference back to milestone-155's F1 remediation (this milestone closes that debt).

Include a jq recipe for consumers:

```bash
# Filter cmake-find-package components by source-file depth:
jq '.components[] | select(.properties[]?
  | select(.name == "mikebom:source-mechanism" and .value == "cmake-find-package"))
  | select(.properties[]?
    | select(.name == "mikebom:source-files" and (.value | contains("cmake/modules/"))))
  | .purl' sbom.cdx.json
```

## R8 — Verification approach per SC

- **SC-001** (Kamailio ≥10): manual operator-cadence per quickstart.md Scenario 1. Same maintainer testbed as milestone 155's SC-001.
- **SC-002** (byte-identity guard): automated via `cargo test --test cdx_regression` + `spdx_regression` + `spdx3_regression`. Zero diffs expected for the cmake fixture. Fails if any depth-2+ file in the milestone-090 cmake fixture accidentally emits.
- **SC-003** (symlink cycle safety): automated integration test #7 above; timeout gate at 5s.
- **SC-004** (depth-N synthetic emission): automated integration test with a `find_package(Foo)` at depth-3 (`cmake/modules/vendor/Extra.cmake`).
- **SC-005** (cross-depth version consolidation): automated integration test with `find_package(OpenSSL 1.1.0)` at depth-1 + `find_package(OpenSSL 3.0)` at depth-2. Asserts emitted PURL is `pkg:generic/openssl@3.0` and both source paths present.
- **SC-006** (exclude-path): unit test #5 above.
- **SC-007** (pre-PR gate): `./scripts/pre-pr.sh --no-fail-fast` (per the milestone-155 fix's memory lesson).
- **SC-008** (unit-test count ≥6):
  ```bash
  grep -cE "^\s+fn discover_cmake_files_" mikebom-cli/src/scan_fs/package_db/cmake.rs
  ```
- **SC-009** (CHANGELOG): grep for milestone-156 keywords in the [Unreleased] block.
- **SC-010** (no wire-format changes): `git diff main --name-only -- mikebom-cli/src/generate/` MUST be empty; `git diff main --name-only -- docs/reference/sbom-format-mapping.md` MUST be empty.
- **SC-011** (flag off/on behavior): automated integration test verifying zero vs one emission with/without the env var set.

## R9 — Interaction with milestone-155's emission pipeline

**Decision**: No changes to milestone-155's parse or emit logic. The extended walker feeds a LONGER list of file paths into `read()`'s per-file loop; each file passes through `parse_find_package_calls` + `parse_pkg_check_modules_calls` + `parse_fetch_block` + `parse_vendored` identically to milestone 155. The Q1 highest-version-wins consolidation in `emit_find_package_entries` operates on the full accumulated hit list — depth-2 declarations of the same package name group with depth-1 declarations naturally.

**Verified via** SC-005 integration test (cross-depth version consolidation).

**Rationale**: preserves FR-012 (no changes to milestone-155's parsing regex, emission logic, annotation shape). The walker's scope expansion is orthogonal to the parser's behavior.

## R10 — Zero wire-format changes (SC-010)

**Verified files that MUST NOT change**:

- `mikebom-cli/src/generate/cyclonedx/**` — emitter code untouched.
- `mikebom-cli/src/generate/spdx/**` — emitter code untouched.
- `docs/reference/sbom-format-mapping.md` — no new catalog row. Milestone 155's C55 + C103 rows cover the annotation vocabulary this milestone touches.
- `mikebom-cli/src/parity/extractors/**` — no new parity extractors (no new annotation keys per FR-015).
- `mikebom-cli/src/scan_fs/package_db/*.rs` (except cmake.rs) — other readers untouched.
- `mikebom-common/**`, `mikebom-ebpf/**` — other crates untouched.

**Expected diff file-list** (per SC-010):
- `mikebom-cli/src/scan_fs/package_db/cmake.rs` (primary — extended `discover_cmake_files` + safe_walk integration + new function signature)
- `mikebom-cli/src/scan_fs/package_db/mod.rs` (single-line call-site update)
- `mikebom-cli/src/scan_fs/binary/mod.rs` (single-line call-site update)
- `mikebom-cli/src/cli/scan_cmd.rs` (new arg struct field + env-var propagation)
- `mikebom-cli/tests/*.rs` (5 new integration test files)
- `mikebom-cli/tests/fixtures/cmake-walker-depth/**` (new fixture directory for the integration tests)
- `CHANGELOG.md` (milestone entry)
- `CLAUDE.md` (auto-updated by speckit-plan)
- `specs/156-cmake-walker-depth/**` (speckit branch artifacts)
