# Implementation Plan: Release-flow implementation — realize the 228 two-channel recommendation

**Branch**: `229-release-flow-impl` | **Date**: 2026-08-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/229-release-flow-impl/spec.md`

## Summary

Real implementation milestone (unlike 227/228 which were docs). Ships concrete workflow YAML + `build.rs` + `Cargo.toml` + `RELEASING.md` changes that realize the [228 survey](../228-release-flow-exploration/spec.md)'s two-channel recommendation, PLUS the three Q1/Q2/Q3 clarifications from 229's own clarify session (30-day nightly retention, sign-all-channels, no-gate bridge policy).

**Delivery split into two sequential PRs** per FR-010:

1. **Infrastructure PR** (this feature's primary work): add `.github/workflows/nightly.yml` (cron + skip-if-unchanged + 30-day retention cleanup + delegate to release.yml via workflow_dispatch), add `WAYBILL_VERSION` env-override in `waybill-cli/build.rs`, modify `.github/workflows/release.yml` (broaden tag-trigger regex, integrate unconditional `--sign` per Q2), delete `.github/workflows/auto-tag-release.yml`, add `RELEASING.md`, add README-level channel-picker callout.
2. **Release-bump PR** (follows infrastructure PR): bump `[workspace.package].version` from `0.1.0-alpha.70` → `0.2.0`, regenerate 6 golden test files, cut the first-stable `v0.2.0` tag manually (per memory `reference_release_process`).

Total surface: **1 new workflow YAML** (nightly.yml), **1 modified workflow YAML** (release.yml — expand tag regex + unconditional `--sign`), **1 deleted workflow YAML** (auto-tag-release.yml), **1 modified `build.rs`** (~30 LOC for env-override + unit-test entry point), **1 modified `Cargo.toml`** (version bump on the second PR only), **6 regenerated golden files** (on the second PR only), **1 new `RELEASING.md`** (~150 lines), **1 modified `README.md`** (~5-line addition), **1–2 new unit tests** for the env-override behavior. Zero new Cargo crates.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–228; no nightly required for this user-space-only work). GitHub Actions YAML for workflows. Bash for cleanup logic (retention step).
**Primary Dependencies**: Existing only — `std::env` (for the `build.rs` env-var read), `std::process::Command` (already used by nightly cleanup for `gh` CLI shell-out if needed; alternatively pure API via `actions/github-script` in-workflow). GitHub Actions workflow uses existing `actions/checkout`, `sigstore/cosign-installer` (already present per m222), plus a lightweight retention-cleanup step invoking `gh release list` + `gh release delete-with-tag`. **No new Cargo dependencies. No new GitHub Actions the release.yml doesn't already invoke.**
**Storage**: N/A for runtime; nightlies stored as GitHub release-artifact archives (existing storage surface — 4 platform tarballs + multi-arch OCI image per release).
**Testing**:
- Unit test coverage for `WAYBILL_VERSION` env-override (FR-012): assert override applies when set; fallback to `env!("CARGO_PKG_VERSION")` when unset; invalid SemVer strings produce build-time compile error.
- Workflow YAML validation via `gh workflow view` OR `actionlint` (FR-011) — pre-PR gate should catch YAML errors, but explicit check documented in tasks.md.
- End-to-end dry-run of nightly.yml on a test branch before merging the infrastructure PR (validate cron trigger fires + skip-if-unchanged works + retention cleanup works on a seeded fixture). Practical constraint: workflow_dispatch invocation on a feature branch tests only the workflow logic, not the cron path — a scheduled cron on a feature branch requires additional setup. Acceptable: rely on workflow_dispatch validation + first-cron on `main` after merge to catch the schedule-specific path.
- Pre-PR gate `./scripts/pre-pr.sh` MUST exit 0 on both PRs.
**Target Platform**: GitHub Actions runners (Ubuntu latest for the cron + delegate steps; the platform-specific builds happen inside release.yml on macOS/Windows/Linux runners as today). Repository: `kusari-oss/waybill`.
**Project Type**: real-code feature — CLI + workflow YAML + docs. First shipping code change since m222 keyless signing.
**Performance Goals**:
- Nightly.yml total wallclock (cron trigger → tag pushed → release.yml dispatch complete): target < 15 min per SC-003 (release.yml itself is ~10 min for the multi-arch build; nightly.yml's cron + skip-check + dispatch + cleanup adds < 5 min).
- WAYBILL_VERSION override MUST NOT trigger full cache invalidation (SC-007) — target < 8 min for the second `cargo build --release` after an override change on the same commit.
**Constraints**:
- **228 survey §4 is the authoritative source** (spec Assumptions §8): the 5 recommendation fields (channel manifest, cadence, tag convention, signing decision, migration path) are the design contract. This feature implements them literally; drift from those 5 fields requires updating 228 first.
- **Q1 clarification (30-day retention)**: FR-011a mandates nightly-tag auto-deletion after 30 days. Stables + bridge pre-releases preserved forever (FR-011a's inclusion regex is anchored to the nightly regex only).
- **Q2 clarification (sign all)**: overrides 228 §4.4's per-channel decision. `release.yml` invokes `--sign` unconditionally on all release-artifact SBOMs regardless of tag format. Fail-closed (constitution Principle III + FR-004) on signing failure.
- **Q3 clarification (bridge governance)**: bridge pre-releases are always acceptable, no policy gate, any SemVer-valid pre-release suffix works. Cleanup step MUST protect all non-nightly pre-releases via anchored regex.
- **PR-sequencing invariant** (FR-010): infrastructure PR before release-bump PR. Rationale: cleanly separate "does the new machinery work?" from "does the version bump land correctly?". This also isolates the release-bump PR's expected 30+ min cache-invalidation time (memory `feedback_release_bump_prepr_slow`) from the infra PR's normal-cadence CI.
- **Constitution Principle V** (CISA 2026): FR-004's unconditional `--sign` is the where compliance gets wired in. Not optional; not per-channel.
- **Constitution Principle III** (Fail-closed): FR-004 makes signing failure a release-workflow failure — no unsigned fallback.
- **Memory `feedback_release_bump_regen_all_golden_tests`**: release-bump PR regenerates 6 golden files (cdx_regression, spdx_regression, spdx3_regression, oci_pull_backward_compat, optional_dep_classification, pkg_alias_binding_us1). Verified via normalized diff per memory `feedback_verify_golden_churn_normalized`.
- **Existing `release.yml` broader tag-trigger regex needed** — today it only triggers on `v*-alpha.*`, `v*-beta.*`, `v*-rc.*` (verified via `head .github/workflows/release.yml`). Post-229 the trigger must catch plain SemVer stables (`v0.2.0`), nightlies (`v0.2.0-nightly.YYYYMMDD`), AND any other SemVer pre-release suffix the maintainer picks (per Q3). Broadening to `v*` is the simplest fix.
- **GitHub Actions anti-loop policy**: a tag push made via `GITHUB_TOKEN` does NOT trigger downstream workflows. `auto-tag-release.yml`'s current handling uses `gh workflow run` to explicitly dispatch `release.yml` — nightly.yml must use the same pattern. This is documented in the current `auto-tag-release.yml` comment header.
**Scale/Scope**: reaches every waybill user + every downstream CI pipeline pinning to waybill. Effort estimate: nightly.yml ~80 lines YAML + WAYBILL_VERSION build.rs modification ~30 LOC + release.yml modifications ~20 LOC + RELEASING.md ~150 lines + README callout ~5 lines. Total: **~285 lines net addition** across the infrastructure PR. Release-bump PR adds ~5 lines (version bump) + ~2647 lines of golden regeneration (net-zero semantically per memory `feedback_verify_golden_churn_normalized`).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ N/A | Rust-only + YAML + Markdown. No C. |
| II | eBPF-Only Observation | ✅ N/A | No dependency-discovery code touched. |
| III | Fail Closed | ✅ **DIRECTLY ADVANCES** | FR-004 hard-requires signing; signing failure fails the release. No unsigned fallback per Q2. |
| IV | Type-Driven Correctness | ✅ Preserved | Existing `.unwrap()` bans + type-driven surface unchanged. FR-012's env-var handling in `build.rs` must use `.ok_or_else()` / `?` propagation, not `.unwrap()` — noted in contract. |
| V | Specification Compliance | ✅ **DIRECTLY ADVANCES** | FR-003 + FR-004 wire CISA 2026 SBOM Author Signature into every release. Q2 clarification lifts signing from per-channel-optional to universal. This is where Principle V goes from "capability exists" (m222) to "capability applied on every release" (229). |
| VI | Three-Crate Architecture | ✅ N/A | Workspace shape unchanged. |
| VII | Test Isolation | ✅ Preserved | FR-012 unit tests for `WAYBILL_VERSION` behavior — pure logic tests, no eBPF gating. |
| VIII | Completeness | ✅ N/A | Not a scan-emission signal. |
| IX | Accuracy | ✅ N/A | |
| X | Transparency | ✅ **REINFORCES** | RELEASING.md documents the two-channel model + bridge governance + signing invariants explicitly. Downstream consumers can inspect a `.sig` next to every release-artifact SBOM. |
| XI | Enrichment | ✅ N/A | |
| XII | External Data Source Enrichment | ✅ N/A | |
| SB-1 | No lockfile-based discovery | ✅ N/A | |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ **HONORED** | FR-012's `build.rs` code path must not `.unwrap()` on env-var reads; the contract in `contracts/build-rs-version-override.md` fixes the return-type. |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | |

**All gates pass.** Principles III, V, X directly advanced. This is a compliance-hardening milestone (Principle V) with a fail-closed contract (Principle III) and a transparency deliverable (Principle X — RELEASING.md).

## Project Structure

### Documentation (this feature)

```text
specs/229-release-flow-impl/
├── plan.md              # This file
├── research.md          # Phase 0 — nightly.yml trigger technique (workflow_dispatch vs workflow_run vs GITHUB_APP), retention cleanup mechanism, build.rs env-var pattern, release.yml regex broadening decision
├── data-model.md        # Phase 1 — entities: nightly-workflow config, release-tag regex, WAYBILL_VERSION env-var lifecycle, retention cleanup criteria
├── quickstart.md        # Phase 1 — walkthroughs for first-cron-fire, workflow_dispatch dry-run, release-bump PR sequence
├── contracts/
│   ├── nightly-workflow.md            # nightly.yml behavior contract
│   ├── release-workflow-modifications.md   # release.yml diff-contract (tag-trigger regex + --sign integration)
│   ├── build-rs-version-override.md   # WAYBILL_VERSION env-var contract
│   └── releasing-md-structure.md      # RELEASING.md required-sections contract
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify (all items ✅)
```

### Source Files (repository root)

Touched files (implementation scope — real code + workflow YAML + docs):

```text
.github/workflows/
├── nightly.yml                                     # NEW — cron + skip-if-unchanged + retention cleanup + dispatch to release.yml
├── release.yml                                     # MODIFY — broaden tag trigger; unconditional --sign; drop unsigned-nightly branching
└── auto-tag-release.yml                            # DELETE — broken workflow retired

