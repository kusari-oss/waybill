# Feature Specification: Line-stable walker-audit allow-list

**Feature Branch**: `117-line-stable-allowlist`
**Created**: 2026-06-13
**Status**: Draft
**Input**: Issue #347 follow-up to milestone 115. The walker-audit CI gate's allow-list entries today are `<file>:<line>:<content>` — verbatim `grep -rEn` output, which pins each entry to an absolute line number. Any insertion above an allow-listed `fn walk*` shifts every subsequent walker's line number, producing a CI-red false positive that contributes no signal. Two real incidents in two days of post-shipping use (milestone 116 PR-A line-shift, PR-B line-shift) prove the brittleness in practice. The fix is to change the allow-list and audit pattern to omit the line number, so the fingerprint depends only on file path + matched-line content.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — A contributor whose PR adds unrelated code above an existing walker doesn't get a false-positive CI failure (Priority: P1)

A contributor opens a pull request that touches a file under the filesystem-scanning layer of the codebase — for example, adding a 50-line helper function to a Maven reader, an Npm reader, or any other module that contains one or more allow-listed walker entries. The contributor's change has no semantic effect on any walker function: no new walkers, no removed walkers, no renamed walkers, no signature changes. With today's gate, this PR fails Continuous Integration red because the line numbers of every walker function below the newly-inserted helper have shifted — the live audit grep produces different `<file>:<line>:` columns than the committed allow-list, even though the rest of each line is byte-identical. The contributor must regenerate the allow-list, push a noise commit, and explain to the reviewer "ignore me, just line numbers." After this feature, the same PR passes CI cleanly: the gate compares only file path + matched-line content, ignoring positional drift.

**Why this priority**: This is the textbook false-positive case that motivates issue #347. Without it, every PR that inserts code above an existing walker pays a noise tax and trains reviewers to skim past allow-list diffs. As the codebase grows, that noise tax compounds: more PRs touch `scan_fs/`, more walkers exist below newly-inserted code, more regenerations are forced. The gate's signal-to-noise ratio degrades until contributors and reviewers learn to ignore it — exactly the failure mode the gate was designed to prevent. P1 because the gate's whole purpose (forcing reviewer attention onto walker-set changes) erodes the moment its diffs are dominated by noise.

**Independent Test**: A test PR adds a synthetic ~50-line helper function at the top of an existing source file under the filesystem-scanning layer, above an allow-listed walker. The helper's body is unrelated to walking (e.g., a small parser or formatter). The PR makes no changes to the allow-list file. CI runs and the walker-audit step exits green. Verifies that pure line-number drift no longer triggers the gate.

**Acceptance Scenarios**:

1. **Given** the post-115 baseline allow-list with 28 entries, **When** a PR adds a ~50-line helper function above an allow-listed walker in any source file under the filesystem-scanning layer (no walker functions added, removed, renamed, or signature-changed), **Then** the walker-audit CI step exits with success and the contributor does NOT have to touch the allow-list.
2. **Given** the same baseline, **When** a PR adds a new top-level function whose name matches the audit pattern (a genuine new walker addition outside the allow-list), **Then** the walker-audit CI step exits red with the unchanged failure-message contract from milestone 115 — the change in detection contract is purely about line-number ignoring; everything else stays identical.
3. **Given** the same baseline, **When** a PR renames an existing allow-listed walker function (the function name in the matched line content changes), **Then** the walker-audit CI step exits red. A rename IS a semantic change; the new fingerprint catches it.
4. **Given** the same baseline, **When** a PR changes an existing allow-listed walker's signature (the matched line content changes due to e.g. an added parameter), **Then** the walker-audit CI step exits red. A signature change IS a semantic change; the new fingerprint catches it.
5. **Given** the same baseline, **When** a PR moves an existing allow-listed walker to a different source file (the file path in the matched line changes), **Then** the walker-audit CI step exits red. A relocation IS a structural change worth reviewer attention; the new fingerprint catches it.
6. **Given** the same baseline, **When** a PR deletes an allow-listed walker entirely AND removes the allow-list entry in the same PR, **Then** the walker-audit CI step exits green. A clean removal IS a legitimate change; the contributor edits the allow-list in the same diff.

### Edge Cases

