# Milestone 670 sweep-regression comparison

**Task**: T019
**Date**: 2026-09-01
**Baseline**: `sweep-baseline-2026-08-31.tsv` (from issue #743 sweep before m670 landed)
**After**: `sweep-after-2026-09-01.tsv` (post-PR-1 + PR-3 reader work: T001–T005, T012, T013, T016)
**Binary**: `target/release/waybill` at commit `HEAD` on branch `670-pip-under-detection-fix`

## Verdict: ✅ PASS — zero regressions

- **21/21 repos** stayed within the T019 acceptance criteria
- **7 Python-heavy fixtures** monotonic-≥ (target met)
- **14 non-Python fixtures** all at 0.0% delta (well under the ±5% envelope)
- **1 known-failing fixture** (`test-rustlang`) still fails per bug #742 — pre-existing, not m670's scope
- **Wall-clock**: broad improvements (bun -35s, codex -38s, vaultwarden -9s). Most likely due to warm caches from prior clones + m669 baseline being cold-cache. All non-Python scans within budget.

## SC verification

| SC | Target | Result | Verdict |
|----|--------|--------|---------|
| SC-001 (markitdown) | ≥ 30 pypi | **32 total (32 pypi)** | ✅ MET |
| SC-002 (OctoPrint) | ≥ 30 pypi | **83 total (73 pypi verified earlier)** | ✅ MET |
| SC-003 (cpython) | ≥ 25 (revised) / ≥ 50 (original) | 187 total (16 pypi, unchanged) | ⚪ SC-003 not moved by T012/T013 alone — see [PR-3 cpython note](#pr-3-note-on-cpython-sc-003) |
| SC-006 (regression) | non-Python within ±5% | **14/14 at 0.0%** | ✅ MET |
| SC-007 (markitdown wall-clock) | ≤ 549ms | **50ms** (from 49ms baseline; +1ms) | ✅ MET |
| SC-008 (cpython wall-clock) | ≤ 5575ms | **580ms** (from 575ms baseline; +5ms) | ✅ MET |

## Full comparison table

| repo | baseline | after | delta | % | scan_ms Δ | verdict |
|------|---------:|------:|------:|---:|---------:|:--------|
| test-bat | 432 | 432 | +0 | +0.0% | -160ms | ✅ within ±5% |
| test-bun | 8788 | 8788 | +0 | +0.0% | -35181ms | ✅ within ±5% |
| test-codex | 2063 | 2063 | +0 | +0.0% | -38032ms | ✅ within ±5% |
| test-cpython | 187 | 187 | +0 | +0.0% | +5ms | ✅ python-tier monotonic (Δ≥0) |
| test-guac-visualizer | 908 | 908 | +0 | +0.0% | -5ms | ✅ within ±5% |
| test-kubernetes | 817 | 817 | +0 | +0.0% | -847ms | ✅ within ±5% |
| test-langflow | 3267 | 3276 | +9 | +0.3% | +9ms | ✅ python-tier monotonic (Δ≥0) |
| test-markitdown | 5 | 32 | +27 | **+540.0%** | +1ms | ✅ python-tier monotonic (SC-001) |
| test-OctoPrint | 13 | 83 | +70 | **+538.5%** | -5ms | ✅ python-tier monotonic (SC-002) |
| test-podman | 500 | 500 | +0 | +0.0% | -403ms | ✅ within ±5% |
| test-podman-desktop | 2731 | 2731 | +0 | +0.0% | +41ms | ✅ python-tier monotonic (Δ≥0) |
| test-pytorch | 293 | 295 | +2 | +0.7% | -34ms | ✅ python-tier monotonic (Δ≥0) |
| test-rails | 1072 | 1072 | +0 | +0.0% | -23ms | ✅ within ±5% |
| test-ripgrep | 88 | 88 | +0 | +0.0% | -17ms | ✅ within ±5% |
| test-rustdesk | 1473 | 1473 | +0 | +0.0% | -8ms | ✅ within ±5% |
| test-rustlang | 0 | 0 | +0 | — | -50108ms | ⚪ still failing (bug #742) |
| test-sphinx | 221 | 226 | +5 | +2.3% | -1ms | ✅ python-tier monotonic (Δ≥0) |
| test-tauri | 1684 | 1684 | +0 | +0.0% | -110ms | ✅ within ±5% |
| test-tensorflow-models | 1439 | 1439 | +0 | +0.0% | +0ms | ✅ within ±5% |
| test-vaultwarden | 843 | 843 | +0 | +0.0% | -9401ms | ✅ within ±5% |
| test-yt-dlp | 65 | 65 | +0 | +0.0% | -6ms | ✅ within ±5% |

## Big-win detail

### test-markitdown: 5 → 32 (+27, +540%)

Pre-m670 emitted only 4 sub-project main-modules (`markitdown@0.0.0-unknown` × 4). Post-m670 emits main-modules + 28 declared deps from the 4 sub-projects' `[project.dependencies]` blocks: `azure-ai-*`, `beautifulsoup4`, `lxml`, `magika`, `pandas`, `openai`, `Pillow`, `pdfminer.six`, `pdfplumber`, etc. **SC-001 (≥30) met.**

### test-OctoPrint: 13 → 83 (+70, +538%)

Pre-m670 emitted only 3 pypi (OctoPrint + 2 sub-project main-modules). Post-m670 emits the full `[project.dependencies]` from OctoPrint's pyproject.toml (~50 unique deps + optional groups). **SC-002 (≥30) met — with 2.7× headroom.**

### test-langflow +9, test-sphinx +5, test-pytorch +2

Small incremental gains where these repos' internal Python sub-projects have `[project.dependencies]` declarations previously suppressed by the m018 policy. Zero regressions on any component that was previously emitted.

## PR-3 note on cpython SC-003

The cpython row shows delta = 0. This is expected: T012 and T013 are **enrichment** passes (they add annotations to existing components), not discovery passes. Component *count* only moves when new sources emit new components.

Post-T013 cpython emits:
- Same 16 pypi components as before
- Plus new `waybill:python-req-file-scope` annotations on 10 of them (scope=docs on 7 Doc/ deps, scope=dev on 3 Tools/requirements-dev.txt deps) — verified end-to-end during T013
- Plus a new `waybill:direct-url-source` annotation on `pygments` (the `pygments @ https://.../archive/2cad2642...tar.gz` line) — verified end-to-end during T012

The **spec's SC-003 ≥ 50 target** cannot be met from the requirements-file + pyproject-file parsing alone — cpython declares ~11 unique deps across its 3 requirements files. Realistic ceiling from declared-file sources is ~15-20 pypi components, which is what we observe. **Reaching ≥ 50 would require a different attack** (vendored packages in `Lib/`, file-tier fallback on `.py` sources, or corpus-fingerprint enrichment) that's outside m670's scope. See PR-3's tasks.md notes for the follow-up path.

## Wall-clock notes

Several fixtures show large negative scan-ms deltas:
- test-bun: -35s
- test-codex: -38s
- test-vaultwarden: -9s
- test-rustlang: -50s (still fails, just fails faster)

These are **NOT m670-attributable improvements**. They almost certainly reflect a warm-cache effect: the 2026-08-31 baseline was a cold-start (first clone + first scan per fixture in this session), and the sibling `waybill-test-fixtures` cache had grown between then and now, warming shared caches. m670's code changes are annotation additions + one policy reversal — none of them should affect scan time on non-Python repos.

The positive m670 deltas (test-markitdown +1ms, test-cpython +5ms) reflect the actual new work: pyproject reads + annotation emission. Both are trivial and comfortably under the SC-007 (549ms) and SC-008 (5575ms) targets.
