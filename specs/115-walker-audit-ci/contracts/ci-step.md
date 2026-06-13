# Contract: Walker-Audit CI Step

**Feature**: 115-walker-audit-ci
**Date**: 2026-06-13
**Consumed by**: `.github/workflows/ci.yml`
**Spec mapping**: FR-001, FR-002, FR-003, FR-004, FR-005, FR-009, FR-010

This contract defines the externally observable behavior of the single CI step this feature ships. It is the surface against which the negative-test runbook in [quickstart.md](../quickstart.md) verifies the gate, and the surface a maintainer can rely on when reading `ci.yml`.

> **Supersession note (2026-06-13)**: The "Pipeline", "Outputs", and "Backwards compatibility" sections below are superseded by [`specs/117-line-stable-allowlist/contracts/ci-step.md`](../../117-line-stable-allowlist/contracts/ci-step.md), which adds a symmetric `sed` line-number strip to both sides of the comparison so unrelated insertions above an existing walker no longer trigger false-positive failures (closes issue #347). The other sections (Step name, Trigger surface, Step ordering, Inputs, Performance, Idempotency, Failure-mode parity, Out-of-band overrides) remain canonical here.

## Step name (workflow YAML)

```text
Walker-audit allow-list check
```

The name appears verbatim in the GitHub Actions UI's step list under the `Lint + test (linux-x86_64)` job. The name choice trades terseness for searchability — a contributor seeing it red can copy-paste the name into the repo's grep and immediately find both the YAML and the docs.

## Trigger surface

- **Workflow file**: `.github/workflows/ci.yml`
- **Job**: `Lint + test (linux-x86_64)` (existing; at line 39 in the post-114 tree)
- **Trigger events**: every `pull_request` and every `push` to main (inherited from the existing job's triggers; the audit fires whenever clippy + tests fire)
- **Step ordering**: runs AFTER `actions/checkout@v4` and BEFORE the existing clippy step at `ci.yml:216` (and therefore also before `cargo test --workspace`). Rationale: a fast failure short-circuits both the lint suite and the slow test suite.

## Inputs

The step reads ONE file from the working directory at PR HEAD:

| Input | Path | Required? | Behavior if missing |
|---|---|---|---|
| Source tree | `mikebom-cli/src/scan_fs/**/*` | yes | grep returns empty; allow-list non-empty → diff fails → step exits non-zero ✓ (the source-tree-disappeared-entirely case is hypothetical but the gate still fails closed) |
| Allow-list | `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` | yes | step exits non-zero with a "missing or empty allow-list" failure message ✓ (FR-010 strict-enforcement bootstrap) |

## Outputs

### Exit code

| Exit code | Meaning |
|---|---|
| 0 | Allow-list bytes equal the canonicalized live grep output. PR may proceed. |
| ≠0 | Drift detected OR allow-list missing OR allow-list empty. PR is blocked. |

### Stdout / stderr

The step always prints SOMETHING; what depends on the path:

**Pass path (exit 0)**:
```
Walker-audit allow-list check: OK (<N> entries; <M> ms)
```

Where `<N>` is the entry count and `<M>` is the wall time. The success line is short by design — most CI runs hit this path and excess output dilutes the signal of legitimate failures.

**Fail path — drift (exit non-zero)**:
```
[FAIL] Walker-audit allow-list mismatch — see mikebom-cli/src/scan_fs/walk.rs's module-level comment for the exception policy.

--- mikebom-cli/src/scan_fs/walk.audit-allowlist.txt (expected)
+++ live: grep -rEn 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sort -u (actual)
@@ -X,Y +X,Y @@
 ...unchanged context...
-removed-entry-1
+added-entry-1
+added-entry-2
 ...unchanged context...

If your PR intentionally adds a new walker exception, see CONTRIBUTING.md § Walker-audit CI gate.
If your PR did NOT intend to add a walker, remove the new fn walk_* function and use scan_fs::walk::safe_walk instead.
```

**Fail path — missing allow-list (exit non-zero)**:
```
[FAIL] Walker-audit allow-list mismatch — see mikebom-cli/src/scan_fs/walk.rs's module-level comment for the exception policy.

ERROR: mikebom-cli/src/scan_fs/walk.audit-allowlist.txt is missing or empty.

This file is the bootstrap baseline for the walker-audit CI gate (feature 115).
If you intentionally removed it, restore from the previous commit:
    git show HEAD~1:mikebom-cli/src/scan_fs/walk.audit-allowlist.txt > mikebom-cli/src/scan_fs/walk.audit-allowlist.txt

See CONTRIBUTING.md § Walker-audit CI gate for the file's purpose.
```

These two fail-path message templates are reviewer-policed for stability — changing them is a contract change and requires updating the spec's FR-004 + this contract.

## Performance contract

- **p95 wall time**: ≤5 s (SC-002)
- **Expected p50**: <500 ms on ubuntu-latest given ~50 source files in `scan_fs/`
- **Hard timeout**: inherited from the parent job's `timeout-minutes` setting; no per-step timeout configured

If wall time creeps above 5 s (unlikely but possible if `scan_fs/` grows to thousands of files), revisit the step's grep scope — possibly switch to `ripgrep` (`rg`) if it's available on future runner images, or scope the grep to specific subdirectories.

## Idempotency

The step is fully idempotent: running it twice on the same source tree produces identical output. No side effects, no temp-file leakage outside `${RUNNER_TEMP}`.

## Failure-mode parity

The step's pass/fail outcome MUST be byte-deterministic across:
- Multiple consecutive runs on the same commit SHA
- Re-runs of the same job (the `Re-run failed jobs` button)
- Different ubuntu-latest runner images (Ubuntu 22.04 vs 24.04 etc.)

This is satisfied by Decision 4 (LC_ALL=C sort) and Decision 2 (POSIX-only tools). FR-005 is the spec requirement; this contract is its operationalization.

## Out-of-band overrides

There is no `MIKEBOM_SKIP_WALKER_AUDIT` env var, no `[skip walker-audit]` PR-title token, no `walker-audit-bypass` label. The gate is unconditionally enforcing per Q1's strict-enforcement bootstrap clarification. An emergency bypass (if ever needed) is the same as for any other failed CI gate: a PR that explicitly modifies `ci.yml` to comment out the step, with the maintainer reviewing that modification as the bypass authorization.

This is intentional — the gate's value derives entirely from being unconditional. A bypass mechanism is a delegation problem (who has bypass rights? logged how?), and the cost of NOT having one is low (the step itself is fast to fix-up via a follow-up commit).

## Backwards compatibility

Pre-merge of this PR: the step does not exist; existing CI is unaffected. Post-merge: every subsequent PR pays the <500 ms step cost. There is no rolling-deprecation window; the gate is hot on day one.

If a future milestone introduces a way to legitimately have an empty allow-list (e.g., every walker has migrated to `safe_walk`), that milestone updates this contract's "missing or empty" failure-path text. Until then, "empty = fail" is correct.
