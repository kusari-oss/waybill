# Implementation Plan: Keep test output paths reserved (#1727)

**Branch**: `1660-held-temp-paths` | **Date**: 2026-11-26 | **Spec**: [spec.md](spec.md)

## Summary

1. **Measure:** a throwaway stress harness counts duplicate temporary-file names across concurrent processes, with a control that's forced to collide.
2. **Fix:** change the 23 dropped sites found by a statement-level scan (research R1; #1727's line-context list was mostly wrong) to a held `TempDir` with a file inside it.
3. **Enforce:** add `scripts/check-held-temp-paths.py`, run by `pre-pr.sh` and CI, with self-test controls.

## Technical Context

**Toolchain:**
- **Language:** Rust stable, pinned 1.98.1, and Python 3 standard library for the check. No `+toolchain`.
- **Dependencies:** none new in the workspace. The harness is a standalone crate with an empty `[workspace]` table, pinned to the lockfile's `tempfile` 3.27.0.

**Changes:**
- **Code:** test code and scripts only. Production code has no dropped site (R1).
- **Platforms:** all CI lanes. The held `TempDir` shape also avoids Windows handle locking.

## Constitution Check

| Rule | Status |
|---|---|
| VII. Test Isolation | Pass. This is the point of the spec: tests no longer share output paths. |
| III. Fail Closed | Pass. The check fails on a violation, and its self-test fails if the check stops detecting the shape. |
| Pre-PR gate (4.1.0) | One step added, run from the same script in CI. That is in line with the walker-audit precedent, not a change to the gate's contract, so no amendment is needed. |
| Zero new Cargo dependencies | Pass. The harness crate is outside the workspace and lockfile. |

## Project Structure

```text
scripts/check-held-temp-paths.py     # new
scripts/pre-pr.sh                    # + the check, next to walker-audit
.github/workflows/ci.yml             # + one step next to the walker-audit step
waybill-cli/tests/<22 files>         # 23 sites → held TempDir
specs/1660-held-temp-paths/measurements/
├── collide/ (Cargo.toml, src/main.rs), collide.sh   # stress harness
├── collide-results.tsv
├── sites.md                         # per-site verdicts
└── check-controls.md
```

## Phases

1. Run the stress harness and its control (US1). This comes before any change.
2. Write the check with `--self-test`. Run it on the unmodified tree as the positive control: it must list all 23 sites.
3. Review each site, record it in `sites.md`, and fix it.
4. Re-run the check, which must be clean, and wire it into `pre-pr.sh` and CI.
5. Run the suite under nextest 3 times, run the gate, open the PR, and check every CI lane.

## Complexity Tracking

None.
