# T020: the TheRock SDK and source tree

Branch `e9c76676` against `main` `857e542e`, CI only (`kusari-sandbox/waybill-fixture-therock-sdk`):
- measure-full (the SDK install tree `therock-dist-linux-gfx1150-7.14.0a20260612`): runs 44640105563 and 44643520451;
- measure-source (TheRock `b1cae390` with submodules): run 44640120276.

## Scan time (SC-006)

| Tree | Ratio to `main` (median of 3, after a warm-up) |
|---|---|
| SDK | **1.006** (25.73 s / 25.58 s) |
| Source | 0.981 |

Budget ≤ 1.05×.

## The 9 records (SC-001)

Exactly research R6's prediction:

| Record | `main` | Branch |
|---|---|---|
| `hsa-runtime64@1.21.0` | Optional, C122 `cmake-conditional` | **required**; C236 `["else: rocprofiler-sdk_BUILD_TREE AND EXISTS …"]` unchanged |
| `amd_smi@26.5.0.99999` | Optional | **required**; C236 unchanged |
| `zstd@1.5.7` | Optional | **required**; C236 `["LLVM_ENABLE_ZSTD"]` unchanged |
| `absl@20250512` | Optional | **required**; C236 `["NOT TARGET absl::strings"]` unchanged |
| `openblas@0.3.31.dev`, `openblas64@0.3.31.dev` | Optional | **required**; C236 unchanged |
| `rocprofiler-sdk-rocattach@1.3.1` | Optional, no C236 | **required** |
| `rocprofiler-sdk-tests@1.3.1` | Optional, no C236 | **Test**, C62 `source-test-directory` |
| `rocmcmakebuildtools@0.14.0` | Optional, C236 | unchanged (no library file; a non-test conditional search) |

## Everything else (SC-002, SC-003)

**SDK:**
- 3,150 components and 3,589 edges on both sides; none added or removed.
- **Exactly 8 components change scope:** the eight above. None gains a non-runtime scope it did not have.
- No C235 or C236 value changes.

**Source tree:** the branch document is **identical** to `main`'s once serial, timestamps and document namespace are removed, as predicted (0 installed pairings).

## Formats, duplicates, repeats (SC-005)

- **`waybill sbom parity-check`** on the SDK: the same count-row gaps as `main` (A12, B1, B2). C42, C62, C122, C235 and C236 agree across the formats.
- **Integrity:** 0 duplicate `bom-ref`s.
- **Repeats on one runner:** the three branch runs of 44640105563 are identical.
- **Across runners:** CycloneDX, SPDX 2.3 and SPDX 3 from runs 44640105563 and 44643520451 are identical once serial, timestamps and document namespace are removed. The second run's scan-time ratio was 1.004.
