# T018, T019: public and quality corpora (SC-004)

Final source: `1474a600`. Later commits change only test goldens and `specs/`, so the measurements describe the shipped source.

## Public corpus

Run 44975674649 of `public-corpus.yml`, dispatched `--ref main -f branch=1642-requirements-owner-editable -f regen_goldens=true`. The scanned commit `1474a600` is read from the log.
- **Attempt 1: `Run public corpus` failed.** One target, `nix-closure-slacklinker`, failed class `corpus-infra`, invariant `nix-realise`: the runner could not build the pinned Nix output. The other 165 targets passed. `Report corpus failure` was skipped (scheduled runs only).
- **Attempt 2 (re-run of the failed job): success**, `test result: ok. 166 passed; 0 failed`, with `corpus_nix_closure_slacklinker ... ok`.

**Goldens.** Attempt 1's `corpus-goldens-regen` held 66 JSON documents. Each was compared with `cmp` against its committed copy:
- **3 differ, all `python-flask`.** Each differs in one value, `waybill:source-files`, which goes from `["uv.lock"]` to `[".","uv.lock"]` (CycloneDX property, SPDX 2.3 comment, SPDX 3 statement). Flask's main module is merged with its `uv.lock` entry (FR-001). These are committed in `a7360de7`.
- **The other 63 are identical.** Slacklinker's three are not evidence from attempt 1, because that target failed before its scan.

Attempt 2's 66 documents, slacklinker's included, were compared against the goldens as committed after `a7360de7`: **66 identical, 0 differ, 0 missing.**

## Quality corpus

Runs of `quality-corpus.yml`, both dispatched from `main`'s workflow:
- **branch:** 44975678562, scanned `1474a600`;
- **main:** 44975680694, scanned `79b3350b`.

Each run's `Measure SBOM quality corpus` succeeded, and `Report quality-corpus failure` was skipped. Both report 18 targets with no violations. The `measurements[]` arrays were compared with `wall_ms` left out. Every field is equal except `sbom_bytes` on two targets:

| target | `main` | branch |
|---|---|---|
| python-flask | 328,425 | 328,431 |
| rust-zizmor | 926,150 | 926,156 |

### Component by component

The quality report keeps no documents, so `census-owners.yml` (fixture repository) uploaded each target's CycloneDX document:
- **branch:** run 44975709870, at `1474a600`;
- **main:** run 44975715661, at `79b3350b`.

Every job's `refs.txt` names its waybill commit. Run 44975709870's five jobs for the targets with same-rank directories (therock, rust-ripgrep, ruby-rails, python-pytorch, maven-guice) failed in `census/owners.py` after the scan. The script read an `owner_candidates.step` field that the census amendment removed (fixed in fixture commit `a4b3082`). The scan had already written each document, and the upload step ran (`if: always()`), so all 19 documents are present. The comparison: every component's properties and scope, and every dependency edge.

| target | components (`main` / branch) | same `bom-ref`s | changed | edges −/+ |
|---|---|---|---|---|
| python-flask | 108 / 108 | yes | 1: `pkg:pypi/flask@3.1.2`, C18 `["uv.lock"]` → `[".","uv.lock"]` | 0 / 0 |
| rust-zizmor | 474 / 474 | yes | 1: `pkg:pypi/zizmor@0.0.0-unknown`, C18 the same | 0 / 0 |
| the other 16 quality targets | equal | yes | 0 | 0 / 0 |

The +6 bytes in each `sbom_bytes` are the `".",` added to C18. These are the two lockfile merges the census counted (`census.md`).
