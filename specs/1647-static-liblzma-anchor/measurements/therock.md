# T016: quality corpus and TheRock at the final code commit (US4, SC-006, SC-007)

The final code commit is `541e2bff22e340a829c522fad3fd7e71e872dc7f`. `main` is T001's control, `b6ad3c26bb57b1c7f4aeeda6a9d13d75242fe95e`.

## Runs

| Run | Workflow (TheRock SDK fixture repository) | Dispatch | Jobs | Commit check |
|---|---|---|---|---|
| 45206545886 | `measure-full` | `--ref main`, base `b6ad3c26…`, branch `541e2bff…` | 1 of 1 succeeded | the log names `BASE_REF: b6ad3c26…` and `WAYBILL_REF: 541e2bff…` |
| 45206551405 | `census-versions.yml` | `--ref main`, `waybill_ref=541e2bff…` | 20 of 20 succeeded | all 20 `refs.txt` name `541e2bff` |

Every control in the census job passes in its wired state:
- `spec 1647 control ok: wired 5.4.5 ['pkg:generic/liblzma@5.4.5'] no stripped event`;
- spec 1646's `spec 1646 control ok: wired`;
- spec 1645's `identity control ok`;
- spec 1644's `control ok`.

## TheRock: no change

`measure-full`'s `base` and `branch` documents, compared in full after `jq -S`, with version-4 UUIDs and RFC 3339 timestamps masked:

| Format | Differing lines, UUIDs masked | Also timestamps masked |
|---|---|---|
| CycloneDX | 2 (`metadata.timestamp`) | **0** |
| SPDX 2.3 | 50,510 (`annotationDate` on every annotation, and `created`) | **0** |
| SPDX 3 | 2 (`created`) | **0** |

The two arms' documents are identical apart from when each scan ran. That is the outcome the census predicted: no TheRock file defines a liblzma anchor in `.symtab` alone (`census.md`). Components, versions, CPEs, edges and evidence are all unchanged.

## parity-check

`waybill sbom parity-check --format json` on each arm's three documents. Both arms have 261 rows, with 260 passing. The one failing row is the same on both arms: A5 (author, person), which is restricted in SPDX 3 by design.

## Quality corpus

Each census document at the log-only commit (run 45201916920, whose output equals `main`'s, `control.md`) was compared with the same document at the wired commit, with UUIDs and timestamps masked. That covers 22 documents: 18 quality-corpus targets, TheRock's census scan, and the control job's 3 trees (`control`, `identity`, `static`).

- **21 identical**, including all 18 quality-corpus targets and TheRock's.
- **1 changed, as intended**: the control's `static` tree, the runner's real static build of `census/static_lzma/a.c`. It gained `pkg:generic/liblzma@5.4.5` and nothing else (PURLs at the log-only commit: `static-tree`, `waybill-fixture-xzstatic`, `waybill-fixture-xzstripped`; after: the same plus `liblzma@5.4.5`).

## SC-007: timing

TheRock SDK scan, wall-clock seconds, from `timing.json`. Each arm had one warm-up (base 33.03, branch 28.53). The runs were interleaved: base, branch; branch, base; base, branch.

| Arm | min | median | max |
|---|---|---|---|
| `main` `b6ad3c26` | 25.78 | 25.87 | 25.88 |
| branch `541e2bff` | 25.88 | 25.90 | 26.02 |

**Median ratio: 25.90 / 25.87 = 1.001**, within 1.05. The `.strtab` gate costs one substring search per ELF file, and the iteration it guards ran on no TheRock file.
