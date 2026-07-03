# Data Model — milestone 156

Phase 1 output. Types + wire shapes introduced or extended by the CMake walker depth extension.

## 1. Signature changes

### `cmake::read` — extended parameters

```rust
// Post-milestone-155 (pre-156):
pub fn read(scan_root: &Path, include_vendored: bool) -> Vec<PackageDbEntry>

// Milestone 156:
pub fn read(
    scan_root: &Path,
    include_vendored: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry>
```

The `include_third_party_recursive` boolean is NOT a parameter — it's read from `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE` env var inside `read` (mirrors milestone-102's `MIKEBOM_INCLUDE_VENDORED` pattern at `read_all:1193`). This zero-plumbing propagation keeps the 75-callsite chain unchanged.

**Callers updated**:
- `mikebom-cli/src/scan_fs/package_db/mod.rs:1533` (inside `read_all`)
- `mikebom-cli/src/scan_fs/binary/mod.rs:198` (milestone-109 binding pass)

Both callers already have `exclude_set` in scope; no upstream signature changes propagate beyond these two lines.

### `discover_cmake_files` — extended parameters

```rust
// Post-milestone-155 (pre-156):
fn discover_cmake_files(scan_root: &Path) -> Vec<PathBuf>

// Milestone 156:
fn discover_cmake_files(
    scan_root: &Path,
    include_third_party_recursive: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf>
```

The single internal caller (`cmake::read` at the top of its body) updates its invocation.

## 2. New CLI flag

### `--cmake-third-party-recursive`

**Type**: boolean (clap `#[arg(long)]` derive).
**Default**: `false` (i.e., not set).
**Location**: `mikebom-cli/src/cli/scan_cmd.rs:365` (immediately after `pub include_vendored: bool`).
**Env alias**: `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1` (or `true`, case-insensitive).

**Field addition**:

```rust
/// Extend the CMake reader's recursive descent to third_party/.
/// By default (unset) third_party/ is walked at depth-1 only
/// (matching milestone-102 behavior); recursive descent applies
/// only to cmake/ and Modules/. Setting this flag treats
/// third_party/ the same way. Useful when the parent project has
/// vendored a large dep tree (LLVM, Chromium, WebRTC, etc.) whose
/// transitive find_package declarations should surface in the SBOM.
///
/// Also accepts MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1 env var.
#[arg(long)]
pub cmake_third_party_recursive: bool,
```

**Env-var propagation** (mirrors milestone-102's `MIKEBOM_INCLUDE_VENDORED` at scan_cmd.rs:1703):

```rust
if args.cmake_third_party_recursive {
    // SAFETY: single-threaded at this point in the scan-cmd lifecycle.
    unsafe {
        std::env::set_var("MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE", "1");
    }
}
```

## 3. Internal helper types

### `discover_cmake_files` internal structure

Three new module-private helpers replace the milestone-102 read_dir-based body:

```rust
fn collect_cmake_files_recursive(
    dir: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
    out: &mut Vec<PathBuf>,
);

fn collect_cmake_files_depth1(
    dir: &Path,
    out: &mut Vec<PathBuf>,
);

fn is_cmake_file(p: &Path) -> bool;
```

- `collect_cmake_files_recursive` wraps `safe_walk` (milestone 054) with a `WalkConfig { max_depth: 16, should_skip: no-op, exclude_set }` config.
- `collect_cmake_files_depth1` preserves the milestone-102 read_dir pattern for the `third_party/` default path.
- `is_cmake_file` extracts the milestone-102 extension + filename check for reuse across the two collect fns.

**Lifetime**: all three are stateless helpers; the visited-set inside `safe_walk` is per-invocation and dies at return.

## 4. `WalkConfig` shape used by milestone 156

```rust
use crate::scan_fs::walk::{safe_walk, WalkConfig};

let cfg = WalkConfig {
    max_depth: 16,
    should_skip: &|_candidate: &Path, _rootfs: &Path| false,
    exclude_set,
};
```

- `max_depth = 16`: defensive cap; realistic projects have depth <5.
- `should_skip = no-op`: no name-based directory skipping beyond `exclude_set`.
- `exclude_set`: passed through from `cmake::read`'s caller.

## 5. `PackageDbEntry` shape — UNCHANGED

**No struct-level changes.** The extended walker produces `PackageDbEntry` instances via the milestone-155 `emit_find_package_entries` + `emit_pkg_check_module_entries` code paths unchanged (per FR-012).

Milestone 155's `mikebom:source-mechanism` values (`cmake-find-package`, `cmake-pkg-check-modules`), the `mikebom:cmake-find-package-name` conditional annotation, and the Q1 highest-version-wins consolidation all apply to depth-2+ discoveries identically.

## 6. `extra_annotations` — UNCHANGED

**No new annotation keys.** FR-015 forbids introducing any `mikebom:*` key in this milestone. Consumers filter depth-N emissions by inspecting `mikebom:source-files` path prefixes.

## 7. Wire examples — UNCHANGED

CDX / SPDX 2.3 / SPDX 3 emissions are identical to milestone 155's shapes for every emitted component. The only observable difference from a consumer's perspective is:
- **More components** in the SBOM (Kamailio 1 → ≥10 identified).
- **`mikebom:source-files` values may contain nested paths** (e.g., `cmake/modules/FindNETSNMP.cmake` instead of only `cmake/defs.cmake`).

No emitter code changes.

## 8. Fixture layout (new)

New fixture directory: `mikebom-cli/tests/fixtures/cmake-walker-depth/`

Subdirectories (one per integration test):

```
cmake-walker-depth/
├── symlink-cycle/           # SC-003 testbed
│   ├── CMakeLists.txt
│   └── cmake/
│       ├── defs.cmake
│       └── loop -> ../cmake/   # relative symlink; safe_walk visited-set catches it
│
├── depth3-emission/         # SC-004 testbed
│   ├── CMakeLists.txt
│   └── cmake/
│       └── modules/
│           └── vendor/
│               └── Extra.cmake   # contains find_package(Foo 2.5)
│
├── cross-depth-version/     # SC-005 testbed
│   ├── CMakeLists.txt        # contains find_package(OpenSSL 1.1.0)
│   └── cmake/
│       └── modules/
│           └── FindOpenSSL.cmake   # contains find_package(OpenSSL 3.0)
│
├── exclude-path-integration/# SC-006 testbed
│   ├── CMakeLists.txt
│   └── cmake/
│       ├── defs.cmake        # contains find_package(Bar 1.0)
│       └── modules/
│           └── FindFoo.cmake # contains find_package(Foo)
│                              # test excludes cmake/modules/, expects only Bar emission
│
└── third-party-opt-in/      # SC-011 testbed
    ├── CMakeLists.txt
    └── third_party/
        └── somedep/
            └── cmake/
                └── deps.cmake  # contains find_package(VendoredDepDep)
                                # test runs twice: without flag = 0 emissions;
                                # with flag = 1 emission
```

Total fixture LOC: ~30 lines across 15 files.

## 9. Golden fixtures — UNCHANGED

Post-156 goldens MUST byte-match post-155 goldens for the milestone-090 cmake fixture (per SC-002). If `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression` produces a diff for the cmake fixture, the milestone is failing SC-002.

## 10. `docs/reference/sbom-format-mapping.md` — UNCHANGED

No new catalog rows. C55 (`mikebom:source-mechanism`) and C103 (`mikebom:cmake-find-package-name`) from milestone 155 cover everything milestone 156 emits. This milestone's file-list guard (SC-010) MUST show zero changes to this file.
