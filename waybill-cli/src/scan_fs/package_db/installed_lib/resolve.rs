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

use super::{ctest, location, targets, CtestFile, Phase1, Record, RecordKind, SkipReason, TargetsFile};
use crate::scan_fs::package_db::PackageDbEntry;
use crate::scan_fs::test_scope::{TestDeclarations, TestRule};

/// C225: the document-scope coverage record.
pub(crate) const ANN_INSTALLED_LIBRARY_COVERAGE: &str = "waybill:installed-library-coverage";
/// C224: the private-only `Requires.private` targets of a component.
pub(crate) const PRIVATE_REQUIRES: &str = "waybill:pkg-config-private-requires";
/// C227 (spec 1534): which evidence files came from embedded source paths.
pub(crate) const EMBEDDED_PATH_BINARIES: &str = "waybill:embedded-path-binaries";
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
    /// Spec 1532 FR-007: present only when an exported-targets file was
    /// matched.
    pub(crate) exported_targets: Option<ExportedTargetsCoverage>,
    /// Spec 1534 FR-007: present only when an examined binary carried a
    /// source path (FR-008).
    pub(crate) embedded_paths: Option<EmbeddedPathsCoverage>,
}

/// Spec 1534 FR-007 (contracts/wire.md). One binary counts in exactly one
/// of the outcome buckets after `read` and `with_paths`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddedPathsCoverage {
    pub(crate) read: usize,
    pub(crate) with_paths: usize,
    pub(crate) attributed: usize,
    pub(crate) shared: usize,
    pub(crate) header_only: usize,
    pub(crate) toolchain_only: usize,
    pub(crate) no_versioned_match: usize,
}

/// Spec 1532 FR-007 (data-model.md). Counts after `read` and
/// `owned_by_os_package` are per (directory, resolved location) pair.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ExportedTargetsCoverage {
    pub(crate) read: usize,
    pub(crate) owned_by_os_package: usize,
    pub(crate) locations: usize,
    pub(crate) attached: usize,
    pub(crate) not_shipped: usize,
    pub(crate) no_package_version: usize,
    pub(crate) unresolved_location: usize,
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
        let mut v: BTreeMap<&str, serde_json::Value> = BTreeMap::from([
            ("cmake-version", kind(self.cmake_read, self.cmake_emitted)),
            ("pkg-config", kind(self.pkg_config_read, self.pkg_config_emitted)),
            ("skipped", serde_json::Value::Object(skipped)),
            ("unresolved-requires", self.unresolved_requires.into()),
            ("version-disagreements", self.version_disagreements.into()),
        ]);
        if let Some(t) = &self.exported_targets {
            v.insert(
                "exported-targets",
                serde_json::json!({
                    "attached": t.attached,
                    "locations": t.locations,
                    "no-package-version": t.no_package_version,
                    "not-shipped": t.not_shipped,
                    "owned-by-os-package": t.owned_by_os_package,
                    "read": t.read,
                    "unresolved-location": t.unresolved_location,
                }),
            );
        }
        if let Some(e) = &self.embedded_paths {
            v.insert(
                "embedded-paths",
                serde_json::json!({
                    "attributed": e.attributed,
                    "header-only": e.header_only,
                    "no-versioned-match": e.no_versioned_match,
                    "read": e.read,
                    "shared": e.shared,
                    "toolchain-only": e.toolchain_only,
                    "with-paths": e.with_paths,
                }),
            );
        }
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

/// Phase 2's output.
#[derive(Debug, Default)]
pub(crate) struct Resolved {
    /// `.pc` entries first, then CMake, each in path order.
    pub(crate) entries: Vec<PackageDbEntry>,
    /// C225; `None` when phase 1 matched no package metadata file.
    pub(crate) coverage: Option<Coverage>,
    /// Spec 1533: what installed test declarations name.
    pub(crate) tests: TestDeclarations,
}

