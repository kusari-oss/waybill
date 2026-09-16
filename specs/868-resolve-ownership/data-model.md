# Phase 1 — Data Model

Only what changes. Existing types described far enough to say what happens to
them.

---

## Resolve component (new)

A component standing for one named resolve, sitting between the document root
and that resolve's top-level requirements.

| Property | Value |
|---|---|
| Identity | `pkg:generic/` with the resolve's declared name (research R3) |
| Source | the `[python.resolves]` entry that names it — never inferred from directory layout |
| Nature | explicitly marked as a resolve, not a package (FR-002a) |
| Lifecycle | build-time when a tool declares `install_from_resolve`; runtime otherwise (FR-003a/b) |
| Cardinality | one per declared resolve. Eight on the measured target |

**Why it is not a package.** It has no upstream, no version to fetch, and no
vulnerability surface of its own. A consumer treating it as a package would
try to resolve it against an index and fail, or report it as an unscannable
dependency. FR-002a's marker exists to prevent exactly that, and is why the
`pkg:generic/` PURL type alone is not considered sufficient.

**Not created when**: no resolve is declared. A project with no lockfile
resolve gains nothing and emits nothing new (FR-007).

---

## Resolve membership (existing — `waybill:pants-resolve`, C143)

**No change.** Already emitted on 248 of 272 pypi components on the measured
target, already carrying the resolve name verbatim, already has extractors in
all three formats.

This is the grouping the resolve component expresses structurally. FR-005 and
SC-005 are therefore **already satisfied** and the plan verifies rather than
builds them.

The 24 pypi components carrying no resolve tag came from manifest
declarations rather than a lockfile. They are out of this feature's scope:
they belong to no resolve, so no resolve can own them.

---

## Anchor edges (new)

Two kinds, both ordinary dependency edges so that consumers walking the graph
reach them without special handling:

| Edge | Meaning |
|---|---|
| root → resolve component | this project uses this pinned set |
| resolve component → top-level requirement | this resolve was asked to provide this |

**Top-level requirement**: a package in a resolve that nothing else *in that
resolve* depends on. 99 on the measured target. Determined per-resolve, not
globally — a package can be a top-level requirement of one resolve while
being a transitive dependency inside another.

**No edge is created into a package the scan did not observe** (FR-008). This
feature connects what exists; it never invents a target.

---

## Lifecycle classification (existing — `LifecycleScope`)

No shape change. What changes is **how the value is derived**:

| | Today | After |
|---|---|---|
| Signal | resolve name matched against a hardcoded allowlist | `install_from_resolve` declaration, falling back to the allowlist |
| Precedence | allowlist only | declaration wins; allowlist applies only where nothing declares |
| Reporting | none | count classified by fallback is emitted (FR-003c) |
| Default | runtime | runtime, unchanged |

The default direction is deliberate and unchanged: mis-marking a runtime
resolve as build-time hides packages from a consumer filtering for runtime
risk, while the converse over-reports. Only the first failure is silent.

**Measured consequence** on the target: `coverage-py` and `setuptools` move
from runtime to build-time, correcting two live misclassifications. No
resolve moves the other way.

---

## Validation rules

| Rule | Source | Enforced at |
|---|---|---|
| Resolve contents reachable from root | FR-001 | anchor construction; asserted against the emitted graph |
| One component per declared resolve | FR-002 | construction |
| Resolve component distinguishable from a package | FR-002a | emission |
| Package in several resolves reachable via each | FR-002b / FR-006 | falls out of per-resolve anchoring |
| Declaration beats name heuristic | FR-003a (as amended per research R1) | classification |
| Undeclared ⇒ runtime | FR-003b | classification |
| Fallback-classified count reported | FR-003c | emission; always present, including zero |
| No resolve ⇒ output unchanged | FR-007 | construction; asserted across the corpus |
| No package component invented | FR-008 | construction; component count asserted |
| Self-report agrees with the graph | FR-009 | BFS consumes the emitted edges (research R5) |
