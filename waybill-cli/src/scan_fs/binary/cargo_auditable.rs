//! cargo-auditable manifest extractor (milestone 029).
//!
//! `cargo auditable build` (https://github.com/rust-secure-code/cargo-auditable)
//! embeds the full build-time crate dependency closure as a
//! zlib-compressed JSON blob in a `.dep-v0` linker section. The
//! format is universal across binary formats:
//!
//! - **ELF**:  `.dep-v0` section in the section header table.
//! - **Mach-O**: `__DATA,.dep-v0` segment+section
//!   (`object::section_by_name_bytes(b".dep-v0")` matches by section
//!   name, ignoring the segment prefix — proven for the symmetric
//!   `__DATA,__cstring` reads in `scan.rs::collect_string_region`).
//! - **PE**: `.dep-v0` entry in the COFF section table.
//!
//! Cargo distributions in **Debian Trixie+, Fedora 40+, Alpine Edge,
//! and the official Rust container images** ship a pre-configured
//! Cargo wrapper so most Rust binaries built in those environments
//! get the embedded manifest automatically. Without extraction, those
//! binaries present to mikebom as opaque `pkg:generic/<filename>`
//! components even though the binary itself carries the answer to
//! "which crates are statically linked here".
//!
//! Wire format: zlib-compressed JSON. Schema (subset we deserialize):
//!
//! ```text
//! {
//!   "packages": [
//!     {
//!       "name":          "<crate>",
//!       "version":       "<semver>",
//!       "source":        "registry"|"git"|"local"|"path"|"unknown",
//!       "kind":          "runtime"|"build"|"dev"  (optional, older
//!                                                  format versions
//!                                                  don't carry it),
//!       "dependencies":  [<idx>, ...]              (indices into the
//!                                                  same `packages[]`
//!                                                  array),
//!       "root":          true|false                (exactly one entry
//!                                                  has root=true)
//!     },
//!     ...
//!   ]
//! }
//! ```
//!
//! Only `packages`, `name`, `version`, `source` are required by this
//! deserializer. `kind` / `dependencies` / `root` are optional with
//! `#[serde(default)]` to handle older format versions and forward-
//! compat. Unknown fields are silently ignored.
//!
//! `parse_dep_v0` returns `Option<CargoAuditableManifest>` — `None`
//! on any failure (malformed zlib, malformed JSON, schema mismatch).
//! No panics.

use std::io::Read;

/// The full extracted cargo-auditable manifest.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct CargoAuditableManifest {
    pub packages: Vec<CargoAuditablePackage>,
}

/// One crate entry from the manifest's `packages[]` array.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct CargoAuditablePackage {
    pub name: String,
    pub version: String,
    /// `"registry"` | `"git"` | `"local"` | `"path"` | `"unknown"`.
    /// cargo-auditable's own enum is not pinned across versions —
    /// we keep this as a free String and map per-source-shape on
    /// the consumer side (PURL minting in `binary/entry.rs`).
    pub source: String,
    /// `"runtime"` | `"build"` | `"dev"`. Optional — older
    /// format versions of cargo-auditable did not emit this field.
    #[serde(default)]
    pub kind: Option<String>,
    /// Indices into the same `packages[]` array. Empty for leaf
    /// crates. cargo's dep graph is a DAG by construction, so no
    /// cycle handling is needed.
    #[serde(default)]
    pub dependencies: Vec<usize>,
    /// Exactly one entry has `root: true` — the crate the binary's
    /// `main()` came from.
    #[serde(default)]
    pub root: bool,
}

