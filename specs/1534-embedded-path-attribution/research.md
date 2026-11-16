# Research: embedded-path attribution

**Fixture:** `kusari-sandbox/waybill-fixture-therock-sdk`, the pinned ROCm 7.14 SDK tarball. Measurements run in CI only.
- **Input:** `measurements/unattributed-binaries.txt`, 708 regular-file binaries (3.015 GB) that no package claims and that are not test-scoped, taken from `measure-full` run 43776638887.
- **Census:** three runs of `census-binaries`: 43784750746 (v1), 43786377987 (v2) and 43786608413 (v3).
- **Committed** under `measurements/`: the census scripts, `census.json` (v1), `attribution.json` (v3) and `versioned-names.txt` (the 312 versioned installed-library component names).

## R1. Where the paths are (measured, v1 and v2)

| Source | Files | Size |
|---|---|---|
| Absolute source paths in strings | 545 | 2.977 GB |
| DWARF compile units | 42 | 0.260 GB |
| GNU build-id | 122 | 0.115 GB |

Under the clarified rules (v2), attribution read from:
- the read-only region, capped at 16 MiB;
- the same region uncapped;
- `.debug_str` + `.debug_line_str`.

**Results:**
- The 16 MiB cap changes the result for **0** files.
- Debug strings add **0** names.

**Decision:** read paths from the string region the scanner already collects for its version-string patterns (`scan.rs::collect_string_region`: `.rodata` and `.data.rel.ro` on ELF, `__cstring`/`__const` on Mach-O, `.rdata` on PE, capped at 16 MiB). Do not parse DWARF.
- **Rationale:** it is the same buffer, so there is no second read and no new dependency (FR-009).
- **Alternative rejected:** hand-parsing `.debug_line`, at 0 measured gain.

## R2. The matching rule (measured, v2 and v3)

| Rule | Attributed to one package | Shared |
|---|---|---|
| Any directory named for a versioned package | 88 (0.810 GB) | 171 (0.524 GB) |
| **Outermost such directory per path** | 258 (1.269 GB) | 1 (0.066 GB) |
| **Outermost + stoplist `amd`** (adopted) | **258 (1.308 GB)** | **0** |

**The two collisions:**
- **`hip`.** Paths such as `…/projects/rocthrust/testing/unittest/hip/testframework.cu` and `…/rocprofiler-sdk/hip/stream.cpp` contain a `hip` directory inside another project. Matching any directory made every rocThrust test "shared" with HIP. The outermost rule fixes it.
- **`amd`.** SuiteSparse's `amd@3.3.3` (in `lib/host-math`) matched `/gpu/drm/amd/amdkfd/…` and `/patch_source/src/amd/common/…` (kernel and Mesa sources). The stoplist fixes it.

**Decision:** the outermost directory, plus a committed stoplist, `["amd"]`. The stoplist grows only with a measurement.

**Header paths** (`.h`, `.hpp` …) never attribute. 39 files name a package only through headers, and they stay unattributed.

## R3. What is attributed (v3, adopted rule)

| Package | Binaries | Size |
|---|---|---|
| `rocthrust` | 164 | 0.353 GB |
| `miopen` | 2 | 0.359 GB |
| `rocblas` | 13 | 0.143 GB |
| `rocprofiler-sdk` | 9 | 0.140 GB |
| `rocshmem` | 1 | 0.108 GB |
| `libhsakmt` | 1 | 0.066 GB |
| `hipblaslt` | 43 | 0.037 GB |
| others (12 packages) | 25 | 0.102 GB |

Notes:
- rocThrust is header-only, so its 164 binaries are its tests and benchmarks. Attribution says which project built a file; #1564's scope is unchanged.
- MIOpen's 0.353 GB library is attributed to `miopen` alone. Its other project directory is `composablekernel`, which does not equal the component `composable_kernel` (exact names).
- On this tree no binary is shared, but the rule is kept and tested.

## R4. Bundled trees (v3)

52 binaries name bundled trees:
- `abseil-cpp`, `dyninst`, `fftw3`, `fmt`, `googletest`, `gotcha`, `grpc`, `json`, `perfetto`, `ptl`, `pybind11`, `timemory`;
- and `sysdeps`.

`sysdeps` is TheRock's own vendoring directory (`third_party/sysdeps/…`), not a project.

**Decision:** an exclusion list of `["sysdeps"]`, committed and measured. Each other name becomes a versionless component nested under the binary that names it.

## R5. Where the names come from, and when

Matching needs the set of versioned component names before the binary scan, so the scan can keep only the outermost matching name per path, which is a few bytes per binary. #1531's phase-1 records are parsed before `binary::read` (the #1564 recognition already uses them).

**The set:** each record with a recognised prefix, a parsed version and no OS owner, lowercased. That is exactly the set phase 2 will emit.

**Rejected alternative:** carry each binary's directory segments to phase 2. That means thousands of strings per large binary, held for the whole scan.

## R6. Where attribution and bundling land

- **Attribution** happens in #1531/#1563's phase 2 (`installed_lib::resolve`), next to the exported-targets attachment, by the same `extra_source_paths` route. The package record is found the way #1563 finds one: the CMake version record first, then a same-name `.pc`. The hash is reused from the binary scan (#1563 R5).
- **Bundled components** are built in `binary::read`, beside the existing embedded identities. They follow the symbol-fingerprint entry's shape: tier `analyzed`, their own `evidence_kind`, confidence `heuristic`, version absent, and `parent_purl` set to the binary's file component.
- **Excluded from attribution:**
  - a binary another reader already attributed, whose path is in a #1531/#1563 evidence list;
  - a binary an OS package owns. The binary scan already skips claimed files.

## R7. Reporting (FR-007)

**C225** gains an `embedded-paths` object, as #1563 added `exported-targets`. It counts:
- the binaries read and those carrying paths;
- those attributed to one package, and those attributed as shared;
- those with header-only, toolchain-only or no-versioned-match paths;
- the bundled components emitted.

C225 already exists whenever versioned installed-library components do, which is the only case in which attribution is possible.

**Per component**, a new row C227 `waybill:embedded-path-binaries` lists which evidence files came from embedded paths, and the packages each one names when it is shared. Without it a consumer cannot tell them from exported-targets evidence (FR-004).
