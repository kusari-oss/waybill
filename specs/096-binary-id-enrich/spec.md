# Feature Specification: Identify-unknown-binaries enrichment — embedded version strings + packer detection + symbol fingerprinting

**Feature Branch**: `096-binary-id-enrich`
**Created**: 2026-05-12
**Status**: Draft
**Input**: User description: "deal with random binaries that I don't know where they came from" — the milestone-095 source-side Conan reader is deferred; this milestone focuses on extracting more identification signal from compiled binaries when no source-side context is available.

## Background

mikebom's binary scanner today (milestone 004 + 038) extracts: dynamic linkage (ELF `DT_NEEDED`, Mach-O `LC_LOAD_DYLIB`, PE Import Directory), `.note.package` (ELF), GNU build-id (ELF), debuglink (ELF), RPATH/RUNPATH, binary class + stripped state, cargo-auditable embedded Rust deps, Go BuildInfo, and SHA-256 deep-hash.

When an operator scans a binary whose provenance is unknown — a stripped ELF blob in a customer environment, a third-party Mach-O artifact, a packed PE from a vendor's installer — the existing extraction gives a useful baseline (dynamic-link list + hashes) but leaves three big gaps:

1. **What's statically linked into this binary?** Most "vendored" libraries (OpenSSL, zlib, libcurl, sqlite, libxml2, boost, libpng, freetype, libssl, etc.) bake a version-string constant into the binary's `.rodata` / `__DATA,__cstring` / PE `.rdata` section. A stripped binary that statically links OpenSSL 3.0.13 has `"OpenSSL 3.0.13 30 Jan 2024"` somewhere in its read-only data segment. Without this signal, statically-linked CVEs are invisible to downstream scanners.
2. **Has this binary been intentionally obfuscated?** UPX-packed, mpress-packed, or otherwise compressed binaries defeat static identification (`DT_NEEDED` reads succeed but tell you nothing useful about the *contents*). Operators need a transparency flag: "this binary appears packed; identification confidence is reduced; consider running it through an unpacker before deeper analysis".
3. **What does the binary export/import?** Symbol tables — ELF `.dynsym`, Mach-O `LC_DYSYMTAB`, PE Export Directory — list the functions a binary publishes (export) and consumes (import). Specific symbol names are strong static-link signatures: a binary exporting `OPENSSL_init_ssl` almost certainly embeds OpenSSL even if no `libssl.so` is in `DT_NEEDED`.

This milestone adds these three signal channels to the existing binary scanner. All three are observable in the binary file itself (no external lookups, no DB downloads), match the existing `pkg:generic/...` + `evidence.identity[]` shape mikebom already uses for linkage components, and require no new Cargo dependencies (ELF/Mach-O/PE parsing already happens via the `object` crate).

The user's specific framing: "random binaries that I don't know where they came from". This milestone's deliverables answer that question with three new evidence channels and the existing infrastructure (PURL emission, evidence tracking, normalization-friendly output).

Out of scope: external fingerprint database lookups (deps.dev, ClearlyDefined), CPE candidate emission, DWARF debug-info extraction, source-side C/C++ readers (Conan / vcpkg / CMake — deferred to a separate milestone track). These are higher-value but higher-cost; "start simple" means stick to inferences the scanner can make from the binary alone.

## Clarifications

### Session 2026-05-12

- Q: Dedup strategy when both embedded-version-string AND symbol-fingerprint techniques match the same library on the same binary → A: B — emit a single component with BOTH `evidence.identity[]` entries (composite-evidence pattern). CDX's `evidence.identity[]` is designed to hold multiple corroborating techniques; preserves both signals transparently per Constitution X.
- Q: Packer-detection property emission convention (always-emit vs emit-only-when-packed) → A: A — always emit `mikebom:binary-packer` on file-level binary components, with value `none` for unpacked binaries and `<packer-name>` (e.g., `upx`) when a packer is detected. Matches the existing `mikebom:binary-stripped` always-emitted convention; downstream filters can use value-equality without presence checks.
- Q: Cross-binary dedup semantics for the new-technique outputs (version-resolved vs symbol-fingerprint-only — should they merge?) → A: A — strict PURL-equality global dedup. Each distinct PURL string is its own component; `pkg:generic/openssl@3.0.13`, `pkg:generic/openssl@3.0.12`, and `pkg:generic/openssl` (no version) are three separate components, each with its own merged `evidence.occurrences[]`. Matches existing `linkage.rs` behavior; preserves version-specificity for CVE-matching downstream consumers.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator scans a stripped binary that statically links OpenSSL and sees an OpenSSL component (Priority: P1)

