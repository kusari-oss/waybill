# Phase 1: Data Model — CISA 2026 SBOM Minimum Elements coverage audit

**Feature**: 221-cisa-2026-elements-audit
**Date**: 2026-07-29

The feature adds five new types (three domain, two configuration) and
extends one existing enum with a mapping method. All new types
satisfy Constitution Principle IV (newtypes over raw `String`, no
`.unwrap()` in production, `thiserror` for library errors).

---

## New types

### `SbomVersion` (newtype, `waybill-common/src/types/sbom_version.rs`)

Newtype wrapping a `NonZeroU32` to match the CDX 1.6
`metadata.version` schema (`{"type": "integer", "minimum": 1}`).
Enforces the FR-013/FR-014 accept/reject contract at construction
time so downstream emitters cannot receive an invalid value.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct SbomVersion(NonZeroU32);

impl SbomVersion {
    pub const DEFAULT: SbomVersion = SbomVersion(NonZeroU32::MIN); // 1

    pub fn parse(raw: &str) -> Result<Self, SbomVersionError> {
        let n: u32 = raw.parse().map_err(|_| SbomVersionError::NotInteger)?;
        NonZeroU32::new(n).map(SbomVersion).ok_or(SbomVersionError::LessThanOne)
    }

    pub fn as_u32(self) -> u32 { self.0.get() }
}

#[derive(Debug, thiserror::Error)]
pub enum SbomVersionError {
    #[error("--sbom-version must be a positive integer (matches CDX 1.6 metadata.version schema)")]
    NotInteger,
    #[error("--sbom-version must be >= 1")]
    LessThanOne,
}
```

**Validation rules** (enforced in `parse`):
- Rejects empty string, non-numeric (`v2`, `latest`, `2.0`)
- Rejects any embedded whitespace or control chars (via `u32::from_str`
  which rejects them)
- Rejects `0` (via `NonZeroU32`)

**Serialization**: `#[serde(transparent)]` emits as a bare integer,
matching CDX schema. SPDX emitters format via `format!("{}",
version.as_u32())` for annotation strings.

---

### `SigningMode` (enum, `waybill-cli/src/sbom/signer.rs`)

Represents the operator's choice at CLI parse time. Mutually
exclusive by construction per FR-007.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigningMode {
    /// --sign — Sigstore keyless. OIDC → Fulcio → Rekor → Sigstore Bundle.
    Keyless {
        fulcio_url: String,        // default: https://fulcio.sigstore.dev
        rekor_url: String,         // default: https://rekor.sigstore.dev
        oidc_provider: OidcProvider,
    },
    /// --sign-key <REF> — static key material. Result: JSF signature.
    StaticKey {
        key_ref: KeyRef,
        passphrase_env: Option<String>,
    },
    /// Neither flag set (the default). Emitters produce byte-identical output.
    Unsigned,
}
```

`OidcProvider` and `KeyRef` reuse the existing m006 types from
`waybill-cli/src/attestation/signer.rs` — no duplication.

**State transitions**: None. `SigningMode` is constructed once at
CLI parse and consumed by emitters. If keyless mode fails at any
step (R6 sequence: OIDC → Fulcio → sign → Rekor → Bundle), the
error surfaces as `SbomSigningError` and the CLI fails-close.

---

### `SbomSignatureEnvelope` (enum, `waybill-cli/src/sbom/signer.rs`)

The output of the signing operation. What ends up in the CDX
`signature` slot or the SPDX sidecar file.

```rust
#[derive(Clone, Debug, serde::Serialize)]
#[serde(untagged)]
pub enum SbomSignatureEnvelope {
    /// Sigstore Bundle (protobuf-JSON). Emitted for keyless mode.
    /// Content-type: application/vnd.dev.sigstore.bundle+json;version=0.3
    Keyless(SigstoreBundle),

    /// JSF (JSON Signature Format, draft-cyberphone-jsf-00).
    /// Emitted for static-key mode into CDX; DSSE-wrapped for SPDX sidecar.
    StaticKeyJsf(JsfSignature),
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SigstoreBundle {
    #[serde(rename = "mediaType")]
    pub media_type: String, // "application/vnd.dev.sigstore.bundle+json;version=0.3"
    #[serde(rename = "verificationMaterial")]
    pub verification_material: VerificationMaterial,
    #[serde(rename = "messageSignature")]
    pub message_signature: MessageSignature,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct JsfSignature {
    pub algorithm: String,       // e.g., "ES256"
    #[serde(rename = "publicKey")]
    pub public_key: JsfPublicKey, // JWK-shaped
    pub value: String,            // base64url-encoded signature bytes
}
```

**Constraints** (from FR-007a/007b + FR-008):
- `Keyless` is emitted into CDX `signature` slot (in-document) AND
  into an SPDX sidecar at `<output>.sig.bundle.json` when SPDX 2.3
  or SPDX 3 is also being emitted.
- `StaticKeyJsf` is emitted into CDX `signature` slot. For SPDX 2.3
  and SPDX 3, waybill computes an additional DSSE envelope
  (existing m006 machinery) and writes it to `<output>.sig.json`.
- Neither variant is ever multiplexed into stdout — FR-008a rejects
  the combination at CLI parse time before this type is constructed.

**Serialization boundary**: The `serde::Serialize` impl produces
JSON bytes that go directly into the CDX slot or sidecar file. No
intermediate `String` step (avoids the raw-string boundary that
Principle IV forbids).

---

### `GenerationContextAlias` (extension to existing enum, `waybill-common/src/attestation/metadata.rs`)

Extends the existing `GenerationContext` enum with a mapping method
per R5. Does NOT add new variants — waybill's three variants remain
authoritative; the alias is a derived view.

```rust
impl GenerationContext {
    /// CISA 2026 § SBOM Generation Context vocabulary alias.
    /// Emitted alongside the waybill-native variant per FR-012.
    pub fn as_cisa_2026_lifecycle(&self) -> &'static str {
        match self {
            Self::BuildTimeTrace => "build",
            Self::FilesystemScan => "after-build",
            Self::ContainerImageScan => "after-build",
        }
    }

    /// Wire-format waybill-native identifier — matches existing kebab-case
    /// serde rename (`build-time-trace`, `filesystem-scan`, `container-image-scan`).
    pub fn as_waybill_native(&self) -> &'static str {
        match self {
            Self::BuildTimeTrace => "build-time-trace",
            Self::FilesystemScan => "filesystem-scan",
            Self::ContainerImageScan => "container-image-scan",
        }
    }
}
```

**Emission format** in SPDX 2.3 / SPDX 3 Annotation comment:
```text
waybill:generation-context=<as_waybill_native>;waybill:cisa-2026-lifecycle=<as_cisa_2026_lifecycle>
```

**Rationale**: Semicolon-separated key=value pairs match existing
waybill annotation convention (m080, m111, m145). Single Annotation
element carries both keys to keep SPDX 3 element count flat (R7
tail).

---

### `CisaCoverageMatrix` (in-memory model for the coverage doc, tests only)

Not emitted; used by `waybill-cli/tests/cisa_2026_coverage_matrix.rs`
to machine-verify every ✅ verdict in `docs/cisa-2026-coverage.md`
against a live scan (FR-017).

```rust
struct CisaCoverageMatrix {
    elements: Vec<CisaElement>,
    verdicts: HashMap<(CisaElementId, Emitter), Verdict>,
}

