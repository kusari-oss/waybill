# TheRock measure-source (T028: SC-005, SC-006)

Run 44918350832 of `measure.yml` in the TheRock SDK fixture repository: `--ref main -f waybill_ref=ebbffb21… -f base_ref=4eec5a4…`. The single `measure` job succeeded.

`refs.txt` of the artifact:

```
base 4eec5a4ccc195c6b438244875f6c40ac7390f262
branch ebbffb21f28cda2272c9ff38398a19e4a99b9884
therock b1cae390cc745283dce25581415353f9bc906162
```

One source change follows ebbffb21. b57f0c36 removes the C56 skip from the annotation fold in `deduplicator.rs`; mutation M9 (`mutations.md`) showed no test could observe it. Run 44925390727 repeats this measurement with the final source (cbac1650) against ebbffb21 as base, so the comparison changes one variable. Its result is at the end: output identical, time ratio 0.998.

## Time (SC-006)

These are the `times.txt` figures for the submodule scan. Each side gets a warm-up and then 3 timed runs:

| side | run 0 | run 1 | run 2 | median |
|---|---|---|---|---|
| `main` | 38.486 s | 38.578 s | 38.878 s | 38.578 s |
| branch | 38.463 s | 38.449 s | 38.416 s | 38.449 s |

The ratio of the medians, branch over `main`, is **0.997**, within the 1.05 ceiling. The warm-ups took 82.3 s for `main`, the first scan on a cold cache, and 39.2 s for the branch. They are excluded.

## Component by component (SC-005)

Every component was compared, counting nested components recursively and including `metadata.component`. Each side's CycloneDX document was compared:
- with the C56 property removed from every `properties[]`;
- ignoring `serialNumber` and `metadata.timestamp`.

| mode | components (`main` / branch) | C56 (`main` / branch) | document identical once C56 is removed |
|---|---|---|---|
| submodules | 4,215 / 4,215 | 0 / 84 | yes |
| no submodules | 266 / 266 | 0 / 1 | yes |

The C56 count is the same in all three formats:
- CycloneDX properties;
- SPDX 2.3 annotations;
- SPDX 3 annotations.

The counts are 84 / 84 / 84 with submodules and 1 / 1 / 1 without (FR-005, FR-006, FR-011). No output on either side has C242 or the `waybill-source-mechanism` methods key. `metadata.component` carries no C56 in either mode, since the root project absorbs no member with a different mechanism here.

**Every change is a C56 addition (FR-001, FR-004).** No C55, PURL, version, edge or other property changes, and no component appears or disappears (FR-003, FR-010).

The 84 additions in the submodule scan, by survivor C55 and C56:

| C55 | C56 | components |
|---|---|---|
| `cmake-project` | `["cmake-find-package"]` | 56 |
| `git-submodule` | `["cmake-find-package"]` | 7 |
| `cmake-find-package` | `["cmake-pkg-check-modules"]` | 6 |
| `cmake-externalproject` | `["cmake-fetchcontent-url"]` | 6 |
| `git-submodule` | `["cmake-find-package","cmake-project"]` | 2 |
| `git-submodule` | `["cmake-find-package","cmake-pkg-check-modules"]` | 1 |
| `git-submodule` | `["cmake-fetchcontent-git"]` | 1 |
| `git-submodule` | `["cmake-externalproject","cmake-project"]` | 1 |
| `git-submodule` | `["cmake-externalproject","cmake-fetchcontent-git"]` | 1 |
| `cmake-project` | `["cmake-pkg-check-modules"]` | 1 |
| `cmake-project` | `["cmake-find-package","cmake-pkg-check-modules"]` | 1 |
| `cmake-fetchcontent-git` | `["cmake-externalproject"]` | 1 |

The scan without submodules adds one: C55 `git-submodule`, C56 `["cmake-find-package"]`.

74 of the 84 list a package search (`cmake-find-package` or `cmake-pkg-check-modules`), the figure the CHANGELOG quotes. Grouped by C56 value alone, these counts equal the `census-merges` trace in `corpus.md` (63, 7, 6, 2, 2, 1, 1, 1, 1). That trace comes from separate runs, 44918370186 and 44918372804.

The two workflows count components differently: this one recursively over the whole document, the `census-merges` trace over its own scan. So the 4,215 here and that trace's 2,948 are not compared. What matters is that each side's own count is equal, `main` against branch.

## Orphans

From `report.md`, unreachable components without an orphan reason:

| side | no submodules | submodules |
|---|---|---|
| `main` | 0 | 0 |
| branch | 0 | 0 |

They do not increase. All four rows of the report are otherwise identical: 69 own-declared PyPI packages, all 69 reachable, root targets 46 / 630 with the formats agreeing.

## The final source (run 44925390727)

`refs.txt`: base ebbffb21f28cda2272c9ff38398a19e4a99b9884, branch cbac165012eb2e03f280648ec6bca95fe98b8cc8, TheRock b1cae390cc745283dce25581415353f9bc906162. The one source change between them is b57f0c36's removal of the fold's C56 skip. The single `measure` job succeeded.

All six documents were compared: CycloneDX, SPDX 2.3 and SPDX 3, with and without submodules. The comparison ignored the per-run fields: `serialNumber`, `metadata.timestamp`, `creationInfo`, `documentNamespace`, SPDX 3 `created`, and SPDX 2.3 `annotationDate`. Once those are masked, **all six are identical**, and C56 counts are 84 / 84 / 84 and 1 / 1 / 1 on both sides. Before masking, the SPDX 2.3 documents differ only in `annotationDate`: one second apart, in 30,710 annotations with submodules.

| side | run 0 | run 1 | run 2 | median |
|---|---|---|---|---|
| ebbffb21 | 38.331 s | 38.324 s | 38.328 s | 38.328 s |
| cbac1650 | 38.253 s | 38.255 s | 38.319 s | 38.255 s |

Ratio 0.998. Orphans without a reason: 0 on every row. The removal changes neither output nor time, so the comparison against `main` above holds for the final source.