/// Decompress and parse a `.dep-v0` section's bytes into a typed
/// manifest. Returns `None` on any failure (malformed zlib,
/// malformed JSON, schema mismatch). Never panics.
///
/// Uses `flate2::read::ZlibDecoder` — no new crate dependencies; the
/// `flate2 = "1"` dep is already pulled in via tar handling in the
/// container-image scanner.
pub fn parse_dep_v0(bytes: &[u8]) -> Option<CargoAuditableManifest> {
    let mut decoder = flate2::read::ZlibDecoder::new(bytes);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).ok()?;
    serde_json::from_slice(&decompressed).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;

    /// Round-trip helper: encode a JSON string as a zlib-compressed
    /// blob — the same wire format cargo-auditable produces.
    fn zlib_compress(payload: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::ZlibEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        );
        encoder.write_all(payload).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn parse_dep_v0_round_trips_synthetic_manifest() {
        let json = br#"{
            "packages": [
                {
                    "name": "myapp",
                    "version": "1.2.3",
                    "source": "local",
                    "kind": "runtime",
                    "dependencies": [1, 2],
                    "root": true
                },
                {
                    "name": "serde",
                    "version": "1.0.193",
                    "source": "registry",
                    "kind": "runtime",
                    "dependencies": [],
                    "root": false
                },
                {
                    "name": "tokio",
                    "version": "1.35.1",
                    "source": "registry",
                    "kind": "runtime",
                    "dependencies": [],
                    "root": false
                }
            ]
        }"#;
        let compressed = zlib_compress(json);
        let manifest = parse_dep_v0(&compressed).expect("must parse");
        assert_eq!(manifest.packages.len(), 3);
        assert_eq!(manifest.packages[0].name, "myapp");
        assert_eq!(manifest.packages[0].version, "1.2.3");
        assert_eq!(manifest.packages[0].source, "local");
        assert_eq!(manifest.packages[0].kind.as_deref(), Some("runtime"));
        assert_eq!(manifest.packages[0].dependencies, vec![1, 2]);
        assert!(manifest.packages[0].root);
        assert_eq!(manifest.packages[1].name, "serde");
        assert!(!manifest.packages[1].root);
        assert!(manifest.packages[1].dependencies.is_empty());
    }

    #[test]
    fn parse_dep_v0_returns_none_for_corrupt_zlib() {
        // Bytes that aren't a valid zlib header.
        let garbage = b"this is not a zlib stream at all";
        assert_eq!(parse_dep_v0(garbage), None);
    }

    #[test]
    fn parse_dep_v0_returns_none_for_invalid_json() {
        // Valid zlib stream wrapping non-JSON bytes.
        let compressed = zlib_compress(b"this is not JSON");
        assert_eq!(parse_dep_v0(&compressed), None);
    }

    #[test]
    fn parse_dep_v0_returns_none_for_missing_required_field() {
        // Package entry without `name` should fail deserialization.
        let json = br#"{
            "packages": [
                {
                    "version": "1.0.0",
                    "source": "registry"
                }
            ]
        }"#;
        let compressed = zlib_compress(json);
        assert_eq!(parse_dep_v0(&compressed), None);
    }

    #[test]
    fn parse_dep_v0_handles_optional_kind_field() {
        // Older cargo-auditable format — entries without `kind`
        // should still deserialize successfully.
        let json = br#"{
            "packages": [
                {
                    "name": "myapp",
                    "version": "1.0.0",
                    "source": "local",
                    "root": true
                }
            ]
        }"#;
        let compressed = zlib_compress(json);
        let manifest = parse_dep_v0(&compressed).expect("optional kind must parse");
        assert_eq!(manifest.packages.len(), 1);
        assert_eq!(manifest.packages[0].name, "myapp");
        assert_eq!(manifest.packages[0].kind, None);
        // dependencies + root default-impl should also work.
        assert!(manifest.packages[0].dependencies.is_empty());
        assert!(manifest.packages[0].root);
    }

    #[test]
    fn parse_dep_v0_handles_unknown_fields_gracefully() {
        // Forward-compat: a future cargo-auditable version may add
        // fields. serde_json's default behavior is to ignore
        // unknown fields, so deserialization should succeed.
        let json = br#"{
            "packages": [
                {
                    "name": "myapp",
                    "version": "1.0.0",
                    "source": "local",
                    "future_field_we_do_not_know_about": 42
                }
            ],
            "format_version": "v999",
            "another_unknown_top_level_field": [1, 2, 3]
        }"#;
        let compressed = zlib_compress(json);
        let manifest = parse_dep_v0(&compressed).expect("forward-compat must parse");
        assert_eq!(manifest.packages.len(), 1);
        assert_eq!(manifest.packages[0].name, "myapp");
    }

}