struct CisaElement {
    id: CisaElementId,       // e.g., ComponentHashValue, SbomAuthorSignature
    category: Category,      // Metadata | Component | Practice
    change_class: ChangeClass, // New | MajorUpdate | MinorUpdate | Removed | Unchanged
    definition_ref: &'static str, // section pointer in CISA 2026 doc
}

enum Emitter { CycloneDx16, Spdx23, Spdx301 }

enum Verdict {
    Native { source_cite: &'static str, jq_recipe: &'static str },
    Annotation { source_cite: &'static str, jq_recipe: &'static str, followup: Option<UserStoryId> },
    Absent { followup: UserStoryId },
    Practice { how_operator_satisfies: &'static str },
}
```

**Validation** (per FR-017): the integration test loads
`docs/cisa-2026-coverage.md`, parses it into a `CisaCoverageMatrix`,
runs a fresh `waybill scan` against the milestone-090 fixture repo,
and asserts that every `Verdict::Native` and `Verdict::Annotation`
row's jq_recipe extracts a non-empty value from the corresponding
emitter output. Regression that empties a native field fails the
test with a diff pointing to the failing (element, emitter) cell.

---

## Entity relationship diagram

```text
┌────────────────────┐        ┌──────────────────────┐
│  SigningMode       │──uses──│  SbomSignatureEnv    │
│  (CLI parse-time)  │        │  (emit-time output)  │
└────────────────────┘        └──────────────────────┘
         │                              │
         │emits-into                    │emits-into
         ▼                              ▼
┌────────────────────┐        ┌──────────────────────┐
│  CDX metadata.     │        │  SPDX sidecar file   │
│  signature slot    │        │  <output>.sig.*.json │
└────────────────────┘        └──────────────────────┘

┌────────────────────┐        ┌──────────────────────┐
│  SbomVersion       │──emits─│  CDX metadata.version│
│  (NonZeroU32)      │        │  (native integer)    │
└────────────────────┘        └──────────────────────┘
         │
         │also-emits (formatted string)
         ▼
┌────────────────────────────────────────────┐
│  SPDX 2.3 SPDXRef-DOCUMENT Annotation      │
│  SPDX 3 top-level Annotation on SpdxDocument│
│  Both share one annotation with generation- │
│  context per R7 element-count optimization  │
└────────────────────────────────────────────┘

┌────────────────────────┐        ┌──────────────────────┐
│  GenerationContext     │──has───│  CISA lifecycle alias│
│  (existing enum, +     │        │  (derived, &'static  │
│  as_cisa_2026_lifecycle │        │  str)                │
│  method)               │        └──────────────────────┘
└────────────────────────┘
         │
         │emits-both-into (semicolon-separated)
         ▼
┌────────────────────────────────────────────┐
│  Same SPDX Annotation shape as SbomVersion  │
│  above; one Annotation carries both signals │
└────────────────────────────────────────────┘
```

---

## Storage / persistence

None. All types are in-process for the duration of a single scan
(matches every emit-time metadata milestone since 002). Signing
material (Fulcio cert, Rekor entry) lives only in the emitted
Sigstore Bundle; no cache. Static key material stays on-disk under
the operator's control (`--sign-key <path>`); waybill reads it once
per invocation and drops the parsed keypair at scan end.

---

## Compatibility

- **Existing goldens**: unchanged when `--sign`, `--sign-key`,
  `--sbom-version` are all unset AND `ScanArtifacts.generation_context`
  is emitted as it was pre-feature into SPDX component annotations
  (the new doc-scope Annotation from R3/R4 is additive, so goldens
  DO need regen for SPDX 2.3 + SPDX 3 — but the CDX golden path
  stays byte-identical). Regen list per SC-007 documented in the
  tasks.md that `/speckit.tasks` will produce.
- **Existing types**: `GenerationContext` gets two `fn`s but no
  variant change → no serde-boundary shift → no consumer break.
