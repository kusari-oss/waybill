# Implementation Plan: A must-pass registry for invariant cells whose Pass is load-bearing

**Branch**: `1217-must-pass-registry` | **Date**: 2026-10-18 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1217-must-pass-registry/spec.md`
**Issue**: #1227 (Refs #1216, #1155) — carries feature 1156's deferred SC-010/SC-011 and its eleven deferred tasks

## Summary

A committed registry recording, per `(invariant, target)` cell, that the
cell's `Pass` is load-bearing — so a cell cannot quietly become
`NotApplicable` and keep reading green. The mirror image of
`invariant-declarations.toml`: that file records cells known to be broken,
this one records cells known to be meaningful, and a cell may be in at most
one of them.

The mechanism is a new `must_pass` module beside `declarations`, a seven-rule
validator that runs before any comparison, and a corpus-level pass
`unmet(cells, file)` modelled on `unfalsifiable_invariants(cells)` —
explicitly **not** a third argument to `verdict()`, which keeps its two-input
eight-row shape. The registry becomes an explicit constructor input to the
matrix report so that must-pass failures print with the other lane failures
and every registered cell is marked in the grid. Seeded with exactly the four
Pants I8 cells, with its own `NotApplicable` mutation test, because a registry
that cannot be shown to fail is the same vacuity one level up.

**One plan-time finding changed the spec.** The spec asserted the four seeded
targets have no other versionless components, so anchors leaving would trip
all four cells. Measured, that is true of one: django carries thirteen more
versionless components, jvm and python one each, so on those three an anchor
could vanish with the cell still reading `Pass`. The spec now carries the
table, FR-013a requires each reason to state its own target's reach, and Out
of Scope names the different mechanism that would close the residual gap. The
finding also *strengthened* the decision to delete feature 1156's stopgap: it
asserts the same `Pass` with the same blind spot, so it is a pure duplicate.
A second measurement then narrowed what to do about it. The gap needs an
assertion that the anchor is *present*, which is a claim about one target —
the per-target expectation layer, which already carries exactly such an
assertion for two of the seven seeded anchors. So it is not new machinery, and
the remaining five are folded into this feature (FR-019, FR-019a) rather than
deferred: #1230 is filed as their stated reason. Details and the probe:
[research.md](./research.md) M1 and M3.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. No `+toolchain` argument when running the
gate — #1189 is the incident that makes that explicit.
**Primary Dependencies**: existing only — `toml = "0.8"` (already parses the
declarations file), `serde`, `serde_json`, and dev-only `tempfile` (already a
dev-dependency) for the missing/empty-file fixtures. **Zero new Cargo
dependencies**, at any level including the lockfile.
**Storage**: one committed TOML file,
`waybill-cli/tests/corpus_harness_195/invariant-must-pass.toml`. No runtime
state, no cache, no per-run artifact of its own — the existing
`target/invariants/run-<id>.json` gains fields.
**Testing**: `cargo test --workspace --no-fail-fast` for the hermetic lane
(`invariants::tests::golden_matrix_is_evaluable_and_gated` plus the new
`must_pass` tests); the network-gated `public_corpus` lane for the per-target
path. Both reached through `./scripts/pre-pr.sh` under `scripts/gate.sh`.
**Target Platform**: developer and CI hosts, macOS and Linux. Test-only code;
the shipped `waybill` binary is byte-identical pre/post merge.
**Project Type**: test-harness extension inside `waybill-cli`'s integration
tests (`waybill-cli/tests/corpus_harness_195/`). Not a library, not a CLI
surface.
**Performance Goals**: N/A. The added work is one file read plus
O(entries x cells) = 4 x 128 comparisons per lane run, below measurement
noise. No performance criterion is claimed and none should be invented.
**Constraints**: no network, no subprocess, no container pull, and no
filesystem read beyond the registry file — enforced by the existing
source-level `no_new_io` gate, whose enumeration of permitted repository-file
reads must name the registry (FR-016). The hermetic lane must keep running
unprivileged with no network (Constitution VII).
**Scale/Scope**: 8 invariants x 17 manifest targets = 136 cells; the hermetic
lane evaluates 8 x 16 = 128 (one target's goldens are deliberately filtered).
Seed is 4 entries. The machinery is general to all 136.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Applies | Assessment |
|---|---|---|
| **I. Pure Rust, Statically Linked** | yes | Zero new dependencies; no C, no scripting runtime. The one new non-Rust artifact is a TOML data file. **PASS** |
| **II. eBPF-Only Observation** | no | No observation path touched. |
| **III. Fail Closed** | yes | This feature *is* a fail-closed rule: an invalid registry fails the lane rather than being partly honoured (FR-004), and a missing one fails rather than reading as empty (FR-011). **PASS** |
| **IV. Type-Driven Correctness** | yes | `Unmet` is a record, not a formatted string, precisely so no consumer parses a message back (data-model.md). `clippy::unwrap_used` is denied in `waybill-cli` including `#[cfg(test)]`, so the tests module carries the established `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard. No production code changes. **PASS** |
| **V. Specification Compliance** | no | No emission change; no CDX/SPDX/PURL surface touched. No catalogue row, because nothing new appears on the wire. |
| **VI. Three-Crate Architecture** | yes | Test-only addition inside `waybill-cli/tests/`. No crate boundary moves. **PASS** |
| **VII. Test Isolation** | yes | The hermetic lane runs under plain `cargo test` with no privileges and no network; the registry adds a repository-file read only. Validation rule 3 exists so an entry cannot come to depend on the network-gated lane. **PASS** |
| **VIII. Completeness / IX. Accuracy** | indirect | No SBOM content changes. The feature protects the harness that measures accuracy, which is why FR-013a insists the file be accurate about its own reach. **PASS** |
| **X. Transparency** | yes | The grid mark and the reason-carrying failure message are the transparency requirement applied to the harness itself: a reader can see which passes are load-bearing instead of inferring it. **PASS** |
| **XI–XII. Enrichment / External Data** | no | No enrichment, no external source. |
| **Development Workflow — Pre-PR Verification (MANDATORY)** | yes | Run as CLAUDE.md and `scripts/pre-pr.sh` have it — `./scripts/gate.sh --expect … -- ./scripts/pre-pr.sh`, with **no** `+toolchain` argument. The constitution's `### Pre-PR Verification (MANDATORY)` table still printed `cargo +stable clippy` / `cargo +stable test` when this plan was written; #1189 is the incident that showed `+stable` overrides the `rust-toolchain.toml` pin and silently runs the gate on a different compiler than CI. The clause predated that fix, so it is not an instruction to re-introduce `+stable` here. Its own amendment is PR #1232 (constitution v4.0.1), which strikes both — once that lands this row is a plain PASS. **PASS, with a stale-clause note pending #1232** |

