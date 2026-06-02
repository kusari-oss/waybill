# Feature Specification: External symbol-fingerprint corpus via sibling repo + cache

**Feature Branch**: `108-fingerprint-corpus`
**Created**: 2026-06-02
**Status**: Draft
**Input**: User description: "Land issue #208 — external symbol-fingerprint corpus via sibling repo + cache. Move the symbol-fingerprint table out of `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs` into a sibling repo (`mikebom-fingerprints` or similar) as the source of truth. SHA-pin per mikebom release, cache locally per-host, stamp the corpus SHA on every emitted SBOM as provenance. Opt-in per Constitution XII; bundled defaults preserved as fallback. Same pattern as milestone 090's `mikebom-test-fixtures` split. Unblocks scaling symbol fingerprinting from today's 7 hand-curated libraries to potentially hundreds without bottlenecking on mikebom-cli PR review."

## Context

mikebom's milestone-096/099 symbol-fingerprint scanner identifies statically-linked C libraries (openssl, zlib, libcurl, sqlite, pcre, pcre2, gnutls — 7 libraries) from their `.dynsym` exports. The fingerprint corpus today lives **inline** in `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::FINGERPRINTS` — a hand-curated `const` table reviewed at PR time.

The growth model bottlenecks at maintainer review bandwidth. Every new library — libpng, freetype, boost, libssh2, nghttp2, zstd, libarchive, openldap, krb5, etc. — requires a mikebom-cli PR with curated public-API symbols, version-stability rationale, and prefix-distinctiveness justification. The practical ceiling is ~15–20 libraries before the in-source table becomes unmanageable.

This milestone moves the corpus to a sibling repo (working name `mikebom-fingerprints`) maintained as the source of truth. `mikebom-cli` pulls a SHA-pinned snapshot at scan time, caches locally per-host, and stamps the corpus SHA on every emitted SBOM as a `mikebom:fingerprint-corpus-sha` provenance annotation. This is the same pattern as milestone 090's `mikebom-test-fixtures` sibling-repo split — independently versioned, SHA-pinned, cache-first with offline fallback to bundled defaults.

The headline outcome: a maintainer adding a new library fingerprint touches only the sibling repo, not mikebom-cli. The contribution path shortens; the review surface narrows; the corpus scales from 7 libraries toward 100+ without revisiting mikebom-cli's `Cargo.toml`.

## Clarifications

### Session 2026-06-02

- Q: Corpus serialization format + sharding strategy? → A: **JSON, one file per library** at `corpus/<library>.json` + an `index.json` enumerating them. Aligns with mikebom's existing `serde_json` default; one-file-per-library scales to 100s of libraries without merge-conflict hotspots; the index supports cheap batch loading and shape validation in a single read.
- Q: Pinning strategy when `--fingerprints-corpus` is set without `--fingerprints-rev`? → A: **Build-time-embedded SHA** (reproducibility default). The corpus SHA is baked into mikebom-cli at build time; the same mikebom binary always produces the same SBOM for the same scan target. Operators wanting a fresher corpus opt in explicitly via `--fingerprints-rev <sha>`. Preserves SC-007's hermetic-build story and the existing byte-identity-goldens contract.
- Q: Minimum-match threshold for identification? → A: **Per-record `min_symbols` field** in each corpus entry. Curators tune per library based on the library's API stability (tightly-versioned libraries like sqlite can take 5; generic-named ones like zlib need 15+). Threshold travels with the corpus SHA so cross-scan reproducibility is preserved.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Maintainer adds a new library fingerprint (Priority: P1) 🎯 MVP

