# Phase 0 Research: Durable eBPF Build Resilience

**Feature**: `234-fix-ebpf-linker-regression`
**Date**: 2026-08-12
**Purpose**: Resolve the three items the spec deferred to the plan phase
(canary cadence, upstream fallback window, single-source-of-truth
mechanism) plus one plan-emerged question (verify-ebpf script layout).

---

## R1 — Canary cadence

**Decision**: **Daily at 06:00 UTC** (matching the existing nightly
release cron so ops signals cluster in one wake window).

**Rationale**:

- SC-004 bounds detection at ≤48 hours upstream. Daily satisfies with a
  ~1-day margin.
- The existing `.github/workflows/nightly.yml` fires at `0 6 * * *`
  (06:00 UTC per m229). A canary at the same hour means whoever is
  investigating a nightly failure has the canary result on the same
  page — natural correlation.
- GitHub Actions quota impact is negligible: one ubuntu-latest job,
  ~5 minutes of runtime, once/day = ~150 min/month vs the ~40k-min
  monthly free-tier allowance.
- Twice-daily (12h) buys only marginal detection improvement; upstream
  crates.io publications are single events, not streams, and 24h is
  already inside the SC-004 budget.
- `workflow_dispatch` input remains available for on-demand runs when
  an operator suspects a new bpf-linker release is being cut.

**Alternatives considered**:

- **6-hour cadence** — 4× the Actions load for no operational benefit
  (SC-004 already satisfied). Rejected.
- **Twice-daily (12h)** — Marginal detection improvement; discarded
  for the same reason.
- **On push to main** — Wouldn't detect upstream drift between pushes;
  fails the "catch it before release" purpose.
- **Weekly** — Would miss SC-004's 48h window; rejected.

---

## R2 — Upstream fallback window length

**Decision**: **30 calendar days** from upstream-issue-open to
downstream-mitigation-execute.

**Rationale**:

- 30 days is a conventional courtesy window that gives upstream one
  release cycle plus buffer. `bpf-linker` upstream shipped
  0.10 → 0.11 in roughly 90 days historically; 30 days is one-third
  of that, sufficient signal without indefinite blockage.
- Long enough that a small fix contributed by us (or the upstream
  maintainer) can land, be released, and be verified. Short enough
  that we're not stuck on an increasingly stale pin.
- Matches typical OSS security-advisory embargo windows (Sigstore
  uses ~30-90 day; GitHub Security Advisory workflow suggests 30-90).
  We're being reasonably patient without being lax.
- Practical mechanism: US1 opens a tracking issue on our side titled
  `[m234] upstream bpf-linker fallback window`. The issue's opened-at
  timestamp starts the clock; a manual comment 30 days later
  ("window expired, executing downstream mitigation") starts US1b.
- If upstream lands a fix mid-window, we un-pin immediately and close
  the tracking issue.

**Alternatives considered**:

- **14 days** — Too short; upstream release cycles run monthly-ish.
  Would force downstream mitigation in most realistic scenarios.
  Rejected.
- **60 days** — Too long; leaves us on a stale pin for two months.
  Rejected.
- **Open-ended (revisit case-by-case)** — Leaves the un-pin decision
  in ambient limbo, which is exactly the problem this spec addresses.
  Rejected.

---

## R3 — Single source of truth for the bpf-linker version pin

**Decision**: **Composite GitHub Action** at
`.github/actions/install-bpf-linker/action.yml` + version env var in
`.github/env/bpf-linker.env` that the action reads by default.

**Rationale**:

- Composite actions are the GitHub-native mechanism for reusing
  install logic across multiple workflows. Any yaml site that today
  runs `cargo install bpf-linker --version 0.10.4` becomes one line:
  `uses: ./.github/actions/install-bpf-linker`.
- The `.env` file gives Docker a natural consumption point:
  `docker build --build-arg BPF_LINKER_VERSION=$(cat .github/env/bpf-linker.env | grep BPF_LINKER_VERSION | cut -d= -f2)` — and Dockerfile's `ARG BPF_LINKER_VERSION` picks it up.
