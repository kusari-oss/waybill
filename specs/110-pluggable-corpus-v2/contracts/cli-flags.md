# CLI flags + env-var conventions (FR-006, FR-007, FR-011 contracts)

## New flags on `mikebom sbom scan`

### `--fingerprints-source URL[=ENV_VAR]` (repeatable)

Declares an additional corpus source. Optional `=ENV_VAR` names the environment variable holding the bearer token for this source (the value itself never appears in argv).

```bash
mikebom sbom scan \
  --fingerprints-corpus \
  --fingerprints-source https://corpus.example/private.tar.gz=KUSARI_CORPUS_TOKEN \
  --fingerprints-source https://other.example/public-extras.tar.gz \
  --path ./build/
```

Multiple `--fingerprints-source` invocations are union'd with each other AND with sources from `MIKEBOM_FINGERPRINTS_SOURCES` env-var and the config file. The milestone-108 default source is always implicitly included unless `--fingerprints-source-no-default` is also passed.

### `--fingerprints-source-no-default`

Boolean. Suppresses the implicit milestone-108 default source. Use case: air-gapped operators who explicitly do NOT want any default-public-source fetch attempted, OR operators with their own internal mirror of the milestone-108 corpus who want to explicitly point at the mirror via `--fingerprints-source` instead.

### `--scan-as <purl-or-name>`

Operator override for self-identity resolution (research R8). Accepts:
- A bare library name: `--scan-as openssl` (case-insensitive match against record PURLs' `name` segment).
- A full PURL: `--scan-as pkg:github/openssl/openssl@*` (matches record PURL with same scheme/namespace/name; version segment ignored for self-suppression purposes).

When set, this overrides auto-detection from cmake `project()` / cargo / npm / pep621 / git-remote. When unset, the auto-detection ladder runs.

## Retained from milestone 108 (unchanged)

### `--fingerprints-corpus` (alias `MIKEBOM_FINGERPRINTS_CORPUS=1`)

Opt-in for ANY corpus loading. Without this flag, NO corpus is fetched or consumed regardless of `--fingerprints-source` declarations. Preserves milestone-108's opt-in semantics; this milestone doesn't change the gate.

### `--fingerprints-rev <SHA>` (alias `MIKEBOM_FINGERPRINTS_REV=<SHA>`)

Pins the SHA used when fetching the milestone-108 default source ONLY. Continues to work; semantics unchanged. For new sources declared via `--fingerprints-source`, the SHA pin is encoded in the source URL or in the source's `release.json` manifest (per the fetch protocol contract).

## New flags on `mikebom fingerprints fetch`

### `--source URL[=ENV_VAR]` (repeatable)

Fetch one or more specific sources. Default: all configured sources + the milestone-108 default (unless `--no-default` is set).

### `--force`

Bypass the 24-hour cache TTL. Re-fetches every targeted source unconditionally.

### `--no-default`

Skip the milestone-108 default. Useful with `--source` to fetch a specific source for testing without re-fetching the public corpus.

## Environment variables

| Variable | Purpose | Format |
|---|---|---|
| `MIKEBOM_FINGERPRINTS_CORPUS` | Boolean opt-in (alias for `--fingerprints-corpus`) | `1` / `0` / `true` / `false` |
| `MIKEBOM_FINGERPRINTS_REV` | Pin for milestone-108 default source | hex SHA |
| `MIKEBOM_FINGERPRINTS_SOURCES` | Comma-separated source URLs (alias for repeated `--fingerprints-source`) | `URL1[=ENV1],URL2[=ENV2],...` |
| `MIKEBOM_FINGERPRINTS_NO_DEFAULT` | Boolean alias for `--fingerprints-source-no-default` | `1` / `0` |
| `MIKEBOM_SCAN_AS` | Alias for `--scan-as` | PURL or bare name |
| `<custom-per-source>` | Bearer token for the source declaring `=<CUSTOM>` | opaque string |

## Config file (`~/.config/mikebom/config.toml`)

New `[fingerprints]` section:

```toml
[fingerprints]
sources = [
    { url = "https://corpus.example/private.tar.gz", credential_env = "KUSARI_CORPUS_TOKEN", allowed_issuers = ["https://github.com/kusari/mikebom-corpus-private/.github/workflows/release.yml@refs/tags/*"] },
    { url = "https://other.example/public-extras.tar.gz" }
]
no_default = false
```

Precedence (highest → lowest, but additive):
1. CLI flags
2. `MIKEBOM_FINGERPRINTS_*` env vars
3. Config file `[fingerprints].sources`
4. Implicit milestone-108 default (unless explicitly suppressed)

Sources from all four layers are UNION'd; no source-replacement semantics. Operators wanting "only this source, ignore defaults" use `--fingerprints-source-no-default` plus an explicit single `--fingerprints-source` flag.

## Error-message contract (SC-005)

When a configured source fails, mikebom emits a warning naming the source URL + a failure category from this closed set:

| Category | Trigger | Example operator message |
|---|---|---|
| `missing-credential` | source declared `credential_env=X` but `$X` is unset/empty | `WARN: fingerprints source https://corpus.example/private.tar.gz declared credential_env=KUSARI_CORPUS_TOKEN but the env var is unset; skipping this source. Set the env var or remove the source from configuration.` |
| `invalid-credential` | HTTP 401/403 from the source | `WARN: fingerprints source https://corpus.example/private.tar.gz returned HTTP 401 (Unauthorized); check that KUSARI_CORPUS_TOKEN value is current. Skipping this source for this scan.` |
| `network-unreachable` | DNS/connect/timeout/404 | `WARN: fingerprints source https://corpus.example/private.tar.gz unreachable (connect timed out); skipping this source for this scan. Other sources unaffected.` |
| `signature-mismatch` | Sigstore verification rejected the certificate's identity | `WARN: fingerprints source https://corpus.example/private.tar.gz signature verification failed (identity X did not match allowed_issuers); rejecting archive. Check the source's allowed_issuers config.` |
| `archive-malformed` | tar extraction failed or VERSION file rejected | `WARN: fingerprints source https://corpus.example/private.tar.gz archive malformed (VERSION file claims schema 3, unsupported); skipping this source. Check the source's release artifact format.` |

Every warning ends with a clear next-step (set env var / check token / etc.) per the SC-005 contract.
