# Real wheels: census and scans (T001, T021)

Workflow `measure-wheels.yml` in `kusari-sandbox/waybill-fixture-therock-sdk`. The report script is `census/wheels_report.py`. Both are committed beside this file, at fixture-repo commit `3bad3df`. Every download and scan ran in CI. Nothing was fetched locally beyond PyPI's JSON metadata, used to pin the NumPy wheels, and the ROCm index listing, used to find the platform tag.

## T001: census and baseline, run 43884997910 (base `main` `1ae157d5`)

## Wheels

| wheel | sha256 | bytes |
|---|---|---|
| `numpy-2.5.5-cp312-cp312-macosx_11_0_arm64.whl` | `b96e9ee49ccbdb5676359f852a4497f7a9c5df6c377d8877f5330d6fd3ff42e4` | 10749920 |
| `numpy-2.5.5-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl` | `704d819265e5d1064d932968e9ee85fff2103ccc78c05b878c06943ba27b8507` | 15720599 |
| `rocm_sdk_libraries-10.1.0-py3-none-linux_x86_64.whl` | `ffb36d5e1d97e96c191052a10c39a1a7a4ddb4da4a88d3166ab4bf7bfdb94cf6` | 260814462 |

## Census

### numpy-2.5.5-cp312-cp312-macosx_11_0_arm64

```
{
 "native": 20,
 "in_record": 20,
 "libs_dir": 0,
 "dylibs_dir": 1,
 "hashed_in_libs": 0,
 "hashed_elsewhere": 0,
 "record_outside": 0,
 "over_256MiB": 0,
 "over_500MiB": 0,
 "largest": 13899920
}
```

- `numpy/.dylibs/libscipy_openblas64_.dylib`

### numpy-2.5.5-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64

```
{
 "native": 20,
 "in_record": 20,
 "libs_dir": 1,
 "dylibs_dir": 0,
 "hashed_in_libs": 1,
 "hashed_elsewhere": 0,
 "record_outside": 0,
 "over_256MiB": 0,
 "over_500MiB": 0,
 "largest": 23848649
}
```

- `numpy.libs/libscipy_openblas64_-4b78795a.so`

### rocm_sdk_libraries-10.1.0-py3-none-linux_x86_64

```
{
 "native": 26,
 "in_record": 26,
 "libs_dir": 0,
 "dylibs_dir": 0,
 "hashed_in_libs": 0,
 "hashed_elsewhere": 0,
 "record_outside": 0,
 "over_256MiB": 3,
 "over_500MiB": 1,
 "largest": 541649305
}
```



## Scans

refs: base `1ae157d5cad2`

| wheel | ref | .dist-info | components | nested | binaries (nested) | identified | linkage | file tier | C230 | seconds |
|---|---|---|---|---|---|---|---|---|---|---|
| `numpy-2.5.5-cp312-cp312-macosx_11_0_arm64` | base | with | 20 | 0 | 0 (0) | 1 | 0 | 19 | `absent` | 0.1 |
| `numpy-2.5.5-cp312-cp312-macosx_11_0_arm64` | base | without | 22 | 0 | 20 (0) | 1 | 1 | 0 | `absent` | 0.1 |
| `numpy-2.5.5-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64` | base | with | 21 | 0 | 0 (0) | 1 | 0 | 20 | `absent` | 0.2 |
| `numpy-2.5.5-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64` | base | without | 27 | 0 | 20 (0) | 1 | 6 | 0 | `absent` | 0.2 |
| `rocm_sdk_libraries-10.1.0-py3-none-linux_x86_64` | base | with | 23 | 0 | 0 (0) | 0 | 0 | 23 | `absent` | 0.2 |
| `rocm_sdk_libraries-10.1.0-py3-none-linux_x86_64` | base | without | 41 | 1 | 25 (0) | 0 | 16 | 0 | `absent` | 1.6 |


### What the census settles

- **Research R6 is confirmed.** NumPy's `manylinux` wheel vendors `numpy.libs/libscipy_openblas64_-4b78795a.so`. The name matches `^(.+)-[0-9a-f]{8}(\.so(\..+)?)$`, with an empty version tail. No hash-suffixed name occurs outside `*.libs/` in any of the three wheels.
- **Research R8 is confirmed.** The NumPy macOS 11 arm64 wheel vendors `numpy/.dylibs/libscipy_openblas64_.dylib`. The macOS 14 wheel is not used, because it links Accelerate and vendors nothing.
- **The ROCm wheel** (`rocm_sdk_libraries` 10.1.0, `linux_x86_64`) holds 26 native files, all listed in `RECORD`, in neither repaired layout. One is 541,649,305 bytes, over the binary scan's fixed 500 MiB cap (`elf::MAX_BINARY_SIZE_BYTES`), so the no-`.dist-info` scan reports 25 binaries. Three are over the default `--max-file-size` of 256 MiB. That flag gates hashing, not the binary scan (#1528).
- **The regression on real data.** At `main`, each wheel scanned with its `.dist-info` reports **0** binary components and **0** linkage components; only file-tier inventory (19, 20 and 23). Without the `.dist-info`, the same trees report 20, 20 and 25 binary components and 1, 6 and 16 linkage components. ROCm's `librccl.so.1` also carries a nested bundled `fmt` tree (spec 1534).
- `pkg:generic/numpy@2.5.5` is the report's "identified" row for NumPy. It is not a binary identification: the report classifies by PURL shape, and its docstring says so.

