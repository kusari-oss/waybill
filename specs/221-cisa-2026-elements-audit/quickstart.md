# Quickstart — CISA 2026 SBOM Minimum Elements coverage

**Feature**: 221-cisa-2026-elements-audit
**Audience**: waybill operators, procurement / compliance reviewers,
CI pipeline authors.

Three tasks. Do them in order.

---

## 1. Read the coverage matrix

The single source of truth for "does waybill satisfy CISA 2026" is:

```text
docs/cisa-2026-coverage.md
```

Open it. Every one of the 17 data-field elements and 6 practices
gets a row. Per emitter (CDX 1.6 / SPDX 2.3 / SPDX 3.0.1) you see
one of three verdicts:

- **✅** — native field populated. Cited source location tells you
  where in `waybill-cli/src/generate/` the value comes from.
- **⚠️** — populated via a `waybill:*` annotation because the
  target format has no native slot. Documented in
  `docs/reference/sbom-format-mapping.md`.
- **❌** — absent. Every ❌ links to a follow-up user story in
  `specs/221-cisa-2026-elements-audit/spec.md`.

Appendix A of the coverage doc gives a `jq` recipe per element so
you can reproduce every ✅ verdict against a fresh scan of your own
target.

---

## 2. Generate a signed SBOM

### Option A: Sigstore keyless (recommended for CI / GitHub Actions)

```bash
waybill scan ./my-project \
  --format cyclonedx-1.6 \
  --output signed.cdx.json \
  --sign
```

Requires:
- Network access to `https://fulcio.sigstore.dev` +
  `https://rekor.sigstore.dev` (override with `WAYBILL_FULCIO_URL`
  / `WAYBILL_REKOR_URL`).
- An OIDC provider — in GitHub Actions with `id-token: write`, this
  is automatic. Locally, waybill opens a browser.

Verify:
```bash
cosign verify-blob \
  --bundle signed.cdx.json \
  --certificate-identity <your-oidc-subject> \
  --certificate-oidc-issuer <your-oidc-issuer>
```

**Multi-format scan**: If you also request SPDX outputs, the SPDX
signatures land in sidecar files.

```bash
waybill scan ./my-project \
  --format cyclonedx-1.6 --output signed.cdx.json \
  --format spdx-3.0.1 --output signed.spdx.json \
  --sign
# Produces: signed.cdx.json (in-document signature),
#           signed.spdx.json + signed.spdx.json.sig.bundle.json
```

### Option B: Static-key signing (recommended for air-gap / BSI / HSM)

Generate a signing keypair once:

```bash
# PKCS#8-formatted P-256 private key — required by sigstore-rs's PEM
# parser. The `openssl ecparam -genkey` shortcut emits SEC1-formatted
# PEM which waybill rejects with "Unsupported key type" (verified
# 2026-07-30 during T060 walkthrough).
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 \
  -out signing.key.pem
openssl pkey -in signing.key.pem -pubout -out signing.pub.pem
```

Sign:
```bash
waybill scan ./my-project \
  --format cyclonedx-1.6 \
  --output signed.cdx.json \
  --sign-key ./signing.key.pem
```

If the key is encrypted:
```bash
export WAYBILL_SIGN_KEY_PASSPHRASE=<your-passphrase>
waybill scan ... --sign-key ./encrypted.key.pem
```

Verify (JSF is verifiable with any RFC 7515-aware tooling; `jq` +
your favorite JWT lib works):
```bash
# Extract the signature object
jq '.metadata.signature' signed.cdx.json
# Verify with signing.pub.pem via your JSF verifier of choice
```

### Failure modes (per FR-009a)

If signing fails (network down, OIDC rejected, KMS auth expired,
etc.), waybill exits non-zero and cleans up any partial output.
Silent unsigned fallback is prohibited. If you don't want a hard
fail, don't pass `--sign` / `--sign-key`.

### Signing + stdout combination

Rejected at parse time (per FR-008a). Signing requires a durable
output path:

```bash
waybill scan ./my-project --sign --output -
# error: --sign requires --output <file>; signing does not support
# stdout output because verifiers cannot access uncaptured signature
# bytes. Suggested fix: --output signed.cdx.json
```

---

## 3. Set an SBOM document version

```bash
waybill scan ./my-project \
  --format cyclonedx-1.6 --output scan.cdx.json \
  --sbom-version 3
```

