# Feature Specification: Nix derivation closure as SBOM content

**Feature Branch**: `1035-nix-closure-sbom`
**Created**: 2026-09-29
**Status**: Draft
**Issues**: **#1034** (how deep to follow a Nix closure) and **#1040** (vulnerability
signals nixpkgs already carries). The directory number is the next sequential
milestone number, not an issue reference.

## Why this exists

Milestone 1034 shipped `--nix-eval`, which asks Nix for package versions instead
of reconstructing them from nixpkgs files. It emits **53** and **190** hackage
components for two real Haskell libraries.

The derivation closure for those same projects holds **1,275** and **1,535**
derivations. Most of what a Nix build actually consumes is absent from the
document.

Two things live in that gap, and they are different in kind.

**Components waybill cannot currently see.** C libraries, toolchain, and the
transitive build inputs that no manifest mentions. A Nix-built artifact depends
on them; the SBOM does not say so.

**Evidence that a vulnerability was already fixed.** nixpkgs backports security
patches without moving the version string. `unzip` in nixpkgs carries CVE-named
patches against a version unchanged since 2009. Nothing in an SBOM keyed on
version strings can express that, in either direction — neither "this build is
patched" nor "this version is affected".

## Measurements this feature is built on

All taken 2026-09-28/29. The classifier is committed at
`specs/1034-nix-eval-tier/measurements/classify-derivation-closure.py` and
reproduces every figure below.

| | moat | slack-web |
|---|---|---|
| derivations in closure | 1,275 | 1,535 |
| — artifact input | 264 | 390 |
| — build tooling only | 134 | 146 |
| — both | 52 | 55 |
| — neither | 825 | 944 |
| of "neither": patch derivations | 43 | 50 |
| **distinct CVEs, via `env.patches`** | **18** | **14** |
| components carrying them | 4 | 3 |
| hackage components waybill emits today | 53 | 190 |
| closure query cost (warm) | 1.09 s / 5.7 MB | 1.07 s / 7.4 MB |

**Scanning derivation names undercounts fivefold.** That approach finds 3 and
4 CVEs; joining through each derivation's own `patches` field finds 18 and 14,
because many patches are files referenced by store path rather than separate
CVE-named derivations. `unzip 6.0` alone carries **11 CVEs across 26 patches**
in both closures — the case #1040 was filed on, confirmed rather than argued.

The components carrying them are `unzip` (tooling), `libssh2` (artifact),
`perl` (both), `jq` and `lua` (neither) — every role, which is why A-4 is
narrow and why build tooling is emitted.

### How much of this is actually new

Measured 2026-09-29, comparing closure artifact inputs against the components
waybill emits today for the same project:

| | moat | slack-web |
|---|---|---|
| closure artifact inputs | 249 | 378 |
| hackage components emitted today | 53 | 190 |
| overlap | 33 | 160 |
| **in closure, not emitted** | **216** | **218** |
| emitted, not in closure | 20 | 30 |

So the closure is roughly **4×** moat's current component count and **2×**
slack-web's, and the gain is genuinely new content rather than the same
components counted differently. Examples of what is missing today:
`ChasingBottoms`, `Diff`, `OneTuple`, `adjunctions`, `aeson`, `autoconf`,
`automake`.

**The reverse direction is a finding of its own.** 20 and 30 components waybill
emits today do *not* appear in the closure of `.#default`. Enumerated, they are
mostly explained:

- **GHC boot libraries** — `base`, `bytestring`, `containers`, `text`,
  `template-haskell`, `ghc-prim`, `integer-gmp` and the rest. These ship inside
  the compiler derivation rather than as closure members of their own, so their
  absence is an artefact of where nix puts them, not a gap. 18 of moat's 20 and
  roughly 22 of slack-web's 30.
- **The scanned project's own main modules** — `moat` and `readme`. Expected.
- **Declared by a stanza nix does not build — 8 on slack-web.** `butcher` and
  `monad-loops` are declared by the `executable slack-web-cli` stanza
  (`slack-web.cabal:243`); the rest are reached transitively from them.
  `packages.<system>.default` builds the library, so an executable's
  dependencies never enter its closure.

**This settles replace-versus-supplement: supplement.** The two sets answer
different questions — the manifest set covers every cabal stanza, the closure
covers only what the chosen attribute builds. Neither is wrong, and a document
replacing one with the other loses real content whichever way it chooses. See
research §R1; the original hypothesis, that these belonged to another flake
attribute, was measured and disproved.

