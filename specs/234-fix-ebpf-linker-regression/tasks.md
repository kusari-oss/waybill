---

description: "Task list for m234 — Durable eBPF Build Resilience After bpf-linker v0.11.0 Regression"

---

# Tasks: Durable eBPF Build Resilience After bpf-linker v0.11.0 Regression

**Input**: Design documents from `/specs/234-fix-ebpf-linker-regression/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Tests are minimal — the spec calls for a manual canary smoke test (Scenario E in quickstart.md) and a verify-ebpf.sh self-test. No new Rust tests. Rationale: this milestone is CI/build-infrastructure only; the Rust workspace's existing `ebpf-tracing` lane in `ci.yml` IS the regression test — if it goes green post-un-pin, US1 is verified.

**Organization**: Tasks grouped by user story (US1 = un-pin, US2 = canary, US3 = local harness). Setup + Foundational carry the shared substrate (composite action + env file) both US1 and US2 depend on.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

Repository root is `/Users/mlieberman/Projects/mikebom/`. All paths below are repo-relative.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish the directory structure the composite action + env file live in.

- [X] T001 Create directory `.github/actions/install-bpf-linker/`
- [X] T002 Create directory `.github/env/`
- [X] T003 [P] Add `.github/env/README.md` explaining the purpose of the env directory (single source of truth for pinned tool versions; documented under FR-001 of spec.md)

**Checkpoint**: Directory scaffolding exists. Move to Phase 2.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Ship the `BpfLinkerPin` env file + `install-bpf-linker` composite action. Both US1 and US2 depend on these.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T004 Create `.github/env/bpf-linker.env` with `BPF_LINKER_VERSION=0.10.4` (matches interim hotfix from PR #681; per data-model.md `BpfLinkerPin` entity)
- [X] T005 Create `.github/actions/install-bpf-linker/action.yml` implementing the composite-action contract at `specs/234-fix-ebpf-linker-regression/contracts/install-bpf-linker-action.md`. Inputs: `version` (default empty → read env file), `toolchain` (default `nightly`). Steps: install rustup toolchain + rust-src, then `cargo +${toolchain} install bpf-linker --locked [--version <ver>]`. Special-case `version: latest` to omit the `--version` flag. Output `installed-version` from `bpf-linker --version` (consumed by T024's latest-mode assertion).
- [ ] T006 [P] ~~Self-verify T005 by adding a workflow_dispatch trigger to the composite action's own smoke test — a tiny `.github/workflows/action-smoke.yml`~~. **Implementation decision (2026-08-12)**: skipped in favor of implicit verification — the ci.yml `ebpf-tracing` lane's first post-merge run IS the composite-action smoke test. Adding a dedicated smoke workflow just to delete it later is churn. The pin-consistency guard (T011) catches the failure mode this would have caught.

**Checkpoint**: `.github/env/bpf-linker.env` + `.github/actions/install-bpf-linker/action.yml` both exist and the composite action installs bpf-linker successfully when called with defaults. US1 + US2 can now proceed.

---

## Phase 3: User Story 1 — Un-pin bpf-linker (Priority: P1) 🎯 MVP

**Goal**: Move waybill off the interim `bpf-linker@0.10.4` pin to a currently-supported version by (a) rewiring all four existing install sites to consume the composite action + env file, then (b) executing the upstream-first fix strategy per FR-011.

**Independent Test**: After all US1 tasks complete, the release lead can (i) verify locally with `scripts/verify-ebpf.sh --version <candidate>`, (ii) edit the version in `.github/env/bpf-linker.env` to the candidate, (iii) push a PR, and (iv) observe CI's `ebpf-tracing` lane pass. Success matches SC-001 (bump PR passes CI green on first push).

### Rewire existing install sites (US1a — MUST land before US1b)

- [X] T007 [US1] Rewire `.github/workflows/ci.yml` `Lint + test (linux-x86_64, --features ebpf-tracing)` job: replace the inline `cargo +nightly install bpf-linker --locked --version 0.10.4` step with `uses: ./.github/actions/install-bpf-linker`. Preserve the "Install bpf-linker" step name for log continuity.
- [X] T008 [US1] Rewire `.github/workflows/release.yml` `Build eBPF object` job: replace inline install with `uses: ./.github/actions/install-bpf-linker`. Preserve the `skip_ebpf` workflow_dispatch input's gating on this job (per FR-009).
- [X] T009 [US1] Verify `.github/workflows/nightly.yml` requires no rewire (`git grep -n 'bpf-linker' .github/workflows/nightly.yml` MUST return 0 matches). Nightly is a dispatcher to `release.yml`; the pin is inherited transitively via the release-run. If this grep ever returns a match in the future, an install step has been added and needs rewiring — reopen this task.
- [X] T010 [US1] Rewire `Dockerfile.ebpf-test` install: add `ARG BPF_LINKER_VERSION=0.10.4` near the top; change `cargo +nightly install bpf-linker --version 0.10.4` to `cargo +nightly install bpf-linker --locked --version ${BPF_LINKER_VERSION}`. Update the pin-comment at line 31-33 to reference `.github/env/bpf-linker.env` as the source of truth.
- [X] T011 [US1] Add a CI helper step (either in ci.yml or as a small `.github/workflows/verify-pin-consistency.yml`) that greps for stray `bpf-linker --version` occurrences outside `.github/env/bpf-linker.env` and fails if any are found. Prevents future accidental drift where someone re-hardcodes a version.
- [X] T012 [US1] Verify pin-consistency: run `git grep -n 'bpf-linker.*--version' -- ':!.github/env/bpf-linker.env' ':!specs/'` locally → MUST return zero matches after T007–T011 land.

### Land the rewire PR (US1a checkpoint)

- [ ] T013 [US1] Land the rewire changes (T007–T012 + US2 canary + US3 verify-script + docs) as a single PR titled `feat(m234): durable eBPF build resilience (rewire + canary + verify script)`. This PR MUST pass CI green (pin value unchanged: still `0.10.4`). [Bundled per user's Option-A scoping decision — US2 + US3 + docs land alongside US1a.]

**Checkpoint US1a**: All four install sites now consume `.github/env/bpf-linker.env` via the composite action. Bumping the version is now a one-line edit. US1b (the actual un-pin) is decoupled.

### Execute the un-pin (US1b — upstream-first per FR-011)

- [ ] T014 [US1] File upstream tracking issue at `aya-rs/bpf-linker` with the 2026-08-12 reproducer (log from release run 31616692311; symptom: `unable to find library -lLLVM` on ubuntu-24.04 with v0.11.0). Get explicit "yes, open the PR to X" confirmation from user before firing per feedback_upstream_prs_need_explicit_approval memory.
- [ ] T015 [US1] Populate `.github/env/bpf-linker.env` with `BPF_LINKER_UPSTREAM_ISSUE=<url>` and `BPF_LINKER_FALLBACK_DEADLINE=<today+30d ISO-8601>` per data-model.md `BpfLinkerPin` optional fields. Land as a separate small PR.
- [ ] T016 [US1] **Wait state** — no code changes while the fallback window runs. Track via calendar reminder + the tracking issue. **Exit condition A (→ T017)**: upstream `aya-rs/bpf-linker` publishes a fixed release with a `cargo install --locked` install path that succeeds against ubuntu-24.04. **Exit condition B (→ T018)**: the `BPF_LINKER_FALLBACK_DEADLINE` set in T015 elapses without a working upstream release; execute the downstream mitigation branch.
- [ ] T017 [US1] **US1b-happy-path (upstream fix)**: On upstream fix release, run `scripts/verify-ebpf.sh --version <new>` locally. If PASS, edit `.github/env/bpf-linker.env` to the new version + clear `BPF_LINKER_UPSTREAM_ISSUE` + `BPF_LINKER_FALLBACK_DEADLINE`. Open bump PR titled `chore(deps): bump bpf-linker to <ver>`.
- [ ] T018 [US1] **US1b-fallback-path (window expired)**: If T016's window expires without upstream fix, execute downstream mitigation. The mitigation shape depends on the exact upstream regression state — pick from: (a) explicit LLVM install + `LD_LIBRARY_PATH` export step added to `.github/actions/install-bpf-linker/action.yml`; (b) switch install to pre-built binary from bpf-linker GH releases (avoid `cargo install` entirely); (c) fork bpf-linker + patch. Decision made at fallback-execute time based on live upstream state; the T014 tracking issue's comment thread records the choice. **Constitution Principle I check**: option (a)'s `sudo apt-get install -y llvm-<ver>` is not a new C-toolchain per Principle I — it makes an existing transitive runtime dep of bpf-linker's Rust bindings explicit. Option (b) is byte-copy of an upstream Rust binary. Option (c) is pure Rust (fork of bpf-linker itself). All three preserve Principle I compliance; the executing PR MUST cite which option was taken and reaffirm the compliance clause.
- [ ] T019 [US1] Post-un-pin: verify next nightly release runs green (SC-002 baseline — start the 90-day clock).

**Checkpoint US1 complete**: `.github/env/bpf-linker.env` now pins a currently-supported bpf-linker version. Nightly + release lanes work without special handling.

---

## Phase 4: User Story 2 — Canary Regression Detection (Priority: P2)

**Goal**: Ship `.github/workflows/ebpf-canary.yml` per contracts/canary-workflow.md. Daily 06:00 UTC scheduled run against latest bpf-linker; on failure, auto-open a deduped GitHub issue.

**Independent Test**: Manual smoke test per quickstart.md Scenario E — dispatch canary with `version=0.11.0` (known-broken), verify issue is created with correct body; dispatch again with same version, verify a comment is added to the same issue (not a new one); dispatch with `version=0.10.4`, verify the issue is auto-closed. Success matches FR-003, FR-004, FR-005.

**Depends on**: Phase 2 (composite action must exist so the canary can call it).

- [X] T020 [P] [US2] Create `.github/workflows/ebpf-canary.yml` per contracts/canary-workflow.md. Triggers: `schedule: cron 0 6 * * *` + `workflow_dispatch` with `version` (default `latest`) and `dry_run` (default `false`) inputs. Job: `canary` on `ubuntu-latest`, timeout 15 min. Steps 1–5 per data-model.md `EbpfCanaryWorkflow`.
- [X] T021 [US2] Implement the `report-failure` step in `.github/workflows/ebpf-canary.yml` using `actions/github-script@v7` (SHA-pinned). Search for open issue with exact title `[canary] bpf-linker eBPF build regression`; if found, post comment; else, create new issue with contract-specified body. Labels: `canary`, `ebpf`, `regression`. Skip if `inputs.dry_run == true`.
- [X] T022 [US2] Implement the `report-success` step in `.github/workflows/ebpf-canary.yml`. If matching open issue exists, post closing comment + close issue with reason `completed`.
- [X] T023 [P] [US2] Create issue labels if they don't already exist: `canary`, `ebpf`, `regression`. One-time bootstrap via `gh label create` command documented in quickstart.md or via a small setup workflow.
- [ ] T024 [US2] Smoke-test the canary per quickstart.md Scenario E. Dispatch with `version=0.11.0` (known-broken) — verify issue creation + body content match contract. Dispatch again — verify comment append. Dispatch with `version=0.10.4` — verify issue auto-closes. **Additional latest-mode assertion**: dispatch with `version=latest` and verify the emitted `installed-version` output from the composite action (see contracts/install-bpf-linker-action.md §Outputs) is NOT equal to `BPF_LINKER_VERSION` from `.github/env/bpf-linker.env` — guards against a cargo-install cache hit silently reusing the pinned version and masking a real regression.
- [X] T025 [US2] Verify workflow-triggered issues respect `dry_run=true` — no side effects.

**Checkpoint US2 complete**: Canary runs daily; failure creates deduped issue; success auto-closes tracked regressions.

---

## Phase 5: User Story 3 — Local Un-Pin Readiness Command (Priority: P3)

**Goal**: Ship `scripts/verify-ebpf.sh` per contracts/verify-ebpf-script.md. Contributors can validate a candidate bpf-linker version end-to-end without a PR round-trip.

**Independent Test**: Contributor runs `scripts/verify-ebpf.sh --version 0.10.4` on Linux → PASS. Contributor runs `scripts/verify-ebpf.sh --version 0.11.0` → FAIL with output naming `bpf-linker` explicitly. Contributor on macOS runs the same command → routes through Docker; either succeeds or emits a clear "docker required" message. Success matches SC-003 (contributor verifies under 30 min Linux / 60 min container).

**Depends on**: Phase 2 (composite action + env file exist so the script can read the default version). Independent of Phase 3 and Phase 4.

- [X] T026 [P] [US3] Create `scripts/verify-ebpf.sh` implementing the contract at `specs/234-fix-ebpf-linker-regression/contracts/verify-ebpf-script.md`. Auto-detect Linux native vs container path. Read version from `.github/env/bpf-linker.env` by default; accept `--version` and `--container` overrides. Emit PASS/FAIL output per contract with tool name + version + failing command + log path.
- [X] T027 [US3] Make `scripts/verify-ebpf.sh` executable (`chmod +x`) and confirm the shebang line (`#!/usr/bin/env bash`) matches existing `scripts/*.sh` conventions.
- [X] T028 [US3] Self-test on Linux native — run against `0.10.4` (expect PASS) and `0.11.0` (expect FAIL naming bpf-linker). If not on Linux, self-test via `--container` path with Colima or Docker Desktop.
- [X] T029 [US3] Self-test on macOS — run without flags; verify it auto-routes to container path with a clear "using Docker" info line.

