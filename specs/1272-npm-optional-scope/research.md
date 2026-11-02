# Phase 0 Research: npm-ecosystem optional-axis classification

**Milestone**: 1272 | **Date**: 2026-11-02 | **Spec**: [spec.md](./spec.md)

All five managers were installed and run. Nothing below is read from
documentation. The harnesses are committed beside this file and are
expected to be re-run rather than trusted indefinitely:
`measurements/optional-axis-probe.sh` (where the axis lives) and
`measurements/cancellation-probe.sh` (what the axis means).

Tool versions for every measurement here: node v22.23.1, npm 10.9.8,
pnpm 12.16.1, yarn 1.22.22, yarn 4.6.0 via corepack 0.34.6, bun 1.4.2,
darwin arm64.

---

## R1 — Does a non-optional path cancel optionality? (the spec's one open question)

**Decision**: Yes. FR-006 and FR-007a stand exactly as the spec wrote
them. Nothing in the requirements rests on inference any more.

**Rationale**: `cancellation-probe.sh` built the graph shape neither
earlier probe had: one package reachable by both an optional and a
non-optional path. Probe C of `measurements/findings.md` has the full
record; the decisive rows:

| case | npm `node_modules/ms` | pnpm `snapshots."ms@2.1.2"` |
|---|---|---|
| C1 — root declares `debug` runtime, `ms` optional | `optional: false` | `{}`, no key |
| C2 — root declares `debug` optional only | `optional: true` | `optional: true` |

C1 settles cancellation: both managers refuse to mark `ms` optional
*even though the root declared it optional*, because a runtime path
reaches it. C2 settles transitive propagation in the same run: `ms` is
declared optional nowhere and is marked optional by both, purely by
being reachable only along an optional edge.

Two guards make the result trustworthy rather than merely convenient.
C1 printed exactly **one** `ms` node (2.1.2) — had the two ranges
resolved differently there would have been two nodes and no contested
shape at all. And C1 and C2 **disagree**, so the harness is reading the
managers rather than reporting itself; uniform answers across cases
that should differ is the failure recorded in
`feedback_a_probe_can_lie_about_the_system`.

**Alternatives considered**: Taking the rule from npm's documented
phrasing of the flag, which is what the spec did provisionally. It
happened to be right, but a correct guess and a measurement are not the
same artifact — and the spec committed to measuring this before any
implementation.

---

## R2 — Which managers can answer the closure question, and which only record edges?

**Decision**: npm and pnpm publish a finished closure. yarn v1, yarn
Berry and bun publish only the edge on the declaring parent, so for
those three the closure is waybill's to compute.

**Rationale**: measured in Probes B and C. In C2 both npm and pnpm
marked the *transitively* optional `ms`; Berry's lockfile carried
`dependenciesMeta` on the root workspace entry naming only the root's
own declaration (`debug`), and bun likewise recorded the declaration on
the workspace and the edge in the parent's metadata, with no closure
anywhere in either file.

**Consequence for the work**: this is why the four arms are four
different fixes and not one shared one, and why pnpm is cheapest:

| reader | root declaration | transitive | waybill must close? |
|---|---|---|---|
| pnpm v9 | `snapshots.<id>.optional` | same field | **no** |
| bun | `workspaces.<p>.optionalDependencies` | parent metadata | yes |
| yarn Berry | lockfile entry `dependenciesMeta` ∪ manifest | parent entry `dependenciesMeta` | yes |
| yarn v1 | `package.json` only | parent `optionalDependencies:` sub-block | yes |

---

## R3 — What is actually wrong in the pnpm reader?

**Decision**: one wrong source expression. The classifier is already
built; it reads the flag from the wrong section.

**Rationale**: `pnpm_lock.rs:464-490` already has the complete
three-way classifier — `dev` → Development, `optional && !peer` →
Optional plus the `waybill:optional-derivation` annotation, otherwise
Runtime. It cannot fire for v9 because `is_optional` at
`pnpm_lock.rs:384` reads `tbl`, which is the `packages:` entry, and
measurement showed **v9 `packages:` entries carry no `optional:` key at
all** — the flag moved to `snapshots:`. `build_snapshots_lookup`
(`pnpm_lock.rs:141`) already visits exactly the right entries and
already returns a per-identity map, but discards everything except the
dependency names.

So the pnpm arm is: have that function also carry `optional` and `peer`
per snapshot identity, and for v9 read `is_optional`/`is_peer` from
that map instead of from `tbl`. v6/v7 keep reading `tbl`, which is
where the flag genuinely is for those dialects (FR-002).

**Alternatives considered**: computing the closure ourselves for pnpm
too, for uniformity across the four readers. Rejected — pnpm already
publishes the answer we would be recomputing, and recomputing it would
introduce a second source that can disagree with the lockfile.

---

## R4 — What is wrong in the bun reader?

**Decision**: two defects — the root declaration never seeds, and the
existing cancellation is depth-1 rather than transitive.

