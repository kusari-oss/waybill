//! Phase 2 of the installed-library reader (spec 1531, research R2–R6, R9).
//!
//! Called from `scan_fs::scan_path` after the binary scan, with the
//! OS-package claims and the binary file components. It:
//! 1. drops records an OS package owns (FR-015) or that failed to parse;
//! 2. builds one `pkg:generic/<name>@<version>` entry per remaining record;
//! 3. resolves `Requires` / `Requires.private` to exact PURLs (FR-004,
//!    FR-011);
//! 4. lists each library's matching shared-library files in its evidence
//!    (FR-010);
//! 5. counts everything in the coverage record (C225, FR-008).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use waybill_common::resolution::BinaryRole;
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::{location, Record, RecordKind, SkipReason};
use crate::scan_fs::package_db::PackageDbEntry;

/// C225: the document-scope coverage record.
pub(crate) const ANN_INSTALLED_LIBRARY_COVERAGE: &str = "waybill:installed-library-coverage";
/// C224: the private-only `Requires.private` targets of a component.
pub(crate) const PRIVATE_REQUIRES: &str = "waybill:pkg-config-private-requires";
/// C103, widened (research R5): the declared case of a CMake package name.
const CMAKE_DECLARED_NAME: &str = "waybill:cmake-find-package-name";

/// The OS-package claims (`DbScanResult::os_claimed_*`).
pub(crate) struct OsClaims<'a> {
    pub(crate) paths: &'a HashSet<PathBuf>,
    #[cfg(unix)]
    pub(crate) inodes: &'a HashSet<(u64, u64)>,
}

/// The C225 coverage record (data-model.md).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Coverage {
    pub(crate) pkg_config_read: usize,
    pub(crate) pkg_config_emitted: usize,
    pub(crate) cmake_read: usize,
    pub(crate) cmake_emitted: usize,
    pub(crate) skipped: BTreeMap<SkipReason, usize>,
    pub(crate) unresolved_requires: usize,
    pub(crate) version_disagreements: usize,
}

impl Coverage {
    /// The wire value: a JSON object with sorted keys, every skip reason
    /// present.
    pub(crate) fn wire(&self) -> String {
        let mut skipped = serde_json::Map::new();
        for r in SkipReason::ALL {
            skipped.insert(r.as_str().to_string(), self.skipped.get(&r).copied().unwrap_or(0).into());
        }
        let kind = |read: usize, emitted: usize| serde_json::json!({"emitted": emitted, "read": read});
        let v: BTreeMap<&str, serde_json::Value> = BTreeMap::from([
            ("cmake-version", kind(self.cmake_read, self.cmake_emitted)),
            ("pkg-config", kind(self.pkg_config_read, self.pkg_config_emitted)),
            ("skipped", serde_json::Value::Object(skipped)),
            ("unresolved-requires", self.unresolved_requires.into()),
            ("version-disagreements", self.version_disagreements.into()),
        ]);
        serde_json::to_string(&v).unwrap_or_default()
    }

    fn skip(&mut self, r: SkipReason) {
        *self.skipped.entry(r).or_default() += 1;
    }
}

/// A record that will become an entry.
struct Live {
    record: Record,
    prefix: PathBuf,
    lname: String,
    version: String,
    purl: Purl,
}