A security researcher discovers that mikebom doesn't identify statically-linked libxml2 in a customer's image. They open a PR to the **sibling repo** (`mikebom-fingerprints`) adding a libxml2 fingerprint record — a list of public-API symbols (`xmlParseDoc`, `xmlReaderNewMemory`, `xmlFreeDoc`, …), the target PURL form (`pkg:generic/libxml2`), and a version-stability rationale. **They do not need to open a mikebom-cli PR.** The sibling repo's CI validates the record's shape; mikebom-cli consumers pull the new corpus on their next scan (or via the next mikebom release's pinned SHA).

**Why this priority**: this is the headline outcome of the milestone. The current model's bottleneck is the inline `FINGERPRINTS` table; everything else this milestone does (caching, SHA stamping, opt-in defaults) is in service of making this scenario work cleanly.

**Independent Test**: clone the sibling repo, add a single fingerprint record to its `corpus/` directory, run the sibling repo's CI validation, push a PR; on merge, the SHA pin in mikebom-cli is updated and the next mikebom scan against a fixture binary linking against the new library emits a `pkg:generic/<libname>` component.

**Acceptance Scenarios**:

1. **Given** the sibling repo with N library fingerprints, **When** a contributor adds the (N+1)th record and the PR passes shape validation, **Then** the PR is mergeable without any mikebom-cli changes.
2. **Given** a mikebom-cli release pinned to corpus SHA `<X>`, **When** the sibling repo merges PRs advancing to SHA `<Y>`, **Then** the existing mikebom-cli release continues working unchanged (it uses the embedded `<X>` SHA) and the next mikebom-cli release picks up `<Y>`.

---

### User Story 2 — Operator opts into the external corpus for richer identification (Priority: P1)

A platform-security engineer runs `mikebom sbom scan --image <ghcr-uri>` on their production image. Today's bundled corpus identifies 3 of the image's 50 statically-linked binaries. With the external corpus enabled (via `--fingerprints-corpus` flag or `MIKEBOM_FINGERPRINTS_CORPUS=1` env var), mikebom auto-fetches the sibling-repo corpus on the first scan, caches it locally, and matches 25 of the 50 binaries. Subsequent scans are network-free.

**Why this priority**: this is the operator-visible value-delivery story. Without the opt-in flag working, the milestone delivers maintainer convenience but no downstream impact.

**Independent Test**: scan an image fixture containing a representative set of statically-linked-but-stripped binaries against the bundled corpus (7 libraries) vs the external corpus (≥20 libraries after seeding). Verify component-count delta + verify the SBOM carries `mikebom:fingerprint-corpus-sha` on components identified via the external corpus.

**Acceptance Scenarios**:

1. **Given** a binary statically linked against libxml2 (which is in the external corpus but not the bundled fallback), **When** the operator scans with `--fingerprints-corpus`, **Then** the SBOM emits a `pkg:generic/libxml2` component carrying `mikebom:source-mechanism: "symbol-fingerprint"` + `mikebom:fingerprint-corpus-sha: <12-hex>`.
2. **Given** the same scan WITHOUT `--fingerprints-corpus`, **When** mikebom runs against the same binary, **Then** no libxml2 component emerges (bundled corpus has only the 7 default libraries) — but the binary still emits its `pkg:generic/<basename>?file-sha256=<hex>` placeholder. **No regression for the bundled-corpus-only path.**

---

### User Story 3 — SBOM consumer verifies which corpus version produced a match (Priority: P2)

A vulnerability-triage analyst receives an SBOM claiming a binary contains libxml2 v2.12.0. They want to know "did mikebom identify this from a 6-month-old corpus or from yesterday's?" They look at the component's `mikebom:fingerprint-corpus-sha` annotation, find it points at sibling-repo SHA `<X>`, and check out that SHA from the sibling repo to inspect the exact fingerprint rules that produced the match. They confirm or override.

**Why this priority**: provenance and reproducibility — Constitution X (Transparency) compliance for any identification produced by the heuristic fingerprint path. Without this annotation, the heuristic identification is opaque.

**Independent Test**: take an emitted SBOM with `mikebom:fingerprint-corpus-sha: <X>`, check out `<X>` from the sibling repo, locate the matching fingerprint record, and confirm its symbol list matches what's used in the live mikebom-cli cache.

**Acceptance Scenarios**:

1. **Given** an SBOM with a fingerprint-derived component, **When** an analyst inspects the component's properties, **Then** a `mikebom:fingerprint-corpus-sha` annotation is present with a 12-hex SHA value pointing at a real commit in the sibling repo. (12-hex matches `git rev-parse --short` default; full 40-hex is reachable by an operator who needs the canonical form via the sibling-repo URL.)
2. **Given** two SBOMs produced by the same mikebom binary against the same artifact but against different corpus SHAs, **When** the analyst diffs them, **Then** the `fingerprint-corpus-sha` annotations differ AND any new component identifications are attributable to the corpus delta.

---

### User Story 4 — Air-gapped operator pre-fetches the corpus (Priority: P2)

A defense-industry operator scans inside an air-gapped environment with no internet access. They need to pre-fetch the corpus on an internet-connected machine, ship the cache to the air-gapped network, and run mikebom there. A `mikebom fingerprints fetch [--corpus-rev <sha>]` (or equivalent) subcommand resolves the desired SHA, fetches the sibling-repo contents, and writes them to a known cache location. The air-gapped scans then run with `--offline` and the cache is honored without any network calls.

**Why this priority**: hermetic / air-gapped operability is a Constitution III + Strict Boundary 2 ("No MITM proxy") concern. The opt-in story (US2) auto-fetches; this story makes the opt-out-of-network path practical.

**Independent Test**: run `mikebom fingerprints fetch --corpus-rev <sha>` on machine A; tar the cache directory; restore it on machine B; run `mikebom sbom scan --offline --fingerprints-corpus` against a fixture binary; verify the same identification fires as on machine A.

**Acceptance Scenarios**:

1. **Given** a populated cache from a prior fetch, **When** the operator runs `mikebom sbom scan --offline --fingerprints-corpus`, **Then** the scan completes WITHOUT network access and emits fingerprint-derived components carrying the cached corpus SHA.
2. **Given** an EMPTY cache + `--offline`, **When** the operator runs `mikebom sbom scan --fingerprints-corpus`, **Then** mikebom emits a clear stderr warning ("external corpus requested but cache is empty and network is disabled; falling back to bundled defaults") and proceeds with the bundled 7-library corpus.

---

### User Story 5 — Hermetic build pins the corpus SHA (Priority: P3)

A reproducible-builds shop wants two mikebom invocations from any machine, at any time, to produce byte-identical SBOMs for the same scan target. They build mikebom-cli with the corpus SHA embedded at compile time (`cargo build`-time embed via `Cargo.toml` / `build.rs`-style pin). At runtime, `--fingerprints-corpus` uses the embedded SHA by default; a `--fingerprints-rev <sha>` flag allows runtime override for the case where the operator wants to use a NEWER (or older) corpus than the one pinned to the binary's release.

**Why this priority**: this preserves the existing reproducibility guarantees mikebom-cli releases provide (byte-identity goldens). Without this, two mikebom v1.2.3 binaries on different machines could produce different SBOMs depending on what's in each machine's corpus cache.

**Independent Test**: pin mikebom-cli to corpus SHA `<X>` at build time; run the same scan on two machines (one with empty cache, one with a newer corpus cached); verify both emit byte-identical SBOMs (the build-time-embedded `<X>` SHA wins; the cache for `<Y>` is ignored unless `--fingerprints-rev Y` is passed).

**Acceptance Scenarios**:

1. **Given** mikebom-cli built with embedded corpus SHA `<X>`, **When** two operators run the same scan against the same artifact, **Then** both SBOMs share identical `mikebom:fingerprint-corpus-sha = <X>` annotations regardless of what's in their local caches.
2. **Given** the same mikebom-cli binary, **When** the operator passes `--fingerprints-rev <Y>` and `<Y>` is present in cache, **Then** the scan uses `<Y>` AND the emitted SBOM stamps `<Y>` as the corpus SHA (the override is reflected, not silently ignored).

---

### Edge Cases

- **Corpus repo unreachable on first fetch** (network error, registry down, sibling-repo not yet seeded with content): mikebom emits a single `tracing::warn!`, falls back to bundled 7-library defaults, and stamps SBOMs with `mikebom:fingerprint-corpus-sha = bundled` (sentinel value). Scan does NOT abort.
- **Cache corruption** (truncated JSON, partial fetch): mikebom detects the corruption on load, deletes the cache directory for that SHA, retries the fetch once if network is available; falls back to bundled defaults on retry failure.
- **Two libraries claim the same symbol set** (false-positive risk): the corpus enforces a per-record "min unique symbols" floor (Q3) and the runtime falls back to the higher-min-symbol-match record when both fire. Ties broken by record-insertion-order in the sibling repo's index file.
- **Corpus contains duplicate library names** (vendor fork of upstream): each record carries a `library-name` + optional `variant` discriminator. Both records emit independently if both match.
- **Operator overrides `--fingerprints-rev` with a SHA that doesn't exist in the sibling repo**: mikebom exits non-zero with a clear message naming the missing SHA and the resolved corpus repo URL.
- **Cache directory growth** (operator runs many scans across many corpus SHAs over time): mikebom does NOT auto-evict in this milestone. A `mikebom fingerprints cache-clear [--keep-rev <sha>]` subcommand provides explicit cleanup.
- **Bundled defaults out-of-sync with sibling-repo current** (the bundled 7 libraries' symbol lists drift from what the sibling repo has for the same 7 libraries): bundled defaults are the FALLBACK only — when the external corpus is enabled and reachable, its records win for those 7 libraries too.
- **Corpus record carries an invalid PURL form** (e.g., `pkg:bogus/...` from a contributor mistake): mikebom skips the malformed record with a `tracing::warn!` and continues; other records in the corpus still load.
- **Air-gapped operator wants the corpus baked into a Docker image**: the cache layout is documented (path under `~/.cache/mikebom/fingerprints/<sha>/`) so a Dockerfile `COPY` step is sufficient. Cache is plain-files JSON; no DB or daemon.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom MUST continue to use a bundled in-source fingerprint corpus (the current 7 libraries) when no external corpus is enabled. **No regression** for existing operators who don't opt in.
- **FR-002**: mikebom MUST support an opt-in flag (working name: `--fingerprints-corpus`; also accepts `MIKEBOM_FINGERPRINTS_CORPUS=1` env var) that, when set, causes the scanner to use the external corpus in preference to bundled defaults.
- **FR-003**: mikebom MUST cache the fetched corpus locally at `~/.cache/mikebom/fingerprints/<sha>/` (per-host, per-SHA). The cache is content-addressable: two SHAs do not collide; multiple SHAs may coexist.
- **FR-004**: mikebom MUST be cache-first: a fetch is performed only when the cache for the requested SHA is empty AND the operator has NOT passed `--offline`. With `--offline` and an empty cache, mikebom falls back to bundled defaults and emits a stderr warning.
- **FR-005**: mikebom MUST emit a `mikebom:fingerprint-corpus-sha` annotation on every emitted SBOM component that was identified via the symbol-fingerprint path. Value: 12-hex truncation of the corpus repo's commit SHA (matches `git rev-parse --short` default), OR the literal `bundled` when bundled defaults produced the match.
- **FR-006**: mikebom MUST support a runtime override `--fingerprints-rev <sha>` that selects a specific corpus SHA from the cache, overriding the build-time-embedded default. When the requested SHA is absent from the cache AND network is available AND not `--offline`, mikebom MAY auto-fetch it. When `--fingerprints-rev` is **explicitly set** AND the requested SHA is unavailable (cache miss AND (network unavailable OR `--offline`)), mikebom MUST exit non-zero naming the missing SHA — the operator explicitly asked for THIS SHA; silent fallback would subvert their intent. The bare `--fingerprints-corpus` path (no `--fingerprints-rev`) follows FR-004's cache-first + bundled-fallback semantics instead.
- **FR-007**: The sibling-repo SHA used at scan time MUST default to the **build-time-embedded SHA** baked into mikebom-cli at build time (the reproducibility default — preserves SC-007 and the byte-identity-goldens contract). The runtime override `--fingerprints-rev <sha>` is the ONLY mechanism for selecting a different SHA; absence of network connectivity, presence of newer SHAs on the corpus repo's `main` branch, and operator-local-cache state do NOT change the resolved SHA. When the build-time-embedded SHA is absent from the local cache, FR-004's cache-first fetch behavior applies (fetch when online + not `--offline`, otherwise fall back to bundled defaults with a stderr warning).
- **FR-008**: A new `mikebom fingerprints fetch [--corpus-rev <sha>]` subcommand MUST exist for air-gapped pre-fetch. When `--corpus-rev` is omitted, the build-time-embedded SHA is fetched. Exit zero on success; exit non-zero on network failure or missing-SHA-at-remote.
- **FR-009**: A `mikebom fingerprints cache-clear [--keep-rev <sha>]` subcommand MUST exist for explicit cache cleanup. With `--keep-rev <sha>`, only that SHA's cache directory is retained; others are removed.
- **FR-009a**: A `mikebom fingerprints list` subcommand MUST exist for cache introspection, enumerating per-SHA cache directories with their record count + last-modification timestamp on stdout. Useful for operators debugging "which corpus versions does this machine have available?" — particularly air-gapped operators verifying their pre-fetched cache transferred correctly. Read-only; never modifies the cache.
- **FR-010**: Corpus records MUST be stored as **one JSON file per library** at `corpus/<library>.json`, with an aggregated `corpus/index.json` enumerating the per-library files. Each record's schema includes at minimum: library identifier (or canonical name), target PURL form, list of public-API symbol names, **mandatory `min_symbols` integer field** declaring how many of the listed symbols must match a binary's `.dynsym` before mikebom emits the identification (per-record so curators tune per library — sqlite may use 5, zlib may use 15+; defaults are NOT inferred at runtime, the field is required), optional version range or version-stability marker, optional variant/fork discriminator. Records that fail schema validation (including missing `min_symbols`) are skipped with `tracing::warn!` at load time; other records still load.
- **FR-011**: The sibling repo MUST be PUBLIC and Apache-2.0 licensed (matching mikebom-cli's posture). Initial seed: the existing 7 libraries from `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::FINGERPRINTS`. Anyone may open a PR to contribute additional records.
- **FR-012**: The sibling-repo SHA pinned in mikebom-cli MUST be advanced via a normal mikebom-cli PR (i.e., updating the pinned SHA constant or `Cargo.toml` entry). This makes corpus version bumps reviewable on the mikebom-cli side without requiring a corpus content review.
- **FR-013**: When two corpus records fire on the same binary, mikebom MUST emit BOTH components (no silent deduplication). Cross-record collisions are recorded via `mikebom:also-detected-via` per the existing milestone-105 dedup pipeline pattern, so consumers can see which corpus records voted for the same coord.
- **FR-014**: The new fingerprint-corpus subsystem MUST be pure-Rust + filesystem + ONE narrowly-scoped network-operation TYPE (the GitHub archive-download fetch), invoked from exactly **two call sites** sharing one implementation: (a) the explicit `mikebom fingerprints fetch` subcommand, (b) the implicit cache-miss auto-fetch from `mikebom sbom scan --fingerprints-corpus`. Both delegate to `mikebom-cli/src/scan_fs/binary/fingerprints/fetch.rs::fetch_corpus`. No new subprocess invocations beyond `git` (already in mikebom's dependency closure per milestones 053 + 090). No new C dependencies.
- **FR-015**: The bundled in-source corpus MUST remain valid and useful as a fallback even when the external corpus is enabled but unreachable. Operators with no network access AND no pre-fetched cache MUST still get the 7-library baseline identification.

### Key Entities

- **Fingerprint corpus**: a versioned collection of fingerprint records. Source-of-truth lives in the sibling repo; mikebom-cli accesses it via SHA-pinned cache.
- **Fingerprint record**: a single library's identity claim. Carries library name, target PURL form, list of public-API symbol names, version-stability metadata, optional variant discriminator.
- **Corpus SHA**: the git commit SHA of the sibling-repo state at the time of fetch. Used as the cache directory key AND the value of the `mikebom:fingerprint-corpus-sha` SBOM annotation.
- **Cache directory**: per-SHA on-disk storage at `~/.cache/mikebom/fingerprints/<sha>/`. Layout mirrors the sibling-repo's `corpus/` directory verbatim for transparency + Docker-COPY-friendliness.
- **Bundled defaults**: the in-source `FINGERPRINTS` const at `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs:61` (current 7 libraries). Retained as the no-network / no-cache fallback.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After this milestone ships, a contributor adds a new library fingerprint to the sibling repo and the addition is consumed by the next mikebom-cli release WITHOUT requiring any code review of `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs`. Measurable: the file's `FINGERPRINTS` const stops growing post-milestone; new identifications come from the cache.
- **SC-002**: The sibling repo is seeded with at least the existing 7 libraries from the current bundled corpus on day 1 of the post-milestone state. Future PRs against the sibling repo expand coverage; this milestone does NOT itself add new libraries.
- **SC-003**: Operators who do NOT opt into the external corpus see ZERO behavioral change from this milestone. Bundled-corpus-only scans produce byte-identical SBOMs pre- and post-milestone for the same scan target. Verified via the existing 33 byte-identity goldens (which exercise the binary path through statically-linked fixture artifacts).
- **SC-004**: A scan with `--fingerprints-corpus` against an internet-accessible network completes the corpus fetch in ≤5 seconds on a typical broadband connection (verified by manual timing during operator first-run; not CI-asserted because CI broadband variance would create flake). Subsequent scans against the same SHA are network-free; cache-hit overhead vs the bundled-corpus path is **validated by inspection** (the cache load is a single `serde_json::from_str` over ~75 KB + a per-binary `HashSet<&str>` lookup, same big-O as the bundled-corpus path's in-source `&[FingerprintRecord]` iteration). Micro-benchmarking the delta is explicitly deferred — adding `tests/bench_corpus_load.rs` would introduce CI flakiness disproportionate to the signal.
- **SC-005**: Air-gapped operability verified end-to-end: a `mikebom fingerprints fetch` on machine A, followed by a tarball transfer of the cache directory to machine B, followed by `mikebom sbom scan --offline --fingerprints-corpus` on machine B, produces an SBOM with the same fingerprint-derived components as machine A would have produced.
- **SC-006**: Every component identified via the external corpus carries `mikebom:fingerprint-corpus-sha = <hex>` AND that SHA resolves to a real commit in the sibling repo (verified by HTTP GET against the sibling repo's tree-API endpoint).
- **SC-007**: Build-time SHA pinning works: building mikebom-cli with corpus SHA `<X>` produces a binary that stamps `<X>` on emitted SBOMs even when the operator's local cache contains a different SHA. Override via `--fingerprints-rev` is the only way to deviate.
- **SC-008**: The FR-014 audit (no new C deps, only existing `git` subprocess + the narrow corpus-fetch HTTP call) passes a build-time grep test similar to milestones 106/107's offline-mode audit. The corpus fetch's network surface is the ONLY allowed network call in the new subsystem.

## Assumptions

- **Sibling-repo hosting**: the corpus repo lives on GitHub under the `kusari-sandbox` org (or similar), matching mikebom-cli's hosting. Network fetch resolves via GitHub's archive download API (`https://github.com/<org>/<repo>/archive/<sha>.tar.gz`) — no GitHub API auth required for a public repo.
- **Cache lifetime**: per-SHA cache directories persist indefinitely. No automatic eviction in this milestone. Disk-pressure-aware eviction is a follow-up if needed.
- **Reproducibility expectations**: this milestone treats build-time embedded SHA as the default. Runtime override is operator-explicit (`--fingerprints-rev`) and the operator accepts the reproducibility break in exchange.
- **No CPE-database changes**: this milestone is about library IDENTIFICATION (PURL emission), not vulnerability matching. The milestone-097 CPE candidates path is unchanged.
- **No yara-rule alternative**: the corpus stays in mikebom's own format (per Q1). Yara-rule corpora are rejected per issue #208 (dep weight + audit-model mismatch).
- **No replacement of existing milestone-098 embedded-version-string scanning**: the symbol-fingerprint corpus complements but does not replace the curated version-string regex table. Some libraries are identified via embedded version strings only; others via symbol fingerprints; some by both.
- **Constitution Principle V native-field audit performed**: no CDX 1.6, SPDX 2.3, or SPDX 3 native field carries "which corpus version produced this match" semantically. The `mikebom:fingerprint-corpus-sha` annotation is parity-bridging per the documented exception in Principle V. Documented in `docs/reference/sbom-format-mapping.md`'s C-row catalog (next available row; T057 in `tasks.md`). The annotation rides existing format-native containers: CDX `properties[]`, SPDX 2.3 `annotations[]`, SPDX 3 graph-element Annotation.
- **Sibling-repo CI validates records at PR time**: schema validation, prefix-distinctiveness rule, ≥N symbol minimum. mikebom-cli at scan time treats records as TRUSTED (they passed the upstream review). Malformed records arriving via SHA override are silently skipped per FR-010.

## Out of Scope (this milestone)

- **Adding new library fingerprints beyond the existing 7** — the sibling repo is seeded with what's already in the in-source corpus; expansion happens via independent sibling-repo PRs after this milestone ships.
- **CPE-database lookup or expansion** — separate concern from PURL identification; tracked in milestone 097 + follow-ups.
- **Yara-rule corpora** — rejected per issue #208.
- **In-binary symbol obfuscation handling** (e.g., binaries built with `-fvisibility=hidden` that strip exports) — orthogonal; tracked as a separate concern.
- **Vulnerability matching** — the corpus identifies libraries, not CVEs. Downstream vuln-scanners take the emitted PURL and do their own matching.
- **Auto-update of the build-time-embedded SHA** — bumping the pinned SHA in mikebom-cli is a manual maintainer step (a PR). No background or scheduled corpus advancement.
- **Cache disk-space management** — explicit `cache-clear` subcommand only; no auto-eviction or LRU.
- **Corpus signing / cryptographic attestation of the sibling repo** — git SHA pinning is the integrity mechanism (collision-resistant against accidental drift; reproducibility-friendly). cosign-style signing of corpus releases is a possible follow-up.

## Open Clarifications (resolve via `/speckit.clarify`)

(All initial open clarifications resolved in Session 2026-06-02.)
