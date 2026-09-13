# Feature Specification: what `--root-name` does when a scan has more than one main module

**Feature Branch**: `860-multi-main-module-override`
**Created**: 2026-09-13
**Status**: Draft
**Input**: Decide the N>1 main-module policy for the operator root override (issue #863)

> Spec Kit numbers features sequentially. Feature `860` is unrelated to
> pull request #860; the issue this addresses is **#863**.

## Context

When an operator names the SBOM subject with `--root-name`, waybill
replaces the auto-derived root. Milestone 077 chose *clean replacement*:
the manifest-derived main-module component is removed from
`components[]` so the document does not carry two roots. Milestone 149
added `--preserve-manifest-main-module`, which keeps that component
demoted to a library instead.

Both decisions assumed **one** main module. Milestone 149 explicitly
declined to handle more, at `root_selector.rs:525`:

> Multi-main-module scans (per milestone 127 — Cargo workspace,
> polyglot) where N>1 components carry the main-module role tag fall
> through to the drop path even when `preserve_main_module` is true,
> because there's no SINGLE manifest-derived main-module to demote.

So on any workspace with several modules, naming a root deletes **every**
module from the SBOM, and `--preserve-manifest-main-module` does not
prevent it. This feature decides what should happen instead.

## Observed impact

Measured on the committed public-corpus goldens (all eleven targets are
scanned with `--root-name`, so all are exposed to the policy):

| target | main-module components | components without override | with override | removed | dangling `dependsOn` refs |
|--------|-----------------------:|----------------------------:|--------------:|--------:|--------------------------:|
| maven-guice | 16 | 61 | 45 | 16 | 5 |
| rust-ripgrep | 10 | 68 | 58 | 10 | 9 |
| python-flask | 4 | 109 | 105 | 4 | 0 |
| other eight targets | 0 | — | — | 0 | 0 |

Three of eleven targets are affected. The other eight have **zero** main
modules, so no policy choice touches them. Notably **no corpus target
has exactly one** main module, which is why the N=1 path is invisible
here and why converging on a single policy costs nothing in golden
churn.

`python-flask` shows the failure in its plainest form: it has no
dangling references, because nothing depends on the four modules that
disappear. They are simply gone — including `pkg:pypi/flask@3.1.2`.
Scanning the Flask repository and naming the subject currently removes
Flask itself from the inventory.

The removed components are real: they are the workspace's own modules,
and sibling modules declare dependencies on them. Those dependency edges
are **not** removed with the component, so the emitted graph references
fourteen components that do not exist in the document.

This is pre-existing. The same dangling references appear in goldens
generated before and after the milestone-856 version fix; that fix only
made them easier to see, by removing the `@unknown` twin components that
were previously standing in for the same coordinates.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer walks the dependency graph (Priority: P1)

A security tool ingests a waybill SBOM produced with `--root-name` and
walks `dependencies[]` to build the graph.

**Why this priority**: A reference that resolves to nothing is the
failure that reaches other tools. It is the only symptom in this feature
that a third party observes directly.

**Acceptance**: Every `dependsOn` target (CDX) and every relationship
endpoint (SPDX 2.3, SPDX 3) resolves to a component present in the same
document, for every corpus target, with and without `--root-name`.

### User Story 2 - An operator names the subject of a workspace scan (Priority: P1)

An operator scans a multi-module repository and passes `--root-name` to
label the SBOM subject as their product rather than whichever module
waybill picked.

**Why this priority**: This is the flag's advertised purpose, and today
it silently removes most of the inventory. On the two affected corpus
targets that is 16 of 61 and 10 of 68 components.

**Acceptance**: Naming the subject changes the document's declared
subject. It does not reduce the set of components discovered in the
scanned tree.

### User Story 3 - A maintainer reads why a module is not the root (Priority: P3)

A maintainer inspects an SBOM from an override scan and wants to know
which components were main modules before the override applied.

**Why this priority**: Useful for debugging root selection; not required
for correctness.

**Acceptance**: A component that carried the main-module role before the
override is distinguishable from one that never did.

### Edge Cases

- **Exactly one main module.** The existing milestone-077 and
  milestone-149 behaviour is already specified and in use. Any change to
  the N>1 path must leave N=1 alone unless this spec says otherwise.
- **Zero main modules.** Override applies to `metadata.component` only;
  nothing to drop or keep. Unchanged.
- **A main module that nothing depends on.** Removing it produces no
  dangling reference, so the graph stays valid — but the component is
  still absent from the inventory.
- **The override names a coordinate that matches an existing module.**
  The root absorbs it: the module is not emitted separately and its
  outbound edges attach to the root (FR-011). Reachable because waybill
  mints `pkg:generic/` main modules for some ecosystems, the same
  namespace the override uses. No current corpus target exhibits this,
  so it needs a synthetic test rather than a corpus assertion.
- **`--preserve-manifest-main-module` passed on an N>1 scan.** Today it
  is silently a no-op with an INFO diagnostic. Whatever policy is chosen,
  the flag's behaviour on this path must be stated rather than left to
  fall through.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every dependency reference in an emitted document MUST
  resolve to a component present in that same document, in all three
  formats.
- **FR-002**: Naming the SBOM subject MUST NOT remove components that
  were discovered in the scanned tree.
- **FR-003**: The document MUST declare exactly one subject. Keeping
  workspace modules as components MUST NOT produce a second root.
- **FR-004**: A component that held the main-module role before an
  override MUST remain distinguishable from one that never held it.
- **FR-005**: One policy MUST apply at every value of N. The N=1 path
  converges on the policy defined here, superseding the milestone-077
  clean-replacement default and making the milestone-149 demote
  behaviour unconditional. Behaviour MUST NOT depend on a count the
  operator cannot see and did not specify.
- **FR-006**: `--preserve-manifest-main-module` MUST continue to be
  accepted, and becomes a no-op because its behaviour is now
  unconditional. It MUST NOT error, and its help text MUST say it is
  retained for compatibility and no longer changes output. Removing it
  would break existing callers for no gain.
- **FR-007**: A retained former main module MUST keep its own outbound
  dependency edges. Its edges MUST NOT be re-anchored onto the override
  root, superseding the milestone-149 decision for this path.
- **FR-008**: The override root MUST declare a dependency on **every**
  retained former main module — a flat fan-out, independent of the
  inter-module edges, so the emitted graph is root → modules →
  libraries and reachability does not depend on inbound-edge data being
  complete. Emitting root edges only for modules nothing else depends on
  is explicitly rejected: a missing inbound edge would silently orphan a
  module.
- **FR-011**: When a retained module's PURL equals the override root's
  PURL, the module MUST NOT be emitted as a separate component, and its
  outbound edges MUST attach to the root. No two components may assert
  the same coordinate.
- **FR-009**: The chosen policy MUST apply identically across CycloneDX,
  SPDX 2.3 and SPDX 3, via the existing shared helper rather than three
  parallel implementations.
- **FR-010**: Any change in emitted output MUST be reflected in the
  public-corpus goldens in the same change, with the diff attributed.

### Key Entities

- **Main-module component** — a component carrying the
  `waybill:component-role = main-module` annotation, assigned by root
  selection (milestone 127).
- **Root override** — the operator-supplied subject identity from
  `--root-name` / `--root-version`.
- **Redirected PURL set** — the coordinates whose outbound edges are
  re-anchored onto the override root today. Inbound edges are not
  currently considered; this feature's defect lives in that asymmetry.
  Under FR-007/FR-008 this set stops driving edge removal and instead
  identifies which components the root should depend on.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Zero unresolvable dependency references across all eleven
  corpus targets in all three formats. Currently fourteen, across two
  targets.
- **SC-002**: On `maven-guice`, component count under `--root-name`
  matches the count without it. Currently 45 versus 61.
- **SC-003**: On `rust-ripgrep`, the same. Currently 58 versus 68.
- **SC-004**: On `python-flask`, the same: 105 becomes 109, and
  `pkg:pypi/flask@3.1.2` is present.
- **SC-005**: The four targets with zero main modules
  (`image-postgres16`, `pants-example-django`, `pants-example-jvm`,
  `pants-example-python`) are byte-identical before and after. The
  policy must not disturb scans that have nothing to retain.
  *(Corrected from "eight": four further targets turned out to have
  exactly one main module — see research.md R6.)*
- **SC-006**: `--preserve-manifest-main-module` is accepted and produces
  output identical to omitting it.
- **SC-007**: Exactly one subject is declared per emitted document, for
  every target, verified in all three formats.
- **SC-007a**: Every retained former main module is reachable from the
  subject in the emitted graph — zero orphaned modules on the three
  affected targets.
- **SC-007b**: Retained modules keep their own edges: on `maven-guice`
  the inter-module dependency edges present without `--root-name` are
  present with it.
- **SC-008**: The public-corpus lane is green on the refreshed goldens,
  and the accompanying diff is attributed per
  `docs/development/refreshing-corpus-goldens.md`.

## Clarifications

### Session 2026-09-13

- Q: Does the N=1 path converge on the new policy, or stay as it is? →
  **A: Converge.** One rule for all N. Measurement showed no corpus
  target has exactly one main module, so convergence adds no golden
  churn beyond the three N>1 targets — it is the cheaper option here as
  well as the simpler one to explain.
- Q: What happens to a retained module's outbound dependency edges? →
  **A: Retain them, and anchor the root to the modules.** The override
  root declares a dependency on each retained former main module, giving
  root → modules → libraries. This supersedes the milestone-149
  re-anchoring decision (recorded 2026-06-29) for this path: absorbing
  every module's edges onto the root flattens the workspace layer, and
  retaining edges without anchoring the root leaves the subject
  disconnected from everything it contains.
- Q: Which retained modules does the root depend on? → **A: Every one
  of them**, a flat fan-out, with inter-module edges retained alongside.
  Selecting only the modules nothing else depends on would mirror a
  reactor more closely, but it relies on inbound-edge data being
  complete — and this project routinely scans in conditions where it is
  not (offline, no module cache; see #857). A missing inbound edge would
  then silently orphan a module, which is the failure this feature
  exists to remove.
- Q: What if a retained module's identity equals the override root's? →
  **A: The root wins and absorbs it.** The module is not emitted as a
  separate component and its outbound edges attach to the root. The
  operator named the subject, so the subject prevails; merging also
  avoids two components asserting the same coordinate. Reachable in
  principle because waybill mints `pkg:generic/` main modules for some
  ecosystems (pip apps, npm CLI tools — `scan_cmd.rs:1648`), the same
  namespace the override uses; no current corpus target exhibits it.
- Q: What happens to `--preserve-manifest-main-module`? → **A: Keep it
  as an accepted, documented no-op.** Its behaviour becomes
  unconditional, so the flag has nothing left to do; removing it would
  break existing callers for no benefit.

## Assumptions

- The intended reading of `--root-name` is "name the subject", not
  "reduce the inventory". The flag's documentation describes subject
  naming; component removal was a consequence of the clean-replacement
  design rather than a stated goal.
- Keeping a former main module as a library-typed component is
  acceptable to consumers, since milestone 149 already ships exactly that
  shape behind a flag for the N=1 case.
- The fourteen dangling references are a defect rather than an accepted
  trade-off. No test asserts them, and no documentation describes them.
- Corpus goldens will change for the two affected targets; the procedure
  for that exists and is documented.

## Out of Scope

- Root *selection* — how waybill picks a main module when the operator
  does not name one (milestones 127, 201).
- The `--split` modes, which project workspaces into separate documents
  by a different mechanism.
- Issue #855 (self-referential SPDX relationships) and #857 (Go orphan
  classification), which also concern graph shape but have unrelated
  causes.
