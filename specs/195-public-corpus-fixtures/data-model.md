# Data Model: Public SBOM Regression Corpus

**Date**: 2026-07-14
**Purpose**: Type shapes for the corpus manifest + per-target invariant records + cache-key records + failure diagnostics.

All types live in `mikebom-cli/tests/public_corpus/` and are integration-test-only (never linked into the shipped `mikebom` binary).

## Entity 1: `CorpusTarget`

The manifest entry — one per corpus target. Populated as a `const &[CorpusTarget]` slice in `mikebom-cli/tests/public_corpus/manifest.rs`.

```rust
pub struct CorpusTarget {
    /// Machine-friendly identifier, kebab-case; used as directory name
    /// under both cache and fixtures. Examples: "go-cobra",
    /// "image-postgres16", "rust-ripgrep".
    pub name: &'static str,

    /// Where the target's bytes come from.
    pub source: SourceKind,

    /// Pinned identifier — commit SHA (git) or image digest (OCI).
    pub pinned: PinnedRef,

    /// Ecosystem tag — drives per-target expected-invariant grouping
    /// and lets the harness verify SC-002 (one target per ecosystem).
    pub ecosystem: Ecosystem,

    /// Human-readable one-line description of what this target
    /// exercises. Shown in diagnostic output.
    pub exercises: &'static str,

    /// Layer 1 assertion function (per-target, see `layer1_assertions.rs`).
    pub layer1: fn(&EmittedSboms) -> Result<(), AssertionFailure>,
}

pub enum SourceKind {
    /// Publicly-cloneable git repository.
    Git { clone_url: &'static str },
    /// Publicly-pullable OCI image (via docker / crane / skopeo).
    OciImage { image_ref: &'static str },
}

pub enum PinnedRef {
    /// Git commit SHA — full 40-hex.
    Sha { hex: &'static str },
    /// OCI image digest — `sha256:<64-hex>`.
    Digest { algo_hex: &'static str },
}

pub enum Ecosystem {
    Go,
    Rust,
    Npm,
    Python,
    JavaMaven,
    PolyglotImage,
}
```

**Validation rules**:
- Every `CorpusTarget.source` MUST reference a public URL (FR-003). Validated by an audit test that greps the manifest for `kusari` substring (case-insensitive) — fails immediately if any Kusari hostname sneaks in.
- Every `CorpusTarget.pinned` MUST use a fully-qualified identifier — no partial SHAs, no tags. Enforced by `PinnedRef` shape (`hex` field is `&'static str`; the harness asserts `hex.len() == 40 && hex.chars().all(|c| c.is_ascii_hexdigit())` at test-time boot).
- The manifest MUST include at least one `CorpusTarget` per `Ecosystem` variant. Enforced by a `cross_ecosystem_coverage` unit test that checks presence.

**Initial manifest content** (per research §R1):

| name | source | pinned | ecosystem |
|---|---|---|---|
| `go-cobra` | Git `https://github.com/spf13/cobra` | (v1.9.1 SHA) | Go |
| `rust-ripgrep` | Git `https://github.com/BurntSushi/ripgrep` | (14.1.1 SHA) | Rust |
| `npm-express` | Git `https://github.com/expressjs/express` | (5.1.0 SHA) | Npm |
| `python-flask` | Git `https://github.com/pallets/flask` | (3.1.2 SHA) | Python |
| `maven-guice` | Git `https://github.com/google/guice` | (7.0.0 SHA) | JavaMaven |
| `image-postgres16` | OciImage `docker.io/library/postgres:16` | (sha256:...) | PolyglotImage |

The actual SHAs / digest are resolved at first-commit time via `scripts/corpus/refresh-pins.sh` and pinned into the manifest.

## Entity 2: `EmittedSboms`

The parsed output of one corpus target scan — all three formats, ready for Layer 1 assertion inspection.

```rust
pub struct EmittedSboms {
    /// Parsed CycloneDX 1.6 JSON.
    pub cdx: serde_json::Value,
    /// Parsed SPDX 2.3 JSON.
    pub spdx_2_3: serde_json::Value,
    /// Parsed SPDX 3.0.1 JSON.
    pub spdx_3: serde_json::Value,
    /// Path on disk to each emitted file (used by Layer 2 golden diff).
    pub paths: EmittedPaths,
}

pub struct EmittedPaths {
    pub cdx: PathBuf,
    pub spdx_2_3: PathBuf,
    pub spdx_3: PathBuf,
}
```

**Construction**: `harness::scan_target(&CorpusTarget) -> Result<EmittedSboms>` clones/pulls if needed (per cache logic §Entity 4), invokes `mikebom sbom scan ... --format cyclonedx-json,spdx-2.3-json,spdx-3-json --output <format>=<path>`, then reads + parses each file.

## Entity 3: `AssertionFailure`

Structured diagnostic returned by Layer 1 assertion functions when an invariant breaks.

