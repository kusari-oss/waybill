# TheRock source tree: why 841 unreachable components carry no marker (FR-001, SC-001)

**Document:** `measure-source` run 44149685219 in `kusari-sandbox/waybill-fixture-therock-sdk`, the branch side, `branch.sub.cdx.json`. That is the full 17 GB source tree with submodules, scanned at `cc85a5e9`, whose reader code is what `main` carries since #1599 (`b503bd6a`). The scan passes `--root-name TheRock --root-version 10.3.0`. It is a CI artifact, read locally; TheRock was not scanned locally.

**Instrument:** `probe_i7_on_document`, an ignored test in `waybill-cli/tests/public_corpus.rs`. It runs the corpus harness's own I7 predicate (`orphan_explained::is_explained`) and reachability (`cdx_reachable`) on the document, so nothing re-derives either.

## The three reachability answers

| measure | count |
|---|---|
| the document's `waybill:graph-completeness-reason` | 846 not reachable from root |
| I7: top-level components unreachable from the subject over `dependencies[]` | 2,522 |
| … of which file-tier (excluded from completeness by design) | 1,676 |
| … not file-tier | **846**: the same count as the document's |
| … explained by C151 | 5 |
| … carrying any `waybill:orphan-reason` | **0** |
| **unexplained** | **841** |

The document's completeness pass and the I7 check agree on the count, 846, on this tree. The source scan has no nested components.

## The classifier's reachable set is not the document's

The orphan classifier gives **every** unreachable npm component a code: `dead-lockfile-entry` with a reachable sibling, otherwise `hoisted-unused` (`classify_orphans`, the `("npm", _)` arms). The document has 4 unreachable npm components with no code. So the classifier counted them reachable.

That confirms #1593's inference from the code and the document together. The classifier is called with `RootComponentOverride::default()` and a target ref of `<name>@0.0.0`, while the emitters use the operator's root. Both inputs feed `select_root` and `build_ecosystem_root_set`, which decide the seed set, so the two passes start their walk from different seeds.

The exact seed that differs is to be confirmed by a contract test at implementation (T-trace). It is not needed to decide FR-002, because passing the emitters' root inputs to the classifier makes the two calls identical.

## Populations of the 841

| count | population | cause | rule that applies |
|---|---|---|---|
| 572 | `pkg:generic`, binary analysis, every file under a test-data directory (`test`, `tests`, `Inputs`, `unittests`, `testsuite`, `fixtures`, `test_assets`) | no rule covers binaries | new code (FR-004): test-data binary |
| 215 | `pkg:generic`, binary analysis, elsewhere: 212 `.co` GPU code objects in `rocm-libraries/dnn-providers/hip-kernel-provider/src`, 2 `.hsaco` in `rocm-systems/projects/rdc/rdc_libs`, 1 `.winmd` in a mesa fork | no rule covers binaries | new code (FR-004): the rest |
| 35 | `pkg:pypi` from `hipblaslt/tensilelite/uv.lock` and `rccl/bindings/nccl4py/uv.lock` | **a reader gap.** nccl4py's root entry is `source = { editable = "." }` with no `version` (dynamic), and the uv reader skips any entry with no version (`uv_lock.rs`, "uv.lock entry has no `version`; skipping"). Its `dependencies`, `optional-dependencies` (`cu12`, `cu13`) and `dev-dependencies` then have no incoming edge. tensilelite's root (`tensile@5.0.0`) is emitted; its unreached packages are to be attributed one by one. | FR-005: the reader emits the lock's own project and its edges |
| 8 | `pkg:cargo`, all from `rocm-systems/emulation/mirage/Cargo.lock`, each an older second version (`anyhow@1.0.102` beside `1.0.103`) | expected `dead-lockfile-entry` (Cargo.lock is in `LOCKFILE_NAMES`); not fired | FR-002, then re-measure |
| 4 | `pkg:npm` from LLVM tooling and a dashboard lockfile | classifier counted them reachable (above) | FR-002 |
| 2 | `pkg:conan` `gtest@1.11.0`, `benchmark@1.5.2`, from rocprim/hipcub/hiprand/rocrand `conanfile.py` | trace at implementation | FR-002 or FR-007 |
| 2 | `pkg:vcpkg` `zlib`, `zstd`, from hiptensor's `vcpkg.json` | trace at implementation | FR-002 or FR-007 |
| 1 | `pkg:deb` glibc, identified inside kpack's `test_assets/bundled_binaries` executables | binary-derived, under a test-data directory | FR-004's test-data rule, if it is stated over source files |
| 1 | `pkg:generic/tensile`, `workspace-root`, from tensilelite's `pyproject.toml` | the lock's project, unattached | FR-005 |
| 1 | `pkg:generic/wkmi@1.0.0`, from a CMake package-version file in `amdgpu-windows-interop` | trace at implementation | FR-007 |

The 572 / 215 split counts a binary as test data when **every** one of its source files lies under a test-data directory.