- What happens when two different walker functions in the SAME source file happen to produce IDENTICAL matched-line content (file path + line content, ignoring the line number column)? This is structurally impossible — two function declarations producing byte-identical matched-line text would be a Rust compile error (duplicate function names in the same module). The dedup step on the line-number-stripped sort collapses byte-identical lines, but byte-identical lines cannot arise from valid Rust code in practice.
- What happens when a PR removes a function whose name matches the audit pattern? The grep no longer produces the entry; the allow-list still contains it; the diff reports a deletion; CI fails red. The contributor must remove the entry from the allow-list in the same PR. This is the documented "stale exception" detection path from milestone 115 FR-003, and the new fingerprint shape preserves it identically.
- What happens when a PR migrates an existing hand-rolled walker to delegate to `safe_walk` (the function gets removed; the migration introduces no new functions matching the audit pattern)? The grep loses one entry; the allow-list loses one entry; CI passes green. This is the milestone-114 migration path; the new fingerprint shape preserves it identically.
- What happens when a contributor regenerates the allow-list using the OLD command form (with line numbers) by accident? The CI step's preprocessing strips line numbers before diff, so the comparison still works correctly — the OLD-form allow-list, after stripping, equals the live output after stripping. No spurious failure. (The documentation guides contributors to the NEW form; the OLD form remains a benign legacy path.)
- What happens when the new fingerprint shape ships AND the committed allow-list file's content is bootstrapped to the new shape AT THE SAME TIME (this PR)? The CI step starts comparing line-number-stripped output against line-number-stripped allow-list from day one of the new shape — no migration window where the old and new forms coexist in the gate's logic.
- What happens when a future maintainer needs to audit the SOURCE LOCATION of a specific allow-listed walker for compliance review or documentation purposes? The grep without `-n` is the natural local invocation; reviewers who need a specific line number run a separate one-shot grep with line numbers. The allow-list is for the gate's correctness, not for archival source-location records.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The walker-audit CI step MUST compare the line-number-stripped audit grep output against the committed allow-list. Both sides of the comparison MUST be normalized to `<file>:<matched-line-content>` shape, with the absolute line number removed, before the diff runs.
- **FR-002**: The committed allow-list file MUST store entries in the new `<file>:<matched-line-content>` shape. The shape MUST be byte-stable across runs and across hosts via the existing milestone-115 `LC_ALL=C sort -u` canonicalization.
- **FR-003**: The walker-audit CI step's failure-message contract from milestone 115 (FR-004 of `specs/115-walker-audit-ci/spec.md`) MUST be preserved unchanged: the headline, the unified-diff hunks, the two-line pointer to CONTRIBUTING.md / safe_walk. Only the diff content's shape changes (entries are shorter — no `:1249:` middle column).
- **FR-004**: A new walker function added outside the allow-list MUST still fail CI red. The gate's primary purpose (catch unauthorized walker additions) MUST be preserved bit-for-bit.
- **FR-005**: An existing walker function deleted from the source tree without a corresponding allow-list entry removal MUST still fail CI red. The gate's secondary purpose (catch stale exceptions) MUST be preserved.
- **FR-006**: A walker function renamed in the source tree MUST fail CI red. A rename changes the matched-line content; the new fingerprint shape catches it.
- **FR-007**: A walker function whose signature changes (added/removed parameter, return-type change visible on the same line) MUST fail CI red. A signature change is reviewer-attention-worthy and the new fingerprint catches it.
- **FR-008**: A walker function moved to a different source file MUST fail CI red. A relocation IS a structural change; the file-path column in the fingerprint catches it.
- **FR-009**: An insertion of unrelated code above an existing allow-listed walker — code that introduces no new walker functions, removes none, renames none, signature-changes none, relocates none — MUST NOT trigger a CI failure. This is the textbook false-positive case the feature exists to eliminate.
- **FR-010**: The CONTRIBUTING.md `## Walker-audit CI gate` section MUST be updated to document the new allow-list regeneration command (the one-liner contributors run when they legitimately add or remove a walker). The new command and the new file shape MUST be documented as a coordinated pair.
- **FR-011**: The `specs/115-walker-audit-ci/contracts/ci-step.md` contract document MUST be updated to record the new allow-list entry shape. The change is a documented evolution of milestone 115's CI-step contract; the contract document carries a note linking back to issue #347 / this feature for the rationale.
- **FR-012**: The pull request that introduces this feature MUST commit the bootstrap allow-list in the NEW shape. There is NO transition mode where the gate accepts either shape — the change is total at PR-ship time. Per the existing milestone-115 strict-enforcement bootstrap rule (FR-010 of 115), a missing or empty allow-list still fails the build.

