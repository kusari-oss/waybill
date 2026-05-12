# Data Model — milestone 096

Per-file shape of every deliverable. Three new extraction passes in `mikebom-cli/src/scan_fs/binary/` + one parity-catalog row + per-format extractor stubs + three integration test files. Zero schema changes outside emission (every new field flows through existing `PackageDbEntry` + property mechanisms).

## File inventory

| File | State | Owner FRs / clarifications |
|------|-------|---------------------------|
| `mikebom-cli/src/scan_fs/binary/version_strings.rs` | EXTEND existing stub | FR-001, FR-002 |
| `mikebom-cli/src/scan_fs/binary/packer.rs` | EXTEND existing stub | FR-003, Q2 (always-emit) |
| `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs` | NEW | FR-004 |
| `mikebom-cli/src/scan_fs/binary/mod.rs` | MODIFY | wire three new passes; composite-evidence merging per Q1; strict-PURL-equality dedup per Q3 |
| `mikebom-cli/src/parity/extractors/mod.rs` | MODIFY | add C12 row registration |
| `mikebom-cli/src/parity/extractors/cdx.rs` | MODIFY | add `c12_cdx` extractor |
| `mikebom-cli/src/parity/extractors/spdx2.rs` | MODIFY | add `c12_spdx23` extractor |
| `mikebom-cli/src/parity/extractors/spdx3.rs` | MODIFY | add `c12_spdx3` extractor |
| `mikebom-cli/src/generate/cyclonedx/...` | MODIFY | emit `mikebom:binary-packer` in `component.properties[]` |
| `mikebom-cli/src/generate/spdx/annotations.rs` | MODIFY | emit `mikebom:binary-packer` in `Package.annotations[]` / SPDX 3 `Annotation.statement` |
| `docs/reference/sbom-format-mapping.md` | MODIFY | add C12 row |
| `mikebom-cli/tests/binary_embedded_version_strings.rs` | NEW | integration tests for FR-001 |
| `mikebom-cli/tests/binary_packer_detection.rs` | NEW | integration tests for FR-003 |
| `mikebom-cli/tests/binary_symbol_fingerprint.rs` | NEW | integration tests for FR-004 |

## `version_strings.rs` — extension

**Per-library pattern table** (in-source `const`, per research.md §1):

```rust
/// (library_name, regex_pattern_string) — locked v1 starter set per
/// specs/096-binary-id-enrich/research.md §1.
///
/// Each `regex` matches the binary's `.rodata` (ELF), `__cstring`
/// (Mach-O), `.rdata` (PE) bytes. Capture-group 1 yields the version
/// triplet. Anchors are chosen for high distinctiveness (copyright
/// lines, author names) to bound false-positive risk per SC-007.
const VERSION_STRING_PATTERNS: &[(&str, &str)] = &[
    ("openssl",  r"OpenSSL (\d+\.\d+\.\d+[a-z]?)\s+\d{1,2}\s+[A-Z][a-z]{2}\s+\d{4}"),
    ("zlib",     r"deflate (\d+\.\d+\.\d+) Copyright \d{4}-\d{4} Jean-loup Gailly"),
    ("libcurl",  r"libcurl/(\d+\.\d+\.\d+)(?:-[A-Za-z0-9]+)?"),
    ("sqlite",   r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} [0-9a-f]{64}"),  // SQLITE_SOURCE_ID; co-locate with version
    ("libxml2",  r"libxml2[/-](\d+\.\d+\.\d+)"),
];
```

**Note on SQLite**: the SQLITE_SOURCE_ID pattern doesn't capture the version triplet directly; v1 also scans for `sqlite3_libversion` symbol-table presence + extracts the version from a co-located rodata region. Implementer can choose either: (a) lock SQLite to the source-id-only pattern and emit `pkg:generic/sqlite` (no version) when source-id is found alone; or (b) do the more-involved co-location check. Either approach satisfies FR-001 (`5+ libraries`); the simpler path (a) is recommended for v1.

**Output**: each match → `PackageDbEntry` with:
- `name` = library name (lowercase)
- `version` = captured group 1
- `purl` = `pkg:generic/<name>@<version>` (lowercase name per packageurl-spec)
- `evidence.identity[].technique` = `binary-analysis` (per research §6 — CDX-native)
- `evidence.identity[].confidence` = 0.6
- `evidence.identity[].methods[].technique` = `binary-analysis`
- `properties[]` includes `mikebom:identification-method = embedded-version-string` (per research §6 — companion property capturing sub-technique)
- `evidence.occurrences[]` = list of binary file paths where this exact `<name>@<version>` was found (per Q3 strict-PURL-equality dedup → 1 occurrence per binary contributor)

