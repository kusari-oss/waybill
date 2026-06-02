# Data Model — External symbol-fingerprint corpus (108)

All entities below live in-process for the lifetime of a single scan (mikebom is in-process per-scan, same as every milestone since 002). The cache directory on disk is the only persistent state and is content-addressable by corpus SHA.

---

## `FingerprintRecord`

One library's identity claim. Stored on disk as `corpus/<library>.json` in the sibling repo, schema-validated by sibling-repo CI per FR-010.

| Field | Type | Required? | Notes |
|---|---|---|---|
| `library` | `String` | yes | Canonical library name. Used as the filename stem + as part of the emitted PURL. Lowercase ASCII + `-` + `.` only; matches regex `^[a-z][a-z0-9\-\.]*$`. |
| `target_purl` | `String` | yes | The PURL form mikebom emits when this record fires. MUST parse via the existing `mikebom_common::types::purl::Purl::new`. Typical: `pkg:generic/<library>` (no version — symbol-fingerprint can identify the library but not version, modulo an optional version-range hint). |
| `symbols` | `Vec<String>` | yes | List of public-API symbol names. Order is informational only; mikebom builds a `HashSet` at load time. Sibling-repo CI enforces `len() ≥ 2 × min_symbols`. |
| `min_symbols` | `u32` | yes | Minimum number of `symbols` that must appear in a target binary's `.dynsym` for this record to fire (Q3 clarification). Sibling-repo CI enforces `≥ 5`. |
| `version_hint` | `Option<String>` | no | Free-form version range or stability marker (e.g., `">=3.0, <4.0"`, `"stable-api-since-1.18"`). Informational; surfaces in the emitted SBOM as `mikebom:version-hint` annotation when present. |
| `variant` | `Option<String>` | no | Fork/variant discriminator (e.g., `"libressl"` vs `"openssl"`). Two records with the same `library` but different `variant` fire independently if both match. Surfaces as `mikebom:library-variant` annotation. |
| `notes` | `Option<String>` | no | Free-form curator note. Not emitted to the SBOM; sibling-repo internal-use only. |

**Validation rules at mikebom-cli load time** (defensive — sibling-repo CI already enforces, but malformed records from a non-CI SHA override path are skipped):

- Missing `library` / `target_purl` / `symbols` / `min_symbols` → skip + `tracing::warn!`
- `target_purl` fails `Purl::new` → skip + `tracing::warn!`
- `min_symbols == 0` → skip + `tracing::warn!`
- `symbols.is_empty()` → skip + `tracing::warn!`

---

## `FingerprintCorpus`

The in-memory collection of records loaded for the current scan. Owns a `Vec<FingerprintRecord>` + the source SHA + the load-time-resolved `CorpusSource`.

```rust
pub(super) struct FingerprintCorpus {
    pub records: Vec<FingerprintRecord>,
    pub source: CorpusSource,
}
```

The collection is **flat** — no per-library lookup index in-process. mikebom iterates records linearly per binary (`O(records × symbols)`); at 100 libraries × 50 symbols-average this is ~5000 string comparisons per binary, negligible cost.

---

## `CorpusSource`

Enum tracking where the corpus came from. Drives the `mikebom:fingerprint-corpus-sha` annotation emission.

```rust
pub(super) enum CorpusSource {
    /// Bundled in-source FINGERPRINTS const fired. Annotation value: "bundled".
    Bundled,
    /// External corpus loaded from cache (no network fetch needed).
    Cached { sha: CorpusSha },
    /// External corpus loaded after a successful network fetch.
    Fetched { sha: CorpusSha },
}
```

The `Cached` vs `Fetched` distinction is internal-only for telemetry / log differentiation; both stamp the same 12-hex SHA into the SBOM annotation.

---

## `CorpusSha`

Typed newtype around a 20-byte git SHA-1.

```rust
pub(super) struct CorpusSha([u8; 20]);

impl CorpusSha {
    /// Parse a 40-hex string. Returns Err on wrong length or non-hex chars.
    pub fn from_hex(s: &str) -> Result<Self, ...>;

    /// 40-hex lowercase for cache directory names.
    pub fn to_full_hex(&self) -> String;

    /// 12-hex truncation for the SBOM annotation (matches `git rev-parse --short` default).
    pub fn to_short_hex(&self) -> String;
}
```

Build-time-embedded SHA is resolved via `env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")` at module-init time. The runtime override (`--fingerprints-rev <sha>`) re-parses through `from_hex`.

---

## `IndexEntry`

One line in `corpus/index.json`. Lets the loader batch-fetch + validate all library files in a single directory walk without `readdir`-driven discovery (more deterministic across filesystems).

| Field | Type | Required? | Notes |
|---|---|---|---|
| `library` | `String` | yes | Must match the `library` field of the referenced record + the filename stem. |
| `path` | `String` | yes | Relative path within the corpus dir, e.g., `openssl.json`. Always `<library>.json` in practice; explicit field for future-proofing. |
| `digest` | `Option<String>` | no | SHA-256 of the per-library JSON file's content. Optional defense-in-depth; sibling-repo CI populates if/when enabled. |

The aggregated `index.json` schema is `{"version": 1, "entries": [IndexEntry, ...]}`.

---

## Cache directory layout (on-disk entity)

```text
~/.cache/mikebom/fingerprints/
├── <full-40-hex-sha-A>/
│   └── corpus/
│       ├── index.json
│       ├── openssl.json
│       ├── zlib.json
│       └── ...
├── <full-40-hex-sha-B>/
│   └── corpus/...
└── .tmp-<uuid>/                                # Atomic-write staging; cleaned up on success/failure
```

**Invariants**:

- Each `<sha>/` directory is either fully populated + readable, OR absent. No partial state visible to readers.
- Atomic write: the fetcher extracts into `.tmp-<uuid>/` then `std::fs::rename`s to `<sha>/`. POSIX rename of an empty target is atomic per the kernel.
- Validation at load time: `<sha>/corpus/index.json` MUST exist + parse + have `version: 1`. Failure → treat the directory as corrupt; next fetch (if online + not `--offline`) overwrites.
- No subdirectories beyond `corpus/`. The `schema/` and `.github/` directories from the sibling repo are NOT extracted into the cache (only the runtime-needed `corpus/`).

---

## State transitions

`CorpusSource` has no transitions — it's set once per scan at corpus-load time and immutable thereafter. The cache directory transitions are managed by the fetcher (write-then-rename) and by `mikebom fingerprints cache-clear` (delete).

---

## Data-volume assumptions

| Scenario | Expected size |
|---|---|
| Per-library `corpus/<library>.json` | ~500 bytes – 2 KB (50–200 symbols + small metadata) |
| `corpus/index.json` at 100 libraries | ~5 KB |
| Entire `corpus/` directory tarball at 100 libraries | ~75 KB compressed |
| Entire `corpus/` directory uncompressed at 100 libraries | ~200 KB |
| In-memory `FingerprintCorpus` at 100 libraries | ~250 KB (Vec + HashSet for each record's symbols) |
| Cache disk space at 5 SHAs cached | ~1 MB total |

All linear in library count. No quadratic or higher scaling.