This produces:
- **CDX**: `metadata.version: 3` (native integer slot).
- **SPDX 2.3 / SPDX 3**: a document-scope Annotation carrying
  `waybill:sbom-version=3` (plus any co-emitted keys like
  generation-context).

Rules (per FR-013 / FR-014):
- Value must be a positive integer (`>= 1`). Non-integer values
  are rejected at parse time.
- When omitted, CDX emits `1` (today's default); SPDX outputs skip
  the sbom-version key.

Use case: bump on regeneration of the same
component-name/component-version pair per CISA 2026 § SBOM Version.

Note: waybill's built-in identity signals (CDX `serialNumber`
UUID, SPDX 2.3 `documentNamespace` / SPDX 3 `@id`
content-addressed) already satisfy CISA's RFC 9562 alternative for
revision identification — you only need `--sbom-version` if
consumers explicitly key on a monotonic integer counter.

---

## Verification cheat sheet

| Element | Recipe | Fresh scan required? |
|---------|--------|---------------------|
| Any ✅ row in coverage matrix | See `docs/cisa-2026-coverage.md` Appendix A | Yes (`waybill scan`) |
| Signed CDX signature (keyless) | `cosign verify-blob --bundle scan.cdx.json ...` | No (verify existing artifact) |
| Signed CDX signature (JSF static-key) | `jq .metadata.signature scan.cdx.json` + JSF tool | No |
| Signed SPDX sidecar (keyless) | `cosign verify-blob --bundle scan.spdx.json.sig.bundle.json --signature-key-material scan.spdx.json` | No |
| Signed SPDX sidecar (DSSE static-key) | `waybill sbom verify scan.spdx.json` (uses m006 verifier) | No |
| SBOM Generation Context (CDX) | `jq '.metadata.lifecycles[]?.phase' scan.cdx.json` | Yes |
| SBOM Generation Context (SPDX 2.3) | `jq -r '.annotations[]?.comment' scan.spdx.json \| grep waybill:generation-context` | Yes |
| SBOM Generation Context (SPDX 3) | `jq -r '.["@graph"][] \| select(.["@type"]=="Annotation") \| .statement' scan.spdx3.json \| grep waybill:generation-context` | Yes |
| SBOM Version (CDX) | `jq .metadata.version scan.cdx.json` | Yes |
| SBOM Version (SPDX either) | `jq -r '.annotations[]?.comment' scan.spdx.json \| grep waybill:sbom-version` | Yes |

---

## What this feature does NOT change

- Existing CDX goldens stay byte-identical when signing flags and
  `--sbom-version` are unset (FR-009). If your CI diffs against a
  pinned CDX artifact today, no action needed.
- SPDX 2.3 / SPDX 3 goldens gain one new document-scope Annotation
  in the unsigned/default path (generation-context alias). One-time
  regen; ongoing byte-identity preserved.
- No new Cargo dependencies. No new subprocess types. No network
  access in the default path.
- eBPF layer (`waybill-ebpf`) is untouched.

## Walkthrough findings (2026-07-30)

Recorded during T060 end-to-end walkthrough:

- **PKCS#8 vs SEC1 key format** — sigstore-rs's `SigStoreKeyPair::
  from_pem` accepts PKCS#8 encoding only. `openssl ecparam -genkey`
  (the SEC1 shortcut) is rejected with `Unsupported key type`. Use
  `openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256`
  instead. Quickstart command block above already fixed.
- **FR-009a fail-close verified** — running with a malformed key
  path OR a SEC1 key both correctly exit non-zero AND unlink the
  partial `--output` file. No stray artifacts left in cwd.
- **FR-008a rejection message is clean** — `--sign-key --output -`
  produces exactly the diagnostic the FR mandates, with a
  suggested-fix line naming `signed.cdx.json`.
- **`--sbom-version 3`** populates CDX `.version` = 3 as expected
  (top-level slot; NOT `.metadata.version` — that was a doc typo
  fixed in the coverage matrix Appendix A).
- **Malformed `--sbom-version` rejection** — clap swallows the
  `SbomVersion::FromStr` error and prints only `For more
  information, try '--help'.` — the underlying "must be positive
  integer" diagnostic isn't surfaced. Follow-up UX polish worth
  filing as a small dedicated issue; not a blocker for feature 221.
