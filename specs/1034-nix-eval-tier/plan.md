# Implementation Plan: Opt-in `nix eval` resolution tier

**Branch**: `1034-nix-eval-tier` | **Date**: 2026-09-28 | **Spec**: [spec.md](./spec.md)
**Issue**: **#971 part A**. The directory number is the next sequential milestone
number, *not* an issue reference — issue #1034 is the downstream full-closure
work this feature unblocks.
**Input**: Feature specification from `/specs/1034-nix-eval-tier/spec.md`

## Summary

waybill learns a Nix-built project's package versions by fetching and parsing
nixpkgs files — a reconstruction of what Nix would compute, which #1033 proved
can be wrong. This adds an opt-in tier that asks Nix instead, keeping
file-parsing as both the default and the fallback.

The approach: one bounded `nix eval` subprocess per scan, run in **pure** mode
with import-from-derivation **refused and verified refused**, for an explicitly
named system, at the revision the project's `flake.lock` already pins. Evaluated
versions supersede file-parsed ones; the superseded value is retained as
metadata so the next divergence is findable the way #1033 was. Every failure
degrades to today's behaviour with a distinguishing reason code.

Phase 0 changed three things the spec assumed (research R2, R3, R4). The most
consequential: **passing `--option allow-import-from-derivation false` to a
`nix` that does not support it is a silent no-op with exit code 0**, so the
safety control has to be *verified in effect*, not merely requested.

