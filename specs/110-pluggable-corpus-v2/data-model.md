# Data Model — milestone 110 (Pluggable fingerprint corpus v2)

Date: 2026-06-03
Branch: `110-pluggable-corpus-v2`

All types live in `mikebom-cli/src/scan_fs/binary/fingerprints/`. No new crates. All payload-bearing types derive `serde::{Deserialize, Serialize}` with `#[serde(deny_unknown_fields)]` for strict-shape rejection at deserialization time (per research R5).

## CorpusRecordV2

The on-disk shape of a v2 corpus record (one JSON file per record OR one entry in an aggregated archive).

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusRecordV2 {
    /// Stable identifier within the corpus, e.g., "openssl-3.1.4-glibc-amd64".
    pub id: RecordId,
    /// Canonical PURL. Required.
    pub purl: Purl,
    /// Alternative ecosystem-specific PURLs for the same identity.
    #[serde(default)]
    pub purl_aliases: Vec<Purl>,
    /// CPE candidates for NVD lookups.
    #[serde(default)]
    pub cpe_candidates: Vec<Cpe>,
    /// Semantic version range this record covers (e.g., ">=3.1.4,<3.2.0").
    pub version_range: VersionRange,
    /// Target architectures (e.g., "x86_64-linux-gnu"). Empty = any.
    #[serde(default)]
    pub architectures: Vec<ArchitectureTriple>,
    /// ABI marker (e.g., "glibc-2.31+"). Free-form; used for human triage.
    #[serde(default)]
    pub abi: Option<String>,
    /// At least one indicator block is required (validated at deserialization).
    pub indicators: BTreeMap<IndicatorKind, IndicatorSpec>,
    /// Cross-record collision hints.
    #[serde(default)]
    pub collision: CollisionSpec,
    /// Provenance metadata. Required.
    pub provenance: Provenance,
    /// Schema version; MUST be 2 for v2 records (validated at deserialization).
    pub schema_version: u8,
}
```

**Validation rules** (enforced at deserialization OR in a post-deserialization `validate()` call):
- `schema_version == 2` → reject with `CorpusError::WrongSchemaVersion` if not.
- `indicators.len() >= 1` → reject with `CorpusError::NoIndicators` if empty (FR-001).
- `purl` parses as a valid PURL via the existing `Purl::parse` constructor.
- `cpe_candidates` each parse as a valid CPE 2.3 via the existing `Cpe::parse`.
- `version_range` parses via `semver::VersionReq::parse` OR an `unknown` literal for libraries without semver discipline.

## IndicatorKind (closed enum)

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorKind {
    /// ELF .dynsym / Mach-O LC_SYMTAB / PE IMAGE_EXPORT_DIRECTORY
    ExportedSymbols,
    /// .rodata literals like "OpenSSL 3.1.4"
    VersionString,
    /// .note.gnu.build-id (ELF)
    BuildId,
    /// LC_UUID (Mach-O)
    MachoUuid,
    /// CodeView GUID:age (PE)
    PePdb,
    /// Versioned ELF symbols (OPENSSL_3_0_0, GLIBC_2.34, etc.)
    AbiMarker,
}
```

`#[serde(deny_unknown_fields)]` does NOT apply to enum variants — unknown variants in incoming JSON are rejected by `serde` automatically. This keeps the indicator set closed at v2; v2.1 records can add new variants but will be unknown to current mikebom and that record will be skipped with a warning.

## IndicatorSpec (per-indicator config inside a record)

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "kebab-case")]
pub enum IndicatorSpec {
    SymbolSet {
        required: Vec<String>,         // Symbol names mikebom must observe
        min_match: usize,              // Minimum count to consider this indicator matched
        confidence_baseline: Confidence,
        #[serde(default)]
        suppress_when_self_identity_matches: bool,  // Default false; weak indicators override to true
    },
    RodataLiteral {
        patterns: Vec<String>,         // Substring patterns; any one matching counts
        confidence_baseline: Confidence,
        #[serde(default)]
        suppress_when_self_identity_matches: bool,
    },
    ExactHash {
        sha_or_uuid_set: Vec<String>,  // Lower-case hex-encoded
        confidence_baseline: Confidence,
        #[serde(default = "default_true")]
        suppress_when_self_identity_matches: bool,  // Default true; build-ids of the project itself are still useful
    },
}
fn default_true() -> bool { true }
```

The `tag = "type"` discriminator means JSON looks like:
```json
{ "type": "symbol-set", "required": [...], "min_match": 8, "confidence_baseline": 0.70 }
```

## Confidence (newtype around f64)

```rust
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Confidence(f64);

impl TryFrom<f64> for Confidence {
    type Error = CorpusError;
    fn try_from(v: f64) -> Result<Self, Self::Error> {
        if (0.0..=1.0).contains(&v) {
            Ok(Confidence(v))
        } else {
            Err(CorpusError::ConfidenceOutOfRange(v))
        }
    }
}

impl Confidence {
    /// Const constructor for compile-time-known valid baselines (e.g., the v1-upgrade
    /// 0.70 baseline). Inputs are out of 100 — so `from_pct_in_range_const::<70>()`
    /// constructs `Confidence(0.70)`. The const generic boundary makes "PCT in 0..=100"
    /// a compile-time check, so this constructor cannot panic and satisfies constitution
    /// principle IV's no-`unwrap()`-in-production rule for fixed-baseline construction
    /// sites that don't have an Err-return surface available (e.g., struct-literal init).
    pub const fn from_pct_in_range_const<const PCT: u8>() -> Self {
        // const-assert: PCT must be in 0..=100. The compiler rejects out-of-range
        // const arguments at the call site, so no runtime check is needed.
        const { assert!(PCT <= 100, "Confidence percentage must be 0..=100"); }
        Self(PCT as f64 / 100.0)
    }