- Bumping the pin becomes a one-line change to
  `.github/env/bpf-linker.env`. `git blame` on that file is the
  version-history log.
- The composite action can add a `version` input that overrides the
  env for the canary use case (canary passes `version: latest`).
- Dependabot can be pointed at the env file via a custom updater rule
  in a follow-up (out of scope for m234's core).

**Alternatives considered**:

- **Matrix input at every workflow site** — DRY violation; a bump
  still requires N edits. Rejected.
- **Extract a workflow_call reusable workflow** — heavier than needed;
  composite action is the right granularity for a single
  install-step.
- **Hard-code the version in the composite action only (no .env file)** —
  loses the Docker consumption path; forced to duplicate the version
  in `Dockerfile.ebpf-test`. Rejected.
- **Pre-build a Docker image with bpf-linker baked in and pull it in
  CI** — heavier maintenance; requires a registry + rebuild pipeline;
  overkill for a 30-second `cargo install`. Rejected.

---

## R4 — verify-ebpf.sh: standalone vs folded into pre-pr.sh

**Decision**: **Standalone** at `scripts/verify-ebpf.sh`. Callable
directly. `pre-pr.sh` remains unchanged. Documentation notes both
paths.

**Rationale**:

- `pre-pr.sh` is called on every PR by contributors who may not care
  about eBPF (macOS default lane, m020's feature-gate contract).
  Injecting eBPF verification into it — even under a flag — invites
  confusion about which flag does what
  (`WAYBILL_PREPR_EBPF=1` vs a new `WAYBILL_VERIFY_EBPF=1`).
- Standalone script gives the un-pin readiness command a single,
  memorable name (`scripts/verify-ebpf.sh`). Docs can say
  "run `scripts/verify-ebpf.sh` to check bpf-linker readiness" without
  qualification.
- The existing `WAYBILL_PREPR_EBPF=1` opt-in remains as-is; a
  contributor who wants full eBPF pre-PR verification runs it. A
  contributor who *only* wants un-pin readiness runs
  `scripts/verify-ebpf.sh` — much faster (just the eBPF build path,
  not the full test suite).
- No cross-script coupling means either can evolve independently.

**Alternatives considered**:

- **Fold into pre-pr.sh under a new flag** — adds a flag matrix; ties
  un-pin verification to full-PR-readiness verification. Rejected.
- **Replace the existing `WAYBILL_PREPR_EBPF=1` path entirely** —
  loses full-PR eBPF verification. Rejected.

---

## Cross-cutting: Docker layer caching for the canary

**Decision**: The canary does NOT reuse the Dockerfile.ebpf-test
harness. It runs directly on ubuntu-latest with a fresh
`cargo install bpf-linker` (unpinned or version-input). Rationale:

- Building the ebpf-test image on every canary run costs ~10 min for
  layer caching to warm up. Running the two build commands
  (`cargo xtask ebpf` + `cargo build --features ebpf-tracing`)
  directly on ubuntu-latest matches release.yml's shape and completes
  in ~5 min.
- The canary is a *toolchain resilience* check, not an *integration*
  check. Dockerfile.ebpf-test's job is end-to-end eBPF-attach
  integration; canary's job is "does cargo install → cargo build
  succeed with the latest bpf-linker?".
- The verify-ebpf.sh script (US3) is where the container harness
  shows up — that path is optimized for *contributor* verification,
  not scheduled canary.

**Consequence**: Two distinct verification surfaces:
1. Canary (US2) — ubuntu-latest, `cargo install` + `cargo build`,
   fast, scheduled.
2. verify-ebpf.sh (US3) — container harness end-to-end, thorough,
   on-demand.

---

## Summary: all NEEDS CLARIFICATION resolved

| Item | Decision |
|---|---|
| Canary cadence | Daily at 06:00 UTC (colocated with nightly cron) |
| Upstream fallback window | 30 calendar days |
| Single source of truth | Composite action + `.env` file |
| verify-ebpf.sh layout | Standalone, no pre-pr.sh coupling |
| Canary uses Docker harness? | No — direct ubuntu-latest run |
