# Quickstart — milestone 129

Three operator-facing scenarios. Each is a one-command repro of the corresponding user story.

## Scenario 1 — Scan a .NET image (US1)

A Microsoft-published .NET runtime image carries `.deps.json` files alongside its assemblies. Post
milestone 129, mikebom enumerates every NuGet package.

```sh
mikebom sbom scan \
    --image mcr.microsoft.com/dotnet/runtime:8.0-alpine \
    --output cyclonedx-json=/tmp/dotnet-runtime.cdx.json \
    --root-name dotnet-runtime
```

**Expected output** (verifiable via jq):

```sh
jq -r '.components[].purl' /tmp/dotnet-runtime.cdx.json | grep -c '^pkg:nuget'
# Expected: > 0 (alpha.48 emits 0)
```

For the audit image, expect ≥1,415 `pkg:nuget/...` components per SC-001.

## Scenario 2 — Scan a Rust binary with `cargo auditable` metadata (US2)

Astral's `uv` is built with `cargo auditable`. After milestone 129, mikebom enumerates every crate
listed in the `.dep-v0` ELF section.

```sh
# Pull uv into a scannable location
docker run --rm -v /tmp/uv-cache:/out ghcr.io/astral-sh/uv:latest cp /uv /out/uv

mikebom sbom scan \
    --path /tmp/uv-cache \
    --output cyclonedx-json=/tmp/uv.cdx.json
```

**Expected output**:

```sh
jq -r '.components[].purl' /tmp/uv.cdx.json | grep -c '^pkg:cargo'
# Expected: ~200 (per uv's cargo-auditable manifest)
```

## Scenario 3 — Scan a Spring Boot fat JAR (US3)

A Spring Boot uber JAR carries dozens of dependency JARs nested in `BOOT-INF/lib/`. After milestone 129,
mikebom descends into them.

```sh
# Clone spring-petclinic and build the uber JAR
git clone https://github.com/spring-projects/spring-petclinic /tmp/petclinic
cd /tmp/petclinic && ./mvnw clean package -DskipTests

mikebom sbom scan \
    --path /tmp/petclinic/target \
    --output cyclonedx-json=/tmp/petclinic.cdx.json
```

**Expected output**:

```sh
jq -r '.components[] | select(.purl | startswith("pkg:maven"))
                    | .properties[]?
                    | select(.name == "mikebom:source-mechanism")
                    | .value' /tmp/petclinic.cdx.json | sort | uniq -c
# Expected (post 129):
#   55 maven-jar           (top-level, existing milestone 009)
#   50 maven-jar-nested    (NEW, milestone 129)
```

---

## How to verify mikebom didn't regress (SC-008 byte-identity)

For any image where the milestone-129 readers find no applicable inputs (e.g. a pure-Go image), the
emitted SBOM MUST be byte-identical to the alpha.48 output:

```sh
./scripts/regen-goldens.sh
git status --short mikebom-cli/tests/fixtures/
# Expected: no .cdx.json or .spdx.json files in the diff
```

## How to verify the audit regressed in the right direction

The headline audit was against `767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner:latest`:

```sh
mikebom sbom scan \
    --image 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner:latest \
    --output cyclonedx-json=/tmp/rp-129.cdx.json \
    --root-name 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner \
    --offline

jq -r '.components[].purl' /tmp/rp-129.cdx.json | grep -oE '^pkg:[^/]+' | sort | uniq -c | sort -rn
```

**Expected ecosystem breakdown** (post 129):

| Ecosystem | alpha.48 | milestone 129 target | syft baseline |
|---|---|---|---|
| nuget | 0 | ≥ 1,415 (SC-001) | 1,489 |
| cargo | 58 | ≥ 937 (SC-002) | 986 |
| maven | 72 | ≥ 335 (SC-003) | 372 |
| (other ecosystems) | unchanged | unchanged | unchanged |

And the sbom-comparison weighted score:

```sh
/Users/mlieberman/Projects/sbom-comparison/sbom-comparison --format summary \
    /tmp/rp-129.cdx.json \
    ~/Downloads/remediation-planner-syft-image-sbom.json
# Expected weighted score: mikebom 4.0+ vs syft 2.6 (alpha.48 had mikebom 3.3 vs syft 2.6).
# Completeness moves from 1/5 to 4/5; quality dimensions unchanged.
```
