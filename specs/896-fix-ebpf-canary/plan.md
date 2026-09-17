# Implementation Plan: A trustworthy eBPF canary signal

**Branch**: `896-fix-ebpf-canary` | **Date**: 2026-09-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/896-fix-ebpf-canary/spec.md`
**Issue**: [#685](https://github.com/kusari-oss/waybill/issues/685)

## Summary

The eBPF canary has produced **zero green runs in its lifetime** — 36 runs,
36 failures, 2026-08-13 through 2026-09-17 — and has reported every one of them
as an upstream regression in `aya-rs/bpf-linker`. The failures are its own: its
job installs stable Rust only, then relies on `cargo +nightly` inside xtask,
and `rust-src` is never installed. The two lanes that build the same artifact
successfully (`ci.yml`, `release.yml`) both install it explicitly.

Two changes, in that order:

1. **Make the canary able to build.** Bring its environment to parity with
   `ci.yml`'s eBPF lane (nightly + `rust-src`, stable, default-stable,
   `rustup-init` removal), verify the artifact is actually produced, and add a
   check that fails when the two environments diverge again.
2. **Make its failures self-attributing.** When the build against `latest`
   fails, build again against the pinned version as a control. Pinned fails
   too → the canary is broken, reported under its own title with no mention of
   upstream. Pinned passes → the component regressed, reported under the
   existing title naming both versions. Attribution reads outcomes only, never
   step position or error text — both of which mis-classify the live failure.

Escalation moves onto a clock that starts at the streak's first failure
(the report's `created_at`) rather than at a human filing upstream, which is
the loophole that let 35 days pass with escalation technically not yet due.

## Technical Context

**Language/Version**: GitHub Actions YAML + POSIX bash + `actions/github-script` (Node). No Rust source changes.
**Primary Dependencies**: existing only — `actions/checkout`, `dtolnay/rust-toolchain`, `actions/github-script`, `Swatinem/rust-cache`, the in-repo `./.github/actions/install-bpf-linker` composite, `gh` CLI (preinstalled). All SHA-pinned. **Zero new Cargo dependencies; zero new marketplace actions.**
**Storage**: none. Streak state is the GitHub issue's `created_at`; run state is job outputs. No new persistence.
**Testing**: `workflow_dispatch` against a real runner (`version`, `dry_run`, and a new deliberate-break input). Not reachable from `cargo test` — see research R9 and quickstart.md.
**Target Platform**: `ubuntu-latest` GitHub-hosted runner; kernel-side target `bpfel-unknown-none`.
**Project Type**: CI/release infrastructure. The shipped `waybill` binary is byte-identical pre/post merge.
**Performance Goals**: none. Nightly job, 15-minute timeout, typical run ~5 min. The control build runs only on the failure path, so the green path stays at one build.
**Constraints**: the schedule, the watched component, and the pin mechanism are unchanged (spec Assumptions). The `[canary] bpf-linker eBPF build regression` title is frozen — #685 lives under it. `xtask` and `ci.yml`/`release.yml` are out of scope.
**Scale/Scope**: one workflow file, one docs page, one new dispatch input. ~1 issue title added.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | No first-party code added in any language; no dependency changes. The workflow already installs `clang`/`llvm` in sibling lanes for the toolchain, not for waybill's own code. **Pass.** |
| **II. eBPF-Only Observation** | Not engaged — this feature builds the eBPF artifact, it does not change what is observed or how the SBOM is populated. **N/A.** |
| **III. Fail Closed** | Directly reinforced. FR-002a makes a build that exits zero without producing an artifact a failure; the current canary would report it green. A canary whose green authorises a version bump must not emit one it cannot back with an artifact. **Pass — strengthened.** |
| **IV. Type-Driven Correctness** | No Rust domain types added. The attribution function is a total function over a closed outcome domain (data-model.md), which is the YAML-level equivalent. **Pass.** |
| **V. Specification Compliance** | No SBOM emission touched. **N/A.** |
| **VI. Three-Crate Architecture** | No crate boundaries touched. **Pass.** |
| **VII. Test Isolation** | The control build must not share a `CARGO_TARGET_DIR` with the subject build (research R5). Verification dispatches use `dry_run` so they do not mutate #685. **Pass.** |
| **VIII. Completeness / IX. Accuracy / X. Transparency** | Not SBOM output, but the same values applied to a CI signal: the whole feature is "do not assert a cause you have not established, and say what you actually observed." FR-005's evidence requirement is Transparency's shape at the report level. **Pass — aligned in spirit.** |
| **XI / XII. Enrichment** | Not engaged. **N/A.** |

**No violations. Complexity Tracking table omitted.**

One judgement call worth recording rather than hiding: the more durable fix is
to drop `+nightly` from `xtask::build_ebpf` so `waybill-ebpf/rust-toolchain.toml`
(which already declares `components = ["rust-src"]`) governs every build site.
That fixes the class rather than the instance. It is deliberately **not** taken
here — `build_ebpf` is shared with `release.yml`, cannot be verified from a
macOS dev machine, and a regression there costs a release rather than a
nightly. Recorded as follow-up in research.md.

## Project Structure

### Documentation (this feature)

```text
specs/896-fix-ebpf-canary/
├── plan.md                           # This file
├── spec.md                           # 16 FRs, 10 SCs, 4 clarifications
├── research.md                       # Phase 0 — R1..R9, all evidence-backed
├── data-model.md                     # Phase 1 — 5 entities, attribution table
├── quickstart.md                     # Phase 1 — dispatch-based verification
├── checklists/requirements.md        # 16/16 from /speckit.specify
├── contracts/
│   └── canary-workflow-v2.md         # Phase 1 — C-1..C-10, supersedes m234's
└── tasks.md                          # Phase 2 — NOT created by /speckit.plan
```

### Source (repository root)

```text
.github/
├── workflows/
│   ├── ebpf-canary.yml               # PRIMARY — environment, control build,
│   │                                 #   artifact check, attribution, two titles,
│   │                                 #   elapsed-days escalation
│   ├── ci.yml                        # REFERENCE ONLY — the divergence check's
│   │                                 #   comparison target; not modified
│   └── release.yml                    # REFERENCE ONLY; not modified
├── actions/install-bpf-linker/
│   └── action.yml                    # REFERENCE ONLY — invoked twice; not modified
└── env/bpf-linker.env                # READ — supplies the control version

