# Quickstart: m194 Close Remaining Orphan Gaps

**Date**: 2026-07-14
**Audience**: Reviewer verifying m194 against the Kusari pico corpus + operators running the reproducer locally.

## Prerequisites

- mikebom binary at or after m194 (`cargo build --release -p mikebom`)
- `jq`
- Optional: local checkouts of pico corpus repos (regenerate.sh paths)

## Reproducer 1 — Synthetic Go source repo (US1)

```bash
mkdir -p /tmp/m194-go && cd /tmp/m194-go
cat > go.mod <<'EOF'
module example.com/m194

go 1.22

require github.com/spf13/cobra v1.8.0
EOF
cat > main.go <<'EOF'
package main
import "github.com/spf13/cobra"
func main() { _ = cobra.Command{} }
EOF

mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m194-go --output /tmp/m194-go.cdx.json
```

### Assertion 1 — Graph completeness reports `complete`

```bash
jq -r '.metadata.properties[] | select(.name=="mikebom:graph-completeness") | .value' /tmp/m194-go.cdx.json
```
**Expected**: `complete`
**Pre-m194**: `partial`

### Assertion 2 — Go mainmod has edge to stdlib

```bash
jq -r '.dependencies[] | select(.ref | test("example.com/m194")) | .dependsOn[] | select(test("stdlib"))' /tmp/m194-go.cdx.json
```
**Expected**: `pkg:golang/stdlib@v1.22.0` (or similar version)
**Pre-m194**: no output (stdlib was orphaned)

## Reproducer 2 — Nested nameless npm workspace (US2)

```bash
mkdir -p /tmp/m194-npm/nested && cd /tmp/m194-npm
cat > package.json <<'EOF'
{"name":"@my/pkg","version":"1.0.0","dependencies":{"axios":"1.5.0"}}
EOF
cat > package-lock.json <<'EOF'
{"name":"@my/pkg","version":"1.0.0","lockfileVersion":3,
 "packages":{"":{"name":"@my/pkg","version":"1.0.0",
                 "dependencies":{"axios":"1.5.0"}},
             "node_modules/axios":{"version":"1.5.0"}}}
EOF
cat > nested/package.json <<'EOF'
{"dependencies":{"chalk":"5.0.0"}}
EOF
cat > nested/package-lock.json <<'EOF'
{"name":"nested","lockfileVersion":3,
 "packages":{"":{"dependencies":{"chalk":"5.0.0"}},
             "node_modules/chalk":{"version":"5.0.0"}}}
EOF

mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m194-npm --output /tmp/m194-npm.cdx.json
```

### Assertion 3 — Nested mainmod synthesized

```bash
jq -r '.components[] | select(.name=="nested") | .purl' /tmp/m194-npm.cdx.json
```
**Expected**: `pkg:npm/nested` (versionless)
**Pre-m194**: no such component

### Assertion 4 — Nested mainmod has edge to chalk

```bash
jq -r '.dependencies[] | select(.ref=="pkg:npm/nested") | .dependsOn' /tmp/m194-npm.cdx.json
```
**Expected**: array including `"pkg:npm/chalk@5.0.0"`

### Assertion 5 — Graph completeness `complete`

```bash
jq -r '.metadata.properties[] | select(.name=="mikebom:graph-completeness") | .value' /tmp/m194-npm.cdx.json
```
**Expected**: `complete`

## Reproducer 3 — Real Kusari pico corpus

Full end-to-end validation against the reported corpus:

```bash
for pair in "kusari-cli:https://github.com/kusaridev/kusari-cli.git:c12f150" \
            "pico:https://github.com/kusaridev/pico.git:2c2f9719" \
            "guac:https://github.com/guacsec/guac.git:ebb808e" \
            "molcajete:https://github.com/kusaridev/molcajete.git:0a40304"; do
  name=$(echo "$pair" | cut -d: -f1)
  url=$(echo "$pair" | cut -d: -f2-3)
  ver=$(echo "$pair" | cut -d: -f4)
  rm -rf /tmp/$name-corpus
  git clone "$url" /tmp/$name-corpus
  git -C /tmp/$name-corpus checkout "$ver"
  mikebom sbom scan --offline --format cyclonedx-json \
    --path /tmp/$name-corpus --output /tmp/$name-corpus.cdx.json \
    --root-name "$name" --root-version "$ver"
  echo "=== $name ==="
  jq '.metadata.properties[] | select(.name | test("graph-completeness"))' /tmp/$name-corpus.cdx.json
done
```

### Assertion 6 — All 4 report `complete`

Every entry should show `mikebom:graph-completeness: complete` and no `graph-completeness-reason`. Matches SC-005.

## CI verification recap

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace` MUST pass clean.

New test file:
- `mikebom-cli/tests/graph_completeness_operator_root.rs` — extended with 2 new US1 + US2 integration tests

New unit tests:
- `golang/legacy.rs::tests` — 2-3 tests for stdlib edge emission
- `npm/mod.rs::tests` — 3-4 tests for nested nameless mainmod synthesis
