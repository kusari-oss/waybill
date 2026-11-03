# Phase 0 research: closure members that are known Hackage packages

**Milestone**: 1275 | **Spec**: [spec.md](./spec.md) | **Date**: 2026-11-03

Every claim below is measured at `001fa50`, aarch64-darwin, nixpkgs
`a799d3e3886da994fa307f817a6bc705ae538eeb`, on a flake depending on `aeson`
and `text` (1,290 derivations → 341 components). The sizing probe is committed
at `measurements/size-closure-members.py`.

---

## R1 — the obvious design is wrong, and a previous milestone already measured why

The root cause is **ordering**, not a missing capability:

| line in `cli/scan_cmd.rs` | what happens |
|---|---|
| 4503 | the closure is queried and classified |
| 4540 | `haskell_packages::enrich` runs — versions, licenses, hashes, edges |
| 4729 | `merge_closure_components` adds the closure's components |

Closure members arrive **189 lines after** the pass that would have resolved
them. So the obvious fix is to move the merge earlier.

**That has been tried, and it regressed.** The merge site carries milestone
1035's comment (#1034, #1040):

> Deliberately after the nixpkgs-haskell enrichment rather than beside the
> closure query above. That enrichment assigns versions and rewrites
> dependency-edge endpoints, and running before it meant the closure's
> components were present, versionless-adjacent and name-matchable while it
> worked. Measured: 41 components that a flag-off scan emitted as
> `pkg:hackage/QuickCheck@2.15.0.1` came out of a flag-on scan as
> `pkg:generic/QuickCheck@2.15.0.1` — enabling the closure *downgraded*
> precise PURLs to the generic fallback.

Enabling `--nix-closure` made 41 PURLs **worse**. Reordering would reintroduce
exactly the defect this feature exists to remove.

**Decision: a new pass, after the merge.** It reads the package set, which is
already cached on disk per revision, and reclassifies in place. No existing
code moves.

**Alternatives considered:**

- *Move the merge before `enrich`.* Rejected on m1035's measurement above.
- *Reclassify inside `closure/emit::components`.* Rejected: the emit function
  takes only a `ClassifiedClosure` and has no revision, so it cannot know
  which package set to consult. Threading one in would make the closure tier
  depend on the Haskell tier, for a reason that is specific to one ecosystem.
- *A second `enrich` call after the merge.* Rejected on R2.

## R2 — `enrich` would skip them even if the ordering allowed it

```rust
fn is_enrichable(c: &ResolvedComponent) -> bool {
    is_haskell(c) && c.version.is_empty()
}
```

The enrichment resolves **versionless** components. Closure members always
carry the version nix recorded, so they are not enrichable by construction —
a second reason reordering cannot work, and the reason a second `enrich` call
would be a no-op rather than a fix.

`enrich` also returns `None` early when no component is enrichable and when
`flake.lock` is absent. On a repository whose only Haskell content comes from
the closure, the pass therefore never runs at all. The new pass must **not**
inherit that gate, or the scans most affected by #1282 are the ones it skips.

## R3 — every field the feature needs is already parsed

`PackageSetEntry` holds, per attribute:

| field | since | what the feature uses it for |
|---|---|---|
| `version` | m926 | the exact-pair test (FR-001) |
| `source_hash` | m926 | FR-003 |
| `license: Option<DeclaredLicense>` | m1273 | FR-002 |
| `runtime_relations: Vec<String>` | m985 | **FR-012** |

`runtime_relations` is `libraryHaskellDepends + executableHaskellDepends`,
merged and deduplicated. So FR-012's edges are not new parsing: they are the
field m985 added, never consulted for closure members.

`EnrichmentSummary` already carries `closure_edges: Vec<ClosureEdge>` with the
note that "the caller converts them to relationships". **The mechanism that
gives the 65 resolved components their 245 out-edges is the mechanism FR-012
needs**, fed by a different set of components.

**Decision**: no new parsing, no new fetch, no new cache. FR-010 is satisfied
by construction.

## R4 — the lookup shape, measured

`PackageSet` is keyed by **attribute name**, not by `pname`, and nixpkgs
encodes multiple versions as distinct attributes (`hashable`,
`hashable_1_4_7_0`). The closure gives a `pname`. So "is this in the set" has
two possible readings, and they differ:

| rule | closure members matched (of 276) |
|---|---|
| the **default attribute**'s version equals the closure's — a single `get(name)` | **59** |
| only a versioned alias matches — needs a reverse `pname → attributes` index | **1** (`hashable 1.4.7.0` → `hashable_1_4_7_0`) |
| no attribute at that version | 202 |

