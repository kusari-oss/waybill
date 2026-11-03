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

---

## Probe F — US4's population, re-measured after US2 and US3 landed (T057)

The spec's Assumptions say US4's remaining population "must be measured after
US2 lands rather than predicted now", because the 2 contradicting components
are the same 2 US2 is about. This is that measurement, taken with US1, US2
and US3 on the branch.

`reproduce-1273.sh`, same fixture and cached revision as Probe C.

| | at 0.14.0 | now |
|---|---|---|
| #1278 hpack-only components (3 declared) | **0** | **3** |
| #1278 `graph-completeness` | `complete` | `partial` |
| #1278 document record of the hpack read | **absent** | `seen=1 read=1 unread=0 partial=0` |
| #1281 `aeson`/`async` with a `flake.lock` | runtime, no derivation | **optional**, derivation intact |
| #1281 reconciliation | `restored=2` | `reviewed=2 withdrawn=0 declined=2` |
| #1280 components carrying a license | **0 / 66** | **47 / 66** |
| #1279 resolved AND unresolved-reasoned | **2** | **2** |

### F1 — the spec's expectation about US4 was wrong, and that is why it was measured

The spec said "part of it may resolve as a consequence of US2". It did not.
`aeson` and `async` keep their optional mark now (US2 working), and the
nixpkgs pass still assigns them a version — those are independent facts. A
component can be optional *and* version-resolved, and it is the **version**
that makes its `waybill:unresolved-reason` stale, not its scope.

So US4's population is unchanged at 2, and the fix is exactly what R5
described: strip the annotation where the version is assigned. Had this been
predicted rather than measured, the natural prediction — "US2 fixes one or
both" — would have been wrong in a way that made T058 look unnecessary.

### F2 — 47 of 66, not 66 of 66, and that is correct

SC-004 asks for "all of those the cached set declares one for", not all 66.
The 19 without a license are the components the pass did not resolve (FR-012
forbids inventing one) plus any whose declaration is `"unknown"` (FR-011a
treats that as absent). The figure is a floor that moves with the fixture, so
the success criterion is stated against what the set declares rather than
against a count.

### F3 — the probe had stopped printing

Two of `reproduce-1273.sh`'s four sections silently vanished once the #1281
fix changed the log line it keyed on: `grep -o "restored=[0-9]*"` matched
nothing, exited 1, and `set -o pipefail` killed the script mid-run. The
output simply ended after the closure line, with exit status 0 visible
nowhere in the terminal.

This is the zero-match-grep hazard the project has hit before, and it is
worth noting what it looked like from outside: **not an error, a short
report.** A probe that stops measuring half of what it claims to measure
reads exactly like a probe whose later findings are unchanged. Corrected with
`|| true` on each grep and a pattern that accepts both the old and new log
lines, so the script keeps working against a binary from either side of the
fix.

---

## Probe re-run after all four stories (T080 / SC-008)

`run-all.sh`, with all four stories on the branch:

```
──── Probe B — license shapes: PASS
──── Probe C — the four issues: PASS
──── Probe D — SC-003 public library: PASS
──── Probe A — hpack semantics: PASS
all probes that ran reproduced their finding
```

**Each probe's recorded finding is now post-fix.** Probe C and Probe D
describe the fixed behaviour; the pre-fix numbers they used to print are
preserved in `baseline-0.14.0.md`, captured by T003 before any reader was
touched. That is what FR-016 is for: a correction justified against a
measurement rather than against the spec, and the measurement has to predate
the change to be worth anything.

### The harness failed before the probes did, twice

First run of `run-all.sh` reported **Probe C: FAIL** and **Probe D: FAIL**.
Neither finding had regressed. The script `cd`s into its own directory and
the probes it calls do not, so `WAYBILL=./target/debug/waybill` — the natural
thing to type from the repo root — pointed nowhere once the `cd` had
happened. Every scan failed to launch and both probes reported their findings
unreproducible.

The fix then failed a second time, differently: `REPO_ROOT` was derived from
`$0` *after* the `cd`, and `$0` is relative to the caller's original
directory, so it no longer resolved. `run-all.sh` now absolutises the binary
path, resolves a relative one against `$OLDPWD` (where the caller actually
stood), and **exits 2 with a build hint** if the binary is not executable —
rather than letting every probe fail and look like a regression.

Verified from three invocation points: repo root with an explicit relative
path, repo root with no `WAYBILL` at all, and from inside the measurements
directory.

This is the shape worth remembering. A probe that cannot find its subject
reports the same thing as a probe whose subject is broken: **FAIL**. The
difference is invisible in the summary line, and the summary line is what
gets read. `run-all.sh` now refuses to run at all rather than produce that
ambiguity, which is the same principle as `mutate.py` refusing to report a
verdict for a mutation it could not apply.

---

## Probe H — the corpus refresh, and a defect it found (T075–T077)

