# Data Model — milestone 081 SBOM-type signaling clarity

The milestone introduces ONE new internal Rust type (`SbomType` enum) and TWO new pure functions in the existing `mikebom-cli/src/generate/lifecycle_phases.rs` helper. No new modules; no new structs beyond the enum. The aggregation logic mirrors the existing `aggregate_phases` pattern verbatim.

## Internal Rust types (NEW — `mikebom-cli/src/generate/lifecycle_phases.rs` extensions)

### `SbomType` enum

```rust
/// The 6 CISA SBOM Types (April 2023). Mapped 1:1 with SPDX 3's
/// `software_SbomType` enum and (via tier_to_phase) with CDX 1.6's
/// `metadata.lifecycles[].phase` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SbomType {
    Design,
    Source,
    Build,
    Analyzed,
    Deployed,
    Runtime,
}

impl SbomType {
    /// Returns the SPDX 3 IRI string the emission writes into
    /// `software_Sbom.software_sbomType[]`.
    pub fn as_spdx3_iri(&self) -> &'static str {
        match self {
            Self::Design   => "spdx:Software/SbomType/design",
            Self::Source   => "spdx:Software/SbomType/source",
            Self::Build    => "spdx:Software/SbomType/build",
            Self::Analyzed => "spdx:Software/SbomType/analyzed",
            Self::Deployed => "spdx:Software/SbomType/deployed",
            Self::Runtime  => "spdx:Software/SbomType/runtime",
        }
    }

    /// Returns the lowercase string for `--sbom-type` flag parsing.
    pub fn as_str(&self) -> &'static str { /* "design" / "source" / ... */ }

    /// Parse a `--sbom-type` flag value. Case-sensitive; valid set is
    /// {design, source, build, analyzed, deployed, runtime}.
    pub fn parse_str(s: &str) -> Result<Self, ParseSbomTypeError> { /* ... */ }
}

#[derive(Debug, thiserror::Error)]
#[error("--sbom-type '{value}' is not a valid CISA SBOM type; valid values are design/source/build/analyzed/deployed/runtime")]
pub struct ParseSbomTypeError {
    pub value: String,
}
```

**Invariant**: `as_spdx3_iri()` returns one of the 6 IRI strings the SPDX 3 SHACL constraint accepts. Validator-conformance is encoded in the type system per Constitution Principle IV.

**Lifetime**: `Copy`-cheap; created at CLI parse time + emission time; no allocation.

### `tier_to_spdx3_sbomtype_iri` helper function

```rust
/// Map a `mikebom:sbom-tier` string to its corresponding SPDX 3
/// `software_SbomType` IRI. Returns `None` for unrecognised tiers
/// (matches the existing `tier_to_phase` pattern for unknown-tier
/// resilience).
///
/// 1:1 mapping per research §2 equivalence table:
/// - "design"   → "spdx:Software/SbomType/design"
/// - "source"   → "spdx:Software/SbomType/source"
/// - "build"    → "spdx:Software/SbomType/build"
/// - "analyzed" → "spdx:Software/SbomType/analyzed"
/// - "deployed" → "spdx:Software/SbomType/deployed"
/// - "runtime"  → "spdx:Software/SbomType/runtime"
pub fn tier_to_spdx3_sbomtype_iri(tier: &str) -> Option<&'static str> { /* ... */ }
```

### `aggregate_spdx3_sbom_types` helper function

```rust
/// Aggregate the unique set of SPDX 3 SbomType IRIs observed across
/// the given components' `sbom_tier` values. Returns the IRI list
/// sorted lexicographically (deterministic for byte-identity
/// goldens). Mirrors the existing `aggregate_phases` pattern.
///
/// When `override_assertion` is `Some(SbomType)`, returns a
/// single-element Vec with the operator-asserted IRI (per research
/// §4 override semantics). Per-component tier values in the input
/// are IGNORED in this case — the operator's document-level claim
/// wins.
pub fn aggregate_spdx3_sbom_types<'a>(
    components: impl IntoIterator<Item = &'a ResolvedComponent>,
    override_assertion: Option<SbomType>,
) -> Vec<&'static str> { /* ... */ }
```

## Wire-format entities — per format

### CDX 1.6 — UNCHANGED from milestone 047

