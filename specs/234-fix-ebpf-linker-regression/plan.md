# Implementation Plan: Durable eBPF Build Resilience After bpf-linker v0.11.0 Regression

**Branch**: `234-fix-ebpf-linker-regression` | **Date**: 2026-08-12 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/234-fix-ebpf-linker-regression/spec.md`

## Summary

**Primary requirement**: Move waybill off the interim `bpf-linker@0.10.4`
pin onto a currently-supported upstream version (US1); prevent the next
external-tool regression from surprising us at release time (US2); make
un-pin verification cheap enough that any contributor can run it (US3).

**Technical approach**:

- **US1 (Un-pin)** — Upstream-first per the clarify-session posture. File
  an upstream issue against `aya-rs/bpf-linker` with today's reproducer;
  contribute a fix if scoped small; if upstream doesn't move within the
  fallback window (see Phase 0 R2 — 30 days), execute the downstream
  mitigation (explicit LLVM path setup step in each install site) and
  un-pin to `--locked` latest. Either path produces a single bump PR
  whose merge criterion is the standard CI green + local harness green.
- **US2 (Canary)** — New scheduled workflow
  `.github/workflows/ebpf-canary.yml` runs every 24 hours (Phase 0 R1)
  on ubuntu-latest. Installs bpf-linker at HEAD (unpinned) + runs
  `cargo xtask ebpf` + `cargo build --features ebpf-tracing`. On
  failure, opens/updates a deduped GitHub issue titled
  `[canary] bpf-linker eBPF build regression` (per FR-003) with the
  installed bpf-linker version, the failing command, and the relevant
  log window. Reruns update the existing issue's timestamp comment
  rather than open new ones.
- **US3 (Local harness)** — Extend `scripts/pre-pr.sh` with a new
  opt-in flag (or extract `scripts/verify-ebpf.sh`) that invokes the
  existing `Dockerfile.ebpf-test` container harness end-to-end. On
  macOS/Windows the command routes through the container harness
  (matching the existing Colima/Docker Desktop pattern). On Linux with
  a native rust nightly + kernel headers, the harness may build
  directly against the host toolchain. Exit 0 iff the same build path
  release.yml executes succeeds.

**Single source of truth for the pin (FR-001)**: Phase 0 R3 decision —
introduce `BPF_LINKER_VERSION` env var in a shared file
`.github/actions/install-bpf-linker/action.yml` (composite action) that
all three install sites reuse. The install sites are: `ci.yml`,
`release.yml`, and `Dockerfile.ebpf-test`. (Analysis-phase N1
correction: `nightly.yml` was initially listed as a fourth site but
verified 2026-08-12 as containing zero bpf-linker references —
nightly.yml is a dispatcher that shells out to `release.yml` via
`gh workflow run`, so it inherits the pin transitively.) The Dockerfile
can `ARG` the same value via a build arg passed by CI or read from a
top-level `.env` file.

## Technical Context

**Language/Version**: N/A — this milestone modifies GitHub Actions YAML
+ Dockerfile + shell. No Rust source touched. Existing Rust toolchain
(stable for user-space, nightly for eBPF) is unchanged.

**Primary Dependencies**: `bpf-linker` (currently pinned to `0.10.4`;
US1 target: latest working upstream version). No new Cargo deps.
Workflow adds only actions already in use elsewhere in the repo
(`actions/checkout@…`, `dtolnay/rust-toolchain@stable`,
`Swatinem/rust-cache@v2`, `actions/github-script` for issue
create/update).

**Storage**: N/A. All state is per-workflow-run (canary logs) or
per-issue (dedupe key in issue title).

**Testing**:

- **Manual smoke test** — trigger the canary workflow via
  `workflow_dispatch` with an override input for a known-broken
  bpf-linker version (v0.11.0). Verify the issue is created/updated
  with correct body content.
- **Un-pin readiness command** — run
  `scripts/verify-ebpf.sh` (US3) against v0.10.4 (expect PASS) and
  v0.11.0 (expect FAIL with `bpf-linker` named in the log).
- **Regression guard** — after US1 lands, next nightly release build
  MUST pass without touching the eBPF step's install commands.

**Target Platform**: `ubuntu-latest` GitHub Actions runners
(currently ubuntu-24.04) + local Linux dev hosts + macOS/Windows via
container harness.

**Project Type**: CI / build-infrastructure hardening. Modifies
`.github/workflows/*.yml`, `.github/actions/install-bpf-linker/`,
`Dockerfile.ebpf-test`, `scripts/verify-ebpf.sh`, and one docs page.
No `Cargo.toml` changes.

**Performance Goals**: Canary detects regression ≤48hrs upstream (SC-004
bound); 24-hour cadence (Phase 0 R1) satisfies with margin.

**Constraints**:

- Zero new Cargo dependencies (workspace `Cargo.toml` untouched).
- No changes to Rust MSRV.
- No changes to nightly channel used for eBPF (still `nightly` with
  `rust-src` component per m020).
- The interim `skip_ebpf` workflow_dispatch input from PR #681 MUST
  stay in `release.yml` (FR-009).
- The canary MUST NOT run on every PR (would consume Actions quota
  and add noise); it runs on schedule + on-demand via
  `workflow_dispatch`.

**Scale/Scope**: 3 pinned install sites today (`release.yml`, `ci.yml`,
`Dockerfile.ebpf-test`). `nightly.yml` inherits the pin transitively
via its release.yml dispatch and requires no rewire (analysis-phase N1
verified 2026-08-12: zero bpf-linker references in nightly.yml).
Post-plan: 1 shared composite action + 1 env var + 1 canary workflow
+ 1 verify script + 1 docs note.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Waybill Constitution v2.1.0 principles evaluated:

- **I. Pure Rust, Zero C** — No new C source, no libbpf bindings, no new
  C compiler toolchains. `bpf-linker` (Rust + LLVM linkage) is a
  pre-existing eBPF build-toolchain dependency introduced in m020 and
  is covered by the pre-existing eBPF-tracing feature-gate. This
  milestone only pins/manages the version. ✅
- **II. eBPF-Only Observation** — Not applicable. Milestone touches
  build tooling, not the eBPF observation runtime. ✅
- **III. Fail Closed** — Not applicable to CI infrastructure per se, but
  the canary itself embodies fail-closed by exiting non-zero on
  regression and creating an issue rather than silently succeeding. ✅
- **IV. Type-Driven Correctness** — No Rust source touched. ✅
- **V. Specification Compliance** — No SBOM emission code path touched.
  ✅
- **VI. Three-Crate Architecture** — No new crates. ✅
- **VII. Test Isolation** — The canary + verify-ebpf script run the
  same eBPF-privileged path already gated behind
  `WAYBILL_PREPR_EBPF=1` (m020 feature-flag contract). No unit tests
  affected. ✅
- **VIII. Completeness** — Not applicable (build infra, not SBOM
  emission). ✅
- **IX. Accuracy** — Not applicable. ✅
- **X. Transparency** — The canary's auto-opened GitHub issue IS the
  transparency mechanism for external-tool drift. ✅
- **XI. Enrichment** — Not applicable. ✅
- **XII. External Data Source Enrichment** — Not applicable. ✅

**Strict Boundaries**:
- No lockfile-based dependency discovery — unchanged. ✅
- No MITM proxy — unchanged. ✅
- No C code — no new C. ✅
- No `.unwrap()` in production — no Rust source touched. ✅
- No file-tier duplicates in default mode — unchanged. ✅

**Pre-PR Verification** (m020 feature-gate contract): the
`WAYBILL_PREPR_EBPF=1 ./scripts/pre-pr.sh` local opt-in path continues
to work; the new `scripts/verify-ebpf.sh` (US3) is composable with it
(the plan-phase decision is whether to fold verify-ebpf into pre-pr.sh
under a flag OR keep it as a standalone script — see Phase 0 R4).

**Gate**: PASS. No violations, no waivers needed.

## Project Structure

### Documentation (this feature)

```text
specs/234-fix-ebpf-linker-regression/
├── plan.md              # This file (/speckit.plan output)
├── research.md          # Phase 0 output — cadence, fallback window, single-source mechanism
├── data-model.md        # Phase 1 output — entity + state definitions
├── quickstart.md        # Phase 1 output — how to run the local harness + canary manually
├── contracts/
│   ├── canary-workflow.md          # Behavioral contract for the canary
│   ├── install-bpf-linker-action.md # Composite action contract (single-source-of-truth)
│   └── verify-ebpf-script.md       # Contract for the local un-pin readiness command
├── checklists/
│   └── requirements.md  # From /speckit.specify
└── tasks.md             # Phase 2 output (from /speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
.github/
├── actions/
│   └── install-bpf-linker/         # NEW: composite action wrapping the pinned install
│       └── action.yml              #     input: version (default env BPF_LINKER_VERSION)
├── workflows/
│   ├── ci.yml                      # MODIFIED: replace inline install with composite action
│   ├── release.yml                 # MODIFIED: replace inline install with composite action
│   └── ebpf-canary.yml             # NEW: scheduled + workflow_dispatch canary (24hr cadence)
│                                   # NOTE: nightly.yml is NOT modified — it's a dispatcher
│                                   #       to release.yml and inherits the pin transitively
├── env/
│   └── bpf-linker.env              # NEW: single source of truth for BPF_LINKER_VERSION
│                                   #      (workflows source this via `. .github/env/…`)

Dockerfile.ebpf-test                # MODIFIED: ARG BPF_LINKER_VERSION with env-driven default
                                    #           CI passes --build-arg from .github/env/bpf-linker.env

scripts/
├── pre-pr.sh                       # UNCHANGED (per R4 decision — verify-ebpf is standalone)
└── verify-ebpf.sh                  # NEW: un-pin readiness command (US3)

docs/
└── development/
    └── ebpf-toolchain.md           # NEW OR EXTENDED: how the pin works, how to un-pin,
                                    #                  what the canary means, how to read
                                    #                  its failure issues
```

**Structure Decision**: This is CI/infrastructure-only. No Rust source
tree changes. The composite action + env file + canary + verify script
+ docs page are the five new artifacts. Every existing consumer of
`bpf-linker` gets rewired to the composite action so future bumps are
a one-line edit.

## Complexity Tracking

*Not required — Constitution Check passed with no violations.*
