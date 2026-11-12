//! Milestone 966 (#966) — the libraries each GHC release ships, and their
//! versions.
//!
//! A GHC boot library (`base`, `process`, `Cabal`, …) has no version in the
//! nixpkgs package set: nixpkgs binds it to `null` and uses the compiler's
//! copy. Its version is therefore a property of the compiler release, and it
//! changes between patch releases (`base` and `process` move within every
//! multi-release series), so this table is keyed by the exact release
//! `X.Y.Z`, never by series.
//!
//! The data is `ghc_boot_libraries.tsv`, produced by
//! `scripts/ghc-boot-libraries.py` from GHC's release notes, Stack's
//! `global-hints.yaml` for what the notes omit, and GHC's release tags where
//! the two disagree. Its header records the sources and the observation date.
//! It is embedded at build time: a scan never reaches the network for it
//! (FR-012). Contract: `specs/966-ghc-boot-versions/contracts/boot-library-table.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// The committed table.
const TSV: &str = include_str!("ghc_boot_libraries.tsv");

/// Where one row's version came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableSource {
    /// GHC's release notes, "Included libraries".
    ReleaseNotes,
    /// Stack's `global-hints.yaml`, for a library the notes omit.
    StackGlobalHints,
    /// GHC's `ghc-X.Y.Z-release` tag, settling a disagreement between the two.
    GhcReleaseTag,
}

impl TableSource {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "release-notes" => Some(Self::ReleaseNotes),
            "stack-global-hints" => Some(Self::StackGlobalHints),
            "ghc-release-tag" => Some(Self::GhcReleaseTag),
            _ => None,
        }
    }
}

/// Which systems a GHC ships a library on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    All,
    /// `Win32`: listed by every source, shipped only by a Windows GHC.
    Windows,
}

impl Platform {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "all" => Some(Self::All),
            "windows" => Some(Self::Windows),
            _ => None,
        }
    }

    fn applies_to(self, system: &str) -> bool {
        match self {
            Platform::All => true,
            Platform::Windows => is_windows(system),
        }
    }
}

/// Whether a Nix system double names Windows (`x86_64-windows`).
fn is_windows(system: &str) -> bool {
    system.ends_with("-windows") || system.ends_with("-mingw32")
}

/// One `(release, library)` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BootRow {
    pub(crate) version: String,
    pub(crate) source: TableSource,
    pub(crate) platform: Platform,
}

/// The parsed table.
#[derive(Debug, Default)]
pub(crate) struct BootLibraryTable {
    /// The `# observed:` header date.
    observed: String,
    /// release (`9.10.3`) -> library -> row.
    rows: BTreeMap<String, BTreeMap<String, BootRow>>,
    /// Lines that did not parse. Skipped, never a panic; the unit tests
    /// assert this is 0 for the embedded file.
    malformed: usize,
    /// `(release, library)` pairs seen twice; the later line is ignored.
    duplicates: usize,
}

/// The embedded table, parsed once per process.
pub(crate) fn table() -> &'static BootLibraryTable {
    static TABLE: OnceLock<BootLibraryTable> = OnceLock::new();
    TABLE.get_or_init(|| parse(TSV))
}

