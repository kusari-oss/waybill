# Research: declared datasets

**Census:** `census-downloads` run 43800233978 in `kusari-sandbox/waybill-fixture-therock-sdk` (workflow `4071bd8`).
- **What it read:** sparse, blob-less checkouts of every `*.cmake` and `CMakeLists.txt` in TheRock's ten pinned submodules, at the pins in the captured `therock_manifest.json`.
- **Committed:** the script, `measurements/downloads.py`, and its output, `measurements/downloads.json`. The script records each `file(DOWNLOAD)`, its URL form, and whether it resolves under FR-001.

## R1. What declared downloads exist (measured)

**26 `file(DOWNLOAD)` calls in 17 files:**

| Submodule | Calls |
|---|---|
| rocm-libraries | 19 |
| rocm-systems | 4 |
| llvm-project | 2 |
| libhipcxx | 1 |

**12 resolve under FR-001:**

| Download | Kind | In a loop over a literal list | Paired digest list |
|---|---|---|---|
| `rocsparse/cmake/ClientMatrices.cmake`, `rocsparse_clientmatrices.cmake`: `https://sparse.tamu.edu/MM/${m}.tar.gz` + `cise.ufl.edu` fallback | matrices | yes, `TEST_MATRICES` (24) | `TEST_MD5HASH` |
| `hipsparse/cmake/ClientMatrices.cmake`, `hipsparse_clientmatrices.cmake`: the same two URLs | matrices | yes, `TEST_MATRICES` (19) | `TEST_MD5HASH` |
| `rocroller/GPUArchitectureGenerator`: `gpuopen.com/download/machine-readable-isa/latest` | an ISA description (data, unversioned) | no | no |
| `rocprofiler-sdk` benchmark: timemory `timem` installer `.sh` | tool | no | no |
| `rocprofiler-sdk` docs: Miniforge installer `.sh` | tool | no | no |
| `rocprofiler-sdk` tests: `trace_processor_shell` | tool | no | no |

**The 14 that do not resolve** use variables that are not same-file literal lists: `${mat}` in mirror and local paths, `$ENV{…}`, `${rocm_cmake_url}`, `${CCCL_MINIMUM_VERSION}` and similar. They are counted (FR-004).

## R2. The dataset rule (FR-008)

**Decision:** a resolved download is a dataset when it sits in a loop over a literal list defined in the same file **and** a digest list is paired with it element by element and checked by `file(MD5|SHA…)`.

**Rationale:**
- On the measured population this classifies exactly the 8 matrix calls: 4 files, each with a primary and a fallback URL.
- It classifies none of the 3 tool downloads.
- A declared list of named things, each with its own expected digest, fetched one by one, is a collection of data files by its shape. A fetched dependency is declared one at a time (`FetchContent_Declare`).

**What it misses:** the `machine-readable-isa/latest` file. It is a single literal URL with no digest, so it is counted as a resolved download that is not classified, per the clarification. That is a gap, never a wrong type.

**Alternatives rejected:**
- *Every resolved download:* it types three tool installers as data.
- *Host allow-lists* (`sparse.tamu.edu`): a name guess, with one host as witness.
- *Variable-name heuristics* (`TEST_MATRICES`): a name guess.

**The public corpus cannot test false positives.** None of its 21 targets is a CMake project (all are Go, Haskell, image, Maven, Nix, npm, Pants, Python or Rust). It can only confirm byte-identity (SC-003).

## R3. Identity

**Decision:** the dataset's identity is the **list element**, not the URL. A fallback URL for the same element is the same dataset (US1 scenario 5), and the same element declared in rocSPARSE's and hipSPARSE's files is the same dataset.

**The PURL** is `pkg:generic/<name>` with these parts:
- **namespace:** the element's part before the last `/`, when there is one. For `Williams/mac_econ_fwd500`, `Williams` is the namespace and `mac_econ_fwd500` the name.
- **`download_url`:** the first resolved URL;
- **`checksum`:** `<alg>:<hex>`, from the paired digest list.

No version: the declaration states none.

**A literal-URL download**, if one is ever classified, would take its name from the URL's last path segment, minus archive extensions.

## R4. SPDX 2.3 bridge

**Decision:** reuse C160 `waybill:component-kind`, a closed enum documented as extensible "without new C-rows", with a new value, `dataset`. Its two consumers (`graph_completeness`, `v3_document`) match only `lockfile-resolve`, so the new value triggers neither. `primaryPackagePurpose` is `OTHER`.

## R5. Coverage

**Decision:** a new document-scope row, C229 `waybill:declared-downloads`, holds the counts FR-007 asks for:

```json
{"datasets":N,"digest-mismatches":N,"not-classified":N,"read":N,"unresolved":N}
```

It is present only when any `file(DOWNLOAD)` was read. No existing row counts CMake downloads.

## R6. Tier, `datasetType`, version

**Tier:**
- `source` when a digest is declared, as the CMake reader does for `URL_HASH` fetches;
- `design` otherwise. This does not arise under R2.

**`datasetType`:** `noAssertion` (FR-006). Nothing in a download declaration says what kind of data it is. Calling a sparse matrix `numeric` or `graph` would be a guess.

**Version:** absent, using the existing handling.

## R7. Discovery

The CMake reader parses only the root `CMakeLists.txt`, `cmake/`, `Modules/` and `third_party/` (`cmake.rs::discover_cmake_files`; #1516). rocSPARSE's declarations are in `projects/rocsparse/cmake/`, so:
- **SC-001's scans** use `projects/rocsparse` and `projects/hipsparse` as their roots;
- **a monorepo-root scan** of `rocm-libraries` reaches them only after #1516.

This is recorded as a dependency, not worked around.