`public-corpus.yml` dispatched against this branch with `regen_goldens=true`,
run 41571149440. **17 of 17 targets wrote goldens** (the #1286 guard), and the
run's own log confirms the scanned commit was `e0ebe6e9` — this branch's
closeout — not `main`. That check matters: on a `workflow_dispatch` the
display shows `headBranch=main`, because the workflow definition loads from
the default branch while Checkout uses `inputs.branch`.

### H1 — Probe E's predictions held exactly

| target | predicted | measured |
|---|---|---|
| `haskell-aeson` | **unchanged** (the control) | **unchanged** |
| `haskell-security-advisories` | changed | changed, 3 formats |
| `haskell-language-server` | changed | changed, 3 formats |
| `nix-closure-moat` | changed | changed, 3 formats |
| the other 13 targets | not predicted | **all unchanged** |

Thirteen unrelated targets byte-identical is the blast-radius answer, and
`haskell-aeson` holding is the control doing its job: it has no `flake.lock`
and no `package.yaml` at its pin, so no code path in this milestone can reach
it. Had it moved, the milestone would be wrong whatever the other three did.

### H2 — the three real projects, before and after

| | moat | security-advisories | language-server |
|---|---|---|---|
| hackage components | 53 | 248 | 360 |
| carrying a license | 1 → **33** | 6 → **221** | 6 → **327** |
| carrying a reason | 19 → 7 | 53 → 14 | 122 → 25 |
| **resolved AND reasoned** | 12 → **0** | 39 → **0** | 97 → **0** |

**#1279's self-contradiction across three real projects: 148 → 0.** Licenses:
13 → 581. These are the numbers the synthetic fixtures were standing in for.

### H3 — the corpus found a defect 15 fixtures and 33 mutants did not

`nix-closure-moat`'s root `package.yaml` came back **unread**:

```
reason: not-a-manifest
detail: invalid type: floating point `0.1`, expected a string
```

The manifest declares `version: 0.1`. Unquoted in YAML that is a **float**,
and the reader declared `version: Option<String>`, so `serde_yaml` refused
the whole document. Real hpack accepts it. A perfectly good project was
reported as having an unreadable manifest — FR-003's diagnostic firing
correctly over a bug of mine, which is at least the honest failure mode.

Every synthetic fixture in this milestone used a quoted or three-component
version (`0.1.0`, `"1.2.3"`), so **15 fixtures, 4 integration suites and 33
mutants all missed it**. A fixture set written by whoever wrote the parser
shares its blind spots; this came from scanning somebody else's repository.
Fixed with a `Scalar` deserializer that accepts what hpack accepts, a
regression fixture (`numeric_version/`), and **M34**, which confirms the test
catches the original behaviour.

The other unread manifest is **correct**:
`haskell-language-server`'s only `package.yaml` is test data for its
cabal-add plugin and genuinely has no `name:` field, so `not-a-manifest` is
the right answer and FR-003 names it rather than passing over it.

### H4 — T077's claim is NOT yet satisfied, and the goldens are not landed

T077 asserts that `nix-closure-moat`'s refreshed golden "now reads its root
`package.yaml`". It does not: the golden from this run records it as unread,
because of H3. The fix is in the tree but **the goldens in that artifact
predate it**, so landing them would commit a document asserting a defect that
no longer exists.

The refresh has to be re-dispatched against the branch with the H3 fix on it.
Until then the three changed targets' goldens remain un-landed, and that is
the correct state rather than a partial one.

### H5 — a second corpus finding: FR-002 dropped a license

The re-dispatched run (41572401991, scanned commit `0b629fe4`) satisfied
T077 — `nix-closure-moat` reports `manifests-read: 1, manifests-unread: 0` —
and the diff still matched Probe E exactly, control included. But reading the
golden's substance rather than its diff found a **regression this milestone
introduced**:

| | buggy run | fixed run |
|---|---|---|
| `moat`'s own main module | `evidence=cabal-pkg-descriptor`, `licenses=['MIT']` | `evidence=hpack-manifest`, `licenses=[]` |

The evidence change is FR-002 working. The license loss is not.

**hpack *infers* the license from a `LICENSE` file** and writes it into the
generated `*.cabal`; it is never a `license:` key in the manifest. Verified
against the real files: `moat`'s `package.yaml` has no license field, and its
generated `moat.cabal` carries `license: MIT` / `license-file: LICENSE`. So
suppressing the generated file under FR-002 lost the main module's license
outright.

Carrying it forward is **not** a second source of truth. FR-002 settles which
file answers for the *dependency set*; a field the manifest never mentions
has no competing answer to choose between, and the generated file is that
same manifest rendered by the tool that owns it. Milestone 954 added the main
module's license precisely because it is usually not a published package —
there is no registry to enrich from, so a drop is permanent.

Fixed, with `manifest_without_license/` as the regression fixture and **M35**
confirming the test catches the loss.

### H6 — what the corpus caught that nothing else could

Two real defects, neither reachable by anything in this milestone's own
test apparatus:

| defect | why the fixtures could not find it |
|---|---|
| `version: 0.1` rejected (H3) | every synthetic manifest used a quoted or three-component version |
| the inferred license dropped (H5) | no fixture had a manifest *without* a license beside a generated `*.cabal` *with* one |

Both are shapes a fixture set written by the parser's own author does not
contain, and a mutation pass can only probe shapes the set already has. 17
fixtures, 5 suites, 35 mutants and nine green gates all passed over both.

It also shows what a green workflow does and does not establish. Run
41571149440 was green at every step, its #1286 guard confirmed 17 of 17
targets wrote goldens, and the per-target diff matched the prediction
exactly — and its goldens were still wrong to land. "The workflow succeeded"
and "the artifact is correct" are different claims; only reading the C200
payload and the main module's licenses told them apart.
