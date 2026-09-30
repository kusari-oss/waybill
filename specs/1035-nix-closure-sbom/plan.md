# Implementation Plan: Nix derivation closure as SBOM content

**Branch**: `1035-nix-closure-sbom` | **Date**: 2026-09-29 | **Spec**: [spec.md](./spec.md)
**Issues**: **#1034** (closure depth), **#1040** (nixpkgs vulnerability signals)
**Input**: Feature specification from `/specs/1035-nix-closure-sbom/spec.md`

## Summary

Emit what a Nix build actually consumed, and the vulnerability evidence nixpkgs
already records but no version string can express.

Milestone 1034's tier emits 53 and 190 components for two real Haskell
libraries. Their derivation closures hold 1,275 and 1,535 derivations, of which
216 and 218 artifact inputs are absent from today's output. Separately, 43 and
50 patch derivations — 3 and 4 naming CVEs — carry backported security fixes
against version strings that never moved.

Three things follow, all from measurement rather than design preference:

- **Supplement, not replace** (research R1). The manifest set covers every
  cabal stanza; the closure covers only what the chosen attribute builds. The
  original hypothesis for the 30-component gap was measured and disproved.
- **Patches are not components.** They go in CycloneDX `pedigree.patches[]`, a
  native carrier verified against `bom-1.6.schema.json`.
- **A backport yields two graded VEX statements**, not one — `affected` for the
  version, `not_affected` for this build (clarified 2026-09-29).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde_json` (closure JSON is 5.7–7.4
MB; parsing is the one place volume matters), `std::process::Command` with the
budgeted-subprocess pattern from milestone 1034's `eval/invoke.rs`, `regex`
(already direct; CVE extraction from patch names), `clap`, `tracing`,
`anyhow`/`thiserror`. **Zero new Cargo dependencies.**
**External runtime dependency**: the `nix` binary, opt-in, absent by default.
**Storage**: N/A — in-process per scan. The closure JSON is parsed and dropped;
nothing is cached, because Nix's own store and eval cache already serve that
role.
**Testing**: `./scripts/pre-pr.sh`; the m195 corpus harness; the m1034
integration suite as the pattern for degradation coverage.
**Target Platform**: wherever `nix` runs. Windows degrades, as today.
**Project Type**: CLI subcommand behaviour (`waybill sbom scan`).
**Performance Goals**: closure query measured at 1.09 s / 5.7 MB and 1.07 s /
7.4 MB warm. Cold-store cost unmeasured (research R6, task T-R3).
**Constraints**: the project flake MUST be addressed through the CLI flakeref
form, because `builtins.getFlake` on a local path requires `--impure` (R4);
PR #1044's argv guard MUST hold (R5); `--offline` refuses, inheriting m1034.
**Scale/Scope**: ~1,300–1,550 derivations per closure; ~380 artifact inputs.

## Constitution Check

*GATE: evaluated before Phase 0, re-evaluated after Phase 1. Constitution v3.0.0.*

| Principle | Verdict | Basis |
|---|---|---|
| **I. Pure Rust, Statically Linked** | **PASS** | Subprocess invocation links nothing. Settled for this code path by milestone 1034's research R1. |
| **II. eBPF-Only Observation** | **N/A** | Trace-path scoped; this is `sbom scan`. |
| **III. Fail Closed** | **PASS, by scope** | Scoped by its own text to the eBPF trace. Principle XI mandates the opposite for enrichment — emit with the field omitted and a transparency annotation. Same reasoning as m1034. |
| **IV. Type-Driven Correctness** | **PASS, with obligation** | Closure member, patch record and evidence grade are domain values and get newtypes/enums; `thiserror` for degradation. |
| **V. Specification Compliance** | **PASS — and the point** | `pedigree.patches[]` is a *native* carrier, verified against the schema. This is the first waybill feature to use `pedigree` at all. Annotation bridges only where SPDX has no equivalent. |
| **VI. Three-Crate Architecture** | **PASS** | Entirely within `waybill-cli`. |
| **VII. Test Isolation** | **PASS, with obligation** | Anything mutating `PATH` or nix env routes through `crate::testing::EnvGuard::acquire()`. |
| **VIII. Completeness** | **PASS — and the point** | 216 and 218 components currently absent. |
| **IX. Accuracy** | **PASS, and load-bearing** | The evidence grade (FR-009, FR-012a) exists because a CVE parsed from a filename is weaker than it looks. See Complexity Tracking. |
| **X. Transparency** | **PASS** | FR-010 (patches without a CVE), FR-018 (emitted/suppressed/patch counts), the scope marker, and the evidence grade. |
| **XI. Enrichment** | **PASS** | Degradation with transparency annotations is XI's explicit requirement. |
| **XII. External Data Source Enrichment** | **PASS, with note** | Constraint 1 forbids introducing components not observed in the trace. Written for the trace-first model; see Complexity Tracking. |
| **Strict Boundary 1** (no lockfile discovery) | **N/A, by scope** | Trace-path scoped; the whole `scan_fs` subsystem operates outside it. |
| **Strict Boundary 3** | **PASS** | Nothing linked, no C authored. |
| **Strict Boundary 4** | **PASS, with obligation** | `clippy::unwrap_used` denied at the crate root, enforced in tests by `--all-targets`. |

**Gate result: PASS.** Two items carried to Complexity Tracking.

## Complexity Tracking

| Item | Why needed | Simpler alternative rejected because |
|---|---|---|
| **Emitting components no manifest declares**, against Principle XII constraint 1 ("external sources MUST NOT introduce new components") | XII is written for the trace-first model, where the trace is authoritative and a lockfile must not add to it. On the `sbom scan` path there is no trace, and the derivation closure is a *stronger* statement about what a build consumed than any manifest — it is what nix will actually build. Suppressing 216 components to satisfy a trace-path rule would make the document knowingly incomplete. | **Emitting only components a manifest also declares** — rejected: that is the current behaviour, and the 216/218 gap is the feature. **Emitting them unmarked** — rejected: Principle X requires the provenance. **Resolution**: emit with derivation provenance and a scope marker, so a consumer applying a strict trace-first reading can filter. |
| **Asserting a VEX status from a filename** | A CVE parsed from `CVE-2019-13232-1.patch` is the only in-band evidence a backport exists. Refusing to use it means the signal is lost entirely, and nothing else in an SBOM can express "patched without a version bump". | **Emitting `not_affected` alone** — rejected as overclaiming; a consumer could suppress a real finding on a filename match. **Emitting nothing** — rejected; discards the only available evidence. **Resolution**: two statements with different subjects, both carrying an evidence grade that identifies the derivation as filename-based (FR-011, FR-012a). |

## Project Structure

### Documentation (this feature)

```text
specs/1035-nix-closure-sbom/
├── spec.md
├── plan.md                      # this file
├── research.md                  # Phase 0 — two findings overturned spec assumptions
├── measurements/
│   └── closure-vs-emitted.py    # the overlap probe behind SC-001
├── data-model.md                # Phase 1
├── contracts/
│   ├── cli-flags.md
│   ├── closure-invocation.md
│   └── emission.md
├── quickstart.md
└── tasks.md                     # /speckit.tasks — not created here
```

(The closure classifier lives at
`specs/1034-nix-eval-tier/measurements/classify-derivation-closure.py`, where
it was first written; it is cited rather than copied.)

### Source Code (repository root)

```text
waybill-cli/src/
├── cli/
│   └── scan_cmd.rs                          # + the opt-in flag and attribute override
└── scan_fs/package_db/nix/
    ├── eval/                                # (m1034) reused: preflight, invoke, reason
    │   └── invoke.rs                        #   + the CLI-flakeref closure query
    └── closure/                             # NEW
        ├── mod.rs                           #   orchestration + degradation
        ├── derivation.rs                    #   parse `nix derivation show -r` output
        ├── classify.rs                      #   artifact / tooling, from nix's own fields
        ├── patches.rs                       #   patch records + CVE extraction + grade
        └── emit.rs                          #   components, pedigree, VEX statements

