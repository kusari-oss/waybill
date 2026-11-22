# T018: quality corpus and TheRock at the final code commit (US3, SC-001, SC-005, SC-007)

The final code commit is `cd88e02b35c322b770f7906580ec5a999461964d`. The docs commit after it (`c7d556ea`) changes only the module's doc comment and `docs/`. `main` is T001's control, `bb9d6cc485aa13043891c39c50af70669dd40106`.

## Runs

| Run | Workflow (TheRock SDK fixture repository) | Dispatch | Jobs | Commit check |
|---|---|---|---|---|
| 45082092571 | `measure-full` | `--ref main`, base `bb9d6cc4…`, branch `cd88e02b…` | 1 of 1 succeeded | the log names `BASE_REF: bb9d6cc4…` and `WAYBILL_REF: cd88e02b…` |
| 45082095568 | `census-versions.yml` | `--ref main`, `waybill_ref=cd88e02b…` | 19 of 20 succeeded | all 20 `refs.txt` name `cd88e02b` |

The one failure in run 45082095568 is the `control` job's spec 1645 identity assertion. That assertion encoded the log-only state only. At the wired commit it reported `[('ncurses', 'merge')]` with no version-change or CPE-form events, which is the wired behaviour:
- LLVM's two PURLs are now equal, so there is no shared file to decide;
- the two census-only event kinds were removed at T012 and T013.

Fixture commit `0fa1b9c` makes the control accept either state. The 18 corpus jobs and the SDK job succeeded.

## Quality corpus: 18 targets, no change

Each target's CycloneDX components were compared between the log-only census (run 45080107747, whose output equals `main`'s: the public corpus passed 166 of 166 at that commit) and this run. The comparison covered PURL, CPE, version and every property, recursively, sorted.

| Target | Components | Differing |
|---|---|---|
| cmake-nlohmann-json | 26 | 0 |
| cpp-mongo | 727 | 0 |
| go-cobra | 8 | 0 |
| go-kubernetes | 795 | 0 |
| gradle-apache-solr | 1189 | 0 |
| gradle-bitwarden-android | 345 | 0 |
| maven-guice | 62 | 0 |
| npm-express | 45 | 0 |
| pants-backend-ai | 373 | 0 |
| pnpm-vue-core | 658 | 0 |
| python-ansible | 373 | 0 |
| python-flask | 109 | 0 |
| python-pytorch | 341 | 0 |
| ruby-rails | 765 | 0 |
| rust-ripgrep | 69 | 0 |
| rust-zizmor | 475 | 0 |
| uv-meilisearch-python | 31 | 0 |
| yarn-guac-visualizer | 901 | 0 |

No target was empty, so none of the zeros is vacuous.

## TheRock: every changed component

From `measure-full`'s `base` and `branch` documents: 3,284 components on `main` and 3,282 on the branch, counted recursively.

| `main` | Branch | Traced to |
|---|---|---|
| `pkg:generic/llvm@23.0.0`: banner match, files `libLLVM-23git.so`, `libLLVM.so`, `libLLVM.so.23.0git`; CPE `llvm:llvm:23.0.0` | **gone**, merged into `llvm@23.0.0git`, which gains `libLLVM-23git.so` (150 → 151 files) and keeps its mechanism (`cmake-package-version`) and CPE | FR-002: the banner states `23.0.0git`, so the PURLs are equal and the existing merge applies |
| `pkg:generic/ncurses@6.6`: banner match, files `libncursesw.so`, `librocm_sysdeps_ncursesw.so.6`; CPEs `…:ncurses:6.6:*` | **one** `pkg:generic/ncurses@6.6.20251230`, files `libncursesw.so`, `librocm_sysdeps_ncursesw.so.6`, `pkgconfig/ncursesw.pc`; mechanism `pkg-config`; CPEs `invisible-island`, `ncurses_project`, `gnu` `:ncurses:6.6:20251230` | FR-013 (date kept), FR-003 to FR-006 (`ncursesw` qualifies; equal versions; one record), FR-008 (CPE form) |
| `pkg:generic/ncursesw@6.6.20251230`: `ncursesw.pc`, files `libncursesw.so`, `ncursesw.pc`; no CPE | **gone**, merged into the above | as above |

Edges: 3,903 on `main` and 3,901 on the branch. `sdk@0.0.0 -> llvm@23.0.0` and `sdk@0.0.0 -> ncursesw@6.6.20251230` are gone, and `sdk@0.0.0 -> ncurses@6.6` became `-> ncurses@6.6.20251230`. Nothing else changed: no other component or edge, and `ncurses++w@6.6.20251230` (`ncurses++w.pc`, a different library) is untouched.

This is exactly census T010's prediction.

**No new duplicate.** `main` has 14 PURLs appearing more than once and the branch has the same 14: `diff` of the two lists, 14 lines each, found them identical. None of them is ncurses or llvm.

## parity-check

`waybill sbom parity-check --format json` was run on each arm's three documents with this branch's build. Both arms report 260 universal rows, 258 passing, and the same 2 gaps, which are rows restricted by design.

Changed rows, each by the same amount in every format:

| Row | `main` (cdx/spdx2.3/spdx3) | Branch | Change |
|---|---|---|---|
| A1 PURL | 2549/2549/2549 | 2547/2547/2547 | −2: `llvm@23.0.0` and `ncursesw@…` merged |
| A2 name | 2250/2250/2250 | 2249/2249/2249 | −1: the name `ncursesw` |
| A3 version | 91/91/91 | 89/89/89 | −2 |
| A12 CPE | 33/33/52 | 32/32/51 | −1: `llvm:llvm:23.0.0` merged away; ncurses keeps one primary CPE |
| B1 runtime edge | 2935/3045/3082 | 2933/3043/3080 | −2: the two root edges to merged components |

## SC-007: timing

TheRock SDK scan, wall-clock seconds, from `timing.json`. Each arm had one warm-up, and the runs were interleaved as: base warm-up, branch warm-up, base, branch, branch, base, base, branch.

| Arm | min | median | max |
|---|---|---|---|
| `main` `bb9d6cc4` | 20.89 | 21.17 | 21.38 |
| branch `cd88e02b` | 21.00 | 21.02 | 21.07 |

**Median ratio: 21.02 / 21.17 = 0.993**, within 1.05. The arms' ranges overlap.
