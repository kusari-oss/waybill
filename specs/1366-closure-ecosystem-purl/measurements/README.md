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

## Open — not yet measured

| item | task | why not yet |
| --- | --- | --- |
| waybill's own confirmed count | T040 | needs the implementation |
| SC-005 end-to-end feed query | T041 | needs the implementation |
| FR-005 / FR-006 byte-identity diffs | T038, T039 | needs the implementation |
| SC-007 mutation check | T028 | needs the tests |
