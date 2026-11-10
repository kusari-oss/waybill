//! Milestone 1378 (#1396) — the modules compiled into the binaries under one
//! path, and nothing else.
//!
//! The nix closure tier asks this of each closure member's output. It is
//! deliberately **not** a scan of that output. Measured on `rustc-1.95.0`, a
//! scan returns 709 crates and not one of them comes from a binary: they are
//! `Cargo.lock` files the compiler ships as source. A caller that wants to
//! know what was *linked into* a program must not be handed every lockfile a
//! build tool happens to carry.
//!
//! So this runs exactly two readers — Go BuildInfo and cargo-auditable — by
//! calling the functions the standalone binary scan calls. That is the whole
//! reason it lives here: two paths that share the recognition code cannot
//! disagree about what a binary contains.
//!
//! A binary's description of *itself* (the Go main module, the cargo root
//! crate) is returned separately and never as a module. The caller already
//! has a component for the program; emitting the self-description would give
//! the document two.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use waybill_common::types::purl::Purl;

use crate::scan_fs::package_db::exclude_path::ExclusionSet;
use crate::scan_fs::package_db::{go_binary, PackageDbEntry};

/// Which ecosystem a binary's self-description belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SelfEcosystem {
    Golang,
    Cargo,
}

/// What a binary says about its own module or crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelfDescription {
    pub(crate) ecosystem: SelfEcosystem,
    /// Go module path or crate name.
    pub(crate) name: String,
    /// `None` when the binary does not know: Go writes `(devel)` for a build
    /// outside version control, which is every nixpkgs build.
    pub(crate) version: Option<String>,
    pub(crate) binary: PathBuf,
}

/// What the two readers found under one path.
#[derive(Debug, Default)]
pub(crate) struct EmbeddedContents {
    /// Go dependency and `stdlib` entries and non-root crates, as the
    /// standalone readers build them, deduplicated by PURL across binaries.
    pub(crate) modules: Vec<PackageDbEntry>,
    pub(crate) self_descriptions: Vec<SelfDescription>,
    pub(crate) binaries_examined: usize,
    /// A discovered binary could not be read. Distinct from "read and found
    /// nothing", which is the ordinary case and not a failure.
    pub(crate) failed: bool,
}

/// Read the embedded build information of every binary under `path`.
///
/// `path` may be a directory or a single file: a nix output is either.
pub(crate) fn embedded_contents(path: &Path) -> EmbeddedContents {
    let mut out = EmbeddedContents::default();
    let mut seen: HashSet<String> = HashSet::new();
    let binaries = super::discover::discover_binaries(
        path,
        &HashSet::new(),
        &ExclusionSet::new_empty(),
    );

    for binary in binaries {
        out.binaries_examined += 1;

        // One read, both readers.
        let bytes = match std::fs::read(&binary) {
            Ok(b) => b,
            Err(e) => {
                tracing::debug!(path = %binary.display(), error = %e, "embedded: unreadable binary");
                out.failed = true;
                continue;
            }
        };

        // Go. A non-Go file is reported as such rather than as an error.
        if let (go_binary::BuildInfoStatus::Ok, Some(info)) =
            go_binary::read_binary_bytes(&binary, &bytes)
        {
            if let Some((name, version, _)) = &info.main_module {
                out.self_descriptions.push(SelfDescription {
                    ecosystem: SelfEcosystem::Golang,
                    name: name.clone(),
                    version: known_go_version(version),
                    binary: binary.clone(),
                });
            }
            for entry in go_binary::dependency_entries(&info) {
                if seen.insert(entry.purl.as_str().to_string()) {
                    out.modules.push(entry);
                }
            }
        }

        // cargo-auditable: one section lookup. A universal Mach-O needs its
        // slices walked, which only the full scanner does, so that shape —
        // rare, and never produced by nixpkgs for a single platform — takes
        // the scanner's path and gets the scanner's answer.
        let manifest = if super::scan::has_fat_macho_magic(&bytes) {
            super::scan::scan_binary(&binary, &bytes).and_then(|s| s.cargo_auditable)
        } else {
            super::scan::dep_v0_manifest(&bytes)
        };
        let Some(manifest) = manifest else {
            continue;
        };
        if let Some(root) = manifest.packages.iter().find(|p| p.root) {
            out.self_descriptions.push(SelfDescription {
                ecosystem: SelfEcosystem::Cargo,
                name: root.name.clone(),
                version: (!root.version.is_empty()).then(|| root.version.clone()),
                binary: binary.clone(),
            });
        }
        for entry in crate_entries(&manifest, &binary) {
            if seen.insert(entry.purl.as_str().to_string()) {
                out.modules.push(entry);
            }
        }
    }
    out
}