/// Phase 2. `binaries` are the binary scan's entries. Returns the new
/// entries (`.pc` first, then CMake, each in path order) and the coverage
/// record, which is `None` when phase 1 matched no file.
pub(crate) fn resolve(
    records: Vec<Record>,
    scan_root: &Path,
    claims: &OsClaims<'_>,
    binaries: &[PackageDbEntry],
) -> (Vec<PackageDbEntry>, Option<Coverage>) {
    if records.is_empty() {
        return (Vec::new(), None);
    }
    let mut cov = Coverage::default();
    let mut live: Vec<Live> = Vec::new();
    for record in records {
        match record.kind {
            RecordKind::PkgConfig => cov.pkg_config_read += 1,
            RecordKind::CmakeVersion => cov.cmake_read += 1,
        }
        let outcome = match (&record.prefix, &record.version) {
            (None, _) => Err(SkipReason::UnrecognisedLocation),
            (Some(_), _) if os_owned(&record, scan_root, claims) => Err(SkipReason::OwnedByOsPackage),
            (Some(_), Err(r)) => Err(*r),
            (Some(prefix), Ok(v)) => Ok((prefix.clone(), v.clone())),
        };
        let (prefix, version) = match outcome {
            Ok(x) => x,
            Err(r) => {
                tracing::warn!(
                    reader = "installed-lib",
                    path = %record.path.display(),
                    reason = r.as_str(),
                    "installed-library file skipped"
                );
                cov.skip(r);
                continue;
            }
        };
        let lname = record.name.to_lowercase();
        let purl_str = format!("pkg:generic/{}@{}", encode_purl_segment(&lname), encode_purl_segment(&version));
        let Ok(purl) = Purl::new(&purl_str) else {
            tracing::warn!(reader = "installed-lib", path = %record.path.display(), purl = %purl_str, "unparseable PURL");
            cov.skip(SkipReason::Malformed);
            continue;
        };
        live.push(Live { record, prefix, lname, version, purl });
    }
    live.sort_by(|a, b| (a.record.kind, &a.record.path).cmp(&(b.record.kind, &b.record.path)));

    cov.version_disagreements = count_disagreements(&live);
    let deps = resolve_requires(&live, &mut cov);
    let evidence = match_binaries(&live, binaries);

    let entries: Vec<PackageDbEntry> = live
        .iter()
        .enumerate()
        .map(|(i, l)| {
            match l.record.kind {
                RecordKind::PkgConfig => cov.pkg_config_emitted += 1,
                RecordKind::CmakeVersion => cov.cmake_emitted += 1,
            }
            let (depends, private_only) = deps[i].clone();
            entry(l, depends, private_only, evidence.get(&i).cloned().unwrap_or_default())
        })
        .collect();
    tracing::info!(reader = "installed-lib", coverage = %cov.wire(), "installed libraries");
    (entries, Some(cov))
}

/// FR-015: listed by dpkg/apk/alpm/opkg/rpm, or inside a Homebrew keg.
fn os_owned(record: &Record, scan_root: &Path, claims: &OsClaims<'_>) -> bool {
    let claimed = |p: &Path| {
        crate::scan_fs::binary::is_path_claimed(
            p,
            claims.paths,
            #[cfg(unix)]
            claims.inodes,
        )
    };
    if claimed(&record.path) || record.resolved.as_deref().is_some_and(claimed) {
        return true;
    }
    record.resolved.as_deref().is_some_and(|r| in_homebrew_keg(r, scan_root))
}

/// `<root>/<brew-prefix>/Cellar/<name>/<version>/…` with that version
/// directory holding `INSTALL_RECEIPT.json` (research R3). `resolved` is
/// already root-relative-resolved, so this reads nothing outside the root.
fn in_homebrew_keg(resolved: &Path, scan_root: &Path) -> bool {
    let Some(rel) = location::relative_to_root(resolved, scan_root) else {
        return false;
    };
    crate::scan_fs::package_db::brew::HOMEBREW_PREFIXES.iter().any(|bp| {
        let cellar = Path::new(bp).join("Cellar");
        let Ok(in_cellar) = rel.strip_prefix(&cellar) else {
            return false;
        };
        let mut it = in_cellar.components();
        match (it.next(), it.next()) {
            (Some(name), Some(version)) => scan_root
                .join(&cellar)
                .join(name.as_os_str())
                .join(version.as_os_str())
                .join("INSTALL_RECEIPT.json")
                .is_file(),
            _ => false,
        }
    })
}

