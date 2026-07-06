# Research: milestone 165 — Kubernetes + ArgoCD audit

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Phase 0 research. Since this is an audit milestone (not a code milestone), research documents the methodology + tooling decisions rather than solving code unknowns.

## R1 — Trivy version pin + installation

**Decision**: Pin Trivy at **0.71.1** (matches milestone-083's `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1` pin). Install via `brew install aquasecurity/trivy/trivy` on macOS OR `go install github.com/aquasecurity/trivy/cmd/trivy@v0.71.1` cross-platform.

**Rationale**: milestone-083 established 0.71.1 as the transitive-parity audit reference version. Using the same version keeps milestone-165 numbers comparable with 083's audit baseline.

**Alternatives considered**:
- **A. Latest release**: rejected — introduces unnecessary drift vs milestone 083.
- **B. 0.69.3** (older milestone-083 baseline per `tests/transitive_parity_cargo.rs`): rejected — 0.71.1 is the current milestone-083 pin per `tests/transitive_parity_common.rs`; using older would introduce unnecessary drift.

**Install-verification command**: `trivy --version | head -1` should print `Version: 0.71.1` — task T001 verifies.

## R2 — Syft version pin

**Decision**: Pin Syft at **1.44.0** (matches milestone-083 pin, and matches the version already installed at `syft --version = "syft 1.44.0"` on the audit host per 2026-07-05 pre-check).

**Rationale**: Already installed at the correct pin. No install step needed.

**Verification**: `syft version | head -1` should show `Syft 1.44.0`.

## R3 — SPDX 3 conformance validator

**Decision**: Reuse the existing pinned `spdx3-validate==0.0.5` at `.venv/spdx3-validate/bin/spdx3-validate` per memory `reference_spdx3_validator`.

**Rationale**: Already validated in prior milestones (078, 079, 080, 081); pinned to the version that mikebom's SPDX 3 emitter targets.

## R4 — Upstream commit-SHA pinning strategy

**Decision**: Clone at HEAD **at audit execution time**, record the exact commit SHA in the report header + reproduction appendix. Do NOT pre-pin at spec time (SHA would drift between spec and implementation).

**Rationale**: The audit is a snapshot-in-time measurement. Pinning at execution time gives the most-recent-realistic baseline. Reproducibility comes from recording the exact SHA in the report — future contributors can `git checkout <sha>` and re-run.

**Concrete commands**:
```bash
git clone --depth 1 https://github.com/kubernetes/kubernetes.git
KUBE_SHA=$(cd kubernetes && git rev-parse HEAD)
```

## R5 — Metrics collection methodology

**Decision**: Collect the following metrics per (target, tool) pair, matching milestone-158 T035's schema for comparability:

1. **Component counts**: `.components | length` (total) + `.components | map(select(.purl | startswith("pkg:golang/"))) | length` (Go) + `.components | map(select(.purl | startswith("pkg:npm/"))) | length` (npm) + `.components | length - (Go + npm)` (other).
2. **Edge counts**: `.dependencies | map(.dependsOn) | flatten | length`.
3. **BFS reachability**: BFS from `metadata.component.purl`, count reachable / total npm+Go components as percentage.
4. **Empty-version PURLs**: `[.components[].purl | select(test("^pkg:(npm|golang)/[^@]+@$"))] | length`.
5. **Phantom edges**: `[.dependencies[].dependsOn[] | select(test("^pkg:(npm|golang)/[^@]+@$"))] | length`.
6. **Scan wall-clock time**: `time` (real seconds).

**Rationale**: These are the standard metrics milestone 158 T035 used and milestone 164 continued. Cross-milestone comparability.

**BFS algorithm**: Python `collections.deque` implementation from milestones 158/163/164 podman-desktop measurements. Reused via a small shell wrapper.

## R6 — Root-cause classification methodology

**Decision**: For each mikebom-emitted orphan, empty-version PURL, or phantom edge, categorize by a **named bucket** derived from empirical patterns. Bucket-naming follows milestone-158's convention (kebab-case reason codes). Expected buckets (may extend during execution):

**Go-specific**:
- `stale-go-sum-entry`: entry in `go.sum` but not in the resolved module graph
- `vendored-not-on-go-mod-path`: `vendor/` component not surfaced via `go.mod`
- `generated-code-orphan`: file-tier component from generated Go source
- `staging-repo-artifact`: kubernetes-specific `staging/src/*` subrepo pattern

**npm-specific** (post-milestone-164, expected to be rare):
- `dead-lockfile-entry`: same class as podman-desktop's 12 residual orphans
- `optional-peer-not-installed`: `optionalDependencies:` never resolved by any peer
- `hoisted-unused`: pnpm hoisted a package no peer actually consumes

**Cross-cutting**:
- `binary-tier-attribution-gap`: milestone-096/109 binary components without source-tier PURL bindings
- `license-decode-failure`: license text decodes to SPDX id but decode fails (milestone 152/153 territory)
- `file-tier-unattributed`: milestone-133 file-tier component with no attribution

**Rationale**: Named buckets make the report skimmable + comparable across audits. If a bucket doesn't apply to this audit, it's just absent from the report.

**Disposition rule** (per FR-004): every bucket gets EITHER `fix-in-follow-on-milestone` OR `accept-as-is-with-rationale`. No "TBD" or "needs investigation" — the audit RESOLVES each bucket before the report ships.

## R7 — Tool comparison delta methodology

**Decision**: For each (target, ecosystem) pair, compute set differences between the three tools' component PURLs. Report:

```text
mikebom_advantage = mikebom_PURLs - trivy_PURLs - syft_PURLs
trivy_advantage   = trivy_PURLs - mikebom_PURLs - syft_PURLs
syft_advantage    = syft_PURLs - mikebom_PURLs - trivy_PURLs
common_all_three  = mikebom_PURLs ∩ trivy_PURLs ∩ syft_PURLs
mikebom_trivy_only = (mikebom ∩ trivy) - syft
etc.
```

**Rationale**: milestone-158 T035 used similar set-based comparison. Provides an at-a-glance signal of which tool is "richer" per ecosystem + a component-list for spot-checking.

**Sample-size cap**: for buckets with >20 members, the report includes the first 20 (sorted lex) + a "and N more" line. Full lists are dumped to `specs/165-k8s-argocd-audit/artifacts/<target>/delta_full.json` for reproducibility.

## R8 — Report structure

**Decision**: The report follows this section order:

```markdown
# Mikebom Audit: Kubernetes + ArgoCD (2026-07-05)

## Executive Summary
   3-5 sentences. Headline: mikebom's overall quality state + top follow-on recommendation.

## Baseline
   mikebom commit SHA + Trivy + Syft version pins + spdx3-validate version.

## Target 1 — Kubernetes
### Snapshot
   Upstream URL, commit SHA, clone size, scan wall-clock times.
### Per-Tool Metrics
   Table: mikebom | trivy | syft × {components, edges, BFS reach, per-ecosystem breakdown}.
### mikebom Failure Modes
   For each named bucket: description + count + concrete example (PURL) + disposition.
### Tool Comparison Delta
   mikebom_advantage / trivy_advantage / syft_advantage lists + common counts.
### SPDX Validation
   SPDX 2.3 pass/fail + SPDX 3 pass/fail (mikebom only per FR-006; Trivy/Syft "not attempted" if unsupported).

## Target 2 — ArgoCD
   Same structure as Target 1, plus:
### Cross-Ecosystem Interactions
   Any edges from pkg:npm/... → pkg:golang/... or vice versa.

## Recommended Follow-On Milestones
   Top 3 ranked (impact, blast radius, effort). Each: problem statement + evidence + rough scope.

## Backlog Observations
   Smaller findings not making the top 3 but worth recording.

## Reproduction Appendix
   Exact commands + tool versions + expected wall-clock ranges + how to regenerate all numbers.
```

**Rationale**: Skimmable top-to-bottom (Exec Summary → Per-Target → Recommendations → Appendix). Matches technical-audit-report conventions.

## R9 — Handling the "clean pass" case (SC-011)

**Decision**: If the audit finds mikebom is at parity with Trivy/Syft on both targets (no distinct failure modes worth follow-on milestone work), the "Recommended Follow-On Milestones" section explicitly reads:

```markdown
## Recommended Follow-On Milestones

**No immediate follow-on needed on the measured targets.**

The audit confirms mikebom is at competitive parity with Trivy 0.71.1 + Syft 1.44.0 on Kubernetes and ArgoCD source-tree scans. All observed orphans classify as `accept-as-is` (see Failure Modes sections). Future audit rounds may re-measure against different targets (e.g., Rust monorepos, Python data-science stacks) to surface additional bug classes.
```

**Rationale**: SC-011 explicitly permits this outcome. A clean-pass report is still a valuable milestone deliverable — it documents the CURRENT state as evidence for future baseline comparison.

## R10 — Intermediate artifact retention

**Decision**: The raw SBOM JSON files (~10-30 MB each × 6 files = ~60-180 MB total) live under `specs/165-k8s-argocd-audit/artifacts/` and are **gitignored via a `.gitignore` in that dir**. Only the analysis JSON (~10 KB per target) may optionally be committed if it aids report reproducibility.

**Rationale**: Milestone-090's fixture-stayset guidance says only manifest-bearing / test-critical artifacts belong in the repo. Raw SBOMs are regenerable from `<commit-SHA> + <tool-version> + <scan-command>` and shouldn't be committed. The report itself contains all the extracted metrics.

## Open items (none blocking)

All research questions resolved. Ready for Phase 1 (data-model.md + contracts/README + quickstart.md).