**Checkpoint US3 complete**: `scripts/verify-ebpf.sh` is available as the documented un-pin readiness command; both native and container paths work; failure output names bpf-linker.

---

## Phase 6: Polish & Documentation (Cross-Cutting)

**Purpose**: Documentation + spec close-out per FR-010. Depends on US1 landing (so docs can reference the actual un-pin path taken), but can start in draft state after US1a lands.

- [X] T030 [P] Create `docs/development/ebpf-toolchain.md` covering: (a) how the pin works (composite action + env file + Dockerfile ARG); (b) how to bump (edit `.github/env/bpf-linker.env`, verify via `scripts/verify-ebpf.sh`, open PR); (c) how to read canary failure issues; (d) what the `skip_ebpf` release.yml escape hatch is and when to use it; (e) what a canary auto-closed issue means; (f) fallback window process. Cross-link from CLAUDE.md if the section doesn't grow too large.
- [X] T031 [P] Update `CLAUDE.md` `## Feature flags` section: add a small "eBPF toolchain pin" subsection pointing at `docs/development/ebpf-toolchain.md` and citing `.github/env/bpf-linker.env` as the SoT.
- [ ] T032 [P] Add spec close-out note to `specs/234-fix-ebpf-linker-regression/spec.md` under a new `## Close-out (post-implementation)` section (per FR-010 + FR-011): (a) which un-pin path was taken (US1b-happy vs US1b-fallback); (b) upstream PR/issue link (or downstream mitigation description); (c) final version now pinned; (d) links to the bump PR + canary smoke-test run URLs.
- [X] T033 Verify all m234 acceptance criteria pass — walk the SC list from spec.md, mark each PASS/FAIL. Any FAIL → open a follow-up task or reopen the spec.
- [X] T034 Add a `memory/reference_bpf_linker_pin.md` auto-memory entry pointing future sessions at `.github/env/bpf-linker.env` as the version SoT + `scripts/verify-ebpf.sh` as the local verify command + the canary workflow.

