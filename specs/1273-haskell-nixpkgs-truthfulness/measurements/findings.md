# Milestone 1273 — Phase 0 measurements

Every number below was produced by a script committed in this directory.
Where a figure is inherited from an issue rather than re-run here, it says so.

Probes:

| script | subject | needs |
|---|---|---|
| `reproduce-1273.sh` | all four issues, at whatever binary it is pointed at | cached nixpkgs revision |
| `hpack-semantics.sh` | hpack's rendering rules for the FR-001a subset | the real `hpack` |
| `license-shapes.sh` | how the cached package set declares a license | cached nixpkgs revision |

---

## Probe A — hpack's rendering rules (FR-001a's measurement obligation)

FR-001a names four constructs as modelled and then requires that their exact
semantics "MUST be measured before a requirement depends on them, not read
from its documentation alone". This is that measurement.

**hpack 0.38.3**, from
`/nix/store/dsydwj18w6qhp5pi8c9ww2hg1dhp4zif-hpack-0.38.3`, obtained with
`nix build --no-link --print-out-paths nixpkgs#haskellPackages.hpack`.

hpack is the oracle. The question is not what waybill should do with a
`package.yaml`, it is what the file *means*, and hpack's rendered `.cabal` is
the definition of that.

### A1 — inheritance reaches every component kind

`c1`: a top-level `dependencies: [base, text]` renders into **all four**
stanza kinds — `library`, `executable`, `test-suite` and `benchmark` — each
getting `base, text`.

Consequence: the modelled reader must apply the top-level list to test and
benchmark stanzas too, and those stanzas already carry
`LifecycleScope::Development` in `stanza_lifecycle_scope`. A reader that
inherited only into the library would under-report; one that inherited into
everything but kept the library's scope would mis-scope.

### A2 — a per-component list is additive, not a replacement

`c2`: top-level `[base]`, library `[containers]`, executable `[aeson]`.
Rendered: library `base, containers`; executable `aeson, base`. Neither
per-component list displaced the inherited `base`.

### A3 — for a repeated name, the LAST declaration wins outright. There is no intersection.

Four cases, all on one name:

| case | manifest | rendered |
|---|---|---|
| `c3` | top `base >= 4 && < 5`, library `base` | `base` |
| `c4` | top `base`, library `base >= 4 && < 5` | `base ==4.*` |
| `c5` | top `text >= 1.0`, library `text < 3` | `text <3` |
| `c6` | top `text >= 1.0` **and** `text < 3`, no per-component list | `text <3` |

`c3` is the sharp one: a bare per-component entry **discards** a constraint
the top level declared. `c6` shows the rule is not "component beats
top-level" but a single ordered merge in which a later entry replaces an
earlier one for the same name — the same thing happens inside one list with
no inheritance involved.

So the merge is **last-wins per name, replacing the constraint entirely**.
An implementation that intersected `>= 1.0` with `< 3` would report a range
the project does not declare.

### A4 — hpack rewrites constraints; waybill should not

`c4` declares `base >= 4 && < 5` and hpack renders `base ==4.*`. That is a
cosmetic normalisation of an equivalent range, and it means the rendered
`.cabal` is not a character-level record of the manifest.

waybill should record the range **as the manifest declares it**, which is
also what the `.cabal` reader already does for `build-depends`
(`DeclaredDep.range` / `all_ranges`, milestone 895 FR-007). Reproducing
hpack's normalisation would be work in service of matching an artifact
waybill no longer reads.

### A5 — `when:` is not evaluated, except for a literal `false`

| case | condition | rendered |
|---|---|---|
| `c7` | `true` | `build-depends: base` **plus** `if true { build-depends: unix }` |
| `c8` | `false` | `build-depends: base` — the block is **gone** |
| `c9` | `os(darwin)`, `flag(fast)` (default `false`) | `if os(darwin) { unix }`, `if flag(fast) { vector }` — both passed through |

hpack prunes a literal `false` and otherwise lowers the block to a Cabal
`if`. It does **not** evaluate `os(...)`, and it does **not** evaluate
`flag(...)` against the default it was given in the same file — `fast`
defaults to `false` and `vector` is still emitted, conditionally.

