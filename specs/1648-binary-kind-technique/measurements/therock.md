# T017: quality corpus and TheRock at the final code commit (US4, SC-004, SC-005, SC-006)

The final code commit is `bc7d589c715a6045c2acf0e2356edb2141b58770`. `main` is T001's control, `33c445f660b4f06a2d8d24007ad7004317697694`.

## Runs

| Run | Workflow (TheRock SDK fixture repository) | Dispatch | Jobs | Commit check |
|---|---|---|---|---|
| 45245317848 | `measure-full` | `--ref main`, base `33c445f6…`, branch `bc7d589c…` | 1 of 1 succeeded | the log names `BASE_REF: 33c445f6…` and `WAYBILL_REF: bc7d589c…` |
| 45245324519 | `census-versions.yml` | `--ref main`, `waybill_ref=bc7d589c…` | 20 of 20 succeeded | all 20 `refs.txt` name `bc7d589c` |

Every control in the census job passes in its wired state:
- `spec 1648 control ok: wired ['pkg:generic/libc.so.6'] 2 go-buildinfo`;
- `spec 1647 control ok: wired`;
- `spec 1646 control ok: wired`;
- `identity control ok` and `control ok`.

## TheRock: every changed component

From `measure-full`'s `base` and `branch` CycloneDX documents, compared component by component (keyed by `bom-ref`, nested components included):

| Measure | `main` | Branch |
|---|---|---|
| Components | 3,281 | 3,281, same `bom-ref`s |
| `dependencies` entries | 3,282 | 3,282, equal |
| Components whose technique changed | — | **694**: 645 `embedded-source-path` (15 distinct PURLs, each nested under a binary) and 49 `dynamic-linkage`, all `manifest-analysis` → `binary-analysis` |
| Components differing in anything else | — | **0** |

These match T010's census exactly: 49 / 49 and 645 / 15.

Whole documents, after `jq -S`:

| Format | Differing lines, UUIDs and timestamps masked | Also technique values masked |
|---|---|---|
| CycloneDX | 1,388 | **0** |
| SPDX 2.3 | 128 (64 D1 annotations: 49 + 15 packages) | **0** |
| SPDX 3 | 128 | **0** |

## parity-check

`waybill sbom parity-check --format json` on each arm's three documents. Both arms have 261 rows, with 260 passing. The one failing row is the same on both arms: A5 (author, person), which is restricted by design. The D1 row is identical between the arms: `binary-analysis` was already one of its values (spec 1646), so only which packages carry it changed.

## Quality corpus

Each census CycloneDX document at the log-only commit (run 45240732755, whose output equals `main`'s, `control.md`) was compared with the same document at the wired commit, by `bom-ref`, with technique values masked. That covers 23 documents: 17 quality-corpus targets, TheRock's census scan, and the control job's 5 trees.

| Document | Technique changes (`manifest-analysis` → `binary-analysis`) | Other differences |
|---|---|---|
| `pants-backend-ai` | 8 `dynamic-linkage` | 0 |
| `cpp-mongo` | 3 `dynamic-linkage` | 0 |
| TheRock's census scan | 49 `dynamic-linkage`, 645 `embedded-source-path` | 0 |
| control `kind` tree | 1 `dynamic-linkage` (`libc.so.6`) | 0 |
| the other 15 quality-corpus targets and 4 control trees | 0 | 0 |

In TheRock's census document, 547 further components differ only because a component nested inside them changed. A check over every changed component found that all 547 are ancestors of a changed `embedded-source-path` or `dynamic-linkage` component, and none differs in its own fields.

## The `binary_merge` debug count (analyze L2)

The analysis expected spec 1646's `binary_merge` debug count to rise once `dynamic-linkage` became binary. Counted from the census JSON:

| Target | Log-only | Wired |
|---|---|---|
| TheRock SDK | 23 | 23 |
| pants-backend-ai | 2 | 2 |
| cpp-mongo | 0 | 0 |

It did not rise, because no deduplicator merge involves either kind (`census.md`: `kind_merge` 0 everywhere; repeated imports are joined before deduplication).

## SC-006: timing

TheRock SDK scan, wall-clock seconds, from `timing.json`. Each arm had one warm-up (base 35.83, branch 28.49). The runs were interleaved: base, branch; branch, base; base, branch.

| Arm | min | median | max |
|---|---|---|---|
| `main` `33c445f6` | 25.67 | 25.85 | 26.07 |
| branch `bc7d589c` | 25.65 | 25.85 | 25.91 |

**Median ratio: 1.000**, within 1.05.
