# Contract — milestone 080 user-provided SBOM metadata

The milestone's only contract.

## CLI surface

**Five new flags** on both `mikebom sbom scan` and `mikebom trace run`:

| Flag | Type | Repeatable | Pairs with | Default |
|---|---|---|---|---|
| `--creator <Type: Name>` | string | yes (`ArgAction::Append`) | none | empty |
| `--annotator <Type: Name>` | string | yes (`ArgAction::Append`) | next `--annotation-comment` | empty |
| `--annotation-comment <text>` | string | yes (`ArgAction::Append`) | preceding `--annotator` | empty |
| `--metadata-comment <text>` | string | no (single-valued) | none | none |
| `--scan-target-name <name>` | string | no (single-valued) | interacts with `--root-name` | auto-derived |
| `--metadata-file <path.json>` | path | no | none | none |

**Type validation** (per VR-080-001 + VR-080-002):
- `<Type>` ∈ `{Tool, Organization, Person}` (case-sensitive). Any other value fails parsing with `"--{flag} 'X: Y' has invalid type 'X'; valid types are Tool, Organization, Person"`.
- `<Name>` is non-empty UTF-8 with no control characters (mirrors milestone-077's `--root-name` validation).

**Pairing validation** (per VR-080-003 + research §3):
- `--annotator` and `--annotation-comment` MUST appear in equal counts. clap's `ArgAction::Append` preserves insertion order; element-N pairs with element-N.
- The CLI parser walks `std::env::args()` once and asserts strict interleaving for early UX-friendly errors on the `--annotator A --annotator B --annotation-comment C` typo case.

## Library surface (`mikebom-cli` crate)

**No new public Rust API.** All new types are `pub(crate)`:

- `crate::binding::user_metadata::Creator { kind: CreatorKind, name: String }`
- `crate::binding::user_metadata::CreatorKind` (enum: `Tool` | `Organization` | `Person`)
- `crate::binding::user_metadata::Annotation { annotator: Creator, comment: String, timestamp: DateTime<Utc> }`
- `crate::binding::user_metadata::MetadataFile` + `MetadataFileAnnotator` (deserialize-only; via `#[derive(Deserialize)]`)
- `crate::binding::user_metadata::UserMetadata { creators, annotations, metadata_comment, scan_target_name }`
- `crate::binding::user_metadata::merge_file_and_flags(...) -> Result<UserMetadata, BuildUserMetadataError>`

## Wire-format contract (per format)

### CDX 1.6 — native fields per Phase 0 §1 audit

The CDX 1.6 schema audit (research §1) confirms full native annotation support. **No `mikebom:` parity bridges introduced.**

Routing matrix (per research §2):

| Source | CDX 1.6 destination |
|---|---|
| `--creator "Tool: <name>"` | `metadata.tools.components[]` (or legacy `metadata.tools[]` per existing emission shape) — append entry with `name`, `type: "application"` |
| `--creator "Organization: <name>"` (1st) | `metadata.manufacturer = { name: <name> }` |
| `--creator "Organization: <name>"` (2nd+) | `bom.annotations[]` with `annotator.organization.name = <name>`, `subjects = [<root-bom-ref>]`, `text = "creator"`, `timestamp = <emission-time>`. Stderr warning emitted. |
| `--creator "Person: <name>"` | `metadata.authors[]` append `{ name: <name> }` |
| `--metadata-comment <text>` | `bom.annotations[]` with `annotator.organization.name = "mikebom contributors"`, `subjects = [<root-bom-ref>]`, `text = <text>`, `timestamp = <emission-time>` |
| `--annotator <Type: Name> --annotation-comment <text>` | `bom.annotations[]` with `annotator.<organization\|individual\|component>` set per Type, `subjects = [<root-bom-ref>]`, `text = <text>`, `timestamp = <emission-time>` |
| `--scan-target-name <name>` | `metadata.component.name = <name>` (`--root-name` takes precedence on conflict; stderr warning) |

### SPDX 2.3 — native fields throughout

All flags land at standards-native SPDX 2.3 fields:

| Source | SPDX 2.3 destination |
|---|---|
| `--creator "Type: Name"` (any Type) | `creationInfo.creators[]` append `Type: Name` verbatim |
| `--metadata-comment <text>` | `creationInfo.comment = <text>` |
| `--annotator <Type: Name> --annotation-comment <text>` | `annotations[]` append `{ annotator: "Type: Name", annotationDate: <emission-time>, annotationType: "OTHER", comment: <text> }` |
| `--scan-target-name <name>` | document-level `name = <name>` (top-level on SPDXDocument; independent of root Package) |

### SPDX 3 — Agent elements + Annotation elements

All flags land at SPDX 3 native graph elements (per the milestone-078 wire shape):

| Source | SPDX 3 destination |
|---|---|
| `--creator "Tool: <name>"` | New `Tool` element in `@graph` with deterministic spdxId; referenced from `CreationInfo.createdUsing[]` |
| `--creator "Organization: <name>"` | New `Organization` element in `@graph` (Agent subclass; mirrors milestone-078's `mikebom contributors`); referenced from `CreationInfo.createdBy[]` |
| `--creator "Person: <name>"` | New `Person` element in `@graph` (Agent subclass); referenced from `CreationInfo.createdBy[]` |
| `--metadata-comment <text>` | New `Annotation` element in `@graph` with `subject = <spdxDocument-iri>`, `annotationType: "other"`, `statement: <text>` |
| `--annotator <Type: Name> --annotation-comment <text>` | New `Annotation` element with `subject = <spdxDocument-iri>`; the annotator references the corresponding Agent element in `@graph` (added if not already present from `--creator`) |
| `--scan-target-name <name>` | `software_Sbom.name = <name>` |

## Determinism contract

- Same flag inputs + same scan inputs → byte-identical SBOMs across re-runs (FR-009).
- Repeatable-array insertion order = file-creators + flag-creators (file first, then flag), preserving operator intent (research §6).
- Annotation timestamps = SBOM emission timestamp (single value across all annotations in one emission; matches `creationInfo.created`).
- New SPDX 3 IRIs use `<doc_iri>/<kind>/<slug>-<hash>` where `<hash>` is BASE32-encoded SHA-256 of `<kind>:<name>` (deterministic per name input).

## Observable contract

### Pre-fix: jq post-processing recipe (the issue body's CNCF example)

Operators today run:
```bash
jq --arg owner "$OWNER" --arg repo "$REPO" --arg tag "$TAG" \
  '.creationInfo.creators += ["Tool: cncf-automation-sbom-generator"] |
   .creationInfo.comment = "SBOM for CNCF project: \($owner)/\($repo)@\($tag)"' \
  input.spdx.json > output.spdx.json
```

Multi-format pipelines need three different `jq` invocations (one per format) because field shapes differ.

### Post-fix: native CLI

```bash
mikebom sbom scan --path . \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --creator "Tool: cncf-automation-sbom-generator" \
  --metadata-comment "SBOM for CNCF project: $OWNER/$REPO@$TAG"
```

Single invocation, three formats, no `jq` post-processing. The metadata lands at the standards-native field in each format.

## Test contract

A new file `mikebom-cli/tests/sbom_user_metadata.rs` MUST cover (per US1+US2+US3+US4 acceptance scenarios):

| Test | Acceptance scenario | Validates |
|------|--------------------|-----------|
| `creator_lands_in_all_three_formats` | US1 §1, SC-001 | FR-001 |
| `multi_creator_appends_additively` | US1 §2 | FR-001 + FR-007 |
| `creator_type_routing_per_format` | US1 §3 + edge cases | FR-001 routing matrix |
| `metadata_comment_lands_in_all_three` | US2 §1-3, SC-002 | FR-002 |
| `annotator_pair_emits_annotation` | US2 §4, SC-003 | FR-003 |
| `multi_annotator_positional_pairing` | Q1 clarification | FR-003 |
| `annotator_without_comment_fails` | US2 §5 | FR-003 |
| `scan_target_name_overrides_default` | US3 §1-3, SC-004 | FR-004 |
| `scan_target_name_root_name_precedence` | US3 + research §5 | FR-004 |
| `metadata_file_loads_correctly` | US4 §1, SC-005 | FR-005 |
| `metadata_file_unknown_field_fails` | US4 §3 | FR-005 + VR-080-004 |
| `metadata_file_malformed_json_fails` | US4 §4 | FR-005 |
| `file_and_flags_merge_arrays` | US4 §2 | FR-006 |
| `file_and_flag_conflict_on_singular_fails` | edge case | FR-006 + VR-080-005 |
| `determinism_byte_identical_across_runs` | (smoke) | FR-009 + SC-009 |
| `spdx3_conformance_with_full_metadata` | SC-008 | FR-010 + milestone-078 SHACL gate |
| `cdx_native_annotations_emit_correctly` | Q2 audit confirmation | FR-008 (native path; Q2 fallback NOT triggered) |

Plus: extending `cdx_regression`, `spdx_regression`, `spdx3_regression` test targets is unnecessary — the new flags are off-by-default; existing goldens regen ONLY when emitted with the new flag values.

## Performance contract

- Mapping: pure-function with O(1) per metadata-flag entry. Negligible vs. JSON serialization wall-time.
- Integration test wall-time: ~25–30s for the new 17-test file (most tests are fast — emission + JSON-parse assertions; `spdx3_conformance_with_full_metadata` is the only one that shells out to `spdx3-validate`).
- Determinism (FR-009): re-running the test against the same flag inputs produces byte-identical results.
