# Research — milestone 110 (Pluggable fingerprint corpus v2)

Date: 2026-06-03
Branch: `110-pluggable-corpus-v2`
Spec: [spec.md](./spec.md)
Plan: [plan.md](./plan.md)

This document resolves the technical unknowns surfaced by the plan's Technical Context section. Each item is structured as **Decision / Rationale / Alternatives considered** per the speckit-plan template. The standards-native audit (R1) is the constitution-principle-V gate; everything else is implementation-detail nailing-down.

---

## R1 — Standards-native annotation audit (Constitution Principle V, fifth bullet)

**Question**: For each `mikebom:*` annotation the spec proposes, does CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 already carry the same semantic natively?

**Decision** (per-annotation table):

| Spec annotation | CDX 1.6 native carrier | SPDX 2.3 native carrier | SPDX 3.0.1 native carrier | Final landing |
|---|---|---|---|---|
| `mikebom:identification-confidence` | **YES** — `component.evidence.identity[0].confidence` (already wired via existing C16 + D1 in `docs/reference/sbom-format-mapping.md`) | NO native confidence field in 2.3; existing C16 annotation pattern applies | NO confidence in 3.0.1 stable (evidence profile not yet stable); existing C16 annotation pattern applies | **Re-use existing C16 `mikebom:confidence`**. Spec text is updated to drop the redundant `mikebom:identification-confidence` name in favor of the established C16 carrier (CDX-native via `evidence.identity.confidence`; SPDX annotation via C16's existing parity row). NO new annotation name introduced. |
| `mikebom:indicators-matched` | **PARTIAL** — `evidence.identity[].methods[]` carries `{technique, confidence, value}` per method. We map each matched indicator type to a method entry: `technique = "binary-analysis"` (CDX enum) with a `mikebom:indicator-kind` per-method field naming the indicator (`exported_symbols` / `version_string` / `build_id` / `abi_marker` / etc.) | NO native equivalent | NO native per-method evidence in 3.0.1 stable | **CDX native via `evidence.identity[].methods[]`**, one entry per matched indicator type. SPDX 2.3 + SPDX 3 use a new `mikebom:indicators-matched` annotation (parity-bridge per principle V's last paragraph) carrying a JSON-array string of indicator-kind enum values, deterministically sorted. **New annotation row required**: C59 in `sbom-format-mapping.md`. |
| `mikebom:purl-aliases` | NO native per-component multi-PURL field (CDX 1.6 has ONE `purl` per component + `cpe` separately). | **YES** — `Package.externalRefs[]` with `referenceCategory = PACKAGE-MANAGER`, `referenceType = purl`, multiple allowed. | **YES** — `software_Package.externalIdentifier[]` with `IdentifierType.purl`, multiple allowed. | **CDX-side annotation, SPDX-side native**. CDX 1.6 uses a new `mikebom:purl-aliases` annotation (JSON-array string, deterministic sort, parity-bridge per principle V). SPDX 2.3 emits via `externalRefs[]`; SPDX 3 emits via `externalIdentifier[]`. **New annotation row required**: C60 in `sbom-format-mapping.md` documenting the CDX-only-annotation justification. |
| `mikebom:also-detected-via` | **YES** — established C56 hybrid pattern (CDX native via `evidence.identity[].methods[].mikebom-source-mechanism`; SPDX annotation). | Established annotation (C56) | Established annotation (C56) | **Re-use existing C56**. Extend the C56 row's documentation to note multi-record corpus-collision is a new source-mechanism class but the carrier is unchanged. NO new annotation name. |
| `mikebom:identification-version-range` (mentioned in spec edge case) | NO native per-component "version range that this identification matches" field. CDX's `component.version` is a single string. | NO native equivalent. | NO native equivalent. | **All three formats use a new `mikebom:identification-version-range` annotation** carrying a SemVer range string. Only emitted when the matched record's `version_range` is wider than a single version. **New annotation row required**: C61 in `sbom-format-mapping.md`. |
| `mikebom:fingerprint-corpus-sha` (from milestone 108) | Established as C58. | Established as C58. | Established as C58. | **Re-use existing C58**. Extend the C58 row to note multi-source archives (one SHA per source the matcher consulted) rather than the single-archive milestone-108 semantic — value becomes a JSON-array string of `{source_id, sha}` pairs when multiple sources contributed, single-string preserved for backward compat when only one source matched. |

**Rationale**: Every new spec annotation either re-uses an established row (C16, C56, C58) or earns a documented parity-bridge justification (C59, C60, C61) per the constitution's principle-V audit clause. No annotation is introduced without first verifying its native equivalent exists or documenting why it can't.

**Action items for the implementation phase**:
- Update `docs/reference/sbom-format-mapping.md` with rows C59 (`mikebom:indicators-matched`), C60 (`mikebom:purl-aliases`), C61 (`mikebom:identification-version-range`); extend C56 + C58 wording for multi-source semantics.
- Spec's FR-013 — rename `mikebom:identification-confidence` references to `mikebom:confidence` (matches existing C16).
- Update the parity extractors in `mikebom-cli/src/parity/extractors/` to cover the new rows for the CI parity-row coverage check.

**Alternatives considered**:
- Coining a new `mikebom:identification-confidence` separately from the existing C16 `mikebom:confidence` — rejected because C16 is the established carrier and a new name would split parity tooling.
- Emitting `purl-aliases` as a CDX `externalReferences[]` entry with `type: distribution` — rejected because the CDX `externalReferences` semantic is for fetch URLs / download locations, not alternative identifiers; `cpe` is the only multi-identifier-like field CDX provides per-component and we already use it for the `cpe_candidates` work from milestone 097 (C32).

---

## R2 — Confidence fusion algorithm (numerical pin)

**Question**: The spec says `high` and `medium` are derived from per-indicator confidence baselines via a "documented and version-stable fusion rule" (FR-017). What's the exact rule?

**Decision**: Adopt the design doc §7 "max + bump" rule with numerical buckets:

```text
confidence = max(per-indicator confidence) over all matching indicators of a single record
for each AGREEING additional indicator: confidence = min(0.99, confidence + 0.05)
```

Bucket mapping:
- `0.70 ≤ confidence ≤ 0.84` → bucket `medium`
- `0.85 ≤ confidence` → bucket `high`
- `confidence < 0.70` → suppressed (no emission per the 2026-06-03 clarification)

Per-indicator baselines (carried in the v2 record schema as `confidence_baseline` per indicator):

| Indicator kind | Baseline |
|---|---|
| Build-ID exact match (`build_id`) | 0.99 |
| LC_UUID exact match (`macho_uuid`) | 0.99 |
| PE PDB GUID exact match (`pe_pdb`) | 0.99 |
| Embedded version string (`version_string`) | 0.95 |
| ABI markers (`abi_marker`, e.g., `OPENSSL_3_0_0` versioned ELF symbols) | 0.80 |
| Full exported-symbol match (`exported_symbols` 10/10) | 0.85 |
| Threshold-met exported symbols (`exported_symbols` ≥ `min_match` < total) | 0.70 |
| Single weak indicator (`weak`) | 0.40 (always below floor → suppressed) |

**Rationale**: This is the simplest defensible rule that maps the design doc's calibrated baselines into the spec's two-bucket emission model. Deterministic + version-stable (no learned weights, no per-mikebom-version drift). v1 backward compat (FR-005, the 2026-06-03 clarification) cleanly fits: v1 records have one indicator with baseline 0.70 → `medium`. The "+0.05 per agreeing indicator" rule lets a binary that matches BOTH exported symbols AND version string land at 0.85+ → `high` even when symbols-alone would only be `medium`.

**Alternatives considered**:
- **Bayesian fusion** with per-indicator likelihood ratios — rejected for v2 because it requires a calibrated prior, more test data than we currently have, and harder-to-debug confidence outputs. Promoted to a v3+ research item.
- **Three-bucket emission** (`high` / `medium` / `low` with `low` emitting `pkg:generic/` candidates) — rejected per the 2026-06-03 Q1 clarification; `low` and below are suppressed entirely this milestone.
- **Sum of confidences instead of max** — rejected because confidence values aren't independent probabilities (exported_symbols + version_string overlap in what they observe) and summing can overshoot 1.0 quickly.

---

## R3 — Cache layout (multi-source extension of milestone-108 pattern)

**Question**: Milestone 108 uses `~/.cache/mikebom/fingerprints/<sha>/`. How does this extend to N concurrent sources with the 24h TTL from FR-012a?

**Decision**: Per-source subdirectory keyed on a stable hash of the source URL:

```text
$XDG_CACHE_HOME (or ~/.cache)/mikebom/fingerprints/
├── public-milestone-108/                            # Source ID hash for the milestone-108 default URL
│   ├── <pinned-sha-1>/                              # Per-pinned-content-SHA subdir (milestone-108 pattern)
│   │   ├── corpus/                                  # Extracted archive contents
│   │   ├── archive.tar.gz                           # Verified archive blob
│   │   ├── archive.sig                              # Cosign signature
│   │   └── last_used.touch                          # mtime = last cache hit; checked against 24h TTL
│   └── <pinned-sha-2>/                              # Older cached versions retained until manually pruned
├── <hash-of-private-source-url>/                    # Operator-configured additional source
│   └── <pinned-sha>/
│       └── ... (same layout)
└── _meta/
    └── sources.json                                 # Source ID ↔ URL lookup; updated on each fetch
```

`<source-id>` = `data_encoding::BASE32_NOPAD.encode(&sha256(url)[..10])` (16-char alphanumeric, no slashes — safe for path segments). Special-cased to `public-milestone-108` for the milestone-108 default URL so cache layout stays operator-recognizable on default installs.

TTL check: at scan startup, for each configured source's most-recent `<pinned-sha>/`, `mtime(last_used.touch)` is compared to `now - 24h`. If the file is older OR doesn't exist, the source is re-fetched (which may produce the same SHA, in which case the existing dir is reused and the touch file is updated). `mikebom fingerprints fetch --force` bypasses the comparison.

**Rationale**: Per-source separation prevents archives from different sources colliding on the same SHA (extremely unlikely but cheap to defend against), preserves milestone-108's per-SHA structure for tools that already know the pattern, and makes "which cache dir corresponds to which source" debuggable from a directory listing alone. The `_meta/sources.json` lookup is a 1KB+/file overhead that makes cache forensics straightforward.

**Alternatives considered**:
- Flat layout (`~/.cache/mikebom/fingerprints/<pinned-sha>/`) with a per-archive `source_url` in a sidecar file — rejected because multiple sources COULD coincidentally publish the same content SHA (e.g., a mirror), and merging their TTL state silently would be surprising.
- TTL via a single `cache.json` index file rather than per-dir `last_used.touch` files — rejected because per-file mtime is more robust to concurrent scans (no read-modify-write race), and the file-existence-as-cache-presence pattern is already established in milestone 108.
- 12-hour or 48-hour TTL — rejected per the 2026-06-03 Q2 clarification (24h matches milestone-108).

---

## R4 — Source configuration mechanism (FR-006)

**Question**: How does an operator declare additional corpus sources to mikebom-cli? Config file? Env vars? Both?

**Decision**: **Both, via a documented precedence order**:

1. **CLI flag** (highest precedence): `--fingerprints-source URL[=ENV_VAR]` (repeatable). E.g., `--fingerprints-source https://corpus.example/private.tar.gz=KUSARI_CORPUS_TOKEN`. The optional `=ENV_VAR` names the environment variable holding the bearer token for this source; absent means unauthenticated. Multiple `--fingerprints-source` invocations are union'd.
2. **Environment variables**: `MIKEBOM_FINGERPRINTS_SOURCES` (comma-separated URLs in the same `URL[=ENV_VAR]` syntax). Adds to CLI-flag-supplied sources rather than replacing.
3. **Config file** (lowest, persistent default): existing `~/.config/mikebom/config.toml` (already used for unrelated mikebom settings per milestone 075's URL-credential-strip work) — new `[fingerprints]` section:
   ```toml
   [fingerprints]
   sources = [
       { url = "https://corpus.example/private.tar.gz", credential_env = "KUSARI_CORPUS_TOKEN" },
       { url = "https://other.example/extra.tar.gz" },                             # no auth
   ]
   ```

Sources are union'd across all three layers (no source-replacement semantics). Per FR-012, the milestone-108 public URL is implicitly added at the lowest precedence unless `--fingerprints-source-no-default` is passed (escape hatch for air-gapped operators who explicitly don't want any default fetch).

**Rationale**: Three-layer precedence (CLI → env → config) is the standard mikebom configuration pattern already used for `--output-dir`, `--log-level`, etc. The `URL[=ENV_VAR]` flag syntax keeps credentials OUT of process argv (only the env-var NAME appears) — credentials themselves never appear in `ps`/`ps aux` output. Comma-separated env-var syntax matches the existing `MIKEBOM_*` patterns.

**Alternatives considered**:
- Single env-var-only configuration — rejected because operators with multiple sources need ad-hoc CLI override capability (e.g., temporary `--fingerprints-source` during incident triage).
- JSON-only config file with no env-var passthrough — rejected because TOML is already mikebom's config-file format and adding a JSON one would split the surface.
- Putting credentials in the config file directly — rejected; secrets in config files end up committed to dotfile repos. Env-var-only is the secrets-hygiene-safe pattern.

---

## R5 — JSON Schema validation strategy

**Question**: FR-001 requires schema-conformant records. Where does validation run — fetch time, scan startup, deserialization time, dev-only?

**Decision**: **Deserialization-time strict shape (production) + fixture-driven JSON Schema validation (dev/CI only)**.

- **Production** path: v2 records are `serde_json::from_slice::<CorpusRecordV2>(...)` with `#[serde(deny_unknown_fields)]` on the struct + on every sub-struct. Unknown fields → deserialization error → record skipped with a warning (matches spec edge case "Single malformed record"). The struct's typed fields enforce required-vs-optional, the `IndicatorKind` enum enforces closed-set indicator types, and `Purl`/`Sha256` newtype constructors validate string-typed payloads.
- **Dev/CI** path: a separate test in `mikebom-cli/tests/fingerprints_v2_schema.rs` validates every fixture record against the published JSON Schema (`contracts/corpus-record-v2.schema.json`) using `jsonschema = "0.46"` (existing dev-dep). This catches schema drift between the JSON Schema definition (a stable public contract for third-party corpus authors) and the in-memory struct.
- **JSON Schema publication**: the schema file is in `mikebom-cli/contracts/` AND copied to `docs/reference/corpus-record-v2.schema.json` at release time so the GitHub Pages site exposes it at `https://kusari-oss.github.io/mikebom/reference/corpus-record-v2.schema.json` (already a published path for unrelated reference material per milestone-082 docs work). This is the "stable URL" required by FR-004.

**Rationale**: Production code paths don't need full JSON Schema validation per scan (it's expensive + redundant once the deserializer enforces shape). The schema's value is documentation + contract-publication for third-party corpus authors, which is exactly what the dev-only test enforces. This matches the existing SPDX 2.3 / SPDX 3 validation pattern (deserialize + emit in production; jsonschema validate in tests).

**Alternatives considered**:
- Run jsonschema in production on every fetch — rejected; adds ~30ms per record × 100 records = 3s of pure validation overhead with no marginal correctness benefit beyond what the deserializer already provides.
- Skip JSON Schema entirely, ship only the Rust struct as the contract — rejected; third-party corpus authors need a language-agnostic contract.

---

## R6 — Sigstore signature verification (v1 → v2 archive transition)

**Question**: Milestone 089 bumped sigstore to 0.11 with `bundle` + `cosign-rustls-tls` + `fulcio-rustls-tls` features. Does the same flow verify multi-source archives without changes?

**Decision**: **Yes, no sigstore changes required**. The milestone-108 archive-signature flow reads:

```text
archive.tar.gz + archive.sig + archive.cert → sigstore::verify_blob()
                                            → returns matched identity (e.g., the GitHub Actions OIDC identity)
                                            → mikebom checks identity matches an allowed-issuer list
```

For v2 multi-source:
- Default public-source allowed-issuer list: only the milestone-108 issuer (the kusari-sandbox/mikebom-fingerprints GitHub Actions OIDC identity), matching the existing milestone-108 trust anchor.
- Per-source allowed-issuer override: declared in the config-file source entry as `allowed_issuers = ["https://github.com/kusari/mikebom-corpus-private/.github/workflows/release.yml@refs/tags/*"]`. Absent → defaults to the same as the milestone-108 anchor (any source not declaring its own anchor falls back to assuming the corpus is signed by the same identity as the milestone-108 corpus, which is intentionally restrictive — operators consuming a 3rd-party source MUST declare its anchor explicitly).

**Rationale**: Sigstore's keyless OIDC model means "trust" is a function of the OIDC identity that signed the artifact. The milestone-108 anchor stays the safe default; per-source overrides let operators consume corpora signed by other identities (e.g., a vendor corpus signed by that vendor's GitHub Actions identity, or an internal corpus signed by a corporate-IdP-issued identity once Sigstore supports more issuers). No code changes to the sigstore call site; only the config layer extends.