/// A Go main-module version, or `None` when BuildInfo did not record one.
fn known_go_version(version: &str) -> Option<String> {
    let v = version.trim();
    (!v.is_empty() && v != "(devel)").then(|| v.to_string())
}

/// The crates linked into a binary, without the binary's own root crate.
///
/// Built by the standalone reader's own function over a copy of the manifest
/// with the root removed and the dependency indices cleared. The indices are
/// cleared because removing an element shifts them, and because crate-to-crate
/// edges are out of this feature's scope — the standalone path drops all of
/// them today (85 of 85 on ripgrep), and reproducing half of a graph here
/// would be a second, different, answer to the same question.
fn crate_entries(
    manifest: &super::cargo_auditable::CargoAuditableManifest,
    binary: &Path,
) -> Vec<PackageDbEntry> {
    let mut without_root = manifest.clone();
    without_root.packages.retain(|p| !p.root);
    for p in &mut without_root.packages {
        p.dependencies.clear();
    }
    // The entry builder wants the file-level PURL to nest crates under. These
    // are not nested (FR-007), so the value is a placeholder that is cleared
    // straight away.
    let Ok(placeholder) = Purl::new("pkg:generic/waybill-embedded-placeholder") else {
        return Vec::new();
    };
    let mut entries =
        super::entry::cargo_auditable_packages_to_entries(&without_root, &placeholder, binary);
    for e in &mut entries {
        e.parent_purl = None;
        e.depends.clear();
    }
    entries
}

