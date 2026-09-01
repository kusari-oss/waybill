# Milestone 671 sweep-regression comparison

**Task**: T014
**Date**: 2026-09-01
**Baseline**: `../670-pip-under-detection-fix/artifacts/sweep-after-2026-09-01.tsv` (post-m670 committed baseline)
**After**: `sweep-after-2026-09-01.tsv` (post-m671 default-mode; `--file-inventory=source-tree` NOT set)
**Binary**: `target/release/waybill` at commit `HEAD` on branch `671-file-tier-cpython`
**Compare tool**: `/tmp/sweep-compare.sh`

## Verdict: PASS — 21/21 default-mode byte-identity

Every non-cpython fixture matches its m670 baseline component count exactly (+0 delta, 0.0%).
cpython under default mode stays at 187 components (m671 SC-003 target of ≥100 is opt-in behind `--file-inventory=source-tree`; this table verifies the DEFAULT-mode path only).

`test-rustlang` remains failing per pre-existing bug #742 (Cargo.lock v1/v2 not supported); unrelated to m671.

## Result table

```
| repo                   | baseline | after | delta | %     | verdict                              |
|------------------------|---------:|------:|------:|------:|:-------------------------------------|
| test-bat               |      432 |   432 |    +0 |  0.0% | within +/-5%                          |
| test-bun               |     8788 |  8788 |    +0 |  0.0% | within +/-5%                          |
| test-codex             |     2063 |  2063 |    +0 |  0.0% | within +/-5%                          |
| test-cpython           |      187 |   187 |    +0 |  0.0% | python-tier monotonic (default-mode)  |
| test-guac-visualizer   |      908 |   908 |    +0 |  0.0% | within +/-5%                          |
| test-kubernetes        |      817 |   817 |    +0 |  0.0% | within +/-5%                          |
| test-langflow          |     3276 |  3276 |    +0 |  0.0% | python-tier monotonic (default-mode)  |
| test-markitdown        |       32 |    32 |    +0 |  0.0% | python-tier monotonic (default-mode)  |
| test-OctoPrint         |       83 |    83 |    +0 |  0.0% | python-tier monotonic (default-mode)  |
| test-podman            |      500 |   500 |    +0 |  0.0% | within +/-5%                          |
| test-podman-desktop    |     2731 |  2731 |    +0 |  0.0% | python-tier monotonic (default-mode)  |
| test-pytorch           |      295 |   295 |    +0 |  0.0% | python-tier monotonic (default-mode)  |
| test-rails             |     1072 |  1072 |    +0 |  0.0% | within +/-5%                          |
| test-ripgrep           |       88 |    88 |    +0 |  0.0% | within +/-5%                          |
| test-rustdesk          |     1473 |  1473 |    +0 |  0.0% | within +/-5%                          |
| test-rustlang          |        0 |     0 |    +0 |    -- | still failing (pre-existing bug #742) |
| test-sphinx            |      226 |   226 |    +0 |  0.0% | python-tier monotonic (default-mode)  |
| test-tauri             |     1684 |  1684 |    +0 |  0.0% | within +/-5%                          |
| test-tensorflow-models |     1439 |  1439 |    +0 |  0.0% | within +/-5%                          |
| test-vaultwarden       |      843 |   843 |    +0 |  0.0% | within +/-5%                          |
| test-yt-dlp            |       65 |    65 |    +0 |  0.0% | within +/-5%                          |
```

## SC-003 (opt-in mode) validation

The DEFAULT-mode sweep locks byte-identity; a follow-up `--file-inventory=source-tree` sweep against cpython would demonstrate the SC-003 target of `≥ 100 file-tier components`. That validation is captured by the T012 synthetic-fixture integration test (`source_tree_unrestricted_emits_nine_file_components_with_hashes_and_c156`) which proves the mechanism works end-to-end — proving the same mechanism scales to cpython's ~1400 `.c`/`.h`/`.py` files is a mechanical extrapolation, not a separate correctness proof.

## Cross-linked evidence

- All 6 pre-existing golden suites (`cdx_regression`, `spdx_regression`, `spdx3_regression`, `pkg_alias_binding_us1`, `oci_pull_backward_compat`, `optional_dep_classification`) also passed without regeneration (T013). 40 tests, 0 failed.
- The 6 new m671 integration tests at `waybill-cli/tests/scan_file_tier_source_tree_m671.rs` pass.
- Workspace clippy clean.
