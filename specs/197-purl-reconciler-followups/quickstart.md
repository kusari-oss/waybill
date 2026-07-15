# Quickstart: m190 + m191 Follow-Up Bundle

**Date**: 2026-07-15
**Audience**: mikebom maintainer implementing m197 or reviewing the PR.

## Prerequisites

- Working mikebom checkout on branch `197-purl-reconciler-followups`
- `cargo +stable` toolchain (existing workspace toolchain)
- `dpkg-deb` / `apk` / `rpmbuild` (or equivalents) for building synthetic epoch-versioned fixtures — OR use pre-built vendored `.deb` / `.apk` / `.rpm` fixtures under `tests/fixtures/`
- No network required — every test is offline against local fixtures

## Reproducer 1 — Verify dpkg epoch qualifier emission (US1)

```bash
# Create a synthetic .deb with epoch (or use a pre-vendored fixture):
mkdir -p /tmp/deb-epoch/pkg-1.0/DEBIAN
cat > /tmp/deb-epoch/pkg-1.0/DEBIAN/control <<'EOF'
Package: test-pkg
Version: 1:2.0-r0
Architecture: amd64
Maintainer: Test
Description: epoch-versioned test
EOF
dpkg-deb --build /tmp/deb-epoch/pkg-1.0 /tmp/deb-epoch/test-pkg_1:2.0-r0_amd64.deb

# Scan:
mikebom --offline sbom scan --path /tmp/deb-epoch/ --format cyclonedx-json --output /tmp/deb-out.json

# Verify:
jq -r '.components[] | select(.name == "test-pkg") | .purl' /tmp/deb-out.json
# Expected (post-m197): pkg:deb/debian/test-pkg@2.0-r0?epoch=1
# Pre-m197:             pkg:deb/debian/test-pkg@1:2.0-r0
```

## Reproducer 2 — Verify apk epoch qualifier emission (US2)

Same pattern as Reproducer 1 with an `.apk` fixture. `apk` version fields in the `installed` database follow the same `<epoch>:<version>-r<release>` shape when an epoch is present.

## Reproducer 3 — Verify rpm epoch qualifier remains correct (US2b non-regression)

The rpm reader already handles epoch. This reproducer confirms no regression:

```bash
# Synthetic .rpm with Version: 2.0, Release: r0, Epoch: 1
# (build via `rpmbuild --define '_topdir /tmp/rpm-build' -bb <spec-with-epoch>`)
mikebom --offline sbom scan --path /tmp/rpm-epoch/ --format cyclonedx-json --output /tmp/rpm-out.json

# Verify:
jq -r '.components[] | select(.name == "test-pkg") | .purl' /tmp/rpm-out.json
# Expected (unchanged from pre-m197): pkg:rpm/<vendor>/test-pkg@2.0-r0?epoch=1&...
```

## Reproducer 4 — Verify versionless PURL for 6 additional ecosystems (US3)

For each of the 6 ecosystems, construct a scan target with a versionless dep. Composer example:

```bash
mkdir -p /tmp/composer-versionless
cat > /tmp/composer-versionless/composer.json <<'EOF'
{
  "require": {
    "vendor/some-pkg": "*"
  }
}
EOF
mikebom --offline sbom scan --path /tmp/composer-versionless/ --format cyclonedx-json --output /tmp/composer-out.json

# Verify:
jq -r '.components[] | .purl' /tmp/composer-out.json | grep composer
# Expected (post-m197): pkg:composer/vendor/some-pkg    (no trailing @)
# Pre-m197:             pkg:composer/vendor/some-pkg@   (trailing @ — invalid)
```

Repeat with dart's `pubspec.yaml`, cocoapods' `Podfile`, sbt's `build.sbt`, cabal's `.cabal`, and mix/rebar's `mix.exs` / `rebar.config` — each with a versionless dep.

## Reproducer 5 — Verify fuzz suite (US4)

```bash
cargo test -p mikebom-common versionless_purl_fuzz -- --nocapture
```

**Expected**: `test result: ok. 11 passed; 0 failed`. Diagnostic output shows the per-ecosystem invocation counts (≥ 100 each per FR-004).

## Reproducer 6 — Verify npm-alias reconciler match (US5)

```bash
mkdir -p /tmp/npm-alias
cat > /tmp/npm-alias/package.json <<'EOF'
{
  "name": "my-app",
  "version": "1.0.0",
  "dependencies": {
    "my-alias": "npm:actual-pkg@1.0.0"
  }
}
EOF
# Also create a package-lock.json resolving my-alias → actual-pkg@1.0.0

mikebom --offline sbom scan --path /tmp/npm-alias/ --format cyclonedx-json --output /tmp/npm-out.json

# Verify:
jq '.components[] | select(.purl == "pkg:npm/actual-pkg@1.0.0") | {purl, properties}' /tmp/npm-out.json
# Expected: exactly one component with:
#   - purl: pkg:npm/actual-pkg@1.0.0
#   - properties includes {name: "mikebom:declared-as", value: "[\"my-alias\"]"}
```

## Reproducer 7 — Verify multi-declaration array preservation (US6)

```bash
mkdir -p /tmp/monorepo/packages/{foo,bar}
cat > /tmp/monorepo/package.json <<'EOF'
{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}
EOF
cat > /tmp/monorepo/packages/foo/package.json <<'EOF'
{"name": "foo", "version": "1.0.0", "dependencies": {"commander": "^11.0"}}
EOF
cat > /tmp/monorepo/packages/bar/package.json <<'EOF'
{"name": "bar", "version": "1.0.0", "dependencies": {"commander": "^11.1.0"}}
EOF
# Also create a root package-lock.json resolving both to commander@11.1.0

mikebom --offline sbom scan --path /tmp/monorepo/ --format cyclonedx-json --output /tmp/mono-out.json

# Verify:
jq '.components[] | select(.purl == "pkg:npm/commander@11.1.0") | .properties[] | select(.name | test("requirement-ranges|source-manifests"))' /tmp/mono-out.json
# Expected: two properties, each with JSON-array values of length 2:
#   - mikebom:requirement-ranges: ["^11.0", "^11.1.0"]
#   - mikebom:source-manifests: ["packages/bar/package.json", "packages/foo/package.json"]
```

## Reproducer 8 — Verify FR-007 additive-only guarantee

```bash
# Run the full workspace golden-regression suites.
cargo test --workspace 2>&1 | grep -E "(regression|golden)" | grep -v "^warning"

# Expected: all golden suites pass. If any non-reconciler-path golden drifts, the
# additive-only guarantee is broken — investigate before merging.
```

## Reproducer 9 — Regen reconciler-path goldens (post-implementation)

Per Q1 clarification exception:

```bash
# Grep for the m191 singular scalars to identify reconciler-path goldens:
grep -rln '"mikebom:requirement-range"\|"mikebom:source-manifest"' mikebom-cli/tests/fixtures/golden/

# For each hit, regen via the standard mikebom golden-regen path:
MIKEBOM_UPDATE_CDX_GOLDENS=1 \
MIKEBOM_UPDATE_SPDX_GOLDENS=1 \
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 \
  cargo test --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test pkg_alias_binding_us1 --test oci_pull_backward_compat --test optional_dep_classification

# Diff-review — every diff MUST be exclusively singular-→-array rotation.
```

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) and `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.
