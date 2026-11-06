# Measurements — milestone 1366 (#1365)

**Date**: 2026-11-06
**Host**: aarch64-darwin
**Closure**: `nix-closure-moat` at commit `d069055`, nixpkgs pinned `a799d3e3`
**Capture**: `nix derivation show --recursive <moat>#default` → 1,275 derivations, `"version": 4`

> **These figures are aarch64-darwin.** The corpus goldens pin x86_64-linux,
> which is a **different closure**. A count from one is wrong in membership
> when quoted against the other — an error the closure milestones have made
> repeatedly.

Closure identity was confirmed by content, not by its derivation count alone:
the capture contains `moat-0.1`, `ghc-9.10.3-source` and 83 `python3.13-*`
derivations. A matching total is suggestive; a matching total plus the
expected contents is evidence.

---

## T003 — the candidate population and the collision question

Reproduced with the committed probe:

```sh
python3 -I specs/1276-boot-library-scope/measurements/pypi-identity-probe.py closure.json
```

| quantity | value |
| --- | --- |
| closure derivations | 1,275 |
| `pname` in `env` only | 594 |
| `pname` in `structuredAttrs` only | **66** |
| no `pname` in either carrier | 615 |
| PyPI candidate derivations | **83** |
| distinct candidate names | **75** |
| names needing PEP 503 normalisation | **0** |
| unique `(normalised name, version)` pairs | **75** |

**R8 — collisions after normalisation: zero.** No two distinct `pname`s
normalise to one coordinate, and no candidate name appears at more than one
version. Validation rule **V5 is therefore defensive on this closure, not
load-bearing**. It stays in the implementation — a different closure can
contain a collision, and the rule costs one comparison — but no test on this
population exercises it, and none should claim to.

The 83 → 75 gap is build variants, which the existing merge already collapses.

**FR-004 (PEP 503 normalisation) is a no-op on this closure**: all 75 `pname`s
are already in normalised form. The requirement is still correct — nixpkgs is
not obliged to store the normalised spelling — but **no component here
exercises it**, so its test must be synthetic (task T026) and no measurement
may be cited as evidence that it works.

---

## Confirmation: what deps.dev actually answers

```sh
python3 -I specs/1366-closure-ecosystem-purl/measurements/depsdev-confirmation-probe.py closure.json
```

The probe issues the same request `DepsDevClient::get_version` issues, at the
same URL shape (`deps_dev_client.rs:190`).

| outcome | count | maps to |
| --- | --- | --- |
| HTTP 200 — **found** | **75** | `Confirmed` |
| HTTP 404 — **absent** | **0** | `Declined` |
| other — **failed** | **0** | `Unchecked` |

### Probe controls

A uniform result across cases that should differ usually means the harness,
not the system. Two negative controls were run before the result was believed:

| control | expected | observed |
| --- | --- | --- |
| `waybill-fixture-definitely-not-a-project@1.0.0` | 404 | **404** |
| `urllib3@99.99.99` (real project, impossible version) | 404 | **404** |

The probe can report a decline. It reported none because there were none.

---

## ⚠️ This falsifies two claims in the spec

The spec's Clarifications section states:

> *"`librt` has no published project of that name and `roman-numerals` is
> published under a different one; both fail to resolve and are declined,
> which is US2 made true by construction rather than hoped for."*

**Both halves are false.** Measured:

| coordinate | result | what deps.dev holds |
| --- | --- | --- |
| `librt@0.9.0` | **found** | `github.com/mypyc/librt` |
| `roman-numerals@4.1.0` | **found** | `github.com/AA-Turner/roman-numerals` |

