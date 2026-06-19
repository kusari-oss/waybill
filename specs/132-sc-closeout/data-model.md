# Data Model: Close milestone-131 SC misses

**Date**: 2026-06-19
**Branch**: `132-sc-closeout`

This milestone is a metadata-emission closeout. The "data model" is the existing
`ResolvedComponent` plus a handful of new annotation keys / lookup-table entries /
enrichment-source dispatch arms. Each entity below names exactly where in the source
tree it lives, what fields it gains or reads, what validation rules apply, and which
functional requirement / user story drives it.

## Entity: PURL-ecosystem → Supplier-name lookup table (US1)

**Source location**: New module-level `const` in `mikebom-cli/src/scan_fs/mod.rs`
immediately above the existing `supplier_from_purl` function at line 572.
**Driven by**: FR-005, FR-006, FR-007.

### Shape

```rust
const SUPPLIER_TABLE: &[(&str, &str)] = &[
    ("cargo", "crates.io"),
    ("nuget", "nuget.org"),
    ("maven", "Maven Central"),
    ("npm", "npmjs.com"),
    ("pypi", "PyPI"),
    ("gem", "RubyGems"),
    ("apk", "Alpine Package Maintainer"),
    ("deb", "Debian Package Maintainer"),
    ("rpm", "RPM Package Maintainer"),
    // `golang` deliberately omitted: the existing supplier_from_purl heuristic
    // for github.com/gitlab.com/etc. PURL hosts is preserved unchanged.
];
```

### Lookup rule

`supplier_from_purl(purl)` resolves in this priority order (extending the existing
function, not replacing it):

1. Existing reader-populated `entry.maintainer` (set by readers like apk's APKINDEX
   Maintainer field) — WINS over the lookup table per FR-006. Already handled at
   `scan_fs/mod.rs:572` via the `entry.maintainer.clone().or_else(...)` chain;
   milestone 132 inserts at the `.or_else` position.
2. `Purl::ecosystem()` lookup against `SUPPLIER_TABLE`. Returns `Some(supplier)` on hit.
3. Existing golang host-heuristic at lines 580–610 — preserved.
4. `None` → emitted CDX `supplier.name` absent, SPDX 2.3 `Package.originator` absent,
   SPDX 3 `software:supplier` absent.

### Validation

- Static at compile time: every `(ecosystem, name)` pair is `&'static str`. Compiler
  enforces.
- At runtime: PURL ecosystem string is the canonical lowercase form returned by
  `mikebom_common::types::purl::Purl::ecosystem()` (verified at compile time per
  Constitution Principle IV — no raw `String` boundaries).

### State transitions

None. Static lookup; component identity drives a single lookup that either hits or
misses.

### Emission targets

| Format | Field | Source |
|---|---|---|
| CycloneDX 1.6 | `components[].supplier.name` (per-component) AND `metadata.supplier.name` (document-level via existing milestone-001 plumbing) | Looked-up value or null |
| SPDX 2.3 | `packages[].originator` formatted as `"Organization: <value>"` per SPDX 2.3 §7.7 | Looked-up value |
| SPDX 3 | `software:supplier` as a string `Organization` element reference | Looked-up value |

## Entity: Stripped-Informational version annotation (US2)

**Source location**: New annotation emission in
`mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs` inside the existing
`extract_custom_attribute_versions` function (which milestone 131 PR #377 renamed from
`walk_custom_attributes`). Emission point: after the
`mikebom:assembly-version-informational` annotation is populated, before the
`AssemblyAccumulator` dedup pass.
**Driven by**: FR-008, FR-009, FR-010, FR-011.

### Shape

A new key in the existing per-component `extra_annotations: serde_json::Map<String,
Value>`:

```json
{
  "mikebom:assembly-version-informational": "4.8.0-7.25569.25+38896ab4abcdef0123456789",
  "mikebom:assembly-version-informational-stripped": "4.8.0-7.25569.25"
}
```

### Derivation rule

```text
Input:  raw InformationalVersion string V_full
Output: optional stripped string V_strip

1. If V_full contains no '+' character → emit no stripped annotation (skip).
2. Let V_strip = V_full split-once on '+', take the part before '+'.
3. If V_strip fails is_plausible_version_string sanity filter (the milestone-131
   US3 Phase A filter) → emit no stripped annotation (skip silently).
4. Otherwise emit mikebom:assembly-version-informational-stripped = V_strip.
```

### Validation

- FR-009: stripped annotation MUST NOT be emitted when no `+` separator present.
- FR-010: stripped form MUST re-run `is_plausible_version_string`. If the original
  Informational passed sanity but the stripped form does not (e.g. the original was
  `5.0+something-x.y.z` where the prefix `5.0` is fine but the full string was the
  sanity-passing one — rare but possible), the stripped form is silently dropped per
  the maintainer's accuracy preference (Principle IX).

