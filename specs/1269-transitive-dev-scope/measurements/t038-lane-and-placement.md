# T038 — what the corpus lane found, and the placement defect it exposed

**Date**: 2026-10-26 | **Feature**: `1269-transitive-dev-scope` (#1209)
**Run**: `public-corpus.yml` #40249731861, `-f branch=1269-transitive-dev-scope -f regen_goldens=true`

## The lane's result

`106 passed, 3 failed`. `python-flask` and `rust-ripgrep` passed, which
confirms both halves of the amended SC-002: flask's declaration is
correctly deleted, and ripgrep's retained declaration is correctly
still declared.

The three failures were `haskell-language-server` (26),
`haskell-security-advisories` (13) and `nix-closure-moat` (9) — the
same counts as their deleted declarations, all `pkg:hackage/*`
violators.

The job name read `Run public corpus (1269-transitive-dev-scope)` and
the checkout step `ref: 1269-transitive-dev-scope`, which is the
documented check that `-f branch=` took effect rather than the lane
silently scanning main.

> Incidental: the run summary's `Commit:` line read `f8ff263f`, which is
> **main's** tip, not the branch's. It is `github.sha` — the ref the
> workflow file came from — while the checkout used the `branch` input.
> Cosmetic here, misleading in general, and worth a separate issue.

## Diagnosis: the pass was placed before a component producer

**This is a defect in this feature, not in a reader.** The three
targets were reported to the user as "pending, cause unknown"; the
cause is mine.

The call site was chosen on research R1's constraint that "the
component set must be final", verified against `deduplicate()` and both
m191 reconciler passes — all of which precede it. It was **not**
verified against component *producers* downstream of it. Two exist:

```
waybill::resolve::scope_propagation: propagation assigned nothing  immune=117 seeded_from=19
...
waybill::scan_fs::package_db::nix::haskell_packages: nixpkgs-haskell: runtime closure resolved
                                                    declared=128 transitive=232
waybill::cli::scan_cmd: nixpkgs-haskell: closure dependency edges emitted  closure_edges=2126
```

The nixpkgs-haskell runtime-closure resolver and the m1035 nix-closure
tier add 232 components and 2,126 edges to `haskell-language-server`
*after* the pass ran — including every one of its 26 violators. The
pass was operating on a set that was final only for ecosystems with no
post-reconciler producer, which is exactly why `python-flask` worked
and all three hackage targets silently did not.

### How it was found, because the first two hypotheses were wrong

1. **"FR-012 declined — hackage has no root."** Plausible: all three
   failing targets carry `transitive-edges-unresolvable: hackage` in
   their graph-completeness reason and `python-flask` does not. Wrong
   twice over — that reason code is m177's *design-tier* classifier,
   not a statement about roots, and the pass reported
   `rooted_ecosystems=["hackage"]`.
2. **"FR-003 declined — the reader stamped `Runtime`,"** i.e.
   `rust-ripgrep`'s shape. The haskell reader *does* construct every
   freeze- and stack-derived component with
   `lifecycle_scope: Some(Runtime)` (`haskell.rs`, three sites), which
   made this look right. Wrong: `HUnit` carries
   `waybill:nixpkgs-component-origin` and comes from the nixpkgs
   resolver, whose construction sites are `lifecycle_scope: None`.

Both were refuted by one log line, which did not exist until this
investigation needed it. The pass used to return early and silently
when it assigned nothing, so "the feature is a no-op here" and "the
feature declined here, deliberately, for a stated reason" were
indistinguishable from outside. It now logs on that path too, with
`skipped_unrooted_ecosystem` and `rooted_ecosystems`, and the summary
carries both. SC-001's exception (a) requires naming any component
skipped for want of a root, so the pass has to be able to say which.

## After relocating, measured on the cached corpus checkouts

Same binary, same inputs, harness-matching invocation
(`--root-name <target> --root-version <7 hex>`):

| target | assigned | I3 before | I3 after | declared count |
|---|---|---|---|---|
| `haskell-language-server` | 26 `development` | 26 | **0** | 26 ✓ |
| `haskell-security-advisories` | 72 `development` | 13 | **0** | 13 |
| `nix-closure-moat` | 9 `development` | 9 | **0** | 9 ✓ |
| `python-flask` | 51 `optional` | 51 | **0** | 51 ✓ |
| `npm-express` | nothing | 0 | **0** | — (FR-013 control) |
| `rust-ripgrep` | nothing | 2 | **2** | 2 — #1272 |

`haskell-security-advisories` assigns 72 against an I3 count of 13
because the pass marks population B as well, and I3 only counts
population A; the I3 count is what reaches zero.

**Four of the five declared targets are fixed.** Only `rust-ripgrep`
remains, as #1272, which is what the amended SC-002 says.

### A correction to an earlier claim

`haskell-security-advisories` emitted 251 components here, matching its
golden exactly. An earlier note in this feature said it emitted 62 and
was therefore "not measurable on this host", and the per-ecosystem
record's three pending rows rested on that. The 62 came from an
invocation without `--root-name` / `--root-version`. The host was never
the problem — `feedback_match_harness_invocation` says exactly this and
it was not applied.

## Open: the `--exclude-scope` ordering

The relocated call now sits **below** the `--exclude-scope` filter, so
FR-002's constraint 2 no longer holds: an operator excluding
`development` still receives the transitive closure of a declared dev
member. The constraint and the component-set constraint cannot both be
satisfied at the current filter position, because the producers are
below the filter.

Resolving it means moving the filter below the pass, which also makes
it apply to the nixpkgs and closure components it cannot currently see
— a change to an operator-visible flag beyond this feature's scope.
Left as-is and documented at the call site rather than widened
silently. The filter's own comment claims it is "the final step before
serialization", which has not been true since the nixpkgs tiers landed.

---

## The lane after the relocation: green

**Run**: #40251099255, same dispatch form. `109 passed; 0 failed`,
`no lane failures`.

The I3 column of the final matrix, every target:

| cell | targets |
|---|---|
| `.` pass | `python-flask`, `haskell-language-server`, `haskell-security-advisories`, `nix-closure-moat`, `go-opentelemetry`, `haskell-aeson`, `npm-express`, `pants-example-javascript` |
| `d` declared | `rust-ripgrep` — #1272, retained deliberately |
| `-` n/a | the remaining eight: no non-runtime dependency edge declared, so the invariant has no population |

That is SC-002 as amended: four deleted, one retained and still
reporting as declared. And SC-001 with exception (b) — a
reader-assigned `Runtime` — as its only instance. Exception (a), the
unrooted ecosystem, turned out to have **no** instances at all:
`skipped_unrooted_ecosystem=0` on every target measured.

`python-flask`, `haskell-security-advisories` and `maven-guice` keep
their pre-existing **I7** declarations, untouched. That is FR-018
holding in the data rather than in prose: scope marking does not change
reachability, so no orphan count moved.

## Golden churn, attributed (SC-007)

Five of 17 targets changed, all three formats each. Twelve byte-identical.

| target | CDX `scope` added | CDX properties added | scoped edges re-typed |
|---|---|---|---|
| `haskell-security-advisories` | 72 | 144 | 450 → `DEV_DEPENDENCY_OF` |
| `python-flask` | 51 | 102 | 123 → `OPTIONAL_DEPENDENCY_OF` |
| `haskell-language-server` | 26 | 52 | 52 → `DEV_DEPENDENCY_OF` |
| `nix-closure-moat` | 9 | 18 | 20 → `DEV_DEPENDENCY_OF` |
| `go-opentelemetry` | 5 | 10 | 37 → `TEST_DEPENDENCY_OF` |

Properties are exactly 2× the scopes in every row —
`waybill:lifecycle-scope` and `waybill:lifecycle-scope-derivation`,
one each.

**`go-opentelemetry` is the singleton the procedure warns about**: a
change appearing once across the whole review is not a category and
needs its own explanation. It has no I3 declaration and its I3 cell
passed before and after, so its five components are population B —
reachable from a declared non-runtime component but not from the
subject, hence never I3's business. The pass marked them, which is the
feature working on a target nobody had declared. Each of the five
gained `test` scope plus the `transitive` marker; the document is
strictly more informative and no invariant moved.

### Verified semantically, not by eyeballing a normalised diff

The raw diffs are dominated by content-addressed ID cascades — SPDX
2.3 relationship IDs and SPDX 3 `spdxId`/`subject`/`statement` are
hashes, so 102 new annotations move thousands of lines. Counting those
lines proves nothing. Two checkers are committed beside this file:

`spdx2check.py` compares annotation `(package, field, value)` multisets
and relationship `(from, type, to)` multisets with SPDXIDs mapped back
to PURLs:

```
python-flask:                 annotations +102 / -0   relationships +123 / -123
haskell-language-server:      annotations  +52 / -0   relationships  +52 /  -52
haskell-security-advisories:  annotations +144 / -0   relationships +450 / -450
nix-closure-moat:             annotations  +18 / -0   relationships  +20 /  -20
go-opentelemetry:             annotations  +10 / -0   relationships  +37 /  -37
```

Every target: **zero annotations removed**, and additions exactly one
`lifecycle-scope` plus one `lifecycle-scope-derivation` per newly
scoped component. Relationship additions equal removals exactly, and
every pair is `DEPENDS_ON` → a scoped type.

`spdx3flat.py` flattens SPDX 3's grouped `to[]` arrays, because
regrouping makes an untouched relationship read as both added and
removed:

```
endpoint pairs:  added-only 0   removed-only 0   retyped 450 / 52 / 20 / 37
```

**No edge was created or destroyed in any target, in any format.**
Every edge change is a re-typing of an existing endpoint pair, which is
FR-010.

One asymmetry worth stating rather than leaving to be rediscovered:
`python-flask` re-types 123 edges in SPDX 2.3 and **zero** in SPDX 3.
Its propagated scope is `optional`, and SPDX 3's
`LifecycleScopedRelationship.scope` vocabulary is
`design|development|build|test|runtime|other` with no `optional`
member. So the value rides the annotation alone there — the
pre-existing C122 / m179 FR-017 behaviour, consistent with the
catalog, not a parity gap introduced here.

## Reproducibility, before the commit

Step 7 of `docs/development/refreshing-corpus-goldens.md`, in the order
it insists on: a second regeneration of the same tree
(run **#40251448066**) downloaded to a separate fresh directory, then

```
$ diff -r /…/regen /…/regen2 && echo REPRODUCIBLE
REPRODUCIBLE: the two regen artifacts are byte-identical
```

Nothing unmasked is drifting. Run afterwards, a difference here would
mean goldens containing nondeterminism had already been committed.

Both downloads went into fresh empty directories, because
`gh run download` merges into an existing one and a leftover file from
the previous artifact would be silently compared against nothing.

## Step 10 — can the lane still fail?

The procedure requires proving this, because "skipping it is how a
refresh becomes indistinguishable from a disable". It was proven twice
in the course of this work, without a dedicated mutation run:

1. **Lane run #40249731861** failed `3 of 109` — `corpus_haskell_language_server`,
   `corpus_haskell_security_advisories`, `corpus_nix_closure_moat` —
   naming the invariant, the kind, the count and an example component
   each. That is the run that found the placement defect.
2. **The pre-PR gate** failed on `4 undeclared or stale cell(s) in the
   golden matrix`, naming each target and count, when the declarations
   were deleted ahead of the goldens.

Both are real failures of the gate on this branch, not simulated ones,
and both named the target.

**Stated limitation**: neither was a *golden-drift* failure, so neither
exercised the "names all three formats" half of step 10's requirement.
The drift comparison and the invariant matrix are different checks, and
only the invariant half is demonstrated here. A dedicated mutation run
would close that, at the cost of two more lane dispatches; recorded as
not done rather than quietly counted as done.

## T036 — the pre-PR gate, after the goldens landed

```
>>> all pre-PR checks passed.

GATE_NAME=prepr
GATE_EXIT=0
GATE_RESULT=pass
```

`348 targets, 6919 passed, 0 failed`; zero `test result: FAILED` lines
and zero `---- … stdout ----` blocks. Clippy clean. No `+toolchain`
argument, so both halves resolved through `rust-toolchain.toml` (#1189).

The positive signal is the `>>> all pre-PR checks passed.` line, which
`--expect` requires: a gate that exits 0 without printing it fails
here. The first run of this gate exited **101** and is recorded above —
it is what found both the C62 label collision and the
declaration/golden ordering error.
