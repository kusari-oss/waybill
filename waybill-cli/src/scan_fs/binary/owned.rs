//! Spec 1536 (#1567): native binaries a Python package owns.
//!
//! A `.dist-info/RECORD` says which files a package installed. Until this
//! spec that claim also removed the files from binary analysis, which is
//! right for an OS package (a dpkg-owned `/usr/bin/curl` *is* the curl
//! package) and wrong for a wheel, which routinely vendors third-party native
//! code. The binary scan now analyses a file that only a `RECORD` claims, and
//! reports one [`OwnedBinary`] for it. [`attach`] then places the results:
//!
//! - **containment**: package → binary → what the binary identifies;
//! - **edges**: package → binary; binary → its linked libraries, what it
//!   identifies, and the package's other binaries it links.
//!
//! Ownership by an OS package database, or by an `.ipk` archive, still wins:
//! such a binary never reaches here (FR-005, FR-007).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::scan_fs::package_db::PackageDbEntry;

/// C230: the document-scope count of owned binaries.
pub const ANN_PYTHON_OWNED_BINARIES: &str = "waybill:python-owned-binaries";

/// One binary the binary scan analysed because only a `RECORD` claims it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedBinary {
    /// The binary's path, as the walker saw it.
    pub path: PathBuf,
    /// The `.dist-info` whose `RECORD` lists it.
    pub owner: PathBuf,
    /// Its file-level component's PURL.
    pub file_purl: String,
    /// PURLs of the linkage components its external imports became.
    pub links: BTreeSet<String>,
    /// File names of the same package's binaries it links. They are not
    /// external dependencies (FR-004).
    pub internal_links: BTreeSet<String>,
    /// PURLs of the components the scan identified from its content and
    /// nested under it: version strings, symbol fingerprints, ELF notes,
    /// bundled source trees, cargo-auditable crates.
    pub children: BTreeSet<String>,
}

/// C230 `waybill:python-owned-binaries`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OwnedBinaryCoverage {
    /// Owned binaries analysed and nested under a resolved owner.
    pub analysed: usize,
    /// Of those, the ones with at least one content-derived identification.
    /// Linkage alone does not count (spec clarification 4).
    pub identified: usize,
}

impl OwnedBinaryCoverage {
    /// The wire value: a JSON object with sorted keys, as a string.
    pub fn wire(&self) -> String {
        serde_json::json!({ "analysed": self.analysed, "identified": self.identified }).to_string()
    }
}

