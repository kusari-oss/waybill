# TheRock source tree: #1566 and #1538 in CI (SC-006)

**Workflow:** `measure-source.yml` in `kusari-sandbox/waybill-fixture-therock-sdk` (fixture commit `78531e9`). Report script `census/source_report.py`, validated locally on the spec-1537 arms (it reproduces `repro.md`'s c2 disagreement).
- TheRock is cloned at `b1cae390cc745283dce25581415353f9bc906162` (#1538's pin), in CI only.
- **Scans:** `--offline sbom scan --root-name TheRock --root-version 10.3.0` in all three formats, before and after `git submodule update --init --depth 1`.

**Counting rule (#1538's):** PyPI components whose `waybill:source-files` include a top-level `requirements*.txt`, or a path under `build_tools/` or `external-builds/`.

## Baseline at `main` (T003)

Run 43931355085. `base_ref = waybill_ref = 1d25955c`: the same commit on both sides is the determinism control.

| side | mode | own-declared PyPI | with incoming edge | reachable from root | with orphan reason | unreachable, no reason | root targets cdx / spdx-2.3 / spdx-3 | formats agree | verdict |
|---|---|---|---|---|---|---|---|---|---|
| base | nosub | 69 | 0 | 0 | 0 | 69 | 11 / 11 / 150 | **no** | partial |
| base | sub | 66 | 4 | 4 | 0 | 62 | 75 / 75 / 75 | yes | partial |
| branch | nosub | 69 | 0 | 0 | 0 | 69 | 11 / 11 / 150 | **no** | partial |
| branch | sub | 66 | 4 | 4 | 0 | 62 | 75 / 75 / 75 | yes | partial |

**What the baseline shows:**
- **Both sides are identical,** so the measurement is deterministic.
- **Without submodules (#1566):** all 69 own-declared PyPI components are unreachable, with no reason. The formats disagree on the root's direct targets: CycloneDX and SPDX 2.3 list 11, the submodules, while SPDX 3's fallback still fires and lists 150. That is research R1's anchor-ordering defect at full scale. The reporter counted 73 orphans across all ecosystems; this rule counts PyPI only.
- **With submodules (#1538):** 62 of 66 are unreachable with no reason, exactly the reporter's figure. The formats agree here, so the cause is the gate being closed by the root's other edges (the C-3/C-5 mechanism), not the anchor-ordering split.

**Cost:** the TheRock checkout with submodules is 17 GB. Free disk went from 108 GB to 89 GB after cleanup. Single runs, not a median: scanning with submodules took 160 s (base) and 167 s (branch); without, 0.9 s.
