# Quickstart — milestone 154

Validation walkthrough for the SPDX 3 `simplelicensing_CustomLicense` sweep. Mirrors milestones 152/153's operator-cadence pattern — the SC-001 testbed is local to the maintainer; the unit-level coverage is automated.

## Scenario 1 — SC-001 issue-#487 testbed cross-format symmetry (MANUAL operator-cadence)

After the milestone-154 PR merges, the maintainer (or a Yocto-equipped reviewer) runs:

```bash
# 1. Build mikebom at milestone-154 HEAD:
cargo +stable build --release -p mikebom

# 2. Reuse the yocto-test/ testbed from milestones 481 / 485 / 487.

# 3. Emit BOTH SPDX 2.3 AND SPDX 3 for the same rootfs:
./target/release/mikebom sbom scan --offline \
    --path /path/to/yocto-rootfs \
    --format spdx-2.3-json,spdx-3-json \
    --output spdx-2.3-json=/tmp/mikebom-m154/core-image-minimal.spdx.json \
    --output spdx-3-json=/tmp/mikebom-m154/core-image-minimal.spdx3.json

# 4. Assert the SPDX 3 side now has CustomLicense elements:
jq '[.["@graph"][] | select(.type == "simplelicensing_CustomLicense") | .name] | sort' \
    /tmp/mikebom-m154/core-image-minimal.spdx3.json
```

**Expected output** (3 entries, sorted):
```json
[
  "GPL-2.0-with-OpenSSL-exception",
  "PD",
  "bzip2-1.0.4"
]
```

```bash
# 5. Assert cross-format LicenseRef set equality (SC-001 + FR-009):
diff \
  <(jq -c '[.hasExtractedLicensingInfos[].licenseId] | sort' \
      /tmp/mikebom-m154/core-image-minimal.spdx.json) \
  <(jq -c '[.["@graph"][]
             | select(.type == "simplelicensing_CustomLicense")
             | ("LicenseRef-" + .name)] | sort' \
      /tmp/mikebom-m154/core-image-minimal.spdx3.json)
```

**Expected**: empty diff (both sets equal).

```bash
# 6. Assert cross-format placeholder identity (SC-001 + FR-010):
jq -r '.hasExtractedLicensingInfos[].extractedText' \
    /tmp/mikebom-m154/core-image-minimal.spdx.json | sort -u \
  | diff - <(jq -r '.["@graph"][] | select(.type == "simplelicensing_CustomLicense") | .simplelicensing_licenseText' \
      /tmp/mikebom-m154/core-image-minimal.spdx3.json | sort -u)
```

**Expected**: empty diff (one placeholder string used byte-identically across both formats).

```bash
# 7. Assert each CustomLicense's spdxId matches the expected scheme:
jq -r '.["@graph"][] | select(.type == "simplelicensing_CustomLicense") | .spdxId' \
    /tmp/mikebom-m154/core-image-minimal.spdx3.json
```

**Expected** (3 lines, each matching the pattern `<doc_iri>/licenseref/<idstring>`):
```
https://mikebom.kusari.dev/spdx3/doc-<hash>/licenseref/GPL-2.0-with-OpenSSL-exception
https://mikebom.kusari.dev/spdx3/doc-<hash>/licenseref/PD
https://mikebom.kusari.dev/spdx3/doc-<hash>/licenseref/bzip2-1.0.4
```

If all 4 checks pass → ✅ SC-001 PASS. Report PASS in the PR comments + close issue #487 on merge.

## Scenario 2 — SC-002 byte-identical happy path (automated)

```bash
cargo +stable test --workspace
```

**Expected**: every test passes except the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only flake. Any SPDX 3 golden test failure → SC-002 regression (indicates the sweep produced a non-empty Vec for a scan that should have had none).

