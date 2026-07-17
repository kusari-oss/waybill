# Quickstart: Helm `--helm-render` Subprocess Implementation

**Date**: 2026-07-17
**Audience**: mikebom maintainer implementing or reviewing m203.

## Prerequisites

- Working mikebom checkout on branch `203-helm-render-subprocess`.
- `cargo +stable` toolchain (existing workspace toolchain).
- For Reproducer 1 only: `helm` v3.x on `$PATH` (locally install via `brew install helm` on macOS, `apt install helm` on Debian/Ubuntu, or use a container).

## Reproducer 1 — Verify US1 (successful rendered extraction)

```bash
# Requires real helm binary.
cargo test --manifest-path mikebom-cli/Cargo.toml --test helm_reader \
  --features '' \
  -- --nocapture us1 2>&1 | tail

# OR direct scan:
mkdir -p /tmp/m203-chart/templates
cat > /tmp/m203-chart/Chart.yaml <<'EOF'
apiVersion: v2
name: test-chart
version: 0.1.0
EOF
cat > /tmp/m203-chart/values.yaml <<'EOF'
image:
  repository: nginx
  tag: 1.27.0
EOF
cat > /tmp/m203-chart/templates/deployment.yaml <<'EOF'
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
      - image: {{ .Values.image.repository }}:{{ .Values.image.tag }}
EOF

mikebom --offline sbom scan --helm-render --path /tmp/m203-chart \
  --format cyclonedx-json --output /tmp/m203-rendered.cdx.json --no-deep-hash
jq '.components[]? | .purl' /tmp/m203-rendered.cdx.json
```

**Expected post-m203**: `pkg:oci/nginx@1.27.0` (or format-normalized equivalent). No `{{` characters in any emitted PURL.

## Reproducer 2 — Verify US2.1 (missing helm binary → fallback)

```bash
# Empty PATH removes helm from lookup.
PATH="" mikebom --offline sbom scan --helm-render --path /tmp/m203-chart \
  --format cyclonedx-json --output /tmp/m203-fallback-binary.cdx.json \
  --no-deep-hash 2>&1 | grep -E 'BinaryNotFound|falling back|extraction complete'
```

**Expected**: scan exits 0, WARN line mentions `BinaryNotFound`, extraction falls back to unrendered.

## Reproducer 3 — Verify US2.3 (timeout → fallback)

```bash
# Point at a stub script that sleeps forever.
mkdir -p /tmp/m203-stub
cat > /tmp/m203-stub/helm <<'EOF'
#!/bin/sh
sleep 3600
EOF
chmod +x /tmp/m203-stub/helm

PATH="/tmp/m203-stub:$PATH" MIKEBOM_HELM_RENDER_TIMEOUT_SECS=2 \
  mikebom --offline sbom scan --helm-render --path /tmp/m203-chart \
  --format cyclonedx-json --output /tmp/m203-fallback-timeout.cdx.json \
  --no-deep-hash 2>&1 | grep -E 'Timeout|falling back'
```

**Expected**: scan exits within ~3-4s (2s timeout + cleanup), WARN mentions `Timeout`, extraction falls back to unrendered.

## Reproducer 4 — Verify FR-009 (non-Helm scan byte-identity)

```bash
# Scan any non-Helm project WITHOUT --helm-render (default).
mikebom --offline sbom scan --path /some/non-helm/project \
  --format cyclonedx-json --output /tmp/m203-nonhelm-a.cdx.json --no-deep-hash

# Same scan WITH --helm-render — the flag is ignored for non-Helm scans.
mikebom --offline sbom scan --helm-render --path /some/non-helm/project \
  --format cyclonedx-json --output /tmp/m203-nonhelm-b.cdx.json --no-deep-hash

diff /tmp/m203-nonhelm-a.cdx.json /tmp/m203-nonhelm-b.cdx.json && echo "byte-identical"
```

**Expected**: `byte-identical`. Non-Helm scans skip the helm reader entirely (Chart.yaml gate at helm.rs:288+).

## Reproducer 5 — Verify SC-006 pre-PR wall-clock delta

```bash
git checkout main
time ./scripts/pre-pr.sh 2>&1 | tail -3   # baseline

git checkout 203-helm-render-subprocess
time ./scripts/pre-pr.sh 2>&1 | tail -3   # post-m203
```

Delta MUST be ≤ 5s per SC-006. Expected delta ≈0s (small classifier extension; new US2 tests are stub-script based, ~100ms each; US1 gated behind env var).

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.

## Empirical re-verification at implement time (m199-m202 lesson)

Per `feedback_verify_research_empirical_claims` memory: before finalizing tasks.md, re-run:

```bash
git diff --stat mikebom-cli/tests/fixtures/ 2>&1 | tail
```

**Expected**: only the new `helm/render_success_m203/` + `helm/render_stub_scripts_m203/` fixtures. Any existing golden JSON drift signals unexpected non-Helm-scan reclassification — investigate.