An operator has a stripped C/C++ binary from an unknown vendor. The binary statically links OpenSSL 3.0.13 (no `libssl.so` in `DT_NEEDED` / `LC_LOAD_DYLIB` because OpenSSL was linked at compile time as `.a` archives). When mikebom scans the binary, it extracts the embedded `"OpenSSL 3.0.13"` version string from the binary's read-only data segment and emits a component identifying the embedded OpenSSL.

**Why this priority**: this is the user's stated core ask — "what's inside this random binary I don't know about". Statically-linked CVE matching is the dominant unknown-binary pain point.

**Independent Test**: build a small C test program that statically links a recent OpenSSL (or use a known fixture from `mikebom-test-fixtures`), strip it, scan it with mikebom. Expect a `pkg:generic/openssl@3.0.13` (or similar) component with `evidence.identity[].technique = binary-analysis` (the CDX-native enum value), `confidence = 0.6`, and a companion `mikebom:identification-method = embedded-version-string` property capturing the precise sub-technique. Confidence 0.6 is lower than `manifest-analysis`'s 0.85 — heuristic-based binary inference, not authoritative manifest.

**Acceptance Scenarios**:

1. **Given** a stripped ELF binary that statically links OpenSSL 3.0.13, **When** mikebom scans the file, **Then** the emitted SBOM contains a component with `name = openssl`, `version = 3.0.13`, `purl = pkg:generic/openssl@3.0.13`, `evidence.identity[].technique = binary-analysis` (CDX-native enum value) with companion property `mikebom:identification-method = embedded-version-string` capturing the sub-technique, `confidence = 0.6`, and an `evidence.occurrences[]` entry pointing at the binary file path.
2. **Given** the same binary AND its dynamic-linkage emission of `libssl.so.3` (in a case where some of OpenSSL is dynamic + some static), **When** mikebom scans it, **Then** the SBOM contains BOTH the dynamic-linkage `pkg:generic/libssl.so.3` component AND the embedded-version-string `pkg:generic/openssl@3.0.13` component, deduped at the PURL level if and only if they collide. Operators can distinguish "we link OpenSSL dynamically" from "we also have a static OpenSSL embedded".
3. **Given** a binary that contains a known version-string pattern for one of: OpenSSL, zlib, libcurl, sqlite, libxml2, libpng, freetype, boost (the v1 supported pattern set), **When** mikebom scans it, **Then** at least the matching library is identified with version. Other version-string patterns that don't match the v1 set are skipped silently (no false-positive emission).

---

### User Story 2 — Operator scans a packed binary and sees a transparency flag (Priority: P2)

An operator has a binary that's been packed with UPX (or similar). mikebom detects the packer signature and emits a transparency property naming the packer detected. Downstream consumers can filter or escalate based on this property: e.g., "any binary with `mikebom:binary-packer != none` requires manual unpacking before vulnerability scanning".

**Why this priority**: P2 because packed binaries are less common than statically-linked ones, but when they exist the existing scanner silently produces a thin SBOM (no `DT_NEEDED` because the unpacker layer doesn't expose dep info until runtime). The transparency flag is what Principle X requires.

**Independent Test**: take any binary, run it through `upx --best`, scan the packed result. Expect a `mikebom:binary-packer = upx` property on the file-level binary component.

**Acceptance Scenarios**:

1. **Given** a UPX-packed ELF binary, **When** mikebom scans it, **Then** the file-level binary component carries property `mikebom:binary-packer = upx`. Optionally — and only if cheaply extractable — also include the UPX version (`mikebom:binary-packer-version = 4.2.4`).
2. **Given** a binary that's NOT packed, **When** mikebom scans it, **Then** the file-level binary component carries property `mikebom:binary-packer = none` (always emitted; matches the `mikebom:binary-stripped` always-emitted convention per Clarification Q2).
3. **Given** a packed binary AND mikebom emits other linkage components by reading the packed binary's shell layer (e.g., UPX's own stub statically links libc), **When** the operator reads the SBOM, **Then** the packed-binary transparency property signals "the linkage extraction here is the unpacker stub, not the wrapped content — recommend unpacking before deeper analysis".

---

