# Implementation Plan: SLSA build provenance for waybill release artifacts

**Branch**: `668-slsa-provenance` | **Date**: 2026-08-28 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/668-slsa-provenance/spec.md`

## Summary

Add SLSA build provenance attestations to every waybill release artifact — the 4 platform-specific binary tarballs, the multi-arch OCI image, and the source SBOM sidecar — using the current GitHub `actions/attest-build-provenance` action. The release workflow self-verifies each emitted attestation via `gh attestation verify` before publication (FR-015 hard gate). Purely GitHub Actions YAML + Markdown docs; zero Rust code changes, zero Cargo dependency changes.

Technical approach:
1. Add `permissions: { id-token: write, attestations: write, contents: read }` to the 4 build jobs + `publish-container-image` job + the `release` job (source SBOM).
2. Emit provenance via one `actions/attest-build-provenance` step per build job, immediately after the artifact is written.
3. Emit provenance for the OCI image via `subject-digest` + `push-to-registry: true` on the image-publish job.
4. Emit provenance for the source SBOM in the final `release` job.
5. Self-verify each emission in the same job via `gh attestation verify` (FR-015).
6. Add `docs/verifying-releases.md` with copy-paste `gh attestation verify` recipes for each artifact type.

## Technical Context

**Language/Version**: GitHub Actions YAML + Markdown (workflow-only feature). Rust workspace toolchain inherited but untouched.
**Primary Dependencies**: `actions/attest-build-provenance@v3` (SHA-pinned per FR-013), `gh` CLI (preinstalled on GitHub-hosted runners; used for FR-015 self-verify). No new Cargo deps at any layer (FR-012 + SC-007). No new external Actions marketplace additions besides `attest-build-provenance` itself.
**Storage**: N/A — attestations land in GitHub's attestation store (Sigstore Rekor + GitHub-native indexing). Free/unlimited for public repos. No waybill-side persistence.
**Testing**: Post-merge acceptance = run one release cycle + one nightly cycle, verify all 6 subjects via `gh attestation verify` against untampered artifacts (SC-002) and against one byte-flipped tarball (SC-003). Pre-merge acceptance = workflow-YAML syntax validation via GitHub's own workflow parser + the SHA-pin audit script (existing).
**Target Platform**: GitHub Actions runners (Ubuntu, macOS, Windows) executing waybill's release + nightly workflows. Downstream verifiers are any platform running the `gh` CLI ≥ 2.49 or `cosign verify-attestation`.
**Project Type**: Release-pipeline infrastructure change. Not a CLI feature; not a library feature.
**Performance Goals**: SC-005 — release workflow wall-clock increase ≤ 90s vs pre-feature baseline (~30s emission + ~30s self-verify + ~30s slack, per parallelized matrix analysis in Phase 0 research).
**Constraints**: FR-011 (no CLI changes), FR-012 (no Cargo changes), FR-013 (SHA-pinned actions only), FR-014 (no formal SLSA level claim in public messaging), FR-015 (self-verify hard gate).
**Scale/Scope**: 6 SLSA subjects per release (4 tarballs + 1 image + 1 source SBOM). One release cycle per stable + one per night = ~30 attestations per month at current cadence. GitHub attestation store scales orders of magnitude beyond this.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Applicable principles for a release-pipeline-only feature**:

| Principle | Applies? | Status |
|---|---|---|
| I. Pure Rust, Zero C | No | Feature touches YAML + Markdown only; no Rust or C code. Trivially compatible. |
| II. eBPF-Only Observation | No | Not an observation feature. |
| III. Fail Closed | **Yes** | ✅ FR-008 (partial-release failure fails the whole release) + FR-015 (self-verify failure fails the release) codify fail-closed semantics for the emission and verification paths. |
| IV. Type-Driven Correctness | No | No Rust types involved. |
| V. Specification Compliance | **Yes** (SLSA-side) | ✅ FR-004 pins the current SLSA Provenance schema URI at emission time. FR-014 forbids over-claiming levels. Spec compliance for SLSA is the WHOLE feature. |
| VI. Three-Crate Architecture | No | No crate changes; FR-012 explicitly forbids. |
| VII. Test Isolation | No | No new Rust tests. Existing `cargo test --workspace` output must remain byte-identical (SC-006). |
| VIII. Completeness | **Yes** | ✅ FR-001+FR-002+FR-003 cover EVERY release artifact type. Nothing shipped without provenance (FR-008). |
| IX. Accuracy | **Yes** | ✅ Provenance predicate values (subject digest, source commit, workflow-run URL) come from GitHub's own OIDC-bound identity — not inferrable data. FR-015 self-verify catches inaccuracies before publication. |
| X. Transparency | **Yes** | ✅ FR-009 + FR-010 (docs) explain WHAT the attestation contains and HOW consumers verify it. Historical-releases edge case explicitly documented in edge cases. |
| XI. Enrichment | No | Not an SBOM-emission feature. |
| XII. External Data Source Enrichment | No | No external data source. |

**Verdict**: PASS. Every applicable principle is honored. No exceptions or waivers required.

## Project Structure

### Documentation (this feature)

```text
specs/668-slsa-provenance/
├── plan.md              # This file
├── research.md          # Phase 0: action-version choice, verify-vs-cosign coexistence, matrix cost model
├── data-model.md        # Phase 1: SLSA Provenance predicate + subject shape (descriptive, not new-code)
├── quickstart.md        # Phase 1: 5-step operator verification recipe
├── contracts/
│   ├── workflow-step.md         # Contract: what the attest-build-provenance step MUST look like per job
│   └── verification-recipe.md   # Contract: what the docs recipe MUST contain for each artifact type
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root — CHANGES ONLY)

