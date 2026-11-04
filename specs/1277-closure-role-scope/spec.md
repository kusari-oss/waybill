# Feature Specification: a closure member that only builds the artifact must not read as part of it

**Feature Branch**: `1277-closure-role-scope`
**Created**: 2026-11-03
**Status**: Draft
**Input**: Issue #1342 — "A closure member that only builds the artifact reads as part of it: no native `scope` is ever set"

## Context

`--nix-closure` classifies every closure member by how nix referenced it.
Catalog row C182 `waybill:closure-role` carries one of `artifact-input`,
`build-tooling`, `both` or `unreferenced`. The classification is read from
nix's own fields rather than inferred: `buildInputs`,
`propagatedBuildInputs` and `depsHostHost` are what goes into the thing
being built; `nativeBuildInputs`, `depsBuildBuild`, `depsBuildHost` and
`nativeCheckInputs` are what builds it.

No member carries a native CycloneDX `scope` **derived from that role**.
CycloneDX 1.6 states that "if scope is not specified, `required` scope
SHOULD be assumed by the consumer of the BOM", so a consumer filtering
`scope` to answer "what is actually in this artifact" is handed every build
tool as though it shipped.

This is the mirror of milestone 1276 (#1340) on the same field and the same
schema text, sign reversed. #1340: a component that **is** in the artifact
was scoped as absent. This: a component that is **not** in the artifact
reads as present.

## Measured before specifying

Taken on `main` at `5a0fa58` from the committed corpus goldens — the
refreshed ones, post-1276 — rather than from #1342's figures, which were
measured on a different fixture (`waybill-fixture-ifd`, 1,290 derivations).
Harness and raw output: `measurements/baseline.md`.

### Only one corpus target carries closure roles at all

| target | components | with a `closure-role` |
|---|---|---|
| `nix-closure-moat` | 387 | **362** |
| `haskell-security-advisories` | 251 | 0 |
| `haskell-language-server` | 399 | 0 |

The other two Haskell targets do not run `--nix-closure`, so every
corpus-scale criterion below is stated against `nix-closure-moat` alone, and
the rest of the corpus is a byte-identity control.

### The role distribution on that target

| role | components | of those, `pkg:hackage` |
|---|---|---|
| `artifact-input` | 231 | 131 |
| `both` | 34 | 2 |
| `build-tooling` | **80** | 1 |
| `unreferenced` | **17** | 0 |

**97 of 362 members are in a role that does not ship**, and read as
`required` by the schema's own default rule.

### #1342's premise is wrong in one respect, and it matters

#1342 says "**no** member carries a native CycloneDX `scope`". Ten do:

| role | `scope` | n |
|---|---|---|
| `artifact-input` | `excluded` | 9 |
| `both` | `excluded` | 1 |

All ten are test frameworks — `hspec`, `hspec-core`, `hspec-golden`,
`hspec-discover`, `hspec-expectations`, `HUnit`, `QuickCheck`,
`quickcheck-io`, `call-stack`, `haskell-lexer` — carrying
`waybill:lifecycle-scope = development`. They are already scoped, by
milestone 1269's non-runtime propagation, through the *cabal* dependency
graph rather than the nix closure graph.

Their role is `artifact-input` because nixpkgs' Haskell builder passes test
dependencies through `buildInputs`, which is in `ARTIFACT_FIELDS`. So
**`artifact-input` does not mean "present in the artifact at runtime"** — it
means "nix referenced it as an input to the thing being built". #1342's
candidate mapping reasoned from the stronger reading. The weaker one is what
the classifier implements, and it is the reason this feature must only ever
*add* a non-runtime scope and never assert `required`.

### The compiler-supplied overlap is empty on the corpus

Milestone 1276 made a compiler-supplied package immune to **inheriting** a
non-runtime scope; this feature **assigns** one from the role, so a boot
library in a tooling role would be in both populations and invariant I9
asserts it carries no non-runtime scope.

Measured: exactly one `pkg:hackage` component sits in a tooling role on
`nix-closure-moat` — `pkg:hackage/happy@2.1.7`, `build-tooling`. It is **not
compiler-supplied**: in the pinned revision's compiler configurations
`happy` appears only as an alias (`happy = self.happy_1_20_1_1`), never as
`happy = null`, so it is in the alias map and not the nulled set.

**The overlap this feature must reconcile has a population of zero on the
corpus today.** It is reachable in principle — nothing stops a nulled
package appearing in `nativeBuildInputs` — so the rule must be stated and
tested, but it must be tested against a synthetic fixture. A corpus
assertion would pass whether the rule works or not, which is the failure
milestone 1275 found in its own most-important test (F7 there).

## Clarifications

### Session 2026-11-03

- Q: What CycloneDX scope should an `unreferenced` closure member carry? → A: `excluded`, the same as `build-tooling`. The categories the classifier documents for that role — fetched sources, patches, setup hooks, bootstrap stages — are none of them reachable in a runtime call graph, which is the schema's own definition of `excluded`. Leaving the scope absent is not the neutral option: the schema reads absent as `required`, so it claims a patch file ships.
- Q: When a closure member is both compiler-supplied and in a tooling role, does the milestone-1276 exemption win or the role-derived `excluded`? → A: The exemption wins and invariant I9 stays exactly as milestone 1276 wrote it. The measured population is zero, so the two answers are indistinguishable on real data and the choice is only about which statement the code makes when the case appears. Keeping I9 as written makes it **fail loudly** at that moment, so the decision is taken with data rather than speculatively now. The opposing reading was recorded rather than discarded: `excluded` means "not reachable within a call graph at runtime", and a `build-tooling` role is positive evidence of non-presence where the dependency edge m1276 reasoned about was not — so if I9 ever fires on this overlap, that argument is the one to weigh, not a bug to patch.
- Q: `/speckit.analyze` finding C1 — the classification was to be carried in CycloneDX only, leaving both SPDX formats silent. Is that acceptable? → A: No. Both SPDX formats have a native construct for this semantic (2.3 `BUILD_TOOL_OF`, 3.0.1 `LifecycleScopedRelationship` + `LifecycleScopeType/build`), and Principle V's parity-bridge carve-out applies only "when one format has the native field but another doesn't" — so a `waybill:*` substitute is forbidden here and silence is the weakest remaining position, weaker than milestone 112's own. US4, FR-012, FR-013, FR-014, SC-010 and SC-011 added. The plan's earlier recommendation — CycloneDX only, with the gap filed as a follow-up — is superseded; the analysis showed SPDX would carry the classification *not at all*, which is a stronger objection than the one the plan weighed.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - a build tool stops reading as shipped (Priority: P1)

An operator consuming a waybill SBOM asks "what is actually in this
artifact" by filtering out components whose `scope` is not `required`. Today
every nix build tool survives that filter, because its scope is absent and
the schema reads absent as `required`. After this story, a member nix
referenced only through its tooling fields carries `excluded`, and the
filter answers the question the operator asked.

**Why this priority**: it is the whole defect, it covers 80 of the 97
mis-reading components, and the schema text that settles it is the same text
milestone 1276 already turned on. It is also the only story whose population
is non-empty on the corpus without a new fixture.

**Independent Test**: scan a Nix-built project with `--nix-closure` and
confirm every `build-tooling` member carries `excluded`, every
`artifact-input` and `both` member's scope is unchanged from the pre-feature
document, and the non-`--nix-closure` corpus targets are byte-identical.

**Acceptance Scenarios**:

1. **Given** a closure member nix referenced only through `nativeBuildInputs`, `depsBuildBuild`, `depsBuildHost` or `nativeCheckInputs`, **When** the document is emitted, **Then** that component carries `scope: excluded` and retains its C182 `build-tooling` role annotation.
2. **Given** a closure member nix referenced through `buildInputs`, `propagatedBuildInputs` or `depsHostHost`, **When** the document is emitted, **Then** this feature assigns it no scope, and any scope it already carries from the lifecycle propagation is preserved byte-for-byte.
3. **Given** a member referenced both ways (`both`), **When** the document is emitted, **Then** this feature assigns it no scope — the union rule already resolves `Unreferenced` by absorption and an artifact path by presence, so a `both` member has an artifact path.
4. **Given** a scan without `--nix-closure`, **When** the document is emitted, **Then** it is byte-identical to the pre-feature output.

---

### User Story 2 - the exemption that milestone 1276 established is not re-broken (Priority: P2)

A package the pinned revision's compiler supplies is installed in every
build. Milestone 1276 made such a package immune to inheriting a non-runtime
scope, and invariant I9 asserts that no compiler-supplied component carries
one. A role-derived assignment reaches the same field from the other
direction, so a boot library appearing in a tooling role must not acquire
`excluded` through this feature.

**Why this priority**: the rule is required for correctness and I9 would
fail without it, but its corpus population is zero, so it cannot be the MVP
and cannot be demonstrated on real data. It is P2 rather than P1 for that
reason, not because the correctness matters less.

**Independent Test**: a synthetic fixture whose closure places a nulled
package in `nativeBuildInputs`. Without the rule the component acquires
`excluded` and I9 fails; with it the component carries its `build-tooling`
role and no scope. The fixture is the test — the corpus cannot distinguish
the two outcomes.

**Acceptance Scenarios**:

1. **Given** a closure member that is compiler-supplied **and** in a tooling role, **When** the document is emitted, **Then** it carries no scope from this feature and I9 passes.
2. **Given** the same member, **When** the document is emitted, **Then** its C182 role annotation still reads `build-tooling` — the exemption suppresses the scope, not the classification.
3. **Given** a tooling-role member that is **not** compiler-supplied, **When** the document is emitted, **Then** it carries `excluded`, so the exemption is narrow rather than blanket.

---

### User Story 3 - the operator can tell why a scope was set (Priority: P3)

A reviewer reading `scope: excluded` on a closure member cannot tell which
mechanism set it: the lifecycle propagation through the cabal graph, or the
closure role through nix's fields. The two answer different questions and
can be right or wrong independently, so the document should record which one
spoke.

**Why this priority**: it is a transparency improvement rather than a
conformance fix, and the two prior stories are correct without it. It earns
its place because the overlap measured above — ten components already scoped
by the propagation, in a role this feature deliberately leaves alone — is
exactly the configuration a reviewer would otherwise misread.

**Independent Test**: emit a document in which both mechanisms have fired on
different components and confirm each scoped component names its source, and
that a component scoped by neither carries no such annotation.

**Acceptance Scenarios**:

1. **Given** a component scoped by the closure role, **When** the document is emitted, **Then** it records that the role was the source.
2. **Given** a component scoped by the lifecycle propagation, **When** the document is emitted, **Then** its existing derivation annotation is unchanged and this feature adds nothing.
3. **Given** a component with no scope, **When** the document is emitted, **Then** no scope-source annotation is present — absence means "nothing set it", not "set by default".

---

### User Story 4 - the same question answered in SPDX (Priority: P2)

An operator consuming SPDX rather than CycloneDX asks the same question —
what is actually in this artifact. SPDX has no `scope` field; it answers
through relationship types, and both SPDX formats carry a native construct
for exactly this semantic. After this story a member in a tooling role is
related to the artifact as a build tool rather than as a plain dependency,
so the SPDX consumer is not left with the one reading CycloneDX now
forbids.

**Why this priority**: Principle V states that where a native construct
exists, waybill MUST use it as the primary signal — and both exist: SPDX 2.3
`BUILD_TOOL_OF` and SPDX 3.0.1 `LifecycleScopedRelationship` with
`LifecycleScopeType/build`. Principle V's parity-bridge carve-out cannot
cover a `waybill:*` substitute here, because it applies only "when one
format has the native field but another doesn't", and SPDX has one. Emitting
nothing in SPDX is therefore the weakest of the available positions, and
weaker than milestone 112's own, which pairs its CycloneDX field with an
SPDX annotation. P2 rather than P1 because CycloneDX is where the defect was
reported and US1 remains the shippable MVP.

**Independent Test**: emit SPDX 2.3 and SPDX 3 for a Nix-built project with
`--nix-closure`; every member in a tooling role carries the native
build-tool relation, every `artifact-input` and `both` member's relations
are unchanged, and the I-series invariants still pass.

**Acceptance Scenarios**:

1. **Given** a closure member whose unioned role is `build-tooling` or `unreferenced`, **When** SPDX 2.3 is emitted, **Then** it carries a `BUILD_TOOL_OF` relationship to the artifact.
2. **Given** the same member, **When** SPDX 3 is emitted, **Then** it carries a `LifecycleScopedRelationship` whose scope is `build`.
3. **Given** a compiler-supplied member in a tooling role, **When** either SPDX format is emitted, **Then** no build-tool relation is added — the FR-005 exemption governs every format, not only CycloneDX.
4. **Given** the new relations, **When** the corpus invariants run, **Then** graph completeness, the orphan-reason rows and every I-series row still pass, because added relations change the relationship graph those rows read.

### Edge Cases

- **A member in the `unreferenced` role that applies a patch.** Nothing references it, yet it contributes a patch to something that ships. Per FR-004 it carries `excluded`: the patched code ships, the patch does not, and the component is the patch. The 17 on the corpus exist precisely because m1035 measured `jq` and `lua` in this position carrying 6 of one project's 18 CVEs — `excluded` keeps them in the document, which is where that finding needs them, while stopping them reading as shipped.
- **A member that `unreferenced` catches because waybill did not read the field that references it.** The role is derived by absence from two explicit field lists — 3 artifact fields and 4 tooling fields — while nixpkgs' stdenv sets more dependency keys than those seven. Such a member would acquire `excluded` under FR-004 while genuinely shipping. Whether any real derivation does this is **unmeasured**: every fixture in the tree carries only the seven fields the code already knows, so they cannot answer it, and a real closure requires a host `nix`. See FR-004a.
- **A member whose variants disagree about their role.** The classifier unions the flags rather than picking one — measured, 13 of 97 duplicated identities in a real closure disagree, and `flex 2.6.4` is `Both` as one variant and `Unreferenced` as another. The scope must be derived from the unioned role, never from a single variant.
- **A member already scoped by the lifecycle propagation.** Both mechanisms only ever assign a non-runtime scope and neither asserts `required`, so the cases compose: tooling-and-dev-only → `excluded` from either, tooling-and-runtime → `excluded` from the role, artifact-input-and-dev-only → `excluded` from the propagation. There is no combination in which one must overrule the other.
- **A scan where the closure tier degraded.** No roles are emitted, so no scope is assigned — the same fail-closed posture as m1276's `None` boot set. An absent role must never be treated as `build-tooling`.
- **A non-Nix ecosystem.** No closure roles exist, so no document outside a `--nix-closure` scan may move by a single byte.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST assign CycloneDX `scope: excluded` to every closure member whose unioned C182 role is `build-tooling`.
- **FR-002**: The system MUST NOT assign any scope to a member whose role is `artifact-input` or `both`, and MUST leave a scope such a member already carries byte-identical. Absence of a scope is the schema's `required`, and `artifact-input` is measured to include test dependencies, so asserting `required` there would be a claim this feature cannot support.
- **FR-003**: The system MUST derive the scope from the unioned role, never from an individual derivation variant.
- **FR-004**: The system MUST assign CycloneDX `scope: excluded` to every closure member whose unioned C182 role is `unreferenced`. A fetched source, a patch, a setup hook and a bootstrap stage are each a build-time input that is not reachable in a runtime call graph. The patched code ships; the patch does not.
- **FR-004a**: The field lists the role classifier reads MUST be shown to cover the dependency keys real derivations carry, by enumerating the distinct dependency-shaped env keys present in a real closure and reporting any that neither list names. A key found outside both lists means `unreferenced` conflates "nothing references it" with "we did not read the reference", which would make FR-004's assignment unsound for that member; the finding is then recorded and scoped, not silently absorbed.
- **FR-005**: The system MUST NOT assign a role-derived scope to a component that is compiler-supplied, in any role. Invariant I9 MUST remain exactly as milestone 1276 wrote it — unamended — so that a member in this overlap fails the invariant rather than silently acquiring a scope.
- **FR-005a**: The system MUST NOT suppress the C182 role annotation for an exempt member, and MUST record the exemption itself on that component. The exemption withholds the native scope; it does not withhold the role, and it must not withhold the fact that it fired. **Measured requirement**: an exemption that leaves no trace is indistinguishable in the document from a rule that never reached the component, so no invariant can check it and no reader can audit it. This was found by trying to write the invariant FR-011 requires — the exempt member carries no C170 `waybill:haskell-version-unresolved-reason`, so invariant I9's signal does not identify it, and an invariant reusing that signal would have reported correct output as a defect.
- **FR-006**: The system MUST preserve every C182 `waybill:closure-role` annotation value unchanged. This feature adds a native field; it does not replace the annotation, which carries a four-way distinction `scope` cannot express.
- **FR-007**: The system MUST assign no scope when the closure tier degraded or emitted no roles. An absent role is not `build-tooling`.
- **FR-008**: The system MUST emit documents byte-identical to the pre-feature output for every scan that does not enable `--nix-closure`.
- **FR-009**: The system MUST record, on every component **this feature** classifies, that the closure role was the source — and MUST emit that record on no other component. The value MUST distinguish a component classified by the role alone from one the milestone-1269 propagation also classified.
  **Measured scope correction.** An earlier draft required the record on every non-runtime component, whatever classified it. That is redundant and would have been costly: C62 `waybill:lifecycle-scope-derivation` already records the propagation's provenance — `transitive` is precisely "m1269 derived it" — and emitting a second annotation saying the same thing would have put a row on every dev-scoped component in every ecosystem, which is the shape milestone 1276 measured at 35 golden byte-identity failures. Absence of this record therefore means "the closure role did not classify this", which is the only claim it is in a position to make.
- **FR-010**: The system MUST carry any new annotation introduced by FR-009 through all three output formats under a catalog row with a Principle V standards-native audit, performed against the vendored schemas rather than from memory.
- **FR-011**: The system MUST keep invariant I9 passing on every corpus target, and MUST add an invariant asserting **FR-001 and FR-004** — that no member whose unioned role is `build-tooling` or `unreferenced` lacks a non-runtime marker — so neither half of the defect can return silently. An invariant covering only `build-tooling` would leave the 17 `unreferenced` members unguarded.
- **FR-012**: The system MUST emit the SPDX 2.3 native construct for a member in a tooling role — `BUILD_TOOL_OF`, relating the member to the artifact — rather than leaving the classification unexpressed in that format or substituting a `waybill:*` annotation for it. Principle V audit: `relationshipType` in the vendored `spdx-2.3.json` carries `BUILD_TOOL_OF`, `BUILD_DEPENDENCY_OF`, `PATCH_FOR` and `PATCH_APPLIED`; `BUILD_TOOL_OF` is the match for "builds the artifact and is not in it".
- **FR-013**: The system MUST emit the SPDX 3.0.1 native construct for the same member — a `LifecycleScopedRelationship` whose `scope` is `LifecycleScopeType/build`. Principle V audit: the vendored `spdx-3.0.1.json` defines `LifecycleScopeType` with `build`, `design`, `development`, `other`, `runtime` and `test`, and `LifecycleScopedRelationship` to carry it.
- **FR-014**: The FR-005 exemption MUST govern all three formats. A compiler-supplied member in a tooling role acquires no CycloneDX scope, no `BUILD_TOOL_OF` and no build-scoped relationship.

### Key Entities

- **Closure role**: how nix referenced a member, unioned across that member's derivation variants. One of `artifact-input`, `build-tooling`, `both`, `unreferenced`. Already emitted as C182.
- **Role-derived non-runtime classification**: the conclusion, drawn from the closure role, that a member is not in the artifact — as distinct from the same conclusion propagated through the dependency graph by milestone 1269. Each format expresses it with its own native construct: CycloneDX `scope: excluded`, SPDX 2.3 `BUILD_TOOL_OF`, SPDX 3 `LifecycleScopeType/build`. Named for the conclusion rather than for CycloneDX's field, because two of the three formats have no such field.
- **Compiler-supplied set**: the packages the pinned revision's compiler configurations null out, carried by milestone 1276 and exempt from non-runtime scope.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On `nix-closure-moat`, all **80** `build-tooling` members carry `excluded`, up from 0.
- **SC-001a**: On `nix-closure-moat`, all **17** `unreferenced` members carry `excluded`, up from 0, so **97 of 362** members stop reading as `required`.
- **SC-002**: On `nix-closure-moat`, the **265** members in `artifact-input` or `both` roles carry exactly the scopes they carry today — the same 10 `excluded` and no others — so the feature adds no scope outside the roles it targets.
- **SC-003**: A consumer filtering to `required` scope on `nix-closure-moat` sees **97** fewer components than today, and every one removed is a member nix referenced through a tooling field or through neither field set.
- **SC-004**: The 16 corpus targets that do not carry closure roles emit byte-identical documents.
- **SC-005**: Invariant I9, unamended, passes on all 17 corpus targets with no declaration, as it does today. An amended I9 passing would not satisfy this criterion.
- **SC-006**: The new invariant asserting FR-001 fails on a document in which a `build-tooling` member's scope has been removed, demonstrated by a planted mutation rather than by assertion.
- **SC-007**: The compiler-supplied exemption is demonstrated on a synthetic fixture in which it changes the outcome — the component carries `excluded` without the rule and no scope with it. A criterion stated against the corpus would be satisfied vacuously, since the overlap's measured population there is zero.
- **SC-008**: Every component this feature classifies on `nix-closure-moat` names the closure role as the source, and that count equals 97 (SC-001 plus SC-001a). No component outside that set carries the record.
  **Not** "every scoped component names its mechanism", which the first draft asked for and which is unachievable: measured on `nix-closure-moat`, 7 of 11 already-scoped components carry C62 and four carry no provenance at all, because C62 fires only where waybill *derived* the scope rather than reading it from a manifest. That is C62's existing rule and this feature does not change it.
- **SC-009**: The dependency-shaped env keys present in a real closure are enumerated, and the count neither field list names is reported. The criterion is that the number is **known and recorded**, not that it is zero — a non-zero answer is a finding to scope, and quoting zero without the enumeration would be the assumption FR-004a exists to retire.

## Assumptions

- **`artifact-input` and `both` keep an absent scope.** The classifier reads `buildInputs`, which nixpkgs' Haskell builder uses for test dependencies, so the role does not establish runtime presence and this feature will not assert it. The dev-only members in that role are already scoped correctly by the milestone-1269 propagation through the cabal graph — the two mechanisms compose, as the edge-case analysis sets out, and neither needs to overrule the other. Recorded as an assumption rather than a question because the status quo is also the conservative answer.
- **The role mapping and the lifecycle propagation never conflict.** Both only ever assign a non-runtime scope; neither asserts `required`. Every combination of the two is enumerated in the edge cases and none requires a precedence rule. Should a future mechanism assert `required`, that assumption lapses.
- **Only `nix-closure-moat` exercises this feature in the corpus.** Measured: the other two Haskell targets emit zero closure roles. Corpus-scale criteria are therefore single-target, and the breadth comes from synthetic fixtures plus the byte-identity control across the other 16.
- **Goldens are refreshed through CI only.** `public-corpus.yml` with `regen_goldens=true`; never locally (rule zero). One corpus target's goldens will move.
- **The 944 derivations waybill does not emit are out of scope.** 800 unreferenced (107 named `source`) and 183 collapsed build variants. Emitting them needs an identity answer for the source derivations first — `pkg:generic/source` ×107 is the shape m1035's own comment warns against — and m1035 measured that emitting the variants put `perl 5.42.0` in one document seven times. Comparison data: `specs/1276-boot-library-scope/measurements/sbomnix-comparison.md`.
- **`waybill-ebpf` is untouched.** User-space only, no new dependencies.
