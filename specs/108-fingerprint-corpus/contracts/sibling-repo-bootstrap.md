# Contract: Sibling-repo bootstrap (`kusari-sandbox/mikebom-fingerprints`)

## What this repo IS

A standalone GitHub repository hosting the symbol-fingerprint corpus that mikebom-cli pulls at scan time. No Rust code; no Cargo workspace; just JSON + CI + docs.

## What this repo is NOT

- NOT part of the mikebom Cargo workspace.
- NOT a place for general "binary analysis" data — only the symbol-fingerprint corpus. CPE candidates, embedded version strings, etc. stay in mikebom-cli proper.
- NOT a place for binary blobs (test fixtures, golden artifacts). Those live in milestone 090's `mikebom-test-fixtures` repo.

## Repo creation

- **Owner**: `kusari-sandbox` (same as `mikebom`, `mikebom-test-fixtures`, `mikebom-tier-linkage-example`).
- **Visibility**: PUBLIC at creation. Operators outside the org can read + open PRs against it; that's the contribution model.
- **License**: Apache-2.0 (matching mikebom-cli). Contributors implicitly accept Apache-2.0 via the standard GitHub Terms.
- **Default branch**: `main`.
- **Branch protection on `main`**: requires CI green + one approving review.

## Initial content (Phase 1 seed)

### `README.md`

Brief overview. Sections:

- "What this corpus is" (links back to mikebom milestone 108 spec)
- "How to add a library" (links to `CONTRIBUTING.md`)
- "Schema" (links to `schema/fingerprint-record.v1.json`)
- "How mikebom consumes this" (links to `mikebom-cli`'s docs)
- License + maintainer notes

### `CONTRIBUTING.md`

Contribution flow:

1. Fork the repo.
2. Add a new `corpus/<library>.json` file conforming to `schema/fingerprint-record.v1.json`.
3. Run the validator locally (script in `scripts/validate.sh`; uses `ajv-cli` from npm or any conformant JSON Schema validator).
4. Open a PR. CI runs the validator + checks the cross-field invariants documented in `corpus-schema.md`. Reviewer approves; merge.
5. The new corpus SHA is picked up by the NEXT mikebom-cli release that bumps `[package.metadata.fingerprints].corpus_sha`.

Includes reviewer guidelines:

- Symbol list must be 10+ entries (CI enforces).
- `min_symbols` must be 5+ (CI enforces).
- `symbols.length >= 2 * min_symbols` (CI enforces).
- Symbols should be public API only (curator judgment).
- No common-prefix tripwire terms (CI enforces with a curator-overridable allowlist).
- `notes:` field should explain why min_symbols was chosen.

### `LICENSE`

Standard Apache-2.0 LICENSE text.

### `schema/fingerprint-record.v1.json`

The JSON Schema file from `corpus-schema.md`. Copy verbatim.

### `schema/index.v1.json`

The JSON Schema file for the corpus index from `corpus-schema.md`. Copy verbatim.

### `corpus/index.json`

```json
{
  "version": 1,
  "entries": [
    { "library": "openssl",  "path": "openssl.json" },
    { "library": "zlib",     "path": "zlib.json" },
    { "library": "libcurl",  "path": "libcurl.json" },
    { "library": "sqlite",   "path": "sqlite.json" },
    { "library": "pcre",     "path": "pcre.json" },
    { "library": "pcre2",    "path": "pcre2.json" },
    { "library": "gnutls",   "path": "gnutls.json" }
  ]
}
```

### `corpus/<library>.json` × 7

Seeded by exporting the existing `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::FINGERPRINTS` const into the JSON shape. Each `FINGERPRINTS` entry maps 1:1 to a `corpus/<library>.json` file. Phase 2 of the implementation generates these from the in-source const (via a small `mikebom-cli/tests/seed_fingerprint_corpus.rs` script that runs once at bootstrap time, then is removed).

The seed PR includes a note in `notes:` for each library explaining the `min_symbols` choice — the existing in-source corpus has implicit thresholds via the hand-coded N=10 default; the seed migration explicitly captures the threshold per library.

### `.github/workflows/validate-corpus.yml`

CI workflow that runs on every PR:

```yaml
name: validate corpus
on:
  pull_request:
    branches: [main]

jobs:
  validate:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@<pinned-sha>
        with:
          persist-credentials: false
      - name: install ajv-cli
        run: npm install -g ajv-cli@<pinned-version>
      - name: schema-validate per-library files
        run: |
          for file in corpus/*.json; do
            [ "$file" = "corpus/index.json" ] && continue
            ajv validate -s schema/fingerprint-record.v1.json -d "$file" --strict=true
          done
      - name: schema-validate the index
        run: ajv validate -s schema/index.v1.json -d corpus/index.json --strict=true
      - name: cross-field invariants
        run: scripts/validate-invariants.sh
```

`scripts/validate-invariants.sh` is a small Bash script enforcing:

- `symbols.length >= 2 * min_symbols`
- `(library, variant)` uniqueness across all corpus files
- No common-prefix tripwire terms (with allowlist via `# tripwire-ok: <reason>` comment in `notes:`)
- Every `corpus/*.json` file (except `index.json`) has a matching `entries[]` line in `index.json`

The script exits non-zero on any violation with a clear message naming the offending file + field.

## Phase-1 bootstrap PR checklist

(Sibling-repo's first PR, by the milestone-108 maintainer):

- [ ] `README.md` written
- [ ] `CONTRIBUTING.md` written
- [ ] `LICENSE` (Apache-2.0)
- [ ] `schema/fingerprint-record.v1.json` written
- [ ] `schema/index.v1.json` written
- [ ] `corpus/index.json` enumerates 7 libraries
- [ ] `corpus/openssl.json`, `zlib.json`, `libcurl.json`, `sqlite.json`, `pcre.json`, `pcre2.json`, `gnutls.json` written
- [ ] `.github/workflows/validate-corpus.yml` written
- [ ] `scripts/validate-invariants.sh` written
- [ ] CI green on the bootstrap PR
- [ ] Branch protection on `main` enabled (1-approving-review + CI-green required)

On merge, the resulting commit SHA is the value pinned in `mikebom-cli/Cargo.toml`'s `[package.metadata.fingerprints].corpus_sha` in Phase 2.

## Cross-repo PR cadence (post-bootstrap)

| Trigger | Action |
|---|---|
| New library added to `mikebom-fingerprints` | Sibling-repo PR; mikebom-cli unchanged until the next release bumps the pinned SHA. |
| Bug fix to an existing corpus record (typo in a symbol name, threshold too tight) | Sibling-repo PR; same model. |
| Mikebom-cli release (alpha bump) | The release maintainer ALSO updates `[package.metadata.fingerprints].corpus_sha` to the sibling-repo's `main`-branch tip SHA at release time. Mention in the release PR + CHANGELOG. |
| Schema breaking change | New `fingerprint-record.v2.json`; mikebom-cli updated to handle both v1 + v2; sibling-repo migrates files; eventually deprecates v1 in a follow-up release. Out of scope for this milestone. |
