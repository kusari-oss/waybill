# Annotation schema contract — milestone 131

**One new `mikebom:*` annotation key.** All other quality-metadata fields milestone 131 introduces
flow through **standards-native** CDX `licenses[]` + `externalReferences[]` and SPDX `licenseDeclared`
+ `externalRefs[]` — no `mikebom:*` annotation needed.

## C96: `mikebom:license-source`

**Scope**: component

**Values** (sorted lex):

- `"package-dir"` — PE/CLR US2a: a `LICENSE` / `LICENSE.txt` / etc. file was found in the assembly's
  parent-directory walk (depth 3) and the first 4 KB are embedded in the component's
  `licenses[].license.text`.
- `"package-dir-no-license"` — PE/CLR US2a: the assembly was probed but no license file was found
  in the 3-level upward walk. The annotation signals "we looked and came up empty" — actionable
  for downstream auditors deciding whether to backfill manually.
- `"pom-xml"` — Maven nested-JAR US2b: the nested JAR's `META-INF/maven/<g>/<a>/pom.xml` carried a
  `<licenses>` element whose contents flowed into the component's `licenses[]` field via the
  existing top-level path's `SpdxExpression::try_canonical`.
- `"registry-required"` — cargo-auditable US2c: the component's `source` is `"crates-io"`; the
  license is published on `https://crates.io/api/v1/crates/<name>/<version>` but not extracted by
  this milestone. Signal for a future deps.dev enrichment milestone.

**Emitted by**: PE/CLR reader (US2a + US2b through the maven path), cargo-auditable per-crate
emission helper at `binary/entry.rs::cargo_auditable_packages_to_entries` (US2c).

**Principle V audit**:

- CDX 1.6 native equivalent? `licenses[].license.text` carries the license payload; `acknowledgement`
  carries "concluded" vs "declared". Neither tracks the EXTRACTION SOURCE (file path, registry URL,
  etc.). **No.**
- SPDX 2.3 native equivalent? `licenseDeclared` vs `licenseConcluded` similarly distinguish
  inferred-vs-declared but not the extraction source. **No.**
- SPDX 3 native equivalent? `software_declaredLicense` + `software_concludedLicense` distinguish
  the same axis but not extraction source. **No.**

**Verdict**: Valid parity-bridging extension per the Principle V "finer-grained information the
standard does not express" carve-out. Useful for audit/forensic review of mikebom's license
coverage — a license claim sourced from a registry API call has different trust properties than
one extracted from a checked-in LICENSE.txt file or a `<licenses>` element in a pom.xml.

## C97: `mikebom:license-text-sha256`

**Scope**: component

**Value**: hex-encoded SHA-256 of the license file's first 4 KB (the same window the FR-013
fingerprint-matcher probes). Emitted ONLY when the file is found but no SPDX-fingerprint match
fires — gives downstream tools a stable identifier for cross-referencing the same license body
across packages.

**Emitted by**: PE/CLR US2a, only on the "found but unrecognized" branch.

**Principle V audit**:

- CDX 1.6 native equivalent? `licenses[].license.text` could carry the body verbatim, but mikebom
  rejects this path (FR-013 + Principle IV — the existing `SpdxExpression` type can't carry
  free-text). The SHA-256 is an out-of-band identifier the standard doesn't have a slot for. **No.**
- SPDX 2.3 native equivalent? `Package.licenseDeclared` accepts `NOASSERTION` / `LicenseRef-<id>`;
  the `hasExtractedLicensingInfo[]` block carries free-text via `extractedText`. mikebom could
  emit a `LicenseRef-<sha-prefix>` + `extractedText` body — that's a meaningful future
  improvement but out-of-scope here (it would push 4 KB of body into every emitted SBOM). **No
  precise per-package-hash native equivalent.**
- SPDX 3 native equivalent? Same shape as SPDX 2.3. **No.**

**Verdict**: Valid parity-bridging extension. Useful for the "we saw something here but couldn't
classify it" audit trail without ballooning SBOM size.

## C98: `mikebom:cargo-vcs-source-url`

**Scope**: component

**Value**: the parsed VCS URL from the cargo-auditable `.dep-v0` section's `source` field, stripped
of the `git+` prefix, the trailing `.git`, and the `#<rev>` fragment. Example:
`"https://github.com/serde-rs/serde"`.

**Emitted by**: cargo-auditable per-crate emission at
`binary/entry.rs::cargo_auditable_packages_to_entries`, only when the `source` field matches
`^git\+(https?://[^#]+?)(\.git)?(#[a-f0-9]+)?$`. The downstream `scan_fs/mod.rs::supplier_from_purl`
helper consumes this annotation to emit a `vcs`-type CDX `externalReferences[]` entry on the
component.

**Principle V audit**:

- CDX 1.6 native equivalent? CDX `externalReferences[].url` with `type = "vcs"` IS the native
  emission path — and milestone 131 uses it. The annotation itself is the in-process **plumbing
  channel** from the cargo-auditable parse site (which knows the source field) to the URL
  synthesis layer (which only sees the PURL). Not a wire-format substitute for the native
  ExternalReference — both ship.
- SPDX 2.3 / 3 native equivalent? `Package.externalRefs[]` with category `OTHER` covers the
  emission. Same as CDX — the annotation is plumbing, the native ExternalReference is the
  wire-format primary.

**Verdict**: Valid parity-bridging extension as the cross-module plumbing channel. The annotation
preserves the **provenance** of the VCS URL — it came from the cargo-auditable build-time `source`
field declaration, not from a heuristic guess against the PURL name (which is what
`supplier_from_purl` does for golang/github). Auditors comparing two SBOMs can verify the
VCS-source claim's grounding by checking this annotation's presence.

## Cross-cutting catalog wiring

The C96 row MUST be:

1. Catalogued in `docs/reference/sbom-format-mapping.md` with the audit narrative above.
2. Registered as a `cdx_anno!`, `spdx23_anno!`, and `spdx3_anno!` entry in
   `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`.
3. Registered as a `ParityExtractor` slice entry in `mikebom-cli/src/parity/extractors/mod.rs`
   with matching `use` imports.
4. Covered by the existing `extractors_table_is_sorted_by_row_id` +
   `every_catalog_row_has_an_extractor` shape tests (no new test added).

## Standards-native fields used (no `mikebom:*` annotation needed)

For reference — these are emitted directly via existing format builders without any new
annotation key:

| Data | CDX 1.6 | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| License text (US2a) | `licenses[].license.text` | `Package.licenseDeclared` + `hasExtractedLicensingInfo[]` | `software_declaredLicense` |
| License expression (US2b) | `licenses[].license.id` or `licenses[].expression` | `Package.licenseDeclared` | `software_declaredLicense` |
| Supplier URL (US3) | `externalReferences[].url` with `type=website` | `Package.externalRefs[]` with category=PACKAGE-MANAGER | `Element.externalRef[]` |
| VCS URL (US3) | `externalReferences[].url` with `type=vcs` | `Package.externalRefs[]` with category=OTHER | `Element.externalRef[]` |

All four are existing-channel reuse — no new emission code in the format builders.