**The build-versus-artifact split needs no heuristic.** Nix records it:
`nativeBuildInputs` is host tooling, `buildInputs` is what goes into the
artifact. The 134/146 build-tooling-only derivations are identifiable from the
closure itself.

## Clarifications

### Session 2026-09-29

- Q: Build-tooling-only derivations — drop them, or emit them marked? → A: Emit them, marked as build-tooling scope, so consumers can filter rather than lose them.
- Q: Which flake attribute does the closure come from when several exist? → A: `packages.<system>.default` only, with an operator flag to name a different one; degrade if `default` is absent.
- Q: What VEX status does a backported CVE patch produce? → A: Two graded statements — `affected` for the version, `not_affected` for this specific build.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The SBOM lists what the build actually consumed (Priority: P1)

An operator scanning a Nix-built project gets components for the artifact's real
inputs — including the C libraries and transitive dependencies no manifest
mentions — rather than only what the language ecosystem's manifests declare.

**Why this priority**: It is the feature. Without it the other stories have no
document to attach to.

**Independent Test**: Scan a Nix-built project with the closure enabled and
assert the component count exceeds the manifest-derived count, with the added
components traceable to derivations classified as artifact inputs.

**Acceptance Scenarios**:

1. **Given** a project whose closure contains artifact inputs absent from its
   manifests, **When** the closure is emitted, **Then** those appear as
   components carrying their derivation provenance.
2. **Given** the same project, **When** the closure is not enabled, **Then**
   the document is byte-identical to milestone 1034's output.
3. **Given** a derivation nix classifies as build tooling only, **When** the
   closure is emitted, **Then** it appears carrying a build-tooling scope
   marker, distinguishable from artifact inputs by that marker alone.

---

### User Story 2 - Backported security patches are visible (Priority: P1)

A consumer can see that a component carries a backported fix for a named CVE,
in a field their tooling already understands.

**Why this priority**: Equal to US1, and independently valuable — it is the half
of #1040 that needs no vulnerability database, only what nixpkgs already
records. A version string cannot express it.

**Independent Test**: Scan a project whose closure contains a CVE-named patch
and assert the emitting component carries it in CycloneDX
`pedigree.patches[]` with `type: backport` and a `resolves[]` entry of
`type: security`.

**Acceptance Scenarios**:

1. **Given** a closure containing `CVE-2019-13232-1.patch` applied to a
   component, **When** the document is emitted, **Then** that component's
   `pedigree.patches[]` records a `backport` resolving a security issue with
   that CVE id.
2. **Given** a patch derivation whose name carries no CVE, **When** the document
   is emitted, **Then** the patch is still recorded, without a `resolves[]`
   security entry.
3. **Given** SPDX 2.3 or SPDX 3 output, **When** the document is emitted,
   **Then** the same facts are carried, since neither format has a native
   equivalent.

---

### User Story 3 - The patch evidence says how much to trust it (Priority: P1)

A consumer can tell a CVE id that came from a filename from one established some
stronger way, and tooling can decide accordingly.

**Why this priority**: P1 because US2 is *unsafe* without it. The CVE is parsed
out of a filename like `CVE-2019-13232-1.patch`. Backports that do not name a
CVE are invisible, and silently so; a filename is not proof the patch fully
resolves the issue. waybill's existing VEX emits only `under_investigation`,
its source comment calling that "the status waybill can honestly emit today".
Asserting `not_affected` on filename evidence without grading it would be a
larger accuracy claim than the evidence supports (Constitution Principle IX).

**Independent Test**: Assert every emitted patch-derived CVE association carries
an evidence grade, and that a grade exists distinguishing filename-derived from
any stronger provenance.

**Acceptance Scenarios**:

1. **Given** a CVE parsed from a patch filename, **When** it is emitted,
   **Then** it carries an evidence grade identifying it as filename-derived.
2. **Given** a closure whose patch set is partially CVE-named, **When** the
   document is emitted, **Then** the count of patches *without* a CVE is
   recorded, so partial coverage is visible rather than inferred from absence.

---

### User Story 4 - A backport yields both claims, separately (Priority: P2)

A consumer learns that a component's version was considered vulnerable to a CVE,
not merely that this build is patched against it.

