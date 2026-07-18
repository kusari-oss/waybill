# Quickstart: `--no-deps-dev` Aggregate-Disable Fix

**Date**: 2026-07-17
**Audience**: mikebom maintainer implementing or reviewing m207.

## Prerequisites

- Working mikebom checkout on branch `207-no-deps-dev-aggregate`.
- `cargo +stable` toolchain (workspace default).
- Reproducer 1 (SC-001) requires network access for deps.dev lookups OR uses `--offline` to demonstrate the symmetric no-op.

## Reproducer 1 — SC-001 reporter's exact invocation

```bash
# Use any small project — a synthetic cargo tempdir or an existing fixture.
mikebom sbom scan \
  --warm-go-cache=per-workspace \
  --no-deps-dev \
  --no-clearly-defined \
  --path /tmp/some-project \
  --format cyclonedx-json \
  --output /tmp/out.cdx.json

# Post-m207 assertion: zero deps.dev provenance in the emitted SBOM.
jq '[.components[]? | .properties[]? | select(.name == "mikebom:source-files") | .value | select(. == "[\"deps.dev\"]")] | length' /tmp/out.cdx.json
```

**Expected post-m207**: `0`. The `--no-deps-dev` flag now disables BOTH the license lookup and the dep-graph enrichment paths.

**Also verify stderr**:

```bash
mikebom sbom scan --no-deps-dev --path /tmp/some-project 2>&1 | grep "m207 aggregate semantic"
```

**Expected post-m207**: one INFO log line matching. Fires exactly once per scan.

## Reproducer 2 — US2 fine-grained flags still work

```bash
# Skip only the license path; keep the dep-graph.
mikebom sbom scan --no-deps-dev-license --path /tmp/some-project \
  --format cyclonedx-json --output /tmp/license-off.cdx.json

# Skip only the graph path; keep the license.
mikebom sbom scan --no-deps-dev-graph --path /tmp/some-project \
  --format cyclonedx-json --output /tmp/graph-off.cdx.json
```

**Expected post-m207**:
- `license-off.cdx.json`: components with `mikebom:source-files: ["deps.dev"]` MAY appear (dep-graph enrichment ran) but component license fields will NOT carry deps.dev provenance.
- `graph-off.cdx.json`: components with `mikebom:source-files: ["deps.dev"]` do NOT appear (dep-graph enrichment skipped) but license fields MAY carry deps.dev provenance.

## Reproducer 3 — FR-004 `--enrich-sources` allowlist wins

```bash
# Allowlist mode: --no-deps-dev is IGNORED per FR-004.
mikebom sbom scan \
  --enrich-sources deps-dev,clearly-defined \
  --no-deps-dev \
  --path /tmp/some-project \
  --format cyclonedx-json --output /tmp/allowlist.cdx.json

# Post-m207: allowlist takes precedence; deps.dev license enrichment DID run
# despite --no-deps-dev being present.
jq '.components[]? | select(.licenses != null) | .licenses' /tmp/allowlist.cdx.json | head
```

**Expected post-m207**: deps.dev-sourced license data present. `--no-deps-dev` is silently ignored in allowlist mode per FR-004.

## Reproducer 4 — SC-005 wall-clock delta

```bash
# Baseline: pre-m207 --no-deps-dev (network-fetch STILL happens for graph).
time mikebom-alpha63 sbom scan --no-deps-dev --path /tmp/large-project

# Post-m207: same invocation now skips graph fetch too.
time mikebom sbom scan --no-deps-dev --path /tmp/large-project
```

**Expected post-m207**: same-or-faster wall-clock. Skipping the dep-graph fetch is strictly less work than pre-m207's behavior for the same flag.

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.

## Empirical re-verification at implement time (m199-m206 lesson)

Per `feedback_verify_research_empirical_claims` memory: before finalizing tasks.md, re-run:

```bash
# Confirm the line-numbers cited in plan.md / data-model.md are still valid.
grep -n "pub no_deps_dev:\|pub no_deps_dev_graph:\|fn resolve_enrich_sources\|deps_dev_graph: !args.no_deps_dev_graph" mikebom-cli/src/cli/scan_cmd.rs | head
```

**Expected**: `pub no_deps_dev:` near line 599; `pub no_deps_dev_graph:` near line 636; `fn resolve_enrich_sources` near line 1631; `deps_dev_graph: !args.no_deps_dev_graph,` on line 1642 (this is the exact one-line change m207 makes).

If any drift → update tasks.md instructions accordingly.
