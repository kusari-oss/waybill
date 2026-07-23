# Data Model: Cross-ecosystem dep-name edge resolution

**Feature**: 218-cross-ecosystem-edges | **Date**: 2026-07-22

## E1 — `CrossEcosystemInferencePayload` (per-edge value type)

New public type in `waybill-cli/src/generate/cross_ecosystem_edges/mod.rs`.

```rust
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct CrossEcosystemInferencePayload {
    /// PURL type identifier of the source main-module's ecosystem.
    /// Today always `"generic"` (only ecosystem that triggers FR-001).
    pub from_eco: String,
    /// Stable machine-readable identifier of the reader path that
    /// produced the source main-module. m216 registers
    /// `"gemfile-lock-dependencies"`. Future readers register their own.
    pub lookup_via: String,
    /// PURL of the target component the cross-ecosystem lookup resolved to.
    /// Purl-spec conformant. Used by consumers to correlate the annotation
    /// to a specific edge (necessary because CDX + SPDX 2.3 have no
    /// per-target-within-source-annotation slot).
    pub target_purl: String,
    /// PURL type identifier of the target component's ecosystem.
    /// e.g. `"gem"`, `"pypi"`, `"npm"`, `"cargo"`, `"golang"`.
    pub to_eco: String,
}
```

**Validation rules**:
- `from_eco` MUST equal `"generic"` at v1 (FR-001 restriction).
- `to_eco` MUST NOT equal `"generic"` (cross-ecosystem means source ecosystem ≠ target ecosystem).
- `target_purl` MUST parse successfully via `waybill_common::types::purl::Purl::new`.
- `lookup_via` MUST be non-empty and match the regex `^[a-z0-9-]+$` (kebab-case identifier).
- `serde_json::to_string(&payload)` MUST produce canonical bytes (fields declared alphabetically; serde emits struct fields in declaration order).

**Field ordering**: alphabetic (`from_eco`, `lookup_via`, `target_purl`, `to_eco`) — makes serialization canonical without additional sort logic.

## E2 — `CrossEcosystemInferenceAmbiguousPayload` (per-edge value type — ambiguous variant)

```rust
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct CrossEcosystemInferenceAmbiguousPayload {
    /// Sibling records for every OTHER candidate ecosystem that
    /// also matched this dep-name. Sorted lex by `target_purl` for
    /// byte-identity. Does NOT include the current edge's own match.
    pub alternates: Vec<AlternateMatch>,
    pub from_eco: String,
    pub lookup_via: String,
    pub target_purl: String,
    pub to_eco: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct AlternateMatch {
    pub target_purl: String,
    pub to_eco: String,
}
```

