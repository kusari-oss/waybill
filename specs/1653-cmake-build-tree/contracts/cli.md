# Contract: `--cmake-build-tree`

```text
waybill sbom scan --path <SOURCE> --cmake-build-tree <BUILD> [--cmake-build-tree <BUILD> ...]
```

- **Repeatable** (`clap::ArgAction::Append`, the same pattern as
  `--registry-ca-cert`, `scan_cmd.rs:557`). It accepts `PATH` values.
- **Location.** A path can lie inside or outside `--path`. Inside a named
  path, waybill descends into `build/`, `out/` and dot-directories,
  including `.cmake/api/`. The default-descent skip applies only to the
  source scan, which is unchanged.
- **Path errors.** A path that does not exist, or is not a directory, fails
  the scan with exit code 2 and a message naming the flag. This is an
  operator error, not missing evidence. A named path holding no
  `CMakeCache.txt` is **not** an error: it is recorded in C246 as
  `build-dirs: 0`, with an INFO log.
- **Mode.** Directory scans only. With `--image` the flag is rejected, because
  an image rootfs is not a build tree.
- **Without the flag:** output is byte-identical to `main` (FR-001, SC-004).
- **Side channel.** The value is exported as `WAYBILL_CMAKE_BUILD_TREES`,
  joined with `std::env::join_paths`, matching
  `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE`. It is an internal detail. It is
  not documented as an operator interface, and it is removed by #1483's
  `ScanContext`.
- **Logging.** An INFO summary is emitted on the target
  `waybill::cmake_build_tree`. It reports the build directories read and the
  number of declarations in each state, using C246's numbers. Every log path
  uses the redacted form (FR-006).
