# Quickstart: m192 Operator-Root Completeness Fix Verification

**Date**: 2026-07-14
**Audience**: Developer implementing or reviewing m192; operator verifying the fix against a real source repo.

## Purpose

Reproduces the graph-completeness `partial` false-positive on operator-supplied roots (the Kusari pico regression) and verifies the m192 fix flips them to `complete`.

## Prerequisites

- mikebom binary at or after m192 (`cargo build --release -p mikebom-cli`, tagged post-merge)
- `jq` for JSON inspection
- Optional: a local checkout of one of the Kusari pico test-corpus repos (pico, kusari-cli, guac, molcajete) for real-world validation

## Reproducer 1 — Synthetic Go source repo

Build a minimal Go module + scan with `--root-name`:

```bash
mkdir -p /tmp/m192-reprogo && cd /tmp/m192-reprogo
cat > go.mod <<'EOF'
module example.com/m192

go 1.22

require github.com/spf13/cobra v1.8.0
EOF
cat > main.go <<'EOF'
package main

import "github.com/spf13/cobra"

func main() { _ = cobra.Command{} }
EOF

mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m192-reprogo \
  --root-name example-service --root-version abc123 \
  --output /tmp/m192-reprogo.cdx.json
```

### Assertion 1 — `graph-completeness: complete` on the operator-override path

```bash
jq -r '.metadata.properties[] | select(.name=="mikebom:graph-completeness") | .value' /tmp/m192-reprogo.cdx.json
```

**Expected**: `complete`
**Pre-m192 (broken)**: `partial`

Also verify no reason annotation:

```bash
jq -r '.metadata.properties[]? | select(.name=="mikebom:graph-completeness-reason") | .value' /tmp/m192-reprogo.cdx.json
```

**Expected**: no output (annotation absent).
**Pre-m192 (broken)**: `multi-ecosystem-partial-root: golang`

## Reproducer 2 — `--root-purl-type golang` (Q2 answer A path)

```bash
mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m192-reprogo \
  --root-name github.com/example/service --root-version abc123 \
  --root-purl-type golang \
  --output /tmp/m192-reprogo-golang.cdx.json
```

### Assertion 2 — root PURL is golang-typed; still `complete`

```bash
jq -r '.metadata.component.purl' /tmp/m192-reprogo-golang.cdx.json
# Expected: pkg:golang/github.com/example/service@abc123
jq -r '.metadata.properties[] | select(.name=="mikebom:graph-completeness") | .value' /tmp/m192-reprogo-golang.cdx.json
# Expected: complete
```

## Reproducer 3 — Cross-format consistency (FR-006)

```bash
for fmt in cyclonedx-json spdx-2.3-json spdx-3-json; do
  ext=$(echo $fmt | sed -E 's/-json//;s/-/./;s/[.]/./g')
  mikebom sbom scan --offline --format $fmt \
    --path /tmp/m192-reprogo --root-name example-service --root-version abc123 \
    --output /tmp/m192-reprogo.$fmt.json 2>&1 > /dev/null
done

# CDX
jq -r '.metadata.properties[] | select(.name=="mikebom:graph-completeness") | .value' /tmp/m192-reprogo.cyclonedx-json.json

# SPDX 2.3 (grep the mikebom annotation shape)
jq -r '.annotations[]? | select(.comment | test("mikebom:graph-completeness"))' /tmp/m192-reprogo.spdx-2.3-json.json | head

# SPDX 3
jq -r '.["@graph"][]? | select(.type=="Annotation" and (.statement // "" | test("graph-completeness"))) | .statement' /tmp/m192-reprogo.spdx-3-json.json | head
```

**Expected**: all three formats report `complete` for the same input.

## Reproducer 4 — Native-root byte-identity (FR-004 / SC-004)

Scan the same repo WITHOUT `--root-name`:

```bash
mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m192-reprogo --output /tmp/m192-reprogo-native.cdx.json
```

Compare against a pre-m192 baseline SBOM for the same input:

```bash
diff <(jq -S . /tmp/m192-reprogo-native.cdx.json) <(jq -S . /path/to/pre-m192-baseline.cdx.json)
```

**Expected**: no diff. Native-root path is a byte-identity no-op.

## Reproducer 5 — Real gap still surfaces (FR-007)

Manually inject a component with no edges (this test requires a Rust test fixture rather than a shell reproducer — see `mikebom-cli/tests/graph_completeness_operator_root.rs::real_orphan_still_reports_partial`). Assert the emitted `mikebom:graph-completeness-reason` includes `orphaned-components-detected: N component(s) not reachable from root`.

## Reproducer 6 — Real-world Kusari pico corpus

For maximum fidelity against the reported regression, clone one of the pico corpus repos + scan with pico's exact CLI shape:

```bash
git clone https://github.com/kusaridev/pico /tmp/pico-src
cd /tmp/pico-src
git checkout 2c2f9719
mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/pico-src \
  --root-name pico --root-version 2c2f9719 \
  --output /tmp/pico-m192.cdx.json

jq -r '.metadata.properties[] | select(.name=="mikebom:graph-completeness") | .value' /tmp/pico-m192.cdx.json
# Expected: complete (was: partial: multi-ecosystem-partial-root: golang, npm)
```

Repeat for kusari-cli / guac / molcajete (per pico's `regenerate.sh`) to confirm all 4 flip from `partial` → `complete`.

## CI verification recap

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace` MUST pass clean.

New test file:
- `mikebom-cli/tests/graph_completeness_operator_root.rs` — integration tests covering US1 acceptance scenarios 1-5.
- New unit tests in `mikebom-cli/src/generate/graph_completeness/bfs.rs::tests` — Fixture O1/O2/O3/N + real-orphan detection per contracts/classifier-input.md.