### User Story 3 — Operator scans a binary that exports a known library's symbols and sees a fingerprint match (Priority: P2)

An operator has a binary that does NOT have an OpenSSL version string in `.rodata` (or has been obfuscated to strip it) but DOES export the OpenSSL public API surface (e.g., `OPENSSL_init_ssl`, `SSL_CTX_new`, `EVP_DigestInit_ex2`). mikebom matches the exported symbol set against known-library fingerprints and emits a low-confidence component.

**Why this priority**: P2 because the symbol-fingerprint approach catches binaries where the version-string approach misses (deliberately stripped, custom-built without version constants, etc.) — but the confidence is low (symbol names alone don't tell you the version).

**Independent Test**: a binary that statically links OpenSSL with version string stripped but symbols exported — confirm mikebom emits an `openssl` component with no version, `evidence.identity[].technique = binary-analysis` (CDX-native) with companion property `mikebom:identification-method = symbol-fingerprint`, `confidence = 0.4`.

**Acceptance Scenarios**:

1. **Given** a binary that exports `OPENSSL_init_ssl` + `SSL_CTX_new` + `EVP_*` (the OpenSSL public surface, ≥5 well-known symbols), **When** mikebom scans it, **Then** the SBOM contains `pkg:generic/openssl` (no version, since symbol set alone doesn't pin a version) with `evidence.identity[].technique = binary-analysis` (CDX-native) + companion property `mikebom:identification-method = symbol-fingerprint`, `confidence = 0.4`, and a property listing the matched symbol count.
2. **Given** a binary that statically links a library AND has its version string intact, **When** mikebom scans it, **Then** the SBOM emits BOTH a version-string component (higher confidence) AND a symbol-fingerprint component if both signals trigger. The dedup rule: if the version-string component's PURL covers the same library, the symbol-fingerprint component is suppressed (don't double-count); otherwise emit both with their separate evidence trails.
3. **Given** v1's symbol-fingerprint set covers a small starter list (OpenSSL, zlib, libcurl — the same 3 the version-string set focuses on), **When** the operator scans a binary embedding `libxml2` symbols, **Then** no fingerprint match fires for libxml2 (it's not in the v1 set) and the operator sees no emission. v1 silently skips out-of-set patterns; future milestones extend coverage.

---

### Edge Cases

