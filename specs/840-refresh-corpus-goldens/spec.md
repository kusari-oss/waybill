# Feature Specification: Refresh the public-corpus goldens with verified drift

**Feature Branch**: `840-refresh-corpus-goldens`
**Created**: 2026-09-11
**Status**: Draft
**Input**: User description: "Refresh the stale public-corpus goldens across all drifting targets, verifying every delta is intended rather than rubber-stamping the regen (issue #763)"

## Clarifications

### Session 2026-09-11

- Q: If a target is failing for a reason other than emission drift (a real bug, not accumulated churn), what should this feature do? → A: Investigate it; fix it here if the fix is small, otherwise remove it from the lane's gating set with a tracked issue. Either way the lane must still pass 100% of what it gates.
- Q: How should the refresh be delivered across the failing targets? → A: One change covering every failing target together, so the lane goes red-to-green once and a reviewer can compare deltas across targets — the comparison that would expose an anomalous one.
- Q: Where should the per-target delta attribution evidence live? → A: In the pull request description for the refresh. Durable via git history and reachable from the commit, with nothing new to maintain — no committed document that will read as current long after it isn't.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The nightly corpus lane reports real regressions again (Priority: P1)

A maintainer watches the nightly public-corpus lane. Today it fails every night on all eleven targets, so nobody reads it — a lane that always fails carries no information, and a genuine regression arriving tomorrow would be indistinguishable from the existing noise.

After this change the lane passes, and a future failure means something changed.

**Why this priority**: This is the reported defect. It is also the only story that restores the lane's purpose; everything else here exists to make sure restoring it does not cost more than it gains.

**Independent Test**: Run the corpus lane against an unchanged tree and observe it pass on every target. Then introduce a deliberate emission change and observe it fail, proving the lane still detects drift rather than having been widened into uselessness.

**Acceptance Scenarios**:

1. **Given** the refreshed goldens and an unchanged tree, **When** the corpus lane runs, **Then** every gated target passes.
2. **Given** the refreshed goldens, **When** a deliberate change to emitted output is introduced, **Then** the lane fails and names the affected target and format.
3. **Given** the refreshed goldens, **When** the lane runs twice against the same tree, **Then** both runs agree — the goldens are reproducible, not captured from one lucky execution.

---

### User Story 2 - Every accepted change is understood before it is committed (Priority: P1)

A reviewer can see, for each target, what changed between the old golden and the new one and why that change is expected. The refresh is accompanied by evidence that each delta traces to a known merged change rather than to an unnoticed regression.

The goldens were last updated on 2026-07-21 and roughly 147 merges have landed since. At least one of those rewrites nearly every component, so the raw diff is large and mostly benign — which is exactly the condition under which a real regression hides comfortably.

**Why this priority**: Equal to Story 1. A refresh that silently accepts whatever the code currently emits converts a stale gate into a gate that certifies the present, which is worse than leaving it red — it looks green while asserting nothing. The verification *is* the feature; the file update is the easy part.

**Independent Test**: A reviewer reading the change can, for any target, identify which categories of delta occurred and what caused each, without re-deriving it themselves.

**Acceptance Scenarios**:

1. **Given** a refreshed target, **When** a reviewer reads the pull request description, **Then** each category of change for that target is attributed to an identified cause.
2. **Given** a delta that cannot be attributed to a known change, **When** it is found, **Then** it is investigated and either explained or raised as a defect before the refresh proceeds.
3. **Given** the diff for any target, **When** it is produced for review, **Then** content-addressed identifiers, ordering-only differences and other non-semantic churn are normalised away first, so the reviewer sees semantic change only.

---

### User Story 3 - The refresh method is repeatable by the next maintainer (Priority: P2)

A maintainer facing the same staleness in six months can follow a documented procedure rather than rediscovering it. That procedure names where goldens must be generated and why, and what evidence a refresh must carry.

**Why this priority**: Lower than P1 because the lane is restored without it, but this is the third time corpus and baseline artifacts have gone stale and been re-derived from scratch. Writing the method down is what stops a fourth.

**Independent Test**: A maintainer who did not perform this refresh can follow the documented procedure and produce an equivalent result.

**Acceptance Scenarios**:

1. **Given** the documented procedure, **When** a maintainer follows it, **Then** they can refresh goldens and produce the required evidence without consulting the person who wrote it.
2. **Given** the procedure, **When** a maintainer attempts to generate goldens in a way the procedure forbids, **Then** the reason for the prohibition is stated where they will encounter it.

---

### Edge Cases