**Result**: no violations, so Complexity Tracking stays empty. Re-checked
after Phase 1: the design adds one module, one data file, one report field
and one constructor parameter — nothing that trades a principle for
convenience.

## Project Structure

### Documentation (this feature)

```text
specs/1217-must-pass-registry/
├── plan.md                               # This file
├── spec.md                               # Amended at plan time by research M1
├── research.md                           # Phase 0 — M1/M2 measurements, R1–R10 decisions
├── data-model.md                         # Phase 1 — MustPass, MustPassFile, Unmet, the seven rules
├── quickstart.md                         # Phase 1 — how to run it, how to add an entry
├── contracts/
│   └── must-pass-registry.md             # Phase 1 — v2, supersedes feature 1156's copy
├── measurements/
│   └── versionless-populations.py        # The M1 probe, committed so the finding is re-runnable
├── checklists/
│   └── requirements.md                   # From /speckit.specify, re-checked post-clarify
└── tasks.md                              # Phase 2 output — NOT created by /speckit.plan
```

### Source Code (repository root)

```text
waybill-cli/tests/
├── public_corpus.rs                                  # the network-gated lane; loads + validates the registry
└── corpus_harness_195/
    ├── expectations.rs                               # EDIT: pin 5 unasserted Pants anchors (FR-019, #1230)
    ├── invariant-declarations.toml                   # EDIT: I8 header names the registry, not a deleted test
    ├── invariant-must-pass.toml                      # NEW: the registry, seeded with four entries
    └── invariants/
        ├── mod.rs                                    # EDIT: `mod must_pass;`, hermetic lane wiring
        ├── must_pass.rs                              # NEW: types, load_at, validate, unmet, and its tests
        ├── report.rs                                 # EDIT: registry parameter, grid mark, legend, unmet failures
        ├── declarations.rs                           # unchanged — the sibling this mirrors
        ├── goldens.rs                                # unchanged — GOLDENS_ARE_FILTERED is rule 3's universe
        ├── mutation_tests.rs                         # EDIT: delete the 1156 stopgap; keep PANTS_I8_TARGETS
        ├── line_stability.rs                         # unchanged — the pattern must_pass.rs mirrors
        └── no_new_io.rs                              # EDIT: permitted-reads comment names the registry
```

