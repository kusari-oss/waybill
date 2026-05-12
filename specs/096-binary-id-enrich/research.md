# Research — milestone 096 identify-unknown-binaries enrichment

Phase 0 investigation. Five decision points; all resolved with concrete locked-in v1 starter sets.

## §1 — v1 embedded-version-string pattern set (FR-001 / FR-002)

**Decision**: 5 starter patterns covering the dominant statically-embedded C/C++ libraries. Each pattern is anchored on a high-uniqueness substring (copyright lines, banner strings, header-baked literals) to minimize false-positive risk per SC-007.

| Library | Anchor substring | Version-capture regex | Source of truth |
|---------|------------------|----------------------|----------------|
| **OpenSSL** | `OpenSSL ` (capital-O, capital-S-S-L, trailing space) | `OpenSSL (\d+\.\d+\.\d+[a-z]?)\s+\d{1,2}\s+[A-Z][a-z]{2}\s+\d{4}` | `include/openssl/opensslv.h` `OPENSSL_VERSION_TEXT` — e.g., `OpenSSL 3.0.13 30 Jan 2024` |
| **zlib** | `deflate ` + ` Copyright ` (the deflate-banner pattern) | `deflate (\d+\.\d+\.\d+) Copyright \d{4}-\d{4} Jean-loup Gailly` | `deflate.c` `deflate_copyright[]` — e.g., `deflate 1.2.13 Copyright 1995-2022 Jean-loup Gailly and Mark Adler` |
| **libcurl** | `libcurl/` (lowercase, slash) | `libcurl/(\d+\.\d+\.\d+)(?:-[A-Za-z0-9]+)?` | `include/curl/curlver.h` `LIBCURL_VERSION` — e.g., `libcurl/8.4.0 OpenSSL/3.0.13 zlib/1.2.13` |
| **SQLite** | `SQLite format 3` (database-header literal) + version near `sqlite3_libversion` symbol-table entry | `(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} [0-9a-f]{64})` (source-id format; co-locates with the version triplet `(\d+\.\d+\.\d+)`) | `sqlite3.h` `SQLITE_VERSION` + `SQLITE_SOURCE_ID` |
| **libxml2** | `libxml2-` or `libxml2/` (depending on packaging convention) | `libxml2[/-](\d+\.\d+\.\d+)` | `include/libxml/xmlversion.h` `LIBXML_DOTTED_VERSION` |

**Rationale**:
- Each anchor is sufficiently distinctive to avoid coincidental string matches (e.g., the deflate copyright pattern includes the author name — improbable as coincidental data).
- Versions are captured as `<major>.<minor>.<patch>` triplets per the packageurl-spec for generic packages; appended `[a-z]?` for OpenSSL's letter-suffix releases (1.0.2t etc.).
- SQLite's anchor is special — the `SQLite format 3` string IDs the database file header, not the library itself. We additionally need to co-locate the version triplet near the `sqlite3_libversion` symbol-table entry to confirm we're identifying the library, not a database file. Acceptable per FR-002's "configurable in code" pattern: SQLite's matcher has a 2-step check.

**Future extensions** (NOT v1): libpng, freetype, boost, libssh2, libssl (1.x-only), Lua, nghttp2, zstd. Plan-level decision to revisit after v1 ships.

## §2 — UPX packer-detection signatures (FR-003)

**Decision**: detect UPX-packed binaries via TWO complementary signals (one MUST be present; both being present strengthens confidence). v1 detects UPX only; mpress / ASPack / PECompact are deferred to Out-of-Scope per the spec.

**Signal A — section-name heuristic (platform-aware)**:
- **ELF**: section table contains both `UPX0` AND `UPX1` (plus sometimes `UPX2`). Required for ELF positive identification.
- **PE**: section table contains both `UPX0` AND `UPX1` (plus sometimes `UPX2`). Required for PE positive identification.
- **Mach-O**: section name heuristic does NOT apply — UPX renames Mach-O segments differently. Mach-O detection relies on Signal B exclusively.