**Why this priority**: P2 — US2 delivers value without it — but it may be the
more useful half. nixpkgs applied the patch because it believed the package was
vulnerable, and the version string did not move, so nothing else in the document
says so. It tells a consumer that a version-range match they would otherwise
dismiss is real, and flags the *unpatched* build of the same version as
affected.

**Independent Test**: Assert a component carrying a backport for a CVE also
yields a positive affectedness signal for the unpatched version, distinct from
the `not_affected` claim about this build.

**Acceptance Scenarios**:

1. **Given** a component carrying a backport for a CVE, **When** VEX is
   emitted, **Then** two statements appear — `affected` subject to the version
   and `not_affected` subject to this build — each carrying its evidence grade.
2. **Given** the same component, **When** a consumer reads only the
   `not_affected` statement, **Then** its subject makes clear it covers this
   build and not the version generally.

---

### User Story 5 - The operator knows the risk changed (Priority: P1)

An operator enabling this sees that it evaluates the scanned project's own
flake, which milestone 1034 deliberately does not.

**Why this priority**: P1 because the guidance is currently written as a margin
of safety, and this makes it literal. Milestone 1034's tier evaluates nixpkgs at
a pinned revision; repository-authored expressions never run. Taking a closure
requires instantiating the project's flake, so they do.

**Independent Test**: Assert the flag's help and
`docs/reference/nix-evaluation.md` state that repository-authored expressions
are evaluated, and that the existing guard refusing `--accept-flake-config` and
`--impure` is in force on this path.

**Acceptance Scenarios**:

1. **Given** the closure path, **When** it invokes nix, **Then** the argv guard
   from PR #1044 applies unchanged, so the scanned flake cannot select nix's
   evaluation settings.
2. **Given** the docs, **When** an operator reads them, **Then** the statement
   that the project's own flake is not evaluated is corrected for this path.

---

### Edge Cases

- **No `packages.<system>` attribute.** Measured: haskell-language-server
  exposes only `docs` and devShells. moat exposes
  `default, moat-ghc910, moat-ghc94, moat-ghc96`; slack-web exposes
  `default, slack-web`. Degradation, not error.
- **Several package attributes.** moat's four are the same library against
  different GHC versions. Settled: take `default`, let the operator name
  another (FR-015a). A flake with attributes but no `default` degrades and
  lists them (FR-015b).
- **`builtins.getFlake` on a local path requires `--impure`.** Measured:
  `error: cannot call 'getFlake' on unlocked flake reference … (use --impure to
  override)`. The safety guard forbids `--impure`, so the project flake MUST be
  addressed through the CLI flakeref form, which works without it.
- **A patch applying to several components**, or several patches to one.
- **A CVE-named patch nixpkgs applies to a component waybill does not emit.**
- **Closure output size.** 5.7–7.4 MB of JSON per query.
- **`--offline`.** Inherits milestone 1034's refusal for the same reason.

## Requirements *(mandatory)*

### Functional Requirements

**Scope and shape**

- **FR-001**: Closure emission MUST be opt-in and MUST NOT change output when
  not requested.
- **FR-002**: Derivations nix classifies as build-tooling-only MUST be emitted,
  carrying a scope marker distinguishing them from artifact inputs. The
  classification MUST come from nix's own `nativeBuildInputs` / `buildInputs`
  rather than name heuristics.
  *A consumer can filter a marked component down; it cannot recover a dropped
  one. The distinction is free because nix already records it, and a compiler
  that built the artifact is within scope for a CISA Build-type SBOM.*
- **FR-003**: Derivations nix classifies as artifact inputs MUST be emitted as
  components carrying their derivation provenance.
- **FR-003a**: Closure-derived components MUST supplement the manifest-derived
  set, never replace it, and the two MUST remain distinguishable.
  *Measured: the manifest set covers every cabal stanza while the closure
  covers only what the selected attribute builds — slack-web's `butcher` and
  `monad-loops` come from its executable stanza and are absent from the
  library's closure. GHC boot libraries are likewise absent by construction,
  shipping inside the compiler derivation. Treating closure-absence as evidence
  a component is spurious would discard the Haskell standard distribution.*
- **FR-004**: Patch derivations MUST NOT be emitted as components. They describe
  a modification to a component, not a component.