fn is_dotted_digits(v: &str) -> bool {
    !v.is_empty() && v.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn parse(text: &str) -> BootLibraryTable {
    let mut t = BootLibraryTable::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# observed:") {
            t.observed = rest.trim().to_string();
            continue;
        }
        if line.starts_with('#') || line.is_empty() || line == "ghc\tlibrary\tversion\tsource\tplatform" {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let parsed = match fields.as_slice() {
            [ghc, lib, version, source, platform]
                if is_dotted_digits(ghc) && !lib.is_empty() && is_dotted_digits(version) =>
            {
                TableSource::parse(source).zip(Platform::parse(platform)).map(|(source, platform)| {
                    (*ghc, *lib, BootRow { version: (*version).to_string(), source, platform })
                })
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

impl BootLibraryTable {
    /// The date the table's sources were observed.
    pub(crate) fn observed(&self) -> &str {
        &self.observed
    }

    /// Whether the table has rows for this release.
    pub(crate) fn has_release(&self, release: &str) -> bool {
        self.rows.contains_key(release)
    }

    /// The libraries `release` ships on `system`: every `all` row, plus the
    /// `windows` rows on a Windows system. `None` when the release is absent.
    ///
    /// Test-only: production asks per library through [`Self::version_of`],
    /// which applies the same filter, and classifies with [`Self::names`].
    #[cfg(test)]
    pub(crate) fn ships(&self, release: &str, system: &str) -> Option<BTreeSet<String>> {
        self.rows.get(release).map(|libs| {
            libs.iter()
                .filter(|(_, r)| r.platform.applies_to(system))
                .map(|(n, _)| n.clone())
                .collect()
        })
    }

    /// Every library the table lists for `release`, whatever the platform.
    ///
    /// The compiler-supplied set for classification. A `windows` row on a
    /// non-Windows system is kept in it so that `Win32` is never resolved
    /// from the package set, whose version the build does not use; it is
    /// never *versioned* there either, because [`Self::version_of`] filters by
    /// platform. Research R4, as corrected at implementation.
    pub(crate) fn names(&self, release: &str) -> Option<BTreeSet<String>> {
        self.rows.get(release).map(|libs| libs.keys().cloned().collect())
    }

    /// The version `release` ships `name` at on `system`, with the same
    /// platform filter as [`Self::ships`].
    pub(crate) fn version_of(&self, release: &str, name: &str, system: &str) -> Option<&str> {
        self.rows
            .get(release)?
            .get(name)
            .filter(|r| r.platform.applies_to(system))
            .map(|r| r.version.as_str())
    }

    #[cfg(test)]
    fn row(&self, release: &str, name: &str) -> Option<&BootRow> {
        self.rows.get(release)?.get(name)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn every_line_of_the_embedded_table_parses() {
        let t = table();
        assert_eq!(t.malformed, 0, "malformed lines in ghc_boot_libraries.tsv");
        // CONTROL: the file was read, so a zero above is not an empty parse.
        let rows: usize = t.rows.values().map(BTreeMap::len).sum();
        assert!(rows > 1000, "only {rows} rows parsed");
        assert!(t.rows.len() >= 30, "only {} releases", t.rows.len());
    }

    #[test]
    fn no_release_library_pair_appears_twice() {
        assert_eq!(table().duplicates, 0);
        // CONTROL: the counter counts.
        let t = parse("9.0.1\twaybill-fixture-a\t1.0\trelease-notes\tall\n9.0.1\twaybill-fixture-a\t1.0\trelease-notes\tall\n");
        assert_eq!(t.duplicates, 1);
    }

    #[test]
    fn a_malformed_line_is_counted_and_skipped() {
        let t = parse("9.0.1\twaybill-fixture-a\tnot-a-version\trelease-notes\tall\n9.0.1\twaybill-fixture-b\t0.5\tsomewhere\tall\n");
        assert_eq!(t.malformed, 2);
        assert!(!t.has_release("9.0.1"));
    }

    #[test]
    fn every_release_has_base() {
        for (release, libs) in &table().rows {
            assert!(libs.contains_key("base"), "{release} has no base");
        }
    }

    #[test]
    fn the_known_stack_error_is_settled_by_the_release_tag() {
        let row = table().row("9.12.3", "process").unwrap();
        assert_eq!(row.version, "1.6.26.1");
        assert_eq!(row.source, TableSource::GhcReleaseTag);
    }

    #[test]
    fn win32_ships_only_on_windows() {
        let t = table();
        let linux = t.ships("9.10.3", "x86_64-linux").unwrap();
        assert!(!linux.contains("Win32"));
        assert!(linux.contains("unix"));
        assert!(linux.contains("base"));
        let windows = t.ships("9.10.3", "x86_64-windows").unwrap();
        assert!(windows.contains("Win32"));
        assert_eq!(t.version_of("9.10.3", "Win32", "aarch64-darwin"), None);
        assert!(t.version_of("9.10.3", "Win32", "x86_64-windows").is_some());
        // The classification set keeps it, so it is never resolved from the
        // package set on a system where the compiler does not ship it.
        assert!(t.names("9.10.3").unwrap().contains("Win32"));
    }

    #[test]
    fn an_absent_release_is_none() {
        let t = table();
        assert_eq!(t.ships("99.0.1", "x86_64-linux"), None);
        assert_eq!(t.names("99.0.1"), None);
        assert_eq!(t.version_of("99.0.1", "base", "x86_64-linux"), None);
        assert!(!t.has_release("99.0.1"));
    }

    /// Spec US3 acceptance 2: the table says where it came from, when, and
    /// how to regenerate it.
    #[test]
    fn the_header_records_sources_date_and_regeneration() {
        for marker in ["# observed:", "# sources:", "# regenerate:"] {
            assert!(TSV.lines().any(|l| l.starts_with(marker)), "missing {marker}");
        }
        let date = table().observed();
        assert_eq!(date.len(), 10, "observed date {date:?}");
        assert!(date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-', "{date:?}");
    }
}
