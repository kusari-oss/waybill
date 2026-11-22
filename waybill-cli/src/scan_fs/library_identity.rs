//! Spec 1645 (#1662): one library, one component, when the curated
//! embedded-version scanner and an installed-library record (pkg-config,
//! CMake package version) name or version it differently.
//!
//! [`reconcile`] runs once per scan, after the installed-library entries are
//! spliced in ahead of the binary entries. For each file a curated match was
//! read from, it looks at the records whose own library files include that
//! file (research R1) and gives a verdict (data-model):
//! - `merge`: exactly one record has one of the library's names
//!   ([`CuratedLibrary::names`]) and an agreeing version ([`versions_agree`]);
//! - `version-disagrees`: a record has a qualifying name, none an agreeing
//!   version;
//! - `name-unrelated`: no record has a qualifying name;
//! - `ambiguous`: two or more records qualify, so none is picked.
//!
//! On `merge` it rewrites the record and the curated entry to one PURL,
//! `pkg:generic/<slug>@<record version>`, and every `depends` naming an old
//! PURL (R2). The deduplicator then merges them as it merges any equal PURLs.

use std::collections::BTreeMap;

use waybill_common::types::purl::{encode_purl_segment, Purl};

use crate::scan_fs::binary::version_strings::CuratedLibrary;
use crate::scan_fs::package_db::PackageDbEntry;

const SOURCE_MECHANISM: &str = "waybill:source-mechanism";
/// The mechanisms the installed-library reader emits (spec 1531).
const RECORD_MECHANISMS: [&str; 2] = ["pkg-config", "cmake-package-version"];
const CURATED_EVIDENCE: &str = "embedded-version-string";

/// The verdict for one file a curated match was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Merge,
    VersionDisagrees,
    NameUnrelated,
    Ambiguous,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Verdict::Merge => "merge",
            Verdict::VersionDisagrees => "version-disagrees",
            Verdict::NameUnrelated => "name-unrelated",
            Verdict::Ambiguous => "ambiguous",
        }
    }
}

/// FR-005: equal, or the curated version followed by a `.` begins the
/// record's (`6.6` agrees with `6.6.20251230`, not with `6.60`).
pub(crate) fn versions_agree(curated: &str, record: &str) -> bool {
    curated == record || record.strip_prefix(curated).is_some_and(|rest| rest.starts_with('.'))
}

fn record_mechanism(e: &PackageDbEntry) -> Option<&str> {
    e.extra_annotations
        .get(SOURCE_MECHANISM)
        .and_then(|v| v.as_str())
        .filter(|m| RECORD_MECHANISMS.contains(m))
}

/// One file claimed by a curated entry and at least one record with a
/// different PURL.
#[derive(Debug)]
pub(crate) struct Finding {
    pub(crate) curated: usize,
    pub(crate) library: CuratedLibrary,
    pub(crate) file: String,
    /// Indices of the records claiming `file`, sorted by (mechanism, name,
    /// version).
    pub(crate) records: Vec<usize>,
    pub(crate) verdict: Verdict,
    /// The record to merge with, on `merge`.
    pub(crate) merge_with: Option<usize>,
}

impl Finding {
    /// The `shared_file` census value (contracts/wire.md).
    pub(crate) fn to_json(&self, entries: &[PackageDbEntry]) -> String {
        let c = &entries[self.curated];
        let records: Vec<serde_json::Value> = self
            .records
            .iter()
            .map(|&i| {
                let r = &entries[i];
                serde_json::json!({
                    "mechanism": record_mechanism(r),
                    "name": r.name,
                    "name_qualifies": self.library.names().contains(&r.name.as_str()),
                    "version": r.version,
                    "version_agrees": versions_agree(&c.version, &r.version),
                })
            })
            .collect();
        serde_json::json!({
            "curated": { "library": self.library.slug(), "version": c.version },
            "file": self.file,
            "records": records,
            "verdict": self.verdict.as_str(),
        })
        .to_string()
    }
}

/// Every shared file and its verdict, in entry order.
pub(crate) fn findings(entries: &[PackageDbEntry]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (ci, c) in entries.iter().enumerate() {
        if c.evidence_kind.as_deref() != Some(CURATED_EVIDENCE) {
            continue;
        }
        let Some(library) = CuratedLibrary::from_slug(&c.name) else { continue };
        let file = &c.source_path;
        let mut records: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                record_mechanism(r).is_some() && r.purl != c.purl && r.extra_source_paths.iter().any(|p| p == file)
            })
            .map(|(i, _)| i)
            .collect();
        if records.is_empty() {
            continue;
        }
        records.sort_by(|&a, &b| {
            let key = |i: usize| (record_mechanism(&entries[i]), &entries[i].name, &entries[i].version);
            key(a).cmp(&key(b))
        });
        let named: Vec<usize> =
            records.iter().copied().filter(|&i| library.names().contains(&entries[i].name.as_str())).collect();
        let agreeing: Vec<usize> =
            named.iter().copied().filter(|&i| versions_agree(&c.version, &entries[i].version)).collect();
        let verdict = match (named.is_empty(), agreeing.len()) {
            (true, _) => Verdict::NameUnrelated,
            (false, 0) => Verdict::VersionDisagrees,
            (false, 1) => Verdict::Merge,
            (false, _) => Verdict::Ambiguous,
        };
        let merge_with = if verdict == Verdict::Merge { agreeing.first().copied() } else { None };
        out.push(Finding { curated: ci, library, file: file.clone(), records, verdict, merge_with });
    }
    out
}

