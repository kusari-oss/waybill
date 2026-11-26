# After: spec 1659

## Local (Apple M5 Max, idle, each test alone, 3 runs; `runs.tsv`)

| test | before min / median / max | after min / median / max |
|---|---|---|
| `sbom_edit_filter::i1454…` | 33.39 / 33.79 / 34.27 s | 10.44 / 10.47 / 10.54 s |
| `closure_runtime_scope::us4…` | 1.78 / 1.81 / 1.84 s | 1.83 / 1.84 / 1.91 s |
| four self-scan tests (sum of medians) | 4.82 + 4.89 + 4.83 + 4.82 = 19.36 s | one test: 4.84 / 4.90 / 4.95 s |

- **SC-001 missed by 0.3 s.** i1454 plus `us4` comes to 12.31 s against a target of 12 s. The target assumed 9.5 s for i1454's edits and schema checks, from one instrumented run; the median is 10.47 s. Before, the two together took 35.60 s.
- **SC-002 met.** One scan instead of four, 19.36 s → 4.90 s.

## Gate

`GATE_RESULT=pass`, `9299 tests run: 9299 passed, 56 skipped`. That is the previous 9,302, minus the 4 removed tests, plus 1 new one. Doctests: 0 passed, 1 ignored, unchanged.

## Coverage (`mutations.md`)

- **#1725:**
  - On the reference-removal path, every non-conformance produced (M1–M6) is caught by the JSON schema check or `us4`'s new assertions. None needs `spdx3-validate`, and `us4` still runs it on 40 KB.
  - The new control in `us4` fires if its input stops carrying the reference (C1).
- **#1716:**
  - Each of the four guards fails under an injected defect, and two defects give one failure naming both.
  - The CPE and version-string guards also fail when waybill's own OpenSSL detector is loosened to match the binary (P3).
  - P1 and P2 passed because of the mutations themselves, not the guards: the anchor needs a NUL before it, and the parser needs `X.Y.Z`.

## CI, Linux (`ci.tsv`)

The before runs are the first `main` run after spec 1658 (45737629137) and spec 1658's last PR run (45731418569, same test tree). The after runs come from this PR.

| run | phase | `Tests` step | summed test time |
|---|---|---|---|
| 45737629137 | before | 529 s | 1,434 s |
| 45731418569 | before | 676 s | 1,817 s |
| 45741984929 | after | 633 s | 1,638 s |
| 45741984929 (re-run) | after | 618 s | 1,620 s |

The step totals vary by about 25% between the two before runs, more than the work removed, so they show no effect either way. The affected tests' own Linux times (nextest per-test durations, before run 45737629137, after run 45741984929 re-run) do:

| test | before | after |
|---|---|---|
| i1454 | 105.95 s | 38.75 s |
| `us4` | 2.79 s | 4.16 s |
| self-scan guards | 18.50 + 21.47 + 22.15 + 21.63 = 83.75 s | 14.96 s |
| **total** | **192.49 s** | **57.87 s** |

That is 134.6 s less CPU per Linux run, about 34 s of wall time at the observed 4-way parallelism.

## CI failure on the re-run (not caused by this change)

macOS failed one test on the re-run, `scan_pip::scan_pip_dynamic_version_uses_placeholder`. It read another test's SBOM (`editable-pkg@1.0.0`) through an output path taken from a dropped `NamedTempFile`, which nothing reserves once dropped. Filed as #1727, which lists 21 such call sites. The failed job was re-run.
