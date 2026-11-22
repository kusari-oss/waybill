# Controls (T001, T006, T009)

**`main` binary:** built at `83657af2243e9490ce1121a108115c60f33edd13`, saved as `waybill-main-83657af2` in the session scratchpad. Cases run against it with `WAYBILL_VERSION_BOUNDARY_BIN`. Parity-check always runs with the branch binary; its code is not changed by this spec.

**B-1 golden** generated with that binary (`WAYBILL_UPDATE_VERSION_BOUNDARY_GOLDENS=1`).

## Harness corrections made while writing the controls

- **Parity.** `waybill sbom parity-check` exits 1 on any document without a package author: row A5 is absent in CycloneDX and SPDX 2.3 and `restricted` in SPDX 3, which the tool counts as a gap. That is the tool's behaviour on `main`, not a disagreement between formats. The helper counts a row as a gap only when some format carries a value.
- **Root name.** The document's root is named after the scanned directory, and a random temp-directory name made B-1 unreproducible. Scans now use a fixed `waybill-fixture-root` subdirectory.

## Results

| case | against `main` `83657af2` | at the log-only commit | expected at log-only |
|---|---|---|---|
| B-1 | pass | pass | pass |
| C-1 | fail: no census events | pass | pass (census added) |
| C-2 | fail: no census events | pass | pass (census added) |
| Z-1 | pass | pass | pass |
| Z-2 | fail: `zlib: []` | fail: `zlib: []` | fail |
| Z-3 | fail: `zlib: []` | fail | fail |
| Z-4 | pass | pass | pass (guard) |
| Z-5 | pass | pass | pass (guard) |
| Z-6 | pass | pass | pass |
| Z-7 | pass | pass | pass (guard) |
| N-1 | pass | pass | pass |
| N-2 | fail: `ncurses: []` | fail | fail |
| N-3 | fail: `ncurses: []` | fail | fail |
| N-4 | pass | pass | pass (guard) |
| E-1 | pass | pass | pass |
| E-2 | fail: `byte 0x2d: expat []` | fail | fail |
| E-3 | pass | pass | pass (guard) |
| U-1 | pass | pass | pass (guard) |
| U-2 | pass | pass | pass |

Every case expected to fail fails on its stated assertion, and every guard passes. At the log-only commit, `installed_lib_us3`, `python_owned_binaries`, `orphan_explanations`, the `version_strings` unit tests (48) and the `cpe` unit tests (105) pass unchanged.
