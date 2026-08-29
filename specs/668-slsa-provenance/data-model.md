# Phase 1 Data Model: SLSA Provenance predicate + subject shape

**Feature**: `668-slsa-provenance` | **Date**: 2026-08-28

**Nature of this data model**: Descriptive only. Waybill's Rust code neither constructs nor consumes these payloads — the GitHub action produces them and the `gh` CLI verifies them. This document captures the shape so operators, reviewers, and future maintainers know what to expect in the emitted attestations.

## Entities

### 1. SLSA Provenance Predicate (v1.0)

**Schema**: `https://slsa.dev/provenance/v1` (canonical URI). Structured JSON object emitted by `actions/attest-build-provenance@v3`.

**Fields** (per SLSA v1.0 spec — waybill inherits, does not customize):

| Field | Type | Populated by | Waybill-relevant value |
|---|---|---|---|
| `buildDefinition.buildType` | URI string | GitHub action | `https://actions.github.io/buildtypes/workflow/v1` |
| `buildDefinition.externalParameters.workflow` | Object | GitHub action | `{ref, repository, path}` naming the release workflow file |
| `buildDefinition.internalParameters` | Object | GitHub action | Runner metadata (arch, OS image) — not required for verification |
| `buildDefinition.resolvedDependencies[]` | Array | GitHub action | The source commit tree resolved to (source repo URI + digest) |
| `runDetails.builder.id` | URI string | GitHub action | `https://github.com/actions/runner` (SLSA-conformant builder ID) |
| `runDetails.metadata.invocationId` | URI string | GitHub action | The GitHub Actions run URL (`https://github.com/kusari-oss/waybill/actions/runs/<id>`) |
| `runDetails.metadata.startedOn` | RFC 3339 | GitHub action | Job start timestamp |
| `runDetails.metadata.finishedOn` | RFC 3339 | GitHub action | Job finish timestamp |
| `runDetails.byproducts[]` | Array | GitHub action | Empty for waybill's use case (no auxiliary artifacts) |

**Validation rules** (enforced by `gh attestation verify`, not waybill):
- `resolvedDependencies[]` MUST reference the source repo waybill was built from — a match against `--repo kusari-oss/waybill` is the FR-005 verification anchor.
- `metadata.invocationId` MUST match a real GitHub Actions run for the pinned repo (Rekor cross-verification).
- The Sigstore certificate chain wrapping this predicate MUST be issued to a workflow identity in `kusari-oss/waybill`.

### 2. SLSA Attestation Subject

**Schema**: in-toto Statement `subject[]` array element.

**Fields**:

| Field | Type | Populated by | Example |
|---|---|---|---|
| `name` | String | GitHub action | `waybill-v0.3.0-x86_64-unknown-linux-gnu.tar.gz` |
| `digest.sha256` | Hex string (lowercase, 64 chars) | GitHub action, from file bytes | `"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"` |

**Validation rules**:
- `digest.sha256` MUST match the SHA-256 of the artifact bytes (FR-006 tamper-detection anchor).
- `name` uniquely identifies the artifact within the release's subject-set. For OCI images, `name` is the image ref (`ghcr.io/kusari-oss/waybill@sha256:...`); for tarballs, `name` is the tarball filename; for the source SBOM, `name` is the SBOM filename.

**Multiplicity**: Each SLSA attestation MUST reference exactly one subject in waybill's use case. Multi-subject attestations are permitted by the spec but complicate the FR-015 self-verify path (which verifies subject-by-subject). One-subject-one-attestation is the simplest correct shape.

### 3. Sigstore Bundle

**Schema**: [Sigstore bundle v0.3](https://github.com/sigstore/protobuf-specs/blob/main/protos/sigstore_bundle.proto) — the wire format wrapping the in-toto Statement + its signature.

**Fields** (opaque to waybill; consumers extract via `gh` or `cosign`):

- `dsseEnvelope` — the signed DSSE envelope containing the base64-encoded in-toto Statement
- `verificationMaterial.certificate` — the ephemeral Sigstore Fulcio-issued cert with the workflow identity
- `verificationMaterial.tlogEntries[]` — Rekor transparency-log inclusion proofs

**Storage locations**:
1. **GitHub attestation store** (primary): each `attest-build-provenance` step uploads the bundle to `https://api.github.com/repos/kusari-oss/waybill/attestations`. Queryable via `gh attestation verify`.
2. **Sigstore Rekor** (mirror): every bundle is logged to the public Rekor instance at `rekor.sigstore.dev` for transparency. Independently queryable.
3. **Local bundle file** (for FR-010 mirroring): consumers can save the bundle as a file for offline / mirrored-registry verification. Recipe in `docs/verifying-releases.md`.

## Relationships

```
                ┌──────────────────────────────┐
                │  SLSA Provenance Predicate   │
                │  (buildDefinition + runDetails) │
                └──────────────┬───────────────┘
                               │
                               │ predicate
                               ▼
                ┌──────────────────────────────┐
                │  in-toto Statement            │
                │  _type = Statement/v1         │
                │  predicateType = slsa/v1     │
                │  subject[] ────────┐         │
                └──────────┬─────────┼─────────┘
                           │         │
                           │ envelope │ subject
                           ▼         ▼
                ┌──────────────────────┐   ┌──────────────────────┐
                │  Sigstore Bundle     │   │  Attestation Subject │
                │  (DSSE + cert + tlog)│   │  {name, digest.sha256}│
                └──────────────────────┘   └──────────────────────┘

                    Waybill emits one bundle per subject.
```

**Cardinality**: 1 release cycle → 6 bundles (4 tarballs + 1 OCI image + 1 source SBOM). Each bundle wraps exactly 1 predicate + exactly 1 subject.

## State transitions

Attestations are immutable once emitted (Rekor is append-only). The only state transition:

| State | Trigger | Next State |
|---|---|---|
| **Pending emission** | Build job produces artifact | Emission attempt |
| **Emission attempt** | `actions/attest-build-provenance` step runs | **Emitted** (success) or **Emission failed** (fail-closed per FR-008) |
| **Emitted** | Bundle appears in GitHub attestation store + Rekor | Awaiting self-verify |
| **Awaiting self-verify** | FR-015 `gh attestation verify` step runs | **Verified** (success) or **Verify failed** (fail-closed per FR-015) |
| **Verified** | Release job continues | Immutable, indefinitely queryable |
| **Emission failed** OR **Verify failed** | (any subject in the release) | Whole release job fails; no publication; no partial state |

## Validation rules summary

- **V1**: Every release artifact covered by FR-001/FR-002/FR-003 MUST end in the "Verified" state before publication (FR-008 + FR-015).
- **V2**: The bundle's Sigstore certificate identity MUST match `kusari-oss/waybill`'s release or nightly workflow (enforced by `gh attestation verify --repo kusari-oss/waybill`).
- **V3**: The subject's `digest.sha256` MUST equal the SHA-256 of the artifact bytes served from GitHub Releases / GHCR (enforced by `gh attestation verify <artifact>`).
- **V4**: The predicate's `resolvedDependencies[]` MUST reference the source repo — checked by `gh attestation verify --source-repo kusari-oss/waybill` (v2 of the `gh` CLI supports this flag).

## Non-entities (deliberately excluded)

- **A `WaybillProvenance` Rust struct**: not needed. Waybill's Rust code never constructs or parses this data.
- **A crate for SLSA emission**: not needed. The GitHub action is the emitter.
- **A crate for SLSA verification**: not needed for m668; will be needed for #725 (CLI-side offline verification) if that feature is picked up.