No emission change. `metadata.lifecycles[]` continues to aggregate via the existing `aggregate_phases` + `tier_to_phase` pipeline. The `--sbom-type` flag, when asserted, will route through `aggregate_phases` with an override-assertion mechanism (extending the existing helper signature analogously to `aggregate_spdx3_sbom_types`).

### SPDX 2.3 — UNCHANGED from milestone 047

No emission change. `creationInfo.comment` continues to carry the aggregated phase set as free-text per the existing milestone-047 wiring.

### SPDX 3 — NEW `software_Sbom.software_sbomType[]` array (MODIFIED)

```json
// BEFORE (today's emission, milestone 080 baseline):
{
  "type": "software_Sbom",
  "spdxId": "...",
  "rootElement": [...],
  "name": "...",
  "comment": "Scope: manifest (...). Observed lifecycle phases: pre-build."
  // No software_sbomType
}

// AFTER (post-milestone-081):
{
  "type": "software_Sbom",
  "spdxId": "...",
  "rootElement": [...],
  "name": "...",
  "comment": "Scope: manifest (...). Observed lifecycle phases: pre-build.",
  "software_sbomType": [
    "spdx:Software/SbomType/source"
  ]
  // Multi-tier scan would emit:
  // "software_sbomType": [
  //   "spdx:Software/SbomType/build",
  //   "spdx:Software/SbomType/source"
  // ]
  // Operator-asserted via --sbom-type build:
  // "software_sbomType": [
  //   "spdx:Software/SbomType/build"
  // ]
}
```

**Field type**: array of IRI strings, schema-conformant per `software_Sbom_props`.

**Sort order**: lexicographic per the existing milestone-047 contract.

**Optional**: not all SBOMs emit it. Empty SBOMs (zero components) and SBOMs whose components don't carry `mikebom:sbom-tier` annotations have no entries to aggregate; the `software_sbomType` field is OMITTED entirely (matches the milestone-047 `metadata_omits_lifecycles_when_no_tiers_present` behavior).

## Validation rules

- **VR-081-001**: `SbomType::parse_str` MUST accept exactly the 6 vocab values `{design, source, build, analyzed, deployed, runtime}` (case-sensitive). Any other input returns `ParseSbomTypeError`.
- **VR-081-002**: `tier_to_spdx3_sbomtype_iri` MUST return `Some(<IRI>)` for the 6 mikebom-tier values; `None` for unknown tiers (matches the existing `tier_to_phase` resilience pattern).
- **VR-081-003**: `aggregate_spdx3_sbom_types` MUST return a deterministic lexicographically-sorted Vec. With `override_assertion: Some(_)`, returns single-element Vec; with `None`, aggregates from `components` per the existing `aggregate_phases` logic.
- **VR-081-004**: When emitted, `software_Sbom.software_sbomType[]` MUST contain only IRIs from the 6-value SPDX 3 SbomType enum. Verified by SPDX 3 schema validation + the milestone-078 `spdx3-validate` SHACL gate.
- **VR-081-005**: When `--sbom-type` is asserted, the per-component `mikebom:sbom-tier` annotations MUST be preserved unchanged from auto-detection (override is document-level only per research §4).
- **VR-081-006**: All milestone-047 byte-identity goldens for CDX 1.6 + SPDX 2.3 MUST stay byte-identical (no emission change for those formats). All 9 SPDX 3 goldens regenerate as the expected operator-visible change.

## Backward compatibility

- **No new `Cargo.toml` deps**: extends an existing helper.
- **No MSRV change**: stable Rust toolchain per workspace.
- **No nightly required**: pure user-space transform.
- **CDX 1.6 + SPDX 2.3 byte-identity goldens stay byte-identical**.
- **SPDX 3 byte-identity goldens regenerate** with the new `software_sbomType[]` field. Per-fixture diff: +1 array on the SpdxDocument element. Documented as the milestone's expected operator-visible change.
- **Pre-flag invocations** of `mikebom sbom scan` produce SPDX 3 output WITH the new field auto-populated from per-component tiers. This is operator-visible new metadata, not a breaking change — downstream tools that didn't consume `software_sbomType` continue to work; tools that DO consume it now get a populated value.
- **Operators currently parsing `creationInfo.comment` for the phase set** in SPDX 3 continue to work — the comment field is unchanged. The new native-field IS additive.
