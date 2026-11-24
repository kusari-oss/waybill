//! Spec 1653 (#1688) — what each GHC release's boot libraries depend on.
//!
//! A closure document's boot libraries (C218) were emitted as leaves: `base`
//! had no edges on any of the five closure goldens, and shellcheck read
//! `complete`. The dependencies are a property of the compiler release, as
//! milestone 966's versions are, so this table is keyed the same way: exact
//! release `X.Y.Z`, then library.
//!
//! The data is `ghc_boot_dependencies.tsv`, produced by
//! `scripts/ghc-boot-dependencies.py` from each release's own x86_64 Linux
//! bindist: `depends:` in `package.conf.d` (9.4 and later) or `_DEP_NAMES` in
//! `package-data.mk` (9.0, 9.2). It is embedded at build time; a scan never
//! reads a package database, from the network or from the store, so the same
//! closure gets the same edges on any machine (spec Clarification 1).
//!
//! Measured before it was committed (`specs/1653-boot-library-edges/
//! measurements/`): the bindists' dependencies equal nixpkgs' installed
//! package databases in 5 of 5 releases checked, including every compiler
//! output present in CI; and every release's libraries and versions equal
//! milestone 966's. Measuring also corrected the issue's premise: 9.10's
//! `base` depends on `ghc-internal` and `ghc-prim`, not `ghc-bignum`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The committed table.
const TSV: &str = include_str!("ghc_boot_dependencies.tsv");

/// One `(release, library)` row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DependencyRow {
    /// Empty only for `rts` in a make-built release, whose version the
    /// bindist does not record.
    version: String,
    /// Package names, sorted. Empty for a known leaf.
    depends: Vec<String>,
}

/// The parsed table.
#[derive(Debug, Default)]
pub(crate) struct BootDependencyTable {
    observed: String,
    rows: BTreeMap<String, BTreeMap<String, DependencyRow>>,
    /// Lines that did not parse, and `(release, library)` pairs seen twice.
    /// Skipped, never a panic; the unit tests assert both are 0 for the
    /// embedded file.
    malformed: usize,
    duplicates: usize,
}

/// The embedded table, parsed once per process.
pub(crate) fn table() -> &'static BootDependencyTable {
    static TABLE: OnceLock<BootDependencyTable> = OnceLock::new();
    TABLE.get_or_init(|| parse(TSV))
}

fn is_dotted_digits(v: &str) -> bool {
    !v.is_empty() && v.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn parse(text: &str) -> BootDependencyTable {
    let mut t = BootDependencyTable::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# observed:") {
            t.observed = rest.trim().to_string();
            continue;
        }
        if line.starts_with('#') || line.is_empty() || line == "ghc\tlibrary\tversion\tdepends\tlayout" {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let parsed = match fields.as_slice() {
            [ghc, lib, version, depends, layout]
                if is_dotted_digits(ghc)
                    && !lib.is_empty()
                    && (version.is_empty() || is_dotted_digits(version))
                    && matches!(*layout, "package.conf.d" | "package-data.mk")
                    && !depends.is_empty() =>
            {
                let depends: Vec<String> = if *depends == "-" {
                    Vec::new()
                } else {
                    depends.split(' ').map(str::to_string).collect()
                };
                Some((*ghc, *lib, DependencyRow { version: (*version).to_string(), depends }))
            }
            _ => None,
        };
        let Some((ghc, lib, row)) = parsed else {
            t.malformed += 1;
            continue;
        };
        let libs = t.rows.entry(ghc.to_string()).or_default();
        if libs.contains_key(lib) {
            t.duplicates += 1;
            continue;
        }
        libs.insert(lib.to_string(), row);
    }
    t
}

impl BootDependencyTable {
    /// The date the bindists were read.
    pub(crate) fn observed(&self) -> &str {
        &self.observed
    }

    /// Whether the table has rows for this release.
    pub(crate) fn has_release(&self, release: &str) -> bool {
        self.rows.contains_key(release)
    }