**Validation rules**:
- `alternates.len() >= 1` (if it's ambiguous, at least one alternate exists per definition).
- `alternates` sorted lex by `target_purl` for byte-identity.
- All alternates' `target_purl` MUST parse successfully via `Purl::new`.
- Self-consistency: `AlternateMatch { target_purl, to_eco }` for the current edge MUST NOT appear in `alternates` (each ambiguous-annotated edge lists ONLY its siblings).

## E3 — `CrossEcosystemInferenceUnresolvedRecord` (doc-scope element type)

```rust
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct CrossEcosystemInferenceUnresolvedRecord {
    /// PURL of the source main-module whose depends[] contained the
    /// unresolvable name.
    pub source_purl: String,
    /// The unresolvable dep-name (post-normalization applied by
    /// FR-012 — the value the resolver actually searched for).
    pub unresolved_name: String,
}
```

The doc-scope annotation `waybill:cross-ecosystem-inference-unresolved` value is `Vec<CrossEcosystemInferenceUnresolvedRecord>` serialized to canonical JSON. Sort order: lex by `source_purl` then `unresolved_name`.

**Validation rules**:
- `source_purl` MUST parse via `Purl::new` AND MUST start with `pkg:generic/`.
- `unresolved_name` MUST be non-empty.
- The entire annotation MUST be omitted when the vector is empty (FR-011 silence-on-absence — matches m217 C136 precedent).

## E4 — `CrossEcosystemEdgesReport` (scan-scoped aggregate)

Threaded through `ScanArtifacts` per the m134/m173/m204/m217 propagation pattern.

```rust
#[derive(Debug, Default, Clone)]
pub struct CrossEcosystemEdgesReport {
    /// Every crossed edge emitted this scan, keyed by (source_purl, target_purl).
    /// Value carries the payload for CDX / SPDX 2.3 / SPDX 3 emitters to consume.
    pub crossed_edges: BTreeMap<(String, String), CrossEcosystemInferencePayload>,
    /// Every crossed-and-ambiguous edge. Superset structure of the above.
    pub ambiguous_edges: BTreeMap<(String, String), CrossEcosystemInferenceAmbiguousPayload>,
    /// FR-004 unresolved names for the doc-scope annotation. Sorted at
    /// insertion (BTreeMap-derived vector). Absent when empty.
    pub unresolved: Vec<CrossEcosystemInferenceUnresolvedRecord>,
    /// FR-013 INFO log counts.
    pub summary: CrossEcosystemEdgesSummary,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CrossEcosystemEdgesSummary {
    pub edges_resolved: usize,
    pub edges_ambiguous: usize,
    pub names_unresolved: usize,
}
```

**Validation rules**:
- `ambiguous_edges` is a subset of `crossed_edges` in the sense that every ambiguous edge is ALSO in the crossed set (with the base payload) — this lets the CDX emitter walk `crossed_edges` once and check `ambiguous_edges` for the sibling annotation.
- `summary.edges_resolved == crossed_edges.len() - ambiguous_edges.len()` (single-match resolutions).
- `summary.edges_ambiguous == ambiguous_edges.len()`.
- `summary.names_unresolved == unresolved.len()`.
- Empty report when the FR-000 flag is OFF (constructor takes flag state; returns default).

## E5 — Flag-state carrier

New field on `waybill_cli::cli::scan_cmd::ScanArgs`:

```rust
#[arg(
    long = "experimental-cross-ecosystem-edges",
    env = "WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES",
    action = ArgAction::SetTrue,
    help = "EXPERIMENTAL: enable cross-ecosystem dep-name edge resolution \
            (bridges pkg:generic/ main-modules to pkg:gem/ / pkg:pypi/ / etc. \
            transitive dependencies via lockfile-declared bare names). \
            Default off. See docs/reference/cross-ecosystem-edges.md."
)]
pub experimental_cross_ecosystem_edges: bool,
```

**Precedents**:
- m173 `--warm-go-cache` (env `WAYBILL_WARM_GO_CACHE`) — same `SetTrue` pattern.
- m119 `--supplement-cdx <file>` (env `WAYBILL_SUPPLEMENT_CDX`) — same env-var-alias pattern.

## E6 — Resolver-index shape (existing, unchanged)

The `name_to_purl: HashMap<(String, String), String>` at `scan_fs/mod.rs:536` is unchanged. This milestone READS the existing index; does not modify its shape.

For clarity, the existing key semantics:
- Key `.0`: source-side ecosystem string (from `entry.purl.ecosystem()` at insertion).
- Key `.1`: normalized name via `normalize_dep_name(source_ecosystem, entry.name)`.
- Value: canonical PURL string of the component being indexed.

The cross-ecosystem search (R2) filters keys by `.1 == normalize_dep_name(candidate_ecosystem, dep_name)` for each candidate ecosystem present in `name_to_purl.keys().map(|(eco, _)| eco).collect::<HashSet<_>>()`. Candidate ecosystems is precomputed once per scan.

## E7 — Sibling-ecosystem set (per FR-003 tie-break)

Local to the resolver pass, no persistence:

```rust
// Precomputed once before the main resolver loop.
let sibling_ecosystems: HashSet<String> = packages
    .iter()
    .filter(|p| p.is_main_module && p.purl.ecosystem() != "generic")
    .map(|p| p.purl.ecosystem().to_string())
    .collect();
```

Used only inside the R3 tie-break intersection. Dropped at resolver-pass exit.

## State Transitions

None — all data is scan-lifetime in-process. No state machines, no persistence.

## Data Volume Assumptions

- **Fastlane fixture (production-scale gem case)**: 27 DEPENDENCIES entries → up to 27 cross-ecosystem lookups per scan → payload objects ≤ ~150 bytes each → total ~4 KB of new annotation data per scan.
- **Multi-ecosystem polyglot (worst case imagined)**: 10 sibling ecosystems × 100 pkg:generic/ main-module depends[] entries → 1000 lookups, of which perhaps 50 are ambiguous → 50 ambiguous payloads × ~500 bytes = 25 KB. Still trivial against SBOM sizes measured in MBs.
