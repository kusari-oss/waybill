# Data Model — milestone 072 cross-tier SBOM binding

The milestone introduces six new types in `mikebom-cli/src/binding/` plus extends one existing type in `mikebom-cli/src/generate/openvex/statements.rs`. Constitution Principle IV: every domain value is a newtype or enum, no raw `String` across function boundaries; production code uses `anyhow::Result`, no `.unwrap()`.

## Entities

### `BindingHashInputs` (new)

The FR-002 layered triple. Each side is `Option<String>` because not every project carries every input.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingHashInputs {
    /// VCS commit identifier (e.g., 40-char SHA-1 from `git rev-parse HEAD`,
    /// Go BuildInfo `vcs.revision`, cargo-auditable embedded VCS).
    pub vcs: Option<String>,
    /// SHA-256 hex of the project's lockfile bytes (Cargo.lock /
    /// package-lock.json / Gemfile.lock / go.sum / poetry.lock /
    /// requirements.txt's `--hash=` content).
    pub lockfile: Option<String>,
    /// SHA-256 hex of the project's top-level manifest after canonical
    /// normalization (Cargo.toml / package.json / pom.xml / *.gemspec /
    /// pyproject.toml / go.mod).
    pub manifest: Option<String>,
}

impl BindingHashInputs {
    /// Count populated sides. Used to derive BindingStrength.
    pub fn populated_count(&self) -> usize {
        self.vcs.is_some() as usize
            + self.lockfile.is_some() as usize
            + self.manifest.is_some() as usize
    }
}
```

**Validation**: at least one side MUST be populated; `populated_count() == 0` → caller emits `BindingStrength::Unknown` with `reason: "no-evidence"` and skips hash computation.

### `BindingHash` (new)

Newtype wrapping the SHA-256 hex string.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindingHash(String);

impl BindingHash {
    /// Construct from a hex-encoded SHA-256 (64 lowercase hex chars).
    /// Returns an error if the input fails the format check.
    pub fn from_hex(hex: impl Into<String>) -> anyhow::Result<Self> { /* ... */ }
    pub fn as_hex(&self) -> &str { &self.0 }
}
```

### `BindingStrength` (new)

Three-variant enum derived from `BindingHashInputs::populated_count()`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStrength {
    /// All 3 sides populated AND match source-tier recomputation.
    Verified,
    /// Exactly 2 sides populated AND match.
    Weak,
    /// < 2 sides populated, or any present side fails verification.
    Unknown,
}
```

**Derivation rule** (FR-012):

- `populated_count == 3` AND all three match → `Verified`
- `populated_count == 2` AND both populated sides match → `Weak`
- otherwise → `Unknown`

### `SourceDocumentId` (new)

Stable identifier for the source SBOM document.

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceDocumentId {
    /// SHA-256 hex of the canonical source SBOM bytes. Verifier-computable.
    pub sha256: String,
    /// Optional IRI for human-readable cross-reference. May be a URL,
    /// a urn:uuid:..., or any stable handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iri: Option<String>,
}
```

### `SourceDocumentBinding` (new)

The per-component annotation payload. Attaches via the existing `MikebomAnnotationCommentV1` envelope (SPDX 2.3 + SPDX 3) and a CDX `properties[]` entry, all of which carry the JSON-encoded form of this struct.

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceDocumentBinding {
    /// Pointer to the source-tier SBOM document.
    pub source_doc_id: SourceDocumentId,
    /// Per-component layered hash. None if strength == Unknown
    /// (then no hash to verify against).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<BindingHash>,
    /// Cross-tier identity strength.
    pub strength: BindingStrength,
    /// Structured rationale, especially for Unknown markers (per FR-003).
    /// Common values: "no-evidence", "base-layer-system-package",
    /// "sideloaded-binary", "source-not-found-in-bind-target",
    /// "verification-failed".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Algorithm version. Always "v1" for milestone 072. Bump only via
    /// a versioned binding scheme (V1 → V2 with parallel emission).
    #[serde(default = "default_algo_v1")]
    pub algo: String,
}

fn default_algo_v1() -> String { "v1".to_string() }
```

### `VexPropagationMode` (new)

Three-variant enum for the `--vex-propagation-mode` flag.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum VexPropagationMode {
    /// Pre-072 behavior — propagate by PURL match without binding check.
    Permissive,
    /// Default in milestone 072. Propagate but tag binding-unverified
    /// statements with a structured caveat.
    Caveated,
    /// Refuse propagation when binding strength != Verified.
    Strict,
}

impl Default for VexPropagationMode {
    fn default() -> Self { Self::Caveated }
}
```

### `OpenVexProduct` (existing — extended)

Located at `mikebom-cli/src/generate/openvex/statements.rs:71`. Add an `identifiers` map:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenVexProduct {
    /// Component identifier — mikebom emits the PURL string here.
    #[serde(rename = "@id")]
    pub id: String,
    /// Milestone 072 / FR-008: per-instance identifier map.
    /// Standard keys: "purl", "cyclonedx-bom-ref", "spdx-spdxid".
    /// Pre-072 emission produces an empty map (skipped via
    /// skip_serializing_if), preserving back-compat wire shape.
    /// Post-072 emission populates this with the originating
    /// component's bom-ref/SPDXID alongside the PURL.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub identifiers: BTreeMap<String, String>,
}
```

## Relationships

```text
SourceDocumentBinding
   │
   ├── 1 ── points-to ───> SourceDocumentId (which source SBOM)
   ├── 1 ── carries ─────> BindingHash (or None when strength == Unknown)
   ├── 1 ── classified-by ──> BindingStrength
   ├── 1 ── (optional) ──> reason: String  (required when strength == Unknown)
   └── 1 ── tagged-by ───> algo: "v1"