- **FR-004a**: A closure member that **applies** a patch MUST be emitted
  whatever its role, including `Unreferenced`.
  *Distinct from FR-004, which is about the patch file. Measured (research
  R8): the components carrying CVEs span every role — `unzip` is tooling,
  `libssh2` an artifact input, `perl` both, `jq` and `lua` neither. Scoping any
  role out would drop part of the evidence; `jq` and `lua` alone carry 6 of
  moat's 18 CVEs.*

**Patches**

- **FR-005**: A patch applied to an emitted component MUST be recorded in that
  component's CycloneDX `pedigree.patches[]` with `type: "backport"`.
  Verified against `bom-1.6.schema.json`: the `patch.type` enum is
  `['unofficial','monkey','backport','cherry-pick']`.
- **FR-006**: A patch whose name carries a CVE identifier MUST record it in
  `resolves[]` as an issue of `type: "security"` with that `id`. Verified: the
  `issue.type` enum is `['defect','enhancement','security']` and `issue`
  carries `id`, `source` and `references`.
- **FR-007**: SPDX 2.3 and SPDX 3 MUST carry the same facts. Neither has a
  native equivalent, so both need the annotation bridge.
- **FR-008**: waybill uses `pedigree` nowhere today; this MUST use the native
  field rather than a `waybill:` property (Constitution Principle V).

**Evidence**

- **FR-009**: Every CVE association derived from a patch filename MUST carry an
  evidence grade identifying it as such.
- **FR-010**: The number of patches carrying no CVE identifier MUST be recorded
  at document scope, so partial coverage is visible rather than inferred.
- **FR-011**: A backport MUST produce **two** VEX statements, not one:
  `affected` for the component version, and `not_affected` for the specific
  build this document describes. Both MUST carry the evidence grade from
  FR-009.
  *The two claims have different evidential strength. That nixpkgs applied a
  CVE-named patch is strong evidence someone believed the version vulnerable;
  that the patch fully resolves the issue is weaker, resting on a filename. A
  single `not_affected` would let a consumer suppress a real finding on the
  weaker half.*
- **FR-012**: The two statements MUST be distinguishable by their subject — the
  version versus this build — so a consumer cannot collapse them into one
  claim.
- **FR-012a**: Neither statement MUST be emitted without its evidence grade.
  An ungraded `not_affected` derived from a filename is precisely the claim
  FR-009 exists to prevent.

**Invocation**

- **FR-013**: The project flake MUST be addressed through the CLI flakeref form.
  `builtins.getFlake` on a local path requires `--impure`, which FR-014 forbids.
- **FR-014**: The argv guard from PR #1044 MUST apply on this path: no
  `--accept-flake-config`, no `--impure`.
- **FR-015**: A flake exposing no usable attribute MUST degrade with a reason
  code, not error.
- **FR-015a**: The closure MUST be taken from `packages.<system>.default`, and
  the operator MUST be able to name a different attribute.
  *Merging several attributes would put three GHC toolchains and three copies
  of every dependency into one document, describing a build nobody performed.
  moat exposes `default`, `moat-ghc910`, `moat-ghc94`, `moat-ghc96` — the same
  library against three compilers.*
- **FR-015b**: A flake exposing `packages.<system>` but no `default` MUST
  degrade with a reason code naming the attributes that *are* available, so the
  operator can pick one rather than guess.
- **FR-016**: The closure query MUST be bounded in wall-clock time, and MUST
  inherit milestone 1034's `--offline` refusal.

**Transparency**

- **FR-017**: The flag's help and `docs/reference/nix-evaluation.md` MUST state
  that this path evaluates repository-authored expressions, correcting the
  statement that the project's own flake is not evaluated.
- **FR-018**: The document MUST record how many closure members were emitted,
  suppressed as tooling, and recorded as patches.

### Key Entities

- **Closure member**: one derivation, with a name, a version where it has one,
  and how nix referenced it (tooling, artifact input, both, neither).
- **Patch record**: a modification applied to a component, optionally naming a
  CVE, with the grade of that identification.
- **Affectedness signal**: the inverse reading of a backport — that the version
  was considered vulnerable — distinct from a status claim about this build.

## Success Criteria *(mandatory)*

- **SC-001**: On both measured projects, closure emission adds at least 200
  components not currently emitted, and every added component traces to a
  derivation classified as an artifact input. Baseline measured: 216 and 218
  such components exist.
