# Quickstart: `mikebom:image-extraction-completeness` Annotation Implementation

**Date**: 2026-07-17
**Audience**: mikebom maintainer implementing or reviewing m204.

## Prerequisites

- Working mikebom checkout on branch `204-helm-completeness-annotation`.
- `cargo +stable` toolchain (existing workspace toolchain).
- For Reproducer 2 only: `helm` v3.x on `$PATH` (locally install via `brew install helm` on macOS, `apt install helm` on Debian/Ubuntu, or use a container).

## Reproducer 1 — Verify US1 (default unrendered helm scan → `"partial"`)

```bash
mkdir -p /tmp/m204-chart/templates
cat > /tmp/m204-chart/Chart.yaml <<'EOF'
apiVersion: v2
name: test-chart
version: 0.1.0
EOF
cat > /tmp/m204-chart/templates/deployment.yaml <<'EOF'
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
      - image: nginx:1.27.0
EOF

for fmt in cyclonedx-json spdx-json spdx-3-json; do
  mikebom --offline sbom scan --path /tmp/m204-chart \
    --format "$fmt" --output "/tmp/m204-us1.$fmt.json" --no-deep-hash
done

# CDX 1.6
jq '.metadata.properties[] | select(.name == "mikebom:image-extraction-completeness")' \
  /tmp/m204-us1.cyclonedx-json.json
# Expected: { "name": "mikebom:image-extraction-completeness", "value": "partial" }

# SPDX 2.3 — the annotation comment is JSON-in-a-string per m071 envelope.
jq '.annotations[]? | select(.comment | test("image-extraction-completeness"))' \
  /tmp/m204-us1.spdx-json.json
# Expected: comment contains "k":"mikebom:image-extraction-completeness","v":"partial"

# SPDX 3
jq '."@graph"[] | select(.type == "Annotation") | select(.statement | test("image-extraction-completeness"))' \
  /tmp/m204-us1.spdx-3-json.json
# Expected: statement carries "mikebom:image-extraction-completeness" = "partial"
```

**Expected post-m204**: all three formats carry the annotation with value `"partial"`.

## Reproducer 2 — Verify US2 (rendered helm scan → `"full"`)

```bash
# Requires real helm binary. Reuses the m204-chart from Reproducer 1.
for fmt in cyclonedx-json spdx-json spdx-3-json; do
  mikebom --offline sbom scan --helm-render --path /tmp/m204-chart \
    --format "$fmt" --output "/tmp/m204-us2.$fmt.json" --no-deep-hash
done

jq '.metadata.properties[] | select(.name == "mikebom:image-extraction-completeness")' \
  /tmp/m204-us2.cyclonedx-json.json
# Expected: value "full"
```

**Expected post-m204**: all three formats carry the annotation with value `"full"`. Value differs from Reproducer 1 iff `helm template` succeeded — otherwise the m203 fallback keeps the value as `"partial"`.

## Reproducer 3 — Verify US3 (non-Helm scan → annotation absent)

```bash
mkdir -p /tmp/m204-nonhelm
echo "hello" > /tmp/m204-nonhelm/readme.txt

for fmt in cyclonedx-json spdx-json spdx-3-json; do
  mikebom --offline sbom scan --path /tmp/m204-nonhelm \
    --format "$fmt" --output "/tmp/m204-us3.$fmt.json" --no-deep-hash
  grep -c "image-extraction-completeness" "/tmp/m204-us3.$fmt.json" || echo "  (0 matches — annotation absent as expected)"
done
```

**Expected post-m204**: `grep -c` returns 0 for every format. Non-Helm scans preserve byte-identity per FR-004 / SC-004.

## Reproducer 4 — Verify parity C123 automatic three-format check

```bash
# The m071 parity test suite exercises every registered
# ParityExtractor row against synthesized 3-format outputs.
# Once C123 lands, the row runs automatically.
cargo +stable test --test parity_synthetic_drift --no-fail-fast 2>&1 | tail -5
cargo +stable test --test holistic_parity --no-fail-fast 2>&1 | tail -5
```

**Expected post-m204**: both test binaries green. If either fails with a `C123` mention, the wire-string mapping (`HelmExtractionMode::as_wire_str`) is diverging between the three per-format extractors — regenerate the trio and check that they produce byte-identical values.

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.

Note: helm-scan goldens will likely need regeneration (they'll gain a new `mikebom:image-extraction-completeness` = `"partial"` entry). Regen via the fixture-specific `MIKEBOM_UPDATE_*` env vars mentioned in `docs/dev/regen-goldens.md`. Golden diff MUST be limited to helm goldens; non-Helm goldens are byte-identical.

## Empirical re-verification at implement time (m199-m202 lesson)

Per `feedback_verify_research_empirical_claims` memory: before finalizing tasks.md, re-run:

```bash
# Verify ScanResult, ScanArtifacts, and destructure sites are still at the line
# numbers cited in plan.md / data-model.md — refactors happen.
grep -n "pub go_workspace_mode\|pub go_transitive_coverage" mikebom-cli/src/scan_fs/mod.rs
grep -n "pub go_workspace_mode:" mikebom-cli/src/generate/mod.rs
grep -n "go_workspace_mode: go_workspace_mode" mikebom-cli/src/cli/scan_cmd.rs
grep -n "go_workspace_mode" mikebom-cli/src/generate/cyclonedx/metadata.rs | head
grep -n "go-workspace-mode\|C112" mikebom-cli/src/parity/extractors/mod.rs

# Verify all 7 build_metadata test callsites are still the shape data-model.md E4 assumes.
grep -c "build_metadata(" mikebom-cli/src/generate/cyclonedx/metadata.rs
```

**Expected**: line numbers within ±10 of the plan's cited references, and build_metadata call-count matches ~7 (test invocations) + 1 (production callsite in `builder.rs`) = 8-ish. Any drift means updating the tasks.md instructions before implementing.
