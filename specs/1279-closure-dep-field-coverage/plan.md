# Implementation Plan: `unreferenced` must mean "nothing depends on this"

**Branch**: `1279-closure-dep-field-coverage` | **Date**: 2026-11-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1279-closure-dep-field-coverage/spec.md`
**Issue**: #1346

## Summary

`waybill:closure-role = "unreferenced"` answers two different questions with
one word: nix records no dependency on this member, or nix records one through
an attribute the classifier does not read. The first is a fact about the
closure, the second about waybill, and a consumer cannot tell them apart.

The fix has two halves, and the clarifications deliberately made them
different sizes. Four attributes become **consulted**, which moves seven
derivations on the pinned closure and retracts one false `scope: "excluded"`.
The two stdenv attributes that carry most of the declarations stay
**unclassified** and instead get a per-component marker naming them, so 3
members on linux (16 on darwin) stop being reported as `unreferenced` without
any census movement at all.

Phase 0 changed two things in the spec. The role delta was measured rather
than predicted, and `artifact-input` turns out to go **down** by two; and the
spec's figures were darwin-measured but unlabelled, while the FR-002
population differs five-fold between systems.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1).
No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: existing only — `serde`/`serde_json` (annotation
values), `std::collections::{BTreeMap, BTreeSet}`, `tracing`. **Zero new Cargo
dependencies.** No subprocess calls, no network, no filesystem writes beyond
the emitted document.
**Storage**: N/A — all state in-process per scan, as every closure milestone
since m1034.
**Testing**: `cargo test --workspace --no-fail-fast` plus `cargo clippy
--workspace --all-targets`, through `./scripts/gate.sh --expect '>>> all
pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with **no `+toolchain`
argument** (#1189). Corpus goldens are `#[ignore]`d locally and refreshed only
through `public-corpus.yml` dispatch — a local run of that target prints
`skipping … WAYBILL_RUN_PUBLIC_CORPUS not set` and then reports `ok` in
0.00s, which is a pass that checked nothing.
**Target Platform**: all supported hosts. The closure path is opt-in behind
`--nix-closure` and needs `nix` present.
**Project Type**: CLI, single Rust workspace, three crates.
**Performance Goals**: none new. Four more field names in two existing loops
over data already parsed.
**Constraints**: the role census must move **only** for members newly
reachable through the four consulted attributes (SC-011). That is what makes
this milestone's effect attributable, and it is why the stdenv pair is
disclosed rather than classified.
**Scale/Scope**: on the pinned x86_64-linux closure — 1,749 derivations, 230
unconsulted declarations across 23 derivations, 7 derivations changing role,
1 false exclusion retracted, 3 members gaining the marker.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Verdict | Basis |
|---|---|---|
| I. Pure Rust, Statically Linked | **PASS** | Zero new dependencies; four string literals added to two existing constants. |
| III. Fail Closed | **PASS** | Scan mode, so Principle X governs: evidence that could not be read is reported. FR-007 reports an unrecognised attribute rather than ignoring it, which is the same disposition #1348 took. |
| IV. Type-Driven Correctness | **PASS, with an obligation** | The marker must be a typed record rather than a string assembled at the emission site, and must name its attributes as a sorted set so the wire value is byte-stable. No `.unwrap()` in production. |
| V. Specification Compliance | **PASS — and the audit rejected the tempting option** | Research R3, read from the vendored schemas: no format has a vocabulary for "referenced through a mechanism the producer does not model". The *tempting* native option is a real `dependsOn` edge, and it is rejected because an edge is a stronger claim than this milestone is entitled to make — stdenv's machinery would enter the dependency graph of every derivation stdenv touches, which is precisely what FR-012 declines. **KEEP-NO-NATIVE**, same category as C203 and C204: provenance about how a conclusion was reached. |
| VI. Three-Crate Architecture | **PASS** | Everything in `waybill-cli`. |
| VII. Test Isolation | **PASS** | No env-var mutation. New fixtures are new files; existing ones are asserted byte-identical. **And no test or probe evaluates a fixture flake with a real `nix`** — doing so rewrote a committed `flake.lock` on 2026-11-04 and broke 11 tests in an unrelated file. |
| VIII. Completeness | **PASS — the principle being repaired** | 230 declarations on linux are currently dropped silently. |
| IX. Accuracy | **PASS** | No component is invented. Two derivations become emitted because they were referenced all along. |
| X. Transparency | **PASS — the other principle being repaired** | A role that means two things cannot be assessed by a consumer, which is what X exists to prevent. |
| XII.3 Enrichment disclosure | **PASS** | The marker is in the document, not only in a log. |

**Gate result: PASS. Complexity Tracking stays empty.** The decision a reader
might expect to find justified there — folding the cross-compilation triples
into `artifact-input` rather than giving them a fifth role — is recorded in
the spec's Assumptions as accepted debt with a named follow-up, not as a
constitutional exception.

## Project Structure

### Documentation (this feature)

```text
specs/1279-closure-dep-field-coverage/
├── plan.md                        # This file
├── spec.md                        # Feature specification
├── research.md                    # Phase 0 — R1..R7
├── data-model.md                  # Phase 1
├── quickstart.md                  # Phase 1
├── contracts/
│   ├── consulted-set.md           # which attributes are read, and by whom
│   └── unclassified-marker.md     # the marker's wire contract
├── checklists/requirements.md
├── measurements/
│   ├── attribute-census.py        # the #1346 baseline
│   └── role-delta-probe.py        # what the four attributes do
└── tasks.md                       # Phase 2 — /speckit.tasks, NOT this command
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/closure/
├── classify.rs       # THE CHANGE. Two constants widen; consulted_fields()
│                     #   follows automatically, which is why the disclosure
│                     #   cannot drift from the classifier (#1348 built that)
├── derivation.rs     # the unclassified-reference index, and ReadOutcome
├── emit.rs           # the marker onto the component
└── mod.rs            # threads the marker set out to emission

waybill-cli/src/resolve/
└── closure_role_scope.rs   # NOT changed — read to confirm `numactl` loses
                            #   its scope by m1277's existing rule

waybill-cli/src/parity/extractors/    # the marker's three extractors
docs/reference/sbom-format-mapping.md # the marker's row

waybill-cli/tests/
├── closure_dep_fields.rs                    # new integration tests
├── fixtures/nix_closure_dep_fields/         # new fixtures
├── fixtures/public_corpus/nix-closure-moat/ # refreshed by dispatch only
└── corpus_harness_195/expectations.rs       # the standing assertions
```

**Structure Decision**: no new modules. The classifier's two constants are the
whole of the classification change — #1348 deliberately made
`consulted_fields()` derive from them so that widening one widens the
disclosure's complement too, and SC-009 asserts that rather than assuming it.
`closure_role_scope.rs` is listed because a task reads it, not because it
changes: `numactl` loses its exclusion through m1277's existing rule, and if a
task finds itself editing that file the confinement argument has broken.

## Phase 0 — Outline & Research

**Complete.** `research.md` resolves R1 through R7. Three findings changed the
plan rather than confirming it:

- **R1**: the role delta is 7 derivations on linux, and `artifact-input`
  **falls by two** — three of its members gain a tooling reference and become
  `both` while only one arrives. A plan that assumed every delta positive
  would have mis-specified SC-011.
- **R2**: exactly one component in the golden loses a false exclusion, and it
  is `numactl`. Named in SC-013 rather than counted, because counting is how
  1278 shipped nine for a population of six.
- **R6**: the spec's figures were darwin-measured and unlabelled, and the
  FR-002 population differs **five-fold** between systems (3 on linux, 16 on
  darwin). The spec now labels every figure and states its criteria against
  linux, which is what the goldens pin.

No `NEEDS CLARIFICATION` markers remain.

## Phase 1 — Design & Contracts

**Complete.** `contracts/consulted-set.md` fixes which attributes are read and
the single-source rule that keeps the disclosure honest;
`contracts/unclassified-marker.md` fixes the marker's wire shape, gating and
Principle V audit; `data-model.md` carries the entities and the validation
rules each requirement imposes; `quickstart.md` is the reproduction and
verification recipe.

### Post-design Constitution re-check

**Still PASS.** The design added no dependency, no module and no subprocess.
The one thing Phase 1 nearly added — a native `dependsOn` edge for the
unclassified references — was rejected on the grounds that it is a stronger
claim than the milestone is entitled to make, which is Principle IX rather
than a compromise of Principle V.

## Phase 2 — Task planning approach

Not executed here; `/speckit.tasks` produces `tasks.md`. Intended shape:

- **Setup**: fixtures built from the shapes in the measured closure, including
  a member referenced only through each of the four new attributes, a member
  referenced only through a stdenv default, and the awkward cases (declared
  but empty, consulted but never declared).
- **Foundational**: the unclassified-reference index in `derivation.rs`.
  Nothing observable changes until it is threaded out.
- **US1 (P1)**: the four attributes consulted, plus the marker — this is the
  story that makes `unreferenced` mean one thing, and it ends with `numactl`
  asserted.
- **US2 (P2)**: propagation, including a transitive chain.
- **US3 (P2)**: the C204 disclosure narrowing to exactly what remains.
- **US4 (P3)**: the unknown-attribute guard and its mutation test.
- **Polish**: the census record, the refresh prediction committed *before*
  dispatch, the dispatch, the scoring, the invariant reconciliation.

Two sequencing constraints, both of which cost a golden refresh if missed: the
marker's catalog row and its three extractors land together, and the refresh
comes last, after every output-changing task.
