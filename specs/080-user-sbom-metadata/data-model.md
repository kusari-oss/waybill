# Data Model — milestone 080 user-provided SBOM metadata

The milestone introduces four new internal Rust types in `mikebom-cli/src/binding/user_metadata/`, plus extensions to three existing per-format builders. No changes to mikebom's other internal types — `UserMetadata` flows alongside `ScanArtifacts` into each format builder without coupling.

## Internal Rust types (NEW — `mikebom-cli/src/binding/user_metadata/`)

### `Creator` (creator.rs)

```rust
/// A creator/contributor entry on the emitted SBOM. User-supplied via
/// `--creator <Type: Name>` or via `--metadata-file`'s `creators[]` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Creator {
    pub kind: CreatorKind,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorKind {
    Tool,
    Organization,
    Person,
}

impl CreatorKind {
    /// SPDX 2.3 prefix string ("Tool:" / "Organization:" / "Person:").
    pub fn spdx_prefix(self) -> &'static str { /* ... */ }
}

/// Parse a `Type: Name` string. Returns Err if Type isn't in
/// {Tool, Organization, Person} or Name is empty after trimming.
pub fn parse_creator_str(s: &str) -> Result<Creator, ParseCreatorError> { /* ... */ }
```

**Validation rules** (per spec FR-001 + Edge Cases):
- `kind` MUST be one of the three enum variants. Invalid prefixes (`Bot:`, `Service:`, etc.) fail parsing with a clear error.
- `name` MUST be non-empty UTF-8 with no control characters (mirrors milestone-077's `--root-name` validation).
- Whitespace between `:` and `Name` is trimmed (so `"Tool: foo"` and `"Tool:foo"` and `"Tool:   foo"` all parse to the same `Creator`).

### `Annotation` (annotation.rs)

```rust
/// A document-level annotation entry. User-supplied via
/// `--annotator <Type: Name> --annotation-comment <text>` pairs or via
/// `--metadata-file`'s `annotators[]` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub annotator: Creator,
    pub comment: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

**Validation rules** (per spec FR-003 + Edge Cases):
- `annotator` MUST be a successfully-parsed `Creator` (so the same Type-prefix validation applies).
- `comment` MUST be non-empty.
- `timestamp` is set at emission time to the SBOM's `creationInfo.created` value (deterministic per scan inputs; same source as mikebom's existing emission timestamp).

### `MetadataFile` (metadata_file.rs)

```rust
/// Schema for the `--metadata-file <path.json>` sidecar input.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataFile {
    #[serde(default)]
    pub creators: Vec<String>,
    #[serde(default)]
    pub annotators: Vec<MetadataFileAnnotator>,
    #[serde(default)]
    pub metadata_comment: Option<String>,
    #[serde(default)]
    pub scan_target_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataFileAnnotator {
    pub type_name: String,
    pub comment: String,
}
```

**Validation rules** (per spec FR-005):
- All top-level fields optional (each defaults to empty per `#[serde(default)]`).
- Unknown top-level fields rejected at parse time via `#[serde(deny_unknown_fields)]` with a clear error naming the offending field.
- The `creators[]` strings parse via `parse_creator_str` — same validation as the CLI flag.
- The `annotators[].type_name` strings parse via `parse_creator_str` — same validation.

### `UserMetadata` (mod.rs)

```rust
/// Aggregator that the CLI parser populates from the merged
/// file-and-flag inputs. Per-format builders consume this verbatim.
#[derive(Debug, Clone, Default)]
pub struct UserMetadata {
    pub creators: Vec<Creator>,
    pub annotations: Vec<Annotation>,
    pub metadata_comment: Option<String>,
    pub scan_target_name: Option<String>,
}

/// Build a UserMetadata from CLI flags + optional --metadata-file.
/// Per FR-006: file values + flag values merge additively for arrays;
/// single-valued fields (metadata_comment, scan_target_name) fail
/// with a clear conflict error if specified in both file AND flags.
pub fn merge_file_and_flags(
    file: Option<MetadataFile>,
    flag_creators: Vec<String>,        // raw "Type: Name" strings from --creator
    flag_annotators: Vec<String>,      // raw from --annotator
    flag_annotation_comments: Vec<String>, // raw from --annotation-comment
    flag_metadata_comment: Option<String>,
    flag_scan_target_name: Option<String>,
    emission_timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<UserMetadata, BuildUserMetadataError> { /* ... */ }
```

**Merge semantics** (per FR-006 + research §6):
- `creators` = `file.creators` parsed + `flag_creators` parsed, in that order (file first, then flag).
- `annotations` = `file.annotators` mapped + `flag_annotators`/`flag_annotation_comments` paired, in that order (file first, then flag).
- `metadata_comment`: if BOTH `file.metadata_comment` AND `flag_metadata_comment` are `Some`, fail with `ConflictError { field: "metadata_comment", file_value, flag_value }`. Otherwise use whichever is `Some` (or `None`).
- `scan_target_name`: same conflict semantics as `metadata_comment`.

## Wire-format entities — per format

### CDX 1.6 — `metadata.tools` / `metadata.authors` / `metadata.manufacturer` / `bom.annotations`

Per research §1 + §2, all four landing slots are standards-native CDX 1.6 fields. No `mikebom:` parity bridges needed.

```json
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.6",
  "metadata": {
    "component": {
      "name": "<scan_target_name OR root-derived>",
      ...
    },
    "manufacturer": {                                    ← --creator "Organization: <name>" (first)
      "name": "ACME Corp"
    },
    "tools": {
      "components": [
        { "name": "mikebom", "version": "0.1.0-alpha.20", ... },  ← auto-populated
        { "name": "my-pipeline", ... }                  ← --creator "Tool: my-pipeline"
      ]
    },
    "authors": [
      { "name": "Alice" }                                ← --creator "Person: Alice"
    ]
  },
  "annotations": [
    {                                                     ← --metadata-comment "X"
      "subjects": ["<root-component-bom-ref>"],
      "annotator": { "organization": { "name": "mikebom contributors" } },
      "timestamp": "<creationInfo.created>",
      "text": "X"
    },
    {                                                     ← --annotator "Tool: T" --annotation-comment "Y"
      "subjects": ["<root-component-bom-ref>"],
      "annotator": { "component": { "type": "application", "name": "T" } },
      "timestamp": "<creationInfo.created>",
      "text": "Y"
    }
  ],
  "components": [...]
}
```

### SPDX 2.3 — `creationInfo.creators[]` / `creationInfo.comment` / `annotations[]`

```json
{
  "spdxVersion": "SPDX-2.3",
  "name": "<scan_target_name OR auto-derived>",
  "creationInfo": {
    "created": "<timestamp>",
    "creators": [
      "Tool: mikebom-0.1.0-alpha.20",                    ← auto-populated
      "Tool: my-pipeline",                                ← --creator "Tool: my-pipeline"
      "Organization: ACME Corp",                          ← --creator "Organization: ACME Corp"
      "Person: Alice"                                     ← --creator "Person: Alice"
    ],
    "comment": "X"                                        ← --metadata-comment "X"
  },
  "annotations": [
    {                                                     ← --annotator "Tool: T" --annotation-comment "Y"
      "annotator": "Tool: T",
      "annotationDate": "<creationInfo.created>",
      "annotationType": "OTHER",
      "comment": "Y"
    }
  ],
  "packages": [...]
}
```

### SPDX 3 — Agent elements + `Annotation` elements

```json
{
  "@context": "...",
  "@graph": [
    { "type": "Organization", "spdxId": ".../agent/mikebom-contributors", "name": "mikebom contributors", ... },  ← milestone 078
    { "type": "Tool", "spdxId": ".../tool/mikebom", "name": "mikebom-0.1.0-alpha.20", ... },                    ← milestone 078
    { "type": "Tool", "spdxId": ".../tool/my-pipeline-<hash>", "name": "my-pipeline", ... },                    ← --creator "Tool: my-pipeline"
    { "type": "Organization", "spdxId": ".../org/acme-corp-<hash>", "name": "ACME Corp", ... },                 ← --creator "Organization: ACME Corp"
    { "type": "Person", "spdxId": ".../person/alice-<hash>", "name": "Alice", ... },                            ← --creator "Person: Alice"
    {
      "type": "CreationInfo",
      "@id": "_:creation-info",
      "createdBy": [
        ".../agent/mikebom-contributors",
        ".../org/acme-corp-<hash>",
        ".../person/alice-<hash>"
      ],
      "createdUsing": [
        ".../tool/mikebom",
        ".../tool/my-pipeline-<hash>"
      ],
      ...
    },
    { "type": "SpdxDocument", ..., "name": "<scan_target_name>", ... },
    {
      "type": "Annotation",                              ← --metadata-comment "X"
      "spdxId": ".../annotation/metadata-comment",
      "creationInfo": "_:creation-info",
      "subject": ".../<spdxDocument-iri>",
      "annotationType": "other",
      "statement": "X"
    },
    {
      "type": "Annotation",                              ← --annotator "Tool: T" --annotation-comment "Y"
      "spdxId": ".../annotation/T-<hash>",
      "creationInfo": "_:creation-info",
      "subject": ".../<spdxDocument-iri>",
      "annotationType": "other",
      "statement": "Y"
    }
  ]
}
```

**SPDX 3 IRI scheme** for new Agent + Annotation elements: `<doc_iri>/<kind>/<slug>-<hash>` where `<slug>` is a deterministic kebab-case version of the name (e.g., "ACME Corp" → "acme-corp") and `<hash>` is a short BASE32 of SHA-256(`<kind>:<name>`) per the existing milestone-078 IRI-derivation pattern. `<kind>` ∈ `{tool, org, person, annotation}`. The hash suffix prevents IRI collisions when two operators pass the same `name` value.

## Validation rules

- **VR-080-001**: `parse_creator_str("<input>")` MUST accept exactly the three prefixes `{Tool, Organization, Person}` (case-sensitive). Any other prefix returns `ParseCreatorError::InvalidPrefix`.
- **VR-080-002**: The Name portion (after `:` + optional whitespace) MUST be non-empty UTF-8 with no control characters. Empty Name returns `ParseCreatorError::EmptyName`.
- **VR-080-003**: `validate_annotator_pairs(annotator: &[String], comment: &[String])` MUST return `Err` if `annotator.len() != comment.len()` with a clear "X annotator(s) but Y comment(s)" message.
- **VR-080-004**: `MetadataFile::deserialize` MUST reject unknown top-level fields via `#[serde(deny_unknown_fields)]`. Unknown nested fields in `annotators[]` MUST also fail.
- **VR-080-005**: `merge_file_and_flags` MUST fail with a conflict error when BOTH `file.metadata_comment` AND `flag_metadata_comment` are `Some`. Same for `scan_target_name`.
- **VR-080-006**: All emitted SBOMs MUST pass schema validation in their respective format: CDX 1.6 schema (the new fixture at `mikebom-cli/tests/fixtures/schemas/cyclonedx-1.6.json`), SPDX 2.3 schema, SPDX 3 schema, AND the milestone-078 `spdx3-validate` SHACL gate.
- **VR-080-007**: Emission MUST be deterministic — same `UserMetadata` + same scan inputs → byte-identical SBOMs across re-runs. Repeatable-array order = file-creators + flag-creators (insertion order preserved per research §6).
- **VR-080-008**: Per research §1, the multi-Organization edge case (operator passes 2+ `--creator "Organization: ..."`) emits a stderr warning and routes the second-and-beyond Organization creators to `bom.annotations[].annotator.organization` instead of `metadata.manufacturer`.

## Backward compatibility

- **No new `Cargo.toml` deps**: `chrono` is already in the workspace dependency closure (used for `creationInfo.created` timestamps).
- **No MSRV change**: stable Rust toolchain per workspace.
- **No nightly required**: pure user-space CLI + emission code.
- **All 27 existing byte-identity goldens regenerate** as the expected operator-visible change of the milestone, BUT only when the new flags are populated. Pre-fix invocations without any of the new flags MUST produce byte-identical output to alpha.20. Per-fixture diff size: zero when no new flags are passed; bounded to the new metadata fields when they are.
- **Downstream operators** can adopt the new flags incrementally — pre-flag invocations continue to work unchanged. The `jq` post-processing recipe operators currently use stays valid as a transitional step until they migrate.