- **False-positive version-string match**: a binary contains the literal string `"OpenSSL 3.0.13"` not because it statically links OpenSSL but because the string appears in a configuration file or test fixture embedded in the binary. The `confidence = 0.6`, `evidence.identity[].technique = binary-analysis`, and `mikebom:identification-method = embedded-version-string` give downstream consumers the right signal to handle false-positives; we deliberately emit at low confidence rather than suppress on ambiguity.
- **Multiple library versions in a single binary**: a binary statically links OpenSSL 1.0.2 (legacy compatibility shim) AND OpenSSL 3.0.13 (current). Two version-string matches; emit both components. The operator sees both. Downstream consumers do their own dedup if needed.
- **Packed binary with embedded version strings INSIDE the packed payload**: the version strings won't be extractable because the binary's body is compressed/encoded. UPX's stub IS present and we'll detect that; the wrapped content is opaque. The `mikebom:binary-packer = upx` transparency property signals this.
- **Statically-linked tiny libraries (musl-libc, micro-libc)**: these typically don't embed version strings. Symbol fingerprinting helps; absent that, the binary is an "I don't know" — that's an honest answer, not a defect.
- **Binary that contains BOTH a `.note.package` AND embedded version strings for libraries inside it**: the `.note.package` describes the file ITSELF (provenance of the binary qua-artifact); embedded version strings describe COMPONENTS WITHIN the binary. Both are legitimate, complementary. The SBOM emits both — file-level provenance + per-embedded-component identification.
- **Pattern collisions**: `"sqlite 3.45.1"` could be SQLite (the database engine) or a coincidental name. v1's pattern set will be deliberately narrow (≤10 well-known C/C++ libraries) to minimize ambiguity. Future milestones can extend with confidence-tiered patterns.
- **Symbol-fingerprinting on heavily-stripped binaries**: a fully-stripped binary (both symbols and `.dynsym` removed) gives no symbol-table signal. The technique only works on binaries that retain at least the dynamic symbol table (true for most production binaries since `.dynsym` is required for dynamic linking).
- **Cross-platform sanity**: ELF, Mach-O, and PE all need parser support. v1 covers ELF for embedded-version-strings + symbol-fingerprinting. Mach-O + PE for these techniques are stretch goals; the packer-detection check covers all three platforms via magic-number scanning.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For each scanned ELF/Mach-O/PE binary, mikebom MUST scan its read-only data sections (ELF: `.rodata`; Mach-O: `__DATA,__cstring` + `__TEXT,__cstring`; PE: `.rdata`) for known version-string patterns. The v1 supported pattern set MUST cover at least 5 well-known C/C++ libraries (proposed: OpenSSL, zlib, libcurl, sqlite, libxml2). Each match emits a component with `name = <library>`, `version = <extracted-version>`, `purl = pkg:generic/<name>@<version>`, `type = library`, `evidence.identity[].technique = binary-analysis` (the CDX-native enum value), `evidence.identity[].confidence = 0.6`, companion property `mikebom:identification-method = embedded-version-string` capturing the precise sub-technique, and `evidence.occurrences[]` pointing at the binary file.
- **FR-002**: The version-string pattern set MUST be configurable in code (not hardcoded inline) so future milestones can extend it without rewriting the extraction logic. A simple in-source `const` table of `(library_name, regex_or_substring_anchor, version_capture)` tuples is acceptable.
- **FR-003**: For each scanned binary, mikebom MUST attempt packer detection. v1 MUST detect at least UPX (the dominant open-source packer; ≥80% of "packed binaries in the wild" are UPX). Stretch goals: mpress, ASPack, PECompact. Detection method: signature scan for the packer's stub-section magic bytes at well-known offsets. mikebom MUST emit `mikebom:binary-packer` on every file-level binary component regardless of pack state — value `none` when no packer is detected, and the lowercase packer name (e.g., `upx`) when one is. The always-emitted convention matches `mikebom:binary-stripped` and lets downstream filters use value-equality (e.g., `properties[?(@.name=='mikebom:binary-packer')].value != 'none'`) without a presence-check.
- **FR-004**: For each scanned ELF binary that has a dynamic symbol table (`.dynsym`), mikebom MUST attempt symbol-fingerprinting against a v1 set of at least 3 well-known C/C++ libraries (proposed: OpenSSL, zlib, libcurl — overlapping with the version-string set so they can corroborate). Each library's fingerprint is a small set (5–10) of high-distinctiveness export symbols; a match requires ≥80% of the fingerprint symbols to be present (4 of 5, 8 of 10). Matches emit a component with `evidence.identity[].technique = binary-analysis` (the CDX-native enum value), `evidence.identity[].confidence = 0.4` (lower than embedded-version-string because the version can't be pinned), and companion property `mikebom:identification-method = symbol-fingerprint` capturing the precise sub-technique.
- **FR-005**: When BOTH embedded-version-string AND symbol-fingerprint techniques match the same library on the same binary, mikebom MUST emit a single component carrying BOTH techniques as separate entries in the `evidence.identity[]` array (composite-evidence pattern). The version-string entry carries `confidence = 0.6`; the symbol-fingerprint entry carries `confidence = 0.4`. Both entries cite the same binary file in `evidence.occurrences[]`. CDX `evidence.identity[]` is designed for this multi-technique corroboration; downstream tools can render either or both per their UX. No double-counting at the component-count level; no silent signal loss at the evidence-trail level.
- **FR-006**: Detected packed binaries MUST still attempt embedded-version-string + symbol-fingerprint extraction on the packer's stub. The transparency property `mikebom:binary-packer` signals to downstream consumers that the extracted content reflects the stub, not the wrapped payload.
- **FR-007**: No new Cargo dependencies. The existing `object` crate (in workspace) handles ELF/Mach-O/PE parsing; substring/regex matching for version strings can use `std` (`memmem` or hand-rolled byte-search). If a regex crate is unavoidable for the pattern table, propose at planning time.
- **FR-008**: No production code changes outside `mikebom-cli/src/scan_fs/binary/`. The 3 new signal channels live within the existing binary-scanner module. Cross-module integration (PURL emission, evidence shape, component dedup) reuses existing infrastructure.
- **FR-009**: Zero byte-identity golden regen for the existing 9 ecosystem goldens IF the scanned fixtures contain no binaries triggering the new signals. If existing goldens regenerate (e.g., the polyglot fixture contains a binary that newly matches), the diff MUST be limited to the new properties / components and MUST NOT change any existing PURL or relationship.
- **FR-010**: PURL emission for new components MUST conform to the packageurl-spec. Specifically: `pkg:generic/<library-name>@<version>` for version-string matches; `pkg:generic/<library-name>` (no version) for symbol-fingerprint-only matches. Lowercase library name, version segment per the spec.
- **FR-011**: Every emitted component MUST carry `evidence.occurrences[]` with the binary file's source path, matching the existing pattern from `linkage.rs`'s cross-binary dedup. Cross-binary dedup uses **strict PURL-equality** per Clarification Q3: each distinct PURL string is its own component, with all binaries that produced that exact PURL merged into a single `evidence.occurrences[]`. Implication: `pkg:generic/openssl@3.0.13`, `pkg:generic/openssl@3.0.12`, and `pkg:generic/openssl` (no version) are three separate components even though they reference the same upstream library. This preserves version-specificity for downstream CVE-matching tools and keeps dedup behavior deterministic + byte-stable for goldens.

