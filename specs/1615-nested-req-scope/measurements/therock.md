# T018: TheRock source tree

`kusari-sandbox/waybill-fixture-therock-sdk` measure-source runs 44550926162 and 44550947405: branch `fbb168ab` against `main` `52b96f1d`, TheRock `b1cae390` with submodules, CI only.

## Scan time (SC-007)

**1.000× and 1.007×** `main`'s median (3 interleaved runs after a warm-up). Budget ≤ 1.05×.

## Labels (SC-001, SC-002)

**Files.** Exactly 15 requirements files change label, read from the packages each file alone declares:
- the 14 (13 → `docs`, rocjitsu's `tests/corpus/dbt/requirements-gfx1250-dbt.txt` → `test`);
- `rocprofiler-compute/requirements-development.txt` → `dev` (FR-006).

Every other file keeps its label, including the 20 labelled today.

**Packages.** 665 pip requirements components on both sides, none added or removed. 265 required on `main`, 148 on the branch.
- **SC-001: 85 of 85** packages that were required on `main` and declared only in the 14 files are now non-runtime with C155 `docs` or `test`. The spec's prediction of 98 came from a path classification in the sizing script that also counted packages under other matches; 85 is the measured population.
- **Declared in the 14 files and elsewhere:** 17. 13 also appear only in other non-runtime files (already-labelled docs files, `.ci/`, `.sphinx/`). With no runtime declaration left, they become non-runtime by spec 1613's merge (FR-007 does not apply). 1 stays required.
- **`requirements-development.txt`:** `pre-commit` and `ruamel.yaml` become `dev`.

**SC-002 / FR-007 do not hold for 11 packages.** These packages are declared in runtime files but become non-runtime with C62 `transitive`:
- pygithub, pynacl@1.6.2 and numpy@1.26.4 are also declared in the 14 files;
- certifi, cryptography ×2, pyjwt ×2, pynacl@1.6.1, requests and typing-extensions are declared only in runtime files.

**Cause, traced in the two documents and the source.**
1. Spec 1269's propagation (`resolve/scope_propagation.rs`) protects a component only if a runtime path from its seed set reaches it.
2. The seed set (`graph_completeness::bfs::component_root_seeds`) is the primary root plus **one top main module per ecosystem**.
3. llvm's `utils/git` requirements files belong to the `pkg:pypi/llvm@0.0.0-unknown` project, and numpy's runtime file to `rocprof-compute`. Both are required main modules (`waybill:demoted-from-main-module`), but neither is the seeded pip main module.
4. On `main` their pins were reachable from the root only through runtime-labelled docs packages: `rocm-docs-core → pygithub → requests → certifi`, and `matplotlib → numpy`. With those packages now Optional, propagation reaches the pins from the docs seeds and assigns the docs packages' scope.

**Decision (2026-11-19, option A):** excepted in FR-007 and SC-002, tracked in #1618.

**This predates this feature.** Walking `main`'s emitted graph from all 64 main modules, over edges into components without a reader-declared non-runtime scope, about 250 components that spec 1269 marked `transitive` are reachable. On the branch it is 68. This walk approximates what an all-main-module seed set would protect; waybill did not compute it.

## Formats, identity, repeats (SC-003, SC-004)

- **`waybill sbom parity-check`:** the same count-row gaps as `main` (A12, B1, B2). C42, C62 and C155 agree across the formats.
- **Integrity:** 0 duplicate `bom-ref`s, and 0 unreachable components without an orphan reason, on both sides.
- **Runners:** CycloneDX, SPDX 2.3 and SPDX 3 are identical across the two runners once serial, timestamps and document namespace are removed.