/// Spec 1533 FR-015 (research R5): a record sits in a recognised install
/// location and no OS package owns it. Needs only phase 1 and the claims, so
/// `scan_path` can decide before the binary scan whether #1524's test-input
/// skip applies.
pub(crate) fn has_installed_record(phase1: &Phase1, scan_root: &Path, claims: &OsClaims<'_>) -> bool {
    phase1.records.iter().any(|r| r.prefix.is_some() && !os_owned(r, scan_root, claims))
}

/// Spec 1534 (research R5): the lowercased names of the records phase 2
/// will turn into versioned components, minus the embedded-path stoplist.
/// Computed before the binary scan so that each binary keeps only the names
/// its embedded paths match.
pub(crate) fn versioned_names(phase1: &Phase1, scan_root: &Path, claims: &OsClaims<'_>) -> BTreeSet<String> {
    phase1
        .records
        .iter()
        .filter(|r| r.prefix.is_some() && r.version.is_ok() && !os_owned(r, scan_root, claims))
        .map(|r| r.name.to_lowercase())
        .filter(|n| !crate::scan_fs::binary::embedded_paths::STOPLIST.contains(&n.as_str()))
        .collect()
}

/// Phase 2. `binaries` are the binary scan's entries.
pub(crate) fn resolve(
    phase1: Phase1,
    scan_root: &Path,
    claims: &OsClaims<'_>,
    binaries: &[PackageDbEntry],
    embedded: &BTreeMap<String, crate::scan_fs::binary::embedded_paths::EmbeddedPathFinding>,
) -> Resolved {
    let Phase1 { records, targets: targets_files, ctest: ctest_files } = phase1;
    let mut out = Resolved::default();
    resolve_ctest(ctest_files, scan_root, claims, &mut out.tests);
    if records.is_empty() && targets_files.is_empty() {
        return out;
    }
    let mut cov = Coverage::default();
    let mut live: Vec<Live> = Vec::new();
    for record in records {
        match record.kind {
            RecordKind::PkgConfig => cov.pkg_config_read += 1,
            RecordKind::CmakeVersion => cov.cmake_read += 1,
        }
        let owned = record.prefix.is_some() && os_owned(&record, scan_root, claims);
        let outcome = match (&record.prefix, &record.version) {
            (None, _) => Err(SkipReason::UnrecognisedLocation),
            (Some(_), _) if owned => Err(SkipReason::OwnedByOsPackage),
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
    let mut evidence = match_binaries(&live, binaries);
    if !targets_files.is_empty() {
        cov.exported_targets =
            Some(attach_exported_targets(&live, targets_files, scan_root, claims, &mut evidence, &mut out.tests));
    }
    let mut embedded_evidence: BTreeMap<usize, Vec<serde_json::Value>> = BTreeMap::new();
    let embedded_cov = attach_embedded_paths(&live, binaries, embedded, scan_root, &mut evidence, &mut embedded_evidence);
    // FR-008: present only when some binary carried a source path, so a scan
    // whose binaries carry none keeps C225 byte-identical.
    cov.embedded_paths = (embedded_cov.with_paths > 0).then_some(embedded_cov);

    let entries: Vec<PackageDbEntry> = live
        .iter()
        .enumerate()
        .map(|(i, l)| {
            match l.record.kind {
                RecordKind::PkgConfig => cov.pkg_config_emitted += 1,
                RecordKind::CmakeVersion => cov.cmake_emitted += 1,
            }
            let (depends, private_only) = deps[i].clone();
            let mut e = entry(l, depends, private_only, evidence.get(&i).cloned().unwrap_or_default());
            if let Some(mut list) = embedded_evidence.remove(&i) {
                list.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
                e.extra_annotations.insert(EMBEDDED_PATH_BINARIES.to_string(), serde_json::Value::Array(list));
            }
            e
        })
        .collect();
    tracing::info!(reader = "installed-lib", coverage = %cov.wire(), "installed libraries");
    out.entries = entries;
    out.coverage = Some(cov);
    out
}

/// Spec 1534 (#1532, data-model "Attribution"): attach each binary file
/// component whose compiled-source paths name versioned package(s) to those
/// packages' records. A binary already in some record's evidence (#1531 name
/// match, #1563 exported targets) is not examined. `findings` is keyed by the
/// binary's `source_path`.
fn attach_embedded_paths(
    live: &[Live],
    binaries: &[PackageDbEntry],
    findings: &BTreeMap<String, crate::scan_fs::binary::embedded_paths::EmbeddedPathFinding>,
    scan_root: &Path,
    evidence: &mut BTreeMap<usize, Vec<String>>,
    c227: &mut BTreeMap<usize, Vec<serde_json::Value>>,
) -> EmbeddedPathsCoverage {
    let mut cov = EmbeddedPathsCoverage::default();
    if live.is_empty() {
        return cov;
    }
    // The record a name attributes to: the CMake version record first, then a
    // same-name `.pc` (as spec 1532's `package_record`).
    let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, l) in live.iter().enumerate() {
        let slot = by_name.entry(l.lname.as_str()).or_insert(i);
        if l.record.kind == RecordKind::CmakeVersion && live[*slot].record.kind != RecordKind::CmakeVersion {
            *slot = i;
        }
    }
    let attributed: HashSet<String> = evidence.values().flatten().cloned().collect();
    let mut seen: HashSet<&str> = HashSet::new();
    for b in binaries.iter().filter(|b| b.binary_class.is_some()) {
        let sp = b.source_path.as_str();
        if attributed.contains(sp) || !seen.insert(sp) {
            continue;
        }
        cov.read += 1;
        let Some(f) = findings.get(sp) else { continue };
        if f.has_paths {
            cov.with_paths += 1;
        }
        let matched: Vec<(&str, usize)> =
            f.packages.iter().filter_map(|n| by_name.get(n.as_str()).map(|i| (n.as_str(), *i))).collect();
        let rel = location::relative_to_root(Path::new(sp), scan_root)
            .map(|r| r.to_string_lossy().into_owned())
            .unwrap_or_else(|| sp.to_string());
        match matched.len() {
            0 if f.header_only => cov.header_only += 1,
            0 if f.toolchain_only => cov.toolchain_only += 1,
            0 if f.has_paths => cov.no_versioned_match += 1,
            0 => {}
            n => {
                if n == 1 {
                    cov.attributed += 1;
                } else {
                    cov.shared += 1;
                }
                for (name, i) in &matched {
                    evidence.entry(*i).or_default().push(sp.to_string());
                    let mut v = serde_json::json!({ "path": rel });
                    if n > 1 {
                        let others: Vec<&str> = matched.iter().map(|(m, _)| *m).filter(|m| m != name).collect();
                        v["shared-with"] = serde_json::json!(others);
                    }
                    c227.entry(*i).or_default().push(v);
                }
            }
        }
    }
    for v in evidence.values_mut() {
        v.sort();
        v.dedup();
    }
    cov
}

/// Spec 1533 (research R1): each installed CTest file's `add_test` locations,
/// resolved from its own directory inside the root. Not tied to an install
/// location: a declaration counts wherever it is found.
fn resolve_ctest(files: Vec<CtestFile>, scan_root: &Path, claims: &OsClaims<'_>, tests: &mut TestDeclarations) {
    for f in files {
        if path_os_owned(&f.path, f.resolved.as_deref(), scan_root, claims) {
            tests.coverage.owned_by_os_package += 1;
            continue;
        }
        let (Some(parsed), Some(dir_rel)) = (f.parsed, location::relative_to_root(&f.dir, scan_root)) else {
            continue;
        };
        tests.coverage.ctest += 1;
        tests.coverage.unresolved += parsed.includes;
        for command in &parsed.tests {
            match ctest::named(command) {
                ctest::Named::Unresolved => tests.coverage.unresolved += 1,
                ctest::Named::Direct(word) => match join_within(&dir_rel, &word) {
                    None => tests.coverage.outside_root += 1,
                    Some(rel) => match shipped(scan_root, &rel) {
                        Some(abs) => {
                            tests.coverage.resolved += 1;
                            declare(tests, scan_root, rel, &abs, TestRule::CtestDeclaration);
                        }
                        None => tests.coverage.not_shipped += 1,
                    },
                },
                ctest::Named::Interpreter(words) => {
                    let mut any = false;
                    for word in words {
                        let Some(rel) = join_within(&dir_rel, &word) else { continue };
                        if let Some(abs) = shipped(scan_root, &rel) {
                            declare(tests, scan_root, rel, &abs, TestRule::CtestDeclaration);
                            any = true;
                        }
                    }
                    if any {
                        tests.coverage.resolved += 1;
                    } else {
                        tests.coverage.unresolved += 1;
                    }
                }
            }
        }
    }
}

/// The regular file `rel` names inside the root, resolved.
fn shipped(scan_root: &Path, rel: &Path) -> Option<PathBuf> {
    location::resolve_in_root(scan_root, rel).filter(|p| p.is_file())
}

/// Declare both spellings of a file: as named, and resolved. The binary
/// scan reports a file under its resolved root-relative spelling (spec 1532
/// M7), and a file-tier path may be either.
fn declare(tests: &mut TestDeclarations, scan_root: &Path, rel: PathBuf, abs: &Path, rule: TestRule) {
    if let Ok(r) = abs.strip_prefix(scan_root) {
        tests.declare(r.to_path_buf(), rule);
    }
    tests.declare(rel, rule);
}

/// `rest` joined onto the root-relative `dir_rel`, lexically. `None` when
/// `..` climbs above the root or `rest` is not relative.
fn join_within(dir_rel: &Path, rest: &str) -> Option<PathBuf> {
    let mut parts: Vec<std::ffi::OsString> = dir_rel.components().map(|c| c.as_os_str().to_os_string()).collect();
    for c in Path::new(rest).components() {
        match c {
            std::path::Component::Normal(s) => parts.push(s.to_os_string()),
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.iter().collect())
}

/// FR-015: listed by dpkg/apk/alpm/opkg/rpm, or inside a Homebrew keg.
fn os_owned(record: &Record, scan_root: &Path, claims: &OsClaims<'_>) -> bool {
    path_os_owned(&record.path, record.resolved.as_deref(), scan_root, claims)
}

fn path_os_owned(path: &Path, resolved: Option<&Path>, scan_root: &Path, claims: &OsClaims<'_>) -> bool {
    let claimed = |p: &Path| {
        crate::scan_fs::binary::is_path_claimed(
            p,
            claims.paths,
            #[cfg(unix)]
            claims.inodes,
        )
    };
    if claimed(path) || resolved.is_some_and(claimed) {
        return true;
    }
    resolved.is_some_and(|r| in_homebrew_keg(r, scan_root))
}

/// Spec 1532 phase 2 (data-model.md "Resolution"): resolve each export
/// set's locations, find its package's live record, and add each shipped
/// file to that record's evidence.
fn attach_exported_targets(
    live: &[Live],
    files: Vec<TargetsFile>,
    scan_root: &Path,
    claims: &OsClaims<'_>,
    evidence: &mut BTreeMap<usize, Vec<String>>,
    tests: &mut TestDeclarations,
) -> ExportedTargetsCoverage {
    let mut cov = ExportedTargetsCoverage::default();
    // One export set per directory, OS-owned files skipped.
    let mut by_dir: BTreeMap<PathBuf, Vec<TargetsFile>> = BTreeMap::new();
    for f in files {
        if f.prefix.is_none() {
            continue;
        }
        if path_os_owned(&f.path, f.resolved.as_deref(), scan_root, claims) {
            cov.owned_by_os_package += 1;
            if f.test_package {
                tests.coverage.owned_by_os_package += 1;
            }
            continue;
        }
        cov.read += 1;
        if f.test_package {
            tests.coverage.exported_targets += 1;
        }
        by_dir.entry(f.dir.clone()).or_default().push(f);
    }
    for (dir, set) in by_dir {
        let raise = set
            .iter()
            .find(|f| f.role == targets::Role::Main)
            .and_then(|f| f.parsed.as_ref())
            .and_then(|p| p.raise);
        let prefix = set.iter().find_map(|f| f.prefix.clone());
        let component = package_record(live, &dir, prefix.as_deref());
        // Spec 1533 FR-004: a test package's export set declares its files,
        // and its own component, test payload.
        let test_set = set.iter().any(|f| f.test_package);
        if let (true, Some(idx)) = (test_set, component) {
            tests.package_purls.insert(live[idx].purl.as_str().to_string());
        }
        let locations: BTreeSet<&str> =
            set.iter().filter_map(|f| f.parsed.as_ref()).flat_map(|p| p.locations.iter().map(String::as_str)).collect();
        for loc in locations {
            cov.locations += 1;
            let rel = match (raise, location::relative_to_root(&dir, scan_root)) {
                (Some(n), Some(dir_rel)) => import_location(&dir_rel, n, loc),
                _ => None,
            };
            let Some(rel) = rel else {
                cov.unresolved_location += 1;
                if test_set {
                    tests.coverage.unresolved += 1;
                }
                continue;
            };
            let Some(resolved) = shipped(scan_root, &rel) else {
                cov.not_shipped += 1;
                if test_set {
                    tests.coverage.not_shipped += 1;
                }
                continue;
            };
            if test_set {
                tests.coverage.resolved += 1;
                declare(tests, scan_root, rel.clone(), &resolved, TestRule::ExportedTargetsTestPackage);
            }
            let Some(idx) = component else {
                cov.no_package_version += 1;
                continue;
            };
            // The binary scan reports a file under this same root-relative
            // spelling, so the evidence-hash cache it seeds is hit
            // (`an_attributed_binary_keeps_the_binary_components_spelling`).
            let path = resolved.to_string_lossy().into_owned();
            evidence.entry(idx).or_default().push(path);
            cov.attached += 1;
        }
    }
    for v in evidence.values_mut() {
        v.sort();
        v.dedup();
    }
    cov
}

/// FR-004: the live record a targets directory belongs to. First the CMake
/// version record in the same directory, then a same-prefix record whose
/// name is the directory's lowercased name.
fn package_record(live: &[Live], dir: &Path, prefix: Option<&Path>) -> Option<usize> {
    if let Some(i) = live
        .iter()
        .position(|l| l.record.kind == RecordKind::CmakeVersion && l.record.path.parent() == Some(dir))
    {
        return Some(i);
    }
    let name = dir.file_name()?.to_str()?.to_lowercase();
    let prefix = prefix?;
    live.iter().position(|l| l.prefix == prefix && l.lname == name)
}

/// FR-003: `${_IMPORT_PREFIX}/<rest>` with `_IMPORT_PREFIX` = `dir_rel` raised
/// `raise` times, as a root-relative path. `None` for any other variable,
/// for raising above the root, or for `..` that climbs above the root.
fn import_location(dir_rel: &Path, raise: usize, loc: &str) -> Option<PathBuf> {
    let rest = loc.strip_prefix("${_IMPORT_PREFIX}/")?;
    if rest.contains("${") || rest.contains("$<") {
        return None;
    }
    let mut parts: Vec<std::ffi::OsString> = dir_rel.components().map(|c| c.as_os_str().to_os_string()).collect();
    for _ in 0..raise {
        parts.pop()?;
    }
    for c in Path::new(rest).components() {
        match c {
            std::path::Component::Normal(s) => parts.push(s.to_os_string()),
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.iter().collect())
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

/// The `.pc` and CMake versions stated for one name in one prefix.
type VersionsByKind<'a> = (BTreeSet<&'a str>, BTreeSet<&'a str>);

/// Names whose `.pc` and CMake versions differ within one prefix.
fn count_disagreements(live: &[Live]) -> usize {
    let mut by_key: BTreeMap<(&Path, &str), VersionsByKind<'_>> = BTreeMap::new();
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
        let r = resolve(Phase1 { records, ..Phase1::default() }, Path::new("/nonexistent-root"), &claims, binaries, &BTreeMap::new());
        (r.entries, r.coverage.unwrap())
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

    // Spec 1532 (T010): the exported-targets object, present only when set.
    #[test]
    fn the_wire_value_gains_exported_targets_only_when_present() {
        let c = Coverage {
            exported_targets: Some(ExportedTargetsCoverage {
                read: 2, owned_by_os_package: 1, locations: 3, attached: 2, not_shipped: 1, ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            c.wire(),
            r#"{"cmake-version":{"emitted":0,"read":0},"exported-targets":{"attached":2,"locations":3,"no-package-version":0,"not-shipped":1,"owned-by-os-package":1,"read":2,"unresolved-location":0},"pkg-config":{"emitted":0,"read":0},"skipped":{"computed-version":0,"malformed":0,"no-version":0,"owned-by-os-package":0,"unrecognised-location":0,"unresolved-version":0},"unresolved-requires":0,"version-disagreements":0}"#
        );
    }

    // Spec 1532 FR-003: prefix arithmetic.
    #[test]
    fn import_locations() {
        let d = Path::new("opt/sdk/lib/cmake/WaybillFixtureP");
        assert_eq!(import_location(d, 3, "${_IMPORT_PREFIX}/lib/libops.a"), Some(PathBuf::from("opt/sdk/lib/libops.a")));
        assert_eq!(import_location(d, 3, "${_IMPORT_PREFIX}/bin/tool"), Some(PathBuf::from("opt/sdk/bin/tool")));
        assert_eq!(import_location(d, 5, "${_IMPORT_PREFIX}/x"), Some(PathBuf::from("x")), "raised to the root");
        assert_eq!(import_location(d, 6, "${_IMPORT_PREFIX}/x"), None, "raised above the root");
        assert_eq!(import_location(d, 3, "${_IMPORT_PREFIX}/../../../../x"), None, "climbs above the root");
        assert_eq!(import_location(d, 3, "${CMAKE_INSTALL_PREFIX}/lib/x.so"), None);
        assert_eq!(import_location(d, 3, "${_IMPORT_PREFIX}/lib/${FOO}.so"), None);
        assert_eq!(import_location(d, 3, "/usr/lib/x.so"), None);
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

    /// Spec 1534 T005: the names handed to the binary scan are exactly the
    /// versioned components phase 2 emits, minus the stoplist.
    #[test]
    fn versioned_names_are_the_emitted_components() {
        let mut no_prefix = pc("/r/x/a.pc", "WaybillFixtureNoPrefix", "/r/x", "1", &[], &[]);
        no_prefix.prefix = None;
        let mut no_version = pc("/r/lib/pkgconfig/b.pc", "waybill-fixture-nover", "/r", "1", &[], &[]);
        no_version.version = Err(SkipReason::NoVersion);
        let records = vec![
            pc("/r/lib/pkgconfig/WaybillFixtureP.pc", "WaybillFixtureP", "/r", "1.2.3", &[], &[]),
            pc("/r/lib/pkgconfig/amd.pc", "AMD", "/r", "3.3.3", &[], &[]),
            no_prefix,
            no_version,
        ];
        let paths = HashSet::new();
        #[cfg(unix)]
        let inodes = HashSet::new();
        let claims = OsClaims {
            paths: &paths,
            #[cfg(unix)]
            inodes: &inodes,
        };
        let phase1 = Phase1 { records: records.clone(), ..Phase1::default() };
        let names = versioned_names(&phase1, Path::new("/nonexistent-root"), &claims);
        assert_eq!(names, ["waybillfixturep".to_string()].into_iter().collect());
        let (entries, _) = run(records, &[]);
        let emitted: BTreeSet<String> = entries.iter().map(|e| e.name.to_lowercase()).collect();
        let stop: BTreeSet<String> = ["amd".to_string()].into_iter().collect();
        assert_eq!(names, emitted.difference(&stop).cloned().collect());
    }
}
