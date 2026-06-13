# Research — Line-stable walker-audit allow-list

**Feature**: 117-line-stable-allowlist
**Date**: 2026-06-13
**Status**: Decisions resolved; no NEEDS CLARIFICATION markers remaining.

## Decision 1 — Line-number-stripping mechanism

**Decision**: `sed 's/^\([^:]*\):[0-9]*:/\1:/'` applied symmetrically to both the live grep output and the committed allow-list before sort+diff.

**Rationale**:
- **POSIX-mandated** — `sed` is guaranteed available on every GitHub Actions runner image and every developer machine that can run the existing `grep`/`sort`/`diff` pipeline. No installation step, no version pinning, no `apt-get install`.
- **Preserves grep's familiar form** — `grep -rEn` is what the audit-pattern documentation across CONTRIBUTING.md + walk.rs comment block + design-notes uses verbatim. Keeping the `-n` flag and filtering downstream means contributors who copy-paste the audit grep from any doc still produce useful local output (line numbers shown for human consumption); the filter just normalizes before comparison.
- **One-line BSD/GNU-compatible regex** — `^\([^:]*\):[0-9]*:` matches "everything up to the first colon, then digits, then a colon" and replaces with "everything up to the first colon, then a colon." Works identically on macOS BSD `sed` and Linux GNU `sed`. No `-E` flag required.
- **Localized change** — adding `| sed '...'` to both `$LIVE` and `$EXPECTED` extractions in the existing `ci.yml` `run:` block is a 2-line edit. The failure-message contract from milestone 115 (FR-003) is preserved bit-for-bit; only the diff content's shape gets shorter by one column.

**Alternatives considered**:
- **`awk -F: '{print $1 ":" substr($0, length($1)+length($2)+3)}'`** — works but the substring math is harder to read and review. Rejected: optimizes for nothing.
- **`grep -r` without the `-n` flag (no line numbers in output)** — simplest possible: the gate stops asking for line numbers in the first place. Rejected because the audit-pattern documentation across multiple places (CONTRIBUTING.md, walk.rs comment block, design-notes) all show the `-rEn` form. Changing the pattern as documented forces churn in every doc that quotes it AND breaks contributors' muscle memory. The sed-strip approach lets the documented command stay `grep -rEn` so the file-path:line-number output is still useful for human consumption while the gate sees the line-number-stripped form.
- **A small Rust helper compiled via `cargo run -p ...`** — heavy: pulls a cargo build into the audit path; requires the Rust toolchain to be installed before the gate fires (defeating the milestone-115 "short-circuit before toolchain install" design); adds dependency on the workspace state. Rejected for the same reasons milestone 115 rejected a Rust-based gate.
- **A small standalone bash function defined in the same `run:` block** — works but uglier than a one-line sed. Rejected: sed is more idiomatic for line transforms.

## Decision 2 — Symmetric vs one-sided filtering

**Decision**: Apply the sed line-strip symmetrically to BOTH sides — live grep output AND the committed allow-list file content — before they meet at `diff -u`.

**Rationale**:
- The allow-list file's committed shape changes to `<file>:<content>` (no line numbers) at PR-ship time per FR-002. But sed-filtering the file as it's read is robust against accidents: if a contributor's regeneration command emits the OLD form (with line numbers, e.g., they used an old copy-paste from milestone 115's docs), the sed-strip on the read makes the OLD form compare correctly against the live output. The gate doesn't false-positive on a hand-edited OLD-form entry.
- Symmetric application means the comparison logic is `strip(live) ≡ strip(file)`, which is true whether `file` is in OLD or NEW form. The file's committed form is NEW per the bootstrap (FR-002), but the filter is forgiving toward OLD-form drift.
- Cost is negligible — one extra sed invocation on ~28 lines of file content. <1 ms.

**Alternatives considered**:
- **Filter only the live output; trust the committed file's shape** — one fewer sed invocation, but loses the forgiveness for accidental OLD-form regen. Rejected: brittleness for negligible savings.
- **Filter only the committed file's content; trust the live grep** — symmetric to the above but loses the same forgiveness. Rejected.

## Decision 3 — Bootstrap timing

**Decision**: Single-PR total cutover. The PR that ships this feature commits BOTH (a) the `ci.yml` change introducing the sed filter AND (b) the regenerated allow-list in the NEW `<file>:<content>` shape. The gate operates exclusively in NEW-form mode from the moment this PR's merge commit lands; there is NO transition window where both shapes coexist.

**Rationale**:
- Matches milestone 115's strict-enforcement bootstrap stance (FR-010 of 115; FR-012 of this feature). Single-PR cutover means the gate's contract is unambiguous after merge: "the allow-list is in NEW form; the gate normalizes both sides via sed; that's it."
- Two-phase rollout (phase 1: accept both shapes via filter, phase 2: enforce new shape via re-bootstrap) would add complexity for no benefit — there are no external consumers of the allow-list, no downstream contributors with their own forks racing to update, and the regen command itself is a one-liner shell pipeline.
- The forgiveness from Decision 2 (sed-strip on read) means even if a future PR accidentally hand-edits an OLD-form line into the allow-list, the gate doesn't false-positive. So the cutover doesn't NEED a transition window; the forgiveness handles drift naturally.

**Alternatives considered**:
- **Two-phase**: ship the filter first, regen the allow-list in a follow-up PR. Rejected: extra PR overhead for no benefit; the contract is cleaner with single-PR cutover.
- **Backward-compatible: accept both forms forever, no allow-list regen** — works but leaves the committed file in OLD form indefinitely, which (a) confuses readers who later inspect the file directly and wonder why the `:1249:` columns are still there, and (b) means the `LC_ALL=C sort -u` ordering is still partially line-number-dependent. Rejected: ergonomic regression for no benefit.

## Decision 4 — Failure-message template preservation

**Decision**: The failure-message template from milestone 115 (FR-004 of 115) is preserved bit-for-bit. The headline (`[FAIL] Walker-audit allow-list mismatch — see mikebom-cli/src/scan_fs/walk.rs's module-level comment for the exception policy.`), the `--- expected` and `+++ live` diff-header lines, the diff hunks, and the trailing two-line pointer to CONTRIBUTING.md / safe_walk all stay byte-identical. The ONE textual change is the `+++ live:` header's command suffix to match the new pipeline:

```diff
- echo "+++ live: grep -rEn 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sort -u (actual)" >&2
+ echo "+++ live: grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sed -e 's/^\\([^:]*\\):[0-9]*:/\\1:/' | sort -u (actual)" >&2
```

(Note: the milestone-115 `+++ live:` line had a minor inaccuracy — it omitted the `--include='*.rs'` scoping that PR-A added; this PR is the right place to also correct that inaccuracy, since we're touching the same line.)

**Rationale**:
- Operators triaging a red CI see the same diff hunks, same trailing pointer, same shape. Their muscle memory carries over. Only individual diff entries get shorter (no `:1249:` column), making the diff easier to read, not harder.
- The `+++ live:` header line is the natural place to document the new pipeline shape so a contributor reproducing it locally can copy-paste exactly what the gate runs.
- Preserving the headline + pointer language verbatim avoids stylistic churn in `specs/115-walker-audit-ci/contracts/ci-step.md` § "Fail path — drift" — only the inline-command excerpt in that section updates.

**Alternatives considered**:
- **Rewrite the failure-message headline to reference issue #347 / this feature** — rejected. The headline's job is to orient the reader; it doesn't need to credit which milestone last touched the gate. Stability is more valuable than provenance.
- **Drop the `+++ live:` header line entirely** — works but makes the failure output less self-documenting. Rejected: cheap to keep accurate.