- **SC-002**: Every derivation classified as build-tooling-only is emitted with
  a scope marker, and none is marked as an artifact input. On the measured
  projects that is 134 and 146 components.
- **SC-003**: With the feature off, all committed corpus goldens are unchanged.
- **SC-004**: `CVE-2019-13232` appears in `pedigree.patches[].resolves[]` on
  **`unzip`**, the component that applies it, for both measured projects, and
  the emitted document validates against the CycloneDX 1.6 schema.
- **SC-005**: Every patch-derived CVE association carries an evidence grade, and
  a test asserts an ungraded one cannot be emitted.
- **SC-005a**: For `CVE-2019-13232` on a measured project, both statements are
  emitted — `affected` subject to the version, `not_affected` subject to this
  build — and a test asserts neither appears without the other.
- **SC-006**: The count of patches without a CVE is emitted at document scope,
  so partial coverage is visible rather than inferred from absence.
- **SC-006a**: At least 18 and 14 distinct CVEs are recovered from the measured
  projects — the counts reachable through `env.patches`. A run recovering only
  3 and 4 indicates the implementation is scanning derivation names instead of
  the join, which is a silent fivefold undercount.
- **SC-006b**: The emitted patch total and no-CVE count match the closure. On
  slack-web that is 320 patches of which 279 name no CVE — so roughly 87% of
  backports carry no CVE in their filename, and a consumer reading only VEX
  statements sees a minority of the patching that occurred. This is the figure
  that makes the coverage limit legible.
- **SC-007**: The argv guard holds on this path, demonstrated by the same
  mutation that proves it on the 1034 path.
- **SC-008**: A flake exposing no `packages.<system>` degrades with a reason
  code and a successful scan — measured case: haskell-language-server.
- **SC-009**: On moat, the closure is taken from `default` alone; the document
  contains one GHC toolchain, not three, and naming `moat-ghc96` instead
  produces a different document.

## Assumptions

- **A-1**: CycloneDX is the primary target for patch data, because it is the
  only format with a native carrier. SPDX gets the bridge.
- **A-2**: Opt-in, separate from `--nix-eval`. An operator may want evaluated
  versions without a 1,500-derivation closure.
- **A-3**: Haskell projects are the measured case. The closure mechanism is
  language-agnostic; only the reader integration is not.
- **A-4**: Of the `neither` bucket, **fetched sources, setup hooks and
  bootstrap toolchain** are out of scope for v1 — nothing measured argues they
  belong in a document.
  *Narrowed 2026-09-29 by research R8. The original assumption excluded the
  bucket wholesale, which would have discarded a third of the CVE evidence:
  `jq` and `lua` are classified `neither` and carry 6 of moat's 18 CVEs. A
  closure member that applies a CVE-named patch is in scope whatever its
  role.*

## Out of Scope

- Making closure emission, or `--nix-eval`, the default.
- A vulnerability database. This emits what nixpkgs already records.
- Resolving the CISA/VEX question of what a backport means for `affected`
  status generally; this feature emits the evidence and grades it.
- Fetched sources, setup hooks and bootstrap toolchain within the `neither`
  bucket — **except** any member that applies a patch, which is in scope
  regardless of role (A-4, research R8).

## Research constraint *(binds the planning phase)*

Milestone 1034's history is the reason this section exists. Three claims about
Nix reached artifacts there without a probe behind them, and all three were
wrong:

- m926's §R1 cited Constitution Principle I to avoid invoking `nix` at all.
- m143's §R7 declared `cabal.project` a presence-only signal.
- m1034's own spec claimed the tier evaluates repository-authored expressions.
  It does not — caught only by mutation-testing the acceptance test that claim
  justified.

The lesson recorded there is narrower than "measure": *a measurement of what a
tool does is not a measurement of what our use of it does.*

Planning MUST cite the measurements in this document or take new ones, and MUST
NOT assert new claims about nix behaviour without a committed probe.

**Not yet measured, and load-bearing:**

- Mechanically attributing a patch derivation to the component it patches. The
  closure records both; the join has not been demonstrated (research T-R1).
- Which component a patch derivation attaches to, mechanically. The closure
  records the patch; the attribution has not been demonstrated.
- Whether closure composition holds outside Haskell.
- The cost on a cold Nix store.
