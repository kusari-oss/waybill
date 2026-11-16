# ROCm source (T014, SC-001)

`census-downloads` run 43802096617 in `kusari-sandbox/waybill-fixture-therock-sdk` (workflow `8e94ef9`).
- **What was scanned:** sparse checkouts of every `*.cmake` and `CMakeLists.txt` at TheRock's pins, `rocm-libraries@9811f2ea5235`.
- **How:** waybill built at `1535-declared-datasets` (`e66b3add`) scanned two roots, `projects/rocsparse` and `projects/hipsparse`. They are separate roots because of the CMake reader's discovery limit (research R7, #1516).
- **Comparison:** each scan's output against the fixture listing's `clients/matrices/` stems, 24 distinct.

| Root | C229 | Data components | With `checksum=md5:` | Matching installed stems | Not installed | SPDX 3 `dataset_DatasetPackage` |
|---|---|---|---|---|---|---|
| `rocsparse` | `{"datasets":24,"digest-mismatches":0,"not-classified":0,"read":8,"unresolved":4}` | 24 | 24 | 24 of 24 | 0 | 24 |
| `hipsparse` | `{"datasets":19,"digest-mismatches":0,"not-classified":0,"read":6,"unresolved":2}` | 19 | 19 | 19 | 0 | 19 |

Example: `pkg:generic/Bai/qc2534?checksum=md5:fda33f178963fbb39dfc8c051fd0279e&download_url=https://sparse.tamu.edu/MM/Bai/qc2534.tar.gz`.

Why each `read` count adds up:
- **rocSPARSE, 8:** two files, each with a `$ENV` mirror (unresolved), a primary URL and a fallback URL. That is 2 unresolved and 4 resolved, and the resolved ones join one component per element. Its `ClientMatrices.cmake` has a second, local-path download (`${mat}`), so `unresolved` is 4.
- **hipSPARSE, 6:** read the same way.

**SC-001 is met.**
