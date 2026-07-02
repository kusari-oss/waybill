# Quickstart — milestone 153

Validation walkthrough for the SPDX 2.3 §10.1 conformance fix. Mirrors the milestone-478 / milestone-152 operator-cadence pattern — the SC-001 testbed is local to the maintainer's machine; the unit-level coverage is automated.

## Scenario 1 — SC-001 issue-#485 testbed verification (MANUAL operator-cadence)

After the milestone-153 PR merges, the maintainer (or a Yocto-equipped reviewer) runs:

```bash
# 1. Build mikebom at milestone-153 HEAD:
cargo +stable build --release -p mikebom

# 2. Reuse the yocto-test/ testbed from milestones 481 + 485:
#    - scarthgap LTS, poky 802e4c1
#    - core-image-minimal, qemux86-64 MACHINE

# 3. Re-run the issue-#485 mikebom command:
./target/release/mikebom sbom scan --offline \
    --path /path/to/yocto-build/tmp/work/qemux86_64-poky-linux/core-image-minimal/.../rootfs \
    --format spdx-2.3-json \
    --output /tmp/mikebom-m153/core-image-minimal.spdx.json

# 4. Assert the array is present:
jq '.hasExtractedLicensingInfos // "MISSING"' /tmp/mikebom-m153/core-image-minimal.spdx.json
```

**Expected**: JSON array of entries (NOT `"MISSING"`).

```bash
# 5. Assert exactly 3 entries corresponding to the 3 referenced LicenseRefs:
jq '.hasExtractedLicensingInfos | map(.licenseId) | sort' \
    /tmp/mikebom-m153/core-image-minimal.spdx.json
```

**Expected output**:
```json
[
  "LicenseRef-GPL-2.0-with-OpenSSL-exception",
  "LicenseRef-PD",
  "LicenseRef-bzip2-1.0.4"
]
```

```bash
# 6. Assert each entry has the locked placeholder text:
jq '.hasExtractedLicensingInfos[] | .extractedText | startswith("License text not extracted by mikebom.")' \
    /tmp/mikebom-m153/core-image-minimal.spdx.json
```

**Expected**: `true` for every entry.

```bash
# 7. Cross-check: every distinct LicenseRef-* referenced in any package's
#    license field has a matching top-level entry:
jq '
  def refs: [.packages[] | .licenseDeclared // empty, .licenseConcluded // empty]
    | map(scan("LicenseRef-[A-Za-z0-9._-]+")) | flatten | unique;
  def defs: [.hasExtractedLicensingInfos[] | .licenseId] | sort;
  refs - defs
' /tmp/mikebom-m153/core-image-minimal.spdx.json
```

**Expected**: `[]` (empty — every referenced LicenseRef- is defined).

If all 4 checks pass → ✅ SC-001 PASS. Report PASS in the PR comments + close issue #485 on merge.

## Scenario 2 — SC-002 byte-identical happy-path regression (automated)

```bash
# The milestone-090 sibling-fixture cache should already be populated:
ls ~/.cache/mikebom/fixtures/*/transitive_parity/cargo/

# Run the workspace test suite — the existing golden tests verify
# SPDX 2.3 byte-identity for happy-path scans (no LicenseRef-*):
cargo +stable test --workspace
```

**Expected**: every test passes except the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only flake. If any milestone-090 SPDX 2.3 golden test fails → SC-002 regression — investigate (likely cause: an unintended non-empty Vec return from the sweep on a happy-path scan).

## Scenario 3 — SC-003 strict SPDX 2.3 validator (manual or automated)

```bash
# Option A: LF SPDX tools validator
docker run --rm -v /tmp/mikebom-m153:/data \
    spdx/spdx-tools spdx-tools --validate /data/core-image-minimal.spdx.json

# Option B: sbomqs conformance mode
sbomqs score /tmp/mikebom-m153/core-image-minimal.spdx.json --category=NTIA-conformance
```

**Expected**: NO "undefined LicenseRef- reference" errors. Prior to milestone 153, both validators would flag 3 undefined references (`LicenseRef-bzip2-1.0.4`, `LicenseRef-PD`, `LicenseRef-GPL-2.0-with-OpenSSL-exception`); after milestone 153, 0 such errors.

## Scenario 4 — SC-004 SPDX 3 investigation outcome (per-run manual)

```bash
# Emit SPDX 3.0.1 for the same testbed:
./target/release/mikebom sbom scan --offline \
    --path /path/to/yocto-rootfs \
    --format spdx-3-json \
    --output /tmp/mikebom-m153/core-image-minimal.spdx3.json

# Run spdx3-validate (per milestone 078 harness):
.venv/spdx3-validate/bin/spdx3-validate \
    /tmp/mikebom-m153/core-image-minimal.spdx3.json 2>&1
```

**Two expected outcomes** (both are milestone-153 PASS; only ONE will happen):

- **Outcome A** — validator reports "undefined LicenseRef" or equivalent: FR-009 Option A path was required; milestone 153 has implemented the SPDX 3 `sweep_custom_licenses` sibling helper; re-run the validator against a build that INCLUDES the fix and confirm 0 errors.
- **Outcome B** — validator reports no LicenseRef-related errors: FR-009 Option B path holds; the SPDX 3 emitter didn't need equivalent work; document the finding + `spdx3-validate` output in the PR description.

## Scenario 5 — SC-006 unit-test count audit (automated)

```bash
grep -cE "^\s+fn (sweep_|placeholder_)" \
    mikebom-cli/src/generate/spdx/document.rs
```

**Expected**: ≥6 (per SC-006 floor; research.md §R9 lists 10 tests).

## Scenario 6 — SC-005 pre-PR gate

```bash
./scripts/pre-pr.sh
```

**Expected**: green except the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only flake.

## Scenario 7 — SC-007 wire-format guard (manual diff check)

```bash
# Verify no wire-format / catalog / annotation-key changes:
git diff main --name-only -- \
    docs/reference/sbom-format-mapping.md \
    mikebom-cli/src/generate/cyclonedx/
# Expected output: (empty)

git diff main --name-only -- mikebom-common/ mikebom-ebpf/
# Expected output: (empty)

# The primary and (conditional) secondary Rust files:
git diff main --name-only -- mikebom-cli/src/generate/spdx/
# Expected output:
#   mikebom-cli/src/generate/spdx/document.rs
#   mikebom-cli/src/generate/spdx/v3_licenses.rs   (only if SPDX 3 fix was needed)
```

## Scenario 8 — SC-008 CHANGELOG presence

```bash
sed -n '/^## \[Unreleased\]/,/^## \[v/p' CHANGELOG.md | grep -A1 "hasExtractedLicensingInfos\|LicenseRef.*§10\.1\|issue #485"
```

**Expected**: entry present, referencing §10.1 conformance + the placeholder text + issue #485 + SPDX 3 outcome.

## Post-merge — operator-cadence external review

The Yocto testbed verification (SC-001) is manual per Assumption 4. The maintainer runs the testbed after merge and reports pass/fail via a follow-up comment on issue #485 (or its close comment).

## Known deferrals (spec Out of Scope)

- No real license-text extraction from `/usr/share/licenses/*/` (per FR-012). Follow-up milestone if operator demand surfaces.
- No CycloneDX 1.6 changes (per FR-010).
- No SpdxExpression newtype changes (per FR-011).
- No `mikebom:*` annotation keys (per FR-013).
- No `DocumentRef-*:LicenseRef-*` emission (mikebom doesn't emit them; regex excludes them).
- No retroactive re-emission of pre-milestone-153 SBOMs.
