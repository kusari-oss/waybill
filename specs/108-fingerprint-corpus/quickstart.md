# Quickstart — Using the external symbol-fingerprint corpus

Once milestone 108 ships (v0.1.0-alpha.44 or later), mikebom can identify statically-linked C libraries beyond the bundled 7 (openssl, zlib, libcurl, sqlite, pcre, pcre2, gnutls) by consulting an external corpus at `kusari-sandbox/mikebom-fingerprints`. This guide walks through the three common operator scenarios.

---

## Scenario 1 — Opt into the external corpus for richer identification

Default scans use only the bundled 7-library corpus. To unlock the full corpus:

```bash
$ mikebom sbom scan \
    --image ghcr.io/myorg/my-app:v1.2.3 \
    --output sbom.cdx.json \
    --fingerprints-corpus
```

The first scan fetches the corpus tarball from GitHub (~75 KB at 100 libraries) into `~/.cache/mikebom/fingerprints/<sha>/`. Subsequent scans against the same SHA are network-free.

Every fingerprint-matched component in the emitted SBOM carries a `mikebom:fingerprint-corpus-sha` annotation:

```bash
$ jq '.components[]
        | select(.properties != null)
        | select(.properties[] | .name == "mikebom:source-mechanism" and .value == "symbol-fingerprint")
        | {name, purl, corpus_sha: (.properties[] | select(.name == "mikebom:fingerprint-corpus-sha").value)}' \
    sbom.cdx.json
```

Annotation value: 12-hex truncation of the corpus repo's commit SHA (matches `git rev-parse --short` default), OR the literal `bundled` if mikebom fell back to the in-source defaults (e.g., the operator was offline + had an empty cache).

---

## Scenario 2 — Air-gapped operator pre-fetches the corpus

Air-gapped operators run `mikebom fingerprints fetch` on an internet-connected machine, then ship the cache directory to the air-gapped network.

### On the internet-connected machine

```bash
$ mikebom fingerprints fetch
fetched: <full-40-hex-sha> → /home/user/.cache/mikebom/fingerprints/<sha>/
```

The default fetch resolves the build-time-embedded SHA. To fetch a different SHA (e.g., to test a corpus update before the next mikebom release):

```bash
$ mikebom fingerprints fetch --corpus-rev a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0
fetched: a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0 → /home/user/.cache/mikebom/fingerprints/a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0/
```

### Transferring to air-gapped

The cache directory is a plain JSON files tree — tar it, ship it, untar on the destination:

```bash
$ tar czf fingerprints-cache.tar.gz -C ~/.cache/mikebom fingerprints/
$ scp fingerprints-cache.tar.gz airgap-host:/tmp/
$ ssh airgap-host 'tar xzf /tmp/fingerprints-cache.tar.gz -C ~/.cache/mikebom/'
```

### Running mikebom in the air-gapped environment

```bash
$ mikebom sbom scan \
    --image local-registry/my-app:v1.2.3 \
    --output sbom.cdx.json \
    --fingerprints-corpus \
    --offline
```

`--offline` disables ALL network calls (including the corpus fetch — but the cache is honored). The SBOM stamps the same corpus SHA as the internet-connected machine would have produced. If the cache is empty under `--offline`, mikebom emits a warning and falls back to bundled defaults.

### Docker-friendly cache layout

The cache is a plain directory tree under `$HOME/.cache/mikebom/fingerprints/`. To bake it into a Docker image:

```dockerfile
FROM ghcr.io/kusari-sandbox/mikebom:v0.1.0-alpha.44
COPY --chown=1000:1000 fingerprints-cache/ /home/user/.cache/mikebom/fingerprints/
USER 1000
ENTRYPOINT ["mikebom", "sbom", "scan", "--fingerprints-corpus", "--offline"]
```

The container runs hermetically with no network access — the cache is fully self-contained.

---

## Scenario 3 — Hermetic / reproducible builds pin the corpus SHA at build time

For shops that build mikebom from source AND want strict reproducibility across machines:

### Building mikebom-cli with a pinned corpus SHA

`mikebom-cli/Cargo.toml` carries the pin in its package metadata:

```toml
[package.metadata.fingerprints]
corpus_sha = "<40-hex-sha-of-the-mikebom-fingerprints-repo>"
```

The `mikebom-cli` `build.rs` reads this at compile time and emits the SHA as the `MIKEBOM_FINGERPRINTS_CORPUS_SHA` env var. The compiled binary uses that SHA as the default whenever `--fingerprints-corpus` is set without `--fingerprints-rev`.

