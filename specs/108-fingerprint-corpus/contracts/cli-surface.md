# Contract: CLI surface

## New flags on `mikebom sbom scan`

### `--fingerprints-corpus`

**Type**: boolean (flag, no value).
**Default**: `false` (opt-in per Constitution XII).
**Env var equivalent**: `MIKEBOM_FINGERPRINTS_CORPUS=1` (or `true` / unset = false).
**Effect when set**: use the external symbol-fingerprint corpus in preference to the bundled in-source 7-library fallback. Triggers the cache-first / fetch-on-miss / fall-back-to-bundled-on-failure flow per FR-002 + FR-004.
**Help text**:

> "Enable the external symbol-fingerprint corpus from kusari-sandbox/mikebom-fingerprints. When off (default), the bundled in-source 7-library corpus is used. When on, mikebom uses the SHA-pinned external corpus, falling back to bundled defaults if the corpus can't be loaded (network unreachable, --offline + cache miss, etc.). See docs/reference/identifiers.md for the full opt-in chain."

### `--fingerprints-rev <SHA>`

**Type**: optional 40-hex git SHA string.
**Default**: unset (the build-time-embedded SHA is used).
**Validation**: must match regex `^[0-9a-f]{40}$`; mikebom exits non-zero on malformed values with a clear error message.
**Effect when set**: overrides the build-time-embedded corpus SHA. The cache is consulted for the requested SHA first; cache-miss + network available + not `--offline` triggers a fetch; cache-miss + (offline OR network failure) emits a warn + falls back to bundled.
**Implicit dependency**: requires `--fingerprints-corpus` (or `MIKEBOM_FINGERPRINTS_CORPUS=1`). When `--fingerprints-rev` is set without the opt-in flag, mikebom emits a warn ("--fingerprints-rev provided without --fingerprints-corpus; ignoring") and proceeds with bundled defaults.
**Help text**:

> "Override the build-time-embedded corpus SHA with a runtime-specified one. Format: 40-hex lowercase. Requires --fingerprints-corpus (or MIKEBOM_FINGERPRINTS_CORPUS=1). Use this to test newer corpora before they're embedded in a mikebom release."

## New top-level subcommand: `mikebom fingerprints`

### `mikebom fingerprints fetch [--corpus-rev <SHA>]`

**Purpose**: air-gapped pre-fetch (FR-008). Fetches the corpus tarball + extracts into the cache without performing any SBOM scan.

**Args**:

- `--corpus-rev <SHA>` (optional, default: build-time-embedded SHA): which corpus revision to fetch.

**Behavior**:

1. Validate the SHA (or use embedded). Exit non-zero on invalid hex.
2. If the cache already has this SHA: print `cache hit: <full-sha>` and exit zero (idempotent).
3. Otherwise, follow the fetch-protocol.md atomic-write flow.
4. On success: print `fetched: <full-sha> → <cache-path>` and exit zero.
5. On any fetch failure: print the specific error class (`network`, `404`, `5xx`, `disk-write`) and exit non-zero.

**Notes**: this subcommand is the ONLY one in mikebom-cli that's REQUIRED to perform a network call. The standard `sbom scan` flow's auto-fetch is permitted to network but ALSO has a non-network fallback path; this subcommand is the explicit "go to the network now" gate.

### `mikebom fingerprints cache-clear [--keep-rev <SHA>]`

**Purpose**: explicit cache cleanup (FR-009).

**Args**:

- `--keep-rev <SHA>` (optional): preserve the cache directory for this specific SHA; remove all others.

**Behavior**:

1. Validate `--keep-rev <SHA>` if provided (40-hex). Exit non-zero on malformed.
2. Iterate `<cache-root>/*`:
   - With `--keep-rev`: skip the matching directory; remove all others (including `.tmp-<uuid>/` staging dirs).
   - Without `--keep-rev`: remove every directory under `<cache-root>/`.
3. Print one absolute path per removed directory on stdout. Exit zero on success.

**Note**: idempotent. Running against an already-empty cache exits zero with no output.

### `mikebom fingerprints list`

**Purpose**: introspection — show what's currently cached.

**Args**: none.

**Behavior**: enumerate `<cache-root>/*` directories. For each, print:

- The full 40-hex SHA (directory name)
- The number of corpus records (`index.json::entries.length`)
- Timestamp of last modification

Useful for operators answering "what corpus versions does this machine have available?"

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | invalid argument (malformed SHA, etc.) |
| 2 | network error during `fingerprints fetch` (DNS, connection, 5xx after retries) |
| 3 | 404 from GitHub archive endpoint — the SHA doesn't exist in the corpus repo |
| 4 | disk-write error (permissions, ENOSPC) |
| 10 | other (uncategorized) |

These match mikebom-cli's existing exit-code conventions for other subcommands.

## Interaction with `--offline`

`--offline` is a top-level mikebom flag (predates this milestone). It disables network calls across the entire run.

| `--fingerprints-corpus` | `--offline` | Cache state | Behavior |
|---|---|---|---|
| off (default) | any | any | Bundled in-source 7-library corpus. No network. No cache I/O. |
| on | off | Hit | Cache load. No network. |
| on | off | Miss | Auto-fetch. Cache populated. Then load. |
| on | on | Hit | Cache load. No network. (Same as cache-hit + --offline-off.) |
| on | on | Miss | Warn + fall back to bundled. NO fetch attempted. |

`mikebom fingerprints fetch` ignores `--offline` (the subcommand's whole purpose is network access). `mikebom fingerprints cache-clear` is purely local; `--offline` has no effect.

## Help-text discoverability

`mikebom --help` lists `fingerprints` as a subcommand group (alongside `sbom`, `trace`, etc.). `mikebom fingerprints --help` lists `fetch`, `cache-clear`, `list`. Each subcommand's `--help` includes the FR-005 / FR-008 / FR-009 acceptance scenarios as worked examples.