BindingHashInputs
   │
   ├── 0..1 ── vcs (commit-id-or-null)
   ├── 0..1 ── lockfile (sha256-of-bytes-or-null)
   ├── 0..1 ── manifest (sha256-of-canonical-bytes-or-null)
   └── derives ──> BindingStrength via populated_count()

VEX propagation
   │
   ├── ── source-side ──> OpenVEX statement on source-tier SBOM
   ├── ── consults ──────> SourceDocumentBinding on target component
   ├── ── checks ────────> binding strength against VexPropagationMode
   └── ── emits ─────────> propagated OpenVEX statement on target SBOM

OpenVexProduct (extended)
   ├── id (PURL — pre-072 only field, always present)
   └── identifiers
        ├── "purl" → <PURL>
        ├── "cyclonedx-bom-ref" → <bom-ref> (optional, when CDX-paired)
        └── "spdx-spdxid" → <SPDXID> (optional, when SPDX-paired)
```

## State / lifecycle

Binding metadata is immutable after emission. Once a SBOM carrying a `SourceDocumentBinding` is written, the binding never updates in-place — a re-scan with new evidence produces a new SBOM document with potentially different bindings.

`VexPropagationMode` is a per-invocation choice; not persisted in the SBOM. The propagation OUTCOME (what got propagated, what got caveated, what got refused) IS recorded in the target SBOM via the `mikebom:enrichment-patch` provenance properties (existing infrastructure at `sbom/mutator.rs:30-44`).

## Validation rules

- **VR-001**: `BindingHashInputs::populated_count() < 2` → emitter MUST set `BindingStrength::Unknown` with non-empty `reason`. (Implements FR-012's strength rules + FR-003's transparency.)
- **VR-002**: `SourceDocumentBinding.hash.is_none()` ⇔ `SourceDocumentBinding.strength == Unknown`. The two fields move together.
- **VR-003**: `SourceDocumentBinding.algo` MUST be `"v1"` for emission; readers MUST reject unknown algorithm strings (forward-compat for V2 + reject malformed input).
- **VR-004**: `OpenVexProduct.identifiers` keys MUST come from the registered set (`purl`, `cyclonedx-bom-ref`, `spdx-spdxid`); unknown keys MUST be tolerated by readers (open-ended dictionary per OpenVEX 0.2.0) but mikebom MUST NOT emit unknown keys itself.
- **VR-005**: `verify-binding` command MUST exit non-zero AND emit a structured per-component rationale when ANY image-tier component's recomputed binding hash fails to match the asserted hash. (Implements FR-005.)
- **VR-006**: When `mikebom sbom enrich --vex-propagation-mode strict` encounters a non-`Verified` binding while propagating, the command MUST exit non-zero with a refusal-rationale annotation written to the target SBOM. (Implements FR-007.)
- **VR-007**: The cross-format-parity test suite (`tests/holistic_parity.rs` from milestone 071) MUST pass after this milestone — meaning the new `mikebom:source-document-binding` annotation gets a new catalog row with `Directionality::SymmetricEqual`, and the per-format extractors all return byte-identical sets after the milestone-071 canonicalization.

## Algorithm-version contract (FR-002 v1)

```text
input:    BindingHashInputs { vcs, lockfile, manifest }
canonicalize:
    {
      "algo": "v1",
      "lockfile": <lockfile or null>,
      "manifest": <manifest or null>,
      "vcs": <vcs or null>
    }
serialize: serde_json::to_string (compact, sorted keys, no whitespace)
hash:     SHA-256 of UTF-8 bytes
output:   BindingHash::from_hex(<lowercase hex>)
```

Implementation detail: keys appear alphabetically sorted (`algo`, `lockfile`, `manifest`, `vcs`) — this is the key-order contract a verifier MUST follow to produce a byte-identical envelope. Reuse `parity/extractors/common.rs::canonicalize_for_compare(value, false)` for the canonicalization step (Constitution-Principle-IV-style consistency: one canonical-JSON primitive across the project).

## Backward compatibility

- The `BindingHashInputs` struct is private to `mikebom-cli/src/binding/`; pre-072 emitters are unaffected.
- `SourceDocumentBinding` only emits on `mikebom:sbom-tier: build` or `deployed` SBOMs; source-tier SBOMs are byte-identical pre-072 (alpha.14 source-tier goldens unchanged).
- `OpenVexProduct.identifiers` is `skip_serializing_if = "BTreeMap::is_empty"`. When mikebom is invoked WITHOUT `--vex-overrides` (the propagation entry point), no per-instance identifiers are populated and the wire shape matches alpha.14.
- `VexPropagationMode::Caveated` is the new default, BUT it only fires when `--vex-overrides <path>` is supplied. Callers passing only `--patch` (the JSON-Patch path) see zero behavior change.
