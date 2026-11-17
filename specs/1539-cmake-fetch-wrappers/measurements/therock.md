# TheRock at the branch (T018: SC-002 to SC-004, SC-008, I-1)

**Run:** `census-fetches` 44105368718 in `kusari-sandbox/waybill-fixture-therock-sdk` (fixture `15e06bd`).
- **Inputs:** `waybill_ref` = `1539-cmake-fetch-wrappers` at `bbbe0b34`, and `base_ref` = `main` at `b8ba7e4d`.
- **Tree:** TheRock at `b1cae390`, with each top-level submodule at its recorded commit. The checkout is sparse (`*.cmake`, `CMakeLists.txt`, `.gitmodules`).
- **Scans:** both refs scanned the same tree 3 times, interleaved. TheRock was not cloned locally.

## C231

| | `main` | branch | prediction (`census.md`, inference) |
|---|---|---|---|
| `read` | 191 | **253** | 241 |
| `emitted` | 110 | **150** | 150 |
| `components` | 65 | **99** | 104 |
| `merged.existing-component` | 37 | 37 | 37 |
| `unresolved.identity` | 44 | **66** | 54 |
| `conditional` / `test` | 42 / 9 | 61 / 9 | |
| `wrappers` | absent | `{"ambiguous-calls":0,"calls":67,"definitions":9}` | `{"ambiguous-calls":0,"calls":53,"definitions":7}` |

V1 holds: 253 = 150 + 0 + 37 + 66. V2 holds: 61 + 9 ≤ 99.

**Why the prediction was off, traced:**
- **Definitions (+2) and calls (+14).** The reader recognises `rocdecode_add_test_ext` and `rocjpeg_add_test_ext` as wrappers. Their body names its project `test_${TEST_EXE}` and passes `SOURCE_DIR ${TEST_SOURCE_DIR}`, both parameters, which is the `Parameter` shape. The census script counted a parameter only as the exact first argument, so it listed them as forwarding nothing.
  - Their 14 calls build in-tree test programs, with no URL, so all 14 are unresolved. That is correct (US2).
  - With them: `read` = 191 − 5 templates + 67 calls = 253, and `unresolved.identity` = 44 − 5 + 13 (LLVM) + 14 = 66. Both match exactly.
- **Components (99, not 104).** All 40 `therock_subproject_fetch` calls are emitted (`emitted` +40). They are 39 distinct PURLs, and 39 components carry C234. Five of those PURLs were already components on `main`, declared directly elsewhere in the tree at the same name and version: `Catch2@3.8.1`, `FunctionalPlus@0.2.25`, `frugally-deep@0.15.31`, `googletest@1.17.0` and `json@3.12.0`. One component per PURL (spec 1538 R4) combines them, so 34 PURLs are new.
  - For each of the five, TheRock's declared hash equals the existing component's.
  - C18 lists both files.

## SC-002: TheRock's third-party sources