**Rationale**: m667's pass tracks per-target optionality
(`bun_lock.rs:675-806`) keyed through `parent_key_to_out_idx`, which by
construction **excludes workspace shapes**. A root or workspace
`optionalDependencies` declaration lives in `workspaces.""`, so it has
no parent entry to be seen from and never enters the tracker. That is
FR-003.

Separately, the finalize pass at `bun_lock.rs:808-860` skips any target
with `state.any_hard`. `any_hard` is set by a non-optional edge from
*any direct parent*, which is cancellation at depth 1 — correct as far
as it goes, but it also suppresses a package whose only parent is
itself optional and which that parent depends on normally. That is
exactly C2's `ms`, which both reference managers mark optional. FR-007a
requires the transitive form.

**Alternatives considered**: extending `any_hard` to propagate. Rejected
as the same computation written less legibly — once the rule is "reach
the runtime closure and subtract", the two-closure form is both what
`v9_importer_scopes` already does for the development axis and what the
reference managers describe themselves as doing.

---

## R5 — What is wrong in the yarn reader, and which half of it?

**Decision**: the Berry path has a wrong source (and a comment that
documents the wrongness as a fact); the v1 path has the transitive
edges already and is missing only the root declaration and the
cancellation.

**Rationale**: Berry's `optional_names` comes solely from
`berry_optional_names_from_pkg_json` (`yarn_lock.rs:133`). Measurement
shows Berry derives `dependenciesMeta.<name>.optional` into its own
lockfile and an ordinary project's manifest has no `dependenciesMeta`
key at all — so the reader's only source is the one real projects do
not populate. The comment at `yarn_lock.rs:127-129` asserts the
opposite ("Berry's lockfile does NOT carry the flag (it lives in
package.json only)"), which is the stated reason for the source choice,
and the `dev_scope_optional_meta_berry` fixture's header repeats the
claim while its own body writes `dependenciesMeta` into the lockfile.
FR-014 covers both texts, because a reader who believes either would
reintroduce the defect.

The v1 path is in better shape than expected: `optional_children_seen`
(`yarn_lock.rs:241`) already collects per-entry `optionalDependencies:`
sub-blocks, and `yarn_lock.rs:372` already subtracts
`regular_children_seen`. What it lacks is the root declaration — which
measurement shows a v1 lockfile cannot carry, because it has no root or
workspace entry — and a *transitive* rather than one-level subtraction.

**Alternatives considered**: replacing Berry's manifest source with the
lockfile outright. Rejected at clarification: hand-writing
`dependenciesMeta` is the documented way to mark a Berry dependency
optional, and the two sources diverge only when the lockfile is stale,
where the manifest is the more current statement of intent. FR-004a
takes the union.

---

## R6 — Can every dialect satisfy FR-011a's unrecognized-version diagnostic?

**Decision**: yes, all four carry a version marker. Measured values
below are the ones these findings were established against; a reader
seeing a higher one reads what it knows and warns.

| dialect | marker | measured |
|---|---|---|
| pnpm | `lockfileVersion` (already parsed, `pnpm_lock.rs:305`) | `'9.0'` |
| bun | `lockfileVersion` (plus `configVersion`) | `2` |
| yarn Berry | `__metadata.version` | `8` |
| yarn v1 | `# yarn lockfile v1` header comment | v1 |

**Rationale**: pnpm already reads and branches on its marker, so that
arm is a diagnostic rather than new parsing. The other three need the
marker read, which is cheap in each case.

**Alternatives considered**: a silent best-effort read. Rejected — it
is the exact failure being fixed. pnpm moved the flag between v7 and
v9 and the reader answered "not optional" where the only honest answer
was "unknown", and nothing in the output distinguished the two.

---

## R7 — Where does the closure belong, and is there a pattern to follow?

**Decision**: per-reader, local to each parse, following
`v9_importer_scopes`' existing shape — seed two sets from the
declarations, close the runtime set over the edge map, subtract.

**Rationale**: that function (`pnpm_lock.rs:200-266`) already computes
precisely this for the development axis: it seeds `protected` and
`development`, closes `protected` over the snapshot edges, then
`dev.retain(|k| !protected.contains(k))`. The optional axis is the same
computation against a different seed, and the readers already hold the
edge maps it needs.

One trap, recorded because it is the whole reason this feature is not a
two-line change: `v9_importer_scopes` puts `dependencies` **and**
`optionalDependencies` into the same `protected` set. For the
development axis that conflation is harmless — both are "not dev". For
the optional axis it is fatal, because the cancelling set would contain
the very packages being classified and you cannot subtract a set from
itself. Any reader that reuses this helper's seeds must split them
first; FR-010 requires the development result be unchanged while doing
so.

**Alternatives considered**: a shared closure helper across all four
readers in `npm/mod.rs`. Deferred rather than rejected — the three
edge maps have different key shapes (`name@version` for pnpm, key-path
for bun, bare name for yarn), so a shared helper would need a
normalisation layer that does not exist yet. Worth revisiting once
three call sites exist; building it before they do would be speculative.

---

## Open items carried into Phase 1

None. The spec's single `NEEDS CLARIFICATION`-class item was R1, and it
is measured. The three clarify-session answers are recorded in the
spec's Clarifications section and need no further research.
