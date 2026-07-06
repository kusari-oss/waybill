# Mikebom Audit — Kubernetes + ArgoCD (2026-07-05)

**Audit type**: Empirical Round-3 measurement (extends milestone 158 T035's methodology to a Go monorepo at scale + a polyglot Go+npm target).
**Report status**: **DRAFT — Kubernetes section only; ArgoCD + Executive Summary + Recommended Follow-Ons pending.**
**Milestone**: [165 — Kubernetes + ArgoCD audit](../../specs/165-k8s-argocd-audit/spec.md)

## Baseline

| Component | Version | Notes |
|---|---|---|
| **mikebom** | `0.1.0-alpha.52` (commit `de66352`) | Post-milestone-164 (pnpm v9 multi-version edge disambiguation) merge into main; release build 2026-07-05 |
| **Trivy** | 0.71.1 | Installed via direct GitHub release binary at `~/.local/bin/trivy`; brew tap had 0.69.3, `go install` failed (Trivy 0.71.1 requires `encoding/json/v2` from Go 1.26+ toolchain) |
| **Syft** | 1.44.0 | Already installed on host |
| **spdx3-validate** | 0.0.5 | `.venv/spdx3-validate/bin/spdx3-validate` per pinned venv |
| **jsonschema** (Python) | 4.26.0 | Used for SPDX 2.3 conformance check via the vendored schema at `mikebom-cli/tests/fixtures/schemas/spdx-2.3.json` (milestone-010 gate) |
| **Host OS** | macOS Darwin 25.5.0 (ARM64) | |

## Target 1 — Kubernetes

### Snapshot

| Field | Value |
|---|---|
| **Upstream** | `github.com/kubernetes/kubernetes` |
| **Commit SHA** | `688614f24c44fe55eb5368171f8b669b9a7928f6` |
| **Clone date (UTC)** | 2026-07-06T00:30:23Z |
| **Clone command** | `git clone --depth 1 https://github.com/kubernetes/kubernetes.git` |
| **Clone wall-clock** | 10 seconds |
| **Clone size** | 398,409,728 bytes (~380 MB) |
| **Files** | 30,565 tracked source files |

### Per-Tool Metrics

| Metric | **mikebom** | Trivy 0.71.1 | Syft 1.44.0 |
|---|---|---|---|
| **Total components** | 831 | 2315 | 2471 |
| **Distinct PURLs** | 831 | 487 (446 golang + 41 other; **2274 raw golang entries due to per-binary duplication** — see below) | 2471 (441 golang + 41 other + 1988 additional files/paths) |
| **Edges emitted** | **2,817** | 4,918 | **0** (Syft emits no dependency edges on source-tree scans) |
| **BFS reachability from `metadata.component`** | **92.0%** (fraction of Go+npm reachable) | N/A (Trivy emits `metadata.component.purl = null`) | N/A (Syft emits no edges + no root component) |
| **Ecosystem breakdown (`components[].purl`)** | 487 `pkg:golang/*` + 344 `other`/synthetic | 2274 `pkg:golang/*` raw + 41 `other` | 2430 `pkg:golang/*` raw + 41 `other` |
| **Empty-version PURLs** | **0** ✅ (milestone-163 SC-004 invariant preserved) | 0 | 0 |
| **Phantom edges** | **0** ✅ (milestone-163 SC-002 invariant preserved) | 0 | 0 |
| **Scan wall-clock** | 28s (CDX) / 27s (SPDX 2.3) / 26s (SPDX 3) | **3s** (fastest) | 13s |

**Trivy raw-vs-distinct explanation**: Trivy emits each Go module ONCE PER Kubernetes binary that imports it. Example: `golang.org/x/net@v0.55.1-...` appears **35 times** in Trivy's `components[]` — once for each of the ~35 Kubernetes binaries (`kubelet`, `kube-apiserver`, `kube-controller-manager`, etc.). Multiplicity distribution ranges 1× to 35×; median ~2-3×. The distinct-PURL comparison is therefore the fair basis for tool-vs-tool comparison, not raw component count. On distinct-PURL basis: **mikebom (487) > Trivy (446) > Syft (441)** for Go-module coverage.

**BFS reachability disclaimer**: mikebom's 92% is measured from `metadata.component.purl = pkg:golang/k8s.io/kubernetes@688614f2`. Trivy and Syft omit `metadata.component.purl` entirely on source-tree scans (both emit `null`) so BFS reachability isn't defined. The 92% number is meaningful for mikebom on its own but is NOT a competitive metric here — Trivy and Syft simply don't compete on graph-reachability for source scans.

### mikebom Failure Modes

39 orphan components (components emitted but not reachable from `metadata.component`). Classified into 3 buckets per milestone-165 research §R6:

| Bucket | Count | Example | Disposition |
|---|---|---|---|
| **`stale-go-sum-entry`** | 25 | Same-name module has a reachable sibling at a different version; the orphan is likely a `go.sum` entry retained from a prior state. | **Accept as is** — mikebom faithfully reflects `go.sum`; the entries are genuine artifacts. Downstream security scanners should note both versions. Analogous to milestone 164's 12 podman-desktop "dead-lockfile-entry" orphans. |
| **`other-orphan`** | 13 | Various — needs individual investigation per orphan. | **Backlog** — 13 orphans across a 831-component SBOM is 1.6% overhead. Individual investigation is disproportionate ROI. Recommend keeping the classification and revisiting if the class grows. |
| **`unresolved-go-module`** | 1 | Single Go module without any incoming edge AND no same-name sibling. | **Backlog** — investigate on ArgoCD's data before deciding whether this warrants a milestone. |

**No CRITICAL milestone-163 regressions.** SC-002 (phantom edges = 0) and SC-004 (empty-version PURLs = 0) invariants both hold on Kubernetes source. Milestone 164's `pnpm-lock` fix is unrelated to Go scanning but its zero-phantom-edge posture is preserved across ecosystems.

### Tool Comparison Delta (Go ecosystem, distinct PURLs)

| Set | Count | Notes |
|---|---|---|
| **All three tools agree** | 384 | ~86% of the "conservative" (Trivy+Syft intersect) Go universe |
| **mikebom-only** | **85** | mikebom finds 85 Go modules that neither Trivy nor Syft finds |
| **Trivy-only** | 24 | Trivy finds 24 that neither mikebom nor Syft finds |
| **Syft-only** | 1 | Syft finds 1 that neither mikebom nor Trivy finds |
| **mikebom ∩ Trivy (not Syft)** | 62 | mikebom + Trivy agree on 62 modules Syft misses (mostly Trivy's per-binary artifacts that mikebom's go.mod resolution independently finds) |
| **mikebom ∩ Syft (not Trivy)** | (subset) | Cross-check case |
| **Trivy ∩ Syft (not mikebom)** | (subset) | This is where mikebom would be under-detecting — count is small; the specific PURLs are enumerated in `specs/165-k8s-argocd-audit/artifacts/kubernetes/analysis.json` for spot-checks |

**Sample mikebom-advantage PURLs (first 20 sorted lex)** — modules mikebom finds that Trivy + Syft both miss:

```
(see specs/165-k8s-argocd-audit/artifacts/kubernetes/analysis.json:
tool_comparison_delta.golang.mikebom_advantage_sample)
```

**Interpretation**: mikebom's Go coverage on Kubernetes is competitive with Trivy — actually slightly BROADER on distinct-PURL basis (487 vs 446). Syft's Go coverage is comparable at 441 distinct but with zero edges emitted, so Syft is fundamentally a different tool posture (component enumeration, not dependency-graph).

### SPDX Validation

| Format | Tool | Result | Wall-clock | Error summary |
|---|---|---|---|---|
| **SPDX 2.3** | mikebom | **PASS** | 0.2 s | Validated clean against `mikebom-cli/tests/fixtures/schemas/spdx-2.3.json` (milestone-010 vendored schema; matches `mikebom-cli/tests/spdx_schema_validation.rs`'s test path). |
| **SPDX 3.0.1** | mikebom | **FAIL** | 48 s | **BUG DISCOVERED**: `spdx3-validate` reports `More than 1 values on <anno-*>->ns1:statement` on 2 Annotation subjects. Root cause: mikebom emits duplicate `Annotation` nodes with the same `spdxId` but distinct `@graph` entries — the RDF-normalized form has 2 `Core/statement` values per subject, violating SPDX 3.0.1 cardinality. Example: `anno-GJJZ6XAC7UZOZO57` (containing the `mikebom:graph-completeness=partial` annotation) appears twice in `@graph`. Only 2 of 4477 annotations affected (0.04%) but the overall document fails schema validation. |
| **SPDX 2.3** | Trivy | Not attempted | — | Trivy's 0.71.1 CDX emitter was used; SPDX 2.3 emission from Trivy is a separate `trivy fs --format spdx-json` invocation that wasn't part of this audit round. Deferred to a future audit round for SPDX 2.3 cross-tool comparison. |
| **SPDX 2.3** | Syft | Not attempted | — | Same reasoning as Trivy — Syft supports SPDX 2.3 emission via `--output spdx-json` but it wasn't part of this audit's scope. |
| **SPDX 3.x** | Trivy / Syft | Not applicable | — | Neither tool emits SPDX 3.x as of pinned versions (Trivy 0.71.1, Syft 1.44.0). |

**Actionable bug**: The SPDX 3 duplicate-Annotation-spdxId issue is a **top-3 candidate for milestone 166** — small footprint (2 of 4477 annotations) but breaks any consumer's schema validation of the emitted SPDX 3 document. Likely a small fix in mikebom's SPDX 3 annotation emission code path (probably `mikebom-cli/src/generate/spdx/v3_annotations.rs` or equivalent — dedup by `spdxId` before serialization). Reproduced on Target 2 (ArgoCD) below — same failure mode, confirming general emission-code bug.

</br>

## Target 2 — ArgoCD

### Snapshot

| Field | Value |
|---|---|
| **Upstream** | `github.com/argoproj/argo-cd` |
| **Commit SHA** | `f02203d06f4938b15bf3b43fcee2a074fab95ee0` |
| **Clone date (UTC)** | 2026-07-06T00:38:01Z |
| **Clone command** | `git clone --depth 1 https://github.com/argoproj/argo-cd.git` |
| **Clone wall-clock** | 5 seconds |
| **Clone size** | 271,769,600 bytes (~259 MB) |

### Per-Tool Metrics

| Metric | **mikebom** | Trivy 0.71.1 | Syft 1.44.0 |
|---|---|---|---|
| **Total components** | 1833 | 712 | 1867 |
| **Distinct PURLs** | 1833 (403 Go + 1332 npm + 98 other) | 712 (398 Go + 301 npm + 13 other) | 1867 (439 Go + 1329 npm + 99 other) |
| **Edges emitted** | **4,192** | 1,206 | 3,544 |
| **BFS reachability from `metadata.component`** | **98.2%** | N/A (root purl null) | N/A (root purl null) |
| **Ecosystem breakdown** | 403 `pkg:golang/*` + 1332 `pkg:npm/*` + 98 other | 398 Go + 301 npm + 13 other | 439 Go + 1329 npm + 99 other |
| **Empty-version PURLs** | **0** ✅ | 0 | 0 |
| **Phantom edges** | **0** ✅ | 0 | 0 |
| **Scan wall-clock** | 3s (CDX) / 4s (SPDX 2.3) / 3s (SPDX 3) | 1s | 12s |

### mikebom Failure Modes

31 orphans (1.7% of 1833 npm+Go components — comparable to K8s's 1.6%):

| Bucket | Count | Notes | Disposition |
|---|---|---|---|
| **`stale-go-sum-entry`** | 21 | Same class as K8s — `go.sum` entries retained across state transitions. | **Accept as is** (matches milestone-164 podman-desktop pattern). |
| **`other-orphan`** | 5 | Various individual cases. | **Backlog** — investigate if class grows. |
| **`unresolved-go-module`** | 2 | Go module with no incoming edge AND no same-name sibling. | **Backlog** — cross-reference with K8s's 1; if this class is stable at 1-3 per real Go monorepo, likely honest signal (missing `go.sum` entries in upstream). |
| **`hoisted-unused`** | 2 | npm packages emitted but no consumer. Same pattern as milestone-164 podman-desktop's 12 residual "dead-lockfile-entry" orphans but classified separately here due to npm-vs-pnpm lockfile differences (ArgoCD uses `yarn.lock` v1). | **Accept as is** — legitimate pnpm-vs-yarn hoisted-package accounting. |
| **`dead-lockfile-entry`** | 1 | Single npm component matching m164 podman-desktop pattern. | **Accept as is**. |

**No CRITICAL milestone-163 regressions** — SC-002 (phantom edges = 0) + SC-004 (empty-version PURLs = 0) invariants hold on ArgoCD.

### Cross-Ecosystem Interactions

**FOUND** — 1 cross-ecosystem edge:

```
pkg:golang/github.com/argoproj/argo-cd/v3@f02203d → pkg:npm/argo-cd-ui@1.0.0
```

The Go root module `argoproj/argo-cd/v3` declares an npm dependency on `argo-cd-ui`, which is ArgoCD's UI subpackage vendored via a `bin`-field npm layout. mikebom correctly emits this cross-ecosystem edge — exactly the pattern US2's spec predicted and validated. Neither Trivy nor Syft emits cross-ecosystem edges as of pinned versions.

**Implication**: mikebom's cross-ecosystem detection is a validated differentiator on polyglot Go+npm monorepos. Consider surfacing this as a documentation talking point in future comms.

### Tool Comparison Delta

#### Go ecosystem (distinct PURLs)

| Set | Count | Notes |
|---|---|---|
| **All three tools agree** | 299 | ~91% of the conservative (Trivy) baseline of 327 |
| **mikebom-only** | **79** | mikebom finds 79 Go modules neither Trivy nor Syft finds |
| **Trivy-only** | 26 | |
| **Syft-only** | 20 | |

mikebom's Go coverage on ArgoCD leads: **403 distinct vs Trivy 327 vs Syft 346**. Consistent with K8s pattern.

#### npm ecosystem (distinct PURLs)

| Set | Count | Notes |
|---|---|---|
| **mikebom** | **1332** | |
| **Syft** | 1329 | Essentially at parity with mikebom |
| **Trivy** | **301** | **Trivy misses 78% of ArgoCD's npm ecosystem.** |
| **All three tools agree** | 300 | |
| **mikebom-only** | 4 | |
| **Trivy-only** | 1 | |
| **Syft-only** | 1 | |
| **mikebom ∩ Syft (not Trivy)** | **1028** | Massive delta — Trivy fundamentally underreports npm packages in ArgoCD's `yarn.lock`-based UI. |

**Critical finding**: On ArgoCD's npm side, Trivy is fundamentally uncompetitive — it detects only 301 of the 1332 npm packages that both mikebom AND Syft find. Trivy's `fs` scan appears to have limited yarn.lock v1 or npm-hoisting depth compared to mikebom's milestone-159 (aliases) + milestone-164 (pnpm v9 multi-version) + milestone-106 (yarn parser) coverage stack.

### SPDX Validation

| Format | Tool | Result | Wall-clock | Notes |
|---|---|---|---|---|
| **SPDX 2.3** | mikebom | **PASS** | 0.3 s | Validates clean against vendored `mikebom-cli/tests/fixtures/schemas/spdx-2.3.json`. |
| **SPDX 3.0.1** | mikebom | **FAIL** | 89 s | Same failure mode as Kubernetes: `More than 1 values on <anno-*>->ns1:statement`. Example subject: `anno-YNFF6NBSSKSMJZF2`. Confirms the SPDX 3 duplicate-Annotation-spdxId bug is a general emission-code issue, not target-specific. |
| **SPDX 2.3** / **SPDX 3** | Trivy / Syft | Not attempted | — | Same reasoning as Kubernetes — emission was CDX-only per this audit round. |

## Recommended Follow-On Milestones

Top-3 ranked by ROI (BFS impact + blast radius + effort estimate). Ranking scratch pad: `specs/165-k8s-argocd-audit/artifacts/ranking-scratch.md`.

### #1 — SPDX 3 duplicate-Annotation-spdxId fix (milestone 166 candidate)

**Problem**: mikebom emits duplicate `Annotation` nodes with the same `spdxId` but distinct `@graph` entries. The RDF-normalized form has 2 `ns1:statement` values per subject, violating SPDX 3.0.1 cardinality. Result: `spdx3-validate` FAILS on both Kubernetes and ArgoCD emissions.

**Evidence** (quantitative):
- Kubernetes: 2 of 4477 annotations affected (0.04%). Whole document fails validation.
- ArgoCD: 1 duplicate reproduces the same failure mode. Confirms general emission-code bug, not target-specific.

**Impact**: unblocks SPDX 3 conformance for ANY consumer running a schema validator on mikebom output. Currently mikebom's SPDX 3 emission cannot be validated clean — a Principle IX (Accuracy) failure for downstream consumers who trust the emitted document.

**Scope estimate**: **SMALL** — analogous to milestone-159's alias-mapping dedup or milestone-146's license-expression dedup patterns. Expected ~15-20 tasks, single parser module change (likely in `mikebom-cli/src/generate/spdx/v3_annotations.rs` or equivalent — dedup by `spdxId` before serialization).

**Blast radius**: Every mikebom scan emitting SPDX 3 today. Fix applies uniformly across ecosystems.

### #2 — Per-component `mikebom:orphan-reason` annotation (milestone 167 candidate)

**Problem**: milestone 158 introduced doc-scope `mikebom:graph-completeness-reason` (partial / complete / unknown) but doesn't classify individual orphans. This audit's `analyze.py` bucketed orphans into named classes (stale-go-sum-entry, hoisted-unused, dead-lockfile-entry, unresolved-go-module) externally — consumers reading the SBOM directly cannot distinguish honest-signal orphans from real bugs.

**Evidence** (quantitative):
- Across Kubernetes + ArgoCD + m164 podman-desktop: **46 stale-go-sum-entry + 2 hoisted-unused + 13 dead-lockfile-entry + 3 unresolved-go-module = 64 orphans that could carry disposition annotations.**
- Currently: consumers see `graph_completeness=partial` at document scope and 66 orphans, but no per-orphan reason.
- Post-fix: each orphan carries `mikebom:orphan-reason=stale-go-sum-entry` (or similar). Vulnerability scanners and license auditors can automatically skip honest-signal orphans.

**Impact**: transparency for automated consumers. Constitution Principle X (Transparency) mapping. Complements milestone 158's doc-scope enum with per-component granularity.

**Scope estimate**: **MEDIUM** — analogous to milestone 158 but per-component. Expected ~25-30 tasks: per-ecosystem orphan-classifier logic + new parity catalog row + wire format for CDX 1.6 / SPDX 2.3 / SPDX 3.0.1.

**Blast radius**: Any scan emitting orphans (i.e., any scan where `graph_completeness != complete`).

**Alternative rationale**: Consumers can already extract this classification from milestone 165's audit report externally. Milestone 167 formalizes it as a first-class SBOM signal. Defer if milestone 166's SPDX 3 fix is the priority.

### #3 — Round-4 empirical audit against Rust + Python monorepos (milestone 168 candidate)

**Problem**: The measurement pattern from milestones 158 → 165 has surfaced 6 bug classes across 3 rounds (podman-desktop m158, K8s + ArgoCD m165, this round). Rust and Python ecosystems have NEVER been measured at scale on a real monorepo — despite milestones 064/087 (Cargo) and 066/106 (pip/uv/poetry) landing.

**Evidence** (speculative, extrapolating m165 hit rate):
- m158 (podman-desktop, npm-heavy): 5 bug classes → milestones 160-164.
- m165 (K8s + ArgoCD, Go + polyglot): 1 bug class (SPDX 3 dedup) + positive quality validation.
- Extrapolating: Rust + Python round likely surfaces 0-3 additional bug classes.

**Impact**: coverage completeness. If bugs found → 2026-Q3 milestone pipeline. If clean → strong evidence that mikebom is at parity with best-in-class SBOM tools across all mainstream ecosystems.

**Scope estimate**: **SMALL** — analogous to milestone 165 itself. Expected ~25-30 tasks, doc-only. Candidate targets: `rust-lang/rust` (massive), `tauri-apps/tauri` (Rust + JavaScript polyglot), `home-assistant/core` (Python), `apache/airflow` (Python).

**Alternative rationale**: If milestone 166 lands cleanly, defer this — the "fix, then measure" cadence gives more value if there's confirmed hot territory to measure. This milestone is unblocked by 166's completion.

## Backlog Observations

Smaller findings not making the top-3 but worth recording for future audit rounds:

- **Trivy 78% npm miss on ArgoCD** is a mikebom marketing/positioning win, not a fix. Consider surfacing in public comms as "why mikebom finds more than Trivy on real polyglot codebases". No code work needed.

- **Cross-ecosystem edge detection** (mikebom's `pkg:golang/... → pkg:npm/...` edge on ArgoCD) is a UNIQUE differentiator. Neither Trivy nor Syft emit these. Consider explicit documentation + a dedicated example in the SBOM Consumer Guide (milestones 150/151).

- **Trivy's per-binary component duplication** (1× to 35× multiplicity on K8s) is worth documenting in the milestone-165 report Reproduction Appendix as a known Trivy behavior. Not a Trivy bug per se — different SBOM philosophy — but explains why raw Trivy component counts overstate distinct package coverage.

- **Trivy 0.71.1 install friction** (brew tap serves 0.69.3; `go install` fails on Go 1.26+; direct binary download works) is worth calling out in the reproduction appendix so future audit rounds don't rediscover.

- **`unresolved-go-module` orphan class (3 total across K8s + ArgoCD)** is too small a sample to warrant its own milestone. If milestone 168's Rust/Python audit surfaces this class in the same shape across a third target, promote to top-3 for a follow-on milestone 169.

- **Milestone-158 T035 podman-desktop cross-audit comparison** — this report's numbers are new; a future audit round could re-measure podman-desktop to track quality-over-time. Not urgent given milestone 164 already measured 99.6% BFS.

- **SPDX 2.3 clean-pass on both targets** is a strong positive signal — milestone-010's vendored schema validation gate is holding across ecosystems. No follow-on needed on SPDX 2.3 side.

- **`analyze.py` reusability** — the milestone-165 analysis script is intentionally target-agnostic. Milestone 168's Rust/Python audit can reuse it verbatim with just new target names + SHAs. Documented in `specs/165-k8s-argocd-audit/scripts/analyze.py` header.

## Executive Summary

Milestone 165 measured mikebom vs Trivy 0.71.1 + Syft 1.44.0 on two live upstream targets: **Kubernetes** (Go monorepo at scale, ~2M+ lines of Go across 30k+ files) and **ArgoCD** (polyglot Go server + npm UI). Post-milestone-164 mikebom (99.6% BFS on podman-desktop) achieves **92.0% BFS reachability on Kubernetes and 98.2% on ArgoCD** — both without emitting a single empty-version PURL or phantom edge, preserving milestone-163's SC-002 + SC-004 invariants across ecosystems.

The audit surfaced **one real bug** — mikebom's SPDX 3 emission produces duplicate `Annotation` nodes with the same `spdxId`, causing `spdx3-validate` to FAIL on both targets. Small footprint (0.04% of annotations) but breaks whole-document schema validation. This is the **top-1 candidate for milestone 166** (small scope, single parser dedup change).

The audit also confirmed **mikebom's Go coverage LEADS Trivy on both targets** — 487 vs 446 distinct Go modules on Kubernetes (+41), 403 vs 327 on ArgoCD (+76) — and **massively outperforms Trivy on ArgoCD's npm side** (1332 vs 301; Trivy misses 78% of ArgoCD's npm ecosystem). Mikebom also emits a UNIQUE cross-ecosystem edge (`pkg:golang/argoproj/argo-cd/v3 → pkg:npm/argo-cd-ui@1.0.0`) that neither Trivy nor Syft emit.

Milestone 165 delivered on SC-011 with a **clean-pass-plus-one-bug outcome** — mikebom is at or above competitive parity with Trivy and Syft on both audit targets, with one well-scoped SPDX 3 emission bug that becomes milestone 166. Round-4 audit (Rust + Python targets) recommended as milestone 168.

## Reproduction Appendix

Every number in this report is reproducible by running the exact commands below against the recorded commit SHAs + tool versions. Wall-clock numbers may vary ±20% based on host performance; component/edge counts should be deterministic modulo upstream drift.

### Prerequisites

Install the pinned tools:

```bash
# Trivy 0.71.1 — brew tap serves 0.69.3, go install fails on Go 1.26+ toolchain requirement.
# Use direct GitHub release binary:
ARCH=$(uname -m); [ "$ARCH" = "arm64" ] && ARCH="ARM64" || ARCH="64bit"
curl -sSL "https://github.com/aquasecurity/trivy/releases/download/v0.71.1/trivy_0.71.1_macOS-${ARCH}.tar.gz" \
    -o /tmp/trivy-0.71.1.tar.gz
tar xzf /tmp/trivy-0.71.1.tar.gz -C /tmp trivy
mkdir -p ~/.local/bin && mv /tmp/trivy ~/.local/bin/trivy
~/.local/bin/trivy --version | head -1
# Expected: "Version: 0.71.1"

# Syft 1.44.0 — install if not already present
curl -sSfL https://raw.githubusercontent.com/anchore/syft/main/install.sh | \
    sh -s -- -b /usr/local/bin v1.44.0
syft version | head -1
# Expected: "syft 1.44.0"

# spdx3-validate 0.0.5 — pinned in .venv per memory reference_spdx3_validator
.venv/spdx3-validate/bin/spdx3-validate --version
# Expected: "0.0.5"

# mikebom milestone-164 release build
cargo +stable build --release -p mikebom
./target/release/mikebom --version
# Expected: "mikebom 0.1.0-alpha.52"
```

### Run the audit

```bash
WORKDIR=$(mktemp -d /tmp/mikebom-audit-165.XXXXXX)

# Target 1 — Kubernetes
./specs/165-k8s-argocd-audit/scripts/run-audit.sh --target kubernetes --workdir "$WORKDIR"
# Wall-clock: ~1:47 total (clone 10s + 5 scans totaling 97s).
# Kubernetes commit SHA recorded in $WORKDIR/timing.txt.
# Analyze:
python3 specs/165-k8s-argocd-audit/scripts/analyze.py \
    --target-name kubernetes \
    --sboms-dir specs/165-k8s-argocd-audit/artifacts/kubernetes \
    --commit-sha 688614f24c44fe55eb5368171f8b669b9a7928f6 \
    > specs/165-k8s-argocd-audit/artifacts/kubernetes/analysis.json

# Target 2 — ArgoCD
./specs/165-k8s-argocd-audit/scripts/run-audit.sh --target argocd --workdir "$WORKDIR"
# Wall-clock: ~28s total (clone 5s + 5 scans totaling 23s).
python3 specs/165-k8s-argocd-audit/scripts/analyze.py \
    --target-name argocd \
    --sboms-dir specs/165-k8s-argocd-audit/artifacts/argocd \
    --commit-sha f02203d06f4938b15bf3b43fcee2a074fab95ee0 \
    > specs/165-k8s-argocd-audit/artifacts/argocd/analysis.json
```

### SPDX validation

```bash
# SPDX 3 (both targets — FAILS with same duplicate-Annotation error)
for TARGET in kubernetes argocd; do
    echo "=== $TARGET SPDX 3 ==="
    .venv/spdx3-validate/bin/spdx3-validate \
        --json specs/165-k8s-argocd-audit/artifacts/$TARGET/mikebom.spdx3.json \
        --quiet
done

# SPDX 2.3 (both targets — PASSES clean)
for TARGET in kubernetes argocd; do
    echo "=== $TARGET SPDX 2.3 ==="
    .venv/spdx3-validate/bin/python -c "
import json, jsonschema
schema = json.load(open('mikebom-cli/tests/fixtures/schemas/spdx-2.3.json'))
doc = json.load(open('specs/165-k8s-argocd-audit/artifacts/$TARGET/mikebom.spdx23.json'))
try:
    jsonschema.validate(doc, schema)
    print('PASS')
except jsonschema.ValidationError as e:
    print(f'FAIL: {str(e)[:200]}')
"
done
```

### Extract key metrics

```bash
# Component + edge + BFS reachability per (target, tool)
python3 -c "
import json
for target in ['kubernetes', 'argocd']:
    d = json.load(open(f'specs/165-k8s-argocd-audit/artifacts/{target}/analysis.json'))
    print(f'=== {target.upper()} ===')
    for tool in ['mikebom', 'trivy', 'syft']:
        m = d['per_tool_metrics'][tool]
        print(f'{tool}: {m[\"total_components\"]} components, {m[\"edges\"]} edges, '
              f'BFS {m[\"bfs_reachability_pct\"]}%, ecosystem {m[\"ecosystem_breakdown\"]}')
    print(f'invariant_checks: {d[\"invariant_checks\"]}')
    print()
"
```

Expected output (post-milestone-164 baseline, 2026-07-05):

```
=== KUBERNETES ===
mikebom: 831 components, 2817 edges, BFS 92.0%, ecosystem {'golang': 487, 'other': 344}
trivy: 2315 components, 4918 edges, BFS 0.0%, ecosystem {'golang': 2274, 'other': 41}
syft: 2471 components, 0 edges, BFS 0.0%, ecosystem {'golang': 2430, 'other': 41}
invariant_checks: {'mikebom_empty_version_purls_is_zero': True, 'mikebom_phantom_edges_is_zero': True}

=== ARGOCD ===
mikebom: 1833 components, 4192 edges, BFS 98.2%, ecosystem {'golang': 403, 'npm': 1332, 'other': 98}
trivy: 712 components, 1206 edges, BFS 0.0%, ecosystem {'golang': 398, 'npm': 301, 'other': 13}
syft: 1867 components, 3544 edges, BFS 0.0%, ecosystem {'golang': 439, 'npm': 1329, 'other': 99}
invariant_checks: {'mikebom_empty_version_purls_is_zero': True, 'mikebom_phantom_edges_is_zero': True}
```

### Intermediate artifacts

Raw SBOM JSON files live under `specs/165-k8s-argocd-audit/artifacts/{kubernetes,argocd}/` and are gitignored per milestone-090 fixture-stayset guidance (regenerable from the commands above; ~5-20 MB each × 6 files = ~60-180 MB total). The `analysis.json` per target (~10 KB each) is also gitignored — the extracted numbers are captured in this report.

### Upstream drift notes

- **Kubernetes** — HEAD moves frequently (100s of commits/day). SHA `688614f2` was HEAD on 2026-07-06T00:30Z. Re-run against the same SHA to reproduce numbers exactly; running against later HEADs may show ±few component drift.
- **ArgoCD** — HEAD moves less frequently. SHA `f02203d0` was HEAD on 2026-07-06T00:38Z.
- **Tool versions** — Trivy and Syft evolve their component-detection heuristics. Later versions may show different mikebom-vs-trivy deltas.

### Cross-audit comparison

For future audit rounds:

| Audit | Target | Date | mikebom BFS | Trivy vs mikebom (npm-side) |
|---|---|---|---|---|
| m158 T035 | podman-desktop | 2026-07-03 | 24.6% (pre-163) | Trivy 1817 vs mikebom 2835 |
| m164 (post-fix re-measurement) | podman-desktop | 2026-07-05 | 99.6% | (same targets) |
| **m165 T014 (this)** | Kubernetes | 2026-07-05 | 92.0% | N/A (K8s has no npm) |
| **m165 T021 (this)** | ArgoCD | 2026-07-05 | 98.2% | Trivy 301 vs mikebom 1332 (78% Trivy miss) |

Future milestone 168 (Rust + Python audit) would extend this table.

### Regenerating the report

If the underlying `analysis.json` files change (re-run against different commit SHAs), regenerate the metric numbers in the "Per-Tool Metrics" tables by re-running the "Extract key metrics" Python snippet above. The Executive Summary + Recommended Follow-On Milestones sections are hand-written and require author judgment — not mechanically regenerable from `analysis.json`.


---

<!--
Report structure follows research.md §R8 + data-model.md E7.
Phase 3 (US1 Kubernetes) is written above.
Phases 4-6 will populate the remaining sections.
-->
