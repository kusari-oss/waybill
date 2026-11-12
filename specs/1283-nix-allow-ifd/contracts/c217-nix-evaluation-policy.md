# Contract: C217 `waybill:nix-evaluation-policy`

**Scope:** document. **Encoding:** a JSON object serialized as a string, keys sorted, like C204 and C216.

**Carriers:** CycloneDX `metadata.properties[]`; SPDX 2.3 document `annotations[]`; SPDX 3 document `Annotation`. **Parity:** `SymmetricEqual`, with an extractor in each format.

**Present when:** a Nix tier (`--nix-closure` or `--nix-eval`) was admitted, meaning it was requested and `--offline` was not set. It appears on success and on degradation. It is absent when no tier was requested. Under `--offline`, C190 already records `offline-requested`.

## Shape

```json
{
  "import-from-derivation": "refused" | "allowed",
  "verified": true | false,
  "sandbox": "true" | "false" | "relaxed" | null,
  "schema": 1
}
```

## Rules

- `import-from-derivation` is what was *requested*: `allowed` only with `--nix-allow-ifd`.
- `verified` is true only when the preflight confirmed nix reports the requested value. When it is false, C190 also carries `ifd-refusal-unverified` or `ifd-allowance-unverified`.
- `sandbox` is nix's reported `sandbox` value, verbatim, or null if nix didn't report one. waybill records it and never changes it.
- Without `--nix-allow-ifd`, the record is `{"import-from-derivation":"refused","verified":true,"sandbox":…}` whenever today's verification succeeds. This is the only new output for existing users. FR-001's "byte-identical" applies to everything except this new row, which SC-004's golden regeneration covers.

## Catalog row

KEEP-NO-NATIVE. CycloneDX 1.6, SPDX 2.3 and SPDX 3.0.1 have no construct for the configuration of a build-system evaluation (research R4).
