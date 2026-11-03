# Phase 0 research: a package the compiler ships is never scoped as absent

**Milestone**: 1276 | **Spec**: [spec.md](./spec.md) | **Date**: 2026-11-03

Every claim is measured on this branch, against the two nixpkgs revisions in
the local per-revision cache (`a799d3e3…`, which has all eight compiler
configurations, and `5ae3b07d…`, `slack-web`'s pin) and the 17 committed
corpus goldens.

---

## R1 — the invariant that demands the marker is the one we are about to break

**This is the finding that changes the work.** The fix is not "exempt the
class from propagation and add an invariant". The project already has an
invariant asserting the opposite, and it is clean today.

`waybill-cli/tests/corpus_harness_195/invariants/checks/scope_propagation.rs`
is **I3**:

> "A component reachable from the subject only through non-runtime edges
> **carries** a non-runtime marker"

A compiler-supplied package reachable only through non-runtime edges satisfies
I3's precondition exactly. Today it is marked, so I3 passes. Remove the marker
and I3 fails.

Measured, by replaying I3's reachability over the goldens with the exempt
class unmarked:

| target | mis-scoped boot libraries | would become I3 violations |
|---|---|---|
| `haskell-security-advisories` | 8 | **8** — `array`, `binary`, `deepseq`, `exceptions`, `ghc-prim`, `os-string`, `stm`, `unix` |
| `nix-closure-moat` | 2 | **2** — `array`, `directory` |

**All 10.** And `invariant-declarations.toml` holds **zero** I3 rows, so I3
passes cleanly on every target today and all ten would be *undeclared*
violations — which `Verdict::UndeclaredViolationKind` and the harness test
`an_undeclared_failure_fails_the_lane` make a hard lane failure, not a
reported cell.

**Decision: amending I3 is part of this feature, not a consequence of it.**
I3's statement, its precondition and its checker must exempt the same class
the propagation does, from the same source of truth, or the two rows disagree
about one document. They land in the same change.

I3's own module comment already shows the care these partitions need: it
documents its boundary with I7 and records that "two instances of one defect
class landed under the wrong number" when the boundary was loose. The same
discipline applies to carving an exemption out of it.

**Alternatives considered:**

- *Declare the 10 in `invariant-declarations.toml`.* Rejected: that file
  records known defects awaiting a fix, and these would be components the
  feature deliberately made correct. Declaring them would assert the opposite
  of the feature's premise.
- *Leave I3 alone and keep the marker, correcting only at emission.* Rejected:
  it is the marker that is wrong, and I3 is what demands it. Correcting
  downstream of the invariant leaves the document and the invariant disagreeing.

## R2 — FR-004a is already satisfied by construction

The Q1 clarification chose "narrow to the series the flake names; union only
as the fallback". `candidate_series` already does exactly that:

```rust
let Ok(text) = std::fs::read_to_string(scan_root.join("flake.nix")) else {
    return KNOWN_GHC_SERIES…           // no flake.nix → all eight
};
for series in KNOWN_GHC_SERIES {
    let attr = format!("ghc{}", series.trim_end_matches(".x").replace('.', ""));
    if text.contains(&attr) { found.insert(series); }
}
if found.is_empty() { KNOWN_GHC_SERIES… } else { found }   // named → only those
```

Corroborated by the cache rather than by reading alone: `slack-web`'s
per-revision directory holds **only** `configuration-ghc-9.10.x.nix`, so its
flake named a series and no union was ever formed.

**Decision: FR-004a needs a test, not an implementation.** The same posture
milestone 1275 took for its FR-008 — "satisfied by construction" is a claim
about code that can change, so it is asserted rather than assumed. The test
is worth more than usual here because the measured cost of the fallback is
known: at `a799d3e3` the union is 50 names against 32 nulled in every series,
so the union exempts up to **15** a given series does not.

## R3 — FR-006 needs no new property

FR-006 requires each exempted component to record that its runtime presence is
owed to the compiler. **C170 `waybill:haskell-version-unresolved-reason =
compiler-supplied` already records exactly that**, and it is present on all 10
affected components — it is how they were identified in every measurement in
this document.

The coincidence is not accidental: both C170 and the exempt class derive from
the same nulled-name set, through `classify()` returning `CompilerSupplied`.

**Decision: reuse C170; no new `waybill:*` property for FR-006, so FR-006a's
conditional citation requirement is discharged by this finding.** The plan
must still *verify* the coincidence rather than assume it — a component in the
boot set that does not carry C170 would be exempt without recording why, and
that is a measurement, not a deduction (C5).

## R4 — FR-007 does need a new document-scope row, and here is its audit

`PropagationSummary` already carries what FR-007 wants — immunity-set size,
assignments per scope, seeds, and a `skipped_unrooted` count added precisely
because "the feature is a no-op here" and "the feature declined here,
deliberately" looked identical from outside. **But it is logged, not
emitted**, and no document-scope row exists for the pass: the only
scope-related rows are C42 (per-component `waybill:lifecycle-scope`) and C62
(per-component `waybill:lifecycle-scope-derivation`).

FR-007's purpose is an operator comparing two **documents** across waybill
versions. A log line cannot serve that, and this project has the scar:
C174 exists because "the summary has always carried every field a consumer
wants, but only `degraded_reason` ever reached the document", and
`feedback_a_log_line_is_not_the_claim` records a derived field asserting
`complete` while the log said otherwise.

**Principle V audit**, against the vendored schemas at
`waybill-cli/tests/fixtures/schemas/`:

- *CycloneDX 1.6* — `compositions[].aggregate` describes the completeness of
  the component list, not a pass's exemption count. No native document-level
  field carries "how many components a scope rule exempted, and why".
  `metadata.properties[]` is the generic bag.
- *SPDX 2.3* — document-level `annotations[]` only; no native member.
- *SPDX 3.0.1* — a document-subject `Annotation`; no native member.

**Decision: one new document-scope row, KEEP-NO-NATIVE, parity-bridge** — the
same shape and rationale as C184, C188, C146, C104 and C118, all doc-scope
resolver-tier records. **This requires a spec amendment**: the constitution
requires the audit result cited in the spec's Functional Requirements, so
FR-007 gains a citation before implementation. Milestone 1275 recorded what
skipping that costs — its plan asserted a reuse that the catalogue forbade,
and the correction came after three commits.

## R5 — the boot set is computed and then discarded

`enrich` destructures `BootSet { boot, aliases, candidates }` at
`mod.rs:941`, uses `boot` for classification, and drops it.
`EnrichmentSummary` does not carry it.

The propagation's call site (`scan_cmd.rs:4851`) needs it, and **the defect
occurs without `--nix-closure`** — `slack-web` and
`haskell-security-advisories` are both `ScanMode::Offline`. So milestone
1275's boot-set load, which lives inside the `--nix-closure` arm, is not
reachable from here and not a precedent to reuse directly.

**Decision: add the nulled set to `EnrichmentSummary`**, exactly as milestone
1275 added `package_set` for the same reason and with the same measurement
behind it: re-deriving it would mean re-reading and re-parsing the compiler
configurations, and the value is already in hand. One field, one assignment.

## R6 — FR-002a's invariant is I3's complement, and the pair must partition

With I3 amended to exempt the class, FR-002a's new invariant asserts the other
half: no compiler-supplied component carries a non-runtime scope, from any
origin. Together they partition the class cleanly, which is the same
discipline I3 already documents for its boundary with I7.

Measured, so the new invariant's population is known before it is written:
all **10** affected components carry
`waybill:lifecycle-scope-derivation = transitive`, so the direct-mark
population is **0**. The invariant is therefore expected to be green on every
target from the moment the fix lands, and SC-007a requires it be demonstrated
to fail — an invariant never seen to fail is not evidence.

## R7 — milestone 1275's `t057` asserts this defect

`waybill-cli/tests/nix_closure_hackage_identity.rs::t057` asserts that
`waybill-fixture-bootlib` **is** `excluded`, having been added by milestone
1275 to pin the behaviour and name this issue. FR-010 requires it be updated
rather than deleted: the `optional_scope` fixture it drives is the only
deterministic reproduction of a boot library inheriting a scope through
closure-derived edges, and that path exists because 1275's declared edges feed
the propagation (that milestone's findings F12).

**Decision: `t057` is amended in this feature, and its assertions invert.**
Its premise assertions — that the fixture's member really is non-runtime and
really has the declared edge — stay, because they are what keeps the test from
passing vacuously after the inversion.

## R8 — no new surface

No new Cargo dependency, no network, no subprocess, no CLI flag. The compiler
configurations are already retrieved and cached per revision; the nulled set
is already parsed; the propagation already maintains an immunity concept. The
feature adds one field to a struct, one condition to an immune set, one
amendment to an invariant, one new invariant and one document-scope row.
