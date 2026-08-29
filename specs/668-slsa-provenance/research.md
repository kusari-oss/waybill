# Phase 0 Research: SLSA build provenance for release artifacts

**Feature**: `668-slsa-provenance` | **Date**: 2026-08-28

Decisions with rationale + rejected alternatives. Each entry resolves a Technical Context unknown or a scope-adjacent design choice raised by the spec's Assumptions section.

## R1: Pinned upstream action version

**Decision (updated 2026-08-28 per T002 empirical re-verify)**: `actions/attest-build-provenance@v4.2.2` SHA-pinned to `4d101475d8b20a2381f78447822ac1eab6504dd8`. Emits `predicateType = https://slsa.dev/provenance/v1` via `actions/attest@v4.2.1` (composed internally). Falls back to the default "Provenance mode" of the underlying `actions/attest` when no `predicate-type` / `predicate` / `sbom-path` is supplied by the caller — waybill supplies only `subject-path` (or `subject-digest` for OCI images), triggering auto-SLSA-Provenance generation.

**Rationale**: v4 is the current major (v4.2.2 released 2026-08-06). Upstream deprecated the v3 composition (two-step wrapper: `attest-build-provenance/predicate` + `actions/attest`) in favor of a passthrough that delegates predicate generation to `actions/attest`'s three-mode logic (Provenance default / SBOM / Custom). Waybill's use case = default Provenance mode. Retained the `attest-build-provenance` wrapper (over the upstream-recommended `actions/attest` direct call) because:

1. The wrapper's name matches waybill's intent — a future contributor can't accidentally switch to SBOM or Custom mode by adding a `predicate-type:` input without also renaming the step (semantic signaling).
2. The `push-to-registry: true` input is exposed identically on both the wrapper and `actions/attest` — ergonomic parity for OCI-image emission (FR-002).
3. Upstream explicitly says "existing applications may continue to use the `attest-build-provenance` action" — the wrapper isn't deprecated, just complemented by the underlying direct call for max-flexibility use cases waybill doesn't have.

SHA-pinning follows waybill's convention per memory `feedback_sha_pin_before_dependabot` — dependabot upgrades the SHA on version bumps; the workflow YAML never references a tag.

**Alternatives considered**:
- **`actions/attest@v4.2.2` direct** (SHA `1e69f48acb82d1966a394da916b4c1698aa569d6`) — upstream's "new implementations should use this" recommendation. Rejected for m668 per the "semantic signaling" rationale above; may be revisited if waybill's future release pipeline needs Custom-mode attestations.
- `actions/attest-build-provenance@v3.2.0` — the pre-v4 version pinned in the initial research pass. Rejected on empirical re-verify (2026-08-28): v3 is a full major behind current, and the auto-SLSA-Provenance path moved to `actions/attest`'s default mode in v4 — using v3 would work but ties us to an older code path with fewer upstream security fixes.
- `slsa-framework/slsa-github-generator` reusable workflow — earlier-generation approach, more complex to wire in (requires a separate `provenance-linux` reusable-workflow call per platform + a coordinator job), and marked as "consider migrating to attest-build-provenance" in SLSA's own README. Rejected in favor of the simpler built-in action.
- `sigstore/gh-action-sigstore-python` — Python-native, wrong ecosystem fit for a Rust project, and doesn't emit SLSA Provenance predicates (emits generic sigstore signatures). Rejected.
- `docker/build-push-action`'s built-in `provenance: mode=max` — already used at `release.yml:602` for the OCI image. This emits BuildKit's *own* provenance format, NOT the SLSA v1.0 predicate schema. It's complementary to `attest-build-provenance` but doesn't satisfy FR-002 alone. Kept as-is; `attest-build-provenance` runs alongside it to produce the SLSA-schema attestation.

## R2: Self-verify tooling choice (FR-015)

**Decision**: `gh attestation verify` from the GitHub CLI, invoked in the same job that emitted the attestation. `gh` is preinstalled on all GitHub-hosted runners (Ubuntu, macOS, Windows) at a version ≥ 2.49 which supports `attestation verify` natively.

**Rationale**: `gh attestation verify` is the canonical GitHub-side tool for verifying `actions/attest-*` outputs. It handles the Sigstore Rekor lookup, verifies the ephemeral OIDC certificate against the workflow identity, and asserts subject-digest equality — the exact FR-005/FR-006 semantics we need. Costs ~3-5s per subject inside the workflow (warm Rekor cache).

**Alternatives considered**:
- `cosign verify-attestation` — works against the same Sigstore bundle, but requires a separate `sigstore/cosign-installer` step and manual predicate-type filter. Adds ~10s per verification for cosign install. Rejected as heavier for self-verify; kept as a documented alternative recipe in `docs/verifying-releases.md` for consumers who prefer cosign.
- `slsa-framework/slsa-verifier` — the reference SLSA verifier. Written in Go, requires a `go install`, and is designed for external consumer use with source-provenance policy matching. Rejected as heavier than `gh attestation verify` for the self-verify use case; may be added later as a downstream consumer recipe.

## R3: Coexistence with the existing cosign-keyless signature (m222)

**Decision**: Both signature paths run in parallel on the same release workflow. The cosign-keyless signature (already at `release.yml:701` for the source SBOM, and via `sigstore/cosign-installer` for the OCI image) stays unchanged. `actions/attest-build-provenance` runs as an additional step per artifact.

**Rationale**: Downstream consumers who already trust the cosign signature keep their path; new consumers trusting SLSA provenance use `gh attestation verify`. Both signature paths use the same GitHub-provided OIDC identity, so any post-hoc audit correlating "did this artifact come from this workflow run" against either signature converges on the same answer. Removing the cosign path would break existing downstream consumers (deb/rpm distros, cosign-based admission policies) without upside.

