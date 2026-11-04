# Phase 0 Research: 1278-closure-structured-attrs

Every claim below is either measured against a committed artifact or read out
of a vendored schema. Where it is neither, it says so.

---

## R1 — Does the incompleteness fact have a native carrier? **It does, and the current output natively claims the opposite.**

**Question**: the clarify session settled that a partial read is disclosed in
the document rather than only logged. Principle V and Principle X both require
spec-native mechanisms before a `waybill:` extension, so: do the three formats
express "this dependency graph is incomplete"?

**Measured against the vendored schemas** at
`waybill-cli/tests/fixtures/schemas/`:

| format | native carrier | verified how |
|---|---|---|
| CycloneDX 1.6 | `compositions[].aggregate`, `aggregateType` enum containing `complete` and `incomplete`. The schema's own description: *"Specifies an aggregate type that describes how complete a relationship is"*, and `incomplete` means *"Additional relationships exist and may include constituent components, services, or dependencies."* | read from `cyclonedx-1.6.json`, `definitions.aggregateType.enum` |
| SPDX 3.0.1 | `Relationship.completeness`, enum `["complete", "incomplete", "noAssertion"]` | read from `spdx-3.0.1.json`, `prop_Relationship_completeness` |
| SPDX 2.3 | **none.** Zero occurrences of `completeness` in the whole schema. | counted in `spdx-2.3.json` |

**And waybill already emits the CycloneDX one.** 31 committed goldens carry a
`compositions` array; it is catalogued as row **E1** with a live extractor, and
m1069 already made SPDX 3 read the same answer through the shared
`dependency_claims` helper.

**The finding that matters.** The committed `nix-closure-moat` golden carries:

```json
[{"aggregate": "incomplete_first_party_only", "assemblies": ["nix-closure-moat@d069055"]},
 {"aggregate": "complete",                    "dependencies": ["nix-closure-moat@d069055"]}]
```

The second record is a **native, schema-blessed claim that the root's
dependency relationships are complete** — "no further dependencies are known
to exist" — in a document whose closure skipped 554 dependency declarations.

So this milestone's defect is not only the nine false `excluded` scopes found
while writing the spec. The document also makes a false claim in CycloneDX's
own vocabulary, in a field a consumer is far more likely to branch on than any
`waybill:` property. That is worse than an inflated `unreferenced` count: it is
an assertion of completeness that the measurement contradicts.

**Decision**: the incompleteness fact goes in the native carriers — CDX
`compositions`, SPDX 3 `Relationship.completeness` — and only the *detail*
that no format expresses goes in a new `waybill:` annotation. SPDX 2.3 gets
the annotation alone, because it has nowhere else to put it.

**Rationale**: Principle V, and the standing lesson that standards-native
fields take precedence over `waybill:` properties. A new annotation carrying
an incompleteness signal *beside* a native field still claiming `complete`
would be the worst of both: two fields disagreeing, with the native one
winning in every consumer that does not know waybill.

**Alternative rejected**: a single new annotation carrying everything, leaving
`compositions` untouched. Rejected because it leaves the false native claim in
place — the thing most consumers actually read.

---

## R2 — Which existing signal demotes the claim? **`degraded_ecosystems`, and it is the wrong shape for the root.**

**Question**: how does a `complete` claim get demoted today, and does the
closure's partial read fit that path?

**Read from `generate/cyclonedx/compositions.rs::dependency_claims`**: the
helper takes `degraded_ecosystems: &HashSet<String>` and sends an ecosystem's
components to `aggregate: unknown` instead of `complete` when the ecosystem is
degraded or any component is unreachable. m866 established the separation of
`assemblies` from `dependencies` for exactly this reason, after the conflation
asserted graph completeness for components with no edges at all.

But the `complete` record in the moat golden is the **root** claim, which
`dependency_claims` derives from `target_aggregate(integrity)` — trace
integrity — not from the ecosystem path.

**Decision**: route the closure's partial read into the ecosystem demotion for
`nix`, *and* add the partial read as an input that can withhold the root
dependency claim. Both, because they are two different claims: one about the
nix-derived components' graph, one about the root's.

**Rationale**: reusing `dependency_claims` means CDX and SPDX 3 cannot drift,
which is the property m1069 exists to maintain. Adding a second, parallel
demotion path would reintroduce the drift it removed.

**Alternative rejected**: demote only the ecosystem claim. Rejected because
the root record is the one measured as false today.

---

## R3 — New catalog row, or a new reason code on C104/C105?

**Question**: C104 `waybill:graph-completeness` is always emitted with
`complete | partial | unknown`, and C105 carries a reason from a closed
vocabulary. Is a partially-read closure just a new C105 reason code?

**Decision**: a new row for the detail, and C104 is left to its own rule.

