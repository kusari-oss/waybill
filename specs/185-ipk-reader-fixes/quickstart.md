# Quickstart: ipk reader bug fixes (m185)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Operator flow

### Scenario 1 — Scanning a Yocto image with kernel modules (US1)

A `core-image-minimal` Yocto build for `qemux86-64` (scarthgap) produces kernel-module ipks with BitBake `SRCPV`-expanded versions:

```
kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard_6.6.127+git0+45f69741c7_70af2998be-r0_qemux86_64.ipk
```

**Before m185**: mikebom emitted:

```json
{
  "name": "kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard_6.6.127+git0+45f69741c7_70af2998be-r0_qemux86_64.ipk",
  "version": "",
  "purl": null
}
```

**After m185**: mikebom emits:

```json
{
  "name": "kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard",
  "version": "6.6.127+git0+45f69741c7_70af2998be-r0",
  "purl": "pkg:opkg/kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard@6.6.127%2Bgit0%2B45f69741c7_70af2998be-r0?arch=qemux86_64"
}
```

Downstream tooling can now identify the component by PURL, run vulnerability lookups against `pkg:opkg/kernel-module-...`, and cross-reference against Yocto's own SPDX rollup.

### Scenario 2 — License extraction on a Yocto opkg-status scan (US2)

Yocto builds populate `/var/lib/opkg/status` with stanzas including a `License:` field mirroring the recipe LICENSE variable:

```
Package: busybox
Version: 1.36.1-r0
Architecture: core2-64
License: GPLv2 & bzip2-1.0.4
Depends: libc6
```

**Before m185**: mikebom emitted the busybox component with `licenses: []` on CDX and `licenseDeclared: "NOASSERTION"` on SPDX 2.3. All 4586 components in a stock scan showed the same wholesale absence.

**After m185**: mikebom normalizes `GPLv2 & bzip2-1.0.4` through the 4-pass pipeline:
1. Pass 1 normalizes `&` → `AND`: `"GPLv2 AND bzip2-1.0.4"`
2. Pass 2 (strict SPDX) fails: `GPLv2` isn't a canonical id
3. Pass 3 wraps unknown operands: `"GPL-2.0-only AND LicenseRef-bzip2-1.0.4"` — canonicalizes successfully

The emitted CDX 1.6 component now shows:

```json
{
  "name": "busybox",
  "version": "1.36.1-r0",
  "licenses": [
    { "license": { "acknowledgement": "declared", "id": "GPL-2.0-only" } },
    { "license": { "acknowledgement": "declared", "name": "LicenseRef-bzip2-1.0.4" } }
  ]
}
```

And the SPDX 2.3 output:

```json
{
  "name": "busybox",
  "licenseDeclared": "GPL-2.0-only AND LicenseRef-bzip2-1.0.4",
  "hasExtractedLicensingInfos": [
    { "licenseId": "LicenseRef-bzip2-1.0.4", "extractedText": "bzip2-1.0.4" }
  ]
}
```

## Filter parity for license audits

Post-m185, a license auditor can query all installed opkg components carrying a specific known SPDX id:

```bash
# Before m185: zero results — no license identifiers were emitted
mikebom sbom scan --path /yocto-rootfs --format cyclonedx-json | \
    jq '[.components[] | select(.licenses[]?.license.id == "GPL-2.0-only") | .name] | length'
# → 0

# After m185: accurate coverage
mikebom sbom scan --path /yocto-rootfs --format cyclonedx-json | \
    jq '[.components[] | select(.licenses[]?.license.id == "GPL-2.0-only") | .name] | length'
# → ~significant fraction (varies by image, but >0 of 4586)
```

## Behavior on unparseable License strings (m185 FR-014)

If a Yocto recipe has a corrupt LICENSE variable that produces a stanza like:

```
Package: broken-example
License: !!! bad syntax &&& random
```

**Before m185**: `licenses: []` (wholesale absence, same as no License field).

**After m185**: mikebom's 4th-pass wholesale-wrap fires. The emitted component carries a single LicenseRef:

```json
{
  "name": "broken-example",
  "licenses": [
    { "license": { "acknowledgement": "declared", "name": "LicenseRef-------bad-syntax-------random" } }
  ]
}
```

