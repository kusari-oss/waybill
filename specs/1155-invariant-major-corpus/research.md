# Phase 0 Research — Universal Semantic-Invariant Layer

Nine decisions. Each records what was chosen, why, and what was
rejected. Claims about existing code were checked against the tree at
`1155-invariant-major-corpus`; file and line references are from that
reading, not recollection.

---

## R1 — Cell outcome shape

**Decision**: The invariant layer gets its own outcome type returning
every cell's state. The existing per-target hook keeps its
`Result<(), AssertionFailure>` signature and is only renamed.

**Rationale**: FR-004 requires every failing cell be reported, not the
first. `harness.rs:35`'s `AssertionFailure` is a single failure and
`Result` short-circuits, so the existing shape cannot express a matrix —
one target's first failure would hide the rest, and the whole value of
the feature is seeing all cells at once. Keeping the per-target
signature untouched means US3's migration moves an assertion *between*
two mechanisms rather than rewriting both at once, and leaves
`layer0_document_integrity` working as-is.

**Alternatives rejected**:
- *Change `CorpusTarget.layer1` to return `Vec<AssertionFailure>`* — forces all 17 per-target functions to be rewritten before any invariant can land, coupling US3 to US1 and making the first increment large.
- *Keep `Result` and run each invariant as its own pseudo-target* — reinstates the target-major shape in a new costume; the cell grid would still not exist as a value.

---

## R2 — Declarations file format

**Decision**: TOML, an array of tables sorted by `(invariant, target)`,
one table per non-passing cell, at
`waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`.

**Rationale**: m115/m117's `walk.audit-allowlist.txt` is the precedent
for *what* is committed — non-passing entries only, passing implicit —
and spec Assumption 2 adopts that. It is not a usable precedent for
*how*: FR-010 requires a reason, a pinned violation kind, a count and an
optional issue per entry, which a positional `<file>:<content>` line
cannot carry without inventing a delimiter scheme. `toml = "0.8"` is
already a direct dependency. Sorting by `(invariant, target)` gives
FR-014c's line stability: adding a declaration inserts at a determined
position and moves nothing else, and adding a corpus target touches
only its own entries. Placement beside the harness mirrors the walker
allowlist sitting beside its subject.

**Alternatives rejected**:
- *Plain text, one line per cell* — cannot hold the fields; would need a bespoke delimiter and a bespoke parser.
- *JSON* — no comments, and a reason field benefits from being readable and annotatable in review.
- *Declarations as Rust attributes on the registry* — moves the gate into code, so a declaration becomes a code change requiring recompilation, and the diff that should show "this fix cleared four cells" becomes a diff of Rust.

---

## R3 — Precondition representation

**Decision**: The precondition is evaluated inside the checker, which
returns `NotApplicable { reason }` as one of its outcome variants. No
separate precondition function.

**Rationale**: Deciding applicability requires inspecting the same
document the check inspects — whether any file-tier component exists,
whether any component carries a non-runtime marker. A separate
precondition function would walk the document twice and could drift out
of agreement with the checker, producing the worst case: a cell that
reports *pass* because the precondition function said applicable while
the checker found nothing to look at. One function, one walk, one
answer. FR-006a's applicability count falls out of counting cells whose
outcome is not `NotApplicable`.

**Alternatives rejected**:
- *Separate `precondition: fn(&EmittedSboms) -> bool`* — double walk plus drift risk, for symmetry that buys nothing.
- *Treat unmet preconditions as pass* — explicitly forbidden by FR-006 and SC-009; it is the vacuity this feature exists to prevent.

---

## R4 — Violation kind representation

**Decision**: Each invariant declares its kinds as `&'static str`
constants. A failing checker returns the set of kinds observed plus a
count per kind.

**Rationale**: Q4 settled that declarations pin the kind and let the
count drift. That needs a kind that is stable against upstream churn
and coarse enough to name, but fine enough that a second, different
defect does not hide under an existing declaration. Static strings keep
the kind greppable, make it printable in the report without a mapping
table, and let the catalog doc list each invariant's kinds for review.