**This is the planning finding of the probe.** A `when:` block renders to
exactly the construct waybill's `.cabal` reader already handles: a Cabal
conditional, evaluated against declared flag defaults by
`stanza_default_buildable` (milestone 955 / PR #1030). The hpack reader
should therefore *lower* `when:` into the same representation the `.cabal`
reader consumes, and inherit its flag-default evaluation and its
`buildable: False` optional mark. Writing a second conditional evaluator
would give one project two answers depending on which file it declared its
dependencies in.

It also means US1 and US2 meet: an hpack-only project with a default-off
`when: condition: flag(...)` block gets an optional mark, and that mark then
goes through the US2 closure. The two stories are not independent at the
fixture level, and the task ordering has to reflect it.

### A6 — `source-dirs` cannot change the dependency set

`c10`: two `source-dirs` change `hs-source-dirs` and add `Other` to
`exposed-modules`. `build-depends` is untouched.

So `source-dirs` is in FR-001a's modelled subset for *module* purposes only.
Nothing in the dependency set depends on it, and an implementation may read
it for `source_path` / evidence without it affecting a single component's
identity or scope.

### A7 — `defaults:` makes hpack itself fail without the network. `!include` does not.

`c11` declares `defaults: {github: waybill-fixture/does-not-exist}`. hpack
refuses the **whole manifest**:

```
Invalid value for "defaults"! File
https://raw.githubusercontent.com/waybill-fixture/does-not-exist/main/.hpack/defaults.yaml
does not exist!
```

It renders nothing at all — there is no partial answer to copy. This is the
measurement FR-001c rests on: honouring `defaults:` is a network fetch, and
there is no offline fallback even in hpack.

`c12` resolves `!include "common.yaml"` from a **sibling file on disk** and
renders `base, text`. No network.

FR-001b announces both constructs together, which is correct as a scope
decision. But they are not the same kind of construct: `!include` is a local
file read and a cheap later win, `defaults:` is a network dependency and is
not. The plan records that asymmetry so a future milestone does not have to
re-measure it.

### A8 — the probe's own first version lied, and how

The first version of `hpack-semantics.sh` filtered the rendered `.cabal`
through awk matching `^  build-depends:` and its `^    ` continuations. A
Cabal `if` block nests its own `build-depends` one level deeper, so every
conditional dependency was invisible. Cases `c7` and `c9` — whose entire
subject is the conditional — printed as though hpack had dropped the
dependency, and `c7`, `c8` and `c9` printed **identically** when the whole
point was that they differ.

Three cases that must differ reading the same is the signature of a broken
harness, not of a uniform system. The committed script prints the stanza
verbatim and does no filtering.

---

## Probe B — how the cached package set declares a license (US3 / FR-010, FR-011)

`hackage-packages.nix` at revision `a799d3e3886da994fa307f817a6bc705ae538eeb`,
already in `~/.cache/waybill/nixpkgs/`, so this is offline. 16,634,427 bytes.

**19,437 license declarations**, against 19,253 `= callPackage` attribute
blocks matched by the parser's own regex. Every derivation block declares
one: there is no "absent license" case *in the file*. #1280's `0 of 66` is
therefore not a coverage limit in the source, it is that nothing reads the
field.

All 19,437 are single-line `license = <expr>;`. **Zero** multi-line forms,
zero `with lib.licenses; [ ... ]` lists, zero list-valued declarations. The
three families partition the file exactly (14,481 + 3,962 + 994 = 19,437):

| family | count | example |
|---|---|---|
| `lib.licenses.<attr>` | 14,481 | `license = lib.licenses.bsd3;` |
| `lib.meta.getLicenseFromSpdxId "<id>"` | 3,962 | `license = lib.meta.getLicenseFromSpdxId "MIT";` |
| a bare string | 994 | `license = "GPL";` |

### B1 — the attribute vocabulary is 15 entries, not a nixpkgs-wide table

Every distinct `lib.licenses.<attr>` at this revision:

```
9069 bsd3        661 gpl3Only     136 bsd2        79 mpl20       3 bsdOriginal
3314 mit         389 asl20        127 lgpl21Only  79 agpl3Only   2 lgpl2Only
 313 publicDomain 152 gpl2Only    105 lgpl3Only   51 isc         1 lgpl21Plus
```

Fifteen names. A 15-entry attribute → SPDX-id mapping covers 14,481 of
19,437 declarations, and the mapping is a lookup table, not a nixpkgs
evaluation. `publicDomain` is the one with no clean SPDX identifier.

### B2 — the SPDX-id family needs no mapping

23 distinct ids, all already SPDX: `0BSD`, `AGPL-3.0-only`,
`AGPL-3.0-or-later`, `Apache-2.0`, `BSD-2-Clause`, `BSD-2-Clause-Patent`,
`BSD-3-Clause`, `CC0-1.0`, `EUPL-1.2`, `GPL-2.0-only`, `GPL-2.0-or-later`,
`GPL-3.0-only`, `GPL-3.0-or-later`, `ISC`, `LGPL-2.0-or-later`,
`LGPL-2.1-only`, `LGPL-2.1-or-later`, `LGPL-3.0-only`, `LGPL-3.0-or-later`,
`MIT`, `MPL-2.0`, `Unlicense`, `Zlib`. Pass-through to
`SpdxExpression::try_canonical`.

### B3 — the bare-string family splits 946 / 48, and `"unknown"` is not a license

All 994 bare strings:

| value | count | canonicalises? |
|---|---|---|
| `"GPL"` | 438 | no — no version |
| `"unknown"` | 360 | no — and it is not a license at all |
| `"LGPL"` | 143 | no |
| `"AGPL"` | 5 | no |
| 17 distinct SPDX expressions | 48 | yes |

The 48 are real expressions, several compound:
`"(BSD-2-Clause OR Apache-2.0)"` (11), `"(Apache-2.0 OR MPL-2.0)"` (8),
`"GPL-2.0-or-later AND BSD-3-Clause"` (3),
`"(AGPL-3.0-only WITH Universal-FOSS-exception-1.0 OR LicenseRef-commercial)"`
(6), and others. The `LicenseRef-` forms are already handled by milestone
153's LicenseRef sweep.

`"unknown"` (360, **36% of the bare-string family**) is a declaration that
the license is *not known*. Emitting it as a license value would publish
the string `unknown` into a `licenses[]` array, which is worse than an
absent field: a consumer filtering for components with no license
information would not find it. **FR-012's "no license may be attached to a
component the resolution did not resolve" has a sibling case here, and the
plan resolves it: `"unknown"` is treated as ABSENT, not as an
uncanonicalisable license.** `"GPL"` / `"LGPL"` / `"AGPL"` (586) are
genuine but unversioned declarations and are FR-011's case — preserved as
declared, recorded as uncanonicalised.

---

## Probe C — the four issues at 0.14.0

`reproduce-1273.sh`, committed with the spec. Its output is the Context
table in `spec.md`. Re-run before any task depends on a figure in it.

---

## Probe D — SC-003's second figure, re-measured (was inherited from #1281)

`sc003-public-library.sh` + `fanout.py`. SC-003 quoted "8 components in
total on the public library #955 was filed against", taken from #1281's
sbomnix 1.9.0 cross-check at 0.13.1. It was the one number in the spec not
re-run, and the requirements checklist flagged it for Phase 0.

**Subject**: `slack-web` @ `7b46273`, whose `cli` executable is gated behind
a flag defaulting to `False` — the project #955 was filed against. Its
`flake.lock` pins nixpkgs `5ae3b07d8d6527c42f17c876e404993199144b6a`, which
is in the local per-revision cache, so the scan is `--offline`. Measured at
**waybill 0.14.0**.

### D1 — the two directly-marked components reproduce exactly

| | `butcher` | `monad-loops` |
|---|---|---|
| no `flake.lock` | `optional` / `cabal-flag-disabled-stanza` | same |
| + `flake.lock` | runtime, no derivation, `@1.3.3.2` | runtime, `@0.4.3` |

`restored=2`, and the closure seed is `declared=45`, which contains both.
Confirms #1281's mechanism at 0.14.0.

### D2 — the "8" does not reproduce. It is **10**, and the composition differs.

The figure cannot be read as a classification disagreement at all: case A
emits 44 hackage components and case B emits 190, because the closure pass
only runs in case B. The six extra components #1281 named are not
*re-classified* by the withdrawal, they are *introduced* by the closure and
then made runtime-reachable by it.

The metric that does capture the defect is reachability: what can the root
no longer reach once the marked components are removed from the graph?

| | measured |
|---|---|
| hackage components | 190 |
| reachable from the root | 190 |
| reachable **without** traversing the marked pair | 179 |
| reachable **only** through the marked pair | **11** |
| — the marked pair itself | 2 (`butcher`, `monad-loops`) |
| — non-boot fan-out below it | 8 (`clock`, `deque`, `extra`, `microlens`, `microlens-th`, `multistate`, `strict-list`, `unsafe`) |
| — compiler-supplied (boot) | 1 (`pretty`) |

**The defect's population is 10**, not 8: the 2 marked plus 8 non-boot
fan-out. #1281 named 6 of those 8; `clock` and `extra` are new. The drift is
attributable to any of three moving inputs — the repository's HEAD, the
nixpkgs revision its lock pins, and 0.13.1 → 0.14.0 — and the probe is
committed so the next person can re-separate them.

`pretty` is excluded deliberately. nixpkgs binds it to `null` in
`configuration-ghc-9.10.x.nix`, so it is supplied by the compiler and is
present in every build of the project; counting it would overstate the
defect. `fanout.py` reads that binding from the same per-revision cache the
scan used rather than from a list of its own, so the exclusion cannot drift
away from the revision under measurement.

**SC-003 is corrected from 8 to 10 in `spec.md` on the strength of this**,
which is what the standing rule requires: a number in a success criterion
must be traceable to an observation, and the artifact must say which.

---

## Probe E — which committed corpus goldens this milestone will churn

Not a probe script: four `gh api .../git/trees/<pin>?recursive=1` queries
against the pinned revisions the corpus manifest names, plus a read of the
committed goldens. Recorded because the answer was first *inferred* and the
inference was wrong, in both directions.

### E1 — the corpus has four Haskell/Nix targets, all with committed goldens

`waybill-cli/tests/corpus_harness_195/manifest.rs` names `haskell-aeson`,
`haskell-security-advisories`, `haskell-language-server` and
`nix-closure-moat`, and all four have `cdx.json`, `spdx-2.3.json` and
`spdx-3.json` under `waybill-cli/tests/fixtures/public_corpus/`.

An earlier draft of `research.md` §R7 asserted the corpus had no Haskell
target, reasoning from milestone 1272's npm work where that was true of npm.
It is false here.

### E2 — `package.yaml` in the goldens is a reason STRING, not a manifest

Every occurrence of the substring `package.yaml` in the goldens — 55 in
`haskell-aeson`, 53 in `haskell-security-advisories`, 19 in
`nix-closure-moat` — is the *value* of `waybill:unresolved-reason`:

```
declared in .cabal / package.yaml; no cabal.project.freeze fallback
```

That is `REASON_CABAL_NO_FREEZE` (`haskell.rs:2217`). Grepping the goldens
for `package.yaml` and concluding those trees contain hpack manifests was
the second wrong inference; the string is the reader's own message about a
`.cabal`.

### E3 — what the pinned trees actually contain

| target | `package.yaml` | `flake.lock` | `*.cabal` |
|---|---|---|---|
| `haskell-aeson` @ `682162c6` | **0** | **0** | 6 |
| `haskell-security-advisories` @ `4dc0b9b9` | **0** | 1 | 6 |
| `haskell-language-server` @ `1b4b3c6b` | 1 | 1 | 75 |
| `nix-closure-moat` @ `d0690555` | 1 | 1 | 2 |

Expected churn, per story:

- **`haskell-aeson`** — no flake, no manifest. US1/US2/US3 cannot reach it,
  and with no nixpkgs pass nothing resolves a version, so FR-013 strips no
  C151 either. Predicted **unchanged**; this is the control.
- **`haskell-security-advisories`** — flake, no hpack manifest. US3 adds
  licenses and US2/US4 may change scopes and reasons. Predicted changed.
- **`haskell-language-server`** — its single `package.yaml` is **test
  data**, at `plugins/hls-cabal-plugin/test/testdata/cabal-add-testdata/packageYaml/`,
  and it *has* a sibling `packageYaml.cabal`. So this target exercises
  FR-002's suppression path, on a fixture inside someone else's test suite
  that the walker does not exclude. Worth naming rather than discovering
  during a golden refresh.
- **`nix-closure-moat`** — a **root `package.yaml` with no sibling
  `.cabal`**. This is #1278's exact shape, live in the project's own
  committed corpus, and its golden currently records 321 hackage components
  derived without reading that file. The primary defect has a real-world
  witness already under version control.

### E4 — the refresh route

The goldens are `#[ignore]`d locally and `public-corpus.yml` runs only on
`schedule` and `workflow_dispatch`, never on a PR, so this churn surfaces as
a red nightly rather than a red PR unless it is driven deliberately.
`WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is never set locally; the refresh
goes through the workflow dispatch. The plan schedules that as its own task
rather than leaving it to the nightly.

---

## Probe G — US2 re-measured after the fix (#1281)

`sc003-public-library.sh` + `fanout.py`, same subject and same pinned
revision as Probe D, re-run with the re-seeded closure in place.

### G1 — the marks survive, and SC-003 is met

| | before | after |
|---|---|---|
| optional in case A, NOT optional in case B | **2** | **0** |
| `butcher` / `monad-loops` still marked with `flake.lock` | neither | **both** |
| `restored=` / withdrawn | 2 | 0 |

SC-003 reads "classifies its flag-gated dependencies identically with and
without a `flake.lock`, unless a non-seed closure path reaches them". The
disagreement is now zero.

### G2 — the fan-out is no longer published as unqualified runtime

Case B now classifies **9** further components Optional that case A does not
emit at all: `clock`, `deque`, `extra`, `microlens`, `microlens-th`,
`multistate`, `pretty`, `strict-list`, `unsafe`.

These are not a disagreement — case A emits 44 hackage components and case B
190, because the closure pass runs only in the `flake.lock` case, so they do
not exist in A to disagree with. They are the blast radius: reachable from
the root only through the marked pair, and now carried as Optional by
milestone 1269's scope-propagation pass rather than as runtime dependencies
of a build that does not contain them.

So the defect's full measured population of **10** is addressed: 2 marks that
survive, plus 8 non-boot components below them that stop being published as
unqualified runtime dependencies. **No code in this milestone propagates to
the fan-out** — m1269's Phase B does it, once the mark it depends on stops
being withdrawn. That interaction was predicted in the plan and is now
measured rather than assumed.

### G3 — a finding recorded, deliberately not fixed here

`pretty` is among the 9. It is a **compiler-supplied (boot) library**:
nixpkgs binds it to `null` in `configuration-ghc-9.10.x.nix`, so GHC ships it
in every build of the project whatever the flags say. Classifying it Optional
is a statement about the *dependency relationship* — the only path to it runs
through an optional component — and not about whether the artifact contains
it, which it always does.

Neither the old answer nor the new one is precise: before this change it was
runtime-reachable through a mark that should never have been withdrawn, which
was wrong for a different reason. The imprecision belongs to how m1269's
propagation treats boot libraries, which no FR in US2 covers, so it is
recorded here rather than fixed under this story. Worth a follow-up issue
alongside the `dependency_claims` disagreement in `mutations.md`.

### G4 — FR-015 / SC-007 blast radius, checked rather than assumed

Every adjacent suite run unchanged:

```
optional_dep_classification     3 passed    scope_propagation_ecosystems    7 passed
optional_dep_yarn_v1_e2e        6 passed    haskell_suppression_scope_m938  4 passed
optional_dep_cross_dialect      3 passed    pnpm_v9_scope                   5 passed
optional_dep_error_paths        3 passed    haskell_cabal_baseline          3 passed
nix_haskell_resolution_m926    41 passed    haskell_edge_cases              8 passed
nix_config_closure             14 passed    nixpkgs_declarations            5 passed
```

m1272's measured near-miss is the reason this is a listed check: folding an
extra section into a runtime seed there silently reassigned a component from
Optional to Development, a change to the *development* axis forbidden by that
milestone's FR-010. The re-seeded closure here removes the marked names and
changes nothing else about the seed.
