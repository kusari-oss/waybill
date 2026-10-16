# Measurements — universal semantic-invariant layer

Everything in the spec that states a number comes from here. Re-run the
probe before trusting any of it: the corpus moves, and an invariant that
holds today is exactly the kind of thing that stops holding quietly.

## Probe

`invariants.py` — reads emitted CycloneDX + SPDX 2.3 pairs from a
directory tree and evaluates seven invariants against every pair,
printing the matrix.

```sh
# against any directory of <target>/<name>-{cdx,spdx}.json pairs
python3 -I invariants.py
```

It is deliberately standalone Python rather than Rust, and deliberately
shares no code with waybill. An instrument that imports its subject
applies the subject's bugs to the measurement — the same reasoning that
keeps `xtask/compare` off `waybill-common` in milestone 780.

## Corpus used for the evidence

**Nine SBOM pairs from the Pico Waybill Nightly**, run of 2026-10-15,
produced by waybill 0.10.1 (`metadata.tools`), scanning the targets in
`app-code/pico/pkg/software/testdata/waybill-nightly/targets.json`.

These were used because they were already emitted and sitting in CI
artifacts. **They are not the 17-target public corpus this feature
changes.** Six are source scans (chatwoot, jellyfin-web, kubernetes,
pydantic, starship) and four are image scans (kube-proxy, node-alpine,
python-slim, ubi9-minimal). The delivered matrix will have different
dimensions and different cell states; the eight public-corpus targets
not represented here are **unmeasured**.

## The matrix

```
                                chatwoot jellyfin kube-proxy kubernetes node-alpine pydantic python-slim starship ubi9
I1 no dependsOn->file-tier          PASS     PASS      FAIL       PASS       FAIL      PASS      FAIL      PASS   FAIL
I2 identity unique                  PASS     PASS      FAIL       FAIL       PASS      PASS      PASS      PASS   PASS
I3 non-runtime propagated           PASS     PASS      PASS       PASS       PASS      FAIL      PASS      PASS   PASS
I4 cross-format root parity         PASS     PASS      PASS       PASS       PASS      PASS      PASS      PASS   PASS
I5 complete means complete          PASS     PASS      PASS       PASS       PASS      PASS      PASS      FAIL   PASS
I6 one unknown spelling             PASS     PASS      PASS       PASS       PASS      FAIL      PASS      PASS   PASS
I7 no silent orphans                FAIL     PASS      PASS       FAIL       PASS      FAIL      PASS      PASS   PASS

12 failing cells of 63
```

Failure detail as printed:

| invariant | target | observation |
|---|---|---|
| I1 | kube-proxy | 6 edges target file-tier components; e.g. `-> kmod` |
| I1 | node-alpine | 7 edges target file-tier; e.g. `-> APKINDEX.630cb808.tar.gz` |
| I1 | python-slim | **82** edges target file-tier; e.g. `-> passwd.postinst` |
| I1 | ubi9-minimal | 1 edge targets file-tier; `-> libnssckbi.so.x86_64` |
| I2 | kube-proxy | 1 duplicated identity; `kmod` ×2 |
| I2 | kubernetes | 14 duplicated identities; worst `util.sh` ×9 |
| I3 | pydantic | 103 dev-only transitives unmarked; e.g. `ast-serialize@0.6.0` |
| I5 | starship | verdict=complete but 6 unreachable (6 file-tier) |
| I6 | pydantic | 2 packages with mixed unknown-version spellings; e.g. `pydantic ['(absent)', '0.0.0-unknown']` |
| I7 | chatwoot | 17 unreachable and unexplained; e.g. `ffi` |
| I7 | kubernetes | 6 unreachable and unexplained; e.g. `github.com/Masterminds/semver/v3` |
| I7 | pydantic | 120 unreachable and unexplained; e.g. `pydantic` |

## What the matrix found that per-target reading had not

The two I7 cells are the reason this feature exists. Both documents had
already been read in detail — including a full per-target sweep that
reported counts for every one of the nine — and neither defect was
identified.

