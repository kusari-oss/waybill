# SC-003 / SC-006: the full ROCm SDK install tree (CI only)

**How it was measured.**
- **Workflow:** `measure-full` in the public
  `kusari-sandbox/waybill-fixture-therock-sdk`.
- **Input:** it downloads
  `therock-dist-linux-gfx1150-7.14.0a20260612.tar.gz`, with the S3 version
  and size pinned as in `capture`. It extracts the whole tree (16.55 GB;
  the runner had 82 GB free on `/mnt`).
- **Scans:** waybill is built at this branch and at `main`. Each side gets
  one warm-up, then 3 scans interleaved with the other side's, each with
  `--offline --no-deep-hash`.

Nothing from the tarball was fetched or scanned locally.

## Final run, 43757567769 (branch `0a08ab53`, base `main`)

| | main | branch |
|---|---|---|
| Components | 2,048 | 2,048 |
| Non-metadata files in installed-library components' evidence | 131 | **1,006** |
| Scan runs (s) | 47.9, 46.3, 40.9 | 39.8, 36.7, 38.7 |
| Median (s) | 46.3 | 38.7 |
| Warm-up (s) | 49.5 | 40.1 |

**Ratio (branch ÷ main, median): 0.835.** SC-006 required at most 1.25×.
The branch is *faster*: seeding the evidence-hash cache from the binary
scan (`hash cache: seeded=1919 reused=8002`) also stops a binary's own
occurrence being hashed a second time, which `main` still does.

**C225 `exported-targets` (branch):**

```json
{"attached":906,"locations":917,"no-package-version":7,"not-shipped":4,"owned-by-os-package":0,"read":149,"unresolved-location":0}
```

## SC-003 reconciled with the census (`targets-census.json`)

- **`read` 149 of 150 files.** The file not read is
  `share/rocmcmakebuildtools/cmake/ROCMInstallTargets.cmake`, a CMake module
  whose name ends in `Targets`. It is not an export, and it is not in a
  search location for its own name.
- **`locations` 917 = 914 distinct + 3.** Three files are named by two
  packages each, and FR-007 counts (directory, location) pairs (analysis
  I1):
  - `libamdhip64.so.7.14.60850-1b2a555677` by `hip` and `hip-lang`;
  - `libcares.a` by `c-ares` and `grpc`;
  - `libre2.a` by `grpc` and `re2`.
- **`attached` 906 = the 903 predicted attachable files + those 3.**
- **`not-shipped` 4 and `no-package-version` 7:** as predicted.

## Earlier runs, kept for the record

- **Run 43753466956 (`83a80010`).** `read` was 148: `elfioTargets.cmake` in
  `share/elfio/cmake/` was not read. A bug, fixed as M8. `elfio` is
  header-only, so no attribution was lost. Ratio 0.894.
- **Run 43755366841 (`740cf1d8`, the first fix for M8).** `read` fell to
  129 and `attached` to 898, because that fix lost `composable_kernel`'s
  per-component files. Fixed as M9 in `0a08ab53`. Ratio 0.93.