```text
.github/workflows/
├── release.yml       # ADD: attest-build-provenance step in each of 6 emission points + FR-015 self-verify
├── nightly.yml       # ADD: same emission + self-verify pattern for nightly cadence
└── (ci.yml, ebpf-canary.yml, etc.) # UNCHANGED — non-release CI lanes emit no provenance per edge case

docs/
└── verifying-releases.md   # NEW: FR-009 verification recipes for tarball, OCI image, source SBOM sidecar

CLAUDE.md                    # auto-updated by spec-kit agent-context hook (no material content change)
```

**Explicitly unchanged**:
- `waybill-cli/` — all Rust crates (FR-011 + SC-006)
- `waybill-common/`, `waybill-ebpf/`, `xtask/` — all crates untouched
- `Cargo.toml`, `Cargo.lock`, per-crate `Cargo.toml` files (FR-012 + SC-007)
- `scripts/pre-pr.sh` — unchanged; existing gate still passes byte-identically pre/post feature
- All other workflows (`ci.yml`, `ebpf-canary.yml`, `public-corpus.yml`, `test-signing.yml`) — no provenance emission for non-release lanes

## Post-Design Constitution Re-check

*GATE: after Phase 1 artifacts, re-check that the design doesn't drift.*

Re-checked after writing research.md, data-model.md, contracts/, quickstart.md:

- Fail Closed (Principle III): ✅ Contract `workflow-step.md` C-3 requires `continue-on-error: false` on every emission step; C-6 requires the same on every verify step. Fail-closed by construction.
- Specification Compliance (Principle V): ✅ Research §R1 pins the exact upstream action version + resolved SHA. Contract `workflow-step.md` C-1 requires the SHA-pin at every use site. Version drift is a review-gate concern, not a runtime concern.
- Completeness (Principle VIII): ✅ Contract `workflow-step.md` C-2 enumerates ALL 6 subjects; quickstart.md walks the verifier through all 6. No subject can slip through without a matching emission step.
- Accuracy (Principle IX): ✅ Contract `workflow-step.md` C-4 requires self-verify (FR-015) as an immediate post-emission step. Any subject-path mismatch or misdigest surfaces inside the same job.
- Transparency (Principle X): ✅ Quickstart.md carries the operator-facing narrative; docs/verifying-releases.md is the durable version. Both name the limitations (historical releases, mirrored artifacts).

**Verdict**: PASS post-design. No drift.

## Phase Outputs Index

- **Phase 0** research → [research.md](./research.md) — action version, verify tooling, cosign coexistence, matrix cost model, 5 total decisions.
- **Phase 1** data model → [data-model.md](./data-model.md) — SLSA Provenance predicate shape (descriptive, since the CLI never touches this data), subject shape, Sigstore bundle envelope.
- **Phase 1** contracts:
  - [contracts/workflow-step.md](./contracts/workflow-step.md) — 8 behavioral contracts C1-C8 for every attest+verify step pair.
  - [contracts/verification-recipe.md](./contracts/verification-recipe.md) — 4 behavioral contracts C1-C4 for the docs verification recipes.
- **Phase 1** quickstart → [quickstart.md](./quickstart.md) — 5-step operator recipe covering tarball + OCI image + source SBOM verification.
- **Phase 1** agent context update → CLAUDE.md's `## Active Technologies` section gets a m668 entry (auto-appended by `.specify/scripts/bash/update-agent-context.sh claude`).

## Progress Tracking

- [X] Phase 0 research complete
- [X] Phase 1 data model complete
- [X] Phase 1 contracts complete
- [X] Phase 1 quickstart complete
- [X] Phase 1 agent context updated
- [X] Post-design Constitution re-check
- [ ] Phase 2 tasks (via `/speckit.tasks`)
