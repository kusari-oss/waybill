# Quickstart — milestone 131

Three operator-facing scenarios mirroring milestone-130's quickstart shape.

## Scenario 1 — Version-fidelity on .NET image (US1)

After milestone 131, `pkg:nuget` PURLs from PE/CLR-derived components carry the published
`AssemblyInformationalVersion` (semver-style, matches NuGet.org) instead of the AssemblyVersion
4-tuple.

```sh
mikebom sbom scan --image mcr.microsoft.com/dotnet/runtime:8.0-alpine \
    --output cyclonedx-json=/tmp/dotnet-runtime-131.cdx.json
```

**Expected output**:

```sh
# Pre-131: Microsoft.AspNetCore@8.0.0.0 (AssemblyVersion 4-tuple)
# Post-131: Microsoft.AspNetCore@8.0.27-servicing.26230.7 (InformationalVersion)
jq -r '.components[] | select(.name == "Microsoft.AspNetCore") | .version' /tmp/dotnet-runtime-131.cdx.json
```

For the audit image, expect <20 VERSION_MISMATCH residual against syft (vs 373 pre-131).

## Scenario 2 — License coverage on the audit image (US2)

```sh
mikebom sbom scan \
    --image 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner:latest \
    --output cyclonedx-json=/tmp/rp-131.cdx.json \
    --offline

jq -r '.components[] | select(.licenses // [] | length > 0) | .name' /tmp/rp-131.cdx.json | wc -l
# Expected: ≥1,500 components carrying license info (vs pre-131 ~700)
```

## Scenario 3 — Supplier external-references (US3)

```sh
jq -r '.components[] | select((.purl // "") | startswith("pkg:cargo")) | (.externalReferences // [])[0].url' /tmp/rp-131.cdx.json | head -3
# Expected: https://crates.io/crates/serde/1.0.193 etc.

jq -r '.components[] | select((.purl // "") | startswith("pkg:nuget")) | (.externalReferences // [])[0].url' /tmp/rp-131.cdx.json | head -3
# Expected: https://www.nuget.org/packages/Microsoft.AspNetCore.App.Runtime.wolfi.20230201-x64/8.0.27

jq -r '.components[] | select((.purl // "") | startswith("pkg:maven")) | select(.properties[]? | (.name == "mikebom:source-mechanism") and (.value == "maven-jar-nested")) | (.externalReferences // [])[0].url' /tmp/rp-131.cdx.json | head -3
# Expected: https://search.maven.org/artifact/<g>/<a>/<v>/jar
```

## End-to-end sbom-comparison verification

```sh
/Users/mlieberman/Projects/sbom-comparison/sbom-comparison --format summary \
    /tmp/rp-131.cdx.json \
    ~/Downloads/remediation-planner-syft-image-sbom.json

# Expected post-131 scorecard:
#   Version Accuracy:        5/5 (was 4/5 pre-131)
#   License Coverage:        ≥3/5 (was 1/5)
#   Supplier Attribution:    ≥3/5 (was 2/5)
#   OVERALL weighted:        ≥3.0 (was 2.4) — mikebom leads syft by ≥0.5 points (SC-001).
```

## Byte-identity verification (SC-005)

```sh
./scripts/regen-goldens.sh && git status --short mikebom-cli/tests/fixtures/
# Expected: zero .cdx.json / .spdx.json churn across 33 alpha.48 goldens.
```