**chatwoot**, 17 unexplained-unreachable: 14 gem (`Gemfile.lock`) + 3
npm (`pnpm-lock.yaml`). The gem ones carry the platform in the version
segment:

```
pkg:gem/ffi@1.17.2                            outdeg=0
pkg:gem/ffi@1.17.2-arm64-darwin               outdeg=0
pkg:gem/ffi@1.17.2-x86_64-darwin              outdeg=0
pkg:gem/grpc@1.72.0  / @1.72.0-arm64-darwin  / @1.72.0-x86_64-darwin
pkg:gem/libdatadog@36.0.0.1.0  (+ 2 platform variants)
```

`1.17.2-arm64-darwin` is not a version that exists, so advisory matching
misses two of every three copies — a silent correctness failure, not a
tidiness problem. purl has a `platform` qualifier for this. Filed as
#1211.

**kubernetes**, 6 unexplained-unreachable: all `pkg:golang`, all sourced
from `go.sum` only, all `hack/tools` build tooling (`golang.org/x/exp`,
`golang.org/x/mod`, `github.com/google/pprof`,
`github.com/rogpeppe/go-internal`).

Taken with the pydantic cell, one defect class — *non-production or
non-selected dependencies are neither marked nor anchored* — is live in
**four ecosystems**: uv, gem, Go, npm. Four prior per-target
investigations had each found one of the four.

## Known limitations of this evidence

- **The seven invariants are biased toward defects already known.** They
  were written from findings produced by reading documents, so the
  matrix is a floor on what an invariant layer finds, not a ceiling.
- **I3 and I7 overlap.** The gem and Go instances of the propagation
  class surfaced under I7 ("no silent orphans") rather than I3
  ("non-runtime propagated"), because I3's seed set only reaches
  components downstream of an already-marked component. A defect landing
  in the wrong class sends the fix to the wrong place. FR-019 requires
  restating both.
- **I4 passed everywhere**, which is a weak signal rather than a strong
  one: it is the one property already effectively enforced, so it
  functions here as a control on the probe rather than as a finding.
  Note it was also the invariant whose first implementation was *wrong*
  — ignoring SPDX's inverse `*_DEPENDENCY_OF` forms produced four false
  mismatches before the comparison accounted for them. `jellyfin-web`
  alone carries 2655 `DEV_DEPENDENCY_OF`.
- **No vacuity check was performed.** No invariant was deliberately
  broken on a passing target to confirm the cell goes red. SC-004
  requires that for every invariant in the delivered catalog; it is the
  check this repo has learned to insist on
  (`feedback_a_passing_test_may_exercise_nothing`).

## Ecosystem coverage of the public corpus

Method: the 17 target names in `corpus_harness_195/manifest.rs` against
the reader modules in `waybill-cli/src/scan_fs/package_db/`.

Exercised by at least one target: golang, nix, cargo, npm, pip/uv,
maven, dpkg (via `image-postgres16`), haskell, and the pants resolves
(python, django, jvm/coursier, clojure, golang, javascript).

Readers present with **no** corpus target, counted from the module
listing: gem, composer, dart, elixir, erlang, cocoapods, nuget, swift,
scala, conan, vcpkg, bazel, cmake, gradle, kotlin-dsl, alpm, brew, rpm,
ipk/opkg, apk, helm — on the order of twenty.

This is the structural reason #1211 was unobservable: **no invariant
over the existing 17 targets could ever have caught a gem defect,
because the corpus contains no gem.** A green matrix therefore needs the
coverage statement of FR-015 beside it, or it reads as far more
assurance than it is.

---

# Implementation measurements (2026-10-15)

Everything above was measured with the Python probe against nine Pico
nightly SBOM pairs. Everything below was measured by the delivered Rust
layer against all **seventeen** corpus targets, and supersedes the
probe's figures wherever the two disagree.

## The corpus run is hermetic — the plan assumed it was not