**Rationale**: C104 answers "did the BFS from the root seed reach everything",
a question about the *assembled graph*. This milestone's signal answers "did
the reader consult every declaration nix emitted", a question about the
*input*. They can disagree in both directions: a closure can be fully read and
still have an unreachable component, and a partially-read closure can still
produce a fully-connected graph from what it did read. Folding one into the
other would make a single field answer two questions — the stated reason C190
was kept separate from C180, and the reason m1069 split `assemblies` from
`dependencies`.

**Alternative rejected**: extend C105's closed vocabulary. Rejected on the
above; also, C105 is present only when C104 is not `complete`, so a
partially-read closure that still produced a complete graph could not be
disclosed at all.

---

## R4 — What shape should the new annotation take?

**Decision**: a document-scope JSON object, following C200
`waybill:hpack-read-outcome` — counts plus closed reason codes — rather than a
bare string or a delimited one.

**Rationale**: C200 is the closest existing precedent (what a reader saw: seen,
read, unread, each with a reason code), and the pattern has already survived a
shape migration: C190 was a bare string before m1274 and had to be widened to
an object, which cost a compatibility note that is still in the catalog. A
field that will carry counts, a field list and a format version should start as
an object. Keys sort, so the value stays byte-stable.

**Alternative rejected**: a delimited string in the style of C158
(`<mode>;unenriched=<n>`). Rejected because the field list is open-ended — R5
makes its size an output of this milestone — and a delimited string with an
embedded list is the shape that forces a migration later.

---

## R5 — The format version the closure was read from

**Measured**: `RawClosure` deserializes only `derivations`. The `version`
field, which reads `4` on every closure measured here, is parsed and dropped.
That is how the v3 → v4 move landed unnoticed.

**Decision**: capture it and put it in the annotation (spec FR-012), while
resolution stays version-agnostic (FR-013).

**Rationale**: a document that records neither its system nor its output shape
cannot be audited after the fact. The system half of that is already handled
where it matters — the corpus manifest's figures state their system explicitly
(an earlier revision of this document claimed they did not; see
`measurements/findings.md` F0). The **output shape** is recorded nowhere at
all, and it is the half that changed under us: the typed carrier arrived with
format version 4 and nothing in any emitted document says which version it was
read from.

**Not decided here**: whether the scanned system belongs in the document too.
Out of scope, and worth its own look.

---

## R6 — Can the widening change output for non-closure scans?

**Measured**: `structuredAttrs` appears nowhere in `waybill-cli/src/`, and
every read in `closure/` funnels through four accessors on one type —
`paths_in`, `pname`, `declared_pname`, `version`. The type is constructed only
from `nix derivation show` output.

**Decision**: the widening is confined to that type, so no scan without
`--nix-closure` can change. SC-009 asserts it rather than assuming it.

**Rationale**: a change at a shared accessor is the kind that leaks. Confining
it to a type only the closure path constructs bounds the blast radius by
construction, not by review.

**The one file outside that boundary, named rather than left implicit**:
`waybill-cli/src/resolve/closure_role_scope.rs` sits on the general resolution
path, and FR-008 concerns its behaviour. The confinement still holds, for a
reason worth writing down instead of assuming: that pass acts only on
components carrying closure-role data, and such components exist only under
`--nix-closure`. So a scan without the flag reaches it with nothing to act on.
If a task finds itself *changing* that file rather than confirming it, this
argument has been invalidated and SC-009 needs re-checking against every
non-closure target, not just asserted.

---

## R7 — Where the measured figures come from, and what can move them

Every count in the spec traces to `measurements/role-census-probe.py` over
`nix derivation show -r --option allow-import-from-derivation false
<moat>#packages.<system>.default`, the same invocation `closure_argv` builds.
Run 2026-11-04, Determinate Nix 3.20.0, format version 4, nixpkgs `a799d3e3`
via moat's own lock.

The probe's env-only arm agrees with the committed golden on **347 of 348**
shared components' roles. That control does double duty: it validates the
reimplementation, and it is the evidence the blind spot is live in CI — if CI's
nix emitted the flat form for these derivations, CI's roles would match the
widened arm instead.

**What can move the numbers**: a nix release changing which derivations use the
typed form, or a nixpkgs bump. The two invariants, SC-001 and SC-002, are
written to survive both. The counted criteria are dated.

**The loose end closed itself during implementation.** The local x86_64-linux
closure has 1,749 derivations where the manifest's darwin figure says 1,275 —
and the goldens' own `waybill:nix-closure` record says **1,749**, with
256 / 101 / 39 / 1,353, byte-for-byte the env-only arm. So the two systems were
never in conflict; they are two different closures, both correctly recorded.
The remaining per-component residue (`docbook-xml` disagreeing, 9 golden names
absent locally) is attributed to nix and nixpkgs drift since the goldens were
cut, and is not run down, because every criterion in the spec is either an
invariant or re-measured against the refreshed goldens.