waybill-cli/src/generate/cyclonedx/
└── pedigree.rs                              # NEW — first use of pedigree in waybill

waybill-cli/tests/
├── nix_closure.rs                           # NEW
└── fixtures/nix_closure/                    # NEW
```

**Structure Decision**: a sibling `closure/` module rather than growing
`eval/`. The two share the safety machinery — pre-flight, argv guard, budget,
degradation reasons — and that is reused directly. They differ in what they
ask nix and what they produce: `eval/` asks for versions of a name list and
corrects a package set; `closure/` asks for a derivation graph and produces
components, pedigree and VEX. Folding them would put two unrelated output
shapes behind one entry point.

## Constitution Check — post-design re-evaluation

*Re-run after Phase 1. Artifacts: `research.md`, `data-model.md`,
`contracts/{closure-invocation,cli-flags,emission}.md`, `quickstart.md`.*

**Gate result: PASS, unchanged.** Obligations discharged:

| Pre-Phase-0 obligation | Discharged by |
|---|---|
| **IV** — newtypes for domain values | `data-model.md`: `ClosureMember`, `DerivationRole`, `PatchRecord`, `EvidenceGrade`. The last is a single-variant enum on purpose, so a future stronger provenance is distinguishable rather than indistinguishable from a filename match. |
| **V** — native carriers first | `pedigree.patches[]`, verified against the schema, for the fact itself. Three bridge rows (C182–C184) each audited against a candidate native field and rejected for a stated reason. |
| **VII** — test isolation | `quickstart.md` degradation table; anything mutating `PATH` or nix env routes through `EnvGuard`. |
| **IX / X** — accuracy and transparency | The evidence grade, the two-statement VEX split, and the patches-without-a-CVE count, which is what lets a consumer judge coverage rather than read absence as absence. |

**Phase 0 changed the design rather than confirming it.** Research R1 disproved
the spec's hypothesis for the 30-component gap — the packages come from an
executable stanza and from GHC's boot set, not another flake attribute — which
settled replace-versus-supplement as **supplement**, on measurement. R2 showed
that treating closure-absence as evidence of spuriousness would discard the
Haskell standard distribution. Both were propagated back into the spec
(FR-003a) rather than left in the plan.

**Complexity Tracking is unchanged**: the same two items, both about claiming
neither more nor less than the evidence supports.

## Phase status

| Phase | Status | Output |
|---|---|---|
| 0 — Research | **complete** | `research.md`, `measurements/closure-vs-emitted.py` |
| 1 — Design & contracts | **complete** | `data-model.md`, three contracts, `quickstart.md`, agent context |
| 2 — Tasks | not started | `/speckit.tasks` |

**Carried into implementation**, each needing a committed probe: **T-R1**
attributing a patch derivation to the component it patches — the closure
records both, the join is undemonstrated; **T-R2** whether closure composition
holds outside Haskell; **T-R3** cold-store cost.

T-R1 is the one that could still reshape the feature: if patches cannot be
mechanically attributed to components, the pedigree half has no anchor and
would need a different carrier.