**Structure Decision**: everything lives in the milestone-195 corpus harness,
beside the file it mirrors. `must_pass.rs` is a sibling of `declarations.rs`
rather than a submodule of it, because the two files are peers — one records
broken cells, one records meaningful ones — and a reader who can read either
should find them at the same depth. No new test binary: the registry is
enforced by the two lanes that already exist, which is what FR-010 is about.

## Phases

Ordered so that the mechanism and its proof land together, per the spec's P1
framing. Each phase ends green.

**Phase 1 — Foundation.** `must_pass.rs` with `MustPass`, `MustPassFile`,
`must_pass_path()`, `load_at() -> Result`, `load()`. `mod must_pass;` in
`mod.rs`. Nothing reads it yet.

**Phase 2 — US1, the mechanism and its teeth (P1).** `validate` (seven
rules), `unmet`, the seeded TOML with its FR-013a reasons, wiring into both
lanes, and the `NotApplicable` mutation test that is the only evidence the
registry does anything. The seed lands here, not later, because US1's
independent test is "record the four cells, then force one to stop applying" —
there is nothing to force without the file. Carries 1156's T026–T029, T031 and
T035.

**Phase 3 — US2, the registry cannot be the vacuity (P2).** Seven independent
rule-violation tests; the validate-before-compare demonstration;
missing-versus-malformed-versus-empty; the coverage test; the line-stability
pair. Carries T036–T038.

**Phase 4 — US3, operator visibility (P3).** Report plumbing, the grid mark,
the legend, and the test that pins the pass to both report scopes (FR-008a,
SC-017) — the responsibility Phase 2 hands to the report in exchange for the
direct lane calls it removes. Carries T030.

**Phase 5 — Anchor presence (FR-019, #1230).** Five assertions in
`expectations.rs`, each mirroring the two that already exist, plus the
mutation that proves they bite: remove one anchor from a golden and the
target's expectations must fail where today they pass. Independent of the
registry — it could ship alone — but it is what stops the registry recording a
commitment it cannot keep.

**Phase 6 — Reconciliation and polish.** Delete the 1156 stopgap and update
the declarations I8 header (FR-017); update the `no_new_io` permitted-reads
comment (FR-016, T042); update feature 1156's deferral note and mark its
SC-010/SC-011 met (FR-018); CHANGELOG; the full gate.

## Risks

| risk | mitigation |
|---|---|
| A later reader "fixes" the `SingleTarget` asymmetry, so the corpus lane stops enforcing the registry | FR-008a requires the behaviour and SC-017 requires a test of it (tasks T038); research R2, the contract and the module header state the reasoning. Prose alone was the whole mitigation until `/speckit.analyze` — and the lane that would have noticed is the network-gated one, so nothing in the gate would have caught the tidy-up |
| The report gains a registry parameter that a caller could pass empty | the parameter is required, not a setter — research R1; an empty registry is a distinct, deliberate state |
| The seed's reasons overstate their reach | FR-013a, the committed probe, and the contract's explicit rejection of that sentence shape |
| The draft in #1227 gets applied rather than re-derived | research R8 tabulates the five places the clarifications changed it |
| An anchor assertion is added for the two targets that need it most and the other three are forgotten | FR-019a names all five; the committed probe prints `NO` rows until none remain (SC-014) |