- A golden is generated in an environment that differs from the one the gate runs in. Goldens capture environment-dependent content, so the lane would then fail for everyone except the person who generated them. This has already happened twice on sibling artifacts: a baseline recorded on one machine class and compared on another, and a fixture whose content depended on optional tooling being installed on the generating host.
- A delta appears that nobody can attribute. Accepting it because the diff is large and the rest is explained is precisely how a regression enters a golden.
- Ordering-only differences in unordered collections present as semantic change. Without normalisation these dominate the diff and conceal real changes beneath volume.
- Content-addressed identifiers change because their inputs changed, cascading into identifiers that reference them. Distinguishing "the hash changed because the content legitimately changed" from "the content changed unexpectedly" requires masking the derived values before comparison.
- A target's upstream repository moves or becomes unavailable during refresh. Since the refresh lands as one change (FR-013), a target that cannot be regenerated blocks the change rather than producing a half-refreshed tree; it is resolved under FR-012 — repaired, or dropped from gating with a tracked issue — before the change proceeds.
- Two targets drift for different reasons and one is a genuine regression. Per-target evidence is required; a single aggregate statement that "the diff is expected" cannot distinguish them.
- A target is failing for a reason unrelated to emission drift. Regenerating its golden would encode the fault as expected output, so this is forbidden (FR-012): the target is repaired here or dropped from gating with a tracked issue, and either way the drop is visible in the lane's output (FR-012a).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The refresh MUST cover every target currently failing the corpus lane, across every emitted format.
- **FR-002**: Goldens MUST be generated in the same environment class in which the gate runs.
- **FR-003**: Goldens MUST NOT be generated on a maintainer's personal machine.
- **FR-004**: The refresh MUST produce, per target, a normalised diff in which non-semantic churn has been removed before review.
- **FR-005**: Normalisation MUST neutralise content-addressed identifiers, ordering-only differences within unordered collections, and per-run values such as timestamps and document serial numbers.
- **FR-006**: Every category of remaining delta MUST be attributed to an identified cause before the refreshed golden is accepted.
- **FR-007**: A delta that cannot be attributed MUST block the refresh for that target until it is explained or raised as a defect.
- **FR-008**: The refresh MUST NOT widen, relax or disable any existing assertion in order to make a target pass.
- **FR-009**: After the refresh, the corpus lane MUST pass on every target it gates, against an unchanged tree.
- **FR-010**: After the refresh, an introduced change to emitted output MUST still cause the lane to fail, demonstrating the gate retains its detection ability.
- **FR-011**: Regenerating goldens twice against the same tree MUST produce identical results.
- **FR-012**: A target failing for a reason other than emission drift MUST NOT have its golden regenerated. It MUST instead be either repaired within this feature, or removed from the lane's gating set with a tracked issue recording why. Accepting its current output as the new expected output is forbidden.
- **FR-012a**: When a target is removed from the gating set, the removal MUST be visible in the lane's own output, so that shrinking coverage cannot be mistaken for passing coverage.
- **FR-013**: The refresh MUST deliver every failing target together as one change, rather than incrementally per target. The lane transitions red-to-green once.
- **FR-013a**: Per-target evidence MUST be presented so deltas can be compared ACROSS targets, not only within one. A target whose delta pattern is unlike its peers is the most likely place for a regression to hide, and that signal only exists when they are seen together.
- **FR-014**: The procedure — where goldens are generated, why, what evidence is required — MUST be documented where a future maintainer will find it.
- **FR-015**: The evidence supporting the refresh MUST be presented in the pull request that performs it, reviewable by someone who did not perform it. It MUST NOT be committed as a standalone document — the repository is actively retiring point-in-time documents that outlive their accuracy (#827), and this evidence describes one moment by nature.
- **FR-015a**: The commit message MUST reference the pull request, so a maintainer reading `git log` years later can reach the attribution without knowing it exists.

### Key Entities

- **Corpus target**: one pinned upstream repository or image the lane scans.
- **Golden**: the committed expected output for one target in one format.
- **Normalised diff**: the difference between an old and new golden after non-semantic churn has been removed.
- **Delta attribution**: the mapping from a category of observed change to the merged change that caused it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The corpus lane passes on 100% of the targets it gates, against an unchanged tree. A target removed under FR-012 is no longer gated and is therefore out of this measure — but its removal is recorded and visible per FR-012a, so coverage loss is never silent.
- **SC-002**: Every target that is refreshed has an attributed explanation for each category of delta; zero unattributed categories are accepted.
- **SC-003**: An introduced emission change is still detected by the lane after the refresh, on at least one target per format.
- **SC-004**: Two consecutive golden generations against the same tree produce identical output.
- **SC-005**: Zero assertions are removed, relaxed or disabled by the refresh.
- **SC-006**: A maintainer who did not perform the refresh can restate the generation procedure from the documentation alone.
- **SC-007**: A maintainer reading the refresh commit can reach the per-target attribution evidence without prior knowledge that it exists.

## Assumptions

- The observed drift is expected rather than regressive, because a large number of changes have merged since the goldens were last updated and at least one of them alters nearly every component. This is an assumption the feature must **verify per target**, not a conclusion it may rely on — FR-006 and FR-007 exist precisely because it may be wrong somewhere.
- The corpus lane's purpose is regression detection, not correctness proof. A refreshed golden asserts "this is what we emit today", which is only meaningful once each change from the previous "today" has been understood. That is why the verification requirements carry equal weight to the file update.
- Masking and normalisation capability already exists in the harness and is expected to be reused and extended rather than rebuilt.
- The count of affected targets is taken from the most recent lane run at the time of writing. It may grow before the work starts; FR-001 is written against "currently failing" rather than a fixed number so the scope tracks reality.
- The lane's non-golden assertions are out of scope. This feature refreshes expected output; it does not revisit what the lane checks.
- Whether any currently-failing target is failing for a non-drift reason is unknown at specification time. FR-012 and FR-012a exist so that discovering one cannot silently convert it into an accepted golden, and cannot silently shrink what the lane covers either.