**Alternatives considered**:
- Single global allowed-issuer list — rejected because a single global list can't reflect the principle that 3rd-party corpora MAY have different (legitimate) signers from the milestone-108 baseline. Per-source overrides preserve trust boundary integrity.
- Drop signature verification on private sources (assume the operator trusts their auth-gated source) — rejected; signature verification is defense in depth against a compromised auth gateway, and the small operational overhead is acceptable per the constitution's supply-chain transparency principle.

---

## R7 — Auth-fetch hermetic testing (dev-dep choice)

**Question**: Do we add `wiremock = "0.6"` as a dev-dep for HTTP fixture testing, or hand-roll a `tokio::net::TcpListener` stub?

**Decision**: **Hand-rolled stub** using `tokio::net::TcpListener` + `hyper = "1.x"` (already a transitive workspace dep via `reqwest`). The test surface is small (3-4 cases: valid-auth-succeeds, missing-auth-401, bad-auth-401, network-timeout); a 100-line dedicated test helper is more controllable than wiremock's macro-driven config and avoids adding a new dev-dep that has its own dependency tree.

Reference implementation pattern: milestone 055's go-mod-proxy hermetic-fetch stub in `mikebom-cli/tests/fixtures/go_mod_proxy_stub.rs` (~80 lines, listens on a random localhost port, returns canned responses per path).