## `packer.rs` — extension

**Per-packer signature table**:

```rust
const PACKER_SIGNATURES: &[PackerSignature] = &[
    PackerSignature {
        name: "upx",
        // Signal A: ELF/PE section-name combo
        elf_section_names_required: &["UPX0", "UPX1"],
        pe_section_names_required:  &["UPX0", "UPX1"],
        // Signal B: universal magic
        magic_bytes: Some(b"UPX!"),
        // optional stretch — emit version from banner if present
        version_banner_anchor: Some(r"\$Id: UPX (\d+\.\d+(?:\.\d+)?) Copyright"),
    },
    // mpress, ASPack, PECompact stretch slots — Out of Scope for v1 per spec
];
```

**Output**:
- Each scanned binary → `mikebom:binary-packer = upx` (or `none`) ALWAYS on the file-level binary component, per Clarification Q2.
- If `version_banner_anchor` matches: also `mikebom:binary-packer-version = <version>` (stretch).
- No new component emitted — this is a property on the existing file-level binary component, not a separate identification.

## `symbol_fingerprint.rs` — NEW

**Per-library fingerprint table** (per research.md §3):

```rust
/// Symbol-fingerprint set for v1 (ELF-only). Match fires when
/// `required_symbol_count` of `symbols` are present in the binary's
/// `.dynsym` exported-symbol set. Threshold = 80% per FR-004.
struct SymbolFingerprint {
    library_name: &'static str,
    symbols: &'static [&'static str],   // 10 per library
    required_symbol_count: usize,        // 8 (80% of 10)
}

const FINGERPRINTS: &[SymbolFingerprint] = &[
    SymbolFingerprint {
        library_name: "openssl",
        symbols: &[
            "OPENSSL_init_ssl", "OPENSSL_init_crypto", "SSL_CTX_new",
            "SSL_library_init", "EVP_DigestInit_ex", "EVP_EncryptInit_ex",
            "RSA_new", "BN_new", "X509_new", "ERR_get_error",
        ],
        required_symbol_count: 8,
    },
    SymbolFingerprint {
        library_name: "zlib",
        symbols: &[
            "deflate", "inflate", "deflateInit_", "inflateInit_",
            "deflateEnd", "inflateEnd", "crc32", "adler32",
            "compress", "uncompress",
        ],
        required_symbol_count: 8,
    },
    SymbolFingerprint {
        library_name: "libcurl",
        symbols: &[
            "curl_easy_init", "curl_easy_setopt", "curl_easy_perform",
            "curl_easy_cleanup", "curl_easy_getinfo", "curl_multi_init",
            "curl_multi_add_handle", "curl_global_init", "curl_version",
            "curl_slist_append",
        ],
        required_symbol_count: 8,
    },
];
```

