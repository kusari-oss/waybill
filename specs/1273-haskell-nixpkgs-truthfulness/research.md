# Phase 0 research: Haskell/nixpkgs resolution correctness

**Milestone**: 1273 | **Spec**: [spec.md](./spec.md) | **Date**: 2026-11-02

Every external behaviour this milestone depends on was measured. The probes
are committed under `measurements/` and their output is recorded in
[`measurements/findings.md`](./measurements/findings.md). This file records
the decisions those measurements produced.

Nothing below is marked NEEDS CLARIFICATION. The spec entered Phase 0 with
zero markers after `/speckit.clarify`, and Phase 0's two measurement
obligations (FR-001a's hpack semantics, SC-003's inherited figure) are both
discharged.

---

## R1 — hpack semantics for the FR-001a subset

**Measured**: hpack 0.38.3, `measurements/hpack-semantics.sh`, 12 cases.
Findings Probe A.

**Decision**: lower a `package.yaml` into the **existing `CabalManifest` /
`CabalStanza` representation** and let every downstream pass — scope
assignment, flag-default evaluation, the optional mark, the C151 reason —
run unchanged.

**Rationale**: the measurement makes this a correctness requirement, not a
convenience. hpack does not evaluate `when:` conditions; it lowers them to
Cabal `if` blocks, and it does so even for `flag(x)` where `x`'s default is
declared in the same file. So a `when:` block *is* a Cabal conditional, and
waybill already evaluates those against flag defaults in
`stanza_default_buildable` (milestone 955 / PR #1030). A second evaluator
would let one project get two answers depending on which file declared its
dependencies — the defect class this milestone exists to remove.

The three rules the lowering must implement, all measured:

| rule | measured behaviour |
|---|---|
| inheritance | the top-level list reaches `library`, `executables`, `tests` **and** `benchmarks` |
| merge | per-component is additive; for a repeated name the **last** declaration wins and replaces the earlier constraint entirely |
| `when:` | a literal `false` is pruned; everything else becomes a conditional stanza |

**Alternatives considered**:

- *A standalone hpack model with its own dependency resolution.* Rejected by
  R1's measurement: it would duplicate the conditional evaluator, and the
  duplicate is exactly what produces divergent answers.
- *Intersecting constraints on a repeated name.* Rejected by measurement —
  `text >= 1.0` plus `text < 3` renders `text <3`, not the intersection.
  Intersecting would publish a range the project does not declare.
- *Reproducing hpack's constraint normalisation* (`>= 4 && < 5` → `==4.*`).
  Rejected: it is cosmetic, and the `.cabal` reader already preserves
  declared ranges verbatim (milestone 895 FR-007). Matching hpack's output
  character-for-character serves an artifact this feature no longer reads.

## R2 — which manifest wins, and what the existing warning must now say

**Decision**: `package.yaml` wins (already resolved in the spec's
clarification session, FR-002). The implementation consequence is where the
suppression happens.

**Rationale and the code shape**: `emit_hpack_warnings`
(`haskell.rs:2417`) iterates `cabal_manifests` and keys on
`manifest.hpack_generated`, so `package_yaml_paths` can only ever *filter*
that loop. A `package.yaml` with no sibling `.cabal` is unreachable in that
function — which is why milestone 143's "detect-and-warn" half fires for
the disagreement case and never for the gap case (FR-004 records this). The
two are separate code paths and fixing one does not fix the other.

So: the hpack reader emits for every `package.yaml`, and a sibling
`.cabal` carrying the hpack generation header is **suppressed** rather than
merged. Suppression is keyed on `hpack_generated`, which already exists and
is already tested (`parse_cabal_manifest_detects_hpack_header`). A
hand-written `.cabal` beside a `package.yaml` is not suppressed, because it
is not hpack output and the manifest is not its source.

FR-004a's rewrite is then forced: the current text says "run 'hpack' to
regenerate before scanning if package.yaml has been edited", and under
FR-002 regenerating cannot change the document. Leaving it would instruct an
operator to perform a step that cannot affect the output, which is worse
than silence because it looks like a remedy.

**Alternatives considered**: emitting from both and deduplicating. Rejected
— this is the second-source-of-truth failure milestone 143 was right to
fear, and FR-001 explicitly does not license it.

## R3 — the optional-mark withdrawal (FR-006)

**Measured**: `measurements/reproduce-1273.sh` (synthetic, `declared=5`
containing both marked names) and `measurements/sc003-public-library.sh`
(`slack-web` @ `7b46273`, `declared=45`, `restored=2`). Findings Probes C
and D.

**Decision**: compute a **second closure** seeded from the declared set with
the marked names removed, and withdraw a mark only for a component that
second closure reaches.

**Rationale**: `reconcile_optional_with_closure` currently builds
`in_closure` by flattening **both endpoints** of every edge:

```rust
let in_closure: HashSet<&str> = closure_edges
    .iter()
    .flat_map(|e| [e.from.as_str(), e.to.as_str()])
    .collect();
```

A marked component is a *seed*, so the walk expands it and it appears as
`e.from` of its own out-edges. Membership is therefore guaranteed by
construction, and the test can only ever confirm the marks it was handed.

Two cheaper-looking fixes were considered and both are wrong:

- **Use `e.to` only.** Still unsound: a marked component reached by another
  marked component is a `to`, so a pair of marks confirms each other.
- **Use `ClosureMember.reached_from` and require one parent outside the
  marked set.** Correct for a directly-marked component, wrong one level
  down. Findings Probe D measured 8 non-boot components reachable from the
  root *only* through the marked pair; each of those has a parent that is
  not itself marked, so a parent-local test would withdraw nothing for the
  pair and still publish all 8 as runtime-reachable. The error fans out, so
  the test has to be global.

Re-seeding is the only formulation that is correct at every depth: a
component the remaining closure still reaches really is built, and one it
does not reach keeps its mark along with everything below it. It is also
what #1281 proposed, independently.

Cost is one extra BFS over an already-parsed in-memory package set, which
the existing walk measures as `relations_walked`. No new I/O, no new fetch.

**FR-008's diagnostic** falls out of the same change: the current
`restored=N` cannot distinguish `N=0` from "the pass returned early". The
function has two early returns (`closure_edges.is_empty()`, and the whole
`enrich` gate above it) that produce no record at all. The reconciliation
must report *ran / did not run* alongside the count, in the document per
FR-018 — which is the same lesson milestone 973 already learned for the
closure summary, recorded in `ClosureSummary`'s own doc comment.

## R4 — licenses from the cached package set (FR-010, FR-011, FR-011a)

**Measured**: `measurements/license-shapes.sh` at revision `a799d3e3`.
Findings Probe B.

**Decision**: extend `PackageSetEntry` with a `license: Option<String>`
parsed at package-set parse time, resolved through a three-family
classifier, and attached only to components this pass resolved.

**Rationale**: the shape is fully enumerable and small. 19,437 declarations,
all single-line, partitioning exactly into three families:

| family | count | handling |
|---|---|---|
| `lib.licenses.<attr>` | 14,481 | 15-entry attribute → SPDX-id table |
| `getLicenseFromSpdxId "<id>"` | 3,962 | pass through to `SpdxExpression::try_canonical` |
| bare string | 994 | canonicalise; 48 succeed, 946 do not |

Three measured facts make this cheap and bound the risk:

1. **Zero multi-line forms, zero list forms, zero `with lib.licenses;`
   blocks.** A line-oriented read is sound — asserted by the probe rather
   than assumed, which is what makes the regex approach defensible in the
   same file the existing `version_re` / `sha256_re` already scan.
2. **The attribute vocabulary is 15 names**, not nixpkgs-wide. A lookup
   table, not a Nix evaluation. `publicDomain` (313) is the one with no
   clean SPDX identifier and is FR-011's case.
3. **Every derivation block declares a license.** #1280's `0 of 66` is not
   a coverage ceiling in the source; it is that nothing reads the field.

**FR-011a** is a Phase 0 finding promoted into the spec: `"unknown"` is 360
of the 994 bare strings, and it asserts the license is *not known*.
Publishing the string `unknown` into `licenses[]` is worse than an absent
field — a consumer filtering for components with no license information
would not find it. Treated as absent. `"GPL"` / `"LGPL"` / `"AGPL"` (586)
are genuine but unversioned and remain FR-011's case: preserved as declared,
recorded as uncanonicalised.

**Alternatives considered**: `nix eval` of the package set's `meta.license`.
Rejected — it is the m1034 evaluation tier, needs `nix` on the host (FR-021,
FR-022 forbid both), and would make a license field depend on host state.

## R5 — reconciling `waybill:unresolved-reason` (FR-013, FR-014)

**Decision**: strip C151 at the point the nixpkgs pass assigns a version,
inside the same mutation that writes it.

**Rationale**: the reader stamps C151 at read time, correctly — at that
moment the component has no version and the reason names the missing
artifact (`REASON_CABAL_NO_FREEZE`, `REASON_STACK_NO_LOCK`,
`REASON_FREEZE_RANGE_NOT_PIN`). The nixpkgs pass later fills the version and
leaves the annotation. So the contradiction is not a wrong reason, it is a
stale one, and the fix belongs where the fact changes rather than in a
sweep at emission time. A sweep would also have to decide what to do about
every other reader's C151, which is out of scope (FR-015).

**Measured population, and why it is not yet final**: the re-measurement
found 2 components both resolved and carrying a reason, and 4 of 66
carrying the annotation at all. The 2 are the same 2 US2 is about, so part
of US4 resolves as a consequence of US2. The spec's own assumption says
US4's remaining population must be measured **after** US2 lands rather than
predicted now, and the task ordering reflects that: US4's measurement task
depends on US2's implementation task.

The 62 that carry no reason are not a defect. They are closure-transitive
members that the pass **did** resolve, so FR-013 says they must not carry
one. C151's "universal across design-tier components" specification is
satisfied by the components that remain unresolved, and FR-014 is about
those.

## R6 — where the signals live (FR-018, FR-019, FR-020)

**Decision**: reuse `waybill:graph-completeness` (milestone 158) for the
completeness claim and `waybill:unresolved-reason` (C151, milestone 236) for
per-component reasons. Add **one** document-scope reason code to
`graph_completeness::ReasonCode` for the unread-manifest condition, and
extend the existing nixpkgs document-scope records (`EnrichmentSummary`,
`ClosureSummary`) with the withdrawal account.

**Rationale**: Principle V requires auditing for an existing construct
before inventing a `waybill:` key, and both of these already express
"waybill could not establish this". `ReasonCode` is an enum with nine
variants whose `as_str` renders into the existing
`waybill:graph-completeness` value, so a new variant adds a reason code
rather than a catalog row — no new parity extractor is needed, which is the
trap FR-019 names. Adding a documentation row ahead of emission code breaks
`parity::extractors::tests::every_catalog_row_has_an_extractor` by design.

**The FR-005 site**: `compute_graph_completeness` returns
`trivially_complete()` for an empty component set — "an empty SBOM is a
well-defined complete graph". That is true of a genuinely dependency-free
project and false of one whose manifest went unread, and the two are
indistinguishable at that point in the code because the reason the set is
empty is not in scope there. So the unread-manifest fact has to be carried
from the reader to the completeness pass; it cannot be inferred at the
completeness pass.

**FR-017's guard**: every diagnostic added here needs an input that triggers
it. That is not a wish — US1 and US2 are both instances of a signal that no
input could produce, and `emit_hpack_warnings` is a measured case of a
warning wired for one branch only. Each new diagnostic gets a fixture whose
sole purpose is to fire it, and mutation-testing (SC-006) is what proves the
fixture is not vacuous.

## R7 — what must not change (FR-015, FR-016, SC-007)

**Decision**: the dev-axis and optional-axis output of every other ecosystem
is held byte-identical, verified by the existing suites rather than by new
assertions.

**Rationale and the specific hazard**: milestone 1271's FR-003 is a live
constraint here. An out-of-band `Optional` must survive a `devDependencies`
declaration of the same name, and milestone 1272 measured that folding an
extra section into a runtime seed silently reassigns a component from
Optional to Development — a change to the *development* axis's output,
forbidden by that milestone's FR-010. This milestone touches an optional
seed set, so the same failure shape is available: the re-seeded closure must
remove the marked names and change nothing else about the seed.

The dev axis caught that mistake itself in 1272, by a test failing. The
equivalent protection here is that the Haskell dev-scope tests
(`stanza_lifecycle_scope`'s `TestSuite`/`Benchmark` → `Development`) run
unchanged, and R1's lowering puts hpack test stanzas through them.

**Goldens**: the corpus goldens are `#[ignore]`d locally and
`public-corpus.yml` runs only on `schedule` and `workflow_dispatch`, never
on a PR, so a reader change surfaces as a red nightly rather than a red PR
unless it is driven deliberately.

An earlier draft of this section asserted the corpus had no Haskell target,
by analogy with milestone 1272's npm work. **That was wrong, and the
correction is load-bearing.** Findings Probe E measured the four
Haskell/Nix targets against their pinned revisions:

| target | `package.yaml` | `flake.lock` | predicted |
|---|---|---|---|
| `haskell-aeson` | 0 | 0 | **unchanged** — the control |
| `haskell-security-advisories` | 0 | 1 | changed by US3, maybe US2/US4 |
| `haskell-language-server` | 1 (test data, with sibling `.cabal`) | 1 | changed; exercises FR-002 suppression |
| `nix-closure-moat` | **1 at the root, no sibling** | 1 | changed by all four stories |

Two consequences for the plan:

1. **`nix-closure-moat` is a live witness to #1278** inside the project's
   own corpus. Its golden records 321 hackage components derived without
   reading the root `package.yaml`. The primary defect is not only
   reproducible on a synthetic fixture; it is committed.
2. **`haskell-aeson` is a control.** No flake and no hpack manifest means
   no code path this milestone touches can reach it. A golden refresh that
   changes `haskell-aeson` is evidence of unintended blast radius, not of
   this milestone working — which is exactly the kind of positive control
   the project's own standing rule asks for.

The refresh therefore gets its own task and goes through the workflow
dispatch. `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is never set locally.

## R8 — no new dependencies, no new surface (FR-021, FR-022)

**Decision**: `serde_yaml` (already a direct dependency, already parsing
`stack.yaml.lock` in this very file), `regex` (already scanning the package
set), `spdx` via `SpdxExpression::try_canonical` (already used by the
`.cabal` license path, milestone 954). **Zero new Cargo dependencies, zero
subprocess calls, zero network calls, no new CLI flag.**

**Rationale**: FR-021 settles the subprocess question by not needing one,
and FR-022 forbids host-`PATH`-dependent behaviour outright. The project has
already shipped fixtures whose Gradle tier was silently decided by a system
`gradle` on CI's runner image, which is the concrete form of the harm.

Note the asymmetry Phase 0 measured: hpack itself is *unavailable* offline
for a `defaults:` manifest — it refuses the whole file. So there is no
configuration in which a subprocess tier would do better than announcing the
gap, which is the strongest available argument for FR-001b/FR-001c.
