# Research — milestone 097 CPE candidate emission

Phase 0 investigation. Four decision points, all resolved with audit-of-existing-code outcomes.

## §1 — Existing CPE infrastructure audit (FR-008)

**Decision**: extend `mikebom-cli/src/generate/cpe.rs::synthesize_cpes()` only. Zero changes to any emission file.

**Evidence (from auditing main at `eca26f1`)**:

| Concern | Where it already lives | Change needed |
|---------|-----------------------|---------------|
| CPE 2.3 format-string construction | `cpe.rs:129-136::format_cpe()` | None — reused |
| Per-spec character escape | `cpe.rs:145-156::cpe_escape()` | None — reused |
| Ecosystem-keyed candidate routing | `cpe.rs:21-111::synthesize_cpes()` match arm | **Add `"generic"` arm** |
| Post-dedup invocation per ResolvedComponent | `scan_fs/mod.rs:641` | None — already runs |
| CDX `component.cpe` emission | `cyclonedx/builder.rs:710` (first candidate) | None — auto-picks new candidate |
| CDX `mikebom:cpe-candidates` property (overflow) | `cyclonedx/builder.rs:711-716` | None — applies when >1 candidate |
| SPDX 2.3 `cpe23Type` external ref | `spdx/packages.rs:406-410` (first candidate) | None — auto-picks new candidate |
| SPDX 3 `Software:cpe` external-id | `spdx/v3_external_ids.rs:51` iterates `cpes` | None — auto-picks new candidate |
| Empty-version fast-return (FR-004) | `cpe.rs:25-28` | None — applies to symbol-fingerprint-only PURLs for free |

**Net delta**: ~10 lines of mapping table + ~10 lines of new ecosystem arm + tests.

**Rationale**: post-milestone-096, the binary-extracted `pkg:generic/openssl@3.0.13` PackageDbEntry flows through the same dedup → `synthesize_cpes(c)` pipeline as every other component. The synthesizer's "_ => empty Vec" catch-all at `cpe.rs:101-104` is the gap; adding a "generic" arm before the catch-all closes it.

**Alternatives considered**:
- **New module `cpe_candidates.rs`**: rejected — adds indirection without benefit. The "generic" mapping is naturally one more arm in the existing per-ecosystem `match`.
- **Mapping table as `HashMap`**: rejected — 10 entries; linear-scan via `iter().find()` on a `const slice` is faster + diff-friendlier + compile-time-checked.
- **Touch emission code**: not needed — all three format emitters already iterate `component.cpes`; adding a candidate to the Vec automatically surfaces.

## §2 — v1 CPE mapping table (FR-001, FR-002)

**Decision**: 10 entries covering the milestone-096 / earlier curated-library set (11 libs minus BoringSSL). Each row is `(library_slug, vendor, product)` with NVD-citation comments.

| `library_slug` | NVD `vendor` | NVD `product` | Rationale |
|---------------|--------------|--------------|-----------|
| `openssl` | `openssl` | `openssl` | NVD canonical — every OpenSSL CVE since 2014 uses this pair |
| `zlib` | `zlib` | `zlib` | NVD canonical |
| `sqlite` | `sqlite` | `sqlite` | NVD canonical |
| `curl` | `haxx` | `curl` | Historical NVD vendor for libcurl; modern records still file under `haxx:curl`. Some recent records use `curl:curl` — secondary candidate emitted via the existing multi-CPE property |
| `pcre` | `pcre` | `pcre` | NVD canonical for PCRE 8.x |
| `pcre2` | `pcre` | `pcre2` | NVD canonical for PCRE 10.x (same vendor, different product) |
| `gnutls` | `gnu` | `gnutls` | NVD canonical |
| `libressl` | `openbsd` | `libressl` | NVD canonical — most CVEs filed under OpenBSD's vendor namespace; `libressl:libressl` also appears for some records (secondary candidate) |
| `llvm` | `llvm` | `llvm` | NVD canonical for the umbrella project (sub-projects clang/lld out of scope for v1) |
| `openjdk` | `oracle` | `openjdk` | NVD canonical — `oracle:openjdk` dominates Java vuln records |

**Omitted from v1**:
- `boringssl` — no NVD-tracked CPE namespace; vulnerabilities flow through `openssl:openssl` records when the issue exists in upstream OpenSSL, but BoringSSL-specific issues lack their own CPE. Documented in spec Edge Cases. Skipping the row means BoringSSL components emit PURL without CPE — explicit "we don't know".

**Multi-candidate emissions** (where two vendor:product pairs are plausible):
- `curl` → `[haxx:curl, curl:curl]` — first goes to `component.cpe`, both go to `mikebom:cpe-candidates`
- `libressl` → `[openbsd:libressl, libressl:libressl]` — same pattern
- Other 8 libraries emit a single candidate.

**Rationale**: the existing `synthesize_cpes` returns a `Vec<String>` to support exactly this multi-candidate case — the CDX emitter takes `[0]` as primary and dumps the full list into the `mikebom:cpe-candidates` property. Operators with fuzzy CVE matchers (Trivy, Grype) take the union; operators with strict matchers get the most-cited candidate. Matches the existing per-ecosystem pattern (dpkg emits `[debian:<pkg>, <pkg>:<pkg>]` per `cpe.rs:36-39`).

## §3 — Version-handling decisions (FR-001 Edge Cases)

**Decision**:

