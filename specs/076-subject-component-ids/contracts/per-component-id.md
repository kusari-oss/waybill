# Contract — milestone 076 per-component user-defined identifiers

Public API for the new `--component-id` flag.

## CLI surface

### New flag (on both `mikebom sbom scan` and `mikebom trace run`)

```
--component-id <PURL>=<scheme>:<value>
```

- Type: repeatable (`Vec<ComponentIdentifierFlag>` in the parsed `Args` after CLI-level parse via the new `component_id::ComponentIdentifierFlag::parse` value parser)
- LHS (`<PURL>`): exact PURL string for byte-identical match against `components[].purl` per research §5
- RHS scheme: any string passing milestone 073's `SchemeName` regex (`^[a-z][a-z0-9_-]*$`), excluding the five built-in names (`repo`, `git`, `image`, `attestation`, `subject`)
- RHS value: any non-empty string passing milestone 073's `IdentifierValue` rules

### Help-text shape (clap-derived)

```
--component-id <PURL>=<SCHEME>:<VALUE>
        Attach a user-defined identifier to a specific component in the
        emitted SBOM. The PURL must byte-equal a component's `purl` field
        in the emitted output; the SCHEME must be a non-built-in scheme
        name (built-in schemes `repo`, `git`, `image`, `attestation`,
        `subject` are reserved for document-level use). Examples:

          --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2"
          --component-id "pkg:cargo/myapp@0.5.1=acme-asset:myapp-prod-001"

        Repeatable. If a selector PURL matches multiple components
        (same PURL across different bom-ref values), the identifier is
        attached to ALL matching components. If a selector matches zero
        components, the scan logs a warning and continues.
```

## Library surface (`mikebom-cli` crate)

### New module `component_id`

```rust
// In mikebom-cli/src/binding/identifiers/component_id.rs (NEW)

pub struct ComponentIdentifierFlag {
    pub selector_purl: String,
    pub scheme: SchemeName,
    pub value: IdentifierValue,
}

impl ComponentIdentifierFlag {
    /// Parse a flag value of form `PURL=scheme:value`.
    /// Splits on the FIRST `=` (LHS = selector, RHS = scheme:value).
    /// Splits the RHS on the FIRST `:` (scheme on left, value on right).
    /// Rejects built-in scheme names per FR-009.
    /// Used as a clap `value_parser`.
    pub fn parse(raw: &str) -> Result<Self, ComponentIdentifierFlagError>;
}

// pub fn for clap::value_parser
pub fn parse_component_id_flag(raw: &str) -> Result<ComponentIdentifierFlag, String>;
```

### Updated `ScanArtifacts` shape

```rust
// In mikebom-cli/src/generate/mod.rs

pub struct ScanArtifacts<'a> {
    // ... existing fields ...
    /// Milestone 076: per-component user-defined identifiers from
    /// --component-id flags. Threaded to per-format emitters which
    /// match selector_purl against emitted components.
    pub component_identifiers: Vec<ComponentIdentifierFlag>,
}
```

## Integration boundary

### Where flags are parsed

```rust
// In ScanArgs / RunArgs:
#[arg(
    long = "component-id",
    value_name = "PURL=SCHEME:VALUE",
    action = clap::ArgAction::Append,
    value_parser = parse_component_id_flag,
)]
pub component_id: Vec<ComponentIdentifierFlag>,
```

### Where matching happens

Per-format emitters consume `ScanArtifacts.component_identifiers: &[ComponentIdentifierFlag]`. Each format emitter does:

1. After emitting the `components[]` array (or per-format equivalent), iterate through the supplied `component_identifiers`.
2. For each flag, find all components whose emitted `purl` byte-equals `flag.selector_purl`.
3. For each matching component, append the identifier to the component's per-format native carrier (CDX `properties[]`, SPDX 2.3 `externalRefs[PERSISTENT-ID]`, SPDX 3 `externalIdentifier[]`).
4. After all flags are processed, sort each component's NEW entries lexically by `(scheme, value)`. Pre-existing properties/externalRefs are preserved at their original positions.
5. After all components are processed, emit a `tracing::warn!` for any flag whose `selector_purl` matched zero components.

The emitter logic must be identical across CDX / SPDX 2.3 / SPDX 3 modulo the per-format carrier name. Phase 1 may extract a small shared helper.

## Per-format wire mapping (per research §2)

### CDX 1.6

`components[].properties[]` with `name = <scheme>` and `value = <value>`:

