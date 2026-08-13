# Feature Specification: Durable eBPF Build Resilience After bpf-linker v0.11.0 Regression

**Feature Branch**: `234-fix-ebpf-linker-regression`
**Created**: 2026-08-12
**Status**: Draft
**Input**: User description: "let's fix the ebpf issue"

## Context (informational)

On 2026-08-12, upstream `bpf-linker` published v0.11.0. Every `cargo install
bpf-linker` in our release + CI pipelines picked up the new version and
failed on ubuntu-latest with `unable to find library -lLLVM`. Symptoms:

- Release run 31638264005 (v0.2.1-alpha.1) initially failed at the eBPF
  build job (root cause: bpf-linker v0.11.0 LLVM path drift).
- Every PR's `Lint + test (linux-x86_64, --features ebpf-tracing)` lane
  failed concurrently, including the hotfix PR itself.
- Interim hotfix (PR #681): pinned `bpf-linker --version 0.10.4` at all
  three install sites (`.github/workflows/release.yml`,
  `.github/workflows/ci.yml`, `Dockerfile.ebpf-test`) and added a
  `skip_ebpf` workflow_dispatch escape hatch to `release.yml`. Post-pin,
  eBPF build in release run 31638264005 succeeded.

The interim pin restored the release path but leaves us on an
increasingly stale bpf-linker minor version. This spec covers the durable
fix — the set of work that lets us un-pin safely and prevents a similar
external-tool regression from blocking future releases without prior
warning.

The `Publish multi-arch container image` job failed in release run
31638264005 for an unrelated reason (transient GitHub Releases CDN 503
during `cosign-installer` fetch of `cosign v3.0.6`; recovered on
first rerun without any code change); that failure is explicitly out
of scope for this spec.

## Clarifications

### Session 2026-08-12

- Q: Confirm scope of the durable eBPF fix — all three stories, or a
  narrower subset? → A: All three stories (P1 un-pin + P2 canary + P3
  local harness).
- Q: Upstream fix vs downstream mitigation posture? → A: Upstream first;
  downstream mitigation only if upstream is unresponsive within a
  defined window (window length to be set in the plan phase).
- Q: Canary notification channel? → A: Auto-open a GitHub issue on
  canary failure (single `gh issue create` step, deduped by title so
  reruns don't spam).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Un-pin bpf-linker and stay on a supported version (Priority: P1)

The waybill release lead needs to move off the pinned `bpf-linker@0.10.4`
onto a currently-supported bpf-linker version (whichever the upstream
maintainers recommend at the time this work lands — likely v0.12+ once
the LLVM-path regression is resolved, or a re-verified v0.10.x if that's
the long-term supported line). Keeping the pin indefinitely means the
waybill CI + release lanes stop receiving upstream bpf-linker fixes,
including any future security or correctness patches. The release lead
should be able to bump the pinned version, run the standard pre-PR gate
plus a targeted eBPF verification, and know within one iteration whether
the newer version works on our runner OS + LLVM stack.

**Why this priority**: This is the only story that actually *fixes* the
eBPF issue in the durable sense — the pin is a workaround. Without an
un-pin path, we accumulate technical debt every week bpf-linker moves
forward, and we lose the ability to consume upstream fixes.

**Independent Test**: Bump the pinned bpf-linker version in all three
install sites to the target version. Run `./scripts/pre-pr.sh` locally
with `WAYBILL_PREPR_EBPF=1`. Push a PR. Observe the `ebpf-tracing` lane
pass. If it fails, the failure diagnostic should point at a concrete
cause (LLVM missing, path drift, or a genuine bpf-linker bug we can
report upstream).

**Acceptance Scenarios**:

1. **Given** the release lead has identified an unblocking bpf-linker
   version published upstream, **When** they update the three pinned
   install sites and push a PR, **Then** the PR's `ebpf-tracing` lane
   either passes cleanly OR fails with a specific, actionable diagnostic
   (not the generic `unable to find library -lLLVM`).
2. **Given** the un-pin PR passes CI, **When** the release lead runs the
   local `Dockerfile.ebpf-test` harness, **Then** the container-level
   eBPF integration test passes end-to-end against the newer bpf-linker.
3. **Given** the un-pin PR merges, **When** the next nightly release
   fires, **Then** the release completes without failing at the eBPF
   build job.

---

### User Story 2 - Detect bpf-linker (and eBPF-toolchain) regressions before they block a release (Priority: P2)

The waybill maintainers want an early-warning signal that a new
bpf-linker release (or any change to the ubuntu-latest runner's LLVM
stack) has broken our eBPF build path — *before* it blocks a release
cut. Right now, the external-tool-drift blast radius includes every PR's
`ebpf-tracing` lane plus the weekly nightly plus any ad-hoc alpha
release; the first person to notice is whoever tries to ship, not the
maintainers. A dedicated canary should detect the regression within one
canary cycle (see SC-004 for the cadence target) and post a single,
clearly-labeled failure that identifies the external tool as the cause,
so the on-call maintainer knows immediately that the underlying pipeline
change is upstream, not in-repo.

**Why this priority**: This closes the "we only find out at release
time" gap that surfaced today. It's P2 (not P1) because the P1 un-pin
story is what actually addresses the current break; the canary just
prevents the next break from repeating today's escalation shape.

**Independent Test**: Manually publish a synthetic "poison" bpf-linker
version (or simulate one via a mock install path) that reproduces the
LLVM path failure. The canary should fire within its scheduled interval
and produce a maintainer-visible failure (email, GitHub issue, or
workflow status) that names bpf-linker explicitly.

**Acceptance Scenarios**:

1. **Given** a hypothetical future bpf-linker release re-breaks the eBPF
   build path, **When** the canary runs on its schedule, **Then** the
   canary fails visibly and the failure message points at `bpf-linker`
   (not at waybill source, not at generic "cargo install failed", not at
   a broken LLVM header).
2. **Given** the canary is green for four consecutive runs, **When** the
   release lead prepares an un-pin PR, **Then** the canary history is a
   reference point they can cite as evidence the target bpf-linker
   version is safe.
3. **Given** the canary fails, **When** the maintainer investigates,
   **Then** the failure log includes the exact bpf-linker version that
   was installed and the exact `cargo install` command that failed, so
   upstream reporting is trivial.

---

### User Story 3 - Repeatable un-pin readiness check (Priority: P3)

Any contributor (not just the release lead) should be able to run a
single documented command locally to verify whether a proposed
bpf-linker version is safe to un-pin to. The command should exercise the
same build path release.yml uses (kernel-side eBPF object build plus
userspace binary build with `--features ebpf-tracing`), so a local pass
gives the same confidence as a CI pass. This decouples "verify a
candidate bpf-linker version" from "open a PR and wait for CI" and lets
contributors iterate quickly on the un-pin question.

**Why this priority**: This is a quality-of-life improvement on top of
P1 and P2. Without it, un-pin remains a slow, PR-round-trip loop. It's
P3 because the P1 un-pin path still works — just less pleasantly —
without it.

**Independent Test**: A contributor runs the documented command against
bpf-linker v0.10.4 (the current pin) and observes success. They run the
same command against a known-broken version (bpf-linker v0.11.0 as of
2026-08-12) and observe a clear failure that names the tool.

**Acceptance Scenarios**:

1. **Given** the un-pin readiness command is documented, **When** a
   contributor runs it against a candidate bpf-linker version on a Linux
   dev host, **Then** the command exits 0 if the eBPF build path works
   end-to-end and exits non-zero (with a clear log) if it doesn't.
2. **Given** the command passes locally, **When** the contributor pushes
   the un-pin PR, **Then** CI agrees with the local result (same
   pass/fail conclusion).
3. **Given** the contributor is on macOS or Windows, **When** they run
   the un-pin readiness command, **Then** it either runs successfully
   through a container path (matching `Dockerfile.ebpf-test`) OR emits a
   clear message that eBPF verification requires Linux + the container
   harness.

---

### Edge Cases

- **Upstream bpf-linker is unavailable or unresponsive to a bug
  report** — the fallback window (per FR-011) expires and we execute
  the downstream mitigation on our install path; the canary (P2)
  continues to run to detect when a new upstream version does become
  available so we can revisit un-pin later.
- **The ubuntu-latest runner image changes its LLVM version** — the
  canary should still catch this even if bpf-linker itself hasn't
  changed, because the canary runs on the same image the release uses.
- **A future bpf-linker version breaks in a new way (not LLVM path)** —
  the un-pin readiness command should surface the new failure mode
  clearly enough that a maintainer can either report upstream or add a
  new install-side mitigation.
- **Contributor tries the un-pin readiness command on Windows/macOS** —
  the command should degrade gracefully (skip with a clear message or
  route through a container) rather than crash mid-run.
- **The pinned bpf-linker version itself gets yanked from crates.io** —
  the release + CI + Dockerfile install commands all fail at the same
  point; the failure diagnostic should distinguish this case
  ("bpf-linker@0.10.4 not found") from the current breakage shape
  ("LLVM path drift").

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a single source of truth for the
  bpf-linker version pin, so bumping the version across all three
  install sites (release.yml, ci.yml, Dockerfile.ebpf-test) is a single
  edit and version drift between sites cannot happen accidentally.
- **FR-002**: System MUST allow the release lead to un-pin `bpf-linker`
  from v0.10.4 to a currently-supported upstream version in a single PR
  whose success criterion is standard CI green + local eBPF harness
  green.
- **FR-003**: System MUST auto-open a GitHub issue against
  `kusari-oss/waybill` when a scheduled canary run detects a bpf-linker
  regression. The issue MUST be created within one canary cycle of the
  regression appearing upstream, and MUST be deduped by a stable title
  (e.g., `[canary] bpf-linker eBPF build regression`) so consecutive
  reruns update the existing issue instead of spamming new ones.
- **FR-004**: The canary MUST identify `bpf-linker` explicitly in the
  issue body / failure message when the failure is attributable to the
  tool (distinguishing it from a waybill-side regression or a
  runner-image regression).
- **FR-005**: The canary MUST log AND include in the auto-opened issue
  body the exact bpf-linker version installed at run time and the
  exact install command that succeeded or failed, so upstream bug
  reports are trivial to compose.
- **FR-006**: System MUST provide a repeatable local verification
  command that exercises the same build path release.yml uses
  (kernel-side eBPF object + userspace binary + integration), returning
  zero on success and non-zero with a clear log on failure.
- **FR-007**: The local verification command MUST work on Linux natively
  and MUST degrade gracefully on macOS/Windows (either via container
  harness or by emitting a clear "Linux required" message).
- **FR-008**: System MUST NOT remove eBPF support from waybill (this is
  a resilience feature, not a functionality regression).
- **FR-009**: System MUST preserve the interim `skip_ebpf` escape hatch
  in `release.yml` for future incident response.
- **FR-010**: System MUST document the un-pin decision and verification
  evidence (canary history + local harness output) somewhere the next
  release lead can find it (a spec close-out, a docs/ note, or a
  reference issue).
- **FR-011**: System MUST attempt upstream investigation FIRST — file
  an upstream issue against `bpf-linker` with a reproducer, and either
  contribute a fix or wait for one. If upstream is unresponsive within
  a defined fallback window (window length to be decided in the plan
  phase), system MUST fall back to a downstream mitigation on our
  install path. The final outcome MUST produce EITHER an upstream PR
  link OR a documented downstream mitigation with rationale, captured
  in the spec close-out.

### Key Entities

- **bpf-linker install site**: One of the three current locations where
  `cargo install bpf-linker` runs — `release.yml`, `ci.yml`,
  `Dockerfile.ebpf-test`. All three currently point at v0.10.4 and must
  move in lockstep.
- **Canary run**: A scheduled workflow invocation that installs a
  configurable bpf-linker target (latest, or a specific candidate
  version) and attempts a full eBPF build + user space build.
- **Un-pin readiness verdict**: The pass/fail conclusion the canary +
  local harness produce for a candidate bpf-linker version. Feeds
  directly into whether the release lead is comfortable opening the
  un-pin PR.
- **Bump PR**: The PR that moves the pinned bpf-linker version. Its
  success criterion is CI + local harness green (see SC-001).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A single, focused bump PR moves bpf-linker from v0.10.4
  to a currently-supported version, passes CI green on the first push
  (no fixup commits), and merges without special handling.
- **SC-002**: Zero release failures attributable to bpf-linker (LLVM
  path drift, install failure, minor-version drift) for the next 90
  days post-un-pin.
- **SC-003**: A contributor unfamiliar with the eBPF path can, given
  the documented un-pin readiness command, verify a candidate
  bpf-linker version end-to-end in under 30 minutes on a Linux dev host
  (or under 60 minutes via the container harness on macOS/Windows).
- **SC-004**: The canary detects a hypothetical future bpf-linker
  regression within 48 hours of the regression appearing upstream
  (assuming a daily or every-other-day canary cadence — exact cadence
  to be decided in the plan phase, but this outcome bounds the space).
- **SC-005**: The canary's failure message is specific enough that the
  investigating maintainer can compose an upstream bug report in under
  15 minutes without further debugging.

## Assumptions

- The 2026-08-12 upstream regression in bpf-linker v0.11.0 (LLVM
  library path drift on ubuntu-latest) is a real, reproducible bug —
  not a transient CDN issue or a one-off runner state problem. If it
  turns out transient, this spec's P1 collapses to "bump to latest and
  un-pin" without upstream work.
- The `Publish multi-arch container image` failure in release run
  31638264005 is unrelated to bpf-linker and is out of scope for this
  spec.
- The pinned v0.10.4 is a safe, functionally-complete bpf-linker
  version for our current eBPF workload — i.e., there is no
  user-visible functional gap on v0.10.4 that we're accumulating by
  staying pinned temporarily.
- Existing CI infrastructure (GitHub Actions with a schedulable
  workflow surface) is sufficient to host the canary; no new external
  service is needed.
- Existing container harness (`Dockerfile.ebpf-test`) is the right
  substrate for the local un-pin readiness check; we're not building a
  new harness from scratch.
- The maintainer group is reached via GitHub issue notifications
  (subscription to `kusari-oss/waybill` issues); the canary
  auto-opens a deduped issue on failure per FR-003.
- The `WAYBILL_PREPR_EBPF=1` local pre-PR opt-in flag continues to be
  the entry point for full-PR eBPF verification. Per Phase 0 research
  R4, the un-pin readiness command (US3) ships as a standalone script
  (`scripts/verify-ebpf.sh`) with no coupling to `pre-pr.sh` — the two
  paths remain independent so either can evolve without touching the
  other.
- No MSRV bump is required to un-pin bpf-linker (as of research time —
  the plan phase will re-verify).