**Signal B — universal magic-bytes scan**:
- ASCII string `UPX!` (the UPX header's `l_magic` field, `0x21585055` little-endian) appears at the start of the compressed payload in **every UPX-packed binary regardless of format**. Linear-scan the file for this 4-byte literal.
- Optionally also check for the banner `This file is packed with the UPX executable packer` (~50 bytes; appears as plaintext in nearly all UPX outputs; not always present in heavily-customized UPX forks but reliable enough for v1).

**Acceptance**: emit `mikebom:binary-packer = upx` when EITHER Signal A (platform-applicable) OR Signal B fires. Cross-format Mach-O packed binaries are detected via Signal B alone.

**Rationale**:
- Section-name heuristic is fast (O(section-count) ≈ tens of comparisons) and unambiguous for ELF/PE.
- Magic-bytes scan is O(file-size) but bounded — typical binaries ≤10 MB; `memchr`-style search is ~1 GB/s on modern hardware.
- Combined, false-positive rate is near-zero: a coincidental `UPX!` substring in a non-packed binary AND coincidental `UPX0`/`UPX1` section names would be statistically vanishing.

**UPX version extraction** (stretch, FR-003 optional): the `$Id: UPX <version> Copyright (C) ...` banner string (when present) provides the packer version. If extracted, emit as `mikebom:binary-packer-version`. Defer to planning whether to implement in v1.

## §3 — Symbol-fingerprint set + match threshold (FR-004)

**Decision**: 3 starter libraries, 10 high-distinctiveness symbols each, 80% match threshold (8 of 10 must be present). ELF only in v1 (per spec FR-004 + Out-of-Scope clause).

| Library | Symbol set (10 each) | Match threshold |
|---------|----------------------|------------------|
| **OpenSSL** | `OPENSSL_init_ssl`, `OPENSSL_init_crypto`, `SSL_CTX_new`, `SSL_library_init`, `EVP_DigestInit_ex`, `EVP_EncryptInit_ex`, `RSA_new`, `BN_new`, `X509_new`, `ERR_get_error` | ≥8 of 10 |
| **zlib** | `deflate`, `inflate`, `deflateInit_`, `inflateInit_`, `deflateEnd`, `inflateEnd`, `crc32`, `adler32`, `compress`, `uncompress` | ≥8 of 10 |
| **libcurl** | `curl_easy_init`, `curl_easy_setopt`, `curl_easy_perform`, `curl_easy_cleanup`, `curl_easy_getinfo`, `curl_multi_init`, `curl_multi_add_handle`, `curl_global_init`, `curl_version`, `curl_slist_append` | ≥8 of 10 |

**Rationale**:
- Each symbol is a public-API function (not internal). Stable across 10+ years of releases.
- 80% threshold = 8 of 10 — balances coverage (must catch real static-link cases) against false-positive risk (a binary that happens to define a function called `inflate` for unrelated reasons won't accidentally match 8 of zlib's 10 symbols).
- ELF only: `.dynsym` parsing is well-defined; the existing `object` crate's ELF code path already iterates `.dynsym`. PE export-table parsing and Mach-O `LC_DYSYMTAB` parsing are more involved + deferred to a future milestone per spec Out-of-Scope.

**Alternatives considered**:
- **Smaller set, 100% threshold** (5 symbols, all required): missed false-negatives where the static library was selectively linked. Rejected.
- **Larger set, lower threshold** (20 symbols, 50% required): higher false-positive risk + more pattern-table maintenance. Rejected.
- **Per-library tuned thresholds** (zlib needs 9/10 because it has fewer total exports; libcurl can tolerate 6/10): adds complexity for marginal gain. Rejected for v1; revisit if false-positive data demands.

## §4 — Performance budget per binary

**Decision**: linear scan of `.rodata` / equivalent sections, bounded by file size. No explicit time budget; rely on the existing `tracing::info!` per-file timing for visibility.

**Reasoning**:
- Typical binaries are ≤10 MB; `.rodata` section is a fraction of that.
- `memchr`-style search for anchor substrings runs at ~1 GB/s on modern hardware — a 10 MB section's full scan completes in ~10 ms.
- 5 pattern anchors × 10 ms = ~50 ms per binary in the worst case.
- The new symbol-fingerprint pass walks `.dynsym` (typically <1 MB of symbol-table data); set-membership checks against 30 fingerprint symbols = negligible.

**No explicit cap**: if a binary is unusually large (e.g., a 500 MB monolithic statically-linked artifact), the scan takes proportionally longer; we accept this. Future milestone can add a `--max-binary-scan-bytes` flag if encountered.

**Telemetry**: the existing `tracing::info!(path = %file_path, "scan starting")` (also used by milestone-094's structural test) gives operator visibility into per-binary scan duration via timestamps. No new instrumentation needed.

## §5 — Parity-catalog C12 row for `mikebom:binary-packer` (Constitution V audit)

**Decision**: add row C12 to `mikebom-cli/src/parity/extractors/mod.rs:140-141` (after C10 / C11 for `binary-class` / `binary-stripped`). Same shape as the existing C10/C11 extractors — `cdx_anno!`, `spdx23_anno!`, `spdx3_anno!` macros (`component`-scoped), `Directionality::SymmetricEqual`, `order_sensitive: false`. Also update `docs/reference/sbom-format-mapping.md` to add the C12 row with the Constitution-V audit justification.

**Audit result (per Constitution V's standards-native-precedence rule)**:
- **CDX**: no native `packer-status` field anywhere in the CDX 1.6 schema. The closest hooks are `component.properties[]` (the existing carrier for `mikebom:binary-class` + `mikebom:binary-stripped`). Decision: use `component.properties[]` with name `mikebom:binary-packer`.
- **SPDX 2.3**: no native packer field. The existing milestone-049/052/084 convention uses `Package.annotations[]` with `comment = "mikebom:<key>=<value>"` for properties that don't map to SPDX-native fields. Decision: same convention for `mikebom:binary-packer`.
- **SPDX 3**: no native packer field. The existing convention uses `Annotation` elements with `subject` referencing the package and `statement = "mikebom:<key>=<value>"`. Decision: same.

**Standards-native equivalents checked + rejected**: no `pe-resource-version`, no `binary-format-extension`, no equivalent in any of the 3 formats. The `mikebom:*` carrier is justified across all 3 by the same audit conclusion that justified `mikebom:binary-class` (C10) and `mikebom:binary-stripped` (C11).

**Documentation**: `docs/reference/sbom-format-mapping.md` gains a C12 row mirroring C10/C11's per-format mapping table.

## §6 — `evidence.identity[].technique` value choice for new techniques

**Decision**: use CDX's standards-native `technique = binary-analysis` for both new identification methods (embedded-version-string + symbol-fingerprint). Add a companion `mikebom:identification-method` property capturing the precise sub-technique value (`embedded-version-string` or `symbol-fingerprint`).

**Rationale**:
- CDX 1.6's `evidence.identity[].methods[].technique` is a string enum: `manifest-analysis | binary-analysis | source-code-analysis | filename | ast-fingerprint | hash-comparison | instrumentation | attestation | other`. Both new techniques are sub-types of `binary-analysis`; using `binary-analysis` keeps the spec-native field compliant with the enum.
- The companion property `mikebom:identification-method` provides the precise sub-technique — same parity-bridging pattern as milestone-091's `mikebom:resolver-step` carrier for Go-resolver step provenance.
- Existing milestone-004 binary-scanner components already use `technique = binary-analysis` (verified via grep at code level); the new techniques extend the same path.

**Confidence values**: 0.6 for embedded-version-string, 0.4 for symbol-fingerprint. Inherits the milestone-004 tiered-confidence convention (manifest-analysis = 0.85, binary-analysis tier-1 = 0.85, version-string evidence = 0.6, symbol-only fingerprint = 0.4). No new tier thresholds.

## Coverage map

| Spec section | Resolution |
|--------------|------------|
| FR-001 (5+ version-string patterns) | §1 → 5 patterns locked: OpenSSL, zlib, libcurl, sqlite, libxml2 |
| FR-002 (configurable-in-code pattern table) | §1 → in-source `const PATTERN_SET: &[(library, anchor, regex)]` |
| FR-003 (UPX detection minimum) | §2 → Signal A (section names) + Signal B (magic bytes) — both implemented in v1; Mach-O via Signal B alone |
| FR-004 (3+ symbol-fingerprint libraries, ≥80% threshold) | §3 → OpenSSL/zlib/libcurl, 10 symbols each, 8/10 threshold |
| FR-005 (composite-evidence dedup per Q1) | data-model.md will spec the merged-`evidence.identity[]` shape |
| FR-006 (packed binaries still attempt extraction) | §2 — UPX stub's strings + symbols are also scanned; transparency property signals payload opacity |
| FR-007 (no new Cargo deps) | §1-§6 → existing `object` crate covers parsing; `memchr` in `std` covers magic-bytes search |
| FR-008 (production code in `binary/` + parity-catalog only) | §5 → C12 row addition is the one cross-module touch |
| FR-009 (no golden regen on 9 ecosystems) | §1+§3 — anchor patterns are distinctive enough to fire only on intentional fixtures; SC-007's ≤1-component bound is the gate |
| FR-010 (PURL conformance) | §1 → `pkg:generic/<lowercase-name>@<version-triplet>` matches packageurl-spec |
| FR-011 (strict PURL-equality global dedup per Q3) | data-model.md will spec the dedup loop |
| Constitution V audit | §5 → C12 row added; no native equivalent across CDX/SPDX2.3/SPDX3 |
| Constitution X transparency | §2 → always-emitted `mikebom:binary-packer` property per Q2; confidence values per §6 |

All open spec questions resolved. Ready for Phase 1 (data-model + contracts + quickstart).