Both appear to be *correct* matches rather than false positives: the closure
also contains `mypy` and `mypy-extensions` (librt is mypyc's runtime), and
`sphinx` (AA-Turner is a Sphinx maintainer). So confirmation is not accepting
the wrong project here — there was simply never a wrong project to reject.

The earlier claim was almost certainly checked against a plain PyPI name
lookup, or inferred, rather than measured against a version-exact query.
`librt@1.0.1` — a version the closure does not contain — **does** 404, which is
how a version-shaped guess produces a confident wrong answer.

### Consequences, stated rather than smoothed over

1. **SC-001's measured answer is 75 of 75.** Every candidate confirms. This is
   a probe of deps.dev, not a run of waybill; the implementation must
   reproduce it (task T040), and only that run may be quoted as the feature's
   result.
2. **US2 has no witness on this population.** Existence-confirmation declines
   nothing here, so it cannot be demonstrated against the real closure. Its
   tests are synthetic by necessity (T022's wiremock 404 arm), which the task
   list already specifies.
3. **The safety of the feature rests on the candidate rule, not on
   confirmation.** On this closure, confirmation is a filter that filters
   nothing. The spec's clarification priced the network dependency against a
   benefit this measurement values at zero for the measured population. That
   is a decision worth re-taking with the real number rather than inheriting.
4. **The spec's residual-risk Assumption is now the whole risk**, not a corner
   case: *"a name that is both a published project and a plausible local one
   would pass."* Nothing measured contradicts it, and nothing measured
   supports the mitigation that was supposed to bound it.

---

## The production rule agrees with the probe

The numbers above come from a Python probe. The reader is Rust. Two
implementations of one rule can agree by coincidence or disagree silently, so
`waybill debug closure-dump` was extended to report the candidate population by
**calling the production predicate** (`ecosystem_identity::is_pypi_candidate`)
rather than by retyping its regex:

```sh
cargo run -q --bin waybill -- debug closure-dump --closure-json closure.json --summary-only
```

| quantity | probe (Python) | reader (Rust) |
| --- | --- | --- |
| derivations | 1,275 | 1,275 |
| `pname` in `env` only | 594 | 594 |
| `pname` in `structuredAttrs` only | 66 | 66 |
| no `pname` | 615 | 615 |
| PyPI candidate derivations | 83 | **83** |
| distinct candidate identities | 75 | **75** |
| closure members (FR-005 protects) | — | 539 |

Exact agreement on every shared quantity. The 539 member identities are the
population SC-003 requires be byte-identical after the change.

---

## deps.dev normalises the name itself

Measured, because the choice of normalisation would otherwise be an assumption
in the lookup path:

| asked as | result | name echoed back |
| --- | --- | --- |
| `zope.interface@8.2` | 200 | `zope-interface` |
| `zope-interface@8.2` | 200 | `zope-interface` |
| `zope_interface@8.2` | 200 | `zope-interface` |
| `ruamel.yaml@0.18.6` | 200 | `ruamel-yaml` |

All spellings resolve, and the registry echoes the PEP 503 form. **The
normalisation choice therefore cannot affect confirmation**, which removes it
as a risk on the lookup path entirely.

It still affects the *emitted* PURL, and there the governing constraint is
cross-reader consistency: milestone 674 FR-015 requires the pip reader and any
other reader to spell one package one way. So the emitted identifier uses
`pip::normalize_pypi_name_for_purl`, the same helper the pip reader uses —
which implements the purl-spec rule (lowercase, `_` → `-`) and not full
PEP 503 (which also folds `.` and collapses runs). No candidate on this
closure contains a `.` or `_`, so the two agree on all 75 and nothing here
exercises the difference.

---

## SC-007 — the mutation check

*"A test that cannot fail proves nothing."* Two mutants, each run against the
five tests in `scan_cmd::closure_ecosystem_identity_tests`:

| mutant | what it breaks | result |
| --- | --- | --- |
| `LookupAnswer::Found => Declined` | confirmation never succeeds | **1 of 5 red** — the US1 test |
| `declared_pname` reads `env` only | the `structuredAttrs` carrier | **4 of 5 red** |

The first is correctly narrow: three of the five tests assert that nothing
moves, and a mutant that confirms nothing cannot disturb them. The second is
broad because the carrier is load-bearing everywhere — which is the point, and
is why the fixture's `waybill-fixture-gamma` declares its `pname` only there.

Reverted, all five pass; full bin suite 4,653 passed, 0 failed.

### One test was wrong before the code was

The FR-012 control first started a `MockServer`, mounted nothing and dropped
it before the call, expecting a transport failure. It measured three
**declines**, because the server was still answering and an unmounted path is
a 404 — the test would have asserted the opposite of its own purpose. A port
nothing listens on produces the failure it was after. A probe can lie about
the system; uniform results across cases that should differ mean the harness.

Likewise the ordering test's control fired on its first run: the edge set was
empty, because the fixture's root is `unreferenced` and so never emitted,
leaving its edges unanchored. The fixture gained an intermediate library that
*is* emitted, rather than the assertion being relaxed.

---

## Golden impact (T043, T044)

Read from the committed goldens rather than predicted:

| | `nix-closure-moat` golden (x86_64-linux) |
| --- | --- |
| components | 1,487 |
| `pkg:generic` | 1,326 |
| `pkg:hackage` | 160 |
| `pkg:github` | 1 |
| C204 `waybill:nix-closure-read` | present |
| C208 `…-identity-outcome` | **absent** (pre-change) |

**Note these are not the aarch64-darwin figures above.** That closure has 942
components of which 781 are generic; this one has 1,487 of which 1,326 are.
They are different closures of the same project on different systems, and a
count from one is wrong in membership against the other.

**Expected change: exactly one document-scope addition per format**, and no
component change at all. The corpus harness passes `--no-deps-dev`
(`harness.rs:218`), so the pass records `reason: "disabled"` and confirms
nothing. The candidate count for *this* closure is not predicted here — it
would require running the rule against the x86_64-linux closure, which has not
been captured. The refresh run will report it.

The other 16 targets never run a closure, so C208 is absent from them and
their goldens are untouched (SC-008).

---

## SC-001 — the measured answer, from a waybill run

```sh
git clone https://github.com/MercuryTechnologies/moat  # pinned d069055
target/debug/waybill sbom scan --path <moat> \
  --nix-closure --nix-closure-attr default \
  --format cyclonedx-json --output cyclonedx-json=moat.cdx.json
```

C208, read out of the emitted document:

```json
{"schema":"waybill-closure-ecosystem-identity/v1","ecosystem":"pypi",
 "candidates":74,"confirmed":74,"declined":0,"unchecked":0}
```

| | before | after |
| --- | --- | --- |
| components | 942 | 942 |
| `pkg:pypi` | **0** | **74** |
| `pkg:generic` | 781 | **707** |
| `pkg:hackage` | 160 | 160 |
| `pkg:github` | 1 | 1 |

781 − 74 = 707 exactly, and the component count is unchanged: the pass moved
identifiers and added nothing (Constitution XII constraint 1).

### Why 74 and not the probe's 75

The probe and the reader both count **75** candidate identities; the scan
reports **74 components**. The difference is one derivation, and it is not a
defect in either:

```text
python3.13-build-1.4.4   declared_pname=build  c182_role=unreferenced
                         pypi_candidate=true   emits_as_component=FALSE
```

`build 1.4.4` is `unreferenced`, so the closure emits **no component for it at
all**. It is part of the separate pname-bearing coverage gap — issue #1366,
which this spec lists under *Out of scope* — rather than of this feature's
population. An identity pass can only change the identity of a component that
exists.

This is incidental confirmation that the two issues really are independent, as
the spec claimed: closing the coverage gap would give `build` a component, and
only then would this pass re-identify it.

The skip now emits a `tracing::debug!`, so the 75-vs-74 difference is
recoverable from a scan rather than only from `waybill debug closure-dump`.

## SC-005 — a feed can match them

Queried by ecosystem-qualified identifier against the emitted document:

| name | identifier found |
| --- | --- |
| urllib3 | `pkg:pypi/urllib3@2.6.3` |
| requests | `pkg:pypi/requests@2.33.1` |
| lxml | `pkg:pypi/lxml@6.0.2` |
| jinja2 | `pkg:pypi/jinja2@3.1.6` |
| certifi | `pkg:pypi/certifi@2026.01.04` |

All five previously answered nothing, because `pkg:generic` names no
ecosystem.

## C207, as emitted

CycloneDX, on the `urllib3` component — native, and with **no** duplicate
property (the suppression holds):

```json
"evidence": {"identity": [{
  "field": "purl",
  "confidence": 0.8,
  "concludedValue": "pkg:pypi/urllib3@2.6.3",
  "methods": [
    {"technique": "instrumentation", "confidence": 0.95},
    {"technique": "manifest-analysis", "confidence": 0.8,
     "waybill-closure-ecosystem-evidence": "versioned-interpreter-prefix"}
  ]}]}
```

---

## SC-003, SC-004, SC-006 — compared, not argued

`before` is a binary built from the pre-change commit `255702d9` in a git
worktree; `after` is this branch. Both runs set `WAYBILL_FIXED_TIMESTAMP`, so
the only differences left are semantic. Harness:
`measurements/compare-before-after.py`.

```sh
python3 -I compare-before-after.py base-on.cdx.json new-on.cdx.json \
                                   base-off.cdx.json new-off.cdx.json
```

| criterion | population | result |
| --- | --- | --- |
| **SC-003** / FR-005 | closure members | **539 before, 539 after, PASS** |
| **SC-004** / FR-006 | components from other readers | **25 before, 25 after, PASS** |
| **SC-006** | every component, confirmation unavailable | **identical, PASS** |

The 539 is the figure the spec predicted for the members `pkg:generic` is
correct for, reproduced exactly by an independent before/after diff.

**What changed, with confirmation on**: 74 components newly carry `pkg:pypi`,
and the component count is 942 both before and after.

**What changed, with confirmation unavailable** (`--no-deps-dev`): nothing but
one added document property. No component differs, none is added or removed,
and no other document property is added, removed or changed:

```json
{"schema":"waybill-closure-ecosystem-identity/v1","ecosystem":"pypi",
 "candidates":74,"confirmed":0,"declined":0,"unchecked":74,"reason":"disabled"}
```

That is SC-006 exactly: today's document, plus a record saying confirmation
did not run. It is also the shape the corpus golden will take, since the
harness passes `--no-deps-dev`.

---

## Open — not yet measured

| item | task | status |
| --- | --- | --- |
| corpus golden refresh | T043, T044 | pending the `public-corpus.yml` dispatch |
