# Quickstart: milestone 165 — Kubernetes + ArgoCD audit

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Contributor onboarding for milestone 165's audit run. Assumes a working mikebom dev environment.

## 1. Prerequisites

- Rust stable toolchain (workspace-managed) — for the milestone-164 release build of mikebom.
- Python 3.10+ — for BFS + orphan classification analysis (matches milestone-078 precedent).
- POSIX tools: `git`, `jq`, `time`, `du`, `python3`.
- **Trivy 0.71.1** — install via `brew install aquasecurity/trivy/trivy` (macOS) OR `go install github.com/aquasecurity/trivy/cmd/trivy@v0.71.1` (cross-platform). Verify: `trivy --version | head -1` shows `Version: 0.71.1`.
- **Syft 1.44.0** — likely already installed (verify: `syft version | head -1` shows `Syft 1.44.0`). If not, install via `curl -sSfL https://raw.githubusercontent.com/anchore/syft/main/install.sh | sh -s -- -b /usr/local/bin v1.44.0`.
- **spdx3-validate 0.0.5** at `.venv/spdx3-validate/bin/spdx3-validate` (per memory `reference_spdx3_validator`; already installed on this repo's dev host).
- **~1 GB free disk space** for the two upstream clones + intermediate SBOM artifacts.

## 2. Overview

Milestone 165 delivers:
- **Primary**: `docs/audits/2026-07-05-kubernetes-argocd.md` — the persistable audit report.
- **Secondary**: `specs/165-k8s-argocd-audit/artifacts/` — regenerable intermediate SBOMs + parsed metrics (gitignored).

Total task surface: ~15-18 tasks (see tasks.md). Estimated wall-clock: 2-4 hours end-to-end (clone: ~5 min; 6 scans: ~10 min; analysis: ~30 min; report writing: 1-3 hrs depending on findings).

## 3. Step-by-step execution

### 3a. Verify toolchain (T001–T003)

```bash
# Trivy pin
trivy --version | head -1
# Expected: "Version: 0.71.1" (or newer with a note in the report)

# Syft pin
syft version | head -1
# Expected: "Syft 1.44.0"

# spdx3-validate
.venv/spdx3-validate/bin/spdx3-validate --version
# Expected: "0.0.5"

# mikebom release build
cargo +stable build --release -p mikebom
./target/release/mikebom --version
```

### 3b. Create directories (T003)

```bash
mkdir -p docs/audits
mkdir -p specs/165-k8s-argocd-audit/artifacts/{kubernetes,argocd}
# gitignore the intermediate artifacts dir
cat > specs/165-k8s-argocd-audit/artifacts/.gitignore <<'EOF'
# Milestone 165 intermediate audit artifacts — regenerable from
# pinned commit SHAs + tool versions per docs/audits/2026-07-05-*.md
# reproduction appendix. Not versioned per milestone-090 fixture
# stayset guidance.
*.json
*.spdx.json
*.cdx.json
EOF
```

### 3c. Clone the targets (T004)

```bash
WORKDIR=$(mktemp -d /tmp/mikebom-audit-165.XXXXXX)
cd "$WORKDIR"

# Kubernetes
git clone --depth 1 https://github.com/kubernetes/kubernetes.git
cd kubernetes && KUBE_SHA=$(git rev-parse HEAD) && cd ..

# ArgoCD
git clone --depth 1 https://github.com/argoproj/argo-cd.git
cd argo-cd && ARGO_SHA=$(git rev-parse HEAD) && cd ..

# Record SHAs — these go in the report header + reproduction appendix.
echo "KUBE_SHA=$KUBE_SHA"
echo "ARGO_SHA=$ARGO_SHA"
echo "AUDIT_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
```

### 3d. Run mikebom + Trivy + Syft (T005–T008)

For each target, run each tool. Time each scan.

```bash
# Kubernetes — mikebom CDX
cd "$WORKDIR/kubernetes"
time /path/to/mikebom-repo/target/release/mikebom --offline sbom scan \
    --path . \
    --output /path/to/mikebom-repo/specs/165-k8s-argocd-audit/artifacts/kubernetes/mikebom.cdx.json \
    --no-deep-hash 2>&1 | tee mikebom-log.txt

# Kubernetes — mikebom SPDX 2.3
time /path/to/mikebom-repo/target/release/mikebom --offline sbom scan \
    --path . \
    --format spdx-2.3-json \
    --output /path/to/mikebom-repo/specs/165-k8s-argocd-audit/artifacts/kubernetes/mikebom.spdx23.json \
    --no-deep-hash

# Kubernetes — mikebom SPDX 3
time /path/to/mikebom-repo/target/release/mikebom --offline sbom scan \
    --path . \
    --format spdx-3-json \
    --output /path/to/mikebom-repo/specs/165-k8s-argocd-audit/artifacts/kubernetes/mikebom.spdx3.json \
    --no-deep-hash

# Kubernetes — Trivy
time trivy fs --format cyclonedx --output /path/.../artifacts/kubernetes/trivy.cdx.json .

# Kubernetes — Syft
time syft . --output cyclonedx-json > /path/.../artifacts/kubernetes/syft.cdx.json

# Repeat for ArgoCD (target=argocd, path=$WORKDIR/argo-cd).
```

### 3e. Analyze SBOMs (T009–T013)

Run the analysis Python script (see tasks.md T009 for the script path). It reads all 6 CDX files + 2 mikebom SPDX files and produces `analysis.json` per target.

```bash
python3 specs/165-k8s-argocd-audit/scripts/analyze.py \
    --target-name kubernetes \
    --sboms-dir specs/165-k8s-argocd-audit/artifacts/kubernetes \
    --commit-sha "$KUBE_SHA" \
    > specs/165-k8s-argocd-audit/artifacts/kubernetes/analysis.json

python3 specs/165-k8s-argocd-audit/scripts/analyze.py \
    --target-name argocd \
    --sboms-dir specs/165-k8s-argocd-audit/artifacts/argocd \
    --commit-sha "$ARGO_SHA" \
    > specs/165-k8s-argocd-audit/artifacts/argocd/analysis.json
```

The `analyze.py` script computes:
- Per-tool component counts, edge counts, BFS reachability
- Ecosystem breakdown
- Empty-version PURL + phantom-edge invariant checks (SC-004 for mikebom)
- Orphan classification into named buckets (research §R6)
- Tool comparison deltas (set differences)

### 3f. Validate SPDX conformance (T011)

```bash
# SPDX 3
.venv/spdx3-validate/bin/spdx3-validate \
    specs/165-k8s-argocd-audit/artifacts/kubernetes/mikebom.spdx3.json
# expect: "PASS" or specific error list

# SPDX 2.3 — via the existing jsonschema-based gate
cargo +stable test --test spdx23_conformance -- kubernetes
# (or manually run the schema validator per milestone-078 pattern)
```

### 3g. Write the report (T014)

Populate `docs/audits/2026-07-05-kubernetes-argocd.md` per data-model.md E7 + research.md §R8 structure. Use `analysis.json` per target as the input; hand-write the Executive Summary + top-3 Recommended Follow-Ons + Backlog Observations.

### 3h. Pre-PR gate (T015)

```bash
./scripts/pre-pr.sh
# Expected: 4000+ tests pass, 0 fail; clippy clean.
# Verifies FR-010 (no production code changes) + SC-008 (byte-identity).
```

### 3i. Commit + open PR (T016)

```bash
git add docs/audits/2026-07-05-kubernetes-argocd.md \
        specs/165-k8s-argocd-audit/

git commit -m "$(cat <<'EOF'
docs(165): empirical audit of mikebom against Kubernetes + ArgoCD

Round-3 empirical measurement extending the milestone 158 T035 audit
pattern to a Go monorepo at scale (kubernetes) and a polyglot Go+npm
target (argocd). Zero production code changes; deliverable is a
persistable Markdown report at docs/audits/2026-07-05-kubernetes-argocd.md.

Empirical findings + top-3 follow-on milestone recommendations
documented in the report. Pre-PR gate: N passed / 0 failed.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"

git push -u fork 165-k8s-argocd-audit
gh pr create --repo kusari-oss/mikebom --base main \
    --head mlieberman85:165-k8s-argocd-audit \
    --title "docs(165): empirical audit of mikebom against Kubernetes + ArgoCD" \
    --body "..."
```

## 4. Verify the deliverable

```bash
# SC-001: report exists
test -f docs/audits/2026-07-05-kubernetes-argocd.md && echo "SC-001 PASS"

# SC-002: both targets measured
grep -c "^## Target" docs/audits/2026-07-05-kubernetes-argocd.md
# Expect: 2

# SC-007: pre-PR gate
./scripts/pre-pr.sh 2>&1 | tail -3
# Expect: ">>> all pre-PR checks passed."

# SC-008: golden byte-identity (via workspace test)
cargo +stable test --workspace --no-fail-fast 2>&1 | grep -E '^test result' | \
    awk '{p+=$4; f+=$6} END {print "passed="p" failed="f}'
# Expect: same counts as pre-165 (no golden diffs)
```

## 5. Common pitfalls

- **Trivy version drift**: newer Trivy versions may add columns to the CDX output. If the `analyze.py` script errors on unknown fields, it'll surface at T009 — pin trivy to 0.71.1 exactly.
- **Kubernetes clone size**: at ~230 MB, the clone alone takes 30-60 seconds on a good connection. Don't count this in scan wall-clock.
- **ArgoCD has no top-level `pnpm-lock.yaml`**: verify at inspection time — argocd's UI is under `ui/` and may use its own package manager. This affects US2's npm-side analysis.
- **License heartbeat**: milestone 152/153 SPDX license work may surface license-decode edge cases on kubernetes' vast license diversity. If so, record as backlog observations rather than blocking recommendations.
- **Report length runaway**: aim for 1000-1500 lines. If you find yourself writing 3000+ lines, that's a signal you're doing follow-on milestone work inside 165 — split it out.

## 6. Post-merge

After PR merges, the report becomes the durable quality-record for this point in time. Future audit rounds (milestone 200, 300, etc.) add new dated files under `docs/audits/`. Cross-audit comparison is the accountability mechanism.