**Rationale**: Zero new dev-deps to the lockfile. Existing precedent in the repo. Wiremock's value is in complex scenarios (recording cassettes, partial matchers, hot-reload); we don't need those for 4 deterministic test cases.

**Alternatives considered**:
- `wiremock = "0.6"` — would let us write tests faster but adds a small dep tree to the lockfile for marginal value.
- `mockito` — even heavier; rejected for the same reason.

---

## R8 — Self-identity resolution (FR-015)

**Question**: How does the matcher resolve the scanned project's self-identity to decide if a corpus record's PURL collides with it?

**Decision**: Resolve in priority order, FIRST hit wins:

1. **Operator override**: `--scan-as <purl-or-name>` CLI flag (NEW for this milestone; takes a literal PURL OR a bare library name that the matcher case-insensitively compares against record PURLs).
2. **CMake `project(<name> ...)` declaration** at the scan root — reuses milestone-102/103's existing cmake reader output.
3. **Cargo `[package].name`** from `Cargo.toml` at the scan root — existing milestone-064 reader.
4. **npm `package.json::name`** at the scan root — existing milestone-066 reader.
5. **PEP 621 `[project].name`** in `pyproject.toml` at the scan root — existing milestone-068 reader.
6. **Git remote URL** (parsed for `<owner>/<repo>`) — existing milestone-073/074 `auto_detect.rs`.
7. **No resolution** → no self-suppression; all corpus records apply unfiltered.