    /// What `library` depends on in `release`: `None` when the release or the
    /// library is absent, `Some(&[])` for a known leaf (`rts`).
    pub(crate) fn depends(&self, release: &str, library: &str) -> Option<&[String]> {
        self.rows.get(release)?.get(library).map(|r| r.depends.as_slice())
    }

    #[cfg(test)]
    fn releases(&self) -> std::collections::BTreeSet<&str> {
        self.rows.keys().map(String::as_str).collect()
    }

    #[cfg(test)]
    fn libraries(&self, release: &str) -> std::collections::BTreeSet<&str> {
        self.rows.get(release).map(|l| l.keys().map(String::as_str).collect()).unwrap_or_default()
    }

    #[cfg(test)]
    fn version(&self, release: &str, library: &str) -> Option<&str> {
        self.rows.get(release)?.get(library).map(|r| r.version.as_str())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use crate::scan_fs::package_db::nix::haskell_packages::boot_table;

    const LINUX: &str = "x86_64-linux";

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_embedded_table_parses_cleanly() {
        let t = table();
        assert_eq!(t.malformed, 0, "every row parses");
        assert_eq!(t.duplicates, 0, "no (release, library) twice");
        assert!(!t.observed().is_empty(), "the header dates the observation");
        assert_eq!(t.releases().len(), 36, "measurements/baseline.md");
        let rows: usize = t.rows.values().map(BTreeMap::len).sum();
        assert_eq!(rows, 1_371, "measurements/baseline.md");
    }

    /// SC-002b: the two tables cover exactly the same releases, in both
    /// directions. A release added to one table and not the other fails here.
    #[test]
    fn the_releases_are_milestone_966s() {
        assert_eq!(table().releases(), boot_table::table().releases());
    }

    /// Each release ships the libraries milestone 966 lists for Linux, at
    /// the same versions. A disagreement is settled when the tables are
    /// regenerated, never at scan time (spec Edge Cases).
    #[test]
    fn libraries_and_versions_equal_milestone_966s() {
        let t = table();
        for release in t.releases() {
            let m966 = boot_table::table().ships(release, LINUX).unwrap();
            let ours: BTreeSet<String> = t.libraries(release).into_iter().map(str::to_string).collect();
            assert_eq!(ours, m966, "{release}: library sets differ");
            for lib in &ours {
                let theirs = boot_table::table().version_of(release, lib, LINUX);
                match t.version(release, lib) {
                    Some("") => {}
                    mine => assert_eq!(mine, theirs, "{release} {lib}: versions differ"),
                }
            }
        }
    }

    /// The issue said `base` depends on `ghc-prim` and `ghc-bignum`. For 9.10
    /// that is wrong: the release's own package database says `ghc-internal`.
    #[test]
    fn base_910_depends_on_ghc_internal() {
        assert_eq!(table().depends("9.10.3", "base"), Some(v(&["ghc-internal", "ghc-prim"]).as_slice()));
    }

    #[test]
    fn base_96_depends_on_ghc_bignum() {
        assert_eq!(table().depends("9.6.6", "base"), Some(v(&["ghc-bignum", "ghc-prim", "rts"]).as_slice()));
    }

    #[test]
    fn rts_is_a_known_leaf_in_every_release() {
        for release in table().releases() {
            assert_eq!(table().depends(release, "rts"), Some(&[][..]), "{release}");
        }
    }

    #[test]
    fn an_absent_release_or_library_is_none() {
        assert_eq!(table().depends("8.10.7", "base"), None);
        assert_eq!(table().depends("9.10.3", "waybill-fixture-absent"), None);
    }

    #[test]
    fn a_malformed_row_is_counted_not_parsed() {
        let t = parse("# observed: 2026-11-24\nghc\tlibrary\tversion\tdepends\tlayout\n9.10.3\tbase\t4.20.2.0\tghc-prim\tpackage.conf.d\n9.10.3\tbad\tx\t-\tpackage.conf.d\n9.10.3\tbase\t4.20.2.0\t-\tpackage.conf.d\n");
        assert_eq!(t.malformed, 1);
        assert_eq!(t.duplicates, 1);
        assert_eq!(t.depends("9.10.3", "base"), Some(v(&["ghc-prim"]).as_slice()));
    }
}
