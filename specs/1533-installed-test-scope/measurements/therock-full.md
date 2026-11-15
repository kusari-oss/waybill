# ROCm SDK, full install tree (T022, SC-001 to SC-004, SC-006)

`measure-full` in the public `kusari-sandbox/waybill-fixture-therock-sdk`, run 43772337096, workflow `59c4677`.

**What it does:** it downloads the pinned `therock-dist-linux-gfx1150-7.14.0a20260612.tar.gz` (S3 version `nC_zQeaccuo2uqpYvvLJV2rismYLhW5t`) and scans the 16.55 GB tree with `--path` using two builds:
- **branch**, `0c06a35`;
- **base**, `main` at `1cb56c5`.

Each build gets one warm-up and then three interleaved runs. All numbers below come from the run's report and its `measure-full` artifact.

## C226 (branch)

```json
{"declarations":{"ctest":28,"exported-targets":2,"owned-by-os-package":0},"installed-tree":"recognised",
 "locations":{"not-shipped":0,"outside-root":0,"resolved":1213,"unresolved":528},
 "scoped":{"ctest-declaration":432,"exported-targets-test-package":23,"test-directory":87}}
```

The base has no C226.

The locations reconcile with research R1:
- `resolved` 1,213 = 1,153 relative `add_test`s + 38 interpreter commands whose script resolved + 22 exported-targets locations.
- `unresolved` 528 = 474 variable-based `add_test`s (`rocprofiler-systems`) + 54 `include()`s (HIP `catch_tests`).

## Scoped, by rule (distinct files, sizes read from the extracted tree)

| C62 value | Components | Bytes | Census (research R3) |
|---|---|---|---|
| `ctest-declaration` | 432 | 2.632 GB | 432 files, 2.632 GB |
| `exported-targets-test-package` | 23 (22 files + the package's own component) | 0.188 GB | 22 files, 0.188 GB |
| `test-directory` | 87 | 0.029 GB | 14,465 regular files, 0.236 GB. Most have no component: data and sources are not file-tier shapes (FR-008) |
| **Total** | **542** | **2.849 GB** | |

- **SC-001 and SC-002:** met. Every file the `rocprofiler-sdk-tests` export set or an installed CTest file names is scoped, and the totals equal the census.
- **SC-004:** 2.849 GB is 46.8% of #1564's 6.09 GB census of test, benchmark and example payload, and 17.2% of the tree.
  - **Unscoped and without a component:** the payload above the file-inventory size limit (three of the four `*_gtest.data` blobs).
  - **Unscoped, by decision:** name-only executables (research R4, about 0.95 GB) and HIP's run-time-discovered `catch_tests` (0.514 GB).

## SC-003: no product library is scoped

- **Installed-library components:** none of a package not named as a test package is scoped.
- **Libraries:** 28 scoped `.so`/`.a` files, all in test directories: 23 in `lib/test_plugins/custom/`, 3 in `lib/test_plugins/`, 1 in `lib/test_plugins/default/`, 1 in `share/roctracer/test/`.
- **Edges from unscoped to scoped components:** 542, all from `pkg:pypi/amdsmi@26.5.0%2B1b2a555677`, a pip main module.
  - On base, that component already depends on 1,855 components.
  - On the branch it depends on 1,909. The extra 54 are exactly the test binaries the branch now reports (FR-016), and no base edge was removed.
  - This is existing orphan anchoring to a main module, not this feature. No component's scope flowed along it.

## SC-006: scan time

| | Runs (s) | Median | Warm-up |
|---|---|---|---|
| base `1cb56c5` | 40.1, 37.7, 37.2 | 37.7 s | 53.0 s |
| branch `0c06a35` | 39.2, 39.2, 42.3 | 39.2 s | 47.9 s |

The ratio is **1.040**, within the 1.10 budget. The branch also hashes 54 more binaries (`hash cache: seeded=1973` against 1919): the test binaries #1524 skipped in a directory scan, now reported because the tree is recognised as installed (FR-016).
