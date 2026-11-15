# SC-006: a TheRock ROCm SDK install tree

Measured in CI only. Nothing from the tarball was downloaded or unpacked
locally.

## The fixture

**Repository:** public, `kusari-sandbox/waybill-fixture-therock-sdk`.

Its `capture` workflow (run 43731970271) downloaded a TheRock nightly dist
tarball, pinned as follows:

| Field | Value |
|---|---|
| File | `therock-dist-linux-gfx1150-7.14.0a20260612.tar.gz` |
| Source | `https://rocm.nightlies.amd.com/tarball/` |
| S3 version | `nC_zQeaccuo2uqpYvvLJV2rismYLhW5t` |
| Size | 3,317,540,625 bytes |
| SHA-256 | `efeb6b07211030d0dea0f89e15017fe269f80c8811e22b61799bfb32b0c8bc5a` |

**What it extracted:** only the files this reader reads, byte for byte:
- 254 `*.pc`;
- 84 `*ConfigVersion.cmake` / `*-config-version.cmake`.

The tarball has 43,900 members, 1,263 of them library files (listed by name
only). The `.pc` files sit in seven prefixes, because TheRock bundles its
dependencies with prefixes of their own:

| Prefix | `.pc` files |
|---|---|
| `lib/rdc/grpc/` (gRPC, abseil) | 206 |
| `lib/rocm_sysdeps/` | 32 |
| `lib/host-math/` | 8 |
| the root | 6 |
| `lib/llvm/` | 1 |
| `lib/rdc/grpc/lib/rocm_sysdeps/` | 1 |

## The measurement

The `measure` workflow (run 43732691410) built waybill at `main` and at this
branch (`48baa9f5`), and scanned `sdk/` three times with each, using
`--offline --no-deep-hash`.

| Side | Components | With a version |
|---|---|---|
| main | 0 | 0 |
| branch | 313 | 313 |

On the branch, 253 components come from `pkg-config` and 60 from
`cmake-package-version`. There are 1,274 dependency edges between them, and
14 carry C224 private-only requirements.

**C225:**

```json
{"cmake-version":{"emitted":75,"read":84},"pkg-config":{"emitted":254,"read":254},"skipped":{"computed-version":9,"malformed":0,"no-version":0,"owned-by-os-package":0,"unrecognised-location":0,"unresolved-version":0},"unresolved-requires":14,"version-disagreements":1}
```

`emitted` (329) exceeds the component count (313) by 16. Those are names
that a `.pc` and a CMake version file state at the same version, and they
merge into one component (FR-009).

**SC-006 holds.** Every file with a literal version yields a component, and
the rest are accounted for by a named reason: 254 + 84 = 329 emitted + 9
skipped. Versioned components go from 0 to 313.

**Timing.** Scans of the metadata-only tree, in seconds:

| Side | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| main | 0.066 | 0.064 | 0.065 |
| branch | 0.113 | 0.106 | 0.106 |

## What the reader found

**ROCm itself.** `hip@7.14.60850`, `hiprtc@7.14.60850`,
`rocm-core@7.14.0`, `rocblas@5.5.0`, `hipblaslt@1.4.0`, `miopen@3.5.2`,
`rccl@2.29.7`, `rocfft@1.0.38`, `rocsolver@3.35.0`, `rocsparse@4.6.0`,
`rocprim@4.4.0`, `rocthrust@4.4.0`, `rocprofiler-sdk@1.3.1`,
`composable_kernel@1.2.0`, `amd_comgr@3.3.0`, `hsa-runtime64@1.21.0`,
and the LLVM toolchain at `23.0.0git` (`llvm`, `clang`, `lld`, `mlir`).

**Bundled system libraries** (`rocm_sysdeps`, `host-math`).
`zlib@1.3.2`, `expat@2.7.5`, `libcap@2.69`, `sqlite3@3.51.3`,
`libzstd@1.5.7`, `liblzma@5.8.1`, `libelf@0.195`, `libdrm@2.4.134`,
`gmp@6.3.0`, `mpfr@4.2.2`, `ncursesw@6.6.20251230`, `openblas@0.3.31.dev`,
`fftw3@3.3.9`, and others. These are the libraries a vulnerability scanner
needs exact versions for. Before this change the SDK stated them and
waybill reported none.

**The 9 `computed-version` skips, all checked by reading the files:**
- `rdc-config-version.cmake` sets `PACKAGE_VERSION ""`, an empty version
  shipped by the build.
- `LLVMConfigVersion.cmake`, `ClangConfigVersion.cmake`,
  `LLDConfigVersion.cmake` and `ROCMConfigVersion.cmake` under
  `lib/cmake/` and `share/rocm/cmake/` are redirect stubs that `include()`
  the real version file elsewhere. The real LLVM, Clang and LLD versions
  are read from `lib/llvm/lib/cmake/`.
- The rest (`libhipcxx`, `libcudacxx`, `TensileConfigVersion`) compute
  their version.

**The 14 unresolved requirements.** *(Corrected 2026-11-15: this first read
"10 name a library the tarball does not ship; 4 come from the renaming". A
recount of the scan log, and of the member listing, gives the figures
below.)*
- **7 come from TheRock's prefix renaming.** The `rocm_sysdeps` ncurses
  files (`formw`, `menuw`, `ncurses++w`, `panelw`) require
  `rocm_sysdeps_*` modules, but the files are named `formw.pc`,
  `ncursesw.pc` and so on. pkg-config resolves `Requires` by file name, so
  it could not resolve them either. Recognising the renaming is the TheRock
  installed-artifact cataloger's concern (#1541), not core.
- **6 name libraries that ship without a `.pc` file:**
  - `openssl` ×3, while `libssl.a` and `libcrypto.a` sit in
    `lib/rdc/grpc/lib/`;
  - `re2` ×3, while `libre2.a` is there too.

  gRPC's `.pc` files require pkg-config modules that the bundle does not
  install. #1563's exported targets do name `libre2.a`.
- **1 names a library the tarball does not ship:** `opentelemetry_api`.

**The 1 version disagreement: gRPC.** `grpc.pc` states `52.0.0`, gRPC
core's library version, while `gRPCConfigVersion.cmake` states the release,
`1.78.1`. Both components are emitted, since FR-009 merges only identical
PURLs, and C225 counts the disagreement.
