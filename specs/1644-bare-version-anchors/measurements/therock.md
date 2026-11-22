# T018: quality corpus and TheRock at the final code commit (US4, SC-007)

The final code commit is `b94a5a19e27dcb8d673b9250b8a08f6241eb6438`. The two commits after it change no code: one adds the mutation records and one changes only the module's doc comment. `main` is T001's control, `110d1a5895cf5fc10bfd42c78f5f36d09be33093`.

## Runs

| Run | Workflow (TheRock SDK fixture repository) | Dispatch | Jobs | Commit check |
|---|---|---|---|---|
| 45039486997 | `measure-full` | `--ref main`, base `110d1a58…`, branch `b94a5a19…` | 1 of 1 succeeded | the log checks out and fetches both full SHAs; `BASE_REF: 110d1a58…`, `WAYBILL_REF: b94a5a19…` |
| 45039489189 | `census-versions.yml` | `--ref main`, `waybill_ref=b94a5a19…` | 20 of 20 succeeded | all 20 `refs.txt` name `b94a5a19…`; SDK tarball `efeb6b07…`, the pinned one |

The quality corpus's `main` documents are census 1's (run 45035417841, at `62198549`). Its output equals `main`'s: the public corpus passed 166 of 166 at that commit, and nothing was emitted from the new code there (`census.md`).

## Quality corpus: 18 targets, no change

Each target's CycloneDX components were compared between census 1 and the branch: PURL, CPE, version and every property, recursively, sorted.

| Target | Components (`main`) | Components (branch) | Differing |
|---|---|---|---|
| cmake-nlohmann-json | 26 | 26 | 0 |
| cpp-mongo | 727 | 727 | 0 |
| go-cobra | 8 | 8 | 0 |
| go-kubernetes | 795 | 795 | 0 |
| gradle-apache-solr | 1189 | 1189 | 0 |
| gradle-bitwarden-android | 345 | 345 | 0 |
| maven-guice | 62 | 62 | 0 |
| npm-express | 45 | 45 | 0 |
| pants-backend-ai | 373 | 373 | 0 |
| pnpm-vue-core | 658 | 658 | 0 |
| python-ansible | 373 | 373 | 0 |
| python-flask | 109 | 109 | 0 |
| python-pytorch | 341 | 341 | 0 |
| ruby-rails | 765 | 765 | 0 |
| rust-ripgrep | 69 | 69 | 0 |
| rust-zizmor | 475 | 475 | 0 |
| uv-meilisearch-python | 31 | 31 | 0 |
| yarn-guac-visualizer | 901 | 901 | 0 |

No target was empty, so none of the zeros is vacuous. As census 1 and 2 found, none of these targets has an anchored binary.

## TheRock: every changed component

From `measure-full`'s `base` and `branch` documents: 3,283 components on `main`, 3,284 on the branch (recursive count).

| Component | `main` | Branch | Traced to |
|---|---|---|---|
| `pkg:generic/bzip2@1.0.8` | absent | **added**, CPE `bzip:bzip2:1.0.8`, `waybill:evidence-kind` `embedded-version-string`, files `lib/rocm_sysdeps/lib/libbz2.so` and `librocm_sysdeps_bz2.so` | the 2 bzip2 binaries, anchor `bzip2/libbzip2`, candidate `1.0.8, 13-Jul-2019` |
| `pkg:generic/gmp@6.3.0` | files `libgmp.so`, `gmp.pc`; no CPE | **adds** `librocm_sysdeps_gmp.so.10` to its files; CPE `gmplib:gmp:6.3.0` | the 2 GMP binaries (`libgmp.so` is a symlink to `.so.10`), anchor `GNU MP:`, candidate `6.3.0` after binary data |
| `pkg:generic/liblzma@5.8.1` | files: the cmake config, `liblzma.so`, `librocm_sysdeps_liblzma.so.5.8.1`, `liblzma.pc`; no CPE | **adds** `librocm_sysdeps_liblzma.so.5` to its files; CPE `tukaani:xz:5.8.1`, candidates `tukaani:xz` and `xz_project:xz` | the 3 liblzma binaries, defined symbols `lzma_version_string` and `lzma_version_number`, candidate `5.8.1` |

Nothing else was added, removed or changed. This is what census 2 predicted (`census.md`): bzip2 gained, and GMP and liblzma merged into their existing `.pc` components at the same version.

The branch's census (run 45039489189) logged 7 anchored events in 7 binaries, all `versioned`: bzip2 1.0.8 ×2, gmp 6.3.0 ×2 and liblzma 5.8.1 ×3. It found 0 ambiguous, 0 shared and 0 bzip2 date forms without the anchor. This matches census 2.

## FR-007: no second component for one library

- `main` has 14 PURLs that appear more than once in its document. The branch has the same 14, and none is bzip2, GMP or liblzma. The comparison was run on the two real lists: 14 lines each, and `diff` found them identical.
- The unversioned `pkg:generic/bzip2?file-sha256=adea0650…` for the executable `lib/rocm_sysdeps/bin/bzip2` remains, as on `main`. It is a different file from the two libraries.
- `pkg:generic/liblzma.so?file-sha256=15402b36…` is unchanged from `main`.

No follow-up issue is needed.

## parity-check

`waybill sbom parity-check --format json` ran on each arm's three documents, using the branch build. Both arms report 260 universal rows, 258 passing, and the same 2 gaps, which are rows restricted by design, such as A5 author.

These rows changed, each by the same amount in every format:

| Row | `main` (cdx/spdx2.3/spdx3) | Branch | Change |
|---|---|---|---|
| A1 PURL | 2548/2548/2548 | 2549/2549/2549 | +1: `bzip2@1.0.8` |
| A3 version | 90/90/90 | 91/91/91 | +1: `bzip2@1.0.8` |
| A12 CPE | 29/29/48 | 33/33/52 | +4: bzip2 1, gmp 1, liblzma 2 |
| C19 `waybill:cpe-candidates` | 43/43/43 | 45/45/45 | +2: liblzma's two candidates |
| B1 runtime edge | 2934/3044/3081 | 2935/3045/3082 | +1: `sdk@0.0.0 -> pkg:generic/bzip2@1.0.8`, the only edge added or removed (CycloneDX `dependencies`: 3,902 → 3,903) |

## SC-007: timing

TheRock SDK scan, wall-clock seconds, from `timing.json`. Each arm had one warm-up, and the runs were interleaved as: base warm-up, branch warm-up, base, branch, branch, base, base, branch.

| Arm | min | median | max |
|---|---|---|---|
| `main` `110d1a58` | 20.59 | 20.68 | 21.07 |
| branch `b94a5a19` | 20.74 | 20.78 | 20.83 |

**Median ratio: 20.78 / 20.68 = 1.005**, within 1.05. The arms' ranges overlap.