**The I2 problem, flagged by the spec checklist**: duplicate identities
need kinds that distinguish *what kind of thing* is duplicated, or every
duplication collapses into one kind and the declaration absorbs
everything. Proposed kinds for I2: `duplicate-file-identity`,
`duplicate-library-identity`, `duplicate-application-identity` — keyed
on the component type, since the measured instances differ exactly
there (`util.sh` ×9 is file-tier; a duplicated library would be a
different defect with a different cause). Each initial invariant's kinds
must be reviewed against FR-012a during Phase 2, and this is the one
with the least obvious answer.

**Alternatives rejected**:
- *An enum per invariant* — more type safety, but the registry would need a type-erased kind to print and compare, reintroducing strings at the boundary.
- *Kind derived from the first offending component's name* — unstable by construction; an upstream rename would redden the lane, which is the failure mode Q4 rejected.

---

## R5 — Catalog location and gate

**Decision**: `docs/reference/sbom-invariants.md`, a markdown table
parsed at test time, gated **bidirectionally** against the registry:
rows without checkers fail, and checkers without rows fail.

**Rationale**: Mirrors the C-row catalog this feature is modelled on.
`every_catalog_row_has_an_extractor`
(`parity/extractors/mod.rs:922`) does exactly this, and reading it
showed the bidirectional half is the important part — it was already
asserting both `missing` (rows without extractors) and `orphans`
(extractors without rows). FR-008 as written only requires the first
direction; the second is adopted anyway, because a checker with no
catalog row is an invariant nobody can discover or review. Siblings in
`docs/reference/` means the two gates are found together, per spec
Assumption 5.

**Alternatives rejected**:
- *Catalog as a Rust const array only* — no reviewable prose home for the statement, the precondition and the kinds; the C-row precedent deliberately keeps the human-readable catalog in docs.
- *One direction only* — permits an orphan checker to exist unreviewed, which the existing parity gate already declines to allow.

---

## R6 — Terminology rename mechanics

**Decision**: Rename the shared field `AssertionFailure.invariant_name`
→ `check_name`; rename the 17 per-target functions `*_layer1` →
`*_expectations`; rename `CorpusTarget.layer1` → `expectations`. The
invariant layer uses the word "invariant" for its rows.

**Rationale**: Q5 reserved "invariant" for universal properties. The
subtlety found while reading the code is that `AssertionFailure` is
shared by `layer0_document_integrity` (where "invariant" is *correct*)
and the 17 per-target functions (where it is a misnomer). Renaming the
field to the neutral `check_name` makes it accurate for both without
inventing a second near-identical failure type, and the concepts become
distinguishable by the names that actually matter — the function and
field names a reviewer reads when answering US3's sorting question.

**Alternatives rejected**:
- *Introduce `ExpectationFailure` alongside `AssertionFailure`* — two structurally identical types whose only difference is which layer constructs them; the report would have to unify them anyway.
- *Leave the field name and rely on prose* — Q5 option C, rejected; FR-020a requires the distinction be legible from names alone.

---

## R7 — Report artifact location and format

**Decision**: Console table for the run, plus a JSON artifact at
`target/invariants/run-<waybill-sha>.json` holding every cell's state,
kinds, counts, the declaration status, applicability counts and the
ecosystem coverage statement. Uncommitted.

**Rationale**: Q1 chose an uncommitted per-run artifact. Three existing
precedents put exactly this under `target/<tool>/run-<key>.json` —
`target/bench/run-<git-sha>.json` (m669), `target/quality/run-<waybill-sha>.json`
(m770), `target/compare/run-<timestamp>.json` (m780) — so keying on the
waybill SHA follows the closest two. JSON because SC-003 wants two runs
compared to establish a fix's reach, and that is a diff over structured
data, not over a rendered table.

**Alternatives rejected**:
- *Commit the artifact* — Q1 option A; rewrites N cells on every target addition, which is the churn m117 had to undo for the walker allowlist.
- *Console only* — Q1 option D; nothing to diff, so SC-003 is unmeetable.

---

## R8 — Ecosystem coverage source

**Decision**: Add an `reader_ecosystems: &[EcosystemId]` field to
`CorpusTarget` declaring what each target exercises, and derive the
universe of ecosystems by parsing the coverage matrix in
`docs/ecosystems.md`. FR-015's uncovered list is the set difference.