**Decision: the default attribute only, via a single `get(name)`.** The
reverse index buys exactly **one** component here and introduces a new
ambiguity — when two aliases carry the same version, something has to choose.
Recorded as a measured limitation rather than an oversight, with the cost
stated: 1 of 60.

## R5 — the corroborating signal FR-005 requires

FR-005 forbids resting identity on two strings agreeing. The independent
signal is **whether the derivation was built by a GHC**: its
`env.nativeBuildInputs` references a GHC store path.

| bucket | n | built by GHC |
|---|---|---|
| exact pair | 60 | **60** |
| name in set, version differs | 26 | **0** |
| name absent from set | 190 | 1 |

The two signals agree on 60 of 60, and the GHC signal rejects all 26
collisions independently. Reachable with no new parsing: `Derivation` keeps
`env: BTreeMap<String, String>` whole, and there is already a helper for store
paths in a whitespace-separated env field.

**A probe lie worth recording.** The first pattern, `-ghc-\d`, reported
`happy-lib 2.1.7` as not-GHC-built and the two signals as agreeing 59 of 60.
`happy-lib` bootstraps with `ghc-binary-9.0.2`, so the pattern — not the
system — was wrong. One row disagreeing between two independent signals was
the tell; the correct pattern is `-ghc(-binary)?-\d`. A signal that disagrees
on one row is a finding to explain, which is why FR-006 requires the
agreement be measured rather than asserted.

## R6 — FR-012's edges, and what the dangling ones turn out to be

From the 59 reclassifiable members, the package set declares **278** runtime
dependencies:

| | n | |
|---|---|---|
| target present in the document by name | **265** | 95% |
| target absent | **13** | `directory` ×11, `ghc-heap` ×1, `pretty` ×1 |

Resolving an edge by **name** against the document, not by the set's version,
is load-bearing: 165 of the 278 targets are present at a version that differs
from the set's, because the closure carries what GHC actually shipped. Binding
edges to the set's version would point 59% of them at components that do not
exist.

**All three dangling targets are GHC boot libraries** — packages the compiler
ships, so they are never separate closure derivations. This is the population
of #1340 and #966, and `haskell_packages/boot_libraries.rs` (m926, #947)
already identifies them from nulls in the revision's configuration.

**Decision for FR-012c: emit the target as a versionless compiler-supplied
component**, reusing m926's boot-library handling and m1273's reason for it.
Neither a dangling edge nor a dropped one. The precedent is already in the
design tier, where a boot library is emitted with no version and a reason
saying the compiler supplies it — milestone 1273's US4 test asserts exactly
that shape.

**Alternatives considered:**

- *Drop the edge.* Rejected: the dependency is real and declared. Dropping it
  makes the graph quietly incomplete, and a count that silently omits 13 edges
  is the shape #1218 was about.
- *Emit a dangling reference.* Rejected: FR-012c forbids it, and #980 measured
  what happens — CycloneDX keeps an edge pointing at nothing, SPDX drops the
  relationship, and neither format errors.

## R7 — FR-008 is free

`merge_closure_components` keys on `(name, version)`, not on PURL. So a
closure member and an identically-named design-tier component already merge —
that is the measured 47 — and the merged component keeps the existing
component's PURL. Reclassification does not change `(name, version)`, so it
cannot create a duplicate the merge would miss.

**Decision**: FR-008 needs a test, not an implementation. A test is still
required, because "satisfied by construction" is a claim about code that can
change.

## R8 — changing a PURL must move the edges with it, in the same step

The rewrite site immediately above the merge carries #980's lesson:

> normalise identities BEFORE anything reads them. Version assignment rewrites
> component PURLs, and the PURL is what `Relationship::from`/`::to` hold. […]
> otherwise every component this pass resolved is orphaned: CycloneDX keeps an
> edge pointing at the old PURL, SPDX drops the relationship outright, and
> neither format errors.

Reclassification rewrites a PURL from `pkg:generic/x@v` to `pkg:hackage/x@v`.
Any relationship already referring to the old PURL is orphaned unless rewritten
in the same step.

**Decision**: the new pass rewrites endpoints as part of reclassification,
reusing `apply_renames`. Not a follow-up step — #980's failure mode is silent
in all three formats, so an ordering mistake here produces a document that
validates and is wrong.

## R9 — no new surface

No new Cargo dependency, no network, no subprocess, no CLI flag. The package
set is already cached per revision on disk; the closure JSON is already
parsed; `env` is already retained. The feature is a pass that reads two
things already in memory and writes identity, license, hash and edges.
