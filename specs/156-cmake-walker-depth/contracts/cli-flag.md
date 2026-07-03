# Contract — `--cmake-third-party-recursive` CLI flag (NEW)

**Feature**: milestone 156 — CMake walker depth extension
**Scope**: adds a single new boolean flag to `mikebom sbom scan`.

## Summary

`--cmake-third-party-recursive` is a boolean opt-in flag that extends the milestone-156 recursive-descent behavior of `discover_cmake_files` to include `<scan_root>/third_party/` at all depths. Default is OFF (depth-1 walk for `third_party/`, matching milestone-102 behavior). `cmake/` and `Modules/` are always walked recursively regardless of this flag (per FR-001).

## Flag definition

**Name**: `--cmake-third-party-recursive`
**Type**: boolean (clap `#[arg(long)]` derive; no value expected)
**Default**: `false`
**Location**: `mikebom sbom scan` subcommand args, immediately after `--include-vendored`
**Env alias**: `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1` (or `true`, case-insensitive)

## Consumer contract

Operators invoking `mikebom sbom scan` can rely on:

1. **Default behavior stability**: without the flag, `<scan_root>/third_party/*.cmake` and `<scan_root>/third_party/CMakeLists.txt` at depth-1 ARE walked (unchanged from milestone 102). `<scan_root>/third_party/somedep/**/*.cmake` at depth-2+ is NOT walked.

2. **Opt-in expansion**: with the flag (or env var), `<scan_root>/third_party/` gets the same recursive treatment as `<scan_root>/cmake/` and `<scan_root>/Modules/`. Every `.cmake` file and `CMakeLists.txt` file at any depth beneath `third_party/` becomes discoverable + eligible for milestone-155 emission.

3. **Emission shape uniformity**: emissions from newly-discoverable depth-2+ `third_party/` files carry the SAME `mikebom:source-mechanism = "cmake-find-package"` (or `"cmake-pkg-check-modules"`) annotation as any depth-1 emission. The `mikebom:source-files` annotation carries the full nested path (e.g., `third_party/llvm/cmake/config.cmake`), letting consumers filter by prefix if desired.

4. **Env-var equivalence**: `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1` produces byte-identical behavior to `--cmake-third-party-recursive`. Either input source is honored; the flag takes precedence when both are set (matches milestone-102's `--include-vendored` / `MIKEBOM_INCLUDE_VENDORED` conflict-resolution).

5. **No interaction with `--exclude-path`**: milestone-113 `--exclude-path` continues to apply uniformly regardless of the flag's state. Operators wanting to include `third_party/` recursively but excluding `third_party/build/` write:
   ```
   mikebom sbom scan --path . --cmake-third-party-recursive --exclude-path third_party/build/
   ```

## Consumer non-contract

Consumers MUST NOT rely on:

1. Specific ordering of emitted components — `discover_cmake_files` returns a `Vec<PathBuf>` whose iteration order depends on `safe_walk`'s per-directory descent order (filesystem-dependent). Consumers grouping by name or PURL should sort before comparing.

2. Behavior of the flag on projects without a `<scan_root>/third_party/` directory — the flag is a no-op in that case. Setting it doesn't change any other reader's behavior.

3. Interaction with `--include-vendored` — milestone-102's `--include-vendored` controls the `add_subdirectory(third_party/<name>)` extraction path, which is orthogonal to milestone 156's `.cmake`-file discovery. Both flags can be set independently; they don't conflict.

## Provider contract (milestone 156 emissions)

Milestone 156 guarantees:

1. **Zero behavior change when flag not set**: any pre-existing scan target's emitted SBOM is byte-identical to milestone 155's output (per SC-002 byte-identity guard). Golden fixtures across CDX / SPDX 2.3 / SPDX 3 stay unchanged.

2. **Full recursive descent when flag set**: `safe_walk` visits every subdirectory under `<scan_root>/third_party/` up to `max_depth = 16`. Every `.cmake` file and `CMakeLists.txt` file encountered is parsed by the milestone-155 pipeline. Milestone-113 exclude-path filtering applies uniformly.

3. **No new annotation keys**: consumers filtering emissions can NOT rely on a `mikebom:cmake-vendored-tree = true` or similar new marker. Filtering by `mikebom:source-files` path prefix is the intended pattern.

## Wire example — CDX (`--cmake-third-party-recursive` set)

A scan target with:
- `<root>/CMakeLists.txt` containing `find_package(Foo 1.0)`
- `<root>/third_party/llvm/cmake/config.cmake` containing `find_package(Zlib)`

emits (both components in the CDX `components[]`):

```json
[
  {
    "purl": "pkg:generic/foo@1.0",
    "properties": [
      {"name": "mikebom:source-mechanism", "value": "cmake-find-package"},
      {"name": "mikebom:source-files", "value": "[\"CMakeLists.txt\"]"}
    ]
  },
  {
    "purl": "pkg:generic/zlib",
    "properties": [
      {"name": "mikebom:source-mechanism", "value": "cmake-find-package"},
      {"name": "mikebom:source-files", "value": "[\"third_party/llvm/cmake/config.cmake\"]"}
    ]
  }
]
```

## Verification

- **SC-011** integration test at `mikebom-cli/tests/cmake_walker_depth_third_party_opt_in.rs`: fixture with a `find_package(VendoredDepDep)` at `<root>/third_party/somedep/cmake/deps.cmake` (depth-3 within `third_party/`). Test runs twice:
  1. Without env var set → assert 0 `cmake-find-package` components emitted.
  2. With `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1` → assert exactly 1 `pkg:generic/vendoreddepdep` component emitted.
- **CLI parse** verified via `cargo test --workspace` — clap's derive test suite exercises the arg struct on every build.
- **Manual sanity** — `./target/release/mikebom sbom scan --path <fixture> --cmake-third-party-recursive` runs without arg-parse error.