/// Spec 1645: log every shared file, and unless `log_only`, rewrite each
/// `merge` pair to one PURL. Returns the number of pairs rewritten.
pub(crate) fn reconcile(entries: &mut [PackageDbEntry], log_only: bool) -> usize {
    let found = findings(entries);
    if tracing::enabled!(target: "waybill::version_census", tracing::Level::DEBUG) {
        for f in &found {
            tracing::debug!(target: "waybill::version_census", shared_file = %f.to_json(entries));
        }
    }
    if log_only {
        return 0;
    }
    let mut renamed: BTreeMap<String, Purl> = BTreeMap::new();
    let mut merged = 0;
    for f in found.iter().filter(|f| f.verdict == Verdict::Merge) {
        let Some(r) = f.merge_with else { continue };
        let version = entries[r].version.clone();
        let raw = format!("pkg:generic/{}@{}", encode_purl_segment(f.library.slug()), encode_purl_segment(&version));
        let Ok(purl) = Purl::new(&raw) else {
            tracing::warn!(purl = %raw, "spec 1645: unparseable merged PURL; pair left as it was");
            continue;
        };
        for i in [r, f.curated] {
            if entries[i].purl != purl {
                renamed.insert(entries[i].purl.as_str().to_string(), purl.clone());
                entries[i].purl = purl.clone();
            }
        }
        entries[r].name = f.library.slug().to_string();
        entries[f.curated].version = version;
        merged += 1;
    }
    if !renamed.is_empty() {
        for e in entries.iter_mut() {
            for d in e.depends.iter_mut() {
                if let Some(p) = renamed.get(d.as_str()) {
                    *d = p.as_str().to_string();
                }
            }
        }
    }
    merged
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn entry(purl: &str, name: &str, version: &str, source: &str) -> PackageDbEntry {
        PackageDbEntry {
            extra_source_paths: Vec::new(),
            depends_ecosystem: None,
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: name.into(),
            version: version.into(),
            arch: None,
            source_path: source.into(),
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

    const LIB: &str = "/t/lib/libncursesw.so.6";

    fn curated(slug: &str, version: &str, file: &str) -> PackageDbEntry {
        let mut e = entry(&format!("pkg:generic/{slug}@{version}"), slug, version, file);
        e.evidence_kind = Some(CURATED_EVIDENCE.into());
        e
    }

    fn record(mechanism: &str, name: &str, version: &str, files: &[&str]) -> PackageDbEntry {
        let mut e = entry(&format!("pkg:generic/{name}@{version}"), name, version, &format!("/t/{name}.pc"));
        e.extra_annotations.insert(SOURCE_MECHANISM.into(), serde_json::json!(mechanism));
        e.extra_source_paths = files.iter().map(|f| f.to_string()).collect();
        e
    }

    fn verdicts(entries: &[PackageDbEntry]) -> Vec<Verdict> {
        findings(entries).into_iter().map(|f| f.verdict).collect()
    }

    #[test]
    fn s1645_versions_agree_at_a_dot() {
        assert!(versions_agree("6.6.20251230", "6.6.20251230"));
        assert!(versions_agree("6.6", "6.6.20251230"));
        assert!(versions_agree("2.7.5", "2.7.5.1"));
        assert!(!versions_agree("2.7.5", "2.7.50"));
        assert!(!versions_agree("23.0.0", "23.0.0git"));
        assert!(!versions_agree("23.0.0git", "23.0.0"));
        assert!(!versions_agree("6.6.20251230", "6.6"));
    }

    #[test]
    fn s1645_each_verdict() {
        let c = curated("ncurses", "6.6.20251230", LIB);
        let merge = [record("pkg-config", "ncursesw", "6.6.20251230", &[LIB]), c.clone()];
        assert_eq!(verdicts(&merge), [Verdict::Merge]);
        let disagree = [record("pkg-config", "ncursesw", "6.5.20240427", &[LIB]), c.clone()];
        assert_eq!(verdicts(&disagree), [Verdict::VersionDisagrees]);
        let unrelated = [record("pkg-config", "waybill-fixture-foo", "6.6.20251230", &[LIB]), c.clone()];
        assert_eq!(verdicts(&unrelated), [Verdict::NameUnrelated]);
        let two = [
            record("pkg-config", "ncursesw", "6.6.20251230", &[LIB]),
            record("cmake-package-version", "ncursestw", "6.6.20251230", &[LIB]),
            c.clone(),
        ];
        assert_eq!(verdicts(&two), [Verdict::Ambiguous]);
        // One agreeing record among a disagreeing one with a qualifying name.
        let one_of_two = [
            record("pkg-config", "ncursesw", "6.6.20251230", &[LIB]),
            record("cmake-package-version", "ncursestw", "6.5.20240427", &[LIB]),
            c.clone(),
        ];
        assert_eq!(verdicts(&one_of_two), [Verdict::Merge]);
    }

    #[test]
    fn s1645_what_is_not_a_finding() {
        let c = curated("ncurses", "6.6.20251230", LIB);
        // The record does not claim the file.
        assert!(verdicts(&[record("pkg-config", "ncursesw", "6.6.20251230", &["/t/lib/other.so"]), c.clone()]).is_empty());
        // Another reader's mechanism is not a record.
        assert!(verdicts(&[record("cmake-fetchcontent-git", "ncursesw", "6.6.20251230", &[LIB]), c.clone()]).is_empty());
        // Equal PURLs already merge; nothing to decide.
        assert!(verdicts(&[record("pkg-config", "ncurses", "6.6.20251230", &[LIB]), c.clone()]).is_empty());
        // A record without a curated entry on the file.
        assert!(verdicts(&[record("pkg-config", "ncursesw", "6.6.20251230", &[LIB])]).is_empty());
    }

    #[test]
    fn s1645_merge_rewrites_the_pair_and_its_edges() {
        let mut formw = record("pkg-config", "formw", "6.6.20251230", &["/t/lib/libformw.so.6"]);
        formw.depends = vec!["pkg:generic/ncursesw@6.6.20251230".into()];
        let mut e = vec![
            record("pkg-config", "ncursesw", "6.6.20251230", &[LIB]),
            formw,
            curated("ncurses", "6.6.20251230", LIB),
        ];
        assert_eq!(reconcile(&mut e, true), 0, "log-only rewrites nothing");
        assert_eq!(e[0].purl.as_str(), "pkg:generic/ncursesw@6.6.20251230");
        assert_eq!(reconcile(&mut e, false), 1);
        assert_eq!(e[0].purl.as_str(), "pkg:generic/ncurses@6.6.20251230");
        assert_eq!(e[0].name, "ncurses");
        assert_eq!(e[2].purl, e[0].purl);
        assert_eq!(e[1].depends, ["pkg:generic/ncurses@6.6.20251230"]);
    }

    #[test]
    fn s1645_a_prefix_merge_takes_the_record_version() {
        let file = "/t/lib/libexpat.so.1";
        let mut e = vec![record("pkg-config", "expat", "2.7.5.1", &[file]), curated("expat", "2.7.5", file)];
        assert_eq!(reconcile(&mut e, false), 1);
        assert_eq!(e[1].purl.as_str(), "pkg:generic/expat@2.7.5.1");
        assert_eq!(e[1].version, "2.7.5.1");
        assert_eq!(e[0].purl, e[1].purl);
    }

    #[test]
    fn s1645_no_rewrite_without_merge() {
        let c = curated("ncurses", "6.6.20251230", LIB);
        for r in [
            record("pkg-config", "ncursesw", "6.5.20240427", &[LIB]),
            record("pkg-config", "waybill-fixture-foo", "6.6.20251230", &[LIB]),
        ] {
            let before = r.purl.clone();
            let mut e = vec![r, c.clone()];
            assert_eq!(reconcile(&mut e, false), 0);
            assert_eq!(e[0].purl, before);
            assert_eq!(e[1].purl.as_str(), "pkg:generic/ncurses@6.6.20251230");
        }
        let mut two = vec![
            record("pkg-config", "ncursesw", "6.6.20251230", &[LIB]),
            record("cmake-package-version", "ncursestw", "6.6.20251230", &[LIB]),
            c.clone(),
        ];
        assert_eq!(reconcile(&mut two, false), 0, "ambiguous");
    }

    #[test]
    fn s1645_census_json() {
        let e = [record("pkg-config", "ncursesw", "6.6.20251230", &[LIB]), curated("ncurses", "6.6.20251230", LIB)];
        let f = findings(&e);
        let j = f[0].to_json(&e);
        for needle in [
            r#""curated":{"library":"ncurses","version":"6.6.20251230"}"#,
            r#""file":"/t/lib/libncursesw.so.6""#,
            r#""mechanism":"pkg-config""#,
            r#""name":"ncursesw""#,
            r#""name_qualifies":true"#,
            r#""version_agrees":true"#,
            r#""verdict":"merge""#,
        ] {
            assert!(j.contains(needle), "{needle} not in {j}");
        }
    }
}