```rust
pub struct AssertionFailure {
    /// Short identifier of the failed invariant, e.g. "graph-completeness",
    /// "stdlib-edge-present", "workspace-peer-edge-count".
    pub invariant_name: &'static str,

    /// Which SBOM format the assertion was checking (or "all" if
    /// cross-format).
    pub format: FailureFormat,

    /// The value the harness observed.
    pub observed: String,

    /// The value the harness expected (either literal or a
    /// human-readable range description like ">= 100").
    pub expected: String,

    /// One-line action hint per FR-009, e.g. "regenerate pins",
    /// "investigate mikebom regression", "check network / registry".
    pub suggested_action: &'static str,
}

pub enum FailureFormat {
    Cdx,
    Spdx23,
    Spdx3,
    /// Cross-format assertion — e.g. "graph-completeness value must
    /// match across all three formats".
    All,
}

impl std::fmt::Display for AssertionFailure {
    // Renders as a multi-line block per FR-009:
    //   ✗ <invariant_name> (<format>)
    //       observed: <observed>
    //       expected: <expected>
    //       next:     <suggested_action>
}
```

## Entity 4: `CorpusCacheKey` + `CorpusCacheDir`

Per research §R3, the cache layout mirrors milestone 090.

```rust
pub struct CorpusCacheKey {
    /// Hex-encoded first 16 chars of sha256(source_url_bytes).
    pub source_id_short: String,
    /// Raw pin — SHA (40 hex) or digest (sha256:<64-hex>).
    pub pin: String,
}

impl CorpusCacheKey {
    pub fn dir(&self, cache_root: &Path) -> PathBuf {
        cache_root
            .join("corpus")
            .join(&self.source_id_short)
            .join(&self.pin)
    }
}

pub struct CorpusCacheDir {
    pub root: PathBuf,   // ~/.cache/mikebom (from $HOME expansion or $XDG_CACHE_HOME override)
}

impl CorpusCacheDir {
    /// Ensures the pinned artifact is present at `key.dir(&self.root)`.
    /// - For git: `git clone <url> <dir>/repo && git -C <dir>/repo checkout <sha>`.
    /// - For OCI: `docker pull <image>@<digest>` (image lands in Docker daemon
    ///   storage; the corpus dir just holds a marker file recording the pull).
    /// - Sentinel: writes `<dir>/.corpus-pin-verified` on success. Presence of
    ///   the sentinel + matching pin skips re-clone/re-pull.
    pub fn ensure_hydrated(
        &self,
        target: &CorpusTarget,
    ) -> Result<PathBuf, CorpusInfraError>;
}
```

**Cache lifecycle**:
- Cache is per-user, persists across mikebom checkouts (FR-011).
- Cache is NEVER auto-cleared by the corpus. Operator can `rm -rf ~/.cache/mikebom/corpus/` to force re-hydration.
- On pin refresh: OLD pin's cache dir is untouched (stay-set semantics per milestone 090); the new pin's cache dir is populated on first corpus run.
- Multi-pin coexistence: `~/.cache/mikebom/corpus/<source-id-a>/<sha-1>/` and `<source-id-a>/<sha-2>/` can coexist safely (allows git-bisect-of-mikebom across corpus pin changes).

## Entity 5: `CorpusInfraError`

Distinguishes corpus-infra failures (FR-012 class b) from mikebom-regression failures (FR-012 class a).

```rust
pub enum CorpusInfraError {
    /// git clone / git checkout failed. Includes stderr in message.
    GitClone { target: &'static str, stderr: String },
    /// docker pull failed (image gone, registry down, docker not installed).
    OciPull { target: &'static str, stderr: String },
    /// mikebom binary invocation failed to produce expected output files.
    SbomEmission { target: &'static str, stderr: String, missing_files: Vec<PathBuf> },
    /// Cache-dir I/O failed (disk full, permissions).
    CacheIo { path: PathBuf, kind: std::io::ErrorKind },
    /// Docker (or equivalent OCI-pull tool) not on PATH — only fatal in CI;
    /// on developer machines, the harness treats this as a "skip
    /// image-tier targets" signal.
    OciToolMissing,
}
```

**Attribution rule** (FR-012):
- `CorpusInfraError` variants → "corpus-infra failure — actionable outside mikebom" diagnostic.
- `AssertionFailure` → "mikebom-behavior regression — actionable within mikebom" diagnostic.

## State Transitions

The corpus harness state machine per target is:

```text
[Start]
  ↓
CheckGate → if MIKEBOM_RUN_PUBLIC_CORPUS != "1": PRINT("skipping") + RETURN OK
  ↓
EnsureCache → if hydration fails: RETURN CorpusInfraError
  ↓
InvokeMikebom → if binary invocation fails: RETURN CorpusInfraError::SbomEmission
  ↓
ParseSboms → if JSON parse fails: RETURN CorpusInfraError (malformed emission)
  ↓
Layer1Assertions → if any fail: RETURN AssertionFailure (skip Layer 2)
  ↓
Layer2GoldenDiff → if MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1: WRITE golden files + PASS
                 → else: compare byte-identity after masking; RETURN AssertionFailure on diff
  ↓
[Pass]
```

## Cross-Cutting: Documentation of the Manifest

The corpus manifest is the source-of-truth artifact for what targets exist. A `list_targets` xtask (out of MVP scope) could pretty-print the manifest for humans; for MVP, the manifest is the code — reviewers read `manifest.rs` directly.