Matching rule (case-insensitive): a record's `purl` (or any `purl_aliases` entry) matches the resolved self-identity when EITHER:
- the PURL's `name` segment matches the resolved name AND the namespace/owner matches (e.g., resolved `pkg:github/openssl/openssl@*` collides with record `pkg:github/openssl/openssl@3.1.4`), OR
- the resolved bare name (from cmake/cargo/npm/pep621) matches the record PURL's `name` segment case-insensitively (allows the operator override to be "openssl" without forcing them to construct a full PURL).

**Per-indicator opt-in**: the v2 schema's per-indicator `suppress_when_self_identity_matches: bool` defaults to `true` for weak indicators (`source_copyright_header`, `exported_symbols` with `min_match < total`) and `false` for strong indicators (`build_id`, `version_string`). When self-identity matches the record, only opted-in indicators are skipped — strong indicators still emit (the openssl-own-binary case from the design doc §7.1 worked example).

**Rationale**: The ladder is identical to the design doc §7.1 sequence; reuse existing readers (no new file parsers). Per-indicator opt-in preserves the semantics that strong indicators (Build-ID specifically) are useful even on self-scans — operators MAY want to know that their binary has the expected Build-ID for the version they think they shipped.

**Alternatives considered**:
- All-or-nothing self-suppression (every indicator skipped when self-identity matches) — rejected because it loses the useful strong-indicator self-attribution case.
- No self-identity resolution this milestone (defer to follow-on) — rejected because without self-suppression the OpenSSL maintainer scanning openssl's own source tree gets `pkg:github/openssl/openssl` emitted as a "third-party dep of itself," which is the design doc's named-failure case and would be a user-visible bug.

