# Phase 0 research

No `NEEDS CLARIFICATION` entries remained in the Technical Context — the
three that existed were resolved in the spec's clarification session, and the
format questions were resolved by measurement before the spec was written.
What follows are the decisions that measurement and code reading settled.

## R1. Where the development axis lives

**Decision**: bun reads it from the lockfile; yarn reads it from
`package.json` on disk, for both dialects.

**Rationale**: Measured, nine cells, two rounds — `measurements/findings.md`.
bun records `workspaces.<path>.devDependencies` for the root and for every
member. yarn v1 does not contain the substring `dev` at all. Berry records a
`"<name>@workspace:<path>"` entry per workspace with an explicit
`dependencies:` map, but that map is the union of the manifest's
`dependencies` and `devDependencies`, so the axis is discarded at write time.

**Alternatives considered**: Reading Berry's workspace entry as the evidence
source, which looks right and is wrong — this is the measurement that most
repaid being taken. Inferring development status from absence of a runtime
path, rejected because it is heuristic gap-filling and a package can be
absent from every runtime path for reasons other than being development-only.

## R2. The classification rule

**Decision**: Reuse milestone 1270's `v9_importer_scopes` shape
(`npm/pnpm_lock.rs:200`) in both readers: collect development-declared
identities, collect runtime and optional declared identities, transitively
close the runtime set over the lockfile's own edges, then subtract the
closure from the development set.

**Rationale**: It is already proved against a real pnpm fixture, and FR-007
makes cross-reader agreement a requirement — two readers implementing two
rules would be the defect FR-007 names. The runtime-wins ordering is what
protects Principle IX: a dependency reachable at runtime from any workspace
must not be marked development-only because some other workspace calls it a
dev tool.

**Alternatives considered**: Classifying by direct declaration only, with no
closure — rejected because milestone 1269's propagation pass needs a seed
whose transitive closure is genuinely development-only, and an unsubtracted
seed would propagate development scope onto runtime packages. Sharing one
helper across all three readers — attractive, but each reader's identity
rules and edge representation differ (pnpm snapshot keys, bun package arrays,
yarn descriptors), so the shared thing would be a trait plus three adapters
for three call sites. Deferred as a refactor that should not ride along with
a fix.

## R3. yarn workspace-member discovery

**Decision**: Discover members in both dialects. Berry member paths come
from the lockfile's own member entries; v1 member paths come from expanding
the root manifest's `workspaces` patterns. Both the bare-array and the
object-with-`packages` spellings are handled.

**Rationale**: Measured — a member-only development dependency is resolved
into all three lockfiles and classified by none of them, so a root-only
reading would leave a real third-party package emitted and permanently
unclassifiable. All three package managers accept both `workspaces`
spellings, so the expansion surface is two shapes, not open-ended.

**Alternatives considered**: Root manifest only, rejected as under-serving a
monorepo. Berry members but not v1, rejected because two dialects of one
package manager would then classify the same project differently, which
FR-007 defines as a defect of this feature.

## R4. Transparency for the FR-012 limitation

**Decision**: No new annotation and no new output field. The diagnostic
FR-006 already emits gains a count of declarations skipped because they
resolve to no locked package.

**Rationale**: Principle X triggers when completeness or accuracy cannot be
guaranteed. An absent optional scope assertion is neither a missing
component nor a wrongly included one, and it is the pre-existing state for
every yarn and bun component today, so X is not triggered and no violation
needs justifying. But US3's promise is that a document which cannot be
filtered says so, and a `patch:`-wrapped development dependency silently
surviving `--exclude-scope dev` is the shape of complaint #1297 makes. A
number in a log line the feature already emits honours that promise at no
cost to FR-008, which governs output fields rather than diagnostics.

**Amendment, made at implementation time**: the count is emitted on the
FR-006 diagnostic as the requirement states, *and* on the summary line the
feature emits when classification ran. FR-006's diagnostic fires only when
the root manifest is absent — and with no root manifest there are no
declarations to skip, and for yarn v1 no `workspaces` patterns either, so
that count is structurally zero. The placement FR-006 names is the one place
the number cannot carry information. An operator chasing a surviving
`patch:`-wrapped development dependency — the shape of complaint #1297
makes — is on a project whose manifest parses fine. Both lines carry it;
neither is a new output field, so FR-008 is intact either way.

**Alternatives considered**: A per-component annotation naming the skipped
protocol — contradicts FR-008 outright. Complete silence — permissible under
the letter of the principles, but it would make the limitation
indistinguishable from an oversight at exactly the moment an operator cares.
Reusing milestone 236's `waybill:unresolved-reason` (C151) — examined and
rejected: that annotation explains why a *component* is unresolved, not why a
*scope* is unassigned, and bending it would corrupt a catalog row with a
second meaning.

## R5. Verifying FR-007 without conflating two kinds of disagreement

**Decision**: Establish the cross-reader baseline before implementation, as
its own task, and record it in `measurements/`.

**Rationale**: FR-007 blocks on a disagreement this feature introduces and
merely records one that predates it. That distinction is undecidable after
the fact — once the readers change, a mismatch cannot be attributed without
knowing what the mismatch was beforehand. The baseline must be captured
first or SC-003 degrades into a judgment call.

**Constraint discovered while planning**: `npm/mod.rs:252-266` is an
`if/else-if` chain, so exactly one lockfile reader runs per project root. The
baseline therefore cannot be a single scan — it is one fixture directory per
lockfile type over one shared manifest, scanned separately and compared. A
project carrying both `package-lock.json` and `yarn.lock` never reaches the
yarn reader at all.

**Alternatives considered**: Comparing readers inside one scan, impossible
for the reason above. Taking the baseline afterwards and reasoning backwards,
which is the failure this project has recorded repeatedly — a theory about
what the state used to be is not a measurement of it.

## R6. Diagnostic severity

**Decision**: Warn level for unavailable evidence, matching the existing
`yarn_lock.rs:52` precedent, which already warns when `package.json` is
missing or unparseable for the milestone 181 optional classifier.

**Rationale**: The spec says "operator-visible", which constrains the level
to one visible by default. Reusing the adjacent log site's level keeps one
reader from having two conventions for the same class of event.

**Alternatives considered**: Info level, rejected because it is not reliably
visible and the point of the diagnostic is that the operator notices.

**Correction, made at implementation time**: the cited precedent does not
exist. `yarn_lock.rs:52` is `tracing::debug!`, not `warn!` — it is milestone
181's FR-004 fail-safe note, a different requirement whose level is not this
feature's to change. The decision stands, but on the spec's
"operator-visible" constraint alone, which info does not satisfy. Left here
rather than rewritten, because a research decision resting on a precedent
that was never checked is the kind of thing worth being able to find again.
