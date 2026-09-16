# Phase 0 — Research

Every claim re-verified against `main` at `3ad457ae` with a binary built from
it. Where a figure appears, the command that produced it is given.

---

## R1 — The spec conflicts with shipped behaviour, and needs an amendment

**This is the finding that most affects the plan, and it was not visible
during clarification.**

FR-003a says classification "MUST NOT be inferred from the resolve's name or
from the path of its lockfile." A classifier doing exactly that already
ships: `pants/resolve_classifier.rs` (m223) matches the resolve name against
a hardcoded `DEV_RESOLVE_NAMES` allowlist —

```
black ruff isort yapf autopep8 flake8 mypy pyright pyre pytest unittest nose
coverage coveragepy bandit safety sphinx docs lint test dev ci check tools
```

— and assigns `LifecycleScope::Development`, defaulting everything else to
`Runtime`. Its module doc records this as a deliberate Q1 decision and
acknowledges the heuristic can "misfire on a custom resolve name".

**It is misfiring right now, on the measured target.** The allowlist contains
`coverage` and `coveragepy`; the resolve is named `coverage-py`. Measured
classification on `lablup/backend.ai`:

| resolve | components | today | `install_from_resolve` declares it? |
|---|---:|---|---|
| `python-default` | 215 | runtime | no |
| `python-kernel` | 16 | runtime | no |
| `pytest` | 8 | development | **yes** |
| `black` | 3 | development | **yes** |
| `towncrier` | 3 | runtime | no |
| `coverage-py` | 1 | **runtime — wrong** | **yes** |
| `mypy` | 1 | development | **yes** |
| `setuptools` | 1 | **runtime — wrong** | **yes** |

So FR-003a is right on the evidence — the heuristic is demonstrably wrong on
two of eight resolves here — but it cannot be implemented as literally
written without deleting a shipped classifier, and that would **regress** any
repository whose tools do not declare `install_from_resolve`, where the
allowlist is currently the only signal.

**Decision**: implement declaration-first with the heuristic retained as an
explicitly-reported fallback. `install_from_resolve` is authoritative where
present; the allowlist applies only where nothing declares; and FR-003c's
counter reports how many resolves were classified by fallback rather than by
declaration, so the weaker signal is visible instead of silent.

**This requires amending FR-003a**, which currently forbids the fallback
outright. The amendment should be made before implementation rather than
during it. Recorded here rather than quietly reinterpreted, because "the spec
said X, the code does Y, I picked one" is how a requirement stops meaning
anything.

**Alternatives considered**: delete the allowlist (matches FR-003a literally,
regresses undeclared repos, no measurement supports the trade); keep the
allowlist authoritative and treat `install_from_resolve` as a tiebreak
(leaves the two measured misfires unfixed).

---

## R2 — What already exists, and how little is new

| Capability | Status |
|---|---|
| `[python.resolves]` name→lockfile map | **parsed** (m672, `pants/mod.rs:159`) |
| per-package resolve attribution | **emitted** — `waybill:pants-resolve`, C143, all three extractors |
| resolve→lifecycle classification | **ships** (m223, allowlist-based — see R1) |
| `install_from_resolve` back-reference | **not parsed** — referenced only as an unhandled key in `pants_shell/config.rs` |
| a component representing a resolve | **does not exist** |
| anchoring root → resolve → requirements | **does not exist** |

So FR-005/SC-005 (per-package resolve attribution) is **already satisfied**;
the plan must verify it rather than build it. The genuinely new work is the
resolve component, the anchor edges, and parsing one additional key.

---

## R3 — Identity for a resolve component

A resolve is not installable and has no upstream coordinates, so no ecosystem
PURL type describes it. The established shape for a component that identifies
something real but non-installable is `pkg:generic/` — used by the Go
toolchain component (`pants_go`, `pkg:generic/go@<version>`), by workspace
roots (`workspace.rs`), and by bundler applications.

**Decision**: `pkg:generic/` with the resolve's declared name. Its
non-installable nature is carried by an explicit marker per FR-002a rather
than inferred from the PURL type — a consumer must not try to fetch or
vulnerability-scan it.

**Alternatives considered**: reusing the lockfile path as identity (not
stable across layouts, and names a file rather than a concept); no component
at all with the root anchoring requirements directly (rejected in
clarification — flattens eight resolves into one and asserts a dependency the
root manifest does not state).

---

## R4 — Principle V audit for the new signals

| Signal | Native field? | Disposition |
|---|---|---|
| resolve → package membership | — | **already shipped** as C143; no new row |
| build-time vs runtime classification | **yes** | `LifecycleScope` already exists and is already emitted; this feature changes how it is *derived*, not how it is expressed. No new vocabulary. |
| "this component is a resolve, not a package" (FR-002a) | none | bridge required |
| count classified by fallback rather than declaration (FR-003c) | none | bridge required |

Two bridges, both document- or component-scope, both needing a catalogue row
**and** a matching `parity/extractors/mod.rs::EXTRACTORS` entry in the same
change — `every_catalog_row_has_an_extractor` and `holistic_parity` fail
otherwise.

The second bridge is the same shape as the unresolved-count added in m867
(C159) and should follow it: always emitted, including zero, so "no fallback
was used" and "the field is missing" stay distinguishable.

---

## R5 — Reachability and the self-report (FR-009)

Today the document reports `orphaned-components-detected: 271 component(s)
not reachable from root`, and `1 of 331` components are reachable.

The completeness classifier runs a BFS from the root over the emitted edges,
so anchoring feeds it directly: no separate update is needed and FR-009 is
satisfied by construction. What the plan must **verify** is that the count
actually falls and does not merely change shape — the same trap as m866,
where a graph looked complete because fabricated edges had made everything
reachable.

**Decision**: assert the reachable count and the orphan count against the
emitted document, never against the classifier's own report.

---

## R6 — Performance

The change adds a small number of components (8 on the measured target) and
one edge per resolve plus one per top-level requirement (~99 here). That is
work proportional to output, not a new traversal: identifying top-level
requirements is one pass over edges already in memory, and the resolve→package
grouping is already computed for C143.

**Baseline** (`lablup/backend.ai`, 331 components, release build at
`3ad457ae`, after a warm-up run): **781 / 778 / 794 ms**.

**Budget**: within run-to-run noise, confirmed by an **interleaved** A/B
measurement rather than against a separately-taken baseline. This is not
pedantry — in milestone 867 a separately-taken comparison showed a false ~3%
regression that vanished when the two binaries were interleaved on identical
machine state.

---

## Deferred

- Whether other ecosystems have unanchored resolve graphs. Plausible, but no
  measurement supports it and this feature does not assert it.
- Widening `DEV_RESOLVE_NAMES`. Adding `coverage-py` and `setuptools` would
  patch the two measured misfires without addressing why a name allowlist is
  the wrong instrument. R1's declaration-first approach makes the allowlist
  matter less rather than more.