/// Names whose `.pc` and CMake versions differ within one prefix.
fn count_disagreements(live: &[Live]) -> usize {
    let mut by_key: BTreeMap<(&Path, &str), (BTreeSet<&str>, BTreeSet<&str>)> = BTreeMap::new();
    for l in live {
        let e = by_key.entry((l.prefix.as_path(), l.lname.as_str())).or_default();
        match l.record.kind {
            RecordKind::PkgConfig => e.0.insert(l.version.as_str()),
            RecordKind::CmakeVersion => e.1.insert(l.version.as_str()),
        };
    }
    by_key.values().filter(|(pc, cm)| !pc.is_empty() && !cm.is_empty() && pc != cm).count()
}

/// Per live record: (sorted exact `depends` PURLs, sorted private-only
/// PURLs). Only `.pc` records have requirements, and only `.pc` modules can
/// be their targets: `Requires` names pkg-config modules (data-model.md).
fn resolve_requires(live: &[Live], cov: &mut Coverage) -> Vec<(Vec<String>, Vec<String>)> {
    let modules: Vec<&Live> = live.iter().filter(|l| l.record.kind == RecordKind::PkgConfig).collect();
    let lookup = |name: &str, prefix: &Path| -> Option<String> {
        let lname = name.to_lowercase();
        let unique = |cands: BTreeSet<&str>| (cands.len() == 1).then(|| cands.into_iter().next().map(str::to_string)).flatten();
        let same_prefix: BTreeSet<&str> = modules
            .iter()
            .filter(|m| m.lname == lname && m.prefix == prefix)
            .map(|m| m.purl.as_str())
            .collect();
        if !same_prefix.is_empty() {
            return unique(same_prefix);
        }
        unique(modules.iter().filter(|m| m.lname == lname).map(|m| m.purl.as_str()).collect())
    };
    live.iter()
        .map(|l| {
            let mut public: BTreeSet<String> = BTreeSet::new();
            let mut private: BTreeSet<String> = BTreeSet::new();
            for (names, set) in [(&l.record.requires, &mut public), (&l.record.requires_private, &mut private)] {
                for name in names {
                    match lookup(name, &l.prefix) {
                        Some(p) if p != l.purl.as_str() => {
                            set.insert(p);
                        }
                        Some(_) => {}
                        None => {
                            tracing::warn!(
                                reader = "installed-lib",
                                path = %l.record.path.display(),
                                module = %name,
                                "Requires names no single module in the scan"
                            );
                            cov.unresolved_requires += 1;
                        }
                    }
                }
            }
            let private_only: Vec<String> = private.difference(&public).cloned().collect();
            let depends: Vec<String> = public.union(&private).cloned().collect();
            (depends, private_only)
        })
        .collect()
}

/// Whether `file_name` is a shared-library file of module `lname`
/// (research R9): `lib<m>.so`, `lib<m>.so.<N…>`, `lib<m>.dylib`,
/// `lib<m>.<N…>.dylib`; `<m>` already beginning with `lib` is not doubled.
pub(crate) fn names_library(file_name: &str, lname: &str) -> bool {
    let f = file_name.to_lowercase();
    let base = if lname.starts_with("lib") { lname.to_string() } else { format!("lib{lname}") };
    let numeric = |s: &str| !s.is_empty() && s.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if f == format!("{base}.so") || f == format!("{base}.dylib") {
        return true;
    }
    if let Some(v) = f.strip_prefix(&format!("{base}.so.")) {
        return numeric(v);
    }
    if let Some(mid) = f.strip_prefix(&format!("{base}.")).and_then(|r| r.strip_suffix(".dylib")) {
        return numeric(mid);
    }
    false
}

/// Live-record index → matched shared-library paths, sorted. A binary is
/// considered against the records of the deepest prefix holding it, and is
/// listed only when exactly one library name matches it (FR-010).
fn match_binaries(live: &[Live], binaries: &[PackageDbEntry]) -> BTreeMap<usize, Vec<String>> {
    let mut out: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for b in binaries {
        if b.binary_role != Some(BinaryRole::SharedLibrary) {
            continue;
        }
        let path = Path::new(&b.source_path);
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(deepest) = live
            .iter()
            .filter(|l| path.starts_with(&l.prefix))
            .map(|l| l.prefix.components().count())
            .max()
        else {
            continue;
        };
        let in_prefix: Vec<(usize, &Live)> = live
            .iter()
            .enumerate()
            .filter(|(_, l)| path.starts_with(&l.prefix) && l.prefix.components().count() == deepest)
            .collect();
        let names: BTreeSet<&str> = in_prefix
            .iter()
            .filter(|(_, l)| names_library(file_name, &l.lname))
            .map(|(_, l)| l.lname.as_str())
            .collect();
        if names.len() != 1 {
            continue;
        }
        for (i, l) in &in_prefix {
            if names.contains(l.lname.as_str()) {
                out.entry(*i).or_default().push(b.source_path.clone());
            }
        }
    }
    for v in out.values_mut() {
        v.sort();
        v.dedup();
    }
    out
}

