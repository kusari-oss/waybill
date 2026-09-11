# Phase 0 Research: Private comparative benchmark harness

**Feature**: 780-comparative-bench-harness
**Date**: 2026-09-09

Findings below come from this repository, from tooling already installed on
this machine, and from measurements taken during the ad-hoc comparison that
motivated the feature. Where a number is an observation it says so; where it
is inference it says that too.

---

## R1 — Wall-clock is not a usable comparative metric on a developer machine

**Decision**: Do not gate on absolute wall-clock. Report **ratios measured
within a single interleaved session**, and treat absolute timings as
context rather than as the comparison.

**Evidence — this project already knew.** `xtask/corpus/quality-corpus.toml`
records, against the cargo target:

> Widest wall-time spread observed (2.6x between two runs on identical hardware).

and consequently sets `wall_ms = { min = 5, max = 5000 }` — a thousand-fold
band. m770 kept wall-time only as a smoke check and abandoned it as a gated
metric. That conclusion was reached independently, before this feature, and
it is the strongest single input to this design.

**Corroborating observation from the motivating episode**: identical
`waybill --offline` invocations on the same kubernetes tree measured 13.95s
(median of 3) and 21.3s (single clean run) — 1.5× apart with nothing
flagging it.

**Why ratios help, and how much.** Within one session a slow machine slows
every tool. Measured across three sessions comparing waybill against another
SBOM generator on the same tree (figures held in the private run records,
not reproduced here — the repository is public):

- waybill's absolute timing moved **1.53×** between sessions
- the waybill-to-other-tool ratio moved **1.38×** across the same sessions

**Ratios are more stable but not dramatically so** — worth stating plainly
rather than overselling, because a design justified by "ratios are stable"
would be resting on a 1.38× wobble.

**Consequence**: interleave. The motivating benchmark ran AAA then BBB, so
any thermal or background drift between blocks biased the comparison
directly. Paired interleaving (A,B,A,B…) cancels first-order drift and is
the single cheapest methodological fix available.

**Alternatives rejected**: gating on absolutes (m770 already demonstrated it
does not work); requiring a reference-class host for every run (conflicts
with R6 — see below).

---

## R2 — Count metrics are exactly reproducible and need no tolerance

**Decision**: Apply the spread gate to timings only. Coverage and accuracy
metrics are compared for exact equality across repeats.

**Evidence**: `quality-corpus.toml` again —

> pkgs / files / edges — fully deterministic: pinned SHA + `--offline` + no
> sampling means the only thing that moves them is a waybill change.
> Verified identical across two runs and across sbomqs v2.0.5 and v2.0.6.

This considerably simplifies the harness. The statistical machinery — repeat
counts, medians, spread gates — is needed for **one** metric family. The
metrics that actually answer "is waybill more accurate" are deterministic and
can be asserted, not estimated.

It also means FR-002's tolerance is a timing concept, and applying it to
counts would be wrong: a coverage figure that varies between repeats is a
defect, not noise, and should fail rather than be averaged.

---

## R3 — Reusable infrastructure already exists

**Decision**: Build `xtask compare` on the existing modules rather than
starting fresh.

| Need | Existing | Location |
|---|---|---|
| Pinned-SHA shallow target fetch + cache | m770 | `xtask/src/quality/fetch.rs` |
| TOML corpus config with pinned targets | m770 | `xtask/src/quality/config.rs` |
| Run schema, metadata, atomic writes | m669 | `xtask/src/bench/schema.rs`, `mod.rs` |
| Host-class classification | m669 | `xtask/src/bench/run.rs::classify_noise` |
| Cross-class comparison guard | m818 | `xtask/src/bench/mod.rs::assert_baseline_is_comparable` |
| Peak-RSS + subprocess timing | m669 | `xtask/src/bench/measure.rs` |

`xtask` currently depends only on `clap`, `sysinfo`, `serde`, `serde_json`,
`chrono`, `tempfile`. It does **not** depend on `waybill-common`.

---

