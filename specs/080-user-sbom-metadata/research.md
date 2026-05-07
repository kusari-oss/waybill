# Research — milestone 080 user-provided SBOM metadata

Six implementation-level decisions to pin before Phase 1 design. The two highest-impact decisions (Q1 multi-annotation parsing; Q2 CDX fallback strategy) were locked during /speckit.clarify; this document validates them against ground truth (the actual CDX 1.6 schema) and pins the remaining details.

## §1 — CDX 1.6 native annotations audit (the Q2 plan-time deferral)

**Decision**: CDX 1.6 has FULL native annotation support. The Q2 parity-bridge fallback (`mikebom:invocation-comment` / `mikebom:annotation` properties in `metadata.properties[]`) is **NOT triggered**. All three flag families (`--metadata-comment`, `--annotator`/`--annotation-comment`, plus `--creator`'s annotator-shape entries) land at standards-native CDX 1.6 fields.

**Audit method**: Fetched `https://cyclonedx.org/schema/bom-1.6.schema.json` (190KB, 4612 lines) and inspected the relevant definitions. Schema fixture also added to `mikebom-cli/tests/fixtures/schemas/cyclonedx-1.6.json` per the project pattern (mirrors the SPDX 2.3 + SPDX 3 schema fixtures from milestones 011 + 012 + 078).

**Audit results — `bom.annotations[]`**:

```
$.properties.annotations
└── { type: "array", items: { $ref: "#/definitions/annotations" } }

$.definitions.annotations
└── { type: "object", required: [subjects, annotator, timestamp, text],
      additionalProperties: false,
      properties: {
        bom-ref:    optional refType,
        subjects:   array of refLinkType | bomLinkElementType (required, uniqueItems),
        annotator:  object with oneOf [organization, individual, component, service] (required),
        timestamp:  ISO 8601 string (required),
        text:       string (required),
        signature:  optional signatureType
      } }
```

**Implications**:
- `subjects[]` is required and non-empty (uniqueItems). For document-level annotations, `subjects[]` MUST contain at least one bom-ref. mikebom's emission pattern: point `subjects[]` at the root component's `bom-ref` (e.g., `metadata.component.bom-ref`, which mikebom always emits). For SBOMs that lack a root component (rare; e.g., empty scans), point at `metadata.tools[0].bom-ref` of the mikebom auto-populated entry.
- `annotator` is a `oneOf` with exactly one of `organization` / `individual` / `component` / `service`. This routes mikebom's `Type: Name` prefix cleanly:
  - `Organization: <name>` → `annotator.organization = { name: <name> }` ← **PERFECT FIT**
  - `Person: <name>` → `annotator.individual = { name: <name> }` ← **PERFECT FIT**
  - `Tool: <name>` → `annotator.component = { type: "application", name: <name> }` (CDX `component` type-enum permits `application`/`framework`/`library`/etc.; `application` is closest for an SBOM-generation tool). Alternative: `annotator.service = { name: <name> }` for "running service". **Recommend `component` with `type: "application"`** because mikebom-style tools are application-shaped (CLI binaries with an SBOM) more than service-shaped (running daemons).

**Audit results — `metadata.tools[]`**:

```
$.definitions.metadata.properties.tools
└── { type: "object", oneOf [components-array | services-array | legacy-tools],
      properties: {
        components: { type: "array", items: $ref: component },
        services:   { type: "array", items: $ref: service },
        ... }}
```

CDX 1.6 deprecated the flat `tools[]` array in favor of `tools.components[]` + `tools.services[]`. mikebom's existing emission emits `metadata.tools[].name = "mikebom"` per the milestone-078 wire shape — verify whether mikebom uses the legacy or the new shape; align this milestone's additions to whatever the existing path uses.

**Audit results — `metadata.authors[]`**:

```
$.definitions.metadata.properties.authors
└── { type: "array", items: $ref: organizationalContact }

$.definitions.organizationalContact
└── { type: "object", properties: {
        bom-ref: optional refType,
        name:    string,
        email:   string,
        phone:   string
      } }
```

**Maps cleanly** for `Person: <name>` → `metadata.authors[]` append `{ name: <name> }`. Email parsing from `Person: Alice <alice@example.com>` is a nice-to-have; spec says treat the entire `Name` portion as a single string for now.

**Audit results — `metadata.manufacturer`** (CDX 1.6 single-organization slot):

```
$.definitions.metadata.properties.manufacturer
└── { $ref: "#/definitions/organizationalEntity" }
```

Single object (NOT array). CDX permits exactly one manufacturer at the document-metadata level. **First `--creator "Organization: ..."` populates `metadata.manufacturer`; subsequent `Organization:` creators are routed to `bom.annotations[].annotator.organization` instead** (or the next slot per the routing table). Alternatively, route ALL `Organization:` creators to `bom.annotations[]` and leave `metadata.manufacturer` for explicit operator opt-in via a future flag. **Recommend**: single-org → `metadata.manufacturer`; multi-org → first goes to `metadata.manufacturer`, rest emit a stderr warning + go to `bom.annotations[]`. Document this in `--creator`'s clap help text.

**Rationale**: The audit confirms full native parity is achievable. Q2's parity-bridge fallback was a defensive hedge that doesn't fire. mikebom emission stays at standards-native fields, fully satisfying Constitution Principle V's standards-native-precedence requirement. No `mikebom:` properties introduced; no `docs/reference/sbom-format-mapping.md` parity-bridge entries needed.

**Alternatives considered**:
- Use `metadata.properties[]` parity bridges anyway for symmetry with how mikebom emits other operator-supplied identifiers — Rejected: violates Principle V when native fields exist; defeats the milestone's "replace the `jq` recipe with native CLI" goal.
- Route `Tool:` creators to `annotator.service` instead of `annotator.component` — Considered. `service` is for running services; mikebom-style automation tools are more application-shaped (one-shot CLI invocations). `component` with `type: "application"` is the closer semantic fit. Document; reverse if operator feedback contradicts.

## §2 — Per-format creator-prefix routing table (definitive)

**Decision**: The complete `Type: Name` → per-format native field mapping is:

| Type | CDX 1.6 (target field) | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| `Tool: <name>` | `metadata.tools[]` (or `tools.components[]` per the audit-confirmed shape) — append entry with `name`, `version` (omitted unless operator passes via `--metadata-file`'s extended schema in a future milestone) | `creationInfo.creators[]` append `Tool: <name>` verbatim | `@graph` add `Tool` element with `creationInfo: _:creation-info`, `name: <name>`, deterministic `spdxId` |
| `Organization: <name>` (first one) | `metadata.manufacturer` set to `{ name: <name> }` | `creationInfo.creators[]` append `Organization: <name>` verbatim | `@graph` add `Organization` element (Agent subclass, mirrors milestone-078's `mikebom contributors` Organization shape) |
| `Organization: <name>` (subsequent) | `bom.annotations[]` add entry with `annotator.organization.name = <name>`, `subjects = [<root-component-bom-ref>]`, `text = "creator"`, `timestamp = <emission-time>`. **Stderr warning** "CDX permits exactly one `metadata.manufacturer`; additional Organization creators routed to bom.annotations[]". | (same as first — SPDX 2.3 `creators[]` permits unbounded) | (same as first — SPDX 3 `@graph` permits unbounded Organization elements) |
| `Person: <name>` | `metadata.authors[]` append `{ name: <name> }` | `creationInfo.creators[]` append `Person: <name>` verbatim | `@graph` add `Person` element |

**Rationale**: Every cell is a standards-native landing. Stderr warnings on the multi-Organization edge case keep the operator informed without failing the scan.

## §3 — Positional-pair clap parsing strategy

**Decision**: Option (a) from the plan — define `--annotator` and `--annotation-comment` as two parallel `Vec<String>` fields under clap's `ArgAction::Append`, then post-validate.

**Implementation shape**:
```rust
#[arg(long = "annotator", action = clap::ArgAction::Append, value_name = "TYPE: NAME")]
pub annotator: Vec<String>,

#[arg(long = "annotation-comment", action = clap::ArgAction::Append, value_name = "TEXT")]
pub annotation_comment: Vec<String>,
```

**Post-validation logic** (in `binding/user_metadata/annotation.rs::validate_annotator_pairs`):
1. Assert `annotator.len() == annotation_comment.len()` — fail with `"--annotator (count={a}) must be paired 1:1 with --annotation-comment (count={c}); each --annotator MUST be immediately followed by exactly one --annotation-comment"`.
2. clap's `ArgAction::Append` preserves CLI insertion order within each `Vec`, so element-N of `annotator` pairs with element-N of `annotation_comment`.
3. **Order verification**: clap's derive does NOT preserve cross-flag CLI order. To detect the `--annotator A --annotator B --annotation-comment C` failure case, walk `std::env::args()` once at parse time and assert the two flags appear strictly interleaved. **Implementation note**: the order check is a best-effort positional gate — if a user shells out to a wrapper that re-orders args, the final pairing semantics still hold (each annotator paired by index with its comment), so the order check is for early UX-friendly errors rather than a correctness invariant.

**Rationale**: Stays within clap's derive macro — consistent with milestone-073's `--component-id` flag style. Post-validation handles all spec-required failure modes. The `std::env::args()` walk adds minimal complexity (~30 LOC) and produces operator-friendly error messages on the common typo case.

**Alternatives considered**:
- Option (b) custom value_parser consuming both args at parse time — Rejected: requires bypassing clap's derive macro for these two flags, fragmenting the CLI definition style.
- Option (c) bypass clap derive entirely for these flags — Rejected: same reason as (b), with more friction.

## §4 — `--metadata-file` JSON schema design

**Decision**: snake_case field names; all fields optional; `#[serde(deny_unknown_fields)]` for FR-005's unknown-field rejection.

**Final schema** (the one the implementation encodes):

```rust
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataFile {
    #[serde(default)]
    pub creators: Vec<String>,                           // ["Tool: T1", "Organization: O", "Person: P"]
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
    pub type_name: String,                               // "Tool: reviewer"
    pub comment: String,                                 // "Approved 2026-05-07"
}
```

**Example**:
```json
{
  "creators": ["Tool: my-pipeline", "Organization: ACME Corp", "Person: Alice"],
  "annotators": [
    {"type_name": "Tool: reviewer", "comment": "Approved 2026-05-07"},
    {"type_name": "Organization: SecOps", "comment": "PCI scan complete"}
  ],
  "metadata_comment": "Release v1.0.0 of foo/bar",
  "scan_target_name": "foo-bar-release"
}
```

**Rationale**: snake_case matches the Python-pipeline conventions most CI integrations already use; matches mikebom's existing `mikebom sbom enrich` JSON-input convention (verified at audit time — if `enrich` uses kebab-case, switch to match for cross-feature consistency). Optional fields via `#[serde(default)]` let operators populate only the fields they need. `deny_unknown_fields` catches typos (`creator` vs `creators`) at parse time with a clear error.

**Alternatives considered**:
- kebab-case — Rejected: less Python-friendly and inconsistent with internal Rust struct naming.
- Pascal-case (`Creators`, `Annotators`) — Rejected: non-idiomatic JSON.
- Flat structure for annotators (`annotator: ["Tool: T:Comment"]` with delimiter) — Rejected: re-introduces the CLI flag-pair ambiguity in JSON form. Explicit `{type_name, comment}` objects are unambiguous.

## §5 — `--scan-target-name` interaction with milestone-077 `--root-name`

**Decision**: Per-format precedence rules:

| Format | `--root-name` target | `--scan-target-name` target | Both passed: |
|---|---|---|---|
| CDX 1.6 | `metadata.component.name` | `metadata.component.name` (same field) | `--root-name` wins; stderr warning emitted |
| SPDX 2.3 | root `Package.name` | document `name` (top-level on SPDXDocument) | Both honored independently; different fields |
| SPDX 3 | root `software_Package.name` | `software_Sbom.name` | Both honored independently; different fields |

**Implementation note**: the stderr warning fires only when both flags are passed AND CDX is in the format set (since SPDX 2.3 + SPDX 3 don't conflict). The warning text: `"--root-name overrides --scan-target-name for CDX metadata.component.name; SPDX 2.3 / SPDX 3 honor both independently."`

**Rationale**: `--root-name` is the existing milestone-077 flag and operators using it expect it to control CDX root-component naming. `--scan-target-name` is the new, broader-scoped flag for "SBOM document name." Letting `--root-name` win when they collide preserves milestone-077's behavior contract; the stderr warning surfaces the ambiguity to the operator without failing the scan.

**Alternatives considered**:
- Make `--scan-target-name` win and override `--root-name` on CDX — Rejected: breaks milestone-077's contract.
- Fail with an error when both are passed — Rejected: operators may legitimately want different document-level vs root-component names in SPDX 2.3/SPDX 3 and not realize they conflict in CDX. Warning + sensible default is operator-friendlier.

## §6 — Determinism contract for repeatable arrays

**Decision**: Stable insertion order, no sorting. clap preserves CLI insertion order for `ArgAction::Append`; the per-format builders MUST iterate `UserMetadata.creators` and `UserMetadata.annotations` in that order without sorting.

**File-and-flag merge order** (per FR-006): `file_creators + flag_creators` for stable interleaving. If the operator passes `--metadata-file meta.json` (with `creators: ["A", "B"]`) AND `--creator "C"` AND `--creator "D"`, the emitted SBOM's tools/creators lists end with `A, B, C, D` order. Document this in the `--metadata-file` flag's clap help.

**Annotation timestamp determinism**: each annotation's `timestamp` field uses the SBOM emission time (same `creationInfo.created` value mikebom already emits). Across a single emission, all annotations share the same timestamp. Across re-emissions of the same scan, the timestamp may differ if the operator doesn't pin emission time — but mikebom's existing `creationInfo.created` already has this property and operators have established workarounds (env-var pin, fixture freezing).

**Rationale**: Insertion order is the operator's intent — the order they listed creators/annotations on the CLI. Sorting would silently rearrange and confuse operators inspecting the output. Stable + deterministic + matches operator expectations.

**Alternatives considered**:
- Sort creators alphabetically by name — Rejected: violates operator intent. If the operator wants alphabetical, they pass alphabetical CLI args.
- Sort creators by `Type` first, then by name within Type — Rejected: same reason. Plus it would couple sort to the routing decision (Tool/Organization/Person), making the JSON output's order divergent from the CLI input's order.