**Output**: each fingerprint match → `PackageDbEntry` with:
- `name` = library name (lowercase)
- `version` = empty (symbol-only fingerprint can't pin a version)
- `purl` = `pkg:generic/<name>` (no `@<version>` segment)
- `evidence.identity[].technique` = `binary-analysis`
- `evidence.identity[].confidence` = 0.4
- `properties[]` includes `mikebom:identification-method = symbol-fingerprint`
- `properties[]` includes `mikebom:fingerprint-symbols-matched = <count>/<total>` (e.g., `9/10`) for transparency
- `evidence.occurrences[]` = list of binary file paths where the fingerprint hit

## `mod.rs` — composite-evidence + strict-PURL-equality dedup

After each binary scan, in the `read()` aggregation phase:

1. **Per-binary dedup of LIBRARY-equal hits across techniques** (Clarification Q1):
   - If both `version_strings.rs` and `symbol_fingerprint.rs` fired for the same `library_name` on the same binary, merge them into a single `PackageDbEntry` with TWO `evidence.identity[]` entries (one per technique).
   - The PURL of the merged entry is the version-string PURL (`pkg:generic/openssl@3.0.13`) — the higher-confidence signal pins the identity. The symbol-fingerprint contributes a secondary evidence trail.

2. **Cross-binary dedup by strict-PURL-equality** (Clarification Q3):
   - The existing `linkage::dedup_globally` pass at end of `read()` already does this for dynamic-linkage components. Extend it to cover the new technique components — same global PURL-keyed merge into a single `PackageDbEntry` per unique PURL, with all binary file paths merged into `evidence.occurrences[]`.
   - Different PURLs (e.g., `pkg:generic/openssl@3.0.13` vs `pkg:generic/openssl@3.0.12` vs `pkg:generic/openssl`) stay as separate components.

## Parity-catalog C12 row

**`parity/extractors/mod.rs`**: append to the existing `EXTRACTORS` table after C11:

```rust
ParityExtractor {
    row_id: "C12",
    label: "mikebom:binary-packer",
    cdx: c12_cdx,
    spdx23: c12_spdx23,
    spdx3: c12_spdx3,
    directional: Directionality::SymmetricEqual,
    order_sensitive: false,
},
```

**`parity/extractors/cdx.rs`**: `cdx_anno!(c12_cdx, "mikebom:binary-packer", component);`
**`parity/extractors/spdx2.rs`**: `spdx23_anno!(c12_spdx23, "mikebom:binary-packer", component);`
**`parity/extractors/spdx3.rs`**: `spdx3_anno!(c12_spdx3, "mikebom:binary-packer", component);`

Each macro registration matches the C10 (`mikebom:binary-class`) and C11 (`mikebom:binary-stripped`) patterns exactly.

## `docs/reference/sbom-format-mapping.md` — C12 row

Add a row mirroring C10/C11's format with:
- `row_id`: C12
- `label`: `mikebom:binary-packer`
- `CDX 1.6`: `component.properties[].value` (key = `mikebom:binary-packer`)
- `SPDX 2.3`: `Package.annotations[].comment` (prefix = `mikebom:binary-packer=`)
- `SPDX 3`: `Annotation.statement` (prefix = `mikebom:binary-packer=`)
- `Constitution-V audit`: "no native packer-status field in any of CDX 1.6 / SPDX 2.3 / SPDX 3 schemas; the existing milestone-049/052/084 `mikebom:*` annotation-carrier convention applies. Justified parity-bridging property; same audit reasoning as C10 (`mikebom:binary-class`) and C11 (`mikebom:binary-stripped`)."

## Test fixtures

Three new integration test files build their fixtures inline at test-build time using `cc` invocations or pre-built scripts (similar to `mikebom-cli/tests/scan_*` patterns). Each fixture is a small synthetic binary:

| Fixture | Contents | Test file |
|---------|----------|-----------|
| `static-openssl-linked` | C program statically linking OpenSSL 3.x (musl-libc-static + openssl-static); stripped | `binary_embedded_version_strings.rs` |
| `upx-packed` | Any small ELF binary (e.g., `cargo build --release -p mikebom-cli` output run through `upx --best`) | `binary_packer_detection.rs` |
| `openssl-symbols-only` | A C program that explicitly exports OpenSSL's symbol set without embedding the version string (e.g., custom-built OpenSSL with `OPENSSL_VERSION_TEXT` macro overridden to empty) | `binary_symbol_fingerprint.rs` |

**Fixture-build mechanism**: each test file includes a `setup_fixture()` helper that runs `cc` / `make` at test time. If the toolchain isn't available (e.g., no `openssl-dev` headers on the CI runner), the test gracefully `eprintln!`s "skipping — fixture unbuildable" and exits 0 — matches the milestone-078 `spdx3-validate` deferred-toolchain pattern. CI does NOT block on these tests when toolchain absent; local dev runs them when toolchain available.

**Alternative**: pre-build the 3 fixtures once and check them into `mikebom-cli/tests/fixtures/binaries/elf/` (stay-set per milestone-090). Lower runtime cost; mild repo-size increase. Decision deferred to implementation — either approach satisfies SC-006.

## Compatibility

- **No `Cargo.lock` change** — the `object` crate already in workspace covers all parsing.
- **No golden regen on existing 9 ecosystems** — pattern anchors are distinctive enough that no current fixture should match (per SC-007 ≤1-component-spurious bound). The 3 new fixtures + 3 new test files are the only diff-adding artifacts.
- **Backward compatibility**: 100% additive. Any binary that wouldn't match v1's patterns produces the same SBOM it does today (just adds `mikebom:binary-packer = none` to file-level binary components per Q2 — that's an additive property, not a behavior change).

## No JSON / no YAML schema additions

Zero new fields in any output schema. The new properties + evidence.identity[] entries flow through the existing component-properties + evidence-trail mechanisms.