### Key Entities

- **Embedded-version-string pattern**: a `(library_name, anchor_pattern, version_capture)` tuple defining how to recognize a library's version constant in binary data. Example: `("openssl", r"OpenSSL (\d+\.\d+\.\d+)", group 1)`. v1 starter set: ~5 entries.
- **Packer signature**: a `(packer_name, magic_bytes, optional_offset)` tuple defining how to recognize a packer's stub. v1 starter set: UPX + 2 stretch.
- **Symbol fingerprint**: a `(library_name, required_symbols[], min_match_count)` tuple. v1 starter set: ~3 entries, 5–10 symbols each.
- **Binary-scanner component**: an emitted `PackageDbEntry` with `evidence.identity[].technique = binary-analysis` (CDX-native enum value) plus companion property `mikebom:identification-method ∈ {embedded-version-string, symbol-fingerprint}` capturing the sub-technique, and `confidence ∈ {0.6, 0.4}`. Reuses the existing `PackageDbEntry` shape; no enum extension required (the CDX `binary-analysis` technique already covers both sub-paths).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator scanning a synthetic binary that statically links OpenSSL 3.0.13 (test fixture built at milestone time) sees `pkg:generic/openssl@3.0.13` in the emitted SBOM, with `evidence.identity[].technique = binary-analysis` (CDX-native) + companion property `mikebom:identification-method = embedded-version-string` and the correct `evidence.occurrences[]` pointing at the fixture binary.
- **SC-002**: An operator scanning a UPX-packed binary (fixture: any small ELF binary processed through `upx --best`) sees `mikebom:binary-packer = upx` on the file-level binary component.
- **SC-003**: A binary that exports the OpenSSL public symbol set without an embedded version string produces a `pkg:generic/openssl` component (no version) with `evidence.identity[].technique = binary-analysis` (CDX-native) + companion property `mikebom:identification-method = symbol-fingerprint` and `confidence = 0.4`.
- **SC-004**: 100% of pre-096 milestone test suites pass post-implementation. No regressions in any ecosystem reader; no regressions in existing binary-scanner assertions; no regressions in SBOM format parity tests; no regressions in transitive-parity tests.
- **SC-005**: `./scripts/pre-pr.sh` clean post-implementation — zero clippy warnings, every test target reports `0 failed`. `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` opt-in also passes (new components carry standards-native fields only).
- **SC-006**: The new fixtures cover: (a) a stripped binary statically linking OpenSSL with embedded version string intact, (b) a UPX-packed binary, (c) a binary that exports OpenSSL symbols without version string. At least one synthetic test fixture per scenario; built reproducibly via a small shell/Rust helper checked into `mikebom-cli/tests/`.
- **SC-007**: False-positive rate measurement: when mikebom is run against the 9 existing ecosystem regression goldens' fixtures (which were NOT designed to trigger the new techniques), at most 1 new component is emitted across all 9. This bounds the risk of unintended emissions from coincidental string matches in unrelated fixtures.

## Assumptions