waybill-cli/
├── build.rs                                        # MODIFY — add WAYBILL_VERSION env-var read + rerun-if-env-changed hook
├── Cargo.toml                                      # UNCHANGED on infra PR; version-bump on release-bump PR only
└── tests/
    └── waybill_version_override.rs                 # NEW — FR-012 unit test file for the env-override behavior

Cargo.toml                                          # UNCHANGED on infra PR; workspace.package.version bump on release-bump PR only

RELEASING.md                                        # NEW — release-cutting guide (stable, nightly, bridge pre-release, disable-nightly)

README.md                                          # MODIFY — small "Which release channel should I use?" callout with cross-link to docs/design/2026-08-05-release-flow-survey.md

waybill-cli/tests/fixtures/golden/                  # UNCHANGED on infra PR; 6 files regenerated on release-bump PR (per memory `feedback_release_bump_regen_all_golden_tests`)
```

No test files DELETED. No parity-catalog rows. No CLI flag changes on the shipping binary (WAYBILL_VERSION is build-time only, not a runtime flag).

**Structure Decision**: two-PR delivery per FR-010. Infrastructure PR (this branch, `229-release-flow-impl`) touches everything EXCEPT the version bump + goldens. Release-bump PR (future branch `release/v0.2.0`, cut after infra PR merges) handles version bump + golden regen + first-stable tag push. This mirrors the successful pattern used for `v0.1.0-alpha.70`'s release-bump work (see PR #664 for reference on golden-regen ceremony).

## Complexity Tracking

*Not applicable* for the primary architectural gates — Constitution passes cleanly. Two implementation-detail tradeoffs surface in Phase 0 research:

1. **Nightly.yml → release.yml handoff mechanism**. Three options: (a) `workflow_dispatch` API via `gh workflow run` (matches current `auto-tag-release.yml` pattern; proven to fail-open when the dispatched workflow's `workflow_dispatch` entry-point accepts the `tag` input; explicit + traceable in workflow-run history); (b) `workflow_call` (release.yml made a reusable workflow — bigger refactor); (c) `push` trigger on the tag (blocked by GitHub anti-loop policy — a `GITHUB_TOKEN`-pushed tag doesn't trigger downstream workflows). Resolved in Phase 0 §A: **Option (a)**, matching the existing pattern.
2. **Retention cleanup implementation**. Two options: (a) inline shell step using `gh release list --json ... | jq | xargs gh release delete-with-tag`; (b) dedicated GitHub-scripted step via `actions/github-script` with typed release-API access. Resolved in Phase 0 §B: **Option (a)**, simpler + no new action-marketplace dependency.
3. **`release.yml` tag-trigger regex**. Two options: (a) enumerate: `v*-alpha.*` + `v*-beta.*` + `v*-rc.*` + `v*-nightly.*` + `v*.*.*` (bare stable); (b) just `v*` catch-all. Resolved in Phase 0 §C: **Option (a)** for defensive anchoring — the catch-all would fire on non-release tags a maintainer might use for other purposes.
