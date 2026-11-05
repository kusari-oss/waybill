# Phase 0 Research: 1280-nix-closure-edges

Five findings. Two changed the design, one changed a success criterion, and
one is a correction to this feature's own spec. Every figure is measured on a
named closure and a named system; `measurements/README.md` holds the runs.

---

## R1 — The identity pass rewrites PURLs after the emitter returns. **Measured.**

**This is the finding the plan is built around.**

`emit::components` returns `Vec<ResolvedComponent>` with `pkg:generic/<name>@<version>`
PURLs, because `pkg:nix` is not a purl-spec type. The milestone-1275 identity
pass then runs — `scan_cmd.rs` calls `closure_identity::reclassify` after
`merge_closure_components` — and `closure_identity.rs:652` does `c.purl = p`,
promoting a confirmed member to `pkg:hackage/<name>@<version>`.

Measured on the pinned x86_64-linux golden:

| C201 outcome | components |
|---|---|
| `confirmed` → PURL rewritten | **104** |
| `not-in-package-set` | 213 |
| `version-disagrees` | 27 |
| `no-version-recorded` | 5 |

Closure components by PURL type in the shipped golden: `pkg:hackage` 136,
`pkg:generic` 245.

`Relationship` carries `from` and `to` as **PURL strings**. So an edge built
inside `emit::components` would name `pkg:generic/QuickCheck@2.15.0.1` for a
component that ends up as `pkg:hackage/QuickCheck@2.15.0.1` — a dangling
endpoint for **27% of the graph**.

**Decision**: build edges after the identity pass. Until then the projection is
keyed on `.drv` paths, which nothing rewrites, and PURLs are resolved from the
final component set in one step.

**Why it would have shipped broken**: no fixture runs the identity pass. It
needs a pinned nixpkgs package set, which the integration suites deliberately
stub out as empty. Every unit and integration test would have passed, and the
corpus goldens would have been the first thing to notice — in CI, on a nightly,
after the fact.

**Alternatives considered**: rewriting edges inside the identity pass alongside
the components (rejected — two passes that must agree about identity is the
drift this project keeps paying for); keying `Relationship` on something other
than a PURL (rejected — it is the pipeline's join key everywhere else, and
changing it is a far larger blast radius than sequencing one call).

---

## R2 — The projection must be a memoized post-order pass, not a walk per component. **Reasoned, bounded by measurement.**

FR-012 asks for linearity. The naive implementation walks from each emitted
component to find its projected dependencies: 381 walks over a 10,342-edge
graph on linux, 388 over 8,230 on darwin.

**Decision**: one post-order pass over the derivation DAG, memoizing per
derivation the set of **nearest package-or-emitted descendants** reachable
through non-package derivations only. A node's set is the union over its inputs
of (that input, if it stops the walk) or (that input's memoized set, if it does
not). Each derivation is visited once; each edge contributes one union.

Measured bound on the set sizes, which is what makes the unions cheap: the
maximum out-degree of the projected graph is **41** on linux and **47** on
darwin, against 381 and 388 components. The sets are small and the union cost
is not hiding a quadratic.

**Alternatives considered**: computing the full transitive closure and
filtering (rejected — that is projection (b), 16,187 edges, and FR-010 chose
otherwise on accuracy grounds); caching per-walk visited sets across walks
(rejected — a memoized post-order pass is the same idea done correctly, and
the incorrect version is where an off-by-one in the stopping rule hides).

---

## R3 — FR-011 needs one new orphan reason code, and the vocabulary is frozen. **Read from the contract.**

`OrphanReasonCode` is an eight-variant enum at
`waybill-cli/src/generate/orphan_reason.rs:194`, and its `as_str` carries this
comment: *"These literal strings are frozen — SBOM consumers key on them."*
The existing codes are `stale-go-sum-entry`, `dead-lockfile-entry`,
`hoisted-unused`, `unresolved-indirect-require`, `flat-attached-fallback`,
`build-toolchain`, `foreign-build-system`, `unreferenced-lock-node`.

