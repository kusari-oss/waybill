# Quickstart: verifying the `--tier` filter flag works

**Feature**: 232-tier-filter-flag
**Phase**: 1

Executes SC-001..SC-005 predicates against the finished implementation. All commands assume repository root at `/Users/mlieberman/Projects/mikebom` and a `cargo build --release -p waybill` binary at `target/release/waybill`.

## Setup

```bash
cargo build --release -p waybill
```

Use the m230 NuGet fixture as the shared test target — it produces a mix of source-tier package components and a design-tier main-module (`pkg:generic/App@0.0.0`):

```bash
FIXTURE=waybill-cli/tests/fixtures/golden_inputs/nuget/packages_lock_present
```

## Walkthrough — SC-003 (default byte-parity)

```bash
mkdir -p /tmp/232-verify
./target/release/waybill --offline sbom scan --path "$FIXTURE" --format cyclonedx-json --output /tmp/232-verify/default.cdx.json --no-deep-hash 2>/dev/null
./target/release/waybill --offline sbom scan --path "$FIXTURE" --tier=all --format cyclonedx-json --output /tmp/232-verify/explicit-all.cdx.json --no-deep-hash 2>/dev/null

# Mask nondeterministic fields (timestamps, serial numbers, content-addressed IDs)
mask() {
  sed -E 's/"timestamp": "[^"]+"/"timestamp": "MASKED"/g
          s/"serialNumber": "[^"]+"/"serialNumber": "MASKED"/g' "$1" | LC_ALL=C sort
}
diff <(mask /tmp/232-verify/default.cdx.json) <(mask /tmp/232-verify/explicit-all.cdx.json)
# Expect: empty diff — --tier=all is a no-op filter.
```

## Walkthrough — SC-001 (`--tier=source-only`)

```bash
./target/release/waybill --offline sbom scan --path "$FIXTURE" --tier=source-only --format cyclonedx-json --output /tmp/232-verify/source.cdx.json --no-deep-hash 2>/dev/null

# Assert every emitted NuGet component is source-tier.
jq -r '.components[] | select(.purl // "" | startswith("pkg:nuget/")) | (.properties // []) | map(select(.name == "waybill:sbom-tier")) | .[0].value' /tmp/232-verify/source.cdx.json | sort -u
# Expect: "source" (or empty output if the m230 fixture happens to have no source-tier components — inspect further if so)

# Assert zero design-tier components emitted.
jq -r '.components[] | (.properties // []) | map(select(.name == "waybill:sbom-tier" and .value == "design")) | length' /tmp/232-verify/source.cdx.json | grep -v "^0$" | head -5
# Expect: empty output (no design-tier components survived)

# Assert dependencies section has no dangling refs.
jq -r '
  ([.components[]."bom-ref"]) as $known
  | .dependencies[] | ((.ref) as $r | .dependsOn // []) as $deps
  | ($deps + [$r]) | .[]
  | select(([., $known] | .[0] as $x | .[1] | index($x)) | not)
' /tmp/232-verify/source.cdx.json | head -5
# Expect: empty output (no dangling PURLs)
```

## Walkthrough — SC-002 (`--tier=design-only`)

```bash
./target/release/waybill --offline sbom scan --path "$FIXTURE" --tier=design-only --format cyclonedx-json --output /tmp/232-verify/design.cdx.json --no-deep-hash 2>/dev/null

# Assert every emitted component is design-tier.
jq -r '.components[] | (.properties // []) | map(select(.name == "waybill:sbom-tier")) | .[0].value' /tmp/232-verify/design.cdx.json | sort -u
# Expect: "design" (single value)

# The m230 fixture has one design-tier main-module (pkg:generic/App@0.0.0)
# so expect .components | length == 1
jq '.components | length' /tmp/232-verify/design.cdx.json
# Expect: 1
```

## Walkthrough — SC-004 (graph-completeness re-evaluation)

```bash
# Pre-filter graph-completeness reason:
jq -r '.metadata.properties[] | select(.name == "waybill:graph-completeness-reason") | .value' /tmp/232-verify/default.cdx.json

# Post-filter graph-completeness reason (source-only):
jq -r '.metadata.properties[] | select(.name == "waybill:graph-completeness-reason") | .value' /tmp/232-verify/source.cdx.json

# Expect: the two values differ, or the post-filter value is absent (design-tier orphans dropped → no orphan-reason emitted).
```

## Walkthrough — SC-005 (cross-format consistency)

```bash
./target/release/waybill --offline sbom scan --path "$FIXTURE" --tier=source-only --format cyclonedx-json --output /tmp/232-verify/source.cdx.json --no-deep-hash 2>/dev/null
./target/release/waybill --offline sbom scan --path "$FIXTURE" --tier=source-only --format spdx-2.3-json --output /tmp/232-verify/source.spdx.json --no-deep-hash 2>/dev/null
./target/release/waybill --offline sbom scan --path "$FIXTURE" --tier=source-only --format spdx-3-json --output /tmp/232-verify/source.spdx3.json --no-deep-hash 2>/dev/null

# CDX PURLs
jq -r '.components[].purl' /tmp/232-verify/source.cdx.json | sort > /tmp/232-verify/purls.cdx.txt

# SPDX 2.3 PURLs
jq -r '.packages[] | (.externalRefs // [])[] | select(.referenceType == "purl") | .referenceLocator' /tmp/232-verify/source.spdx.json | sort > /tmp/232-verify/purls.spdx23.txt

# SPDX 3 PURLs
jq -r '.["@graph"][] | select(."@type" == "software_Package") | .software_packageUrl // empty' /tmp/232-verify/source.spdx3.json | sort > /tmp/232-verify/purls.spdx3.txt

diff /tmp/232-verify/purls.cdx.txt /tmp/232-verify/purls.spdx23.txt
diff /tmp/232-verify/purls.cdx.txt /tmp/232-verify/purls.spdx3.txt
# Expect: both diffs empty — three formats emit the same PURL set post-filter.
```

## Walkthrough — empty-result path (FR-008)

If the fixture has zero binary-tier components, `--tier=source-and-binary` still succeeds; test the true empty case by picking a mode with zero matches:

```bash
# Assuming the m230 fixture has no analyzed-tier or file-tier components,
# a mode that only matches those would produce empty output — but the
# spec only defines four modes. To test the empty-result path, pick a
# design-only scan against a fixture with only source-tier components:
CARGO_FIXTURE=waybill-cli/tests/fixtures/golden_inputs/nuget/csproj_legacy
./target/release/waybill --offline sbom scan --path "$CARGO_FIXTURE" --tier=design-only --format cyclonedx-json --output /tmp/232-verify/empty.cdx.json --no-deep-hash 2>/tmp/232-verify/empty.stderr
grep "tier filter dropped all components" /tmp/232-verify/empty.stderr
# Expect: WARN log line present

jq '.components | length' /tmp/232-verify/empty.cdx.json
# Expect: 0
```

## Post-implementation checklist

- [ ] SC-001: `--tier=source-only` emits only source-tier NuGet components; no design-tier PURLs.
- [ ] SC-002: `--tier=design-only` emits only design-tier components.
- [ ] SC-003: `--tier=all` (or flag omitted) byte-parity with pre-232 emission.
- [ ] SC-004: graph-completeness reason differs pre/post filter when design-tier orphans dominate.
- [ ] SC-005: three formats emit the same PURL set post-filter.
- [ ] FR-008: empty-result path emits WARN + zero components + exit 0.
- [ ] Pre-PR gate: `./scripts/pre-pr.sh` exits 0.