---

## R9 — Indicator extractor reuse + extension audit

**Question**: Which existing extractors does the matcher reuse, and what new extractor work does this milestone add?

**Decision**: All matcher input is from existing extractors — **no new extractor code in this milestone**. The matcher consumes the outputs of these milestones' extractors via their established Rust APIs:

| Indicator kind | Source extractor | Milestone | API surface |
|---|---|---|---|
| `exported_symbols` (ELF `.dynsym`) | `binary/symbol_fingerprint.rs::extract_elf_symbols` | 099 | `Vec<String>` of mangled symbol names |
| `exported_symbols` (Mach-O `LC_SYMTAB`) | `binary/symbol_fingerprint.rs::extract_macho_symbols` | 099 + 305 | Same `Vec<String>` |
| `exported_symbols` (PE `IMAGE_EXPORT_DIRECTORY`) | `binary/pe.rs::extract_pe_export_names` | 309 | Same `Vec<String>` |
| `version_string` | `binary/version_strings.rs::extract_version_literals` | 026 | `Vec<String>` of `.rodata` literals matching curated patterns |
| `build_id` (ELF `.note.gnu.build-id`) | `binary/elf.rs::extract_build_id` | 023 | `Option<String>` hex-encoded |
| `macho_uuid` (Mach-O `LC_UUID`) | `binary/macho.rs::extract_macho_uuid` | 024 | `Option<String>` hex-encoded |
| `pe_pdb` (PE `IMAGE_DEBUG_DIRECTORY` CodeView) | `binary/pe.rs::extract_pe_pdb_id` | 028 | `Option<String>` GUID:age |
| `abi_marker` (versioned ELF symbols) | NEW — but extracted from same `.dynsym` reader as `exported_symbols`; FILTERED at matcher time on symbols matching `^[A-Z]+_[0-9_]+$` (the OpenSSL/glibc versioned-symbol convention). | (milestone 099 surface; no new code) | Filtered subset of the `exported_symbols` Vec |

