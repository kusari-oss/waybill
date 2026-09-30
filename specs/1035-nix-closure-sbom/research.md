# Phase 0 Research: Nix derivation closure as SBOM content

**Feature**: `1035-nix-closure-sbom` (issues **#1034**, **#1040**)
**Date**: 2026-09-29
**Host for all probes**: Determinate Nix 3.20.0 (nix 2.34.6), `aarch64-darwin`

## How to read this

The spec binds this phase: cite measurements already taken, or take new ones,
and assert nothing about nix behaviour without a committed probe. Milestone
1034 acquired three wrong claims by doing otherwise.

Labels: **MEASURED** (probe shown), **REASONED** (drawn from measured facts),
**UNMEASURED** (flagged, with what would settle it).

Two findings below **overturn** what the spec assumed. They are marked ⚠.

---

## R1 ⚠ — Closure emission must SUPPLEMENT, not replace

**Decision**: Closure-derived components are added alongside the
manifest-derived set. Neither supersedes the other.

The spec recorded 20 and 30 components that waybill emits but the closure does
not contain, with a hypothesis that they belong to another flake attribute.
**The hypothesis is wrong.**

**MEASURED** — slack-web exposes two package attributes, and neither closure
contains them:

```
$ nix eval --apply builtins.attrNames '.#packages.aarch64-darwin'
["default","slack-web"]

  attribute    derivations   butcher/deque/microlens/multistate/strict-list/unsafe present?
  default           1535     []
  slack-web         1535     []
```

Both attributes yield 1,535 derivations — they are the same derivation.

**MEASURED** — where those packages actually come from:

```
  butcher        version=1.3.3.2    origin=declared
  monad-loops    version=0.4.3      origin=declared
  deque          version=0.4.4.2    origin=transitive
  microlens      version=0.4.14.0   origin=transitive
  …
```

and in `slack-web.cabal`:

```
117: library
184: test-suite tests
243: executable slack-web-cli
254:     , butcher
256:     , monad-loops
```

`butcher` and `monad-loops` are declared by the **executable** stanza. The
transitive ones are reached from them through the milestone-985 relations walk.

**REASONED, and this is the design consequence**: the two sets answer different
questions.

| | covers |
|---|---|
| manifest-derived set | every cabal stanza — library, test-suite, executable |
| closure of `packages.<system>.default` | only what that attribute builds, here the library |

Neither is wrong. A document that replaced one with the other would lose real
content in whichever direction it chose. ⚠ The spec's open question is
therefore settled as **supplement**, and the answer came from measurement
rather than preference.

**Alternatives considered**: (a) replace, rejected — drops the executable and
test dependencies a project genuinely declares; (b) emit only the closure when
it is available, rejected for the same reason; (c) reconcile into one set and
drop the distinction, rejected — the distinction is the information.

---

## R2 ⚠ — GHC boot libraries are absent from the closure by construction

**MEASURED** — of the components waybill emits but the closure lacks, most are
boot libraries: `base`, `bytestring`, `containers`, `text`, `template-haskell`,
`ghc-prim`, `ghc-bignum`, `integer-gmp`, `array`, `binary`, `deepseq`,
`directory`, `filepath`, `mtl`, `parsec`, `process`, `stm`, `time`,
`transformers`, `unix`, `pretty`, `exceptions`. 18 of moat's 20, roughly 22 of
slack-web's 30. The remainder is the project's own main modules plus R1's
executable-stanza set.

**REASONED**: they ship inside the GHC derivation rather than beside it, so
their absence is about where nix puts them, not a gap. ⚠ A reconciliation that
treated closure-absence as evidence a component is spurious would wrongly
discard the entire Haskell standard distribution.

---

## R3 — What the closure contains, and what of it belongs in a document

**MEASURED** (classifier committed at
`specs/1034-nix-eval-tier/measurements/classify-derivation-closure.py`):

| | moat | slack-web |
|---|---|---|
| derivations | 1,275 | 1,535 |
| artifact input | 264 | 390 |
| build tooling only | 134 | 146 |
| both | 52 | 55 |
| neither | 825 | 944 |
| — of which patches | 43 | 50 |
| — of which naming a CVE | 3 | 4 |

**MEASURED** (`closure-vs-emitted.py`): 216 and 218 artifact inputs are absent
from today's output, against overlaps of 33 and 160.

**Decision**: artifact inputs and build tooling both become components,
distinguished by a scope marker (spec FR-002, clarified 2026-09-29). Patches
become `pedigree` entries, not components. The rest of the `neither` bucket —
fetched sources, setup hooks, bootstrap toolchain — is out of scope for v1.

**Rationale**: nix records the artifact/tooling split itself via
`nativeBuildInputs` versus `buildInputs`, so no heuristic is involved.

---

## R4 — The project flake must be addressed through the CLI, not `getFlake`

**Decision**: invoke `nix <cmd> <path>#<attr>`. Never `builtins.getFlake` on a
local path.

**MEASURED**:

```
$ nix eval --expr 'builtins.attrNames (builtins.getFlake "path:/…/moat").packages.aarch64-darwin'
error: cannot call 'getFlake' on unlocked flake reference 'path:/…/moat',
       at «none»:0 (use --impure to override)

$ cd /…/moat && nix eval --json --apply builtins.attrNames '.#packages.aarch64-darwin'
["default","moat-ghc910","moat-ghc94","moat-ghc96"]
```

**REASONED**: `--impure` is refused by the argv guard shipped in PR #1044, and
for good reason — it restores host-environment access during evaluation of
expressions the repository controls. The CLI flakeref form is the only route
that stays pure, so it is a constraint rather than a preference.

---

## R5 — The safety guard becomes load-bearing on this path

**MEASURED** (probe R9, committed in PR #1044): a flake can request
`allow-import-from-derivation` through its own `nixConfig`. Nix ignores it as
untrusted unless `--accept-flake-config` is passed, at which point the flake
wins and a derivation it supplies is built. Real flakes carry these settings —
slack-web sets both `allow-import-from-derivation` and `extra-substituters`.

**REASONED**: milestone 1034 evaluates nixpkgs at a pinned revision, so
repository-authored expressions never run and the guard is precautionary. This
feature evaluates the project's own flake, so they do. The guard moves from
defence-in-depth to the thing standing between a scanned repository and control
of nix's evaluation settings. Spec FR-014 and FR-017 follow from this.

---

## R6 — Cost

**MEASURED**: `nix derivation show -r .#default`, warm store — 1.09 s / 5.7 MB
for moat, 1.07 s / 7.4 MB for slack-web.

**UNMEASURED**: cost on a store that has never held the closure. Inherits
milestone 1034's position — the budget covers acquisition as well as
evaluation, and a clean runner is needed to measure it. Not blocking: the
feature is opt-in and bounded, and `--offline` refuses it outright.

---

## Research tasks carried into implementation

| ID | What | Why not now |
|----|------|-------------|
| ~~T-R1~~ | ~~Attribute a patch to the component it patches~~ | **Resolved, R7.** `env.patches` gives the join. |
| **T-R2** | Whether closure composition holds outside Haskell. | No non-Haskell Nix project measured |
| **T-R3** | Cold-store cost (R6). | Needs a clean runner |

Each must produce a committed probe.

---

## Summary of changes to the spec

| Spec position | Outcome |
|---|---|
| 20/30 components absent from the closure "plausibly belong to another attribute" | **Wrong.** R1: they are executable-stanza and boot-library components. Both of slack-web's attributes are the same derivation. |
| Replace-versus-supplement is an open scope question | **Settled: supplement.** R1, from measurement rather than preference. |
| Build tooling not emitted | Already reversed by the 2026-09-29 clarification; R3 confirms the split is free. |

---

## R7 ⚠ — Patch attribution works, and finds far more than the filename scan did

**Decision**: T-R1 is resolved. Patches attribute to components mechanically
through the derivation's own `patches` field. No heuristic, no guessing.

**MEASURED**: 111 derivations in moat's closure carry a non-empty `env.patches`
field listing the store paths they apply:

```
  patchutils : /nix/store/b8znwq…-Make-grepdiff1-test-case-pcre-aware.patch …
  pkg-config : /nix/store/n5kkah…-gcc-15.patch /nix/store/f4bvwq…-requires-private.patch
```

Resolving those basenames gives the join. 61 and 66 derivations apply at least
one patch.

⚠ **The earlier CVE count was a five-fold undercount.** Scanning *derivation
names* for CVE patterns — what the first classifier did — found 3 and 4 CVEs.
Joining through `env.patches` finds:

| | moat | slack-web |
|---|---|---|
| distinct CVEs | **18** | **14** |
| components carrying them | 4 | 3 |

The difference is that many patches are not separate derivations with
CVE-shaped names; they are files referenced by store path. The `patches` field
catches both, and the CVE is in the basename either way.

`unzip 6.0` carries **11 CVEs across 26 patches** in both projects — the case
#1040 was filed on, now confirmed in two real closures rather than argued from
one example.

## R8 ⚠ — The CVE-carrying components span every role, including one v1 excluded

**MEASURED**, joining patch attribution against the role classification:

| project | component | role | CVEs |
|---|---|---|---|
| moat | `unzip` | **tooling** | 11 |
| moat | `libssh2` | artifact | 1 |
| moat | `jq` | **neither** | 5 |
| moat | `lua` | **neither** | 1 |
| slack-web | `unzip` | **tooling** | 11 |
| slack-web | `perl` | both | 2 |
| slack-web | `lua` | **neither** | 1 |

Two consequences, both correcting earlier positions.

⚠ **Spec assumption A-4 would discard a third of the evidence.** It scopes the
`neither` bucket out of v1 as "the largest bucket and the least understood, and
nothing measured yet argues it belongs in a document". Something does now: `jq`
and `lua` sit in it and carry 6 of moat's 18 CVEs. A-4 must narrow to exclude
only what has been shown to carry nothing — fetched sources, setup hooks,
bootstrap toolchain — rather than the bucket wholesale.

**The build-tooling clarification turns out to be load-bearing** for a reason
neither of us gave when deciding it. It was argued on "a consumer can filter
down but cannot recover what we dropped". The stronger reason is that `unzip`
is tooling and carries 11 CVEs — the single richest vulnerability signal in
both closures. Dropping tooling, as the spec originally required, would have
discarded it.

**REASONED**: this also sharpens what the feature is *for*. The patch evidence
does not cluster in the Haskell dependency graph at all. It is in the C
utilities nixpkgs uses to build things — `unzip`, `jq`, `lua`, `perl`,
`libssh2` — which no Haskell manifest mentions and which waybill does not emit
today in any form.