    pub const fn into_inner(self) -> f64 { self.0 }
}
```

Newtype boundary per constitution principle IV. The `from_pct_in_range_const` constructor is used at known-baseline call sites (the v1-upgrade shim's `Confidence::from_pct_in_range_const::<70>()`, the design-doc-§7 baseline table) so production code never calls `.unwrap()` to construct a `Confidence` from a literal.

## FusedConfidence (post-fusion bucket; what emits)

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FusedConfidence {
    High,    // confidence >= 0.85
    Medium,  // 0.70 <= confidence < 0.85
    // No Low variant — below 0.70 means no MatchResult is produced at all
}

impl FusedConfidence {
    pub fn from_fused(c: Confidence) -> Option<Self> {
        match c.into_inner() {
            v if v >= 0.85 => Some(Self::High),
            v if v >= 0.70 => Some(Self::Medium),
            _ => None,
        }
    }
}
```

The `Option<Self>` return type encodes the below-floor suppression at the type level — the matcher cannot accidentally emit a "low" component because there's nothing to construct.

## Provenance

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub tier: ProvenanceTier,                    // Tier1Ingestion | Tier2BuildFromSource | Tier3ManualCuration
    pub extracted_from: String,                  // URL of the source artifact
    pub extracted_from_sha256: Sha256,           // Pinned content hash
    pub extraction_toolchain: String,            // e.g., "mikebom-corpus-builder@v0.3.1"
    pub extracted_at: chrono::DateTime<chrono::Utc>,
    #[serde(default = "default_true")]
    pub verified: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProvenanceTier {
    AutomatedIngestion,
    ReproducibleBuild,
    ManualCuration,
}
```

## CollisionSpec

```rust
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionSpec {
    /// Records this one is known to collide with on indicator overlap.
    #[serde(default)]
    pub look_alikes: Vec<LookAlike>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LookAlike {
    pub purl: Purl,
    pub shared_indicators: Vec<IndicatorKind>,
}
```

## CorpusSource (runtime-only; not serialized)

```rust
#[derive(Clone, Debug)]
pub struct CorpusSource {
    pub url: Url,
    pub credential_env_var: Option<String>,      // Name of env var holding bearer token; value never stored
    pub allowed_issuers: Vec<String>,            // Sigstore identity allowlist
    pub source_id: CorpusSourceId,               // Stable hash of url for cache layout (R3)
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CorpusSourceId(String);  // 16-char BASE32 of sha256(url)[..10], OR "public-milestone-108"
```

## MatchResult (matcher output)

```rust
#[derive(Clone, Debug)]
pub struct MatchResult {
    pub purl: Purl,                              // Canonical PURL of the matched record
    pub purl_aliases: Vec<Purl>,                 // Alias PURLs from the record
    pub cpe_candidates: Vec<Cpe>,                // From the record
    pub confidence: FusedConfidence,             // High or Medium only (below-medium yields None at matcher)
    pub indicators_matched: Vec<IndicatorKind>,  // Which indicators actually matched
    pub version_range: VersionRange,             // From the record
    pub record_id: RecordId,                     // Provenance chain back to the corpus
    pub source_id: CorpusSourceId,               // Which configured source contributed this record
    pub also_detected_via: Vec<Purl>,            // Other records whose indicators matched this binary
}
```

## BuildAttributionRegistry (existing — extended)

Milestone 109's existing `BuildAttributionRegistry` is unchanged in shape; only its DOWNSTREAM CONSUMER (the matcher) extends. The matcher takes both the build-attribution registry AND the corpus, applies build-tree attribution FIRST (per milestone 109), THEN falls through to corpus matching for binaries the registry doesn't cover.

## Self-identity types

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelfIdentity {
    /// Bare library name extracted from cmake/cargo/npm/pep621 etc.
    pub bare_name: Option<String>,
    /// Full PURL when resolvable (e.g., from git remote + cargo).
    pub purl: Option<Purl>,
}

impl SelfIdentity {
    pub fn matches_record(&self, record: &CorpusRecordV2) -> bool {
        // Case-insensitive comparison against record.purl + record.purl_aliases per R8
    }
}
```

## State transitions

The matcher state machine is deliberately stateless per-scan. The state lives in:
- **Cache directory** — per-source per-SHA archives + TTL touch files (mutable across scans, immutable within a scan).
- **Loaded `Corpus` struct** — populated at scan startup from cache (immutable for the duration of the scan).
- **Per-binary `BinaryArtifact`** — immutable extracted indicators, populated upstream of the matcher (existing milestone-099+ flow).

No mutable matcher state crosses scan boundaries. This is deliberate per constitution principle II's spirit ("trace is authoritative; enrichment doesn't accumulate persistent claims").

## Error types

```rust
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    #[error("record schema_version {0} not supported; expected 2 (or 1 for backward-compat)")]
    WrongSchemaVersion(u8),
    #[error("record has no indicators")]
    NoIndicators,
    #[error("confidence {0} out of [0.0, 1.0]")]
    ConfidenceOutOfRange(f64),
    #[error("source {source_id} fetch failed: {kind}")]
    Fetch { source_id: CorpusSourceId, kind: FetchFailureKind },
    #[error("source {source_id} signature verification failed: {reason}")]
    SignatureFailure { source_id: CorpusSourceId, reason: String },
    #[error("malformed record {record_id} in source {source_id}: {detail}")]
    MalformedRecord { source_id: CorpusSourceId, record_id: RecordId, detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchFailureKind {
    MissingCredential,
    InvalidCredential,
    NetworkUnreachable,
    ArchiveMalformed,
}
```

Distinct variants per failure category enable the FR / SC-005 actionable-error-message requirement (operators see the specific failure kind, not a stack trace).