`plan.md` assumed the matrix could only run in the dispatched corpus
lane, because that is where documents get emitted. That turned out to be
false in a way that matters.

The 17 committed golden triples under
`waybill-cli/tests/fixtures/public_corpus/<target>/` are complete:
measured directly, they carry `dependencies[]` (264 entries for
`image-postgres16`), every `waybill:*` component property, document-scope
`waybill:graph-completeness`, and both subject pointers
(`metadata.component.bom-ref` for CycloneDX, `documentDescribes` for
SPDX 2.3). Layer 2's masking rewrites only the workspace path and masks
hashes, serial numbers and timestamps — **none of which any of the seven
invariants reads**.

So the full matrix, the gate, the declarations file and the mutation
check all run under `cargo test --workspace`, with no network and no
container pulls. The dispatched lane evaluates freshly emitted documents
in addition. The two are complementary: goldens catch a refresh that
imports a defect, the lane catches a defect before a refresh hides it.

## The first full matrix — 17 targets, 7 invariants, 119 cells

```text
invariant matrix — . pass  X fail  - n/a  E error  (d) declared

target                        I1  I2  I3  I4  I5  I6  I7
go-cobra                       -   .   -   .   -   -   .
go-opentelemetry               .   .   .   .   -   .   d
nix-closure-moat               -   .   d   .   -   .   .
rust-ripgrep                   .   .   d   .   d   .   .
npm-express                    .   .   .   .   -   .   .
python-flask                   -   .   d   .   -   -   d
maven-guice                    .   d   .   .   -   .   d
image-postgres16               d   .   -   .   -   .   .
pants-example-python           d   .   -   .   .   .   .
pants-example-django           d   .   -   .   .   .   .
pants-example-jvm              d   .   -   .   .   .   .
pants-clojure-polyglot         -   .   -   .   .   .   .
pants-example-golang           .   .   -   .   -   .   .
pants-example-javascript       -   .   .   .   .   -   .
haskell-aeson                  .   .   .   .   -   .   .
haskell-security-advisories    -   .   d   .   -   .   d
haskell-language-server        .   .   d   .   -   .   d

no lane failures

applicability (targets whose precondition was met — FR-015a):
  I1: 11/17
  I2: 17/17
  I3: 10/17
  I4: 17/17
  I5: 6/17
  I6: 14/17
  I7: 17/17

ecosystem coverage — 11 of 19 documented ecosystems exercised
  no corpus target exercises: apk, gem, kotlin, nuget, pants (shell), rpm, swift, yocto
  a passing matrix says nothing about those; see docs/ecosystems.md
```

**16 failing cells, not 12.** The probe's twelve were measured on nine
targets; eight of the seventeen were unmeasured, which is why `FR-018`
and `SC-006` were rewritten during analysis to anchor on "every failing
cell in the first full-corpus run" rather than on a literal count. All
16 are declared in
`waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`.

| Invariant | Failing targets | Tracked by |
|---|---|---|
| I1 file-tier dependency edges | image-postgres16 (117), pants-example-django (1), pants-example-jvm (1), pants-example-python (1) | #1206 |
| I2 duplicate identity | maven-guice (7 `pom.xml` file components share one identity) | #1208 |
| I3 unpropagated non-runtime scope | python-flask (51), haskell-language-server (26), haskell-security-advisories (13), nix-closure-moat (9), rust-ripgrep (2) | #1209 |
| I5 `complete` over unreachable components | rust-ripgrep (7) | #1207 |
| I7 unreachable and unexplained | python-flask (23), maven-guice (5), haskell-security-advisories (3), haskell-language-server (2), go-opentelemetry (1) | **none — needs an issue** |