---

## Dependencies

```
Phase 1 (Setup: T001–T003)
        │
        ▼
Phase 2 (Foundational: T004–T006)  ← BLOCKS all user stories
        │
        ├─── Phase 3 US1a (T007–T013) ───→ Phase 3 US1b (T014–T019)
        │                                          │
        │                                          ▼
        │                                    Phase 6 T032 (close-out — needs US1 done)
        │
        ├─── Phase 4 US2 (T020–T025)  ─── independent of US1b; can start in parallel with US1a
        │
        └─── Phase 5 US3 (T026–T029)  ─── independent of US1 + US2; can start in parallel

Phase 6 T030, T031, T034 can start after any US lands (draft state) and finalize at project end.
```

## Parallel execution examples

**Within Phase 2 (foundational)**: T005 + T006 both depend on T004 (env file) but are otherwise independent — after T004 lands, they can be worked concurrently.

**Post-Phase-2 story fanout**: T007 (ci.yml rewire), T008 (release.yml rewire), T009 (nightly.yml verify-only guard), T010 (Dockerfile.ebpf-test rewire) all touch different files with no cross-dependency — do them in parallel [P] if one contributor is comfortable batching, or split across contributors.

**Cross-story parallel**: US1a (T007–T013) + US2 (T020–T025) + US3 (T026–T029) are three independent branches after Phase 2 completes. If three contributors are available, all three ship in parallel. If one contributor: US1a first (MVP), then US2 and US3 in either order.

