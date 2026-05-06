# Contract — milestone 076 document-level `subject:` identifier

Public API for the new `BuiltinScheme::Subject` variant + `--subject-hash` flag.

## CLI surface

### New flag (on both `mikebom sbom scan` and `mikebom trace run`)

```
--subject-hash <ALGO:HEX>
```

- Type: repeatable (`Vec<String>` in the parsed `Args`)
- Each occurrence parses as a single `subject:` identifier
- Algo: must be `sha256` or `sha512` per research §4
- Hex: must be lowercase hex of correct length (64 chars for sha256, 128 for sha512)

### Help-text shape (clap-derived)

```
--subject-hash <ALGO:HEX>
        Attach a `subject:` identifier declaring "this SBOM describes
        the artifact with the given content hash." Format:
        `sha256:<64-lowercase-hex>` or `sha512:<128-lowercase-hex>`.
        Repeatable for multi-subject SBOMs. On build-tier scans
        (`mikebom trace run`), subject identifiers are auto-detected
        from the in-toto attestation envelope's subject set; manual
        flags augment auto-detected entries (deduplicated by exact
        match per milestone 073). On source-tier and image-tier
        scans, no auto-detect runs; manual flags are the only
        source of `subject:` identifiers.
```

## Library surface (`mikebom-cli` crate)

### New `BuiltinScheme::Subject` variant

```rust
// In mikebom-cli/src/binding/identifiers/mod.rs

pub enum BuiltinScheme {
    Repo,
    Git,
    Image,
    Attestation,
    Subject,        // NEW
}
```

The variant is part of the public surface for scheme classification. External consumers reading mikebom's emitted SBOMs match on `subject` as a scheme name; the `Subject` variant is the in-process correspondent.

### New `validate_subject` function

```rust
// In mikebom-cli/src/binding/identifiers/validators.rs

/// Validate a `subject:` value (`<algo>:<hex>`). Accepts `sha256:` and
/// `sha512:` prefixes only. Hex MUST be lowercase, length must match
/// algo (64 chars for sha256, 128 for sha512). Anything else returns
/// IdentifierError triggering soft-fail to UserDefined per FR-005.
pub fn validate_subject(value: &str) -> Result<(), IdentifierError>;
```

### New auto-detect helper

```rust
// In mikebom-cli/src/binding/identifiers/auto_detect.rs

/// Convert an in-toto subject set into a Vec of `subject:`
/// `Identifier` instances. Subjects without a sha256 digest are
/// skipped with `tracing::info!` per FR-002 + 2026-05-06 clarification.
///
/// Caller passes the subject set already collected by the trace
/// pipeline (no I/O happens inside this function).
pub fn subject_identifiers_from_attestation_subjects(
    subjects: &[Subject],
) -> Vec<Identifier>;
```

The exact `Subject` type comes from the existing in-toto witness-v0.1 attestation builder; the function signature is shaped to take a slice of whatever in-process struct represents subjects today.

## Per-format wire mapping

### CDX 1.6

Document-level `subject:` identifiers ride `metadata.component.externalReferences[]` with `type = "attestation"` (research §1):

```json
{
  "type": "attestation",
  "url": "sha256:abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab",
  "comment": "auto-detected from build-tier in-toto subject `myapp`"
}
```

For multi-output builds, multiple entries appear in the array (one per subject), in witness-v0.1 lexical order.

This co-exists with milestone 073's `attestation:` IRI emissions in the same `externalReferences[]` array (they share `type` but differ by URL form: digest vs IRI).

### SPDX 2.3

`subject:` identifiers ride `Package.externalRefs[]` on the main-module package with `referenceCategory = "PERSISTENT-ID"`:

```json
{
  "referenceCategory": "PERSISTENT-ID",
  "referenceType": "subject",
  "referenceLocator": "sha256:abc1234567890..."
}
```

Plus the `creationInfo.creators[]` redundant text line per milestone 073 (extends the same dual-carrier pattern).

### SPDX 3

`subject:` identifiers ride `SpdxDocument.externalIdentifier[]`:

```json
{
  "type": "subject",
  "identifier": "sha256:abc1234567890..."
}
```

Native open-typed `externalIdentifier[].type` — no Principle V parity issue.

## Observable contract from outside the binary

### Build-tier auto-detect (default — no flags)

```bash
$ mikebom trace run --signing-key ./key -- ./build.sh
INFO build-tier auto-detected `subject:sha256:abc1234567890...` from in-toto subject `myapp`
INFO build-tier auto-detected `subject:sha256:def5678901234...` from in-toto subject `myapp-debug`
... (rest of trace flow unchanged)
```

The emitted build SBOM carries the `subject:` identifiers in its native carriers. The wrapping `.attestation.dsse.json` envelope continues to carry the same subjects in its `subjects[]` field; the SBOM-body identifiers are an additive duplicate optimized for SBOM-only consumption.

### Manual `--subject-hash` (source-tier or override)

```bash
$ mikebom sbom scan --path . \
    --subject-hash sha256:custom1234567890... \
    --output out.cdx.json
... (no auto-detect log line; manual values flow through)
```

### Skipped subject (no sha256 digest)

```bash
$ mikebom trace run -- ./build-with-only-sha512-subjects.sh
INFO subject `myapp` has no sha256 digest (available algos: [sha512]); skipping subject: identifier auto-emit. Pass --subject-hash sha512:<hex> manually if needed.
... (the subject does NOT appear in the emitted SBOM as a `subject:` identifier)
```

## Test contract

A new integration-test file `mikebom-cli/tests/identifiers_subject_and_component.rs` MUST cover (subject-identifier portion):

| Test | Acceptance scenario | Validates |
|------|--------------------|-----------|
| `build_tier_autodetects_subject_from_in_toto_subjects` | US1 §1 | FR-002, SC-001 |
| `build_tier_autodetect_skips_subject_without_sha256` | US1 §3 + 2026-05-06 clarification | FR-002 + edge case |
| `build_tier_autodetect_emits_one_subject_per_in_toto_subject` | US1 §2 | FR-002 + multi-output |
| `manual_subject_hash_flag_works_on_source_tier` | US2 §1 | FR-003, FR-006 |
| `manual_subject_hash_flag_repeatable` | US2 §2 | FR-003 |
| `subject_value_validation_soft_fails_to_user_defined` | US2 §3 | FR-005 |
| `subject_identifier_emits_in_all_three_formats` | FR-004 | per-format wire mapping |
| `cross_tier_handshake_image_digest_matches_build_subject` | US3 §1, SC-002 | FR-014 |

Plus unit tests on `validate_subject` for the regex edge cases per research §4.

## Performance contract (per FR-012, SC-001)

- `subject_identifiers_from_attestation_subjects` is a tight loop over the in-process subject set. O(N) in number of subjects; bounded by witness-v0.1's per-attestation cap.
- No I/O. No subprocess. <1ms even for N=100 subjects.
- Validation regex compiles once per scan invocation (lazy_static or once_cell pattern). Match is O(L) in value length.

## Determinism contract (per FR-012, SC-005)

- Auto-detected `subject:` identifier order matches the in-toto subject set's lexical order (already deterministic per witness-v0.1).
- Manual `--subject-hash` order matches operator supply order.
- Composition order in the build-tier identifier vec: `repo:` → `git:` → auto-detected `subject:` entries → manual `--subject-hash` entries → existing manual `--repo` / `--git-ref` / etc.
- Re-running with identical inputs produces byte-identical output.