| Case | Handling | Reason |
|------|----------|--------|
| OpenSSL letter-suffix (`1.1.1w`) | Emit as `cpe:...:openssl:openssl:1.1.1w:...` (verbatim) | CPE 2.3 §6.2 permits lowercase letters in version field; NVD records OpenSSL letters this way |
| OpenJDK build-suffix (`21.0.1+12`) | Strip suffix before emission → `cpe:...:oracle:openjdk:21.0.1:...` | NVD inconsistency — sometimes `update_12`, sometimes `*`, sometimes omitted. Stripping matches the most-common shape |
| SQLite source-id-only (no triplet captured) | Skip CPE emission (empty `version` → `synthesize_cpes` returns empty Vec) | Already handled by `cpe.rs:25-28` empty-version fast-return |
| Symbol-fingerprint-only `pkg:generic/openssl` | Skip CPE emission (empty version) | Same path as SQLite-source-id-only; FR-004 satisfied for free |
| Version with internal escape need (e.g. `+`, `/`) | `cpe_escape()` handles per CPE 2.3 §6.2 | Already in code at `cpe.rs:145-156` |

**OpenJDK suffix strip — implementation**: at table-lookup time, special-case the `openjdk` slug to truncate the version at the first non-`[0-9.]` character. ~5 lines of code; doc-string explains the NVD inconsistency.

## §4 — Test scope (SC-001, SC-002, SC-003, SC-004)

**Decision**: extend the existing `cpe.rs::tests` module with 4 new test functions; add 1 integration test in `mikebom-cli/tests/cpe_binary_id.rs` that scans a synthetic fixture (or uses the milestone-096 control test pattern: mikebom-itself scan + assert NO openssl CPE because mikebom uses rustls).

**Unit tests** (in `cpe.rs::tests`):
1. `generic_openssl_emits_canonical_cpe` — `make_component("pkg:generic/openssl@3.0.13")` → asserts `cpes[0] == "cpe:2.3:a:openssl:openssl:3.0.13:*:..."`.
2. `generic_curl_emits_dual_candidates` — `pkg:generic/curl@8.4.0` → asserts both `haxx:curl` and `curl:curl` candidates present.
3. `generic_unknown_library_returns_empty` — RENAME existing `unknown_ecosystem_returns_empty` to clarify intent: `pkg:generic/weird@1.0.0` still returns empty because `weird` isn't in the mapping table.
4. `generic_openjdk_strips_build_suffix` — `pkg:generic/openjdk@21.0.1+12` → asserts `cpes[0] == "cpe:2.3:a:oracle:openjdk:21.0.1:*:..."` (suffix stripped).

**Integration test** (`mikebom-cli/tests/cpe_binary_id.rs`):
- Scan a directory containing only the mikebom binary itself (which doesn't link OpenSSL) and assert the SBOM has zero `pkg:generic/openssl` components AND zero `cpe:2.3:a:openssl:openssl:*` strings anywhere in the JSON. Negative control for SC-007 spurious-emission bound.
- (Optional positive control if a fixture binary is available: scan it, assert the emitted CDX has `components[*] | select(.purl=="pkg:generic/openssl@<ver>") | .cpe` matches the expected CPE 2.3 shape. Toolchain-graceful-skip if no fixture available — matches the milestone-096 pattern.)

**SC-002 (CPE syntax validation)**: handled by `cpe_escape()` + the `format_cpe()` template — every emitted string is by construction a valid CPE 2.3 formatted-string binding. No external validator needed; mikebom emits, downstream tools parse.

## §5 — Goldens regen forecast (FR-009, SC-006)

**Decision**: regenerate the 3 format goldens once, accept the additive diff.

**Forecast**: per milestone-096 SC-007 ≤1-spurious-bound, at most ONE existing fixture component newly gains a CPE field — likely zero. Realistically:
- The 9 existing ecosystem fixtures (`apk/cargo/deb/gem/golang/maven/npm/pip/rpm`) do not contain `pkg:generic/<v1-library>@<version>` components (binary scanner doesn't fire on those fixtures; they're synthetic package-db fixtures).
- The binary-id fixture set (milestone-096 controls in `binary_id_enrich.rs`) is integration-tested only, not gold-encoded.

**Worst case**: zero golden regen. **Most-likely case**: zero. Diff scope guard via `git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1`.

## Coverage map

| Spec section | Resolution |
|--------------|------------|
| FR-001 (v1 CPE mapping for 10 libraries) | §2 → table locked |
| FR-002 (in-source `const` table, alphabetical) | §2 + §1 → reuse existing `const`-pattern in `cpe.rs` |
| FR-003 (silent skip on missing table entry) | §1 → existing match arm's catch-all returns empty Vec |
| FR-004 (no CPE for versionless components) | §1 + §3 → existing empty-version fast-return |
| FR-005 (composite-evidence inheritance) | §1 → milestone-096 merge produces single PackageDbEntry with `@version` PURL; CPE attaches to that |
| FR-006 (syntax-valid CPE) | §3 + existing `cpe_escape()` |
| FR-007 (no new Cargo deps) | §1-§4 — purely in-source |
| FR-008 (production code scope) | §1 → single-file delta to `cpe.rs` |
| FR-009 (≤ N golden regen) | §5 → forecast: zero |
| FR-010 (parity catalog) | §1 → CPE fields are standards-native; no catalog row needed |
| SC-001/SC-002/SC-003/SC-004 | §4 → unit + integration tests |
| SC-005 (pre-PR gate clean) | inherits from CI |
| SC-006 (golden regen scope) | §5 → expect zero |
| SC-007 (zero Cargo deps) | inherits from FR-007 |
| Constitution V audit | §1 → all 3 format fields are standards-native; no `mikebom:*` annotation added |
| Constitution X transparency | §2 → mapping table in-source with NVD-citation comments per row |

All open spec questions resolved. Ready for Phase 1 (data-model + contracts + quickstart).
