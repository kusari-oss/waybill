# Contract: Corpus JSON Schema

**Lives in**: `kusari-sandbox/mikebom-fingerprints/schema/fingerprint-record.v1.json`
**Enforced by**: sibling-repo CI (PR-time validation)
**Trusted by**: mikebom-cli at scan time (defensive parse only; no re-validation)

## `corpus/<library>.json` schema (v1)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://github.com/kusari-sandbox/mikebom-fingerprints/schema/fingerprint-record.v1.json",
  "title": "mikebom symbol-fingerprint record",
  "type": "object",
  "required": ["library", "target_purl", "symbols", "min_symbols"],
  "additionalProperties": false,
  "properties": {
    "library": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9\\-\\.]*$",
      "minLength": 2,
      "maxLength": 64
    },
    "target_purl": {
      "type": "string",
      "pattern": "^pkg:[a-z][a-z0-9\\-]*\\/.+",
      "maxLength": 256
    },
    "symbols": {
      "type": "array",
      "items": {
        "type": "string",
        "pattern": "^[A-Za-z_][A-Za-z0-9_]*$",
        "maxLength": 128
      },
      "minItems": 10,
      "uniqueItems": true
    },
    "min_symbols": {
      "type": "integer",
      "minimum": 5,
      "maximum": 100
    },
    "version_hint": {
      "type": "string",
      "maxLength": 128
    },
    "variant": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9\\-]*$",
      "maxLength": 32
    },
    "notes": {
      "type": "string",
      "maxLength": 1024
    }
  }
}
```

## Cross-field invariants (enforced by sibling-repo CI script alongside schema validation)

- `symbols.length >= 2 * min_symbols` — otherwise the threshold is meaningless.
- `library` (with optional `variant` suffix) is unique across all corpus files. Two records with the same `(library, variant)` pair fail CI.
- `target_purl` parses cleanly via a PURL-spec parser (the sibling-repo CI invokes a small Rust binary that uses `packageurl = "0.4"` to verify; same parser as mikebom-cli's `Purl::new`).
- `symbols` does NOT include any of the curator-defined "common-prefix tripwire" terms: `init`, `start`, `open`, `close`, `read`, `write`, `error`, `version`, `info`, `debug` — these are too generic and would match unrelated binaries. (Curator can override per-record with a `notes:` justification, in which case CI emits a warning but doesn't block.)

## `corpus/index.json` schema (v1)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://github.com/kusari-sandbox/mikebom-fingerprints/schema/index.v1.json",
  "title": "mikebom symbol-fingerprint corpus index",
  "type": "object",
  "required": ["version", "entries"],
  "additionalProperties": false,
  "properties": {
    "version": {
      "type": "integer",
      "const": 1
    },
    "entries": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["library", "path"],
        "additionalProperties": false,
        "properties": {
          "library": { "type": "string" },
          "path": { "type": "string", "pattern": "^[a-z][a-z0-9\\-\\.]*\\.json$" },
          "digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" }
        }
      },
      "minItems": 1
    }
  }
}
```

## Schema evolution

The schema is versioned by **filename** (`fingerprint-record.v1.json`, future `v2`). When a breaking schema change is needed:

1. Land `fingerprint-record.v2.json` in the sibling repo alongside `v1`.
2. Update mikebom-cli to handle both schemas at load time (read the per-file `$schema` field or fall back to v1 if absent).
3. Migrate corpus files to v2 in a follow-up sibling-repo PR.
4. Eventually deprecate v1 — but only after a mikebom-cli release that handles both.

This is identical to JSON-LD-style versioning; well-trodden pattern.

## Example record

```json
{
  "library": "openssl",
  "target_purl": "pkg:generic/openssl",
  "symbols": [
    "SSL_CTX_new",
    "SSL_CTX_free",
    "SSL_new",
    "SSL_free",
    "SSL_connect",
    "SSL_accept",
    "SSL_read",
    "SSL_write",
    "SSL_shutdown",
    "BIO_new",
    "BIO_free",
    "EVP_DigestInit_ex",
    "EVP_DigestUpdate",
    "EVP_DigestFinal_ex"
  ],
  "min_symbols": 8,
  "version_hint": ">=3.0",
  "notes": "Public API stable across OpenSSL 3.x. min_symbols=8 because 6 of these can be present in any libcurl-using binary that doesn't statically embed openssl; 8+ is the practical floor for confident identification."
}
```
