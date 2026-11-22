# T005: census at the log-only commit (SC-001)

## Runs

- **Census:** run 44973661105 of `census-owners.yml` in the TheRock SDK fixture repository (commit `085215b`), dispatched `--ref main -f waybill_ref=430426588640e8032b4780e31f7f2cf46b76504d`. All 19 jobs succeeded, and every job's `refs.txt` names waybill `43042658`. TheRock tree: `b1cae390cc745283dce25581415353f9bc906162`, with top-level submodules. The 18 quality-corpus targets were scanned at their `xtask/corpus/quality-corpus.toml` pins.
- **Public corpus:** run 44973663395, `regen_goldens=true`, scanned commit `43042658` (from the log). Jobs: `Run public corpus` succeeded; `Report corpus failure` was skipped (scheduled runs only). **66 of 66 regenerated JSON goldens are byte-identical** to the committed ones, so the commit is log-only. The public corpus's census is its regenerated diff at the wired commit (planning amendment).
- **Positive control:** T004, `t004_census_events_fire`, passed at the log-only commit.

`census/owners.py` reads only the events. Events seen twice in one scan (the owner index is built twice) are counted once.

## Results

| target | lockfile merges (R2 reorders) | same-rank dirs (by deciding step under R3 as planned) | lockfile-first candidates | non-runtime main modules (root / added seed) | anchor seeds | kept runtime only by anchors |
|---|---|---|---|---|---|---|
| therock | 4 (0 installed, 0 editable disagreements) | 12 (runtime 3, purl 9) | 2 | 351 (0 / 0) | submodule 11, declared 2 | 4 (pypi) |
| python-pytorch | 0 | 3 (purl 3) | 0 | 7 (0 / 0) | submodule 37 | 0 |
| maven-guice | 0 | 3 (purl 3) | 0 | 0 | 0 | 0 |
| ruby-rails | 0 | 1 (purl 1) | 0 | 0 | 0 | 0 |
| rust-ripgrep | 0 | 1 (purl 1) | 0 | 0 | 0 | 0 |
| python-flask | 1 | 0 | 0 | 0 | 0 | 0 |
| rust-zizmor | 1 | 0 | 0 | 0 | 0 | 0 |
| gradle-apache-solr | 0 | 0 | 0 | 0 | declared 11 | 0 |
| gradle-bitwarden-android | 0 | 0 | 0 | 0 | declared 1 | 0 |
| cmake-nlohmann-json | 0 | 0 | 0 | 5 (0 / 0) | 0 | 0 |
| the other 9 | 0 | 0 | 0 | 0 | 0 | 0 |

The 9 targets with nothing to report: cpp-mongo, go-cobra, go-kubernetes, npm-express, pants-backend-ai, pnpm-vue-core, python-ansible, uv-meilisearch-python and yarn-guac-visualizer.

### TheRock, in detail

**#1640's shape is on TheRock.** In `rocm-libraries/projects/hipblaslt/tensilelite`, `pkg:pypi/rocisa@0.1.0` (its C122 is `pep-735-dependency-groups:dev`) and `pkg:pypi/tensile@5.0.0` both claim the directory from `tensilelite/uv.lock`. Today `rocisa` owns it by name order. R2 gives `rocisa` its own directory (`editable = "rocisa"`), which removes the tie.

**The 12 same-rank directories:**
- **pip:** `tensilelite`, above. It is the 2 lockfile-first candidates.
- **Cargo workspaces** (`rocm-systems/emulation/mirage`, `rocm-systems/projects/rccl/contrib/nccl4rust`, `rocm-systems/runtimes`): every member's first path is the workspace's `Cargo.lock`, so all of them claim the workspace directory and the PURL decides. This is the same defect in the Cargo reader. It is out of this spec's scope (pip), and needs a follow-up issue.
- **CMake projects of one directory** (8): `compiler/amd-llvm/amd/comgr`, `compiler/amd-llvm/flang`, `debug-tools/rocgdb`, `examples/cpp-sdk-user`, `rocm-libraries/dnn-providers/hip-kernel-provider`, `rocm-libraries/projects/hipblaslt`, `rocm-systems/projects/rccl-tests`, `rocm-systems/projects/rocdecode/samples/rocdecDecode`. Spec 1618 treats them as siblings.

**The FR-003 runtime step, as planned, decides 3 of these:**
- `tensilelite`, which R2 already resolves;
- `examples/cpp-sdk-user`, where the owner is unchanged;
- `rocdecode/samples/rocdecDecode`, where it would move ownership from the sample's own project (`pkg:generic/rocdecdecode`, Development by its `samples` directory) to a sibling `project()` in the same `CMakeLists.txt` (`pkg:generic/rocdecode-host@1.0.0`).

**Non-runtime main modules: 351.** Of these, 349 are CMake projects scoped by their directory (spec 1618), 1 is `rocisa` (`pep-735-dependency-groups:dev`), and 1 is `pip-optional-dependencies`. None is chosen as the document root, and none is an added seed.

**Kept runtime only by anchors:** `pkg:pypi/attrs@25.4.0`, `pkg:pypi/numpy@1.24.2`, `pkg:pypi/python-dateutil@2.8.2` and `pkg:pypi/six@1.16.0`. #1640 reported five root-declared packages. Which four of those these are, and the fifth, are checked in T019 against the TheRock measure-source diff.

## Decisions the census settles

- **FR-002: keep the main-module role.** No dev-only main module is a root or an added seed anywhere. The one that owned a directory (`rocisa`) loses it to R2.
- **FR-003: the runtime step is dropped** (amendment). After R2 it decides no pip directory, and on TheRock it would move a CMake sample's directory to a sibling project. The remaining ties are decided by PURL and logged (`waybill::owner_index`): 9 on TheRock (3 Cargo, 6 CMake), 8 across the quality corpus.
- **R2's installed exemption stands:** no installed merge occurs in any target.
- **No editable disagreement** anywhere.
