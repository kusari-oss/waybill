# Quickstart: Milestone 158 verification

**Milestone 158** • End-to-end scenarios that verify the SC-001 through SC-011 outcomes.

## Prerequisites

- Built `mikebom` binary from the `158-graph-completeness` branch (`cargo +stable build --release -p mikebom`).
- `jq` for parsing emitted SBOMs.
- Local clones of the 5 `kusari-sandbox/test-*` repos under `/tmp/kusari-audit/` (matching the milestone-157 audit setup).

## Scenario 1 — test-podman-desktop reaches 100% (SC-001)

**Purpose**: Empirical verification of the primary bug fix.

```bash
./target/release/mikebom --offline sbom scan \
    --path /tmp/kusari-audit/test-podman-desktop \
    --no-deep-hash \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/158-test-podman-desktop.cdx.json

# Compute BFS reachability from root
python3 <<'PY'
import json
with open('/tmp/158-test-podman-desktop.cdx.json') as f: sbom = json.load(f)
root = sbom['metadata']['component']['bom-ref']
edges = {d['ref']: d.get('dependsOn', []) for d in sbom['dependencies']}
npm_refs = {c['bom-ref'] for c in sbom['components'] if c.get('purl','').startswith('pkg:npm/')}
visited = {root}
queue = [root]
while queue:
    n = queue.pop(0)
    for m in edges.get(n, []):
        if m not in visited: visited.add(m); queue.append(m)
reachable_npm = visited & npm_refs
print(f"Reachable npm components: {len(reachable_npm)} / {len(npm_refs)} = {100*len(reachable_npm)/len(npm_refs):.2f}%")
PY

# Read the completeness annotation
jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness") | .value' \
    /tmp/158-test-podman-desktop.cdx.json
```

**Expected**:

- Reachable count: ≥99% of npm components (target: 100% — pre-158 baseline was 19.5%).
- Annotation value: `complete` (workspace peers linked, no orphans).

## Scenario 2 — test-guac-visualizer emits complete (SC-004)

```bash
./target/release/mikebom sbom scan --path /tmp/kusari-audit/test-guac-visualizer \
    --no-deep-hash --format cyclonedx-json \
    --output cyclonedx-json=/tmp/158-test-guac.cdx.json

jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness") | .value' \
    /tmp/158-test-guac.cdx.json
```

**Expected**: `complete` (no workspace, no orphans, single-ecosystem).

## Scenario 3 — test-rails emits `partial` with combined reason (SC-004 + Q2 + Q3)

```bash
./target/release/mikebom sbom scan --path /tmp/kusari-audit/test-rails \
    --no-deep-hash --format cyclonedx-json \
    --output cyclonedx-json=/tmp/158-test-rails.cdx.json

jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness") | .value' \
    /tmp/158-test-rails.cdx.json
jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness-reason") | .value' \
    /tmp/158-test-rails.cdx.json
```

**Expected**:

- Value: `partial`.
- Reason: contains at least `orphaned-components-detected: <N>` due to the `Issue #256: nameless secondary package.json` warning that fires today; MAY also contain `multi-ecosystem-partial-root: npm` if npm root detection can't confidently pick a workspace root.

## Scenario 4 — SC-002 dual-side byte-identity on milestone-090 goldens

```bash
# Regenerate all 11 non-workspace goldens
for ecosystem in alpine apk cargo cyclonedx-source deb gem maven npm pip rpm spdx-source; do
    ./scripts/regenerate-golden.sh "$ecosystem"
done

# Diff against pre-158 goldens (assumes /tmp/158-pre-goldens snapshotted with `git show main:...`)
for ecosystem in alpine apk cargo cyclonedx-source deb gem maven npm pip rpm spdx-source; do
    diff /tmp/158-pre-goldens/$ecosystem.cdx.json \
         mikebom-cli/tests/fixtures/golden/cyclonedx/$ecosystem.cdx.json \
         | grep -E '^[<>]' | wc -l
done
```

**Expected**: For each ecosystem, exactly 2 diff lines (one `<`, one `>`) representing the added `mikebom:graph-completeness = complete` property. Zero other differences (SC-002).

## Scenario 5 — All three formats carry the annotation (SC-003 + FR-007)

```bash
./target/release/mikebom sbom scan --path /tmp/kusari-audit/test-guac-visualizer \
    --no-deep-hash \
    --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
    --output cyclonedx-json=/tmp/158-guac.cdx.json,spdx-2.3-json=/tmp/158-guac.spdx.json,spdx-3-json=/tmp/158-guac.spdx3.json

# CDX
jq -e '.metadata.properties | any(.name == "mikebom:graph-completeness")' /tmp/158-guac.cdx.json

# SPDX 2.3
jq -e '.annotations | any(.comment | startswith("mikebom:graph-completeness="))' /tmp/158-guac.spdx.json

# SPDX 3
jq -e '."@graph" | any(.type == "Annotation" and (.statement | startswith("mikebom:graph-completeness=")))' /tmp/158-guac.spdx3.json
```

**Expected**: All 3 jq expressions return `true`. Milestone-071 parity check (`cargo test parity_symmetric`) validates the emission for symmetric ONE-of-each shape.

## Scenario 6 — Consumer jq recipe (from R9)

```bash
completeness=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness") | .value' \
    /tmp/158-test-podman-desktop.cdx.json)
case "$completeness" in
    complete)
        echo "Graph is fully connected — safe to consume"
        ;;
    partial)
        reason=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness-reason") | .value' \
            /tmp/158-test-podman-desktop.cdx.json)
        echo "Partial graph: $reason"
        ;;
    unknown)
        reason=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness-reason") | .value' \
            /tmp/158-test-podman-desktop.cdx.json)
        echo "Unknown completeness: $reason (recommend re-scan or manual review)"
        exit 1
        ;;
esac
```

**Expected**: On test-podman-desktop, prints `Graph is fully connected — safe to consume`.

## Scenario 7 — FR-013 log line

```bash
RUST_LOG=info ./target/release/mikebom sbom scan --path /tmp/kusari-audit/test-podman-desktop \
    --no-deep-hash --format cyclonedx-json \
    --output cyclonedx-json=/dev/null 2>&1 | grep 'graph completeness computed'
```

**Expected**: One line like:

```
INFO mikebom::cli::scan_cmd: graph completeness computed value=complete reachable_count=2835 total_count=2835 orphan_count=0 reason_codes=[]
```

## SC-006 pre-PR gate

```bash
./scripts/pre-pr.sh
```

**Expected**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` both green. Zero errors.
