# Contract: CLI flags added by feature 221

**Feature**: 221-cisa-2026-elements-audit
**Consumer surface**: `waybill scan` command in `waybill-cli/src/cli/generate.rs`

Three new flags. All are additive; all default to preserving existing
behavior. Mutual-exclusion and combination-with-existing-flags rules
enforced at `clap` parse time.

---

## `--sign` (bool, default off)

**Purpose**: Opt into Sigstore keyless signing (US2 / FR-007a).

**Effect**: When set, waybill:
1. Detects an OIDC provider (`OidcProvider::detect()` from m006).
2. Requests a short-lived signing cert from Fulcio.
3. Signs SBOM canonical bytes with the ephemeral P-256 privkey.
4. Uploads the signature + cert to Rekor.
5. Assembles a Sigstore Bundle.
6. Emits the Bundle into CDX `metadata.signature` (in-document) and
   into `<output>.sig.bundle.json` (SPDX sidecars).

**Mutually exclusive with**: `--sign-key`. If both are set, `clap`
rejects at parse time with `error: --sign and --sign-key are mutually
exclusive (US2/FR-007). Choose one.`

**Rejects when**: `--output -` is also set (FR-008a). Diagnostic:
`error: --sign requires --output <file>; signing does not support
stdout output.`

**Environment variables**:
- `WAYBILL_FULCIO_URL` — override Fulcio endpoint (default:
  `https://fulcio.sigstore.dev`; set to `https://fulcio.sigstage.dev`
  for staging).
- `WAYBILL_REKOR_URL` — override Rekor endpoint (default:
  `https://rekor.sigstore.dev`).
- `WAYBILL_OIDC_TOKEN` — explicit OIDC token; skips
  `OidcProvider::detect()` when set.

**Exit codes**:
- `0` — signing succeeded, SBOM + sidecars written.
- `1` — signing failed (Fulcio unreachable, Rekor unreachable,
  OIDC rejected, bundle assembly error). Any partial output file
  is unlinked (FR-009a).
- `2` — CLI parse error (mutual-exclusion violation,
  `--output -` combination).

---

## `--sign-key <REF>` (string, default unset)

**Purpose**: Opt into static-key signing (US2 / FR-007b).

**Effect**: When set, waybill:
1. Resolves the key ref (file path, KMS URI, or PKCS#11 ref).
2. Loads or references the private key (using
   `load_local_signer()` from m006 for PEM; new KMS + PKCS#11
   adapters added in Phase 2 tasks).
3. Signs SBOM canonical bytes.
4. Emits a JSF-conforming `signature` object into CDX
   `metadata.signature` (in-document).
5. Wraps SBOM bytes in a DSSE envelope for SPDX outputs and
   writes to `<output>.sig.json`.

**Accepted `<PATH>` forms (this feature — Phase 1)**:
- Filesystem path to a PEM-encoded ECDSA P-256, Ed25519, or RSA
  private key (matches existing m006 `load_local_signer` support).

**Deferred to follow-up milestones** (documented for future
compatibility; the `KeyRef` enum is extensible):
- `kms://<uri>` — cloud KMS reference.
- `pkcs11://<uri>` — PKCS#11 device reference.

**Passphrase**: If the key is PEM + encrypted, waybill reads the
passphrase from an env var named by `--sign-key-passphrase-env <NAME>`
(defaults to `WAYBILL_SIGN_KEY_PASSPHRASE` if unset). Never accepts
passphrases on the command line (avoids `ps`-leak).

**Mutually exclusive with**: `--sign`. Same diagnostic as above.

**Rejects when**: `--output -` is also set (FR-008a).

**Exit codes**: Same as `--sign`.

---

## `--sbom-version <N>` (integer, default unset → emits `1`)

**Purpose**: Set the SBOM document version (US4 / FR-013).

**Effect**: When set, waybill:
- CDX 1.6: emits `metadata.version: N` (native integer slot).
- SPDX 2.3: emits an `Annotation` on `SPDXRef-DOCUMENT` with
  `comment: "waybill:sbom-version=<N>;<other-kv-pairs>"`.
- SPDX 3: emits a top-level `Annotation` element with
  `subject: <SpdxDocument @id>` and same statement shape.

When unset, CDX `metadata.version` stays at `1` (current behavior);
SPDX outputs do not emit an sbom-version annotation (no
byte-difference from today's goldens).

**Accepted values**: positive integers `>= 1`. Rejects:
- `0`, negative numbers → `error: --sbom-version must be >= 1`
- Non-integer (`2.0`, `v2`, `latest`, empty string) →
  `error: --sbom-version must be a positive integer (matches CDX
  1.6 metadata.version schema)`
- Whitespace / control chars → same integer-rejection error

**Exit code on invalid value**: `2` (CLI parse error).

---

## `--output` (existing flag) — new validator

**No new flag**; the existing `--output <path>` grows a validator:
if the value is `-` (stdout) AND (`--sign` or `--sign-key`) is set,
reject at parse time per FR-008a.

Diagnostic:
```text
error: --sign / --sign-key requires --output <file>; signing does
not support stdout output because verifiers cannot access
uncaptured signature bytes. Suggested fix: --output signed.cdx.json
```

---

## Compatibility

- **No breaking change**: all existing invocations (no `--sign`,
  no `--sign-key`, no `--sbom-version`) produce byte-identical CDX
  output to today's goldens (FR-009).
- **SPDX goldens require regen** for the R3/R4 doc-scope Annotation
  addition, even in the unsigned no-sbom-version path — the
  annotation carries only the generation-context/CISA-alias pair
  and is always emitted. Golden regen scope documented in tasks.md.

---

## Help text (verbatim, to be added to `--help`)

```text
Cryptographic signatures (opt-in, per CISA 2026 § SBOM Author Signature):

      --sign
          Emit a Sigstore keyless signature (OIDC → Fulcio → Rekor →
          Sigstore Bundle). Bundle is emitted into the CDX
          `metadata.signature` slot and into
          `<output>.sig.bundle.json` sidecar files for SPDX outputs.
          Requires network access to Fulcio + Rekor + your OIDC
          provider. Requires --output <file> (not stdout). Mutually
          exclusive with --sign-key.

      --sign-key <REF>
          Emit a static-key signature using PEM / KMS / PKCS#11 key
          material referenced by <REF>. Produces JSF in CDX
          `metadata.signature` and DSSE in `<output>.sig.json` sidecar
          files for SPDX. Requires --output <file>. Mutually exclusive
          with --sign.

      --sign-key-passphrase-env <NAME>
          Read the PEM decryption passphrase from environment variable
          <NAME>. Defaults to WAYBILL_SIGN_KEY_PASSPHRASE.

Document versioning (opt-in, per CISA 2026 § SBOM Version):

      --sbom-version <N>
          Set the SBOM document version to N (positive integer, matches
          CDX 1.6 `metadata.version` schema). Defaults to 1. Emitted
          into CDX `metadata.version` natively and into SPDX 2.3 / SPDX
          3 as a document-scope Annotation.
```
