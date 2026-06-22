# Data Model — milestone 134

## DivergenceRecord (mikebom-common)

The shared typed representation of a detected divergent-PURL collision. Constructed at the per-ecosystem dedup site; consumed by all three format emitters (CDX / SPDX 2.3 / SPDX 3) and by the parity-catalog extractors.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DivergenceRecord {
    /// Wire-format schema version. Bumped only on incompatible changes.
    /// Always `1` for this milestone.
    pub v: u32,

    /// The shared PURL identity of the colliding manifests.
    pub purl: Purl,

    /// The divergence reason. Drives which payload fields are populated.
    pub reason: DivergenceReason,

    /// Every manifest path that participated in the collision, in
    /// filesystem-walk discovery order (deterministic — sorted entries
    /// per the walker's invariant). Always 2+ entries.
    pub paths: Vec<String>,

    /// Per-path declared direct dep names. Sorted lexicographically.
    /// Populated when `reason` is `DepsDiffer` or `Both`. The keys
    /// match `paths` 1:1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dep_sets_by_path: Option<BTreeMap<String, Vec<String>>>,

    /// Per-path deep-hash hex strings. Populated when `reason` is
    /// `HashesDiffer` or `Both` AND the scan was run with `--deep-hash`.
    /// The keys match `paths` 1:1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes_by_path: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DivergenceReason {
    /// Declared direct dep sets differ across colliding manifests.
    DepsDiffer,
    /// Deep hashes differ across colliding manifests (under --deep-hash).
    HashesDiffer,
    /// Both declared dep sets AND deep hashes differ.
    Both,
}
```

### Validation rules

- `v == 1` for this milestone. Future readers MAY accept higher values they understand.
- `paths.len() >= 2` (a collision requires at least 2 manifests).
- `dep_sets_by_path.is_some()` iff `reason ∈ { DepsDiffer, Both }`.
- `hashes_by_path.is_some()` iff `reason ∈ { HashesDiffer, Both }`.
- When `dep_sets_by_path.is_some()`, its key set equals `paths.iter().collect::<HashSet<_>>()`.
- When `hashes_by_path.is_some()`, its key set equals `paths.iter().collect::<HashSet<_>>()`.

### State transitions

None. `DivergenceRecord` is immutable; constructed once at the per-ecosystem dedup site and forwarded through the emission pipeline.

## CollisionsSummary (mikebom-common)

The document-scope aggregate emitted only when at least one `DivergenceRecord` was detected in the scan.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollisionsSummary {
    /// Wire-format schema version. Always `1` for this milestone.
    pub v: u32,

    /// Every divergent collision detected in the scan, in deterministic
    /// order (sorted by `record.purl.as_str()`). Always non-empty when
    /// this summary is emitted at all; the absence of the wrapping
    /// annotation is the no-collision signal (FR-009).
    pub collisions: Vec<DivergenceRecord>,
}
```

### Validation rules

- `v == 1`.
- `collisions.len() >= 1` whenever this summary is emitted. When zero, the wrapping annotation is omitted entirely (FR-009).
- Each `DivergenceRecord` in `collisions` is independently valid per its own rules.

### State transitions

None. Constructed once at the post-walk dedup-resolution phase; forwarded into the format emitters' document-properties / document-annotations site.

## Integration with existing types

- `Purl` — already a typed newtype in `mikebom-common::types::purl` (introduced milestone 005). Reused unchanged.
- `MikebomAnnotationCommentV1` envelope — the SPDX 2.3 + SPDX 3 annotation transport from the milestone-071 parity-extractors infrastructure. Reused unchanged; `DivergenceRecord` is serialized as the envelope's `value` field.
- `BTreeMap` — chosen over `HashMap` for deterministic JSON key ordering across runs. Same discipline as elsewhere in mikebom's serde output.

## Cargo-reader-side accumulation (transient)

Inside the cargo reader, BEFORE the dedup-resolution phase, a transient per-scan structure accumulates the raw input:

```rust
// Lives in mikebom-cli/src/scan_fs/package_db/cargo.rs only.
// NOT exposed across the crate boundary.
struct CargoManifestCandidate {
    path: String,                  // rootfs-relative
    purl: Purl,
    declared_deps: BTreeSet<String>,
    deep_hash: Option<String>,     // only computed when --deep-hash is set
}
```

The dedup-resolution phase groups candidates by PURL, then for each group of size ≥ 2 with divergence, constructs the `DivergenceRecord` and forwards it through the emission pipeline.

This struct is cargo-specific and stays in the reader. Future ecosystem expansions (npm / maven / pip / gem / go-binary) introduce their own analogous reader-internal candidate types and converge on the same shared `DivergenceRecord`.