docs/development/
└── ebpf-toolchain.md                 # Escalation-window wording; the
                                      #   responsiveness-gated escalation
                                      #   loophole (FR-007a) is fixed here

xtask/src/main.rs                     # NOT MODIFIED — see Constitution Check
waybill-ebpf/rust-toolchain.toml      # NOT MODIFIED
```

**Structure Decision**: single-file feature. `.github/workflows/ebpf-canary.yml`
carries essentially all of it; `docs/development/ebpf-toolchain.md` carries the
escalation-clock wording that FR-007a invalidates. Everything else in the tree
is read, compared against, or invoked — not changed. This is deliberate: the
spec bounds scope at "the trustworthiness of the signal, not its scope", and
the two lanes that currently work are the control this feature is measured
against. Changing them would remove the reference.

## Phase ordering

The delivery order follows the spec's priorities and is not interchangeable —
Story 1 is the precondition for observing anything about Stories 2 and 3.

1. **US1 (P1) — the canary can build.** Environment parity, artifact
   verification, divergence check. Gate: a dispatch against the pinned version
   goes green (SC-001). Until this passes, the control build in US2 has no
   known-good half and the attribution table cannot be exercised.
2. **US2 (P2) — failures self-attribute.** Control build, attribution function,
   second issue title, evidence in reports, per-kind close. Gate: the
   deliberate-break dispatch produces a canary-fault report (SC-003).
3. **US3 (P3) — long failures escalate.** Elapsed-days clock, escalation
   wording, docs fix. Gate: the rule evaluated against #685's `created_at`
   escalates (SC-006a).

## Post-design Constitution re-check

Re-evaluated after Phase 1. No new violations: the design adds no dependencies,
no persistence, no crate boundaries, and no code in any language other than
workflow YAML and bash. Principle III is measurably better served after the
design than before it (FR-002a closes the false-green path, which did not
previously exist as a guarded case).
