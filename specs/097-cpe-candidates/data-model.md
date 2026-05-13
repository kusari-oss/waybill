# Data Model — milestone 097

Single-file delta to `mikebom-cli/src/generate/cpe.rs`. The mapping table + the new `"generic"` ecosystem arm are the entire structural change.

## File inventory

| File | State | Owner FRs |
|------|-------|-----------|
| `mikebom-cli/src/generate/cpe.rs` | EXTEND existing | FR-001, FR-002, FR-003, FR-006 |
| `mikebom-cli/tests/cpe_binary_id.rs` | NEW (or extend `binary_id_enrich.rs`) | SC-001 integration coverage |

No changes outside these two files. SPDX 2.3 / SPDX 3 / CDX emission paths already iterate `component.cpes` and pick up new candidates automatically (audited at research §1).

## `cpe.rs` — extension shape

**New const table at top of module** (after the existing imports):

```rust
/// v1 library → (NVD vendor, NVD product) for `pkg:generic/<lib>@<version>`
/// components per milestone-097 FR-001 + research §2. Sorted alphabetically
/// by library_slug for diff-friendliness per FR-002.
///
/// Each row carries an in-source citation comment documenting the NVD-canonical
/// vendor:product choice. Multi-candidate rows (curl, libressl) emit both pairs
/// so downstream NVD matchers can take the union.
const GENERIC_LIBRARY_CPES: &[(&str, &[(&str, &str)])] = &[
    // OpenSSL — every CVE since 2014 uses openssl:openssl
    ("openssl",  &[("openssl",  "openssl")]),
    // zlib — NVD canonical
    ("zlib",     &[("zlib",     "zlib")]),
    // SQLite — NVD canonical
    ("sqlite",   &[("sqlite",   "sqlite")]),
    // libcurl — historical haxx:curl dominates; modern curl:curl also appears
    ("curl",     &[("haxx",     "curl"), ("curl", "curl")]),
    // PCRE 8.x
    ("pcre",     &[("pcre",     "pcre")]),
    // PCRE 10.x — same vendor, different product
    ("pcre2",    &[("pcre",     "pcre2")]),
    // GnuTLS
    ("gnutls",   &[("gnu",      "gnutls")]),
    // LibreSSL — most CVEs under openbsd:libressl; libressl:libressl secondary
    ("libressl", &[("openbsd",  "libressl"), ("libressl", "libressl")]),
    // LLVM umbrella (sub-projects clang/lld out of scope per spec)
    ("llvm",     &[("llvm",     "llvm")]),
    // OpenJDK — Java vuln records consistently file under oracle:openjdk
    ("openjdk",  &[("oracle",   "openjdk")]),
];
```

**New ecosystem arm** (inserted into the existing `match ecosystem` block in `synthesize_cpes`, immediately before the `_ => return Vec::new()` catch-all):

```rust
"generic" => {
    // Lookup the library slug in the v1 mapping table. Missing slugs
    // return empty (FR-003 — silent skip, no error).
    let mapping = GENERIC_LIBRARY_CPES
        .iter()
        .find(|(slug, _)| *slug == name.as_str())
        .map(|(_, vendors)| *vendors);
    let Some(vendors) = mapping else {
        return Vec::new();
    };
    // OpenJDK special-case: strip build-suffix (`21.0.1+12` → `21.0.1`)
    // before CPE emission per research §3 NVD-shape rationale. Version
    // stays verbatim on the PURL — only the CPE string is normalized.
    let cpe_version = if name == "openjdk" {
        version
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .next()
            .unwrap_or(version)
    } else {
        version.as_str()
    };
    return vendors
        .iter()
        .map(|(vendor, product)| format_cpe(vendor, product, cpe_version))
        .collect();
},
```

**Updated test** (existing `unknown_ecosystem_returns_empty` renamed to clarify post-097 semantics — now matches by table-miss rather than ecosystem-miss):

```rust
#[test]
fn generic_unknown_library_returns_empty() {
    // `weird` is not in the v1 GENERIC_LIBRARY_CPES table → FR-003
    // silent-skip → empty Vec emission.
    let c = make_component("pkg:generic/weird@1.0.0");
    let cpes = synthesize_cpes(&c);
    assert!(cpes.is_empty());
}
```

**New tests** (added to the existing `tests` module):