## R4 — Package-identity normalisation must be hand-rolled in xtask

**Decision**: Implement PURL normalisation inside xtask. Do not add a
`waybill-common` dependency.

`waybill_common::types::purl::Purl` exists and would be the obvious reuse,
but adding it couples the measuring instrument to the library under
measurement. If a normalisation bug existed in `Purl`, the harness would
apply that same bug to every tool's output — including its own — and the
self-check in FR-012 would pass while the comparison was wrong. An
instrument should not share code with its subject.

The normalisation required by FR-006 is string manipulation over a
well-specified format: lowercase the type, normalise the namespace, drop
qualifiers and subpath, keep the version. Perhaps 40 lines, and independently
testable against hand-written expectations.

**Alternatives rejected**: depending on `waybill-common` (shared-bug risk
above); depending on an external PURL crate (a new dependency for something
this small, and it would still be shared with nothing).

---

## R5 — Truth derivation: what is available per ecosystem

**Decision**: Ship Go first with two declared methods; design the config so
further ecosystems are additive.

Measured on the kubernetes tree at `b1856e29`:

| method | count | exactness |
|---|---|---|
| union of all `go.sum` files (39 of them) | 479 | **superset** — go.sum holds hashes for modules considered during resolution, including ones never linked |
| union of all `go.mod` requires | 423 | closer, still not the built set |
| `vendor/modules.txt` | 208 | vendored subset only |
| `go list -m all` | not measured | authoritative, needs toolchain + usually network |

**This matters because it invalidates a claim made during the ad-hoc
comparison.** waybill scored 468 distinct modules against the go.sum union of
479 and was called "closest to ground truth". But go.sum is a superset, so
that scoring penalised every tool for correctly omitting unused modules —
waybill's apparent lead may be partly an artefact of it reporting *more*,
not of it being *right*. FR-008b's superset labelling exists because of this.

---

## R6 — Privacy and reference-class determinism are in tension

**Finding, unresolved by design.** R1 says trustworthy timing wants a
reference-class host. The only reference-class host available to this
project is GitHub-hosted CI. FR-016 forbids wiring the harness into
automation whose artefacts are publicly readable — and artefacts on a public
repository are exactly that.

So the two requirements cannot both be fully satisfied with current
infrastructure. The plan resolves it by **not needing** reference-class
timing:

- Coverage and accuracy — the metrics that answer the questions people
  actually ask — are deterministic on any host (R2).
- Timing is reported as an interleaved within-session ratio with its spread
  shown, never as a gated verdict (R1).

If reference-class timing later becomes necessary, the options are a private
runner or a private mirror. Both are out of scope here and neither is needed
for the harness to be useful.

---

## R7 — Where untracked configuration lives

**Decision**: `xtask/compare/` for committed, tool-agnostic scaffolding;
`xtask/compare/tools.local.toml` for the operator's tool set, gitignored.
Results to `target/compare/`.

`.gitignore` already excludes `target/` (line 2) and has precedent for a
per-milestone results path (`target/quality/`, line 74). Adding
`xtask/compare/*.local.toml` follows the same shape.

The committed side ships an **example** config using placeholder tool names
(`tool-a`, `tool-b`) so the format is documented without naming anything.

---

## R8 — The known-answer fixture

**Decision**: A synthetic Go module tree, committed, whose exact module set
is fixed by construction and asserted in a plain list beside it.

Go first because it is the ecosystem where truth derivation is best
understood (R5) and where the ad-hoc comparison went wrong. The fixture must
include at least one case that discriminates between the reduction rules
considered in clarification — two versions of the same package — so that a
regression to version-stripping fails visibly rather than silently.

---

## Summary of impacts on the spec

| Finding | Impact |
|---|---|
| R1 | Timing is a ratio with spread, never a gated absolute; runs must interleave |
| R2 | The spread gate applies to timings only; counts assert exact equality |
| R5 | Confirms FR-008b was necessary; retires a claim made pre-spec |
| R6 | A requirement tension exists and is resolved by scope, not by machinery |