mikebom ALSO emits a `tracing::warn!` log entry so operators auditing scan logs can see which stanzas hit this fallback:

```
WARN mikebom::scan_fs::package_db::opkg: opkg License string failed strict + per-operand SPDX parse; wholesale-wrapped as LicenseRef per m185 FR-014
  source_path = /yocto-rootfs/var/lib/opkg/status
  package = broken-example
  raw_license = !!! bad syntax &&& random
  wrapped = LicenseRef-------bad-syntax-------random
```

This preserves the raw string for downstream review (the sanitized form is a deterministic transform of the original), rather than dropping the data entirely.

## Precedence rules (operator-visible)

- **US1 filename fallback**: fires ONLY when the ipk archive-extraction path (`ipk_file.rs:455`) fails to open or read the archive. If the archive IS readable, mikebom uses the control-stanza License extraction (unchanged since m169) — which reads License directly from the archive's control.tar.gz. Filename fallback is a last resort.
- **US2 License precedence**: `stanza.license()` is the SOLE source. mikebom does NOT cross-reference recipe files or Yocto's own SPDX output — the opkg-status stanza is authoritative for what mikebom emits.
- **rpm reader unchanged**: any rpm-side scan produces byte-identical output pre- vs post-m185. FR-011 preservation invariant.

## Failure modes

There are none new in m185 — both fixes are additive on the read path:

- Malformed ipk filenames continue to return None from `parse_ipk_filename` (fail-safe preserved).
- Absent License fields continue to emit `licenses: []` (regression pin FR-007).
- Whitespace-only License fields treated as absent.
- Wholly unsanitizable License strings (containing no `[A-Za-z0-9.-]` characters at all — a purely-symbol string) fall through to `licenses: []` via the 4th-pass defensive guard.

## Developer flow — verifying an m185 emission

```bash
# US1 filename fix — verify the multi-underscore version case
jq '.components[] | select(.name | contains("kernel-module-nf-conntrack-tftp"))' mikebom.cdx.json

# Expected post-m185 (US1):
# {
#   "name": "kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard",
#   "version": "6.6.127+git0+45f69741c7_70af2998be-r0",
#   "purl": "pkg:opkg/..."
# }

# US2 license fix — verify component-level license extraction
jq '.components[] | select(.name == "busybox") | .licenses' mikebom.cdx.json

# Expected post-m185 (US2):
# [
#   { "license": { "acknowledgement": "declared", "id": "GPL-2.0-only" } },
#   { "license": { "acknowledgement": "declared", "name": "LicenseRef-bzip2-1.0.4" } }
# ]

# SPDX 2.3 hasExtractedLicensingInfos sweep verification
jq '.hasExtractedLicensingInfos[] | select(.licenseId | startswith("LicenseRef-bzip2"))' mikebom.spdx.json
```

## When NOT to expect the fix

- **Non-Yocto opkg users** — mikebom scans an opkg-managed system (OpenWrt, Yocto) look at the SAME `/var/lib/opkg/status` shape. The fix applies to both. OpenWrt operators get identical benefit.
- **rpm-based Yocto distros** — Yocto with RPM_PACKAGES enabled uses the rpm reader path, which is unchanged by m185. Rpm users continue to see the pre-m185 rpm behavior byte-identically (already correctly extracting licenses via the existing 3-pass pipeline).
- **DPKG (Debian / Ubuntu)** — different reader (`deb.rs`); m185 does not touch it.
- **Legacy ar-format .ipk files** (pre-2015 opkg-build) — mikebom already falls through to filename-only per research §R2b (from m169). License extraction was never possible for those files (no readable archive contents). This is unchanged by m185 (deferred per spec.md's Deferred to Future Milestones).

## Cross-references

- Spec: [spec.md](./spec.md)
- Plan: [plan.md](./plan.md)
- Parser decision matrix: [contracts/parser-decision-matrix.md](./contracts/parser-decision-matrix.md)
- License pipeline contract: [contracts/license-pipeline.md](./contracts/license-pipeline.md)
- Research decisions: [research.md](./research.md)
- Original issue reports: [#538 (filename fallback)](https://github.com/kusari-oss/mikebom/issues/538) · [#539 (license extraction)](https://github.com/kusari-oss/mikebom/issues/539)
