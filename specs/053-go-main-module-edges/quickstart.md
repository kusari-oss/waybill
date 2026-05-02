# Quickstart: milestone 053 (Go main-module edges)

5-minute verification recipe that the milestone closes issue #102 end-to-end. Run after implementation lands; this script doubles as the SC-006 acceptance evidence.

## Prerequisites

- A debug build of the milestone-053 mikebom binary at `target/debug/mikebom` (run `cargo build -p mikebom`).
- `git`, `jq` on `$PATH`.
- Network access for the initial git clone (subsequent steps are offline).

## Steps

### 1. Clone argo-workflows v3.3.9 (the issue-#102 reproduction case)

```sh
mkdir -p /tmp/mikebom-053-verify && cd /tmp/mikebom-053-verify
git clone --depth 1 --branch v3.3.9 https://github.com/argoproj/argo-workflows.git
```

### 2. Run mikebom with an empty GOMODCACHE

The empty cache reproduces a fresh-CI-runner / pre-build state, the exact failure mode of issue #102.

```sh
HOME=$(mktemp -d) GOMODCACHE=$(mktemp -d)/empty \
  /Users/mlieberman/Projects/mikebom/target/debug/mikebom \
    --offline sbom scan \
    --path /tmp/mikebom-053-verify/argo-workflows \
    --format spdx-2.3-json \
    --output /tmp/mikebom-053-verify/argo.spdx.json \
    --no-deep-hash
```

Pre-053 the resulting SBOM had **1 relationship** (just the document `DESCRIBES`) and zero `DEPENDS_ON` edges. Post-053 expectation:

### 3. Inspect SPDX 2.3 output

```sh
jq '{
  total_rels: (.relationships | length),
  total_pkgs: (.packages | length),
  depends_on: (.relationships | map(select(.relationshipType == "DEPENDS_ON")) | length),
  main_module_pkg: [.packages[] | select(.primaryPackagePurpose == "APPLICATION") | .name][0],
  document_describes: .documentDescribes,
  main_module_role: [.packages[] | select(.primaryPackagePurpose == "APPLICATION") | .annotations[].comment | fromjson | select(.field == "mikebom:component-role").value][0]
}' /tmp/mikebom-053-verify/argo.spdx.json
```

**Expected output**:

```json
{
  "total_rels": 16,
  "total_pkgs": 295,
  "depends_on": 14,
  "main_module_pkg": "github.com/argoproj/argo-workflows",
  "document_describes": ["SPDXRef-Package-XXXXXXXXXXXXXXXX"],
  "main_module_role": "main-module"
}
```

(Exact counts may shift slightly with future argo-workflows tag updates; the invariants are: ≥14 `DEPENDS_ON`, exactly one main-module package with `primaryPackagePurpose: APPLICATION`, `document_describes` contains its SPDXID, and the `mikebom:component-role: main-module` annotation is attached.)

### 4. Inspect CycloneDX output

```sh
HOME=$(mktemp -d) GOMODCACHE=$(mktemp -d)/empty \
  /Users/mlieberman/Projects/mikebom/target/debug/mikebom \
    --offline sbom scan \
    --path /tmp/mikebom-053-verify/argo-workflows \
    --format cyclonedx-json \
    --output /tmp/mikebom-053-verify/argo.cdx.json \
    --no-deep-hash

jq '{
  metadata_component: .metadata.component | {name, type, purl},
  metadata_component_role: [.metadata.component.properties[] | select(.name == "mikebom:component-role") | .value][0],
  main_in_components: [.components[] | select(.purl == .metadata.component.purl)] | length,
  direct_edges: [.dependencies[] | select(.ref == .metadata.component."bom-ref") | .dependsOn] | flatten | length
}' /tmp/mikebom-053-verify/argo.cdx.json
```

**Expected output**:

```json
{
  "metadata_component": {
    "name": "github.com/argoproj/argo-workflows",
    "type": "application",
    "purl": "pkg:golang/github.com/argoproj/argo-workflows@v3.3.9"
  },
  "metadata_component_role": "main-module",
  "main_in_components": 0,
  "direct_edges": 14
}
```

Key invariants:
- `metadata.component.type == "application"` (Trivy-aligned, native CDX).
- `metadata.component.purl` is the real Go module path with the resolved version (not `pkg:generic/...`).
- `main_in_components == 0`: the main-module is NOT duplicated in `components[]`.
- `direct_edges` equals the count of direct requires in `argo-workflows`'s `go.mod` (≥14 in v3.3.9).

### 5. Optional: scan against an empty-cache tarball-style fixture for fully-deterministic output

```sh
# Strip the .git dir to force step 3 of the version ladder
rm -rf /tmp/mikebom-053-verify/argo-workflows/.git

HOME=$(mktemp -d) GOMODCACHE=$(mktemp -d)/empty \
  /Users/mlieberman/Projects/mikebom/target/debug/mikebom \
    --offline sbom scan \
    --path /tmp/mikebom-053-verify/argo-workflows \
    --format spdx-2.3-json \
    --output /tmp/mikebom-053-verify/argo-no-git.spdx.json \
    --no-deep-hash

jq '[.packages[] | select(.primaryPackagePurpose == "APPLICATION") | .versionInfo][0]' \
  /tmp/mikebom-053-verify/argo-no-git.spdx.json
```

**Expected**: `"v0.0.0-unknown"` (step 3 of the version ladder fires when `.git` is absent — confirms cross-host golden determinism).

## Smoke-test as a regression guard

The milestone-053 implementation MUST include a corresponding integration test using a tarball-style trimmed fixture (`tests/fixtures/go/argo-style-no-cache/`) that mirrors steps 2–4 above with the no-`.git` invariant baked in for byte-identical goldens. The argo-workflows live-clone variant is reviewer-facing only; CI uses the trimmed fixture.

## Failure modes

If step 3 outputs `"depends_on": 0` and `"document_describes": ["SPDXRef-DocumentRoot-..."]`, the implementation has regressed to pre-053 behavior. Possible causes:

- The Go reader's `build_main_module_entry()` is not being called from `read()` — check the `parsed_roots` loop at `golang.rs:675+`.
- The new entry has `parent_purl: Some(...)` instead of `None` — the SPDX root-selection algorithm requires `parent_purl: None` for top-level qualification.
- The CDX builder is emitting the main-module both in `metadata.component` AND in `components[]` — check the components-builder filter at `generate/cyclonedx/builder.rs`.
- `primaryPackagePurpose` field absent on the SPDX package — check the new `SpdxPackage.primary_package_purpose` serde wiring (need `#[serde(rename = "primaryPackagePurpose", skip_serializing_if = "Option::is_none")]`).
