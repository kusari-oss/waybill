# Phase 0 research — closure-role-derived scope

Every claim below was read out of the tree or the vendored schemas at
`5a0fa58`. Where something could not be measured it says so and becomes a
task, not an assumption.

## R1 — the mechanism exists already, and `LifecycleScope::Build` is it

**Finding.** `LifecycleScope` (`waybill-common/src/resolution.rs:432`) has a
`Build` variant; `is_non_runtime()` is true for every variant but `Runtime`
(`:496`); and the CDX builder emits `scope: "excluded"` for any non-runtime
lifecycle scope (`cyclonedx/builder.rs:1400-1406`). Milestone 052 wired that
one field to the native construct in all three formats.

Principle V names this exact chain as its canonical case:

> Milestone 049's `waybill:dev-dependency` annotation (later removed by
> milestone 052 in favor of CDX `scope`, SPDX 2.3
> `DEV/BUILD/TEST_DEPENDENCY_OF`, and SPDX 3 `LifecycleScopeType`) is the
> canonical motivating case.

So the naive design is one line: set `lifecycle_scope = Build` on every
member whose role is `build-tooling` or `unreferenced`, and all three native
carriers follow for free.

**Two measured consequences rule it out.**

1. **The CDX emission is gated on `include_dev`** —
   `if self.config.include_dev { if let Some(scope) = … }` at
   `builder.rs:1400`. A default scan sets no scope, so the feature would not
   fire at all in the configuration the defect was reported against.
2. **`--exclude-scope build` would then drop the 97 components.** The flag
   maps straight onto `LifecycleScope` (`main.rs:417`) and
   `apply_exclude_scope_filter` (`scan_cmd.rs:3081`) removes matching
   components. That destroys the reason those members are emitted at all:
   m1035 measured `jq` and `lua` in the `unreferenced` role carrying 6 of
   one project's 18 CVEs, and the spec's FR-004 rationale depends on
   `excluded` **keeping them in the document**.

**Decision**: do **not** route through `lifecycle_scope`.

**Alternatives considered.** Ungating the CDX emission: rejected, it changes
the scope field for every non-runtime component in every ecosystem, which is
far outside this spec. Assigning at emit time from the C182 annotation:
rejected under R3, because the exemption's input lives in the resolve stage.

## R2 — milestone 112 already solved both carve-outs, for an identical claim

**Finding.** `BuildInclusion::NotNeeded` emits `scope: "excluded"`
**unconditionally** — outside the `include_dev` gate — at
`builder.rs:1419-1424`, and its comment states the rationale:

> `go mod why` proved the production build does not need this module, which
> is a build-graph fact, not a lifecycle-scope preference the operator can
> toggle. The component is kept in the SBOM (never dropped by scope
> filtering — its `lifecycle_scope` stays `None`, so `--exclude-scope` can't
> match it).

That is both carve-outs, already built, for the same kind of claim. nix
recording a derivation in `nativeBuildInputs` is a build-graph fact in
exactly the sense `go mod why` is: read from the build system's own record,
not inferred, and not an operator preference.

**Decision**: follow m112's shape. A dedicated per-component signal,
`lifecycle_scope` left untouched, unconditional emission. This also keeps
the feature clear of the m1269 propagation and the m1276 immunity, both of
which operate on `lifecycle_scope` — two mechanisms on one field is what
made #1342's premise wrong in the first place.

## R3 — where the exemption reads "compiler-supplied", and why it matters

**Finding.** Two signals for the same fact already exist, read by different
code:

| reader | signal |
|---|---|
| m1276 propagation immunity (`scope_propagation.rs`) | the **boot set**, via `is_haskell_purl && boot.contains(name)` |
| invariant I9 (`checks/compiler_supplied_scope.rs:47`) | **C170**, `waybill:haskell-version-unresolved-reason == "compiler-supplied"`, read off the emitted document |

I9's own comment insists on the discipline: "Both must read one source, or
this row and the propagation disagree about one document and each is locally
satisfiable while the pair is not."

**Decision**: the new exemption reads the **boot set**, co-located with the
propagation in the resolve stage. Two reasons. The boot set does not depend
on the Haskell enrichment having emitted an annotation, and I9 reads the
emitted document independently — so if the two ever disagree, I9 fails
rather than both passing.

**This also settles where the assignment happens.** `boot_libraries` is
already threaded to the resolve stage by m1276 (`scan_cmd.rs` ~4891), and
the C182 role annotation is on `ResolvedComponent.extra_annotations` by then
— `orphan_reason.rs:392` reads exactly that key. Both inputs are in one
place, once, and nothing new needs plumbing.

## R4 — I9 was written for this feature before it existed

Not a decision, but it should be on the record. I9's header says:

> **Why it exists although the population is zero.** Measured at the point
> milestone 1276 landed, all 10 affected components derived their scope
> transitively, so the propagation's immunity is the whole fix and nothing
> else can currently assign one. This row covers the origin the fix does not
> reach: a direct mark, **or a producer this milestone did not anticipate**.
> … milestone 1275's F12 is the precedent, where a new producer reached a
> population nobody had connected it to.

