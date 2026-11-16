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

## At the branch (T019, SC-006)

Run 43957429047. `base_ref = main` (`1d25955c`), `waybill_ref = e3dfd637`, with fixture commit `7a0ffb1`. That commit fixes the report's crash on a root target with no PURL. Run 43936614673 scanned `1e0fef2f` and died in that step.

| side | mode | own-declared PyPI | with incoming edge | reachable from root | with orphan reason | unreachable, no reason | root targets with a PURL, cdx / spdx-2.3 / spdx-3 | verdict |
|---|---|---|---|---|---|---|---|---|
| base | nosub | 69 | 0 | 0 | 0 | 69 | 11 / 11 / 150 | partial |
| base | sub | 66 | 4 | 4 | 0 | 62 | 75 / 75 / 75 | partial |
| branch | nosub | 69 | 69 | 69 | 0 | **0** | 84 / 84 / 150 | partial |
| branch | sub | 69 | 69 | 69 | 0 | **0** | 144 / 144 / 144 | partial |

**SC-006 is met.** Every own-declared PyPI component is reachable from the root, with and without submodules:
- **Without submodules (#1566):** 69 of 69, up from 0.
- **With submodules (#1538):** 69 of 69. At base, 62 of 66 were unreachable with no reason.

**I-1, by `root_targets.py`** (which keys file-tier targets by content hash):

| side | mode | cdx | spdx-2.3 | spdx-3 | |
|---|---|---|---|---|---|
| base | nosub | 11 | 11 | 171 | **differ**: 160 SPDX 3-only, #1566's anchor-ordering split |
| base | sub | 75 | 75 | 75 | agree |
| branch | nosub | 170 | 170 | 170 | agree |
| branch | sub | 144 | 144 | 144 | agree |

In the report's PURL-only column, branch nosub shows 84 / 84 / 150. That difference is naming, not graph: CycloneDX's 86 file-tier targets carry no PURL, and SPDX 3 gives them one.

**Own-declared count, 66 at base and 69 on the branch, with submodules.** This is research R5's pip merge. A package pinned in a submodule's requirements file and again in a top-level one used to keep only its first path, so #1538's rule could not see the top-level declaration.

**14 fewer components with submodules (4,261 → 4,247).**
- The unique PURL sets are identical: 4,014 on each side.
- The difference is file-tier entries named `requirements.txt`, 17 → 3.
- **Mechanism, reproduced with one variable:** take `requirements.txt` (`a`, `b`) and `tools/requirements.txt` (`a`), with `--root-name`.
  - At `main`, the pip merge drops `tools/requirements.txt` from `a`, so no component's occurrences cover that file, and it is emitted as an unattributed file-tier `requirements.txt`.
  - On the branch, it is `a`'s second source file, and no file-tier entry is emitted.
- Each of TheRock's 14 is now a package's source file.
- Inference, not traced: at base these 14 paths already appear in some component's `waybill:source-files`. A later pass that unions source files without unioning occurrences would explain it.

**Not compared:** wall time. These are single runs, 194 s at base and 147 s on the branch with submodules, and one run is not a measurement.

**Closes #1538:** 0 of 69 own-declared requirements components are stranded with submodules checked out, and the formats agree.