To override at build time (e.g., for a private fork):

```bash
$ cargo build -p mikebom -- --config 'package.metadata.fingerprints.corpus_sha = "your-custom-sha"'
```

Or fork the corpus repo entirely + change the `repository` URL — the build.rs reads the pin verbatim and doesn't validate it against any specific upstream.

### Verifying reproducibility

Two operators running `mikebom v0.1.0-alpha.44` against the same scan target should get byte-identical SBOMs (modulo timestamps) regardless of their local cache state — because the build-time-embedded SHA wins.

```bash
$ MIKEBOM_FIXED_TIMESTAMP=2026-06-02T00:00:00Z mikebom sbom scan \
    --image ghcr.io/myorg/my-app:v1.2.3 \
    --output sbom-machine-A.cdx.json \
    --fingerprints-corpus

# (run the same command on machine B with a different cache state)

$ diff sbom-machine-A.cdx.json sbom-machine-B.cdx.json
# (no output — byte-identical)
```

If you WANT to use a different corpus than what's embedded (e.g., to test a corpus advance before a mikebom release):

```bash
$ mikebom sbom scan --fingerprints-corpus --fingerprints-rev <newer-sha> ...
```

The `--fingerprints-rev` override is reflected on the SBOM annotation, so consumers can see the deviation from the build-time-embedded SHA.

---

## Inspecting + managing the cache

### List cached corpora

```bash
$ mikebom fingerprints list
<full-40-hex-sha-A>    23 records   2026-06-02T14:30:00Z
<full-40-hex-sha-B>    17 records   2026-05-15T08:21:42Z
```

### Clear the cache

```bash
$ mikebom fingerprints cache-clear
removed: /home/user/.cache/mikebom/fingerprints/<sha-A>/
removed: /home/user/.cache/mikebom/fingerprints/<sha-B>/
```

Or preserve a specific SHA:

```bash
$ mikebom fingerprints cache-clear --keep-rev <full-40-hex-sha-A>
removed: /home/user/.cache/mikebom/fingerprints/<sha-B>/
```

---

## What's NOT supported (this milestone)

- **Auto-update of the build-time-embedded SHA** — bumping the pinned SHA in `mikebom-cli/Cargo.toml` is a manual maintainer step (a PR). No background or scheduled corpus advancement.
- **Cache disk-space management** — explicit `cache-clear` only; no auto-eviction or LRU.
- **Corpus signing / cryptographic attestation** — git SHA pinning IS the integrity mechanism. cosign-style signing of corpus releases is a possible follow-up.
- **CPE-database lookup or expansion** — this milestone is about library IDENTIFICATION (PURL emission), not vulnerability matching.

These are tracked as follow-up items; see the milestone-108 spec's "Out of Scope" section for the full list.

---

## Troubleshooting

### "external corpus requested but cache is empty and --offline is set; falling back to bundled defaults"

Either run without `--offline` (lets mikebom auto-fetch) or pre-fetch with `mikebom fingerprints fetch` on an internet-connected machine and ship the cache.

### "corpus fetch failed: 404 Not Found"

The SHA you passed to `--fingerprints-rev` doesn't exist in the corpus repo. Verify the SHA is reachable at `https://github.com/kusari-sandbox/mikebom-fingerprints/commits/<sha>` before retrying.

### "scan produced fewer components than expected"

Check whether `--fingerprints-corpus` is set. The bundled 7-library default identifies less than the external corpus. Then verify the binary actually contains the library's symbols:

```bash
$ readelf -W --dyn-syms /path/to/binary | grep -E "SSL_|EVP_"
```

If symbols are absent, the binary likely has its exports stripped — that's an out-of-scope limitation for milestone 108 (tracked separately).

### "the SHA in the SBOM annotation doesn't match what I expected"

Inspect with `mikebom fingerprints list` — the embedded SHA may differ from what's currently cached. Use `--fingerprints-rev` to force a specific SHA if reproducibility matters more than freshness.

---

## Further reading

- Spec: `specs/108-fingerprint-corpus/spec.md`
- Plan: `specs/108-fingerprint-corpus/plan.md`
- Data model: `specs/108-fingerprint-corpus/data-model.md`
- Per-component contracts: `specs/108-fingerprint-corpus/contracts/`
- Sibling repo: https://github.com/kusari-sandbox/mikebom-fingerprints (post-bootstrap)
- mikebom binary analysis architecture: `docs/architecture/binary-analysis.md` (existing)
