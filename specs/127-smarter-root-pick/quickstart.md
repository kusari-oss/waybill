# Quickstart — Smarter root component selection

Three repro recipes operators / reviewers can run end-to-end to verify the feature behavior.

## SC-001 — Multi-module Go workspace (otel-collector)

```sh
cd /tmp && rm -rf otel-collector
git clone --depth 1 --branch v0.105.0 https://github.com/open-telemetry/opentelemetry-collector.git
cd opentelemetry-collector
mikebom sbom scan --path . --format spdx-2.3-json --output /tmp/otel.spdx.json
```

### Expected post-feature

```sh
jq -r '.documentDescribes[0] as $r | .packages[] | select(.SPDXID == $r) | {name, versionInfo, purl: (.externalRefs[] | select(.referenceType == "purl") | .referenceLocator)}' /tmp/otel.spdx.json
```

```json
{
  "name": "go.opentelemetry.io/collector",
  "versionInfo": "v0.105.0",
  "purl": "pkg:golang/go.opentelemetry.io/collector@v0.105.0"
}
```

```sh
jq -r '.annotations[]? | select(.comment | test("root-selection-heuristic")) | .comment' /tmp/otel.spdx.json
```

```json
{"schema":"mikebom-annotation/v1","field":"mikebom:root-selection-heuristic","value":{"heuristic":"repo-root-main-module","confidence":0.95}}
```

### Pre-feature (regression check)

```sh
{name: "go.opentelemetry.io/collector/confmap/provider/httpsprovider", versionInfo: "v0.0.0-unknown", purl: "pkg:golang/go.opentelemetry.io/collector/confmap/provider/httpsprovider@v0.0.0-unknown"}
```

## SC-002 — Polyglot Go-vs-Maven-vs-npm (argo-workflows)

```sh
cd /tmp && rm -rf argo-workflows
git clone --depth 1 --branch v3.5.5 https://github.com/argoproj/argo-workflows.git
cd argo-workflows
mikebom sbom scan --path . --format spdx-2.3-json --output /tmp/argo.spdx.json
```

### Expected post-feature

```json
{
  "name": "github.com/argoproj/argo-workflows/v3",
  "versionInfo": "v3.5.5",
  "purl": "pkg:golang/github.com/argoproj/argo-workflows/v3@v3.5.5"
}
```

The annotation will fire IF there's still ambiguity at the repo root (the Maven `scan_target_coord` was deduplicated by FR-012, but if the Go reader was the only `is_workspace_root` main-module, the count==1 fast path will fire and NO annotation will be emitted — same byte-identity preservation as today's correct-case projects).

If multiple ecosystems all claim the repo root (rare in practice), the annotation looks like:

```json
{"schema":"mikebom-annotation/v1","field":"mikebom:root-selection-heuristic","value":{"heuristic":"ecosystem-priority","confidence":0.70}}
```

AND `stderr` carries:

```text
WARN  mikebom::generate::root_selector: root-component selected via "ecosystem-priority" heuristic (confidence 0.70); operator override recommended for deterministic identity
  selected = pkg:golang/github.com/argoproj/argo-workflows/v3@v3.5.5
  losers = [pkg:maven/io.argoproj.workflow/argo-client-java-tests@0.0.0-VERSION, pkg:npm/argo-workflows@latest, pkg:npm/argo-workflows-ui@1.0.0]
  hint = "pass --root-name and --root-purl-type to override"
```

## SC-003 — Zero regression on the 33 alpha.48 goldens

```sh
cd /Users/mlieberman/Projects/mikebom
cargo +stable test --workspace --test cdx_regression --test spdx_regression --test spdx3_regression
```

Expected: all 11 + 11 + 11 = 33 byte-identity tests pass with no `MIKEBOM_UPDATE_*` env var. The feature MUST be a no-op for every single-main-module project. If any golden churns, the count==1 fast path is broken — debug before merging.

For the broader regen sweep (per the milestone-126 widening), use the wrapper:

```sh
./scripts/regen-goldens.sh
git status --short
# Expected: no churn. If git status shows changes, the fast path regression check failed.
```

## SC-006 — Operator override still wins

```sh
cd /tmp/argo-workflows
mikebom sbom scan --path . --format spdx-2.3-json --output /tmp/argo-override.spdx.json \
  --root-name "argo-workflows-overridden" \
  --root-purl-type "generic" \
  --root-version "3.5.5"

jq -r '.documentDescribes[0] as $r | .packages[] | select(.SPDXID == $r) | .externalRefs[] | select(.referenceType == "purl") | .referenceLocator' /tmp/argo-override.spdx.json
```

Expected: `pkg:generic/argo-workflows-overridden@3.5.5`. No `mikebom:root-selection-heuristic` annotation (override audit channel handles override case per FR-006).

## Local integration-test recipe

```sh
cd /Users/mlieberman/Projects/mikebom

# Build with the feature branch checked out
cargo +stable build --workspace

# Run the new integration tests
cargo +stable test --workspace \
  --test root_selection_us1_multi_module_workspace \
  --test root_selection_us2_polyglot \
  --test root_selection_us3_heuristic_annotation \
  --test root_selection_byte_identity

# Pre-PR gate (mandatory per CLAUDE.md)
./scripts/pre-pr.sh
```

The pre-PR gate runs the full workspace under stable clippy + test. The byte-identity test target (`root_selection_byte_identity`) plus the existing cdx_regression / spdx_regression / spdx3_regression suites catch any unintended churn.