/// Place each owned binary under its package, its identifications under it,
/// and its edges (data-model "The attach pass").
///
/// The owner's PURL is that of the pip component read from
/// `<dist-info>/METADATA`, never rebuilt from the directory name. A binary
/// whose owner produced no component is left as the scan reported it, and is
/// not counted (V2). Returns `None` when nothing was attached.
pub fn attach(entries: &mut [PackageDbEntry], owned: &[OwnedBinary]) -> Option<OwnedBinaryCoverage> {
    if owned.is_empty() {
        return None;
    }
    let owner_purl: BTreeMap<PathBuf, String> = entries
        .iter()
        .filter(|e| e.purl.ecosystem() == "pypi" && e.source_path.ends_with("METADATA"))
        .filter_map(|e| Some((Path::new(&e.source_path).parent()?.to_path_buf(), e.purl.as_str().to_string())))
        .collect();

    // A binary's file name → its PURL, per owner, for internal links.
    let mut by_owner_and_name: BTreeMap<(&Path, &str), &str> = BTreeMap::new();
    for b in owned {
        if let Some(name) = b.path.file_name().and_then(|n| n.to_str()) {
            by_owner_and_name.entry((b.owner.as_path(), name)).or_insert(b.file_purl.as_str());
        }
    }

    let mut coverage = OwnedBinaryCoverage::default();
    let mut package_depends: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for b in owned {
        let Some(owner) = owner_purl.get(&b.owner) else {
            continue;
        };
        let path = b.path.to_string_lossy();
        // Go build-info modules come from another reader, keyed by path.
        let mut children = b.children.clone();
        for e in entries.iter_mut() {
            if e.purl.ecosystem() == "golang" && e.source_path == path && e.parent_purl.is_none() {
                e.parent_purl = Some(b.file_purl.clone());
                children.insert(e.purl.as_str().to_string());
            }
        }
        let mut depends: BTreeSet<String> = b.links.clone();
        depends.extend(children.iter().cloned());
        for name in &b.internal_links {
            if let Some(target) = by_owner_and_name.get(&(b.owner.as_path(), name.as_str())) {
                if *target != b.file_purl {
                    depends.insert((*target).to_string());
                }
            }
        }
        if let Some(file) = entries
            .iter_mut()
            .find(|e| e.purl.as_str() == b.file_purl && e.source_path == path && e.parent_purl.is_none())
        {
            file.parent_purl = Some(owner.clone());
            for d in depends {
                if !file.depends.contains(&d) {
                    file.depends.push(d);
                }
            }
        } else {
            continue;
        }
        package_depends.entry(owner.clone()).or_default().insert(b.file_purl.clone());
        coverage.analysed += 1;
        if !children.is_empty() {
            coverage.identified += 1;
        }
    }
    for e in entries.iter_mut() {
        if let Some(bins) = package_depends.get(e.purl.as_str()) {
            if e.source_path.ends_with("METADATA") {
                for p in bins {
                    if !e.depends.contains(p) {
                        e.depends.push(p.clone());
                    }
                }
            }
        }
    }
    (coverage.analysed > 0).then_some(coverage)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::types::purl::Purl;

    fn entry(purl: &str, source_path: &str) -> PackageDbEntry {
        PackageDbEntry {
            extra_source_paths: Vec::new(),
            depends_ecosystem: None,
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: String::new(),
            version: String::new(),
            arch: None,
            source_path: source_path.to_string(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            raw_version: None,
            parent_purl: None,
            npm_role: None,
            co_owned_by: None,
            hashes: Vec::new(),
            sbom_tier: None,
            shade_relocation: None,
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    const PKG: &str = "pkg:pypi/waybill-fixture-pkg@1.2.3";
    const BIN: &str = "pkg:generic/libz.dylib?file-sha256=aa";
    const BIN2: &str = "pkg:generic/libother.dylib?file-sha256=bb";

    fn owned(path: &str, purl: &str) -> OwnedBinary {
        OwnedBinary {
            path: PathBuf::from(path),
            owner: PathBuf::from("/r/p-1.2.3.dist-info"),
            file_purl: purl.to_string(),
            links: BTreeSet::new(),
            internal_links: BTreeSet::new(),
            children: BTreeSet::new(),
        }
    }

    fn base() -> Vec<PackageDbEntry> {
        vec![
            entry(PKG, "/r/p-1.2.3.dist-info/METADATA"),
            entry(BIN, "/r/p/libz.dylib"),
            entry(BIN2, "/r/p/libother.dylib"),
            entry("pkg:golang/example.com/waybill-fixture-mod@v1.0.0", "/r/p/libz.dylib"),
            entry("pkg:generic/zlib@1.3.1", "/r/elsewhere/libq.so"),
        ]
    }

    #[test]
    fn nests_the_binary_under_its_package_and_builds_the_edge_chain() {
        let mut es = base();
        let mut b = owned("/r/p/libz.dylib", BIN);
        b.links.insert("pkg:generic/libc.so.6".into());
        b.internal_links.insert("libother.dylib".into());
        let cov = attach(&mut es, &[b, owned("/r/p/libother.dylib", BIN2)]).unwrap();
        assert_eq!(es[1].parent_purl.as_deref(), Some(PKG));
        assert_eq!(es[2].parent_purl.as_deref(), Some(PKG));
        assert!(es[0].depends.contains(&BIN.to_string()) && es[0].depends.contains(&BIN2.to_string()));
        assert!(es[1].depends.contains(&"pkg:generic/libc.so.6".to_string()));
        assert!(es[1].depends.contains(&BIN2.to_string()), "internal link is an edge to the sibling");
        assert_eq!(cov, OwnedBinaryCoverage { analysed: 2, identified: 1 });
    }

    /// Go build-info modules come from `go_binary`, keyed by the binary's
    /// path; they nest under the binary and count as identification.
    #[test]
    fn nests_go_modules_from_the_same_path_and_leaves_other_paths_alone() {
        let mut es = base();
        attach(&mut es, &[owned("/r/p/libz.dylib", BIN)]).unwrap();
        assert_eq!(es[3].parent_purl.as_deref(), Some(BIN));
        assert!(es[1].depends.iter().any(|d| d.starts_with("pkg:golang/")));
        assert_eq!(es[4].parent_purl, None, "an entry from another path is untouched");
    }

    #[test]
    fn a_child_recorded_by_the_scan_counts_and_becomes_an_edge() {
        let mut es = base();
        es.remove(3);
        let mut b = owned("/r/p/libz.dylib", BIN);
        b.children.insert("pkg:generic/openssl@3.0.0".into());
        let cov = attach(&mut es, &[b]).unwrap();
        assert!(es[1].depends.contains(&"pkg:generic/openssl@3.0.0".to_string()));
        assert_eq!(cov, OwnedBinaryCoverage { analysed: 1, identified: 1 });
    }

    #[test]
    fn linkage_alone_is_not_an_identification() {
        let mut es = base();
        es.remove(3);
        let mut b = owned("/r/p/libz.dylib", BIN);
        b.links.insert("pkg:generic/libc.so.6".into());
        assert_eq!(attach(&mut es, &[b]).unwrap(), OwnedBinaryCoverage { analysed: 1, identified: 0 });
    }

    /// V2: no pip component for the owner → nothing is nested or counted.
    #[test]
    fn an_owner_without_a_component_attaches_nothing() {
        let mut es = base();
        es.remove(0);
        assert_eq!(attach(&mut es, &[owned("/r/p/libz.dylib", BIN)]), None);
        assert!(es.iter().all(|e| e.parent_purl.is_none()));
    }

    #[test]
    fn wire_value_has_sorted_keys() {
        assert_eq!(OwnedBinaryCoverage { analysed: 3, identified: 1 }.wire(), r#"{"analysed":3,"identified":1}"#);
    }
}