None fits. `build-toolchain` is the nearest and is wrong: it says *this is
tooling, not a dependency of the subject*, whereas FR-011's population is a
genuine dependency whose path runs through a package waybill did not emit.

**Decision**: add one variant, wire value
`reachable-only-through-unemitted-package`, with its own row in
`specs/1218-explain-every-orphan/contracts/orphan-reason-vocabulary.md`-style
contract at `contracts/orphan-reason.md`, including the populations it MUST NOT
appear on.

Measured population, and it is uniform — every residual orphan has this one
cause:

| system | residual orphans | consumed only by |
|---|---|---|
| x86_64-linux | 5 — `diffutils 3.12`, `ed 1.22.5`, `gnumake 4.4.1`, `gnutar 1.35`, `pyproject-hooks 1.2.0` | `stdenv-linux`, `patch 2.8`, `build 1.4.4` |
| aarch64-darwin | 16 | `stdenv-darwin`, `clang-wrapper`, `macOS-SDK`, `bmake`, others |

**Alternatives considered**: reusing `build-toolchain` (rejected — it would
make the code mean two things, which is the defect milestone 1279 just spent
itself fixing for `unreferenced`); emitting no reason (rejected — FR-011, and
it would leave the five indistinguishable from "nothing depends on this", the
exact conflation #1346 is about).

---

## R4 — The closure already contributes relationships, just not dependency edges. **Measured; corrected this spec.**

The spec's first draft said the closure contributes zero relationships. The
shipped SPDX 2.3 golden's relationship census:

| type | count |
|---|---|
| `DEPENDS_ON` | 752 |
| `BUILD_TOOL_OF` | **103** |
| `DEV_DEPENDENCY_OF` | 36 |
| `DESCRIBES` | 1 |
| `BUILD_DEPENDENCY_OF` | 1 |

The 103 are milestone 1277's, one per component classified as absent from the
artifact, emitted `member BUILD_TOOL_OF root` at
`generate/spdx/relationships.rs:481`. CycloneDX has no equivalent and carries
none; the CDX document's 789 `dependsOn` entries are all manifest-derived.

Two consequences:

1. The zero is specifically **dependency** edges, and the spec now says so.
2. `BUILD_TOOL_OF` points *member → root*, which is the wrong direction to
   make anything reachable from the root — which is why those 103 edges have
   not prevented a single one of the 352 orphans. SC-010 pins the count so
   this feature cannot disturb them.

**Decision**: leave them alone (FR-007b). They answer "is this in the
artifact"; these answer "what consumed what".

---

## R5 — The closure root is already a component, spelled by another reader. **Measured; drove FR-004a.**

The closure's root derivation is `moat-0.1`, and nothing else consumes it. It
is **not** emitted by the closure tier: no declared attribute references it, so
its role is `unreferenced`, and it applies no patches.

But it is in the document — as `pkg:hackage/moat@0.1`, emitted by the Haskell
manifest reader, and the document root `nix-closure-moat@d069055` already
depends on it. The closure tier would have spelled the same thing
`pkg:generic/moat@0.1`.

**Decision**: anchor the projection on the **document root component**, and do
not attempt to match the closure root against an existing component. The match
would have to cross a PURL-type boundary, and a wrong match silently reparents
the entire graph. The document root is always present and needs no match —
including for `--nix-closure-attr` targets that are system configurations,
where no package component corresponds to the root at all.

**Alternatives considered**: emitting the closure root as a component
(rejected — a second `moat` beside the manifest's is the divergent-PURL defect
of #1216 and m134); matching on `(name, version)` ignoring PURL type
(rejected — it is a heuristic whose failure mode is silent and total).

---

## Open, and deliberately not resolved here

**Whether `inputs.drvs` is present in every format version the tier may meet.**
Measured present in format version 4 on both closures. The reader does not gate
on the version — milestone 1278 established that a version list is an allowlist
too — so the handling is structural: a derivation with no `inputs.drvs` key
contributes no edges, and a closure where none has it emits none and records
zero in C184, which FR-008 makes distinguishable from a closure never read.
That is a requirement, not an unknown.
