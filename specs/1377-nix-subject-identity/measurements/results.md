# Milestone 1377 results (T035), 2026-11-09

Build `22fc4ee6` (debug), against the control baseline at `49a24a63` (`baseline-49a24a63/`).
nixpkgs `a799d3e3`, aarch64-darwin, grype 0.120.0 with DB schema v6.1.10 **built
2026-11-08T06:19:57Z** (the same DB as the baseline and the spike), Trivy 0.71.1,
osv-scanner 2.7.0. Produced by `gen-sboms.sh waybill`, `scan.sh`, `summarize.sh`
(`DEST=results-final`), `analyze.py` and `precision.py`. 50 of 51 scanner runs exit
0 or 1. The exception is osv on sbomnix's empty openssh runtime document, as in
the spike.

| SC | result | evidence |
| --- | --- | --- |
| SC-001 | **met** | All six subjects are the evaluated package: `openssh 10.3p1`, `curl 8.20.0`, `httpie 3.2.4` (`pkg:pypi`), `gh 2.93.0`, `ripgrep 15.1.0`, `moat 0.1` (`pkg:hackage`). CycloneDX `metadata.component`, SPDX 2.3 `documentDescribes` and SPDX 3 `rootElement` agree, checked on openssh and moat (tasks.md, US1 checkpoint) and by `tests/closure_subject.rs`. Files: `results-final/<target>.subject.json`. |
| SC-002 | **met** | grype: 28 findings on `openssh 10.3p1`, the same 28 CVE IDs as from sbomnix's document (0 differences). Baseline: 0. |
| SC-003 | **met** | All 17 nixpkgs-declared CPEs in sbomnix's `--exclude-cpe-matching` openssh document (16 members plus the subject) are carried exactly, including bash `gnu:bash:5.3:9` and gnugrep `gnu:grep`. waybill carries an 18th, `curl 8.20.0` → `haxx:curl`, reached by the name+version join. |
| SC-004 | **met** | The emitted bash CPE uses the nixpkgs encoding (version `5.3`, update `9`). That encoding at 5.0 patch 11 (`bash-cpe-encoding/nixpkgs-update-bare`) matches CVE-2019-18276 on this DB; the old curated form (`waybill-version-field`) matches nothing. |
| SC-005 | **met** | Every CPE-bearing closure component records its source natively in all three formats. openssh: 72 of 72; httpie: 263 of 263, including members re-identified as `pkg:pypi`. |
| SC-006 | **met** | No finding lost, on any of 6 targets × 3 scanners. Gained: openssh +28 grype (all on openssh 10.3p1); httpie +1 grype (`p11-kit 0.26.2`, via its declared CPE) and +1 osv (`PYSEC-2023-242` on httpie 3.2.4, which is in the document now that it is the subject). Already-patched false positives: 6 per target, unchanged. |
| SC-007 | **met for local goldens; corpus pending** | Every non-corpus golden passes unchanged under the full gate (373 targets ok, 0 failed). Of the corpus targets, only `nix-closure-moat` runs `--nix-closure`. |
| SC-008 | **probe met; golden pending CI** | Override-free probe: moat's subject is `pkg:hackage/moat@0.1`, heuristic `nix-evaluated-package`, no `waybill:` CPE. The corpus golden cannot be regenerated on this host: here the moat closure has 1,275 derivations, against the Linux golden's 1,749 (`elfutils` is Linux-only), so the harness's invariants fail locally for reasons of platform. It needs the `public-corpus.yml` dispatch with `regen_goldens` on `ubuntu-latest`. |
| SC-009 | **met** | C184 `identifiers` equals `meta-eval-cost/reach.py` on every field. openssh: 670 wanted, 189 received (80 drv + 109 name+version), 39 declared, 0 rejected, 481 unreached. gh: 722, 200 (91 + 109), 42, 0, 522. |

## Totals

| | grype findings, summed over targets | patched FP | runtime findings | runtime patched FP |
| --- | --- | --- | --- | --- |
| sbomnix (buildtime) | 1,475 | 115 (8%) | 288 | 1 |
| waybill at `49a24a63` | 1,203 | 36 (3%) | 232 | 1 |
| waybill at `22fc4ee6` | 1,232 | 36 (3%) | 260 | 1 |

Distinct grype findings across all six targets, deduplicated on (id, package,
version): 308 → 337.

## The largest remaining gap

From `analyze.py`/`adjudicate.py` on this run, the biggest runtime-relevant
misses are on components with no CPE from either source. The largest is
`python3 3.13.13`: 35 CVEs on httpie. nixpkgs gives a product and no vendor,
and the curated table has no row for it. That is outside this milestone.