### State transitions

None per scan; one annotation per Informational version present.

### Emission targets

| Format | Field | Source |
|---|---|---|
| CycloneDX 1.6 | `components[].properties[]` entry `{name: "mikebom:assembly-version-informational-stripped", value: V_strip}` | Derivation |
| SPDX 2.3 | `packages[].annotations[].comment` with envelope `{type: "mikebom:assembly-version-informational-stripped", value: V_strip}` per the milestone-071 annotation comment shape | Derivation |
| SPDX 3 | `Annotation` element with `subject` = the package + `annotationType: OTHER` + same envelope shape | Derivation |

### Catalog row

A single C-row addition to `docs/reference/sbom-format-mapping.md` documenting this
parity-bridging `mikebom:*` annotation per Constitution Principle V's mandate. See
`contracts/sbom-format-mapping-row.md` for the exact row content.

## Entity: License-enrichment dispatch (US3 Path A — fingerprint table extension)

**Source location**: Existing constant table inside
`mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs`. Milestone 131 PR #375 introduced
this table at the head of the LICENSE.txt fingerprint matcher with 6 entries.
**Driven by**: FR-013 (Path A complement).

### Shape

Extension of the existing const table:

```rust
const LICENSE_FINGERPRINT_TABLE: &[(&str, &[u8])] = &[
    // Existing milestone-131 entries:
    ("Apache-2.0", b"<embedded canonical first 64 bytes of Apache-2.0 LICENSE>"),
    ("MIT",        b"<...>"),
    ("BSD-3-Clause", b"<...>"),
    ("BSD-2-Clause", b"<...>"),
    ("GPL-3.0",    b"<...>"),
    ("GPL-2.0",    b"<...>"),
    // NEW (milestone 132):
    ("MS-PL",      b"<embedded canonical Microsoft Public License first 64 bytes>"),
    ("LGPL-2.1-only", b"<...>"),
    ("LGPL-3.0-only", b"<...>"),
    ("LGPL-2.1-or-later", b"<...>"),
    ("MIT-0",      b"<...>"),
    ("EPL-1.0",    b"<...>"),
    ("EPL-2.0",    b"<...>"),
];
```

### Match rule

(Unchanged from milestone 131): a PE/CLR-emitted component's first 64 bytes of any
embedded LICENSE.txt are compared byte-for-byte against each table entry; on hit the
SPDX ID is emitted as `licenses[].license.id`.

### Validation