**I7's class is new.** It is not #1206-#1211. The population is
design-tier, toolchain and flake-input components emitted by readers
that post-date milestone 167's `orphan-reason` vocabulary and milestone
194's anchoring passes: `pkg:bazel/*` archives read from `WORKSPACE`,
`pkg:generic/ghc-*@unspecified` compiler pins, `pkg:github/*` flake
inputs, and uv.lock alternate-resolution forks. Verified individually
rather than inferred — `pkg:pypi/alabaster@0.7.16` in `python-flask` is
`sbom-tier: source`, sourced from `uv.lock`, with no edge and no
`orphan-reason`, sitting alongside a reachable `alabaster@1.0.0`. Those
five rows carry no `issue` field, which is why the field is optional.

## Two invariants pass everywhere, and that needed proving

`I4` is applicable on 17/17 and fails nowhere; `I6` on 14/17 and fails
nowhere. A clean row is exactly what a check that exercises nothing
looks like, so both were mutated (SC-004): dropping one edge from the
CycloneDX subject's `dependsOn` reddens `I4` with
`root-direct-only-in-spdx`, and giving one package two sentinel
spellings reddens `I6`. All seven invariants have a mutation test.

**The I6 mutation caught a bug in itself first.** Its first version
picked an existing component with a sentinel and added a second
component with `v0.0.0-unknown` — but on `go-opentelemetry` the component
it picked *already* had `v0.0.0-unknown`, so the mutation added the same
spelling twice and correctly did not fail. The test was wrong, not the
checker. It now adds both spellings itself, and a companion negative
control asserts that `v0.0.0-20260928230214-8a89bd6388cc` (a Go
pseudo-version) and `0.0.0.0` (a .NET version) are **not** sentinels — a
substring match on `0.0.0` would flag roughly forty innocent components
across the corpus.

## SC-003 demonstrated

Perturbing exactly two targets and diffing the two artifacts names
exactly those two cells and nothing else:

```sh
diff <(jq -S '[.cells[]|select(.state=="fail")|{invariant,target}]' target/invariants/run-goldens.json) \
     <(jq -S '[.cells[]|select(.state=="fail")|{invariant,target}]' target/invariants/run-goldens-mutated.json)
# > I7/go-cobra
# > I7/npm-express
```

Asserted in
`invariants::mutation_tests::a_matrix_diff_shows_which_targets_a_change_reached`.
A mutation is the inverse of a fix and exercises the same diff, which is
how the criterion is met without importing a fix that spec Assumption 1
puts out of scope.

## FR-019 — how I3 and I7 were disjointed

The probe's I3 seeded from already-marked components, so a population
that was never marked at all produced no seed and fell through to I7.
The restatement makes the split structural rather than incidental:

- **I3**'s population is components that *are* reachable from the subject, by a path that is entirely non-runtime. Precondition: the document declares at least one non-runtime edge.
- **I7**'s population is components not reachable from the subject at all. Precondition: subject resolvable and at least one non-subject component.

Every component is reachable or not, so a defect satisfies exactly one
of the two preconditions.

## The ecosystem universe was missing two rows

`docs/ecosystems.md`'s coverage matrix had 17 rows and no `haskell` or
`nix` row, while five of the seventeen corpus targets exercise those
readers (four Haskell, and `flake.lock` on three). `EcosystemId` is
gated bidirectionally against that table, so the gap was a hard
blocker rather than a quiet omission — which is the gate working.

Both rows were added from the readers' actual behaviour, not from their
advertised shape: `haskell.rs` matches six basenames and emits no
content hash; neither `pkg:hackage/` nor `pkg:generic/` is indexed by
deps.dev (six systems) or ClearlyDefined (six coordinate types), so both
enrichment columns are `—`.

Coverage now reads 11 of 19 documented ecosystems, with
`apk, gem, kotlin, nuget, pants (shell), rpm, swift, yocto` uncovered —
`gem` among them, which is why #1211 was unobservable here (SC-005).

**Residual gap, stated rather than discovered**: roughly 40 reader
modules exist under `waybill-cli/src/scan_fs/package_db/` and the matrix
documents 19. `maven-guice` demonstrably trips the bazel reader
(`pkg:bazel/*` from `WORKSPACE`) and cannot say so, because bazel has no
matrix row. The report is as complete as the document is, and FR-015's
derive-don't-hand-list rule is what makes it follow the document the day
more rows land.