This feature is that producer. The invariant is already in place, reads the
emitted document, and will fail if FR-005 is got wrong — which is why SC-005
asks for it **unamended** and SC-007 asks for a fixture that makes the rule
load-bearing rather than a corpus assertion that cannot move.

## R5 — the FR-009 annotation passes the Principle V audit; the native fields do not cover it

Audited against the vendored schemas at
`waybill-cli/tests/fixtures/schemas/`, not from memory.

- **CDX 1.6** (`cyclonedx-1.6.json`, `component.scope`): the enum carries the
  *classification* — `required` "required for runtime", `optional` "not
  capable of being called due to them not being installed", `excluded` "not
  reachable within a call graph at runtime", `"default": "required"`. There
  is no slot for which mechanism concluded it.
- **SPDX 2.3**: `relationshipType` has no provenance member either.
- **SPDX 3.0.1**: `LifecycleScopedRelationship` carries a `scope` of
  `LifecycleScopeType` (`build`, `design`, `development`, `other`,
  `runtime`, `test`) — again the classification, not its origin.

**Decision**: FR-009's annotation is permitted under Principle V's carve-out
for "finer-grained information the standard does not express". It must be
documented in `docs/reference/sbom-format-mapping.md` with the audit, and it
should record the **provenance of the non-runtime classification** rather
than "which mechanism wrote the CDX field" — the former is a coherent claim
in all three formats, the latter names a field two of them do not have.

## R6 — the native carriers in SPDX exist, and the spec under-scoped them

**This is the one open question in the plan, and it needs a decision that is
not mine to take.**

The spec's FR-001 and FR-004 say "CycloneDX `scope`". Measured, both SPDX
formats have a native construct for the same semantic:

| format | native carrier | present? |
|---|---|---|
| CDX 1.6 | `component.scope = "excluded"` | yes, spec'd |
| SPDX 2.3 | `BUILD_TOOL_OF`, `BUILD_DEPENDENCY_OF`, and for the patch case `PATCH_FOR` / `PATCH_APPLIED` | **yes, not spec'd** |
| SPDX 3.0.1 | `LifecycleScopedRelationship` + `LifecycleScopeType/build` | **yes, not spec'd** |

Principle V: "If one exists, waybill MUST use the native construct as the
primary signal." On a plain reading that reaches all three formats, not the
one the spec happens to name.

**But the precedent cuts the other way.** Milestone 112, the model this plan
follows in R2, emits the native field in CDX only — `builder.rs:1419` — and
carries SPDX 2.3 and SPDX 3 with an annotation alone
(`spdx/annotations.rs:259`, `spdx/v3_annotations.rs:297`), documented as a
parity bridge. So the codebase's own answer for a build-graph fact today is
CDX-native plus an SPDX annotation.

**Why I am not deciding it here.** Emitting new SPDX relationships for 97
components changes the relationship graph, which feeds graph completeness,
orphan reasons and the I-series invariants. That is observable output, so by
the project's own test — *a spec change if a second component could ever
observe it* — widening the native carriers is a spec change and not a plan
detail. Three ways forward, for the record:

1. **Amend the spec** to require the native carrier in all three formats,
   and add the user stories and success criteria for it.
2. **Keep the spec as written**, emit CDX-native plus the FR-009 annotation
   as m112 does, and file a follow-up issue for the SPDX natives citing
   Principle V. The annotation is then doing parity-bridge duty, which
   Principle V permits explicitly and which the catalog already documents
   for C-rows of this shape.
3. Keep the spec and argue the SPDX relationship is not equivalent. **Not
   recommended**: `BUILD_TOOL_OF` is a close match, and the constitution
   names this family as its canonical case, so the argument would be thin.

**Resolved against this recommendation by `/speckit.analyze` C1.** The
recommendation above was option 2 — CycloneDX only, SPDX gap filed as a
follow-up, on the strength of m112's precedent. The analysis pass found the
objection sharper than the one weighed here: under option 2 the SPDX
formats carry the classification **not at all**, and that is not what
Principle V's parity-bridge clause licenses. Read again, the clause covers a
`waybill:*` annotation "to bridge a parity gap when one format has the
native field but **another doesn't**". SPDX does have the native field. So
the clause cannot cover either an annotation substitute *or*, by extension,
the reasoning that made silence look acceptable: the premise it rests on —
that the construct is missing there — is false.

Option 2 also mis-read the precedent. m112 pairs its CycloneDX field with an
SPDX **annotation**, so no format is silent; option 2 would have been
weaker than the thing it cited.

**Decision: option 1.** The native construct in all three formats, as
US4 — FR-012 (`BUILD_TOOL_OF`), FR-013 (`LifecycleScopedRelationship` with
`LifecycleScopeType/build`), FR-014 (the FR-005 exemption governs every
format), SC-010 and SC-011.

The risk that made option 2 attractive is real and unchanged, which is what
SC-011 is for: new relations for 97 components change the relationship graph
that graph completeness, the orphan-reason rows and the I-series invariants
all read. It is a reason to predict the movement before the refresh and
audit it after, not a reason to leave two of three formats unable to answer
the question the feature exists to answer.