SPDX IDs are validated against the existing
`mikebom_common::types::license::SpdxExpression::try_canonical` helper at module-load
time via a `#[test]` in the file (matches milestone 131's existing test pattern). A
typo in a new SPDX ID fails the unit test, NOT a production-time scan.

## Entity: License-enrichment dispatch (US3 Path C — deps.dev cargo support)

**Source location**: Extension of the existing milestone-012 scaffolding at
`mikebom-cli/src/enrich/depsdev_source.rs`. Milestone 012 wired nuget already; this
milestone adds the `pkg:cargo` arm to the `match purl.ecosystem()` dispatch.
**Driven by**: FR-013 (Path C primary), FR-014.

### Shape

```rust
fn depsdev_endpoint(ecosystem: &str, name: &str, version: &str) -> Option<String> {
    match ecosystem {
        "cargo" => Some(format!(
            "https://api.deps.dev/v3/systems/CARGO/packages/{}/versions/{}",
            urlencode(name), urlencode(version)
        )),
        "nuget" => Some(format!(           // existing milestone-012 arm
            "https://api.deps.dev/v3/systems/NUGET/packages/{}/versions/{}",
            urlencode(name), urlencode(version)
        )),
        _ => None,
    }
}
```

### Validation

- Network call is conditional on `--offline=false` AND on the new
  `--enrich-licenses=depsdev` flag (off by default per Principle III "Fail Closed" —
  operators explicitly opt in to network access).
- Response is deserialized into the existing `DepsDevLicense` newtype; the response's
  `spdxExpression` field passes
  `mikebom_common::types::license::SpdxExpression::try_canonical` before being attached
  to the component (rejecting malformed responses; Principle IX accuracy).
- HTTP errors fall through to the existing milestone-012 retry / cache /
  transparency-annotation paths.

### Emission targets

| Format | Field |
|---|---|
| CycloneDX 1.6 | `components[].licenses[].license.id = <canonical SPDX expression>` AND `properties[]` entry `{name: "mikebom:license-source", value: "depsdev"}` |
| SPDX 2.3 | `packages[].licenseConcluded = <canonical SPDX expression>` AND existing milestone-012 annotation envelope |
| SPDX 3 | `software:declaredLicense` with `LicenseExpression` element |

## Entity: Milestone-131 spec amendment (US4)

**Source location**: `specs/131-quality-metadata-backfill/spec.md` (an existing file
modified in place).
**Driven by**: FR-015, FR-016, SC-007.

### Shape

Two distinct surgical edits per FR-015 / FR-016:

**FR-015 — Each SC line gains an appended `**Status**:` clause**

Before:

```markdown
- **SC-001**: Weighted sbom-comparison score MUST exceed syft by ≥0.5 on the audit image.
```

After:

```markdown
- **SC-001**: Weighted sbom-comparison score MUST exceed syft by ≥0.5 on the audit image.
  **Status (2026-06-19)**: NOT MET. Measured post-milestone-131 score is syft + 0.1.
  Deferred to milestone 132 SC-001 at a revised +0.4 target.
```

**FR-016 — New section appended**

```markdown
## Post-Milestone Outcomes (2026-06-19)

Documented honestly after the milestone-131 PRs (#374, #375, #376, #377) landed and
the audit baseline was re-measured against the pinned image digest.

### Measured scorecard

| SC | Target | Measured | Status | Disposition |
|---|---|---|---|---|
| SC-001 | syft + 0.5 | syft + 0.1 | NOT MET | Deferred to 132 SC-001 (revised +0.4) |
| SC-002 | VERSION_MISMATCH < 20 | 374 | NOT MET | Deferred to 132 SC-002 (revised <50) |
| SC-003 | License Coverage ≥3/5 | 2/5 | NOT MET | Deferred to 132 SC-003 |
| SC-004 | Supplier Attribution ≥3/5 | 2/5 | NOT MET | Deferred to 132 SC-004 |
| SC-005 | Byte-identity goldens preserved | yes | MET | — |
| SC-006 | Scan-time growth <30 % | yes (~5 %) | MET | — |

### What the milestone-131 implementation actually delivered

- PR #374: cargo+nuget+maven `externalReferences[].url` synthesis. Lifted PURL Quality
  to 3/5 ✅ but did not affect Supplier Attribution.
- PR #375: PE/CLR LICENSE.txt fingerprint matcher with 6 SPDX IDs. Lifted nuget license
  coverage from 0 to 339/819 ✅ but only 11.6 % of overall components affected → License
  Coverage stayed at 2/5.
- PR #376: cargo-auditable `--skip-secondary-evidence` gate removal — surfaced 1058
  cargo components correctly. Necessary plumbing but no scorecard movement on its own.
- PR #377: PE/CLR CustomAttributes walker for `AssemblyInformationalVersion`. Surfaced
  the structural disagreement with syft (semver build-metadata suffix) — VERSION_MISMATCH
  count stayed at 374 because the cause is semantic, not a parser bug. This is the SC-002
  premise correction that motivates milestone 132's revised <50 target.

### Why "complete" was declared prematurely

The implementing AI declared each PR's user story complete after the PR merged,
treating PR-landing as SC-evidence. The structural fix is in milestone 132 — see
spec.md §Honest accounting clauses. Future milestones MUST verify SC measurements
against the audit baseline before claiming closure.
```

### Validation

- FR-015 edit MUST preserve the original target text. The amendment APPENDS the
  `**Status**:` clause; it does NOT replace the bullet.
- FR-016 section MUST cite the specific PR numbers (#374, #375, #376, #377) and the
  measured EffectiveRate / VERSION_MISMATCH / weighted score values pulled from the
  re-measured pinned-digest scorecard.
- The two edits are landed in the same PR that lands the milestone-132 implementation
  work (US1 + US2 + US3) so SC-007 has a single closure commit.

## Entity relationships

```text
ResolvedComponent (existing)
  ├── purl: Purl                        (typed; drives all dispatch)
  ├── supplier: Option<String>          (US1 sets via SUPPLIER_TABLE lookup)
  ├── extra_annotations: serde_json::Map
  │     ├── mikebom:assembly-version-informational         (existing, milestone 131)
  │     ├── mikebom:assembly-version-informational-stripped (US2, NEW)
  │     └── mikebom:license-source                         (existing, milestone 012)
  └── licenses: Vec<License>            (US3 Path A sets via fingerprint; US3 Path C
                                        sets via deps.dev)

SbomDocument (existing)
  └── components: Vec<ResolvedComponent>    (US1, US2, US3 all operate per-element)

Milestone131Spec (a markdown document)
  ├── SC-001 .. SC-004 lines              (US4 appends Status clause)
  └── §Post-Milestone Outcomes            (US4 NEW section)
```

No new persistent state. No state machines. Everything is per-scan derivation from
already-typed inputs.