## US3 — the expectations audit (T051)

Per-function assertion inventory, extracted from the source rather than
read by eye:

| Function | Assertions | Universal candidates |
|---|---|---|
| `go_cobra_expectations` | 4 | 0 |
| `go_opentelemetry_expectations` | 3 | 0 |
| `nix_closure_moat_expectations` | 0 (uses its own failure constructor) | 0 |
| `rust_ripgrep_expectations` | 2 | 0 |
| `npm_express_expectations` | 2 | 0 |
| `python_flask_expectations` | 2 | 0 |
| `maven_guice_expectations` | 2 | 0 |
| `pants_example_python_expectations` | 2 | 0 |
| `pants_example_django_expectations` | 3 | 0 |
| `pants_example_golang_expectations` | 2 | 0 |
| `pants_clojure_polyglot_expectations` | 4 | 0 |
| `pants_example_jvm_expectations` | 5 | 0 |
| `pants_example_javascript_expectations` | 4 | 0 |
| `image_postgres16_expectations` | 3 | 0 |
| `haskell_aeson_expectations` | 5 | 0 |
| `haskell_security_advisories_expectations` | 4 | 0 |
| `haskell_language_server_expectations` | 5 | 1 (see below) |
| `layer0_document_integrity` | 3 | **3 — already universal** |

**The migration set is empty, and that is the finding.** Three check
names repeat across functions, and all three are target-specific on
inspection:

- `graph-completeness` (7 functions) asserts a *particular* expected verdict per target. Its universal part is already catalog row I5.
- `pypi-transitives-present` (3) names specific packages per target.
- `pants-resolve-annotation-present` (3) looks universal and is not. Generalising it needs a precondition identifying Pants-lockfile-sourced components, and the only document-level signal for that is the annotation itself — precondition and conclusion collapse into one predicate and the check becomes vacuous. A `waybill:source-files` basename heuristic was measured as a substitute and matches `flake.lock`, `uv.lock` and `Cargo.lock` as well, so it cannot stand in.

### A numbering collision, found and fixed

`layer0_document_integrity`'s three checks were named
`i2-no-dangling-edge-endpoint`,
`i2-no-unknown-spdx-relationship-endpoint` and
`i3-root-out-edge-count-agrees-across-formats`, with diagnostics reading
"invariant I2" and "invariant I3". Once the catalog existed those
numbers meant different properties — catalog `I2` is identity
uniqueness, `I3` is non-runtime scope propagation — so a maintainer
following a layer-0 diagnostic would land on the wrong row. Renamed to
`L2` / `L3`, with the namespace rule stated at the function: `I<n>`
belongs to the catalog alone (FR-001a).

Note that layer 0's `L3` and catalog `I4` are complementary, not
duplicates: `L3` compares root out-edge *counts* across all three
formats, `I4` compares the root's direct dependency *sets* by PURL
across CycloneDX and SPDX 2.3. One catches a missing edge, the other
catches a substituted one.

### I8 candidate, measured and deliberately not landed

`every-versionless-haskell-component-has-a-reason`
(`haskell_language_server_expectations`) generalises cleanly, unlike the
three above. Stated universally: **a component with no version carries
an annotation explaining why.** Measured across all 17 targets:

| | |
|---|---|
| Applicable (≥1 versionless component) | 14 / 17 |
| Failing | 6 |
| Population | `pkg:generic/python-default?pants-namespace=python` and three sibling resolve anchors, `pkg:generic/ghc-9.10.3` and 8 sibling compiler pins, one `pkg:generic/gosu?file-sha256=…` |

Not landed. FR-017 fixes the initial catalog at seven, and this
assertion appears in one function rather than the two T052 scopes
migration to. It is append-only work (FR-001a) and the numbers above are
what a reviewer needs to decide: six more declared cells against one
more class of defect made visible everywhere.