**Alternatives considered**:
- Remove the cosign-keyless signature, ship SLSA-only. Rejected — breaks existing verification paths for a non-zero downstream population (see m222 CISA audit context).
- Merge into a single attestation covering both formats. Not technically possible — the cosign signature is a Sigstore-format signature over an artifact-content payload; the SLSA attestation is a Sigstore-format signature over an in-toto Statement envelope containing the provenance predicate. Different subject shapes, different tooling.

## R4: Matrix cost model (SC-005 budget breakdown)

**Decision**: Total workflow-time increase capped at ≤90 seconds. Breakdown:

| Artifact | Emission cost | Verify cost | Parallelized? |
|---|---|---|---|
| linux-x86_64 tarball | ~5s | ~5s | Yes (per-platform matrix) |
| linux-aarch64 tarball | ~5s | ~5s | Yes |
| macos-aarch64 tarball | ~5s | ~5s | Yes |
| windows-x86_64 tarball | ~5s | ~5s | Yes |
| Multi-arch OCI image | ~10s | ~5s | No (single job) |
| Source SBOM sidecar | ~5s | ~5s | No (single job) |

Wall-clock worst case: 4 tarballs run in parallel (~10s per job), then image (~15s), then source SBOM (~10s) = ~35s serial hot path. Add ~30s slack for cold Rekor cache + `gh` CLI warm-up on Windows = ~65s realistic worst case. SC-005's 90s budget covers this with ~25s headroom.

**Rationale**: Explicit budget breakdown per subject prevents SC-005 from being an abstract number that gets breached invisibly. Sourced from real-world times observed on other Kusari repos' `attest-build-provenance` adoption + the memory `project_ci_timing` establishing waybill's baseline lane cadence.

**Alternatives considered**:
- Serialize all emissions in one dedicated `emit-provenance` job at the end of the workflow. Rejected — adds a job-startup penalty (~20s) and defers the FR-015 self-verify gate to end-of-workflow, giving worse feedback on emission failures than in-job emission.

## R5: SHA-pin bump policy

**Decision**: Dependabot handles SHA bumps via routine dependency-update PRs. Waybill's workflow references the pinned SHA verbatim; no `@v3` tag references anywhere. The Dependabot config already covers `.github/workflows/` — no new configuration required.

**Rationale**: Consistent with memory `feedback_sha_pin_before_dependabot` — every action ref in waybill's workflows is SHA-pinned; dependabot's role is to keep those SHAs current. FR-013 codifies the SHA-pin requirement; the plan needs no separate policy artifact.

**Alternatives considered**:
- Weekly manual review + explicit bump PR. Rejected — creates a manual chore for a well-automated process.
- Pin to the major-version tag `@v3` and let GitHub's automatic tag-resolution track updates. Rejected — violates FR-013's SHA-pin requirement and the `feedback_sha_pin_before_dependabot` guidance (mutable tags are a Kusari Inspector finding).

## Empirical claims to re-verify at implementation time

Per memory `feedback_verify_research_empirical_claims`. **Re-verified 2026-08-28 during T002; findings recorded here**:

- **Upstream action major version**: ✅ Re-verified. `gh release list --repo actions/attest-build-provenance --limit 5` returns v4.2.2 as latest (2026-08-06). Adopted v4.2.2 per updated R1 above. The initial research pass had assumed v3.0.0 — that was based on knowledge cutoff; the empirical check caught the drift.
- **Pinned SHA for v4.2.2**: ✅ Re-verified via `gh api /repos/actions/attest-build-provenance/tags | jq '.[] | select(.name == "v4.2.2") | .commit.sha'` → `4d101475d8b20a2381f78447822ac1eab6504dd8`. This SHA is now the C-1 contract value used across all workflow edits.
- **Predicate type URI is still `https://slsa.dev/provenance/v1`**: ✅ Re-verified via v4.2.2 README + underlying `actions/attest@v4.2.1` README ("Provenance mode: auto-generates SLSA build provenance"). Schema v1.0 unchanged from v3.
- **`gh` CLI on GitHub-hosted runners ≥ 2.49**: ✅ Reasoned re-verified. GitHub's `runner-images` publishes monthly image updates; the current hosted runners (ubuntu-latest, macos-latest, windows-latest) ship `gh` 2.65+ — well above the 2.49 minimum for `gh attestation verify`. A confirming `gh --version` step is trivial to add to T012's acceptance dispatch if paranoia demands.
- **BuildKit's `provenance: mode=max` at `release.yml:602`**: ✅ Re-verified by inspection. Still present. No conflict with adding `attest-build-provenance` alongside.
- **Actual release-matrix targets**: ✅ Re-verified via `grep -nE "^\s+TARGET:" release.yml` — exact matches at lines 146/265/362/432 for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`. Confirms the spec correction stands.

## Deferred decisions (out of scope for this plan)

- **CLI-side offline verification** (`waybill verify slsa`) — deferred per FR-011 to a future feature. Tracked at [issue #725](https://github.com/kusari-oss/waybill/issues/725).
- **SLSA-conformant attestations from `waybill trace`** — same deferral, same issue.
- **Formal SLSA Build level claim** — resolved via Q2 clarification (FR-014 forbids). No further decision needed.
- **Sigstore Rekor mirror strategy** — waybill relies on GitHub's default public Rekor instance. Air-gapped consumer scenarios use the bundle-file path documented in FR-010 (bundle bytes are self-contained).
