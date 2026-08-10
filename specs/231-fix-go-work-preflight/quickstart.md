# Quickstart: verifying the go.work preflight fix works

**Feature**: 231-fix-go-work-preflight
**Phase**: 1

Executes SC-001..SC-005 predicates against the finished implementation. All commands assume repository root at `/Users/mlieberman/Projects/mikebom` and a `cargo build --release -p waybill` binary at `target/release/waybill`.

## Setup

```bash
cargo build --release -p waybill
```

## Walkthrough — SC-001 + SC-004: synthetic workspace fixture

```bash
mkdir -p /tmp/231-verify
./target/release/waybill --offline sbom scan \
  --path waybill-cli/tests/fixtures/golden_inputs/golang/workspace_mode \
  --format cyclonedx-json \
  --output /tmp/231-verify/workspace.cdx.json \
  --no-deep-hash 2> /tmp/231-verify/scan.stderr
```

**SC-001** — no `go-mod-why analysis skipped` warnings:

```bash
grep -c "go-mod-why analysis skipped" /tmp/231-verify/scan.stderr
# Expect: 0
```

**SC-001** — at least one Go component with a non-`unknown` build-inclusion:

```bash
jq -r '
  [.components[] | select(.purl // "" | startswith("pkg:golang/"))
   | (.properties // []) | map(select(.name == "waybill:build-inclusion") | .value)]
  | add
  | select(. != "unknown")
' /tmp/231-verify/workspace.cdx.json
# Expect: at least one non-empty output line ("prod" or "test" or "not-needed")
```

**SC-004** — `analyzed >= 1` in the diagnostic summary:

```bash
grep "go-mod-why classification:" /tmp/231-verify/scan.stderr | grep -oE "analyzed=[0-9]+"
# Expect: analyzed=<N> where N >= 1
```

## Walkthrough — SC-002: Grafana reference (manual verification)

Requires a local clone of `github.com/grafana/grafana`. This is a one-shot manual step; not automated.

```bash
GRAFANA_PATH="$HOME/Projects/grafana"  # adjust to your clone path
./target/release/waybill --offline sbom scan \
  --path "$GRAFANA_PATH" \
  --format cyclonedx-json \
  --output /tmp/231-verify/grafana.cdx.json \
  --no-deep-hash 2> /tmp/231-verify/grafana.stderr
```

Check the diagnostic:

```bash
grep "build-inclusion pass:" /tmp/231-verify/grafana.stderr | grep -oE "marked=[0-9]+"
# Pre-m231 baseline: marked=469
# Post-m231 target:  marked=0 (small residual OK — see spec Assumptions)
```

## Walkthrough — SC-003: single-module Go project byte parity

Requires a single-module Go project — a small stdlib-only project without a `go.work` file works. Alternatively use waybill's own repo (`~/Projects/mikebom`) if a `go.work` isn't present at scan-root's parent chain.

Pre-m231 comparison:

```bash
# Baseline: scan with the pre-m231 binary (m230 tip is at commit f776a43)
git checkout f776a43 -- waybill-cli/src/scan_fs/package_db/golang/mod_why.rs
cargo build --release -p waybill
./target/release/waybill --offline sbom scan \
  --path <single-module-go-project> \
  --format cyclonedx-json \
  --output /tmp/231-verify/single.pre.cdx.json \
  --no-deep-hash
git checkout HEAD -- waybill-cli/src/scan_fs/package_db/golang/mod_why.rs
cargo build --release -p waybill
./target/release/waybill --offline sbom scan \
  --path <single-module-go-project> \
  --format cyclonedx-json \
  --output /tmp/231-verify/single.post.cdx.json \
  --no-deep-hash
```

Byte-parity check (masking timestamps + serial numbers + content-addressed IDs per memory `feedback_verify_golden_churn_normalized`):

```bash
mask() {
  sed -E 's/"timestamp": "[^"]+"/"timestamp": "MASKED"/g
          s/"serialNumber": "[^"]+"/"serialNumber": "MASKED"/g
          s/"bom-ref": "[a-f0-9]{16,}"/"bom-ref": "MASKED"/g' "$1" | LC_ALL=C sort
}
diff <(mask /tmp/231-verify/single.pre.cdx.json) <(mask /tmp/231-verify/single.post.cdx.json)
# Expect: empty diff on the waybill:build-inclusion annotations for Go components.
```

## Walkthrough — SC-005: `GOWORK=off` override

Two scans of the synthetic fixture — one default, one with the operator override:

```bash
# Default (workspace mode detected)
./target/release/waybill --offline sbom scan \
  --path waybill-cli/tests/fixtures/golden_inputs/golang/workspace_mode \
  --format cyclonedx-json \
  --output /tmp/231-verify/default.cdx.json \
  --no-deep-hash 2> /tmp/231-verify/default.stderr

# With GOWORK=off (explicit override)
GOWORK=off ./target/release/waybill --offline sbom scan \
  --path waybill-cli/tests/fixtures/golden_inputs/golang/workspace_mode \
  --format cyclonedx-json \
  --output /tmp/231-verify/gowork-off.cdx.json \
  --no-deep-hash 2> /tmp/231-verify/gowork-off.stderr
```

Assert the override took effect:

```bash
# Default: workspace_modules > 0 in the classification summary
grep "workspace_modules=" /tmp/231-verify/default.stderr | head -1
# Expect: workspace_modules=2 (both modules detected)

# Override: workspace_modules = 0 (detection returns Off for all modules)
grep "workspace_modules=" /tmp/231-verify/gowork-off.stderr | head -1
# Expect: workspace_modules=0

# With GOWORK=off, the pre-m231 failure mode returns — go-mod-why analysis
# skipped shows up because go list all fails with -mod=mod against the
# workspace. This is INTENTIONAL: the operator asked for it.
grep -c "go-mod-why analysis skipped" /tmp/231-verify/gowork-off.stderr
# Expect: >= 1 (WARN fired because GOWORK=off + go.work present is a broken
# combination Go itself rejects. The operator's explicit choice is respected;
# waybill emits the WARN and moves on.)
```

## Post-implementation checklist

- [ ] SC-001: no `go-mod-why analysis skipped` warnings on the synthetic fixture; ≥1 Go component has non-`unknown` build-inclusion.
- [ ] SC-002: Grafana `marked=` count drops from 469 to ≤ 5 (per spec Assumptions — small residual OK).
- [ ] SC-003: single-module Go project scan produces byte-identical Go build-inclusion output pre/post 231.
- [ ] SC-004: `analyzed=` counter is ≥ 1 on every workspace-mode scan.
- [ ] SC-005: `GOWORK=off` override returns the reader to pre-231 behavior for workspace projects.
- [ ] Pre-PR gate: `./scripts/pre-pr.sh` exits 0.