## Scenario 3 — SC-003 `spdx3-validate` continues to pass (automated)

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo +stable test --workspace --test spdx3_conformance
```

**Expected**: all tests pass — including `every_existing_golden_passes_validator` which runs `spdx3-validate==0.0.5` against every committed SPDX 3 golden. If any golden now contains `simplelicensing_CustomLicense` elements (i.e., some existing test fixture happened to trigger the milestone-152 LicenseRef path), the validator run confirms the shape is spec-conformant.

## Scenario 4 — SC-004 cross-format placeholder identity (compile-time)

The `pub(crate)` visibility promotion of `PLACEHOLDER_EXTRACTED_TEXT` mechanically enforces this at compile time — the SPDX 3 emitter imports the exact same const value that the SPDX 2.3 emitter uses. A bonus test (research.md §R6 test #6) explicitly asserts:

```rust
#[test]
fn cross_format_placeholder_identity() {
    // Both formats reference the same const.
    assert_eq!(
        super::document::PLACEHOLDER_EXTRACTED_TEXT,
        super::document::PLACEHOLDER_EXTRACTED_TEXT  // trivially true; documents intent
    );
    // Verify the imported const value matches the wire contract from
    // milestone 153 Clarifications Q1.
    assert!(super::document::PLACEHOLDER_EXTRACTED_TEXT
        .starts_with("License text not extracted by mikebom."));
}
```

## Scenario 5 — SC-005 pre-PR gate

```bash
./scripts/pre-pr.sh
```

**Expected**: green except the documented `sbomqs_parity` env-only flake.

## Scenario 6 — SC-006 unit-test count

```bash
grep -cE "^\s+fn sweep_custom_licenses_" mikebom-cli/src/generate/spdx/v3_licenses.rs
```

**Expected**: ≥5 (per SC-006 floor; research.md §R6 lists 5 + 1 bonus cross-format test = 6).

## Scenario 7 — SC-007 wire-format guard (manual diff check)

```bash
# Only 3 Rust files + CHANGELOG allowed to change:
git diff main --name-only

# Expected output (order may vary):
#   CHANGELOG.md
#   CLAUDE.md   (auto-updated by speckit plan)
#   mikebom-cli/src/generate/spdx/document.rs   (1-line visibility change only)
#   mikebom-cli/src/generate/spdx/v3_document.rs (+~3 LOC wiring)
#   mikebom-cli/src/generate/spdx/v3_licenses.rs (primary deliverable +~90 LOC)
#   specs/154-spdx3-custom-licenses/*   (speckit branch artifacts)
```

**Additional guards**:
```bash
# No CycloneDX changes:
git diff main --name-only -- mikebom-cli/src/generate/cyclonedx/
# Expected: (empty)

# No SPDX 2.3 emission-path changes (only const visibility):
git diff main -- mikebom-cli/src/generate/spdx/document.rs | grep -E "^\+" | grep -v "pub(crate)"
# Expected output: just the `+++ b/` header line — no other + lines
# (verifies FR-018 — the SPDX 2.3 emitter's runtime behavior is unchanged)

# No catalog changes:
git diff main --name-only -- docs/
# Expected: (empty)

# No mikebom-common / mikebom-ebpf changes:
git diff main --name-only -- mikebom-common/ mikebom-ebpf/
# Expected: (empty)
```

## Scenario 8 — SC-008 CHANGELOG presence

```bash
sed -n '/^## \[Unreleased\]/,/^## \[v/p' CHANGELOG.md \
  | grep -A2 "simplelicensing_CustomLicense\|SPDX 3.*symmetry\|closes #487"
```

**Expected**: entry present, referencing the SPDX 3 fix + issue #487 + byte-identical cross-format placeholder guarantee.

## Post-merge — operator-cadence external review

The Yocto testbed verification (SC-001 + SC-003) is manual per Assumption 2. The maintainer runs the testbed after merge and reports pass/fail via a follow-up comment on issue #487 (or its close comment). SC-002 / SC-005 / SC-006 / SC-007 / SC-008 are automated and verified pre-merge.

## Known deferrals (spec Out of Scope)

- No real license-text extraction (per FR-017).
- No SPDX 2.3 or CycloneDX changes (per FR-012 + FR-013).
- No `SpdxExpression` newtype changes (per FR-014).
- No milestone-152 rpm_file.rs changes (per FR-015).
- No new `mikebom:*` annotation keys (per FR-016).
- No `expandedlicensing_CustomLicense` (per Assumption 9 + Out of Scope).
- No `DocumentRef-*:LicenseRef-*` handling.
- No retroactive re-emission of pre-milestone-154 SPDX 3 SBOMs.