```json
{
  "type": "library",
  "name": "serde",
  "version": "1.0.0",
  "purl": "pkg:cargo/serde@1.0.0",
  "properties": [
    {"name": "kusari-id", "value": "asset-shared-lib-v2"},
    {"name": "acme-asset", "value": "shared-001"}
  ]
}
```

Pre-existing properties (e.g., `mikebom:not-linked`) preserved at original positions; new entries appended in lexical order.

### SPDX 2.3

`Package.externalRefs[]` with `referenceCategory = "PERSISTENT-ID"`:

```json
{
  "name": "serde",
  "versionInfo": "1.0.0",
  "externalRefs": [
    {"referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl", "referenceLocator": "pkg:cargo/serde@1.0.0"},
    {"referenceCategory": "PERSISTENT-ID", "referenceType": "kusari-id", "referenceLocator": "asset-shared-lib-v2"},
    {"referenceCategory": "PERSISTENT-ID", "referenceType": "acme-asset", "referenceLocator": "shared-001"}
  ]
}
```

### SPDX 3

`Element.externalIdentifier[]` with `type = <scheme>`:

```json
{
  "type": "software_Package",
  "name": "serde",
  "externalIdentifier": [
    {"type": "purl", "identifier": "pkg:cargo/serde@1.0.0"},
    {"type": "kusari-id", "identifier": "asset-shared-lib-v2"},
    {"type": "acme-asset", "identifier": "shared-001"}
  ]
}
```

Native open-typed identifier list — Principle V audit passes.

## Observable contract from outside the binary

### Default behavior (no `--component-id` flags)

```bash
$ mikebom sbom scan --path . --output out.cdx.json
... (no per-component identifier handling fires; SBOM byte-identical to alpha.17)
```

### Single match

```bash
$ mikebom sbom scan --path . \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2" \
    --output out.cdx.json

$ jq '.components[] | select(.purl == "pkg:cargo/serde@1.0.0") | .properties' out.cdx.json
[{"name": "kusari-id", "value": "asset-shared-lib-v2"}]
```

### Zero match

```bash
$ mikebom sbom scan --path . \
    --component-id "pkg:cargo/nonexistent@0.0.0=asset:foo" \
    --output out.cdx.json
WARN --component-id selector `pkg:cargo/nonexistent@0.0.0` matched zero components; identifier `asset:foo` not attached
... (scan exits 0; SBOM emitted with no extra annotations)
```

### Built-in scheme rejection

```bash
$ mikebom sbom scan --path . \
    --component-id "pkg:cargo/foo@1.0=subject:sha256:abc"
error: invalid value 'pkg:cargo/foo@1.0=subject:sha256:abc' for '--component-id <PURL=SCHEME:VALUE>': scheme 'subject' is reserved for document-level built-in usage
```

## Test contract

The integration-test file `mikebom-cli/tests/identifiers_subject_and_component.rs` MUST cover (per-component portion):

| Test | Acceptance scenario | Validates |
|------|--------------------|-----------|
| `component_id_attaches_to_matching_component_cdx` | US4 §1, SC-003 | FR-007, FR-008 (CDX) |
| `component_id_attaches_to_matching_component_spdx23` | US4 §1, SC-003 | FR-008 (SPDX 2.3) |
| `component_id_attaches_to_matching_component_spdx3` | US4 §1, SC-003 | FR-008 (SPDX 3) |
| `component_id_warns_on_zero_match` | US4 §2, SC-008 | FR-010 |
| `component_id_attaches_to_all_matching_when_multiple` | US4 §3 | FR-011 |
| `component_id_rejects_builtin_scheme_at_parse` | US4 §4, SC-007 | FR-009 |
| `component_id_rejects_malformed_input_at_parse` | US4 §5 | parse contract |
| `component_id_lexical_order_within_new_entries` | FR-012 + research §6 | determinism |
| `component_id_preserves_existing_properties` | research §6 | no churn |

Plus unit tests on `ComponentIdentifierFlag::parse` for the parse-error edge cases.

## Determinism contract (per FR-012, SC-005)

- Per-component identifier matching is deterministic: byte-equality match.
- Per-component emission order: pre-existing entries preserved at their positions; new entries appended in lexical order by `(scheme, value)`.
- Multi-component fan-out: each matching component independently sees the new entry in its own properties/externalRefs.
- Re-running with identical inputs produces byte-identical output.
