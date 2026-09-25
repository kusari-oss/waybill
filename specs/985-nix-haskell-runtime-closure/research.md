# Phase 0 Research — milestone 985 (issue #962)

Every unknown the plan's Technical Context raised, resolved. The numbers behind
the spec are separate, in `measurements/`; this file records **decisions**.

---

## R1 — Where the dependency relations come from

**Decision**: parse `libraryHaskellDepends` and `executableHaskellDepends` out
of each attribute's body in `hackage-packages.nix`, in the same pass that
already extracts `version` and `sha256`.

**Rationale**: the data is in the file milestone 926 already downloads and
caches per revision, so FR-003 (no additional retrieval) is satisfied by
construction rather than by discipline. Verified present at scale: across
19,058 derivations at one measured revision, `libraryHaskellDepends` occurs
17,824 times and `executableHaskellDepends` 4,197.

**Cross-validated**: parsing these two fields reproduces what `nix eval`
reports for `propagatedBuildInputs` — 167 components on one project, exactly.
This is the external oracle that licenses SC-002, and it is the reason the
runtime closure was scoped in and test/benchmark scoped out (issue #985): only
this set has something outside waybill to check against.

**Alternatives considered**:

- *Shell out to `nix eval`.* Rejected. It is the oracle, not the mechanism: it
  requires a host `nix`, which Principle I's self-contained-binary posture
  argues against as a runtime requirement, and it would break FR-003's offline
  guarantee. Retained in `measurements/` precisely because a probe that shares
  code with its subject cannot disagree with it.
- *Fetch a precomputed dependency index from somewhere.* Rejected: new network,
  new trust root, and no such artifact is published per-revision.

---

## R2 — Keying, and why it is not `pname`

**Decision**: key the dependency-relation index on the **attribute** name, the
same key `package_set.rs` uses post-#970.

**Rationale**: 371 attributes at one measured revision share a `pname`
(19,429 attributes → 19,058 distinct pnames). Keying on `pname` returns a
version the build never uses — the defect #970 fixed in the production parser.
Dependency lists reference **attribute** names (`base`, `os-string`), so the
index must be attribute-keyed or lookups will miss or mis-hit.

Note for whoever implements: the older probe at
`specs/926-nixpkgs-haskell-versions/measurements/` still keys on `pname` and
its closure figures are not trustworthy. `measurements/closure_probe.py` in
this milestone is attribute-keyed and is the one to extend.

---

## R3 — Native carrier for "declared vs transitive" (Principle V gate)

**Decision**: no native carrier exists in any of the three formats; emit a
`waybill:` annotation, and record the audit that justifies it.

**Rationale** — audited each format rather than assumed:

| format | candidate | verdict |
|---|---|---|
| CycloneDX 1.6 | no `direct`/`transitive` field on `component`; `dependencies[]` expresses the graph, not each node's distance from the root | **none** — and graph position is unreliable here anyway (see below) |
| SPDX 2.3 | relationship types are about *kind* (`DEPENDS_ON`, `DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`, `OPTIONAL_DEPENDENCY_OF`), not about distance | **none** |
| SPDX 3 | same — `Relationship` carries type and completeness, not depth | **none** |

Confirmed by inspection: waybill's CycloneDX emitter uses no direct/transitive
field today (`grep` for `isDirect`/`dependencyType` returns nothing), and the
SPDX relationship enum carries exactly the four kind-based variants above.

Deriving the distinction from graph position was rejected for a concrete reason
rather than a stylistic one, per the spec's clarification: CycloneDX's
primary-dependency fallback (milestone 894) synthesizes a root edge to *every*
unreferenced component when the root has no declared outgoing edges. Under that
fallback every closure member reads as declared. The fallback is deliberate and
documented; the inference is what breaks.

**Principle V is satisfied** by having done the audit and recorded the absence,
which is what the principle asks — native first, annotation when there is no
native slot.

---

## R4 — Cycle termination

**Decision**: breadth-first walk with a `seen` set checked **before** enqueue.

**Rationale**: Hackage package sets contain mutually recursive relations. The
probe terminated on every run across three projects and seven GHC series, but
FR-010 states termination as a requirement rather than resting on that
observation, because it must hold for package sets nobody has measured.

**Alternatives considered**: depth-limited walk — rejected, a depth cap either
truncates a legitimate closure or is set so high it is not a bound at all, and
either way it makes output depend on an arbitrary constant.

---

## R5 — Boot libraries inside the closure

**Decision**: do not traverse into a boot library; classify it exactly as the
declared path does.

**Rationale**: a compiler-supplied package's dependencies are a property of the
compiler, not of the nixpkgs entry, so walking its nixpkgs dependency list
would attribute relations the build does not have. The existing
union-across-candidate-compilers rule (milestone 926 FR-014a) decides what
counts as boot and is reused unchanged.

**Measured consequence**: this is the only thing that varies by GHC series. The
closure's *shape* is identical across series (425 members on the largest
project at ghc9.6, 9.10 and 9.14); only the *resolvable* count moves, by 1–2,
from differing nulled sets.

---

## R6 — Aliased names inside the closure

**Decision**: resolve through the compiler-configuration alias map, which is
issue #984's work, and emit a versionless component with a reason when a name
still does not resolve.

**Rationale**: dependency lists reference bare names, and some bare names exist
only via `name = doDistribute self.name_X_Y_Z;` in the configuration file. This
is how the gap was found: the file-parsing closure and `nix eval` disagreed on
exactly one name, `os-string`, on one project (32 vs 33).

**Dependency note**: if #984 has not landed when this is implemented, such names
surface as unresolved-with-reason rather than as silent holes — FR-005a already
requires that, so the feature is correct either way. The measured cost of not
having it is 1–3 unresolvable names per project.

---

## R7 — Emitting the edges

**Decision**: emit an edge from each package to each of its resolved runtime
dependencies, not from the root to every closure member.

**Rationale**: FR-009 requires edges to reflect the actual relation, and
invariant I2 (enforced per-PR by `document_integrity.rs` and nightly by the
corpus layer 0 since #981/#982) requires every endpoint to name a component in
the document. Root-to-everything would satisfy I2 while making the graph a lie
about structure — and would be indistinguishable from the m894 fallback it
would accidentally imitate.

**Risk to carry into tasks**: milestone 980 was exactly this class of failure —
version resolution rewrote component identities without rewriting edge
endpoints, disconnecting every component it resolved. The closure multiplies
both component count and edge count, so the same mistake would be larger. The
`apply_renames` helper added in #981 already exists for the identity-rewrite
case and must be applied to closure-added components too.

---

## R8 — Opt-out mechanism

**Decision**: a dedicated flag that disables the closure while leaving
milestone 926's declared-dependency resolution active (FR-016), with FR-017
requiring byte-identical pre-feature output when set.

**Rationale**: the two are separately valuable — an operator who wants versions
on declared dependencies but not a 3.8× document should not have to give up
both. The precedent is `--no-nixpkgs-haskell`, which disables the whole pass;
this is the narrower sibling.

**Verification hook**: SC-009 checks the disabled path against the committed
corpus goldens for the existing Haskell target *before* they are regenerated,
which is the only moment that comparison is available.