fn entry(l: &Live, depends: Vec<String>, private_only: Vec<String>, binaries: Vec<String>) -> PackageDbEntry {
    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "waybill:source-mechanism".to_string(),
        serde_json::json!(l.record.kind.source_mechanism()),
    );
    if !private_only.is_empty() {
        extra_annotations.insert(
            PRIVATE_REQUIRES.to_string(),
            serde_json::Value::Array(private_only.into_iter().map(serde_json::Value::String).collect()),
        );
    }
    if l.record.kind == RecordKind::CmakeVersion && l.record.name != l.lname {
        extra_annotations.insert(CMAKE_DECLARED_NAME.to_string(), serde_json::json!(l.record.name));
    }
    PackageDbEntry {
        extra_source_paths: binaries,
        depends_ecosystem: None,
        build_inclusion: None,
        purl: l.purl.clone(),
        name: l.lname.clone(),
        version: l.version.clone(),
        arch: None,
        source_path: l.record.path.to_string_lossy().into_owned(),
        depends,
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
        sbom_tier: Some("deployed".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn rec(kind: RecordKind, path: &str, name: &str, prefix: &str, version: &str, req: &[&str], private: &[&str]) -> Record {
        Record {
            kind,
            path: PathBuf::from(path),
            resolved: Some(PathBuf::from(path)),
            name: name.into(),
            prefix: Some(PathBuf::from(prefix)),
            version: Ok(version.into()),
            requires: req.iter().map(|s| s.to_string()).collect(),
            requires_private: private.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn pc(path: &str, name: &str, prefix: &str, version: &str, req: &[&str], private: &[&str]) -> Record {
        rec(RecordKind::PkgConfig, path, name, prefix, version, req, private)
    }

    fn run(records: Vec<Record>, binaries: &[PackageDbEntry]) -> (Vec<PackageDbEntry>, Coverage) {
        let paths = HashSet::new();
        #[cfg(unix)]
        let inodes = HashSet::new();
        let claims = OsClaims {
            paths: &paths,
            #[cfg(unix)]
            inodes: &inodes,
        };
        let (e, c) = resolve(records, Path::new("/nonexistent-root"), &claims, binaries);
        (e, c.unwrap())
    }

    fn deps_of<'a>(entries: &'a [PackageDbEntry], purl: &str) -> &'a [String] {
        &entries.iter().find(|e| e.purl.as_str() == purl).unwrap().depends
    }

    // T022: resolution.
    #[test]
    fn same_prefix_wins() {
        let (e, cov) = run(
            vec![
                pc("/r/a/lib/pkgconfig/x.pc", "x", "/r/a", "1", &["c"], &[]),
                pc("/r/a/lib/pkgconfig/c.pc", "c", "/r/a", "2", &[], &[]),
                pc("/r/b/lib/pkgconfig/c.pc", "c", "/r/b", "3", &[], &[]),
            ],
            &[],
        );
        assert_eq!(deps_of(&e, "pkg:generic/x@1"), ["pkg:generic/c@2"]);
        assert_eq!(cov.unresolved_requires, 0);
    }

    #[test]
    fn a_name_unique_across_the_scan_resolves() {
        let (e, _) = run(
            vec![
                pc("/r/a/lib/pkgconfig/x.pc", "x", "/r/a", "1", &["c"], &[]),
                pc("/r/b/lib/pkgconfig/c.pc", "c", "/r/b", "3", &[], &[]),
            ],
            &[],
        );
        assert_eq!(deps_of(&e, "pkg:generic/x@1"), ["pkg:generic/c@3"]);
    }

    #[test]
    fn two_candidates_elsewhere_are_unresolved() {
        let (e, cov) = run(
            vec![
                pc("/r/a/lib/pkgconfig/x.pc", "x", "/r/a", "1", &["c"], &[]),
                pc("/r/b/lib/pkgconfig/c.pc", "c", "/r/b", "2", &[], &[]),
                pc("/r/d/lib/pkgconfig/c.pc", "c", "/r/d", "3", &[], &[]),
            ],
            &[],
        );
        assert!(deps_of(&e, "pkg:generic/x@1").is_empty());
        assert_eq!(cov.unresolved_requires, 1);
    }

    #[test]
    fn a_missing_name_is_unresolved_and_counted() {
        let (e, cov) = run(vec![pc("/r/a/lib/pkgconfig/x.pc", "x", "/r/a", "1", &["gone"], &["gone2"])], &[]);
        assert!(deps_of(&e, "pkg:generic/x@1").is_empty());
        assert_eq!(cov.unresolved_requires, 2);
    }

    #[test]
    fn a_module_in_both_lists_is_one_edge_and_not_private() {
        let (e, _) = run(
            vec![
                pc("/r/a/lib/pkgconfig/x.pc", "x", "/r/a", "1", &["c"], &["c", "d"]),
                pc("/r/a/lib/pkgconfig/c.pc", "c", "/r/a", "2", &[], &[]),
                pc("/r/a/lib/pkgconfig/d.pc", "d", "/r/a", "3", &[], &[]),
            ],
            &[],
        );
        let x = e.iter().find(|e| e.purl.as_str() == "pkg:generic/x@1").unwrap();
        assert_eq!(x.depends, ["pkg:generic/c@2", "pkg:generic/d@3"]);
        assert_eq!(x.extra_annotations[PRIVATE_REQUIRES], serde_json::json!(["pkg:generic/d@3"]));
    }

    #[test]
    fn a_cmake_record_is_not_a_requires_target() {
        let (e, cov) = run(
            vec![
                pc("/r/a/lib/pkgconfig/x.pc", "x", "/r/a", "1", &["c"], &[]),
                rec(RecordKind::CmakeVersion, "/r/a/lib/cmake/c/c-config-version.cmake", "c", "/r/a", "2", &[], &[]),
            ],
            &[],
        );
        assert!(deps_of(&e, "pkg:generic/x@1").is_empty());
        assert_eq!(cov.unresolved_requires, 1);
    }

    #[test]
    fn version_disagreements_are_counted() {
        let (_, cov) = run(
            vec![
                pc("/r/a/lib/pkgconfig/c.pc", "c", "/r/a", "2", &[], &[]),
                rec(RecordKind::CmakeVersion, "/r/a/lib/cmake/C/CConfigVersion.cmake", "C", "/r/a", "2.1", &[], &[]),
                pc("/r/a/lib/pkgconfig/d.pc", "d", "/r/a", "3", &[], &[]),
                rec(RecordKind::CmakeVersion, "/r/a/lib/cmake/d/d-config-version.cmake", "d", "/r/a", "3", &[], &[]),
            ],
            &[],
        );
        assert_eq!(cov.version_disagreements, 1, "C (case-folded) disagrees; d agrees");
    }

    #[test]
    fn entries_are_pc_first_and_carry_mechanism_tier_and_declared_case() {
        let (e, cov) = run(
            vec![
                rec(RecordKind::CmakeVersion, "/r/a/lib/cmake/Foo/FooConfigVersion.cmake", "Foo", "/r/a", "4.5.6", &[], &[]),
                pc("/r/a/lib/pkgconfig/z.pc", "z", "/r/a", "1", &[], &[]),
            ],
            &[],
        );
        assert_eq!(e[0].purl.as_str(), "pkg:generic/z@1");
        assert_eq!(e[1].purl.as_str(), "pkg:generic/foo@4.5.6");
        assert_eq!(e[1].extra_annotations["waybill:source-mechanism"], "cmake-package-version");
        assert_eq!(e[1].extra_annotations[CMAKE_DECLARED_NAME], "Foo");
        assert!(!e[0].extra_annotations.contains_key(CMAKE_DECLARED_NAME));
        assert_eq!(e[0].sbom_tier.as_deref(), Some("deployed"));
        assert_eq!((cov.pkg_config_emitted, cov.cmake_emitted), (1, 1));
    }

    #[test]
    fn the_wire_value_lists_every_reason_with_sorted_keys() {
        let mut c = Coverage { pkg_config_read: 3, pkg_config_emitted: 2, ..Default::default() };
        c.skip(SkipReason::UnresolvedVersion);
        assert_eq!(
            c.wire(),
            r#"{"cmake-version":{"emitted":0,"read":0},"pkg-config":{"emitted":2,"read":3},"skipped":{"computed-version":0,"malformed":0,"no-version":0,"owned-by-os-package":0,"unrecognised-location":0,"unresolved-version":1},"unresolved-requires":0,"version-disagreements":0}"#
        );
    }

    // T026: binary matching.
    #[test]
    fn library_file_names() {
        for f in ["libwaybill-fixture-a.so", "libwaybill-fixture-a.so.1", "libwaybill-fixture-a.so.1.2.3",
                  "libwaybill-fixture-a.dylib", "libwaybill-fixture-a.1.dylib"] {
            assert!(names_library(f, "waybill-fixture-a"), "{f}");
        }
        assert!(names_library("libfoo.so.1", "libfoo"));
        for f in ["libwaybill-fixture-ab.so", "libwaybill-fixture-a.so.x", "waybill-fixture-a.so",
                  "libwaybill-fixture-a.a", "libwaybill-fixture-a-extra.so"] {
            assert!(!names_library(f, "waybill-fixture-a"), "{f}");
        }
    }

    fn shared_lib(path: &str) -> PackageDbEntry {
        let mut e = entry(
            &Live {
                record: pc(path, "bin", "/", "0", &[], &[]),
                prefix: PathBuf::from("/"),
                lname: "bin".into(),
                version: "0".into(),
                purl: Purl::new("pkg:generic/bin@0").unwrap(),
            },
            vec![],
            vec![],
            vec![],
        );
        e.binary_role = Some(BinaryRole::SharedLibrary);
        e
    }

    #[test]
    fn binaries_match_within_their_own_prefix_and_ambiguity_lists_none() {
        let binaries = [
            shared_lib("/r/a/lib/libx.so.1"),
            shared_lib("/r/b/lib/libx.so.1"),
            shared_lib("/r/a/lib/libgone.so"),
        ];
        let (e, _) = run(
            vec![
                pc("/r/a/lib/pkgconfig/x.pc", "x", "/r/a", "1", &[], &[]),
                pc("/r/b/lib/pkgconfig/y.pc", "y", "/r/b", "1", &[], &[]),
            ],
            &binaries,
        );
        let x = e.iter().find(|e| e.purl.as_str() == "pkg:generic/x@1").unwrap();
        assert_eq!(x.extra_source_paths, ["/r/a/lib/libx.so.1"], "the /r/b copy is in another prefix");
        let y = e.iter().find(|e| e.purl.as_str() == "pkg:generic/y@1").unwrap();
        assert!(y.extra_source_paths.is_empty());

        // `libfoo.so` matches both module `foo` and module `libfoo`: ambiguous.
        let (e, _) = run(
            vec![
                pc("/r/a/lib/pkgconfig/foo.pc", "foo", "/r/a", "1", &[], &[]),
                pc("/r/a/lib/pkgconfig/libfoo.pc", "libfoo", "/r/a", "2", &[], &[]),
            ],
            &[shared_lib("/r/a/lib/libfoo.so")],
        );
        assert!(e.iter().all(|e| e.extra_source_paths.is_empty()), "an ambiguous binary is listed under none");
    }
}
