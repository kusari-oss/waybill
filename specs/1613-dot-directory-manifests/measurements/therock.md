# T021 TheRock source tree

Final measurement: `kusari-sandbox/waybill-fixture-therock-sdk` measure-source, two runs at branch `b8b840fe` against `main` `4281e826`, TheRock `b1cae390` with submodules:
- 44426794245;
- 44426810581.

`d87f5d05` (the PR's last code commit) differs from `b8b840fe` only by folding two identical `if` branches for clippy. Its behaviour is the same, and the full gate and M15 were re-run on it.

Earlier rounds found the defects recorded in `research.md`:
- R9 (quality corpus, `bc13f385`);
- R10 (run 44418718300, `38c4babb`);
- R11 (run 44423036451, `bbeb4c5c`).

## Scan time (SC-005)

| Run | `main` median | branch median | ratio |
|---|---|---|---|
| 44426794245 | 23.727 s | 23.778 s | 1.002 |
| 44426810581 | 29.571 s | 29.550 s | 0.999 |

Three interleaved runs per arm, each after a warm-up. Earlier rounds measured 1.010 (`38c4babb`) and 0.995 (`bbeb4c5c`). The absolute times vary by runner: 19.9 s to 33.6 s for the same code. That is why only the within-run ratio is compared.

## Components (SC-001, SC-002)

| | `main` | branch |
|---|---|---|
| non-file components | 2,271 | 2,299 |
| all components | 3,538 | 3,566 |
| edges | 3,469 | 3,542 |
| unreachable, no orphan reason | 0 | 0 |

- **New:** 28, all non-runtime (24 `ci`, 4 `docs`).
- **Gone:** none.
- **Shared, scope changed:**
  - **To non-runtime: 19.** Docs and CI tooling (cryptography, pyjwt, requests, …) declared optional in two files lost its scope on `main` (R9 §1); Conan `gtest` and `benchmark` likewise. Their dependencies then pick up C62 `transitive` (spec 1269).
  - **To runtime: 5.** Each one is a declaration `main` dropped by merging first-wins (R9 §2):
    - **boto3:** `build_tools/packaging/linux/requirements.txt`.
    - **perfetto:** `rocm-systems/projects/rocprofiler-systems/requirements.txt`.
    - **pytest-timeout:** `rccl/test/ext-plugins/requirements.txt`. The classifier checks only the immediate parent directory, and `ext-plugins` is not a test name.
    - **sphinx, myst-parser:** `rocprofiler-compute/docs/archive/requirements-doc.txt`. The classifier knows `docs`, not `doc`, and checks only the immediate parent (`archive`). The file is therefore runtime under the existing rules, which this spec did not change. **This is a classifier gap, filed as #1614**; it already exists on `main`.

**SC-001:** all 8 requirements files under `.ci`, `.github` and `.sphinx` are read. Every package in them is non-runtime, unless an unscoped file also declares it (FR-006). Each non-runtime package carries C155, or C62 `transitive` where spec 1269's propagation reached it.

**SC-001a:** corrected in the spec. Both `dev-requirements.txt` files are cget files; the #1522 check rejects them and nothing is emitted.

**SC-002:** no source under `.upstream-tests`, `._upstream`, `.gitlab-ci`, `.jenkins` or `.claude`, and no file-tier component under any dot-directory.

## Formats and repeats (SC-004)

- **`waybill sbom parity-check`** on both sides reports the same 3 gaps (A12, B1, B2: count rows that differ on `main` too). C42, C62 and C155 pass. There are no duplicate `bom-ref`s. At `38c4babb` the branch had 6 gaps; R10 closed the extra 3.
- **Two runners at one commit:** CycloneDX is byte-identical (serial and timestamp removed), for `main` and the branch. SPDX 2.3 and SPDX 3 differ only in `annotationDate`, `creationInfo.created` and `created`.