/// Test fixtures: the smallest files the two readers accept.
///
/// Synthetic by policy — module and crate names are `waybill-fixture-*` or
/// `example.test/*`, never real coordinates.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::io::Write;

    fn uvarint(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
        out
    }

    /// A file the Go reader accepts: ELF magic, padding past the size floor,
    /// then an inline BuildInfo blob the magic-prefix scan finds.
    pub(crate) fn go_binary(main: &str, main_version: &str, deps: &[(&str, &str)]) -> Vec<u8> {
        let mut mod_info = format!("path\t{main}/cmd\nmod\t{main}\t{main_version}\t\n");
        for (name, version) in deps {
            mod_info.push_str(&format!("dep\t{name}\t{version}\th1:fixture=\n"));
        }
        let mut v = vec![0u8; 2048];
        v[..4].copy_from_slice(b"\x7fELF");
        v.extend_from_slice(b"\xff Go buildinf:");
        v.push(8);
        v.push(0x2);
        v.extend_from_slice(&[0u8; 16]);
        let build = b"go1.22.1";
        v.extend(uvarint(build.len() as u64));
        v.extend_from_slice(build);
        v.extend(uvarint(mod_info.len() as u64));
        v.extend_from_slice(mod_info.as_bytes());
        v
    }

    /// A minimal ELF64 with one named section holding `data`.
    pub(crate) fn elf_with_section(name: &str, data: &[u8]) -> Vec<u8> {
        let mut shstrtab = vec![0u8];
        let name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(name.as_bytes());
        shstrtab.push(0);
        let shstr_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".shstrtab\0");

        // Pad the payload area so the file clears the 1 KiB size floor.
        let data_off = 64u64;
        let pad = 2048usize.saturating_sub(data.len());
        let strtab_off = data_off + data.len() as u64 + pad as u64;
        let shoff = strtab_off + shstrtab.len() as u64;

        let mut f = Vec::new();
        f.extend_from_slice(b"\x7fELF");
        f.extend_from_slice(&[2, 1, 1, 0]); // 64-bit, little-endian, v1, SysV
        f.extend_from_slice(&[0u8; 8]);
        f.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        f.extend_from_slice(&62u16.to_le_bytes()); // x86-64
        f.extend_from_slice(&1u32.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes()); // entry
        f.extend_from_slice(&0u64.to_le_bytes()); // phoff
        f.extend_from_slice(&shoff.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes()); // flags
        f.extend_from_slice(&64u16.to_le_bytes()); // ehsize
        f.extend_from_slice(&0u16.to_le_bytes()); // phentsize
        f.extend_from_slice(&0u16.to_le_bytes()); // phnum
        f.extend_from_slice(&64u16.to_le_bytes()); // shentsize
        f.extend_from_slice(&3u16.to_le_bytes()); // shnum
        f.extend_from_slice(&2u16.to_le_bytes()); // shstrndx
        debug_assert_eq!(f.len(), 64);
        f.extend_from_slice(data);
        f.extend(std::iter::repeat_n(0u8, pad));
        f.extend_from_slice(&shstrtab);

        let mut sh = |name: u32, ty: u32, off: u64, size: u64| {
            f.extend_from_slice(&name.to_le_bytes());
            f.extend_from_slice(&ty.to_le_bytes());
            f.extend_from_slice(&0u64.to_le_bytes()); // flags
            f.extend_from_slice(&0u64.to_le_bytes()); // addr
            f.extend_from_slice(&off.to_le_bytes());
            f.extend_from_slice(&size.to_le_bytes());
            f.extend_from_slice(&0u32.to_le_bytes()); // link
            f.extend_from_slice(&0u32.to_le_bytes()); // info
            f.extend_from_slice(&1u64.to_le_bytes()); // addralign
            f.extend_from_slice(&0u64.to_le_bytes()); // entsize
        };
        sh(0, 0, 0, 0);
        sh(name_off, 1, data_off, data.len() as u64); // SHT_PROGBITS
        sh(shstr_off, 3, strtab_off, shstrtab.len() as u64); // SHT_STRTAB
        f
    }

    /// A file the cargo-auditable reader accepts.
    ///
    /// `crates` is `(name, version, source, kind)`; the first is the root.
    pub(crate) fn cargo_binary(crates: &[(&str, &str, &str, &str)]) -> Vec<u8> {
        let packages: Vec<serde_json::Value> = crates
            .iter()
            .enumerate()
            .map(|(i, (name, version, source, kind))| {
                let mut p = serde_json::json!({
                    "name": name, "version": version, "source": source, "kind": kind,
                });
                if i == 0 {
                    p["root"] = serde_json::Value::Bool(true);
                    p["dependencies"] = serde_json::json!((1..crates.len()).collect::<Vec<_>>());
                }
                p
            })
            .collect();
        let json = serde_json::json!({ "packages": packages }).to_string();
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        // Writing to a `Vec` cannot fail; the fixture is empty if it somehow
        // does, and the test that uses it then fails on its own assertions.
        let _ = enc.write_all(json.as_bytes());
        let section = enc.finish().unwrap_or_default();
        elf_with_section(".dep-v0", &section)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn purls(c: &EmbeddedContents) -> Vec<String> {
        let mut v: Vec<String> = c.modules.iter().map(|e| e.purl.as_str().to_string()).collect();
        v.sort();
        v
    }

    #[test]
    fn a_directory_with_no_binaries_yields_nothing_and_is_not_a_failure() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("README"), "text").unwrap();
        let c = embedded_contents(d.path());
        assert_eq!(c.binaries_examined, 0);
        assert!(c.modules.is_empty());
        assert!(c.self_descriptions.is_empty());
        assert!(!c.failed);
    }

    #[test]
    fn a_go_binary_yields_its_dependencies_and_describes_itself_separately() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("bin")).unwrap();
        std::fs::write(
            d.path().join("bin/app"),
            fixtures::go_binary(
                "example.test/waybill-fixture-app",
                "(devel)",
                &[
                    ("example.test/waybill-fixture-liba", "v1.2.3"),
                    ("example.test/waybill-fixture-libb", "v0.4.0"),
                ],
            ),
        )
        .unwrap();

        let c = embedded_contents(d.path());
        assert_eq!(c.binaries_examined, 1);
        assert_eq!(
            purls(&c),
            vec![
                "pkg:golang/example.test/waybill-fixture-liba@v1.2.3",
                "pkg:golang/example.test/waybill-fixture-libb@v0.4.0",
                "pkg:golang/stdlib@v1.22.1",
            ],
            "the two dependencies and the standard library, and not the main module"
        );
        assert_eq!(
            c.self_descriptions,
            vec![SelfDescription {
                ecosystem: SelfEcosystem::Golang,
                name: "example.test/waybill-fixture-app".to_string(),
                version: None,
                binary: d.path().join("bin/app"),
            }],
            "`(devel)` is not a version"
        );
        assert!(c.modules.iter().all(|e| e.parent_purl.is_none()));
    }

    #[test]
    fn a_go_binary_that_knows_its_version_reports_it() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("app"),
            fixtures::go_binary("example.test/waybill-fixture-app", "v9.9.9", &[]),
        )
        .unwrap();
        let c = embedded_contents(d.path());
        assert_eq!(c.self_descriptions[0].version.as_deref(), Some("v9.9.9"));
    }

    #[test]
    fn a_cargo_auditable_binary_yields_its_crates_flat_and_without_its_root() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("app"),
            fixtures::cargo_binary(&[
                ("waybill-fixture-app", "1.2.3", "local", "runtime"),
                ("waybill-fixture-dep", "0.5.0", "crates.io", "runtime"),
                ("waybill-fixture-build", "2.0.0", "crates.io", "build"),
            ]),
        )
        .unwrap();

        let c = embedded_contents(d.path());
        assert_eq!(c.binaries_examined, 1, "the synthetic ELF must be discovered");
        assert_eq!(
            purls(&c),
            vec![
                "pkg:cargo/waybill-fixture-build@2.0.0?source=crates.io",
                "pkg:cargo/waybill-fixture-dep@0.5.0?source=crates.io",
            ],
            "the root crate is the program itself, not one of its dependencies"
        );
        assert_eq!(
            c.self_descriptions,
            vec![SelfDescription {
                ecosystem: SelfEcosystem::Cargo,
                name: "waybill-fixture-app".to_string(),
                version: Some("1.2.3".to_string()),
                binary: d.path().join("app"),
            }]
        );
        assert!(
            c.modules.iter().all(|e| e.parent_purl.is_none() && e.depends.is_empty()),
            "flat: no nesting under the binary and no half-graph"
        );
        assert!(c
            .modules
            .iter()
            .all(|e| e.evidence_kind.as_deref() == Some("cargo-auditable")));
    }

    #[test]
    fn a_single_file_path_is_read() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("standalone");
        std::fs::write(
            &f,
            fixtures::go_binary(
                "example.test/waybill-fixture-app",
                "(devel)",
                &[("example.test/waybill-fixture-liba", "v1.2.3")],
            ),
        )
        .unwrap();
        let c = embedded_contents(&f);
        assert_eq!(c.binaries_examined, 1);
        assert_eq!(c.modules.len(), 2);
    }

    /// The reason this module exists rather than a call to the full scan.
    #[test]
    fn a_lockfile_beside_no_binary_yields_nothing() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("Cargo.lock"),
            "version = 3\n\n[[package]]\nname = \"waybill-fixture-locked\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        std::fs::write(d.path().join("go.sum"), "example.test/x v1.0.0 h1:abc=\n").unwrap();
        let c = embedded_contents(d.path());
        assert!(c.modules.is_empty(), "lockfiles are not embedded build information");
    }

    #[test]
    fn a_module_in_two_binaries_is_one_entry() {
        let d = tempfile::tempdir().unwrap();
        for name in ["one", "two"] {
            std::fs::write(
                d.path().join(name),
                fixtures::go_binary(
                    &format!("example.test/waybill-fixture-{name}"),
                    "(devel)",
                    &[("example.test/waybill-fixture-shared", "v1.0.0")],
                ),
            )
            .unwrap();
        }
        let c = embedded_contents(d.path());
        assert_eq!(c.binaries_examined, 2);
        assert_eq!(c.modules.len(), 2, "shared dependency + stdlib, each once");
        assert_eq!(c.self_descriptions.len(), 2);
    }
}
