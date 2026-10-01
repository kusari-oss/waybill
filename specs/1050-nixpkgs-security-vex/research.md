# Phase 0 research — nixpkgs security declarations as VEX

Measured 2026-09-30 against `MercuryTechnologies/moat`, aarch64-darwin,
using the project's own pinned nixpkgs rather than a registry one. Probes in
`measurements/`.

## R1 — How is a closure member's declaration reached?

**Decision**: resolve a candidate attribute by `pname` across an *ordered list
of package sets*, first hit wins, then verify by output path.

```
top-level  →  haskellPackages  →  python3Packages  →  perlPackages
```

**Rationale**: `meta` is not in the derivation — measured, zero of moat's
1,275 carry it or any `meta` key — so it must come from a second evaluation
against the pinned package set. `pname` is the only handle a closure member
offers.

Top-level alone is not enough. Measured on moat's 380 distinct members:

| probed sets | confirmed | unreachable | rejected by the path check |
|---|---:|---:|---:|
| top-level only | 86 (22%) | 270 (71%) | 24 (6%) |
| **+ haskell, python3, perl** | **273 (71%)** | 72 (18%) | 35 (9%) |

Contribution by set: haskell 118, python3 69, top-level 86, perl 0. The
nested sets are not an optimisation — they are most of the coverage, and on a
Haskell project the largest single contributor.

**Alternatives considered**: evaluating all of nixpkgs and indexing by output
path would reach 100% but costs a full package-set evaluation on every scan.
At 71% for ~1 second (R2) that trade is not worth making, and the unreachable
18% is *reported* rather than silently dropped (FR-001c), so the gap is
visible rather than misleading.

## R2 — What does the second evaluation cost?

**Decision**: the cost does not threaten FR-020a, so the feature stays
automatic under `--nix-closure`.

**Measured**: 376 distinct names across four package sets evaluated in
**0.6–1.1 s** wall clock. SC-006a set roughly a fifth of closure-scan time as
the threshold that reopens the automatic-by-default decision; a full
`--nix-closure` scan of moat measures in the tens of seconds, so this is an
order of magnitude inside it.

One evaluation for the whole closure, not one per member — the probe builds a
single Nix expression listing every name and evaluates it once. A
per-member invocation would be 376 process spawns and is not a serious
option.

## R3 — The output-path check is load-bearing, not a formality

**Measured**: 35 of 380 members (9%) resolve to an attribute whose output path
does **not** match. Without the check each of those is a security claim
attached to the wrong component.

That is the failure this feature must not have. A false `affected` sends
someone to patch something that was never in the build, and once a consumer
finds one they have no reason to trust any other statement in the document.
Spec FR-001b exists for these 35.

## R4 — A store-path format trap that reads as total failure

**Finding**: the closure JSON stores output paths **without** the
`/nix/store/` prefix (`xfish2…-patchutils-0.4.2`); evaluation returns them
**with** it (`/nix/store/7qw8…-alex-3.5.4.2`). Comparing the two raw forms
yields **zero** matches, which looks exactly like "the mechanism does not
work" rather than "the strings are formatted differently".

This cost three measurement rounds before it was spotted. Milestone 1035
already has the helper — `closure::derivation::store_basename`, whose doc
comment says the basename "is the only form in which two sides of the closure
can be compared". Implementation MUST route both sides through it.

## R5 — A building project has no un-permitted insecure packages

**Finding**: moat's closure contains **zero** declarations, and this is
structural rather than luck. Nix refuses to *evaluate* a package marked
insecure, so a project that builds has either no insecure packages or has
already permitted the ones it has.

Two consequences:

1. **The feature's yield is inherently low and high-signal.** It fires only
   where an operator accepted a policy exception. That is the right shape —
   it is not a scanner — but it means "produced no statements" is the common
   case and must not read as failure.
2. **Testing needs a fixture that permits one.** No public project scanned so
   far exercises the path. A fixture pinning a package with a
   `knownVulnerabilities` entry plus the permission to build it is required,
   and its package names must be synthetic per the repository rule.

## R6 — Native carrier audit (Constitution Principle V)

| Datum | Native carrier? | Decision |
|---|---|---|
| CVE-bearing declaration | **Yes** — OpenVEX `statement` with `status: affected` | Use it. No `waybill:` property. |
| Statement subject | **Yes** — OpenVEX `product` + `subcomponents` (already extended in m1035) | Use it. |
| Asserting party | **Partly** — OpenVEX has a document-level `author`, but not a per-statement source | The document author is waybill, not nixpkgs, so per-statement attribution needs the existing evidence-grade channel. |
| Evidence grade | **No** — no format models the evidentiary strength of a vulnerability association | Extend the existing `EvidenceGrade` enum; carried in `impact_statement` as m1035 does. |
| Prose declaration | **No** — CycloneDX `pedigree.notes` is about lineage, not bundled content; SPDX has no equivalent | New per-component annotation. Parity-bridge row. |
| Acceptance record | **No** — no format has "this build accepted a policy exception" | New document-scope annotation. Parity-bridge row. |
| Unchecked-member count | **No** — closest is CycloneDX `compositions.aggregate`, which describes completeness of the *component list*, not of an enrichment pass | New document-scope annotation, sibling to milestone 1035's closure record. |

## R7 — Where the evidence grade lives

**Decision**: extend `EvidenceGrade` in
`waybill-cli/src/scan_fs/package_db/nix/closure/patches.rs` with a second
variant rather than introducing a parallel type.

**Rationale**: milestone 1035 built it as a single-variant enum with a doc
comment saying exactly this — "a future stronger provenance must be
distinguishable rather than indistinguishable from a filename match". This is
that variant. A `GradedCve` already refuses to exist without a grade, so the
new source inherits that enforcement by construction.

The wire value must be distinct from `filename-derived` and must name the
source, not the confidence: the grade says *how we know*, and a consumer
weighing "the maintainer said so" against "a filename said so" needs the
provenance, not a number that implies a precision nobody measured.


## R8 — what the resolution cannot reach, found during implementation

**Finding**: declarations are read from **plain nixpkgs at the pinned
revision**. Two classes of closure member therefore never resolve:

- **Packages defined in the project's own flake.** They are not in nixpkgs,
  so there is no maintainer statement to read. Correct: inventing one would
  be fabrication.
- **Packages the project overlays or overrides.** The override changes the
  output path, so the verification rejects the candidate and records the
  member unchecked. Safe — no claim lands on the wrong build — but it means
  coverage is bounded by how much a project customises its package set, and
  part of the measured 8% path-mismatch is overlays rather than name
  collisions.

**Why not evaluate the project's actual package set instead?** It needs
`getFlake` on a path, which needs `--impure`, which milestone 1034's argv
guard refuses. That guard is right: `--impure` restores access to the host
environment, and the loss would not be visible in any emitted document. The
narrower reach is the price of keeping the evaluation pure, and it is paid in
coverage rather than in correctness.

**Consequence for testing**: no committed fixture can exercise the
declaration path, because a fixture's packages are by definition its own.
Statement and annotation shapes are unit-tested over hand-built summaries —
milestone 1035's pattern for its backport statements — and coverage is
checked against real closures through the env-gated cross-check.