The new `binary/fingerprints/matcher.rs` is the orchestrator: it reads from each per-binary extractor result (already cached in the `BinaryArtifact` struct populated upstream of the matcher), correlates against each loaded v2 record's indicator list, applies the fusion rule (R2), and emits typed match-result structs.

**Rationale**: Constitution principle VI (three-crate architecture) + the spec's § Assumptions ("indicator extractors are unchanged by this milestone") together force this. Existing extractor APIs are already in use by milestone-108; the matcher is the only new consumer.

**Alternatives considered**:
- Introduce an `IndicatorExtractor` trait with one implementor per indicator kind — rejected as premature abstraction; the matcher reads from a fixed-shape `BinaryArtifact` struct, no plugin surface needed for v2.

---

## R10 — CLI override flag set (final UX)

**Question**: What's the full set of new CLI flags this milestone adds to `mikebom sbom scan` and `mikebom fingerprints`?

**Decision**:

**`mikebom sbom scan` new flags**:
- `--fingerprints-source URL[=ENV_VAR]` — repeatable; declares an additional corpus source (R4).
- `--fingerprints-source-no-default` — boolean; suppresses the implicit milestone-108 default source (escape hatch for air-gapped + paranoid-trust-anchor scenarios).
- `--scan-as <purl-or-name>` — string; operator override for self-identity resolution (R8).