**Implementation then corrected the spec's threat model** (2026-09-29). The
tier evaluates nixpkgs at the pinned revision; it does not evaluate the
project's own flake, so repository-authored expressions never run. See spec
US3's scope correction. The controls stay — nixpkgs is unaudited code, the
revision is a repository-controlled value reaching a Nix expression (now
validated, FR-008a), and project-flake evaluation is what §R8 and issues #1034
/ #1040 need next.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `std::process::Command` +
`std::thread` + `std::sync::mpsc::recv_timeout` (the budgeted-subprocess pattern
at `golang/go_mod_graph.rs:81`, reused at `golang/mod_why.rs:240` and by the
m203 helm renderer), `serde`/`serde_json` (`nix eval --json` output and
annotation values), `clap` (the flag), `tracing`, `anyhow`/`thiserror`.
**Zero new Cargo dependencies at any layer** (research R7).
**External runtime dependency**: the `nix` binary on `PATH` — opt-in, absent by
default, degrading when missing. Same posture as `helm` for `--helm-render`.
**Storage**: N/A — all state in-process per scan. The existing per-revision
cache at `~/.cache/waybill/nixpkgs/<rev>/` (m926) is untouched; this tier adds
no cache of its own, because Nix's own store and eval cache already serve that
role and duplicating them would create a second thing to invalidate.
**Testing**: `cargo +stable clippy --workspace --all-targets` and
`cargo +stable test --workspace` (the mandatory pre-PR gate); the m195 corpus
harness for goldens; the existing `xtask nix-oracle` (#971 part B, `19271a4c`)
as the differential check for SC-001.
**Target Platform**: wherever `nix` runs — Linux and macOS. On Windows `nix` is
absent and the tier degrades, which is the same code path as any other absence.
**Project Type**: CLI subcommand behaviour (`waybill sbom scan`).
**Performance Goals**: measured warm-store single-attribute pure evaluation is
0.49–0.64 s; 423 names at a pinned revision is 0.5 s warm / 7.7 s cold eval
cache. The budget default is set by task T-R6, not quoted here.
**Constraints**: MUST NOT fail a scan (spec FR-011, constitutionally required
per research R6); MUST refuse IFD and verify the refusal (FR-008, research R3);
MUST run pure (FR-009, research R2); MUST bound evaluation in wall-clock time
because Nix will not (FR-012, research R4).
**Scale/Scope**: one subprocess per scan. The corpus Haskell targets resolve
~400–1,300 names.

## Constitution Check

*GATE: evaluated before Phase 0, re-evaluated after Phase 1 design. Constitution
v3.0.0.*

| Principle | Verdict | Basis |
|---|---|---|
| **I. Pure Rust, Statically Linked** | **PASS** | Research R1. The principle governs first-party language and third-party *linkage*; a subprocess links nothing. Precedent: `git`, `go`, `gradlew`, `helm`, `docker`, `podman`, `spdx3-validate`. This explicitly retires m926 §R1's contrary reading. |
| **II. eBPF-Only Observation** | **N/A** | Scoped to the trace path; this is `sbom scan`. |
| **III. Fail Closed** | **PASS, by scope** | Research R6. Principle III is scoped by its own text to the eBPF trace. For enrichment, Principle XI mandates the *opposite* — emit the SBOM with the field omitted and a transparency annotation — and XII constraint 3 repeats it. Stated explicitly rather than assumed, because "this feature degrades where a principle says fail" is exactly the kind of gap that should be argued, not skipped. |
| **IV. Type-Driven Correctness** | **PASS, with design obligation** | The system identifier and the nixpkgs revision are domain values and get newtypes (see data-model.md); no `.unwrap()` in production; `thiserror` for the degradation-reason enum. |
| **V. Specification Compliance** | **PASS, with design obligation** | Native fields first (memory: `feedback_native_fields_first`). See the annotation review in data-model.md — the divergence record has no native CDX/SPDX equivalent, but the evaluation provenance may map to CDX `evidence`. |
| **VI. Three-Crate Architecture** | **PASS** | Entirely within `waybill-cli`. `waybill-common` gains nothing; `waybill-ebpf` untouched. |
| **VII. Test Isolation** | **PASS, with design obligation** | The tier reads `PATH` and spawns processes. Tests that manipulate environment MUST route through `crate::testing::EnvGuard::acquire()` (memory: `reference_podman_test_flake`), or they will race. |
| **VIII. Completeness** | **PASS** | The tier strictly increases what is known; it removes nothing. |
| **IX. Accuracy** | **PASS — this is the point** | The feature exists to replace a reconstruction with the authority. FR-005's retained file-parsed value is the "flagged rather than silently included" discipline applied to the *losing* value. |
| **X. Transparency** | **PASS, and load-bearing** | FR-005, FR-006, FR-013, FR-015, FR-016 are the transparency metadata. Principle X requires spec-native mechanisms "where possible" — assessed per-annotation in data-model.md rather than defaulting to `waybill:` properties. |
| **XI. Enrichment** | **PASS** | XI's degradation clause is precisely US2. |
| **XII. External Data Source Enrichment** | **PASS, with one note** | Constraint 1 ("MUST NOT introduce new components") is written for the trace-first model. Evaluation *can* name a component file-parsing did not. See Complexity Tracking. |
| **Strict Boundary 1** (no lockfile discovery) | **N/A, by scope** | Written for the trace path; `sbom scan` is the established filesystem-discovery path and the entire `scan_fs` subsystem operates there. |
| **Strict Boundary 3** (no first-party C / dynamic linkage) | **PASS** | Nothing linked, no C authored. |
| **Strict Boundary 4** (no `.unwrap()` in production) | **PASS, with design obligation** | Subprocess and JSON handling are the usual offenders; `clippy::unwrap_used` is denied at the crate root and `--all-targets` enforces it in tests too. |

**Gate result: PASS.** One item carried to Complexity Tracking.

## Complexity Tracking

| Item | Why needed | Simpler alternative rejected because |
|---|---|---|
| Evaluation may name a component that file-parsing did not, which sits oddly against **Principle XII constraint 1** ("External sources MUST NOT introduce new components") | XII is written for the trace-first model, where the trace is the authority and a lockfile must not add to it. On the `sbom scan` path there is no trace, and evaluation is *more* authoritative than the file-parsing it supersedes — not less. Suppressing an evaluated component to satisfy a trace-path rule would make the SBOM knowingly incomplete. | **Dropping evaluated-only components** — rejected: it would reintroduce the #1033 defect class in the opposite direction, hiding exactly what the tier was added to find. **Emitting them unannotated** — rejected: Principle X requires the provenance. **Resolution**: emit them, annotated with evaluation provenance and counted at document scope (FR-006's sibling), so a consumer applying a strict trace-first reading can filter them. Flagged here rather than buried, because it is a genuine tension with a principle's letter. |

## Project Structure

### Documentation (this feature)

```text
specs/1034-nix-eval-tier/
├── spec.md                  # Feature specification
├── plan.md                  # This file
├── research.md              # Phase 0 — findings, each labelled MEASURED/REASONED/UNMEASURED
├── measurements/
│   └── probe-nix-behaviour.sh   # The probes behind R2, R3, R4 — re-runnable
├── data-model.md            # Phase 1 — entities, annotation/native-field review
├── contracts/
│   ├── cli-flags.md         # Flag surface, precedence, degradation
│   ├── nix-invocation.md    # Exact argv, pre-flight, budget, parsing
│   └── annotations.md       # Emitted metadata across all three formats
├── quickstart.md            # Phase 1 — how to exercise it
└── tasks.md                 # Phase 2 — NOT created by /speckit.plan
```

### Source Code (repository root)

```text
waybill-cli/src/
├── cli/
│   └── scan_cmd.rs                      # + the opt-in flag and its system parameter
│                                         #   (`sbom scan`'s ScanArgs; cli/scan.rs is `trace`)
└── scan_fs/package_db/nix/
    ├── mod.rs                           # tier dispatch: evaluate, then reconcile
    ├── lockfile.rs                      # (existing) revision from flake.lock — reused
    ├── eval/                            # NEW — the tier
    │   ├── mod.rs                       #   orchestration + degradation decision
    │   ├── preflight.rs                 #   R3 capability verification (IFD refusal in effect)
    │   ├── invoke.rs                    #   argv construction + budgeted subprocess
    │   ├── result.rs                    #   parsed evaluation output
    │   └── reason.rs                    #   the degradation-reason enum (FR-013)
    └── haskell_packages/
        ├── mod.rs                       # + reconcile evaluated vs file-parsed (FR-004/005)
        └── cache.rs                     # (existing) untouched

waybill-cli/tests/
├── nix_eval_tier.rs                     # NEW — degradation matrix, IFD refusal, purity
└── fixtures/nix_eval/                   # NEW — the IFD probe flake as a test fixture

xtask/src/nix_oracle/mod.rs              # unchanged; NOT the template (research R2)
```

**Structure Decision**: The tier is a new `eval/` submodule under the existing
`scan_fs/package_db/nix/` module rather than a peer of it, because it resolves
the *same* entities the existing nix reader resolves and must reconcile against
them in-process. Revision discovery (`lockfile.rs`) and the per-revision cache
(`haskell_packages/cache.rs`) are reused unchanged — this feature adds a second
way to *answer* the question the existing module already knows how to *ask*.

## Constitution Check — post-design re-evaluation

*Re-run after Phase 1, per the plan workflow. Design artifacts:
`research.md`, `data-model.md`, `contracts/*`, `quickstart.md`,
`measurements/probe-nix-behaviour.sh`.*

**Gate result: PASS, unchanged.** Three obligations flagged as "PASS, with
design obligation" before Phase 0 are now discharged by named artifacts, and one
verdict that had been deferred is now decided:

| Pre-Phase-0 obligation | Discharged by |
|---|---|
| **IV** — newtypes for domain values, no `.unwrap()`, `thiserror` for the error enum | `data-model.md`: `NixSystem` newtype with validation; `DegradationReason` as a `thiserror` enum with seven variants |
| **V** — native fields before `waybill:` properties | `data-model.md` Principle V audit, one row at a time. **C179's verdict was deferred in the first draft and has since been decided**: CDX 1.6 `metadata.lifecycles[].phase` was checked directly against `waybill-cli/src/generate/lifecycle_phases.rs:40-67` and rejected — it is a fixed enum already carrying the CISA SBOM type in this codebase, so it cannot carry a platform |
| **VII** — test isolation for anything touching the environment | `quickstart.md` degradation matrix; tests manipulating `PATH`/`NIX_REMOTE` route through `crate::testing::EnvGuard::acquire()` |
| **X** — transparency via spec-native mechanisms where possible | Five rows C177–C181, each with its audit; the extractor-parity gate named in `contracts/annotations.md` |

**One design change strengthened a gate rather than relaxing it.** Research R3
found that an unsupported `nix` option is a silent no-op with exit code 0. FR-008
(refuse import-from-derivation) would therefore have been *unenforceable as
specified* — waybill would have passed the option, `nix` would have ignored it,
and every observable signal would have looked like success. The design now
verifies the refusal is in effect before evaluating, and
`EvaluationOutcome.ifd_refused_verified` records the verification rather than the
request. Principle IX (Accuracy) is better served after Phase 1 than the spec
alone would have achieved.

**Complexity Tracking is unchanged**: one item, the tension between evaluated-only
components and Principle XII constraint 1, with the resolution and both rejected
alternatives recorded above.

## Phase status

| Phase | Status | Output |
|---|---|---|
| 0 — Research | **complete** | `research.md`, `measurements/probe-nix-behaviour.sh` |
| 1 — Design & contracts | **complete** | `data-model.md`, `contracts/{cli-flags,nix-invocation,annotations}.md`, `quickstart.md`, agent context updated |
| 2 — Tasks | not started | `/speckit.tasks` |

**Blocking before implementation**: tasks **T-R5** (cold-store acquisition cost)
and **T-R6** (project-flake evaluation cost) from `research.md` must run before
`--nix-eval-timeout-secs` gets a default and before SC-007 gets its ratio. Both
are measurements, not decisions; neither needs operator input.
