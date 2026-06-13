# Contract: Updated Walker-Audit CI Step

**Feature**: 117-line-stable-allowlist
**Date**: 2026-06-13
**Consumed by**: `.github/workflows/ci.yml`
**Supersedes**: the "Pipeline" section of `specs/115-walker-audit-ci/contracts/ci-step.md`. All other sections of the milestone-115 contract (Step name, Trigger surface, Step ordering, Performance, Idempotency, Failure-mode parity, Out-of-band overrides, Backwards compatibility) remain in force unchanged.
**Spec mapping**: FR-001, FR-002, FR-003, FR-009, FR-012

This contract documents the SUPERSEDING parts of milestone-115's CI step. Each section below either confirms unchanged-from-115 behavior (✓ Preserved) or documents the specific delta this feature introduces (★ Changed).

## Step name (workflow YAML)

✓ Preserved — `Walker-audit allow-list check`. Same string. Same job slot.

## Trigger surface

✓ Preserved — same workflow file, same `Lint + test (linux-x86_64)` job, same trigger events (every PR + every push to main), same ordering before `Install stable Rust` so a failure short-circuits clippy + tests.

## Inputs

✓ Preserved — two inputs:
- Source tree under `mikebom-cli/src/scan_fs/**/*.rs`
- Allow-list file at `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`

★ Changed — the allow-list file's per-entry shape:
- Milestone 115: `<file>:<line>:<content>`
- This feature: `<file>:<content>` (line-number column removed)

The file's location, name, encoding, line-ending policy, sort policy, and strict-enforcement-bootstrap behavior are all preserved.

## Pipeline (the SUPERSEDING section)

The CI step's shell pipeline gains ONE new step on each side: a `sed` invocation that strips the `<line>:` column. The strip applies to both the live grep output AND the committed allow-list before they meet at `diff -u`.

### Updated pipeline

```bash
ALLOWLIST="mikebom-cli/src/scan_fs/walk.audit-allowlist.txt"
STRIP_LINE_NUMBERS='s/^\([^:]*\):[0-9]*:/\1:/'

# Precheck unchanged from milestone 115:
if [ ! -f "$ALLOWLIST" ]; then ... exit 1; fi
EXPECTED=$(grep -v '^#' "$ALLOWLIST" | grep -v '^$' | sed "$STRIP_LINE_NUMBERS" | LC_ALL=C sort -u)
if [ -z "$EXPECTED" ]; then ... exit 1; fi

# Live audit pattern. The sed step is the ONLY new pipeline element vs 115.
LIVE=$(LC_ALL=C grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ \
       | sed "$STRIP_LINE_NUMBERS" \
       | LC_ALL=C sort -u)

# Comparison unchanged from milestone 115.
if DIFF_OUT=$(diff -u <(printf '%s\n' "$EXPECTED") <(printf '%s\n' "$LIVE")); then
    echo "Walker-audit allow-list check: OK (...)"
else
    echo "$FAIL_HEADLINE" >&2
    echo "" >&2
    echo "--- mikebom-cli/src/scan_fs/walk.audit-allowlist.txt (expected)" >&2
    echo "+++ live: grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sed -e 's/^([^:]*):[0-9]*:/\\1:/' | sort -u (actual)" >&2
    printf '%s\n' "$DIFF_OUT" | tail -n +3 >&2
    echo "" >&2
    echo "$FAIL_POINTER_NEW" >&2
    echo "$FAIL_POINTER_BAD" >&2
    exit 1
fi
```

The sed regex `^\([^:]*\):[0-9]*:` matches everything up to the first colon (the file path), then the line-number column (digits between two colons), and replaces with everything up to the first colon (the file path) plus a colon. BRE syntax — works identically on macOS BSD sed and Linux GNU sed. No `-E` / `-r` flag.

### Effect on the diff output's shape

When the gate fails, the unified-diff hunks shown in the CI log carry the NEW shape:

```diff
--- mikebom-cli/src/scan_fs/walk.audit-allowlist.txt (expected)
+++ live: grep ... | sed ... | sort -u (actual)
@@ -22,3 +22,4 @@
 mikebom-cli/src/scan_fs/package_db/nuget/directory_packages_props.rs:    fn walk_up_finds_props_in_ancestor() {
 mikebom-cli/src/scan_fs/package_db/nuget/directory_packages_props.rs:    fn walk_up_stops_at_scan_root() {
 mikebom-cli/src/scan_fs/package_db/rpmdb_sqlite/schema.rs:fn walk_schema_page<F>(
+mikebom-cli/src/scan_fs/synthetic_walker_NEW.rs:fn walk_synthetic_new() {
 mikebom-cli/src/scan_fs/walk.rs:pub(crate) fn safe_walk<F: FnMut(&Path)>(
```

Notice: no `:123:` columns. Each entry is `<file>:<content>` only. The failure-headline, pointer, and exit code are preserved bit-for-bit from milestone 115.

## Outputs

✓ Preserved — exit codes (0 on match, non-zero on drift / missing / empty), the "OK" success message format, the `[FAIL] ...` headline, the trailing two-line pointer to CONTRIBUTING.md / safe_walk.

★ Changed — the `+++ live:` header line in the failure message documents the new pipeline shape. Specifically:

| | Header text |
|---|---|
| Milestone 115 | `+++ live: grep -rEn 'fn walk[_(]' mikebom-cli/src/scan_fs/ \| sort -u (actual)` |
| This feature | `+++ live: grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ \| sed -e 's/^\([^:]*\):[0-9]*:/\1:/' \| sort -u (actual)` |

Side note: milestone 115's header text omitted the `--include='*.rs'` scoping (added in milestone 115 PR-A during the synthetic-walker self-match fix). This feature corrects that minor inaccuracy as part of the header update.

## Performance contract

✓ Preserved — ≤5 s per SC-002 of milestone 115. The new sed step adds <10 ms on ~28 lines; total step wall time stays well under 500 ms.

## Idempotency

✓ Preserved — the step is idempotent. Running it twice on the same source tree produces identical output. The sed step is a pure function; idempotency is unchanged.

## Failure-mode parity

✓ Preserved — same byte-deterministic outcome across consecutive runs, re-run attempts, and runner-image updates. The sed BRE regex's portability across GNU/BSD sed is the new component to verify; tested on both Ubuntu (GNU sed) and macOS (BSD sed) at PR-ship time via local pre-PR runs.

## Out-of-band overrides

✓ Preserved — no env var, no PR-title bypass token, no label-based skip. The gate stays unconditional per milestone-115 design rationale.

## Backwards compatibility

★ Changed posture: there is NO transition window where the gate accepts both shapes (per FR-012 + research §3). The PR that introduces this feature ships the allow-list file in NEW form at the same commit as the CI YAML change. After merge, the gate operates in NEW-form mode exclusively.

✓ Forgiveness preserved: per research §2 / Decision 2, the sed strip applies to BOTH sides. If a future PR accidentally hand-edits an OLD-form line into the allow-list (e.g., a contributor copy-pastes from milestone-115 docs that haven't been updated yet), the strip on the read makes the OLD-form line compare correctly against the new live output. No false positive. This is forgiveness toward drift, not invitation to use OLD form deliberately — the committed file SHOULD always be NEW-form per FR-002.

## Cross-link

The full milestone-115 contract at [`specs/115-walker-audit-ci/contracts/ci-step.md`](../../115-walker-audit-ci/contracts/ci-step.md) remains the canonical source for the unchanged sections. This feature's contract document supersedes ONLY the "Pipeline" + "Outputs" + "Backwards compatibility" sections. A pointer to this document is added to milestone 115's contract as a "superseded by milestone 117 (issue #347)" note at the top.