- The `object` crate already handles ELF/Mach-O/PE section iteration that the new techniques need (per existing usage in `binary/elf.rs`, `binary/macho.rs`, `binary/pe.rs`).
- The packer-detection set can ship with UPX-only in v1; mpress / ASPack / PECompact are stretch goals deferred if planning shows additional complexity. UPX is the dominant target in the wild.
- The version-string pattern set is small enough (≤10 patterns in v1) that linear substring-search is acceptable performance-wise. Most binaries are ≤10 MB; scanning each `.rodata` section is bounded.
- Symbol-fingerprint matching is exact-string (no glob/regex on symbol names). v1 uses simple set-membership against the binary's exported-symbol set.
- New components are emitted as `pkg:generic/...` PURLs because the binary scanner has no package-manager context to assign a more specific PURL type. Future milestones could enrich with CPE candidates or sigstore signatures.
- Confidence values (0.85 for `manifest-analysis` technique, 0.6 for `binary-analysis` + `embedded-version-string` sub-method, 0.4 for `binary-analysis` + `symbol-fingerprint` sub-method) follow the existing convention from milestone 004's binary-scanner work. Tier-3 evidence stays below tier-1.
- Test fixtures live in `mikebom-cli/tests/fixtures/binaries/` (the stay-set per milestone-090). New fixtures may need to be added; their generation script lives in `mikebom-cli/tests/`.

## Dependencies

- Milestone 004 (binary scanner foundation) — the existing infrastructure this milestone extends.
- Milestone 038 (deep-hash) — the SHA-256 emission that complements identification (lets operators cross-reference identified components against fingerprint databases later).
- Milestone 052 (lifecycle scope) — the standards-native scope/property fields the new transparency property aligns with.
- Milestone 090 (fixture-repo split) — new binary fixtures may live in mikebom main (stay-set) per the milestone-090 decision since they're opaque binaries with no manifest content.
- The `object` crate (existing workspace dep) — used for ELF/Mach-O/PE parsing.

## Out of Scope

- **CPE candidate emission from identified components** (e.g., `openssl@3.0.13` → `cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*`). High value for vulnerability matching but out of scope; could be a milestone-097 follow-up.
- **External fingerprint-database lookups** (deps.dev, ClearlyDefined, Software Heritage). Network-fetch fallback; parallel to the milestone-091 / Conan-Center decisions — separate future milestone.
- **DWARF debug-info parsing** (`.debug_info`, `.debug_line` sections). Higher cost; would extract compile-time source-file paths + compiler version. Out of scope; could be a milestone-098 follow-up. Most unknown binaries are stripped, so DWARF helps a minority of cases.
- **Compiler / linker version extraction** (ELF `.comment` section, Mach-O `LC_BUILD_VERSION`, PE LinkerVersion). Useful build-provenance signal but secondary to identification — operator wants "what's INSIDE" more than "who built it". Out of scope for v1.
- **Mach-O LC_LOAD_DYLIB version-field extraction** (current+compat numbers). Improves dynamic-linkage component quality on macOS but is orthogonal to the unknown-binary identification focus. Defer to a separate milestone if maintenance bandwidth permits.
- **Cleaner generic PURLs** (`pkg:generic/libSystem.B.dylib` instead of `pkg:generic/%2Fusr%2Flib%2FlibSystem.B.dylib`). Quality-of-life improvement; can ship as a tiny follow-up after v1.
- **Source-side C/C++ readers** (Conan, vcpkg, CMake, Meson, Bazel). Tracked separately; Conan spec parked at branch `095-conan-reader` for resumption later.
- **Static-library archive parsing** (`.a` / `.lib` archive bodies for embedded version constants). Out of scope; v1 only scans linked binaries, not their pre-link archives.
- **eBPF-trace-based binary identification** (observed library loads at runtime). Different signal channel; orthogonal to static binary inspection. Already exists experimentally for source-side tracing.
- **Yara-rule-based identification** (using community Yara rules for malware-family / library detection). Powerful but adds a large dep (`yara-x` or similar) and a maintained rule corpus. Out of scope for "start simple".
- **Confidence-tier expansion** (e.g., adding tiers between 0.6 and 0.85 for hybrid evidence). v1 uses fixed two-tier values; future milestones can refine.
- **Per-platform PE/Mach-O symbol-fingerprint extraction**. v1 implements symbol-fingerprinting on ELF only (`.dynsym` is well-defined and reliable). PE exports and Mach-O exports are stretch goals.
- **Per-library version-validation rules** (e.g., "if OpenSSL version says 99.99.99, reject as obviously-fake"). v1 trusts the extracted version string verbatim; downstream consumers handle sanity-checking.
- **Patch-level / build-string extraction** beyond major.minor.patch. E.g., extracting `"OpenSSL 3.0.13 30 Jan 2024 built on: ..."`'s build timestamp into a property. Out of scope; only the version triplet is captured.
