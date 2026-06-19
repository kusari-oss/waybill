# Contract: New `sbom-format-mapping.md` C-row

**Driven by**: spec.md FR-002 + FR-008 (US2 stripped-Informational annotation).
**Target file**: `docs/reference/sbom-format-mapping.md` (existing catalog).
**Change shape**: Append one new row to the existing "C-rows" (mikebom:* annotations)
section.

## Row content

```markdown
| `mikebom:assembly-version-informational-stripped` | per-component | CDX 1.6: `components[].properties[]` entry; SPDX 2.3: `packages[].annotations[].comment` carrying `MikebomAnnotationCommentV1` envelope `{ "type": "mikebom:assembly-version-informational-stripped", "value": "<stripped>" }`; SPDX 3: `Annotation` element with `subject` = the package + `annotationType: OTHER` + same envelope | Companion to the existing `mikebom:assembly-version-informational` annotation. Carries the InformationalVersion with everything from the first SemVer §10 build-metadata separator (`+`) onward removed. Justified parity-bridging `mikebom:*`: no CDX/SPDX construct exists for "alternate canonical version representation"; consumers that need to match syft's `+<sha>`-stripping behavior key on this annotation. NOT emitted when no `+` is present in the source InformationalVersion (FR-009). The milestone-131 `is_plausible_version_string` sanity filter is re-applied to the stripped form per FR-010. | milestone 132 (this row) |
```

## Justification clause (per Constitution Principle V v1.4.0)

**Audit of native fields**:

| Format | Construct considered | Verdict |
|---|---|---|
| CDX 1.6 | `components[].version` | NO — single canonical-version field; mikebom already emits the verbatim Informational here. No "alternate representation" slot. |
| CDX 1.6 | `components[].properties[]` | YES — but `properties` is the namespace for parity-bridging `mikebom:*` annotations per the existing C-row convention. Using `properties` IS the emission target; the `mikebom:*` prefix is the namespace marker. |
| SPDX 2.3 | `packages[].versionInfo` | NO — single canonical-version field; same reason as CDX. |
| SPDX 2.3 | `packages[].annotations[]` | YES — same as CDX `properties[]`: the parity-bridging emission slot. |
| SPDX 3 | `software:version` | NO — single canonical-version field. |
| SPDX 3 | `Annotation` element | YES — same as SPDX 2.3 / CDX. |

**Result**: No native construct expresses "alternate canonical version representation".
Therefore a parity-bridging `mikebom:*` annotation is justified. Constitution Principle V's
fifth bullet (audit-and-document mandate) is satisfied by THIS contracts file plus the
C-row addition to `docs/reference/sbom-format-mapping.md`.

## Verification

Once the row is added to `docs/reference/sbom-format-mapping.md`, the existing
`mikebom-cli/tests/parity_catalog_*.rs` integration tests (which read every C-row and
verify the documented emission shape against actual emitted SBOMs) MUST be re-run and
pass:

```sh
cargo +stable test --workspace --test parity_catalog_cdx
cargo +stable test --workspace --test parity_catalog_spdx23
cargo +stable test --workspace --test parity_catalog_spdx3
```

A new fixture under `mikebom-cli/tests/fixtures/parity_catalog/` MUST be added
demonstrating an InformationalVersion containing a `+` separator (the fixture's expected
SBOM contains the stripped annotation) AND another fixture where the Informational
contains no `+` (the fixture's expected SBOM has NO stripped annotation per FR-009).

This contracts file is referenced from `tasks.md` (Phase 2) as the source of truth for
the C-row content; the task that adds the row to the catalog is a verbatim copy-paste
operation.