**`mikebom sbom scan` flag retained from milestone 108 (unchanged)**:
- `--fingerprints-corpus` (alias `MIKEBOM_FINGERPRINTS_CORPUS=1`) — opt-in for any corpus loading. Without this flag, NO corpus is fetched or consumed regardless of `--fingerprints-source` declarations.

**`mikebom sbom scan` flag from milestone 108 (deprecated, kept for backward compat)**:
- `--fingerprints-rev <SHA>` — pins the milestone-108 default-source SHA. Continues to work; semantics unchanged for the milestone-108 default source. For new sources declared via `--fingerprints-source`, the SHA pin is encoded directly in the URL or via the source's release-tag convention (not this flag).

**`mikebom fingerprints fetch` new flags**:
- `--source URL[=ENV_VAR]` (repeatable) — fetch a specific source (default: all configured + the milestone-108 default).
- `--force` — bypass the 24-hour TTL (per FR-012a clarification Q2).

**Rationale**: The flag set keeps the milestone-108 surface working unchanged (no breaking changes for existing CI configurations) while extending in additive ways for multi-source support. `--scan-as` is the operator-override path for the self-identity ladder; without it, the cmake/cargo/npm/pep621/git-remote auto-detection covers the common case.

**Alternatives considered**:
- A `--fingerprints-corpus-source <auto|public|private>` enum flag (from the original spec draft, removed during the OSS-reframe) — rejected because multi-source semantics make "private" meaningless (which private source?); the per-source URL flag is more expressive and aligns with the OSS-friendly framing.
- Replacing `MIKEBOM_FINGERPRINTS_REV` env-var entirely — rejected for backward compat with milestone-108 CI configurations.

---

## Cross-cutting NOTES (no decision required)

- **Observability** (Outstanding category from /speckit.clarify): the matcher's `tracing` events are emitted at structured levels: `tracing::info` for "source X fetched, N records loaded"; `tracing::warn` for fetch failures, signature failures, malformed records; `tracing::debug` for per-binary per-record match attempts. No metric counters added in this milestone — promoted to a follow-on if operators ask for them.
- **Auth header convention** (Deferred from /speckit.clarify): the spec says "bearer token in HTTP Authorization header is the default; other schemes a plan-phase decision if the user case demands". This research is settled: bearer-token-in-Authorization-header is the only scheme implemented this milestone. Other schemes (mTLS, signed URLs, custom headers) re-open if a real customer use case appears.

---

## Outputs

- All 10 unknowns resolved.
- 3 new format-mapping rows queued for `docs/reference/sbom-format-mapping.md` (C59, C60, C61) + extensions to C56, C58, C16.
- Zero new production crates; zero new dev-deps (R7 hand-rolled stub).
- Constitution principle V audit complete (R1); no violation.
- Phase 1 (data-model.md, contracts/, quickstart.md) proceeds with no remaining NEEDS CLARIFICATION.
