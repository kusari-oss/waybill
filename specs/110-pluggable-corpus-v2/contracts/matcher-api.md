# Matcher API contract (internal Rust surface in mikebom-cli)

This contract documents the Rust API surface the v2 matcher exposes to its callers inside `mikebom-cli`. Not a public crate-level API; the surface is internal to the `binary/fingerprints/` module hierarchy. Documented here so the implementation and tests stay aligned.

## Top-level matcher entry point

```rust
pub fn match_binary(
    binary: &BinaryArtifact,
    corpus: &Corpus,
    self_identity: Option<&SelfIdentity>,
    build_attribution: Option<&BuildAttributionRegistry>,
) -> Vec<MatchResult>
```

**Input contract**:
- `binary`: an already-populated `BinaryArtifact` carrying the extracted indicators (exported symbols, version strings, build-id, etc.) from milestones 099 / 023 / 024 / 028 / 026 / 305 / 309.
- `corpus`: the merged set of loaded records from ALL configured sources. Records carry their `CorpusSourceId` so attributions trace back to the contributing source.
- `self_identity`: present when the scan root resolves to a project name (per research R8 ladder). `None` means no self-suppression applies.
- `build_attribution`: milestone-109's registry of cmake `_deps/`-observed source declarations. When present, build-tree attribution takes precedence over corpus matching (matcher returns the build-attributed PURL with `confidence: High` and a `mikebom:source-mechanism: "cmake-fetchcontent-{git,url}"` annotation; corpus records are NOT consulted for the same binary).

**Output contract**:
- Empty `Vec` → no fingerprint identification (binary surfaces at the upstream file-SHA-256 baseline).
- One `MatchResult` → a single corpus record matched at high or medium confidence. Emit one binary-tier component with this PURL.
- Multiple `MatchResult`s → multiple records matched at non-suppressed confidence (FR-014 collision case). Emit one component per result, each with `mikebom:also-detected-via` referencing the others.

**Determinism**: For identical inputs (same `BinaryArtifact`, same `Corpus`, same `SelfIdentity`), the matcher returns the same `Vec<MatchResult>` in the same order. Order: by `(confidence DESC, primary_purl ASC)` for stable SBOM-emission ordering.

## Fusion sub-routines

```rust
/// Per-record: compute fused confidence + matched-indicator set.
/// Returns None if the fused confidence falls below the FusedConfidence floor.
fn fuse_indicators(
    record: &CorpusRecordV2,
    binary: &BinaryArtifact,
    self_identity: Option<&SelfIdentity>,
) -> Option<(FusedConfidence, Vec<IndicatorKind>)>;

/// "Max + bump" rule (per research R2):
/// confidence = max(per-indicator baseline) over all matching indicators
/// for each agreeing additional indicator: confidence = min(0.99, confidence + 0.05)
/// Below 0.70 → None (suppressed).
fn fuse_confidence(matching_indicators: &[(IndicatorKind, Confidence)]) -> Option<FusedConfidence>;
```

## Cross-record collision handling

```rust
/// After per-record fusion, decide which records emit and which become also-detected-via cross-references.
fn resolve_collisions(
    per_record_results: Vec<RecordMatch>,
) -> Vec<MatchResult>;
```

Rules:
- If exactly one record matched → emit it; `also_detected_via = []`.
- If multiple records matched with overlapping `shared_indicators` per their `CollisionSpec` → emit all of them; each one's `also_detected_via` lists the others' primary PURLs.
- If multiple records matched WITHOUT a declared collision (corpus authoring oversight) → still emit all of them with cross-references; log `tracing::warn` so corpus maintainers can add the collision entry.

## Self-identity suppression

```rust
fn apply_self_identity_filter(
    record: &CorpusRecordV2,
    indicator_kind: IndicatorKind,
    self_identity: Option<&SelfIdentity>,
) -> SuppressionDecision;

pub enum SuppressionDecision {
    Apply,         // The indicator participates in fusion normally.
    SkipIndicator, // This indicator is skipped because self-identity matches AND the indicator opted in to suppression.
    SkipRecord,    // The entire record is skipped because all indicators opted in to suppression.
}
```

The decision is per-indicator-per-record: strong indicators (BuildId default `suppress_when_self_identity_matches=true` BUT only because BuildIds of the project itself ARE useful information — actually the design doc says strong indicators DON'T opt in by default; revisit if the implementation surprises). The matcher emits a `MatchResult` only when at least one indicator survives suppression AND fused confidence meets the floor.

## v1 backward-compat shim

```rust
/// Milestone-108 v1 records are upgraded to v2 in memory at load time.
fn upgrade_v1_to_v2(v1: &V1Record) -> CorpusRecordV2;
```

Mapping per spec FR-005 + the 2026-06-03 Q3 clarification:
- `v1.library_name` → `CorpusRecordV2 { purl: Purl::generic(&v1.library_name), ... }`
- `v1.symbols` → `indicators.insert(IndicatorKind::ExportedSymbols, SymbolSet { required: v1.symbols, min_match: v1.min_symbols, confidence_baseline: Confidence(0.70), suppress_when_self_identity_matches: true })`
- `v1.version_range` → `"unknown"` (v1 records have no version)
- Provenance is synthesized: `Provenance { tier: ManualCuration, extracted_from: "milestone-108-v1-record", ... }` (a sentinel value documenting the upgrade path).

The upgrade preserves the SC-002 component-emission contract: a v1 record emits the same `pkg:generic/<name>` component it always did, plus the new `confidence: "medium"` annotation per FR-005.

## Annotation emission contract (per research R1 audit)

```rust
/// Builds the annotation set to attach to a binary-tier component from a MatchResult.
pub fn emit_component_annotations(
    result: &MatchResult,
    target_format: SbomFormat,
) -> ComponentAnnotations;
```

Per-format emission table (matches the R1 audit decisions):

| Signal | CDX 1.6 | SPDX 2.3 | SPDX 3.0.1 |
|---|---|---|---|
| Primary PURL | `component.purl` | `Package.externalRefs[purl]` | `software_Package.externalIdentifier[purl]` |
| PURL aliases | `properties[mikebom:purl-aliases]` | additional `externalRefs[purl]` entries | additional `externalIdentifier[purl]` entries |
| CPE candidates | existing `mikebom:cpe-candidates` (C32) | existing C32 annotation | existing C32 annotation |
| Confidence | `evidence.identity.confidence` (numeric 0-1) + `properties[mikebom:confidence]` (bucket name) | `mikebom:confidence` annotation (C16) | `mikebom:confidence` annotation (C16) |
| Indicators matched | `evidence.identity.methods[]` (one per indicator) + `properties[mikebom:indicators-matched]` (C59) | `mikebom:indicators-matched` annotation (C59) | `mikebom:indicators-matched` annotation (C59) |
| Also detected via | `evidence.identity.methods[].mikebom-source-mechanism` (C56 native form) | `mikebom:also-detected-via` annotation (C56) | `mikebom:also-detected-via` annotation (C56) |
| Version range | `properties[mikebom:identification-version-range]` (C61) | `mikebom:identification-version-range` (C61) | `mikebom:identification-version-range` (C61) |
| Corpus SHA | `properties[mikebom:fingerprint-corpus-sha]` (extended C58 — multi-source array form) | extended C58 annotation | extended C58 annotation |
