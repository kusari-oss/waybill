# Quickstart — milestone 096 maintainer recipes

Six maintainer-facing recipes to apply the binary-identification enrichment and verify each contract.

## Recipe 1 — Implement the three new extraction passes (FR-001, FR-003, FR-004)

Three changes to `mikebom-cli/src/scan_fs/binary/`:

**1a. Extend `version_strings.rs`** — fill in the stub with the v1 pattern table from `data-model.md §version_strings.rs`:

```rust
const VERSION_STRING_PATTERNS: &[(&str, &str)] = &[
    ("openssl",  r"OpenSSL (\d+\.\d+\.\d+[a-z]?)\s+\d{1,2}\s+[A-Z][a-z]{2}\s+\d{4}"),
    ("zlib",     r"deflate (\d+\.\d+\.\d+) Copyright \d{4}-\d{4} Jean-loup Gailly"),
    ("libcurl",  r"libcurl/(\d+\.\d+\.\d+)(?:-[A-Za-z0-9]+)?"),
    ("sqlite",   r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} [0-9a-f]{64}"),
    ("libxml2",  r"libxml2[/-](\d+\.\d+\.\d+)"),
];

pub fn extract_embedded_versions(bytes: &[u8]) -> Vec<EmbeddedVersionMatch> {
    let mut hits = Vec::new();
    for (library, regex_str) in VERSION_STRING_PATTERNS {
        let re = regex::bytes::Regex::new(regex_str).expect("static-compiled regex");
        for cap in re.captures_iter(bytes) {
            // Capture group 1 is the version (or empty for SQLite source-id-only).
            // ...
        }
    }
    hits
}
```

Note on `regex::bytes`: existing workspace dep, no new crate. Confirmed at planning time.

**1b. Extend `packer.rs`** — UPX detection per data-model §packer.rs:

```rust
const UPX_MAGIC: &[u8] = b"UPX!";

pub fn detect_packer(file: &object::read::File<'_>, bytes: &[u8]) -> Option<&'static str> {
    // Signal A: ELF/PE section names.
    let has_upx_sections = file
        .sections()
        .filter_map(|s| s.name().ok())
        .filter(|n| n.starts_with("UPX"))
        .count() >= 2;
    if has_upx_sections {
        return Some("upx");
    }
    // Signal B: universal magic-bytes scan.
    if memchr::memmem::find(bytes, UPX_MAGIC).is_some() {
        return Some("upx");
    }
    None
}
```

The wider context in `binary/mod.rs::read()` always emits the property (per Q2):

```rust
let packer_value = packer::detect_packer(&file, &bytes).unwrap_or("none");
component.properties.insert("mikebom:binary-packer".to_string(),
                            packer_value.to_string());
```

**1c. Create `symbol_fingerprint.rs`** — ELF-only `.dynsym` matcher per data-model §symbol_fingerprint.rs. Use `object::read::File::symbols()` or `object::read::elf::ElfFile::dynamic_symbols()`.

## Recipe 2 — Wire the new passes + composite-evidence aggregation (Q1, FR-005)

`binary/mod.rs::read()` aggregation pseudocode:

```rust
let version_hits = version_strings::extract_embedded_versions(&bytes);
let symbol_hits = symbol_fingerprint::match_symbols(&file);  // ELF only

// Merge by library name: composite-evidence per Q1.
let mut by_library: HashMap<String, PackageDbEntry> = HashMap::new();
for vh in version_hits {
    let entry = by_library.entry(vh.library.to_string()).or_insert_with(|| {
        PackageDbEntry::new(
            format!("pkg:generic/{}@{}", vh.library, vh.version),
            vh.library.to_string(),
        )
    });
    entry.add_identity_evidence(vh.to_identity_evidence());  // confidence 0.6
}
for sh in symbol_hits {
    let entry_key = sh.library.to_string();
    if let Some(existing) = by_library.get_mut(&entry_key) {
        // Composite: append the symbol-fingerprint evidence to the existing entry.
        existing.add_identity_evidence(sh.to_identity_evidence());  // confidence 0.4
    } else {
        // Symbol-only fingerprint, no version.
        let entry = PackageDbEntry::new(
            format!("pkg:generic/{}", sh.library),
            sh.library.to_string(),
        );
        entry.add_identity_evidence(sh.to_identity_evidence());
        by_library.insert(entry_key, entry);
    }
}

let new_components: Vec<PackageDbEntry> = by_library.into_values().collect();
```

Then add `new_components` to the existing component-emission flow. The existing `linkage::dedup_globally` pass handles cross-binary strict-PURL-equality dedup automatically (Q3).

## Recipe 3 — Add the parity-catalog C12 row (research §5)

Edit `mikebom-cli/src/parity/extractors/mod.rs` after the existing C10/C11 entries:

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

Add the extractor stubs to `cdx.rs`, `spdx2.rs`, `spdx3.rs`:

```rust
// cdx.rs:
cdx_anno!(c12_cdx, "mikebom:binary-packer", component);

// spdx2.rs:
spdx23_anno!(c12_spdx23, "mikebom:binary-packer", component);

// spdx3.rs:
spdx3_anno!(c12_spdx3, "mikebom:binary-packer", component);
```

Update `docs/reference/sbom-format-mapping.md` to add the C12 row mirroring C10/C11's table format.

## Recipe 4 — Build the 3 synthetic test fixtures

**4a. OpenSSL-static fixture**:

```bash
mkdir -p mikebom-cli/tests/fixtures/binary-id/openssl-static
cat > mikebom-cli/tests/fixtures/binary-id/openssl-static/main.c <<'EOF'
#include <openssl/ssl.h>
#include <stdio.h>
int main(void) {
    OPENSSL_init_ssl(0, NULL);
    SSL_CTX *ctx = SSL_CTX_new(TLS_method());
    printf("OK\n");
    if (ctx) SSL_CTX_free(ctx);
    return 0;
}
EOF
# Build with static OpenSSL (paths vary by system):
cc -static main.c -o main -lssl -lcrypto -ldl -lpthread
strip main
```

If the CI runner doesn't have static OpenSSL available, the test gracefully skips (matches the milestone-078 `spdx3-validate` deferred-toolchain pattern).

**4b. UPX-packed fixture**:

```bash
# Use mikebom itself as the input binary (any small ELF works):
cp target/release/mikebom mikebom-cli/tests/fixtures/binary-id/upx-packed/sample
upx --best mikebom-cli/tests/fixtures/binary-id/upx-packed/sample
# (upx must be on PATH; if absent, test skips)
```

**4c. Symbol-only-fingerprint fixture**:

```bash
# Compile a C program that explicitly exports OpenSSL's symbol set
# but has the version string stripped (or use OpenSSL build with
# OPENSSL_VERSION_TEXT="" override).
# Pragmatic alternative for v1: use the openssl-static binary but
# overwrite the version-string bytes in .rodata with NULs via a
# small post-build helper.
```

## Recipe 5 — Write the 3 integration tests

`mikebom-cli/tests/binary_embedded_version_strings.rs`:

```rust
#[test]
fn extracts_openssl_3_from_statically_linked_binary() {
    let fixture = build_or_skip("openssl-static");
    let sbom = scan_to_cdx(&fixture);
    let openssl = find_component(&sbom, |c| c["purl"].as_str().unwrap().starts_with("pkg:generic/openssl@"));
    assert!(openssl.is_some(), "expected an openssl component with version");
    // Verify evidence.identity[].confidence == 0.6, identification-method = embedded-version-string.
}
```

Similar shape for `binary_packer_detection.rs` (Contract 2) and `binary_symbol_fingerprint.rs` (Contract 3). Each test SHOULD gracefully skip if the toolchain it needs (`cc`, `upx`, `openssl-dev`) isn't available — `eprintln!` skip reason, exit 0.

## Recipe 6 — Run pre-PR gate + verify diff scope

```bash
./scripts/pre-pr.sh
# Expected: `>>> all pre-PR checks passed.`

# Diff scope guardrails (Contract 7):
git diff --name-only main | sort
# Expected (allowlist):
#   docs/reference/sbom-format-mapping.md
#   mikebom-cli/src/generate/cyclonedx/...     (1-2 files for property emission)
#   mikebom-cli/src/generate/spdx/annotations.rs
#   mikebom-cli/src/parity/extractors/cdx.rs
#   mikebom-cli/src/parity/extractors/mod.rs
#   mikebom-cli/src/parity/extractors/spdx2.rs
#   mikebom-cli/src/parity/extractors/spdx3.rs
#   mikebom-cli/src/scan_fs/binary/mod.rs
#   mikebom-cli/src/scan_fs/binary/packer.rs
#   mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs
#   mikebom-cli/src/scan_fs/binary/version_strings.rs
#   mikebom-cli/tests/binary_embedded_version_strings.rs
#   mikebom-cli/tests/binary_packer_detection.rs
#   mikebom-cli/tests/binary_symbol_fingerprint.rs
#   mikebom-cli/tests/fixtures/binary-id/...   (3 fixture dirs)
#   specs/096-binary-id-enrich/...

# Zero Cargo.lock/toml change (FR-007):
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' && echo "DEP CHURN" || echo "clean"
```

## When in doubt

- **A binary triggers `mikebom:binary-packer = upx` even though it isn't packed**: the magic-bytes scan caught a coincidental `UPX!` substring. Check by inspecting `strings <bin> | grep UPX`. If the only occurrence is the literal `UPX!` AND the section table has no `UPX0`/`UPX1` sections, lower confidence by requiring BOTH Signal A AND Signal B (current implementation requires either). Defer that hardening to a follow-up milestone.
- **Symbol fingerprint missing for a known-statically-linked library**: confirm the binary's `.dynsym` table is present (`readelf -d <bin> | grep DYNSYM`). Fully-stripped binaries (with `.dynsym` removed) bypass symbol-fingerprinting entirely — that's an honest "we don't know", not a defect.
- **A version string match yields wrong version**: the pattern's capture group needs adjustment. Each pattern is in-source `const`; edit `version_strings.rs` and recompile.
- **The composite-evidence aggregator emits TWO components for the same library**: the per-binary library-name merging step in `binary/mod.rs::read()` isn't running, OR the library-name keys don't match (e.g., "openssl" vs "OpenSSL" — case mismatch). Verify the matcher normalizes to lowercase before keying the `HashMap`.
- **Cross-binary dedup emits N components for the same PURL**: the `linkage::dedup_globally` pass isn't being invoked for the new techniques. Confirm the new components are added to the same vector that the dedup pass consumes.
- **Goldens regenerate unexpectedly on an existing-ecosystem fixture**: a spurious version-string or symbol match fired. Check `git diff mikebom-cli/tests/fixtures/golden/` and the new diff content. If the spurious match is real evidence (a library IS statically linked in that fixture), accept the new component. If it's a false positive, narrow the pattern's anchor to be more distinctive.
