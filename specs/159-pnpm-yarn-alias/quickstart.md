# Quickstart: Milestone 159 verification

**Milestone 159** • End-to-end scenarios that verify SC-001 through SC-011.

## Prerequisites

- Built `mikebom` binary from the `159-pnpm-yarn-alias` branch (`cargo +stable build --release -p mikebom`).
- `jq` for parsing emitted SBOMs.
- Local clones of the 3 `kusari-sandbox/test-*` repos with confirmed alias syntax at `/tmp/kusari-audit/` (from the milestone-157 Round-2 audit setup).

## Scenario 1 — Pnpm alias resolution on test-podman-desktop (SC-001)

**Purpose**: Empirical verification of the 6 known dropped pnpm-alias edges.

```bash
./target/release/mikebom --offline sbom scan \
    --path /tmp/kusari-audit/test-podman-desktop \
    --no-deep-hash \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/159-test-podman-desktop.cdx.json

# Spot-check 1: @docusaurus/core@3.10.1 dependsOn includes @slorber/react-helmet-async
jq -e '.dependencies[] | select(.ref == "pkg:npm/%40docusaurus/core@3.10.1") | .dependsOn | any(. == "pkg:npm/%40slorber/react-helmet-async@1.3.0")' \
    /tmp/159-test-podman-desktop.cdx.json

# Spot-check 2: @docusaurus/core@3.10.1 dependsOn includes @docusaurus/react-loadable
jq -e '.dependencies[] | select(.ref == "pkg:npm/%40docusaurus/core@3.10.1") | .dependsOn | any(. == "pkg:npm/%40docusaurus/react-loadable@6.0.0")' \
    /tmp/159-test-podman-desktop.cdx.json

# Spot-check 3: @isaacs/cliui@8.0.2 dependsOn includes string-width@4.2.3 (via string-width-cjs alias)
jq -e '.dependencies[] | select(.ref == "pkg:npm/%40isaacs/cliui@8.0.2") | .dependsOn | any(. == "pkg:npm/string-width@4.2.3")' \
    /tmp/159-test-podman-desktop.cdx.json

# Verify local-name PURLs are NOT present as components (FR-003)
jq -e '.components | all(.purl != "pkg:npm/react-helmet-async@")' \
    /tmp/159-test-podman-desktop.cdx.json
jq -e '.components | all(.purl != "pkg:npm/string-width-cjs@4.2.3")' \
    /tmp/159-test-podman-desktop.cdx.json
```

**Expected**: All jq `-e` checks return true.

## Scenario 2 — Yarn v1 alias on test-guac-visualizer (SC-002)

**Purpose**: Empirical verification of the 1 known yarn-alias edge (`@cosmograph/cosmos → @cosmos.gl/graph`).

```bash
./target/release/mikebom sbom scan \
    --path /tmp/kusari-audit/test-guac-visualizer \
    --no-deep-hash --format cyclonedx-json \
    --output cyclonedx-json=/tmp/159-test-guac.cdx.json

# The aliased-canonical component MUST exist
jq -e '.components | any(.purl == "pkg:npm/%40cosmos.gl/graph@2.6.4")' \
    /tmp/159-test-guac.cdx.json

# The local-name-based PURL MUST NOT exist
jq -e '.components | all(.purl != "pkg:npm/%40cosmograph/cosmos@2.6.4")' \
    /tmp/159-test-guac.cdx.json

# The aliased component carries the mikebom:yarn-alias annotation
jq -e '.components[] | select(.purl == "pkg:npm/%40cosmos.gl/graph@2.6.4") | .properties[]? | select(.name == "mikebom:yarn-alias") | .value == "@cosmograph/cosmos"' \
    /tmp/159-test-guac.cdx.json
```

**Expected**: All jq `-e` checks return true.

## Scenario 3 — Yarn v1 aliases on test-rails (SC-002)

**Purpose**: Empirical verification of the 3 known yarn-alias edges (`string-width-cjs`, `strip-ansi-cjs`, `wrap-ansi-cjs`).

