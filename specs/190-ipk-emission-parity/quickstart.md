# Quickstart: m190 ipk Emission Parity Verification

**Date**: 2026-07-13
**Audience**: Developer implementing or reviewing m190; operator verifying the fix against a real Yocto build.

## Purpose

Reproduces the three issues (#550/#551/#552) and verifies the m190 fix, both against synthetic fixtures and against a real Yocto `core-image-minimal` build.

## Prerequisites

- mikebom binary at or after m190 (`cargo build --release -p mikebom-cli`, tagged post-merge)
- `jq` for JSON inspection
- Python 3.10+ with `.venv/spdx3-validate/bin/spdx3-validate` installed (per memory `reference_spdx3_validator`)
- Optional: a real Yocto build directory of `.ipk` files for real-world validation

## Reproducer 1 — Synthetic fixture (fastest verification)

Build a synthetic ipk with a compound license and epoch prefix:

```bash
# Requires the m187 fixture-builder helper — reproduce by hand for
# quickstart purposes. Alternative: use one of the m190 test fixtures
# from mikebom-cli/tests/fixtures/ipk_m190/ once merged.

mkdir -p /tmp/m190-fixture/build && cd /tmp/m190-fixture/build
cat > control << 'CONTROL'
Package: quickstart-fixture
Version: 1:2.0-r0
Description: m190 quickstart fixture
Section: base
Priority: optional
Maintainer: nobody
License: GPL-2.0-only & MIT
Architecture: all
CONTROL

# Package as ipk (opkg-build-format ar archive; matches m187 pattern).
# Simplified illustrative form; the real fixture-builder produces the ar
# archive with the correct debian-binary + control.tar.gz + data.tar.gz
# member layout.
```

Run mikebom against the fixture directory in all three formats:

```bash
mkdir -p /tmp/m190-out
mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m190-fixture/ --output /tmp/m190-out/out.cdx.json
mikebom sbom scan --offline --format spdx-2.3-json \
  --path /tmp/m190-fixture/ --output /tmp/m190-out/out.spdx.json
mikebom sbom scan --offline --format spdx-3-json \
  --path /tmp/m190-fixture/ --output /tmp/m190-out/out.spdx3.json
```

### Assertion 1 — CDX license normalization (US1 / #550)

```bash
jq -r '.components[]
  | select(.name == "quickstart-fixture")
  | .licenses[0].expression' /tmp/m190-out/out.cdx.json
```

**Expected**: `GPL-2.0-only AND MIT`
**Pre-m190 (broken)**: `GPL-2.0-only & MIT` or `LicenseRef-<hex>`

Grep for raw operators — MUST be empty:

```bash
grep -oE '"expression":\s*"[^"]*[&|][^"]*"' /tmp/m190-out/out.cdx.json
```

### Assertion 2 — SPDX 3 license emission (US2 / #551)

```bash
jq '[.["@graph"][] | select(.type == "simplelicensing_LicenseExpression")] | length' \
  /tmp/m190-out/out.spdx3.json
```

**Expected**: `1` (or more if the corpus has multiple licenses)
**Pre-m190 (broken)**: `0`

Verify the expression value:

```bash
jq -r '.["@graph"][] | select(.type == "simplelicensing_LicenseExpression")
  | .simplelicensing_licenseExpression' /tmp/m190-out/out.spdx3.json
```

**Expected**: `GPL-2.0-only AND MIT`

### Assertion 3 — Epoch qualifier (US3 / #552)

```bash
jq -r '.components[]
  | select(.name == "quickstart-fixture")
  | {name, version, purl}' /tmp/m190-out/out.cdx.json
```

**Expected**:

```json
{
  "name": "quickstart-fixture",
  "version": "2.0-r0",
  "purl": "pkg:opkg/quickstart-fixture@2.0-r0?arch=all&epoch=1"
}
```

**Pre-m190 (broken)**:

```json
{
  "name": "quickstart-fixture",
  "version": "1:2.0-r0",
  "purl": "pkg:opkg/quickstart-fixture@1:2.0-r0?arch=all"
}
```

### Assertion 4 — Cross-format PURL parity (FR-013)

```bash
CDX_PURL=$(jq -r '.components[] | select(.name == "quickstart-fixture") | .purl' /tmp/m190-out/out.cdx.json)
SPDX_PURL=$(jq -r '.packages[] | select(.name == "quickstart-fixture") | .externalRefs[] | select(.referenceType == "purl") | .referenceLocator' /tmp/m190-out/out.spdx.json)
SPDX3_PURL=$(jq -r '.["@graph"][] | select(.type == "software_Package" and .name == "quickstart-fixture") | .software_packageUrl' /tmp/m190-out/out.spdx3.json)

[ "$CDX_PURL" = "$SPDX_PURL" ] && [ "$SPDX_PURL" = "$SPDX3_PURL" ] && \
  echo "PARITY OK: $CDX_PURL" || echo "PARITY FAIL"
```

**Expected**: `PARITY OK: pkg:opkg/quickstart-fixture@2.0-r0?arch=all&epoch=1`

### Assertion 5 — SPDX 3 conformance (FR-007)

```bash
.venv/spdx3-validate/bin/spdx3-validate /tmp/m190-out/out.spdx3.json
```

**Expected**: Exit 0, zero license-related conformance errors.

## Reproducer 2 — Real Yocto core-image-minimal

Assumes a Yocto build tree at `/opt/yocto/build/tmp/deploy/ipk/`.

```bash
mikebom sbom scan --offline --format cyclonedx-json \
  --path /opt/yocto/build/tmp/deploy/ipk/ --output /tmp/coreimage.cdx.json
mikebom sbom scan --offline --format spdx-3-json \
  --path /opt/yocto/build/tmp/deploy/ipk/ --output /tmp/coreimage.spdx3.json
```

### Real-world assertion 1 — No raw BitBake operators in any CDX license expression

```bash
jq -r '.components[].licenses[]?.expression // empty' /tmp/coreimage.cdx.json | \
  grep -E '(^|[^A-Za-z])[&|]([^A-Za-z]|$)'
```

**Expected**: empty output (zero matches).

### Real-world assertion 2 — Every ipk-typed component with a license field emits an SPDX 3 LicenseExpression

```bash
IPK_COUNT=$(jq '[.["@graph"][] | select(.type == "software_Package" and (.software_packageUrl // "" | test("^pkg:opkg/")))] | length' /tmp/coreimage.spdx3.json)
LICENSE_COUNT=$(jq '[.["@graph"][] | select(.type == "simplelicensing_LicenseExpression")] | length' /tmp/coreimage.spdx3.json)

echo "IPK packages: $IPK_COUNT / License elements: $LICENSE_COUNT"
```

**Expected**: `$LICENSE_COUNT > 0` (was `0` pre-m190). Exact ratio to `IPK_COUNT` depends on how many packages have empty license fields; typical: `$LICENSE_COUNT >= 0.9 * $IPK_COUNT`.

### Real-world assertion 3 — At least one epoch-qualifier PURL emitted

```bash
jq -r '.components[].purl // empty' /tmp/coreimage.cdx.json | grep -E '\?.*epoch='
```

**Expected**: at least one line matching `pkg:opkg/<name>@<version>?arch=<arch>&epoch=<N>`. `netbase` is the canonical example on `core-image-minimal`.

### Real-world assertion 4 — SPDX 3 conformance

```bash
.venv/spdx3-validate/bin/spdx3-validate /tmp/coreimage.spdx3.json
```

**Expected**: exit 0 with no new conformance errors relative to the alpha.60 (pre-m190) baseline. If the alpha.60 baseline already had license-related errors on this fixture, m190 MUST reduce that count to zero.

## Regression gate — byte-identity for no-epoch, single-license paths

Run the pre-m190 mikebom (e.g., alpha.60) against the same corpus as m190; diff the outputs for any ipk without a `<digits>:` version prefix AND with a single-SPDX-operand license:

```bash
# Diff should be empty for these components:
diff <(jq -S '.components[] | select(.name == "some-simple-package")' /tmp/alpha60.cdx.json) \
     <(jq -S '.components[] | select(.name == "some-simple-package")' /tmp/m190.cdx.json)
```

**Expected**: no diff. Any diff on such a component indicates a byte-identity regression (SC-006 gate failure) and MUST be investigated before merge.

## CI verification recap

The full pre-PR gate is unchanged from mikebom's standard:

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace` MUST pass clean.

New test files introduced by m190:
- `mikebom-cli/tests/ipk_license_parity.rs` (US1 + US2 + FR-013 cross-format parity)
- `mikebom-cli/tests/ipk_epoch_purl.rs` (US3 epoch qualifier)
- New unit tests in `ipk_file.rs::tests` for the two new helpers.
