# Quickstart — Triaging the Walker-Audit Gate After Issue #347

**Feature**: 117-line-stable-allowlist
**Audience**: a contributor whose PR just hit a red `Walker-audit allow-list check` step in CI; a maintainer reviewing a PR that touches the allow-list; a future maintainer who needs to add a new walker exception.

## The TL;DR

After this feature ships, the walker-audit gate **stops false-positiving on line-number drift**. The diff hunks in CI failures are shorter and easier to read (no `:1249:` columns). Everything else stays identical to milestone 115.

Specifically:

- **Routine PR with unrelated edits**: the gate stays quiet (as before).
- **Insert a 50-line helper above an existing walker**: the gate STAYS quiet now (used to fail red with milestone-115 line-number drift).
- **Add a new hand-rolled walker**: the gate fails red (as before — same message contract).
- **Rename an existing walker**: the gate fails red (as before — the content of the matched line changed).
- **Delete an existing walker AND remove the allow-list entry**: the gate stays quiet (as before — clean removal).

## Five-minute walkthrough — Scenario A: accidental new walker

(Identical to milestone 115's quickstart. Nothing changes here.) You added a new ecosystem reader at `mikebom-cli/src/scan_fs/package_db/elixir/mix_lock.rs` and the CI failed. The failure message in the CI log shows the unified-diff hunks pinpointing your new `fn walk_for_mix_locks` function. You refactor to call `safe_walk` instead. CI goes green.

The only visible difference vs milestone 115 is that the diff hunks no longer carry `:NNN:` columns:

```diff
- mikebom-cli/src/scan_fs/package_db/elixir/mix_lock.rs:88:fn walk_for_mix_locks(root: &Path) -> Vec<PathBuf> {
+ mikebom-cli/src/scan_fs/package_db/elixir/mix_lock.rs:fn walk_for_mix_locks(root: &Path) -> Vec<PathBuf> {
```

Shorter, more readable. Same triage flow.

## Five-minute walkthrough — Scenario B: NEW (post-#347) — legitimate refactor that inserts code above an existing walker

You're refactoring `maven.rs` and add a 90-line `extract_pom_plugin_final_name` helper at line 587 — well above the existing `fn walk_m2_jars` at line 1249. Pre-#347, this would have failed CI red because every walker function below line 587 shifted down by ~90 lines, producing 14 line-number-only diff entries.

Post-#347: CI stays green. The grep run + sed-strip + sort produces the SAME normalized output as the committed allow-list because content (file path + matched line content) is unchanged for every walker. Only their positions in the file changed, and positions no longer affect the fingerprint.

You commit your refactor, push, CI is green, reviewer doesn't see an allow-list diff at all. No noise commit.

## Five-minute walkthrough — Scenario C: legitimate new exception

(Identical to milestone 115's "Scenario B" with the regenerate command updated.) Your Swift Package Manager reader genuinely can't fit `safe_walk`. In the SAME PR, you do two edits:

**Edit 1** — Regenerate the allow-list with the new pipeline:

```bash
LC_ALL=C grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ \
  | sed 's/^\([^:]*\):[0-9]*:/\1:/' \
  | LC_ALL=C sort -u \
  > mikebom-cli/src/scan_fs/walk.audit-allowlist.txt
```

(The only difference vs the milestone-115 regen command is the added `| sed '...'` step.) `git diff mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` should show **exactly one** added line in NEW shape:

```diff
+mikebom-cli/src/scan_fs/package_db/swift_pm.rs:fn walk_swift_packages(...) {
```

**Edit 2** — Add the one-sentence reason in `walk.rs`'s comment block:

```rust
//! ## Documented known exceptions
//! ...
//! - `scan_fs/package_db/swift_pm.rs::walk_swift_packages` — Swift package
//!   manifests carry pre-declared sub-directory targets that prune mid-descent
//!   based on per-package-state; the generic should_skip can't express
//!   per-descent stateful pruning. See PR #NNN.
```

Both edits commit-together. CI goes green. Reviewer sees both the source-tree edit AND the policy edit in one PR.

## Maintainer triage — Reviewing a PR that adds an allow-list entry

Same as milestone 115's quickstart, with one addition:

| Check | Action |
|---|---|
| One new entry, not several | Multiple new entries are a code smell; push back |
| Comment-block entry in `walk.rs` | Verify the one-sentence reason is present; request if missing |
| Reason is concrete | "Doesn't fit safe_walk" is not a reason; "per-descent stateful pruning" is |
| "Why exempt" is testable | Could a reasonable refactor of `safe_walk` accommodate this? If yes, ask for the refactor instead |
| Entry is sorted in correctly | `LC_ALL=C sort -u <file> \| diff - <file>` produces empty output |
| **NEW: entry is in NEW shape** | `<file>:<content>` with no `:NNN:` middle column. (Forgiveness path: an OLD-shape entry still compares correctly via the sed-strip, but the committed file should be NEW shape per FR-002.) |

## Negative test — Verifying the gate's signal preservation

Drop in a synthetic file with a real-looking new walker:

```bash
cat > mikebom-cli/src/scan_fs/synthetic_negative_test.rs <<'EOF'
// THIS FILE EXISTS ONLY TO VERIFY THE WALKER-AUDIT GATE BLOCKS UNEXPECTED ADDITIONS.
fn walk_synthetic_negative(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    vec![root.to_path_buf()]
}
EOF
```

Expected: CI fails red with the unchanged milestone-115 failure-message contract (headline, diff hunks identifying the synthetic file, trailing pointer). The diff hunk shape is in NEW form:

```diff
+ mikebom-cli/src/scan_fs/synthetic_negative_test.rs:fn walk_synthetic_negative(root: &std::path::Path) -> Vec<std::path::PathBuf> {
```

After verifying, delete the synthetic file. CI goes green.

## Positive test — Verifying the gate's noise elimination

This is the new test that motivated the feature. Drop in a 50-line helper function above an existing allow-listed walker:

```bash
cat > /tmp/synthetic-helper.rs <<'EOF'

fn synthetic_helper_zero() {}
fn synthetic_helper_one() {}
fn synthetic_helper_two() {}
fn synthetic_helper_three() {}
fn synthetic_helper_four() {}
fn synthetic_helper_five() {}
fn synthetic_helper_six() {}
fn synthetic_helper_seven() {}
fn synthetic_helper_eight() {}
fn synthetic_helper_nine() {}
EOF

# Insert at the top of maven.rs (above the existing walk_m2_jars):
sed -i.bak '1r /tmp/synthetic-helper.rs' mikebom-cli/src/scan_fs/package_db/maven.rs
```

Expected post-#347: CI stays green. Every existing walker's line number in `maven.rs` shifted by 10, but the sed-strip in the gate ignores line numbers.

Expected pre-#347 (for reference): CI would have failed red with 14 line-number-only deltas for every `walk_*` function in `maven.rs`.

After verifying, restore: `mv mikebom-cli/src/scan_fs/package_db/maven.rs.bak mikebom-cli/src/scan_fs/package_db/maven.rs`.

## Related docs

- [`spec.md`](./spec.md) — the user-visible contract
- [`research.md`](./research.md) — 4 implementation decisions
- [`data-model.md`](./data-model.md) — allow-list entry shape change + pipeline lifecycle
- [`contracts/ci-step.md`](./contracts/ci-step.md) — the CI step contract that supersedes parts of milestone 115
- [`specs/115-walker-audit-ci/`](../115-walker-audit-ci/) — the milestone 115 spec this feature follows up
- Issue #347 — the motivating incident report
- `CONTRIBUTING.md § Walker-audit CI gate` — the contributor-facing doc-of-record (updated by this feature's T-tasks)