## T021: the branch on the same wheels (SC-004)

Run 43890500800 in the fixture repo, `waybill_ref` = `a052a6212dc3` (this branch), base `1ae157d5cad2` (`main`). Same pinned wheels and ROCm sha256 as the census.

| wheel | ref | .dist-info | components | nested | binaries (nested) | identified | linkage | file tier | C230 | seconds |
|---|---|---|---|---|---|---|---|---|---|---|
| `numpy-2.5.5-cp312-cp312-macosx_11_0_arm64` | base | with | 20 | 0 | 0 (0) | 1 | 0 | 19 | `absent` | 0.2 |
| `numpy-2.5.5-cp312-cp312-macosx_11_0_arm64` | new | with | 22 | 0 | 20 (0) | 1 | 1 | 0 | `{"analysed":20,"identified":0}` | 0.2 |
| `numpy-2.5.5-cp312-cp312-macosx_11_0_arm64` | new | without | 22 | 0 | 20 (0) | 1 | 1 | 0 | `absent` | 0.2 |
| `numpy-2.5.5-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64` | base | with | 21 | 0 | 0 (0) | 1 | 0 | 20 | `absent` | 0.2 |
| `numpy-2.5.5-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64` | new | with | 27 | 0 | 20 (0) | 1 | 6 | 0 | `{"analysed":20,"identified":0}` | 0.3 |
| `numpy-2.5.5-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64` | new | without | 27 | 0 | 20 (0) | 1 | 6 | 0 | `absent` | 0.2 |
| `rocm_sdk_libraries-10.1.0-py3-none-linux_x86_64` | base | with | 23 | 0 | 0 (0) | 0 | 0 | 23 | `absent` | 0.3 |
| `rocm_sdk_libraries-10.1.0-py3-none-linux_x86_64` | new | with | 41 | 1 | 25 (0) | 0 | 16 | 0 | `{"analysed":25,"identified":1}` | 2.0 |
| `rocm_sdk_libraries-10.1.0-py3-none-linux_x86_64` | new | without | 41 | 1 | 25 (0) | 0 | 16 | 0 | `absent` | 2.0 |

**SC-004: PASS for all three wheels.** The report's per-wheel check, comparing the branch's with- and without-`.dist-info` scans:

| wheel | identifications without / missing with | linkage without / missing with | paths reported twice |
|---|---|---|---|
| NumPy macOS 11 arm64 | 1 / 0 | 1 / 0 | 0 |
| NumPy manylinux x86_64 | 1 / 0 | 6 / 0 | 0 |
| ROCm `rocm_sdk_libraries` 10.1.0 | 0 / 0 | 16 / 0 | 0 |

**Reading the table:**
- **The regression is closed on real data.** With the `.dist-info` present, `main` reports 0 binaries and 0 linkage. The branch reports 20, 20 and 25 binaries, and 1, 6 and 16 linkage components: the same as the no-`.dist-info` scan. The file-tier rows (19, 20, 23) drop to 0, because each binary now appears once, as its binary component (C-2).
- **"nested 0 of N" is the root-owner case (contract C-1).** In an unpacked wheel, the pypi package is `metadata.component`, so its binaries head `components[]`. Checked in the downloaded CycloneDX: the root is `pkg:pypi/numpy@2.5.5` (resp. `pkg:pypi/rocm-sdk-libraries@10.1.0`), and its `dependsOn` lists 20 (resp. 25) refs, all of them the `file-sha256` binary components.
- **ROCm's one identification nests under its binary.** `pkg:generic/fmt` sits in `components[]` of `librccl.so.1?file-sha256=77d5f37c…`, as in the no-`.dist-info` scan (the bundled tree from spec 1534). C230 counts it: `identified: 1`.
- **NumPy's `identified` column is not a binary identification.** As in the census, it is `pkg:generic/numpy@2.5.5`, classified by PURL shape. C230's `identified: 0` is the binary-level count: the vendored OpenBLAS carries no version string, symbol fingerprint or note that fires.
- **The 26th ROCm native** is the 541,649,305-byte file over the 500 MiB binary cap, unchanged from the census and from `main`.

**Wall time.** These are single runs, so they are not a measurement (no min/median/max). With the `.dist-info`, ROCm took 2.0 s on the branch against 0.3 s on `main`. That is the cost of analysing 25 binaries `main` skipped, and it matches the 2.0 s no-`.dist-info` scan, which analyses the same set. NumPy's scans are 0.2 to 0.3 s on every arm.