**Polish parallelism**: T030 + T031 + T034 all touch different files.

## Implementation strategy — MVP scope

**MVP = US1 only** (Phases 1 + 2 + 3). Ships:
- `.github/env/bpf-linker.env` — the single source of truth
- `.github/actions/install-bpf-linker/action.yml` — composite action
- Rewired ci.yml + release.yml + Dockerfile.ebpf-test (nightly.yml verified as a pass-through dispatcher; no rewire needed)
- `bpf-linker` un-pinned to a working version (either upstream-fixed OR downstream-mitigated)

MVP unblocks the "next release" problem. US2 (canary) and US3 (local harness) prevent recurrence but are not blockers for the next release — they can ship any time after MVP.

**Incremental delivery**:

1. **Cut 1** — Phases 1 + 2 + US1a. Every install site now reads the env file. No behavioral change (pin is still 0.10.4). Merge-safe.
2. **Cut 2** — US1b path (upstream-first). Waits for upstream response OR the 30-day fallback deadline. Merges the version bump when ready.
3. **Cut 3** — US2 canary. Prevents recurrence.
4. **Cut 4** — US3 verify script. Reduces friction for future un-pins.
5. **Cut 5** — Docs + spec close-out (Phase 6).

## Task summary

- **Total tasks**: 34
- **Per phase**: Setup 3, Foundational 3, US1 13 (12 active + 1 verification-only), US2 6, US3 4, Polish 5
- **Per user story**: US1 = 13 tasks (largest — MVP + upstream-first branching + a verify-only nightly.yml no-op guard), US2 = 6 tasks, US3 = 4 tasks
- **Parallel-safe tasks marked [P]**: 8 across all phases
- **Wait-state tasks**: 1 (T016 — 30-day fallback window with two named exit conditions)
- **Verification-only tasks**: 1 (T009 — nightly.yml no-op guard, per analysis-phase N1 correction)

## Format validation

All 34 tasks follow the required checklist format:
- Every task starts with `- [ ]`
- Every task has a sequential ID (T001–T034)
- Every task in Phases 3–5 carries a story label ([US1], [US2], [US3])
- Every task references a concrete file path OR a concrete action against a named artifact
- Tasks marked [P] confirm they touch different files with no ordering dependency
