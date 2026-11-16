# ROCm SDK, full install tree (T017: SC-001, SC-002, SC-004, SC-006)

`measure-full` run 43792913547 in the public `kusari-sandbox/waybill-fixture-therock-sdk` (workflow `9c80479`).
- **Inputs:** the pinned tarball, scanned with `--path`. **branch** is `f9c75061`; **base** is `main` at `96a7ef32`.
- **Timing:** one warm-up, then three interleaved runs per side.
- **Numbers:** computed from the run's `measure-full` artifact (`branch.cdx.json`, `base.cdx.json`, `timing.json`), with file sizes from the committed listing.

## C225 `embedded-paths` and C228 (branch)

```json
"embedded-paths": {"attributed":616,"header-only":71,"no-versioned-match":297,"read":1736,"shared":1,"toolchain-only":120,"with-paths":1105}
"waybill:bundled-source-trees": {"binaries":547,"components":645,"excluded":29}
```

Base has neither.

**Why `read` is larger than the census.** `read` is 1,736, against the census's 708. The census input was binaries that were unattributed **and not test-scoped**. The scan examines every binary file component no package metadata names, including the ones #1564 test-scopes (spec US1 scenario 6).

## Attribution

**617 binaries, 3.004 GB distinct, are attributed.** By package:

| Package | Binaries | Size |
|---|---|---|
| `rocthrust` | 333 | 1.117 GB |
| `miopen` | 3 | 0.398 GB |
| `rocprim` | 71 | 0.316 GB |
| `hipcub` | 47 | 0.228 GB |
| `rocblas` | 14 | 0.221 GB |
| `rocprofiler-sdk` | 19 | 0.140 GB |
| `rocrand` | 54 | 0.108 GB |
| `rocshmem` | 1 | 0.108 GB |
| `rocsparse` | 2 | 0.108 GB |
| others (13 packages) | 53 | 0.270 GB |

- Most `rocthrust`, `rocprim`, `hipcub` and `rocrand` binaries are their CTest-declared tests and benchmarks, test-scoped by #1564. Attribution says which project built them; their scope is unchanged.
- **One binary is shared**, between `rocm_smi` and `amd_smi`.

## SC-001 and SC-002: against the committed census (`attribution.json`, v3)

On the census's own 708 files, 258 have an expected or an actual attribution. **257 agree exactly.**

The one difference is `lib/librocjitsu_kmd.so`: the census expects `libhsakmt`, and the scan attributes nothing.
- The census's only `libhsakmt` path for it is `/rocr-runtime/libhsakmt/src/topology.c`.
- The census regex matches a `/`-run anywhere in a string. The scanner takes a path only where it starts the string or follows a non-path byte (FR-001, absolute paths).
- *Inference, not read from the binary:* the string carries a path character before that `/` (a relative `../rocr-runtime/…`). The scanner is right to skip it, and the census over-counts by one.

No attribution names a package its outermost compiled-source directory does not name.

## SC-006: bundled components

645 components in 547 binaries:

| Name | Count |
|---|---|
| `googletest` | 447 |
| `abseil-cpp` | 48 |
| `fmt` | 46 |
| `dyninst` | 24 |
| `ptl` | 18 |
| `timemory` | 15 |
| `fftw3` | 12 |
| `perfetto` | 10 |
| `pybind11` | 8 |
| `grpc` | 4 |
| `json`, `glog`, `gotcha`, `suitesparse` | 3 each |
| `flatbuffers` | 1 |

- `googletest` dominates because each test executable compiles it in.
- All are project names.
- `sysdeps`, 29 occurrences, was excluded.

## SC-004: scan time

| | Runs (s) | Median |
|---|---|---|
| base `96a7ef3` | 31.5, 29.1, 26.7 | 29.1 s |
| branch `f9c7506` | 25.5, 27.0, 28.9 | 27.0 s |

The ratio is **0.926**, within 1.25. The spreads overlap, so this is not a speed-up; the extraction's cost is inside the noise.