```bash
./target/release/mikebom sbom scan \
    --path /tmp/kusari-audit/test-rails \
    --no-deep-hash --format cyclonedx-json \
    --output cyclonedx-json=/tmp/159-test-rails.cdx.json

# @isaacs/cliui@8.0.2 (or equivalent) dependsOn now includes the aliased canonicals
jq -e '.dependencies[] | select(.ref == "pkg:npm/%40isaacs/cliui@8.0.2") | .dependsOn | any(. == "pkg:npm/string-width@4.2.3")' \
    /tmp/159-test-rails.cdx.json

# Aliased components carry the mikebom:yarn-alias annotation
jq -e '.components[] | select(.purl == "pkg:npm/string-width@4.2.3") | .properties[]? | select(.name == "mikebom:yarn-alias") | .value == "string-width-cjs"' \
    /tmp/159-test-rails.cdx.json
```

**Expected**: All jq `-e` checks return true.

## Scenario 4 — SC-003 dual-side byte-identity on milestone-090 goldens

```bash
# Regenerate the 11 non-alias CDX goldens (should produce zero diff bytes)
./scripts/regen-goldens.sh 2>&1 | tail -3

# Diff against pre-159 goldens (assumes /tmp/159-pre-goldens snapshotted with `git show main:...`)
for eco in apk bazel cargo cmake deb gem golang maven npm pip rpm; do
    for fmt in "cyclonedx:cdx" "spdx-2.3:spdx" "spdx-3:spdx3"; do
        dir=$(echo $fmt | cut -d: -f1); ext=$(echo $fmt | cut -d: -f2)
        new=mikebom-cli/tests/fixtures/golden/${dir}/${eco}.${ext}.json
        old=/tmp/159-pre-goldens/${eco}.${ext}.json
        diff_lines=$(diff "$old" "$new" | grep -cE '^[<>]')
        echo "${eco}.${ext}.json: $diff_lines"
    done
done
```

**Expected**: Every one of the 33 comparisons prints `0` diff lines (byte-identical). Milestone-090 fixtures have no alias syntax per R7 verification, so this is trivially achievable.

## Scenario 5 — SC-004 annotation universal presence on 100% of alias-resolved components

```bash
# On test-podman-desktop: 6 alias-affected components MUST all carry a mikebom:pnpm-alias
jq '[.components[] | select(.purl == "pkg:npm/%40slorber/react-helmet-async@1.3.0" or
                             .purl == "pkg:npm/%40docusaurus/react-loadable@6.0.0" or
                             .purl == "pkg:npm/string-width@4.2.3" or
                             .purl == "pkg:npm/strip-ansi@6.0.1") | .properties[]? | select(.name == "mikebom:pnpm-alias") | .value] | sort | unique' \
    /tmp/159-test-podman-desktop.cdx.json
```

**Expected**: Returns a JSON array containing the 4+ distinct local-names that reached these components (e.g. `["react-helmet-async", "react-loadable", "string-width-cjs", "strip-ansi-cjs"]`).

## Scenario 6 — SC-005 BFS reachability lift on test-podman-desktop

```bash
python3 <<'PY'
import json
with open('/tmp/159-test-podman-desktop.cdx.json') as f: sbom = json.load(f)
root = sbom['metadata']['component']['bom-ref']
edges = {d['ref']: d.get('dependsOn', []) for d in sbom['dependencies']}
npm_refs = {c['bom-ref'] for c in sbom['components'] if c.get('purl','').startswith('pkg:npm/')}
visited = {root}; queue = [root]
while queue:
    n = queue.pop(0)
    for m in edges.get(n, []):
        if m not in visited: visited.add(m); queue.append(m)
reachable = visited & npm_refs
print(f"BFS reachable npm: {len(reachable)} / {len(npm_refs)} ({100*len(reachable)/len(npm_refs):.1f}%)")
PY
```

**Expected**: Reachable npm count ≥708 (milestone-158 baseline was 698; +10 from newly-resolved alias-edges and their transitive closures).

## Scenario 7 — FR-011 alias-resolution info log

```bash
RUST_LOG=info ./target/release/mikebom sbom scan \
    --path /tmp/kusari-audit/test-podman-desktop \
    --no-deep-hash --format cyclonedx-json \
    --output cyclonedx-json=/dev/null 2>&1 | grep 'npm-alias resolution completed'
```

**Expected**: Output includes one line like:

```
INFO mikebom::scan_fs::package_db::npm::pnpm_lock: npm-alias resolution completed lockfile_path=/tmp/kusari-audit/test-podman-desktop/pnpm-lock.yaml alias_count=6 alias_ecosystem=pnpm
```

## Scenario 8 — SC-006 pre-PR gate

```bash
./scripts/pre-pr.sh
```

**Expected**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` both green. Zero errors.