All 40 calls are emitted, as 39 components, each with its declared hash: SHA-256 for 31, SHA-512 for 8.
- **Named from the archive (FR-005):** 36.
- **Keeping the bound name, at `unknown`:** 3. These are `therock-OpenBLAS-sources` (a commit in the filename), `therock-libbacktrace-sources` (a commit) and `therock-sqlite3-sources` (SQLite's `3510300` numbering).

| PURL | hash | call site | scope |
|---|---|---|---|
| `pkg:generic/Catch2@3.8.1` | SHA-256 `18b3f70ac80f…` | `third-party/Catch2` (+2 direct declarations) | required |
| `pkg:generic/FunctionalPlus@0.2.25` | SHA-256 `9b5e24bbc92f…` | `third-party/FunctionalPlus` (+ miopen) | required |
| `pkg:generic/SuiteSparse@7.8.3` | SHA-256 `ce39b28d4038…` | `third-party/SuiteSparse` | required |
| `pkg:generic/binutils@2.46.0` | SHA-512 `30d24f7461…` | `third-party/sysdeps/linux/libiberty` | conditional |
| `pkg:generic/bzip2@1.0.8` | SHA-512 `083f5e675d…` | `third-party/sysdeps/common/bzip2` | conditional |
| `pkg:generic/eigen@3.4.0` | SHA-256 `b4c198460eba…` | `third-party/eigen` | required |
| `pkg:generic/elfio@3.12` | SHA-512 `13ec6be1f8…` | `third-party/elfio` | required |
| `pkg:generic/elfutils@0.195` | SHA-512 `e1e1fdf4f7…` | `third-party/sysdeps/linux/elfutils` | conditional |
| `pkg:generic/expat@2.7.5` | SHA-256 `1032dfef4ff1…` | `third-party/sysdeps/common/expat` | conditional |
| `pkg:generic/fftw@3.3.10` | SHA-256 `56c932549852…` | `third-party/fftw3` (2 calls) | conditional |
| `pkg:generic/flatbuffers@25.9.23` | SHA-256 `9102253214de…` | `third-party/flatbuffers` | required |
| `pkg:generic/fmt@11.1.3` | SHA-256 `7df2fd3426b1…` | `third-party/fmt` | required |
| `pkg:generic/frugally-deep@0.15.31` | SHA-256 `49bf5e30ad2d…` | `third-party/frugally-deep` (+ miopen) | required |
| `pkg:generic/gmp@6.3.0` | SHA-512 `e85a0dab51…` | `third-party/sysdeps/common/gmp` | conditional |
| `pkg:generic/googletest@1.17.0` | SHA-256 `65fab701d982…` | `third-party/googletest` (+3 direct declarations) | required |
| `pkg:generic/grpc@1.78.1` | SHA-256 `96e1d4c82735…` | `third-party/grpc` | required |
| `pkg:generic/hwloc@1.11.13` | SHA-256 `a4494b7765f5…` | `third-party/sysdeps/common/hwloc` | conditional |
| `pkg:generic/json@3.12.0` | SHA-256 `42f6e95cad6e…` | `third-party/nlohmann-json` (+ hipdnn) | required |
| `pkg:generic/libcap@2.69` | SHA-256 `9cc2fa3ee744…` | `third-party/sysdeps/linux/libcap` | conditional |
| `pkg:generic/libdivide@5.2.0` | SHA-256 `73ae910c4cdb…` | `third-party/libdivide` | required |
| `pkg:generic/libdrm-libdrm@2.4.134` | SHA-256 `d77e5af336b6…` | `third-party/sysdeps/linux/libdrm` | conditional |
| `pkg:generic/libmnl@1.0.5` | SHA-256 `274b9b919ef3…` | `third-party/sysdeps/linux/libmnl` | conditional |
| `pkg:generic/libnl@3.12.0` | SHA-256 `fc51ca7196f1…` | `third-party/sysdeps/linux/libnl` | conditional |
| `pkg:generic/libpciaccess@0.18.1` | SHA-256 `4af43444b38a…` | `third-party/sysdeps/linux/libpciaccess` | conditional |
| `pkg:generic/mpfr@4.2.2` | SHA-512 `eb9e7f51b5…` | `third-party/sysdeps/common/mpfr` | conditional |
| `pkg:generic/msgpack-cxx@7.0.0` | SHA-256 `7504b7af7e7b…` | `third-party/msgpack-cxx` | required |
| `pkg:generic/ncurses@6.6` | SHA-512 `02647baae5…` | `third-party/sysdeps/common/ncurses` | conditional |
| `pkg:generic/numactl@2.0.19` | SHA-256 `f2672a0381cb…` | `third-party/sysdeps/linux/numactl` | conditional |
| `pkg:generic/openmpi@5.0.9` | SHA-256 `dfb727625311…` | `third-party/openmpi` (URL from a same-file `set()`) | conditional |
| `pkg:generic/simde@0.8.2` | SHA-256 `ed2a3268658f…` | `third-party/simde` (URL from a same-file `set()`) | conditional |
| `pkg:generic/spdlog@1.15.3` | SHA-256 `15a04e69c222…` | `third-party/spdlog` | required |
| `pkg:generic/therock-OpenBLAS-sources@unknown` | SHA-256 `5d81882fdf45…` | `third-party/host-blas` | conditional |
| `pkg:generic/therock-libbacktrace-sources@unknown` | SHA-512 `8acc3324d3…` | `third-party/sysdeps/common/libbacktrace` | conditional |
| `pkg:generic/therock-sqlite3-sources@unknown` | SHA-256 `acb1e6f5d832…` | `third-party/sysdeps/common/sqlite3` | conditional |
| `pkg:generic/util-linux@2.42.1` | SHA-256 `138a3bd9049a…` | `third-party/sysdeps/linux/util-linux` | conditional |
| `pkg:generic/xz@5.8.1` | SHA-256 `5965c692c4c8…` | `third-party/sysdeps/common/liblzma` (`${_xz_url}`) | conditional |
| `pkg:generic/yaml-cpp@0.8.0` | SHA-256 `fbe74bbdcee2…` | `third-party/yaml-cpp` | required |
| `pkg:generic/zlib@1.3.2` | SHA-256 `bb329a0a2cd0…` | `third-party/sysdeps/common/zlib` | conditional |
| `pkg:generic/zstd@1.5.7` | SHA-256 `eb33e51f49a1…` | `third-party/sysdeps/common/zstd` | conditional |

24 components are conditional (`cmake-conditional`). They stand for the 25 calls the census found under an `if()`; fftw's two calls are one component.

## SC-003: nothing false

- **No component has `${` or `%24` in its PURL.**
- **No component is named after a wrapper parameter.** The only `therock-*-sources` names are the three FR-005 fallbacks above.
- **LLVM's 13 in-tree wrapper calls and rocdecode's and rocjpeg's 14 test-build calls** emit nothing and are counted in `unresolved.identity`.

## SC-004: accounting

- V1 and V2 hold (above).
- Every census call site is accounted for: 40 emitted, 27 unresolved.
- `wrappers.definitions` = 9, which is the census's 7 plus the two the census script under-detected. `ambiguous-calls` = 0, because no wrapper is defined twice on TheRock (census).

## SC-008 and I-1

- **C234:** 39 components in CycloneDX, 39 in SPDX 2.3, 39 in SPDX 3. No direct-only component carries it.
- **I-1:** the root's direct targets agree in the three formats, with 118 in each at the branch and 79 in each at `main` (`specs/1537-requirements-root-attach/measurements/root_targets.py`). The 39 added are the 39 wrapped components, whose call sites are in TheRock's own tree, so the root owns them.

## Scan time of the sparse tree (same job, interleaved)

| | run 1 | run 2 | run 3 | median |
|---|---|---|---|---|
| `main` | 7.012 s | 6.950 s | 6.975 s | 6.975 s |
| branch | 6.980 s | 7.002 s | 6.987 s | 6.987 s |

## SC-005: full tree with submodules (T019)

`measure-source` 44105373483 in the fixture repository scanned the full 17 GB tree with all submodules at `main` (`b8ba7e4d`) and at the branch (`bbbe0b34`). Each side had one untimed warm-up, then three alternating timed rounds.

| | warm-up | run 1 | run 2 | run 3 | median |
|---|---|---|---|---|---|
| `main` | 155.8 s | 117.3 s | 114.5 s | 116.2 s | 116.2 s |
| branch | 120.5 s | 122.2 s | 116.2 s | 117.3 s | 117.3 s |

**Ratio of medians: 1.010.** That is within SC-005's 1.5×, and within the spread of each side's own rounds (114.5 to 122.2 s).

The same documents:
- **Components:** 4,312 → 4,346, and unique PURLs 2,636 → 2,670. Both rise by 34, the new wrapped PURLs. 39 components carry C234.
- **C231** is identical to the sparse census above, at both refs. The full tree adds no CMake file that the sparse checkout lacked.