**Revised after analysis**: the field was first drafted as
`ecosystems: &[&str]`, which missed two things. `CorpusTarget` already
carries `ecosystem: Ecosystem` (`manifest.rs:15`) with its own
`cross_ecosystem_coverage_check` (`manifest.rs:606`) — a different axis
(coarse target classifier, `PolyglotImage` among its variants), so the
new field is additive rather than a widening, and is named
`reader_ecosystems`: `exercises` was the first choice and is already
taken by a free-text field on the same struct (`manifest.rs:16`). And raw strings fail in the one
direction that matters: a typo makes a covered ecosystem read as
*uncovered*, a false claim of exactly the kind this feature exists to
prevent. `EcosystemId` is an enum with one variant per matrix row, gated
bidirectionally against the doc, so a doc rename reddens the lane
instead of quietly shrinking the covered set.

**Rationale**: There is no canonical ecosystem enumeration in the code
— the only candidates found are `EcosystemProbe`
(`resolve/resolvers/cache_probe/mod.rs:172`, a six-cache subset) and
`EcosystemSupport` (`report/schema.rs:55`, a two-variant
supported/no-reader flag). `docs/ecosystems.md` already carries a
coverage-matrix table with ~80 rows and is maintained as the
user-facing answer to "does waybill see my packages", so parsing it
reuses R5's doc-parsing pattern and avoids a second list to keep in
sync.

**Known risk, and it is real**: that doc's own prose opens with "all
nine ecosystems Waybill supports" while roughly forty reader modules
exist under `scan_fs/package_db/`. Parsing the table inherits the
table's accuracy, and the stale sentence is direct evidence that
hand-maintained inventories rot. Mitigations: parse the table rows
rather than the prose, and report the derived count so a wrong universe
is visible rather than silent. **The stale sentence should be filed
separately** — it is a docs defect this research found, not something to
fix inside this feature.

**Alternatives rejected**:
- *A new hand-maintained const list in the harness* — a third inventory to rot, with no gate able to tell when it has.
- *Derive from `package_db/` module names at build time* — would need a build script to enumerate the directory; module names are not ecosystem names (`ipk_file`, `rpmdb_bdb`, `pants_jvm`), so the mapping would be hand-maintained regardless.

---

## R9 — Invariant numbering

**Decision**: `I<n>`, allocated append-only. Retired numbers stay listed
in the catalog with their retirement noted and are never reused.

**Rationale**: Q2 settled the policy; this records the surface form.
`I<n>` parallels the C-row catalog's `C<n>` so the two read as siblings.
Keeping retired rows visible in the catalog is what makes
non-reuse enforceable — the gate can refuse a registry entry claiming a
retired number only if the catalog still remembers it. The repo carries
a specific scar here: `feedback_renumber_by_anchor_not_arithmetic`
records three distinct ways ID references break on insertion, and
append-only allocation forbids the renumbering that caused it.

**Alternatives rejected**:
- *Slug-only identity* (Q2 option D) — more readable in declarations, but diverges from the C-row sibling and makes a meaning change silently invisible, since a slug can be kept while the statement beneath it changes.
- *Renumber freely, key declarations on name strings* (Q2 option C) — rejected by the user and by the recorded scar.

---

## Carried into Phase 2 as open work, not unknowns

- **FR-019's restatement of I3 and I7.** The measured overlap — the gem and Go instances of the propagation class landing under I7 rather than I3 — must be resolved before the catalog lands, because a defect in the wrong row sends its fix to the wrong place. The probe's I3 seeds only from already-marked components, which is why it misses an unanchored population that was never marked at all; the restatement likely splits on *whether a non-runtime marker exists anywhere upstream* rather than on reachability alone.
- **Kind review per invariant against FR-012a** (see R4). I2 is the known-hard case.
- **Eight of the 17 targets have no measured cell state.** The 12 known failures come from nine Pico nightly pairs, not the public corpus. Phase 2 should expect discovery, and the declarations file's initial contents cannot be written from the spec — they must come from a first real run.