### Key Entities

- **Allow-list entry**: A committed source-tree text file entry recording one intentional `fn walk*` match in the codebase. New shape: `<file>:<matched-line-content>` — file path relative to repo root, colon, the line text that contains the `fn walk*` match. Old shape: `<file>:<line>:<matched-line-content>`. The change is removing the middle line-number column.
- **Audit pattern**: The grep command the CI step runs. New form: `grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sed 's/^\([^:]*\):[0-9]*:/\1:/'` (or equivalent line-number-stripping preprocessor on the same output). The grep retains `-n` so the command stays familiar to contributors, but the line-number column is stripped before the comparison.
- **Comparison pipeline**: The shell pipeline inside the CI step that turns live audit output and the committed allow-list into byte-comparable streams. After this feature: both streams pass through the line-number-stripping step before the `LC_ALL=C sort -u | diff -u`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After this feature ships, a PR that adds a 50-line helper above any allow-listed walker passes the walker-audit CI step without any allow-list changes. The "noise tax" of regeneration commits drops to zero for the line-shift-only case.
- **SC-002**: A PR that adds a genuinely new hand-rolled walker (a function whose name matches the audit pattern in a source file outside the allow-list) still fails CI red. The gate's catch-rate for the case it was designed to prevent is unchanged at 100%.
- **SC-003**: A PR that renames, signature-changes, relocates, or removes an allow-listed walker still fails CI red unless the allow-list is updated in the same PR. The five reviewer-attention-worthy change classes (new / removed / renamed / signature-changed / relocated) are all preserved as fail-closed gates.
- **SC-004**: The maintainer cognitive load — measured as the count of "ignore me, just line numbers" allow-list regeneration commits — drops from a frequency proportional to "any PR that inserts code above a walker" to zero. The gate's signal-to-noise ratio shifts entirely toward signal.
- **SC-005**: The CONTRIBUTING.md regeneration snippet contributors copy-paste when they add a new exception produces a byte-stable output matching the new allow-list shape on every supported host. Cross-host instability is impossible because `LC_ALL=C sort -u` already pins ordering.

## Assumptions

- The new allow-list shape is `<file>:<matched-line-content>`. Alternative shapes considered include preserving the line number as `:0:` (placeholder), but they'd add no value while preserving some of the column noise. The shape committed to in this spec is the cleanest variant.
- The CI step's failure-message contract from milestone 115 is unchanged. Only the diff content's shape changes — entries are shorter by one column. The headline, the diff hunks, the trailing pointer are byte-identical to milestone 115's contract.
- The contributor-facing regeneration command in CONTRIBUTING.md changes from `grep -rEn --include='*.rs' 'fn walk[_(]' ... | LC_ALL=C sort -u > allowlist.txt` to a form that strips the line-number column before sort+dedupe. The exact shape (a single `sed` pipeline, an `awk` filter, or a Rust helper) is a planning-phase choice; the user-visible contract is "the contributor runs ONE shell pipeline and gets a byte-correct allow-list."
- The bootstrap is a single PR. There is no "phase 1: accept both shapes, phase 2: enforce new shape" rollout. The shape change is total at PR-ship time, mirroring milestone 115's strict-enforcement bootstrap stance.
- The milestone-115 strict-enforcement bootstrap rule still applies: a missing or empty allow-list file fails CI red. This feature doesn't relax that rule; it just changes the entry shape.
- The walker-audit gate continues to live in the `Lint + test (linux-x86_64)` job, before the Rust toolchain install (same slot milestone 115 chose). No CI re-slotting in this feature.
- A small "noise test" — a synthetic PR that adds an unrelated helper above an existing walker — is the principal validation mechanism. Combined with the unchanged negative test from milestone 115 (synthetic new walker addition still fails red), the two tests together verify the contract: signal preserved, noise eliminated.
- The 28 walker entries existing at PR-ship time will produce a 28-line allow-list in the new shape after bootstrap. Whether that count changes between this PR's open and merge is irrelevant; the bootstrap regenerates against whatever is in the tree at merge time.
- Downstream consumers of the walker-audit allow-list (none currently exist — it's an internal CI artifact) would not be affected by the shape change. The allow-list is not part of mikebom's user-visible contract; it's a contributor-experience implementation detail.
