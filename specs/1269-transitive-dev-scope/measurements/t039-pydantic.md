# T039 — the issue's own target, re-measured (SC-008)

**Date**: 2026-10-25 | **Feature**: `1269-transitive-dev-scope` (#1209)

## Result

| metric | before | after |
|---|---|---|
| components | 310 | 310 |
| components with a non-runtime scope | 76 | **181** |
| **unscoped transitives** (the issue's metric) | **105** | **0** |
| invariant-I3 violations | 2 | **0** |
| C62 `transitive` markers | 0 | 105 |
| reachable from the subject | 110 | 110 |
| runtime-reachable from the subject | 105 | 105 |

SC-008 is met: the count reaches zero, measured on the current build.

The first entries of the before-list are `aiohappyeyeballs`, `aiohttp`,
`aiosignal`, `ast-serialize`, `asttokens`, `async-timeout`, `attrs`,
`auditwheel-emscripten`, `babel`, `backports-tarfile`, `backrefs`,
**`black@24.10.0`** — and `pkg:pypi/black@24.10.0` is the component
#1209 names by hand as "indistinguishable from a runtime dependency".
The metric found the issue's own example without being pointed at it.

## What was measured, and on what

Two binaries, one input:

- **before**: `00b2f37c`, the commit immediately preceding the
  implementation, built in a throwaway `git worktree`.
- **after**: `0687058c`, branch tip.
- **input**: a shallow clone of `pydantic/pydantic` at
  `37dc34b4f001191b4f3db707cc52bac95baf17f0`, scanned `--offline
  --no-deep-hash`, CycloneDX only.

```
waybill --offline sbom scan --path <clone> --no-deep-hash \
  --format cyclonedx-json --output cyclonedx-json=<out>
```

The after-run's own log line, which is the feature declaring what it
did on this document:

```
propagated non-runtime lifecycle scope to transitive dependencies
  immune=104 seeded_from=76 assigned=105 by_scope={"optional": 105}
```

`assigned=105` and the 105 C62 markers in the emitted document agree,
and 76 + 105 = 181 accounts for every marked component.

## This is not the pin #1209 quoted

The issue observed **103** with waybill 0.10.1 against an external pin
that is not one of the 17 corpus targets. That pin is not reproducible
here, so this is a current `main` of the same project measured on the
current build. The spec says the 103 is provenance, not a target, and
this is why: 105 against 103 is not a discrepancy to explain, it is two
different commits of a moving repository.

What makes the two comparable is that the component total is identical
(306 package components in both the issue's note and this run, 310
including the four non-package entries the script counts) and the
metric is the same one.

## The script, and why a zero here is not a vacuous pass

Both scripts are committed beside this file:
`i3.py` (the invariant-I3 predicate) and `unscoped.py` (the issue's
metric). A zero from a broken script and a zero from a clean document
look identical, so the predicate was controlled before it was trusted.

`i3.py` run against the **committed pre-change corpus goldens**
reproduces the declared counts exactly:

| golden | `i3.py` | declared in `invariant-declarations.toml` |
|---|---|---|
| `rust-ripgrep` | 2 | 2 |
| `python-flask` | 51 | 51 |
| `npm-express` | 0 | (no declaration — passes) |

It is the same predicate the harness implements
(`corpus_harness_195/invariants/checks/scope_propagation.rs`),
re-expressed, and it agrees with the harness on three documents
including one that passes. So it can distinguish a violating document
from a clean one, which is the property a zero needs.

`unscoped.py` has its own control in the table above: it reads **105**
on the before-document and **0** on the after-document, same script,
same input, two binaries. A script that always returned zero could not
have produced the 105.

## I3 says 2, the issue says 105 — both are right

The invariant-I3 count on this document is **2 before** (`dnspython`,
`idna`), not 105, and that is not a contradiction.

I3's population is components **reachable from the subject** by a path
that is entirely non-runtime. The issue's population is components
reachable from **any** declared non-runtime component, whether or not
the subject reaches them. pydantic's dev closure is mostly *unreachable*
from the subject — 196 of its 306 components are orphans — so it falls
to I7, not I3.

This is the population A / population B split the spec names, measured:
I3 sees population A (2 components here), the issue's metric sees both
(105). Seeding from declared non-runtime members rather than from the
roots is what covers both, and this is the document that shows why that
design choice mattered: a root-seeded pass would have fixed 2 of 105
here.
