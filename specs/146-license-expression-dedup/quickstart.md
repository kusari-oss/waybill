# Quickstart — milestone 146 SPDX license expression operand dedup

Operator-facing walkthrough.

## Scenario 1 — Verify the dedup on a synthetic RPM

The simplest reproducible check: build a synthetic RPM with `License: MIT AND MIT` and scan it.

```bash
# Build a synthetic RPM via the existing rpm crate fixture infrastructure
# (same pattern as milestone 144 T035). Or use any real RPM whose License:
# header contains a duplicated form — Yocto-built `tmp/deploy/rpm/` is the
# canonical source.

mikebom sbom scan --path <dir-with-duplicated-license-rpm> \
    --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
    --output cyclonedx-json=/tmp/d.cdx.json \
    --output spdx-2.3-json=/tmp/d.spdx.json \
    --output spdx-3-json=/tmp/d.spdx3.json

# CDX: the license should be single-id `id`, not compound `expression`.
jq '.components[] | .licenses' /tmp/d.cdx.json
# Pre-146: { "license": { "expression": "MIT AND MIT" } }
# Post-146: { "license": { "id": "MIT" } }  -- single-id shape; better CDX schema-validation

# SPDX 2.3: licenseDeclared should be the single id.
jq -r '.packages[] | .licenseDeclared' /tmp/d.spdx.json
# Pre-146: "MIT AND MIT"
# Post-146: "MIT"

# SPDX 3: software_declaredLicense should be the single id.
jq -r '.["@graph"][] | select(.type == "software_Package") | .software_declaredLicense // empty' /tmp/d.spdx3.json
# Same expectation as SPDX 2.3.
```

## Scenario 2 — Verify against the Yocto baseline (the audit corpus)

The original issue #470 surfaced 7 distinct duplicated expressions × ≥30 components on the `core-image-minimal` qemux86-64 build. Post-146:

```bash
# Scan the Yocto build output (operator's own testbed):
mikebom sbom scan --path tmp/deploy/rpm/ \
    --rpm-distro poky \
    --format spdx-2.3-json --output /tmp/yocto.spdx.json

# Count packages with the pre-146 X AND X shape (should be 0 post-146):
jq -r '.packages[]
       | select(.licenseDeclared | test("^([^ ]+) AND \\1$"))
       | .name' /tmp/yocto.spdx.json
# Pre-146: ~30 package names listed.
# Post-146: empty output.

# Same query on licenseConcluded:
jq -r '.packages[]
       | select(.licenseConcluded | test("^([^ ]+) AND \\1$"))
       | .name' /tmp/yocto.spdx.json
# Post-146: empty.
```

## Scenario 3 — Verify WITH clauses are preserved atomic (FR-003 guard)

```bash
# Build a synthetic RPM with License: "GPL-2.0-or-later WITH Classpath-exception-2.0":
#  - Verify SPDX-2.3 declaredLicense preserves the WITH clause intact.
mikebom sbom scan --path <dir-with-classpath-exception-rpm> \
    --format spdx-2.3-json --output /tmp/with.spdx.json

jq -r '.packages[] | select(.name == "<your-pkg>") | .licenseDeclared' /tmp/with.spdx.json
# Expected: "GPL-2.0-or-later WITH Classpath-exception-2.0"  (NOT just "GPL-2.0-or-later")
```

## Scenario 4 — Idempotence (Invariant 7 guard)

```bash
# Round-trip via the binary (manual smoke). Construct an SBOM, then
# scan its own emitted-back input:
echo "MIT AND MIT" | <your-rust-test-helper-using-SpdxExpression::try_canonical>
# Output: "MIT"

echo "MIT" | <same-helper>
# Output: "MIT"  (idempotent — second pass is a no-op)
```

## Verification commands (in-tree, CI-binding)

```bash
# Unit tests in mikebom-common:
cargo test -p mikebom-common types::license::

# Integration test in mikebom-cli (synthetic RPM end-to-end):
cargo test --test license_dedup_integration_md146

# Pre-PR gate:
./scripts/pre-pr.sh
```

## Golden refresh (post-fix, before commit)

```bash
# Run all three format updates; inspect diffs:
MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression
MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --test spdx_regression
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression

# Inspect:
git diff --stat -- mikebom-cli/tests/fixtures/golden/
# Expected: small diff set; each affected line is a license-string simplification.
# Reject any unrelated drift.
```

## Cross-format byte-equivalence check

```bash
# All three formats should carry the same deduped license string for any
# shared component:
mikebom sbom scan --path <dir> \
    --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
    --output cyclonedx-json=/tmp/c.cdx.json \
    --output spdx-2.3-json=/tmp/c.spdx.json \
    --output spdx-3-json=/tmp/c.spdx3.json

# Extract licenses keyed by PURL from each format:
jq -r '.components[] | "\(.purl) \(.licenses[0].license.id // .licenses[0].license.expression // "?")"' /tmp/c.cdx.json | sort > /tmp/cdx-lic.txt
jq -r '.packages[] | "\(.externalRefs[] | select(.referenceType == "purl") | .referenceLocator) \(.licenseDeclared)"' /tmp/c.spdx.json | sort > /tmp/spdx-lic.txt
jq -r '.["@graph"][] | select(.software_packageUrl) | "\(.software_packageUrl) \(.software_declaredLicense // "NOASSERTION")"' /tmp/c.spdx3.json | sort > /tmp/spdx3-lic.txt

diff /tmp/cdx-lic.txt /tmp/spdx-lic.txt
diff /tmp/cdx-lic.txt /tmp/spdx3-lic.txt
# Post-146: empty diffs.
```

## Known deferrals (spec Out of Scope)

- Recursive dedup into parenthesized sub-expressions (e.g., `(MIT AND MIT) OR Apache-2.0`).
- Algebraic simplification beyond operand dedup.
- Cross-tier license merging at `resolve/deduplicator.rs` (the deduplicator already doesn't merge licenses per the milestone-145 investigation).
- Changes to the upstream RPM reader (the duplication is upstream-Yocto-side).
- New `mikebom:*` annotations.