```rust
#[test]
fn generic_openssl_emits_canonical_cpe() {
    let c = make_component("pkg:generic/openssl@3.0.13");
    let cpes = synthesize_cpes(&c);
    assert_eq!(cpes.len(), 1);
    assert_eq!(cpes[0], "cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*");
}

#[test]
fn generic_curl_emits_dual_candidates() {
    // Multi-vendor case — both haxx:curl and curl:curl emitted.
    let c = make_component("pkg:generic/curl@8.4.0");
    let cpes = synthesize_cpes(&c);
    assert_eq!(cpes.len(), 2);
    assert_eq!(cpes[0], "cpe:2.3:a:haxx:curl:8.4.0:*:*:*:*:*:*:*");
    assert_eq!(cpes[1], "cpe:2.3:a:curl:curl:8.4.0:*:*:*:*:*:*:*");
}

#[test]
fn generic_openjdk_strips_build_suffix() {
    // NVD-shape normalization — `21.0.1+12` → `21.0.1` for the CPE only.
    let c = make_component("pkg:generic/openjdk@21.0.1+12");
    let cpes = synthesize_cpes(&c);
    assert_eq!(cpes.len(), 1);
    assert_eq!(cpes[0], "cpe:2.3:a:oracle:openjdk:21.0.1:*:*:*:*:*:*:*");
}

#[test]
fn generic_symbol_fingerprint_only_emits_no_cpe() {
    // FR-004 — `pkg:generic/openssl` (no version) → empty version → empty Vec.
    let mut c = make_component("pkg:generic/openssl@dummy");
    c.version = String::new();
    let cpes = synthesize_cpes(&c);
    assert!(cpes.is_empty());
}
```

## `cpe_binary_id.rs` — NEW integration test

Mirrors the milestone-096 negative-control test pattern. Scans the mikebom binary itself (or any unpacked system binary that doesn't statically link OpenSSL) and asserts the emitted SBOM contains no spurious `pkg:generic/openssl` AND no `openssl:openssl` CPE strings.

```rust
//! Milestone 097 integration test — CPE candidate emission for
//! binary-extracted `pkg:generic/<lib>@<version>` components.
//!
//! Negative control: mikebom-itself scan should emit NO openssl CPE
//! (mikebom uses rustls, not OpenSSL). Positive coverage is via the
//! unit tests in `cpe.rs::tests` since toolchain-dependent OpenSSL
//! fixtures aren't reliably present on CI.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::process::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn mikebom_self_scan_emits_no_spurious_openssl_cpe() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("mikebom-under-test");
    std::fs::copy(env!("CARGO_BIN_EXE_mikebom"), &dest).unwrap();

    let out_file = dir.path().join("out.cdx.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mikebom"))
        .args(["sbom", "scan", "--path"])
        .arg(dir.path())
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .output()
        .unwrap();
    assert!(output.status.success());

    let sbom: Value = serde_json::from_slice(&std::fs::read(&out_file).unwrap()).unwrap();
    let json_str = serde_json::to_string(&sbom).unwrap();

    // SC-007-style guard: no spurious openssl CPE on mikebom-itself.
    assert!(
        !json_str.contains("cpe:2.3:a:openssl:openssl:"),
        "mikebom binary should not emit an openssl CPE — \
         milestone-096 SC-007 spurious-match bound violated"
    );
}
```

(Optional addition: a positive-control test gated on a `MIKEBOM_TEST_OPENSSL_FIXTURE` env var pointing at a known-OpenSSL-static binary. Out of scope for v1 — defer until a fixture-management policy emerges.)

## Validation rules

- **Mapping table sort order**: alphabetical by `library_slug`. A unit test (`#[test] fn mappings_alphabetically_sorted()`) walks the table and asserts `.windows(2).all(|w| w[0].0 < w[1].0)` to keep diffs friendly.
- **CPE syntax** (FR-006): by construction — the `format_cpe()` template + `cpe_escape()` produce valid CPE 2.3 formatted-string bindings. No external validator needed.
- **Mapping table completeness** (FR-002): a unit test compares the table's library_slugs against the union of `version_strings::CuratedLibrary::slug()` values and asserts the table covers every supported slug except the explicitly-omitted ones (currently just `boringssl`). Catches the "scanner-team-added-a-library-but-forgot-the-CPE-row" regression.

## Compatibility

- **No `Cargo.lock` change** — pure-Rust, in-source const table.
- **Goldens regen forecast** — zero. Existing ecosystem fixtures don't contain binary-extracted `pkg:generic/<v1-library>@<version>` components.
- **Backward compatibility** — 100% additive. Components that previously had no CPE for `pkg:generic/<known-lib>@<version>` now get one. Components for non-table libraries continue to emit no CPE (FR-003 silent-skip).
- **CDX `mikebom:cpe-candidates` property emission**: unchanged. When the mapping yields ≥2 candidates (curl, libressl), the existing emission path puts the full list in the property and `cpes[0]` in `component.cpe`.

## No JSON / no YAML schema additions

Zero new fields in any output schema. The new candidates flow through the existing `cpes` field on `ResolvedComponent`.
