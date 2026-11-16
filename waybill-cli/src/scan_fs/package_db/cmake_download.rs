//! Declared downloads in CMake files, and the datasets among them (spec
//! 1535, #1565).
//!
//! `file(DOWNLOAD <url> …)` is read when its URL is a literal, or when its
//! only variable is an element of a literal list defined in the same file and
//! iterated by an enclosing `foreach` (FR-001): `foreach(v IN LISTS L)`,
//! `foreach(v a b c)`, or `foreach(i RANGE …)` with `list(GET L ${i} v)`.
//! Nothing else is evaluated.
//!
//! A download in such a loop is a **dataset** when a second literal list is
//! taken at the same index and checked by `file(MD5|SHA1|SHA256|SHA512 …)`
//! in the same loop (research R2: on TheRock's pinned sources this classifies
//! the eight sparse-matrix downloads and none of the three tool installers).
//! Every other resolved download is counted and emits nothing (clarified).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use waybill_common::types::hash::HashAlgorithm;

use crate::scan_fs::package_db::installed_lib::cmake_version::{commands, strip_comments};

/// How a download's URL resolves (data-model `DeclaredDownload`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolution {
    Literal(String),
    /// One element per list entry, in order; `var` is the URL's variable.
    ListLoop { var: String, elements: Vec<String> },
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredDownload {
    pub(crate) file: String,
    pub(crate) url_template: String,
    pub(crate) resolution: Resolution,
    /// Per-element digests, when a digest list is paired and checked.
    pub(crate) digests: Option<(HashAlgorithm, Vec<String>)>,
    /// A digest list was paired but its length differs from the elements'.
    pub(crate) digest_length_mismatch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Classification {
    Dataset,
    NotClassified,
    Unresolved,
}

pub(crate) fn classify(d: &DeclaredDownload) -> Classification {
    match (&d.resolution, &d.digests) {
        (Resolution::Unresolved, _) => Classification::Unresolved,
        (Resolution::ListLoop { .. }, Some(_)) => Classification::Dataset,
        _ => Classification::NotClassified,
    }
}

/// One dataset, keyed by its list element (research R3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatasetDecl {
    pub(crate) element: String,
    /// The first resolved URL.
    pub(crate) url: String,
    pub(crate) digest: Option<(HashAlgorithm, String)>,
    pub(crate) files: BTreeSet<String>,
}

/// C229 counts (data-model).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct DownloadCoverage {
    pub(crate) read: usize,
    pub(crate) unresolved: usize,
    pub(crate) not_classified: usize,
    pub(crate) datasets: usize,
    pub(crate) digest_mismatches: usize,
}

impl DownloadCoverage {
    /// Sorted-key JSON, or `None` when no download was read.
    pub(crate) fn wire(&self) -> Option<String> {
        (self.read > 0).then(|| {
            serde_json::json!({
                "datasets": self.datasets,
                "digest-mismatches": self.digest_mismatches,
                "not-classified": self.not_classified,
                "read": self.read,
                "unresolved": self.unresolved,
            })
            .to_string()
        })
    }
}

/// C229: the document-scope declared-download record.
pub(crate) const ANN_DECLARED_DOWNLOADS: &str = "waybill:declared-downloads";

fn hash_algorithm(word: &str) -> Option<HashAlgorithm> {
    match word.to_ascii_uppercase().as_str() {
        "MD5" => Some(HashAlgorithm::Md5),
        "SHA1" => Some(HashAlgorithm::Sha1),
        "SHA256" => Some(HashAlgorithm::Sha256),
        "SHA512" => Some(HashAlgorithm::Sha512),
        _ => None,
    }
}

/// Same-file `set(<L> <literal>…)`: lists with only literal values, set
/// exactly once. A list set twice is ambiguous without evaluation.
fn literal_lists(cmds: &[(String, Vec<String>)]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Option<Vec<String>>> = HashMap::new();
    for (name, args) in cmds {
        if name != "set" || args.len() < 2 {
            continue;
        }
        let values = &args[1..];
        let literal = values.iter().all(|v| !v.contains("${") && !v.contains("$<") && !v.contains("$ENV{"))
            && !values.iter().any(|v| v == "CACHE" || v == "PARENT_SCOPE");
        let slot = out.entry(args[0].clone()).or_insert(None);
        *slot = if slot.is_none() && literal { Some(values.to_vec()) } else { None };
        if !literal {
            out.insert(args[0].clone(), None);
        }
    }
    // A name seen twice was set to `None` on the second sight only if the
    // first was `Some`; recount to be exact.
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (name, args) in cmds {
        if name == "set" && !args.is_empty() {
            *seen.entry(args[0].as_str()).or_default() += 1;
        }
    }
    out.into_iter()
        .filter(|(k, _)| seen.get(k.as_str()) == Some(&1))
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

/// One `foreach` scope being read.
struct Frame {
    /// The loop variable: an element (`IN LISTS`, item list) or an index (`RANGE`).
    var: String,
    /// `IN LISTS` or inline items: the elements the variable takes.
    elements: Option<Vec<String>>,
    /// `list(GET <L> ${<index>} <v>)` bindings: v -> L.
    bound: BTreeMap<String, String>,
    downloads: Vec<(String, String)>, // (url template, file)
    algorithms: HashSet<HashAlgorithm>,
}

/// Every `file(DOWNLOAD …)` in one CMake file.
pub(crate) fn parse_downloads(content: &str, file: &str) -> Vec<DeclaredDownload> {
    let text = strip_comments(content);
    let cmds: Vec<(String, Vec<String>)> = commands(&text)
        .into_iter()
        .map(|(n, a)| (n.to_ascii_lowercase(), a.into_iter().map(|x| x.text).collect()))
        .collect();
    let lists = literal_lists(&cmds);
    let mut out = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    for (name, args) in &cmds {
        match name.as_str() {
            "foreach" if !args.is_empty() => {
                let var = args[0].clone();
                let rest = &args[1..];
                let elements = if rest.first().map(String::as_str) == Some("IN") {
                    // IN LISTS <L> (exactly one literal list) or IN ITEMS a b c.
                    match rest.get(1).map(String::as_str) {
                        Some("LISTS") if rest.len() == 3 => lists.get(&rest[2]).cloned(),
                        Some("ITEMS") => {
                            let items = rest[2..].to_vec();
                            items.iter().all(|i| !i.contains("${")).then_some(items)
                        }
                        _ => None,
                    }
                } else if rest.first().map(String::as_str) == Some("RANGE") {
                    None
                } else {
                    rest.iter().all(|i| !i.contains("${")).then(|| rest.to_vec())
                };
                stack.push(Frame { var, elements, bound: BTreeMap::new(), downloads: Vec::new(), algorithms: HashSet::new() });
            }
            "endforeach" => {
                if let Some(frame) = stack.pop() {
                    out.extend(close_frame(frame, &lists));
                }
            }
            "list" if args.len() >= 4 && args[0].eq_ignore_ascii_case("GET") => {
                if let Some(frame) = stack.last_mut() {
                    if args[2] == format!("${{{}}}", frame.var) && lists.contains_key(&args[1]) {
                        frame.bound.insert(args[3].clone(), args[1].clone());
                    }
                }
            }
            "file" if args.len() >= 2 => {
                if args[0].eq_ignore_ascii_case("DOWNLOAD") {
                    match stack.last_mut() {
                        Some(frame) => frame.downloads.push((args[1].clone(), file.to_string())),
                        None => out.push(resolve_top_level(&args[1], file)),
                    }
                } else if let (Some(alg), Some(frame)) = (hash_algorithm(&args[0]), stack.last_mut()) {
                    frame.algorithms.insert(alg);
                }
            }
            _ => {}
        }
    }
    // Unclosed loops (a truncated file): their downloads are unresolved.
    for frame in stack {
        for (url, f) in frame.downloads {
            out.push(unresolved(&url, &f));
        }
    }
    out
}

fn unresolved(url: &str, file: &str) -> DeclaredDownload {
    DeclaredDownload {
        file: file.to_string(),
        url_template: url.to_string(),
        resolution: Resolution::Unresolved,
        digests: None,
        digest_length_mismatch: false,
    }
}

fn variables(url: &str) -> Option<BTreeSet<String>> {
    if url.contains("$ENV{") || url.contains("$<") || url.contains("$CACHE{") {
        return None;
    }
    let mut out = BTreeSet::new();
    let mut rest = url;
    while let Some(i) = rest.find("${") {
        let after = &rest[i + 2..];
        let j = after.find('}')?;
        out.insert(after[..j].to_string());
        rest = &after[j + 1..];
    }
    Some(out)
}

fn resolve_top_level(url: &str, file: &str) -> DeclaredDownload {
    match variables(url) {
        Some(v) if v.is_empty() => DeclaredDownload {
            file: file.to_string(),
            url_template: url.to_string(),
            resolution: Resolution::Literal(url.to_string()),
            digests: None,
            digest_length_mismatch: false,
        },
        _ => unresolved(url, file),
    }
}

fn close_frame(frame: Frame, lists: &HashMap<String, Vec<String>>) -> Vec<DeclaredDownload> {
    let mut out = Vec::new();
    for (url, file) in &frame.downloads {
        let vars = match variables(url) {
            Some(v) => v,
            None => {
                out.push(unresolved(url, file));
                continue;
            }
        };
        if vars.is_empty() {
            out.push(resolve_top_level(url, file));
            continue;
        }
        if vars.len() != 1 {
            out.push(unresolved(url, file));
            continue;
        }
        let var = vars.into_iter().next().unwrap_or_default();
        // The element list the URL's variable walks, and its name when it is
        // a `list(GET)` binding (needed to find the paired digest list).
        let (elements, element_list): (Option<Vec<String>>, Option<&String>) = if var == frame.var {
            (frame.elements.clone(), None)
        } else if let Some(l) = frame.bound.get(&var) {
            (lists.get(l).cloned(), Some(l))
        } else {
            (None, None)
        };
        let Some(elements) = elements else {
            out.push(unresolved(url, file));
            continue;
        };
        // Digest pairing: another bound list at the same index, and exactly
        // one hash algorithm checked in this loop.
        let mut digests = None;
        let mut mismatch = false;
        if let Some(el) = element_list {
            let others: Vec<&String> = frame.bound.values().filter(|l| *l != el).collect();
            let distinct: BTreeSet<&&String> = others.iter().collect();
            if distinct.len() == 1 && frame.algorithms.len() == 1 {
                let hlist = &lists[others[0].as_str()];
                let alg = *frame.algorithms.iter().next().unwrap_or(&HashAlgorithm::Sha256);
                if hlist.len() == elements.len() {
                    digests = Some((alg, hlist.clone()));
                } else {
                    mismatch = true;
                }
            }
        }
        out.push(DeclaredDownload {
            file: file.clone(),
            url_template: url.clone(),
            resolution: Resolution::ListLoop { var, elements },
            digests,
            digest_length_mismatch: mismatch,
        });
    }
    out
}

/// Group the datasets by element (research R3) and count everything.
pub(crate) fn datasets(downloads: &[DeclaredDownload]) -> (Vec<DatasetDecl>, DownloadCoverage) {
    let mut cov = DownloadCoverage::default();
    let mut by_element: BTreeMap<String, DatasetDecl> = BTreeMap::new();
    let mut disagree: BTreeSet<String> = BTreeSet::new();
    for d in downloads {
        cov.read += 1;
        if d.digest_length_mismatch {
            cov.digest_mismatches += 1;
        }
        match classify(d) {
            Classification::Unresolved => cov.unresolved += 1,
            Classification::NotClassified => cov.not_classified += 1,
            Classification::Dataset => {
                let (Resolution::ListLoop { var, elements }, Some((alg, hashes))) = (&d.resolution, &d.digests) else {
                    continue;
                };
                for (i, el) in elements.iter().enumerate() {
                    let url = d.url_template.replace(&format!("${{{var}}}"), el);
                    let digest = hashes.get(i).map(|h| (*alg, h.to_ascii_lowercase()));
                    match by_element.get_mut(el) {
                        Some(existing) => {
                            existing.files.insert(d.file.clone());
                            if existing.digest != digest && !disagree.contains(el) {
                                existing.digest = None;
                                disagree.insert(el.clone());
                            }
                        }
                        None => {
                            by_element.insert(
                                el.clone(),
                                DatasetDecl { element: el.clone(), url, digest, files: BTreeSet::from([d.file.clone()]) },
                            );
                        }
                    }
                }
            }
        }
    }
    cov.digest_mismatches += disagree.len();
    cov.datasets = by_element.len();
    (by_element.into_values().collect(), cov)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// The rocSPARSE shape (research R1), with synthetic names.
    const ROCSPARSE_SHAPE: &str = r#"
set(TEST_MATRICES
  WaybillFixtureA/waybill-fixture-m1
  WaybillFixtureB/waybill-fixture-m2
  waybill-fixture-m3
)
set(TEST_MD5HASH
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC
)
list(LENGTH TEST_MATRICES len)
math(EXPR len1 "${len} - 1")
foreach(i RANGE 0 ${len1})
  list(GET TEST_MATRICES ${i} m)
  list(GET TEST_MD5HASH ${i} md5)
  if(DEFINED ENV{WAYBILL_FIXTURE_MIRROR})
    file(DOWNLOAD $ENV{WAYBILL_FIXTURE_MIRROR}/${m}.tar.gz ${CMAKE_MATRICES_DIR}/m.tar.gz)
  else()
    # Primary, then a fallback for the same element.
    file(DOWNLOAD https://example.invalid/MM/${m}.tar.gz ${CMAKE_MATRICES_DIR}/m.tar.gz STATUS DL)
    file(DOWNLOAD https://mirror.example.invalid/MM/${m}.tar.gz ${CMAKE_MATRICES_DIR}/m.tar.gz)
  endif()
  file(MD5 ${CMAKE_MATRICES_DIR}/m.tar.gz hash)
endforeach()
"#;

    #[test]
    fn the_index_loop_shape_resolves_with_digests() {
        let d = parse_downloads(ROCSPARSE_SHAPE, "cmake/m.cmake");
        assert_eq!(d.len(), 3, "{d:#?}");
        assert_eq!(classify(&d[0]), Classification::Unresolved, "$ENV mirror");
        assert_eq!(classify(&d[1]), Classification::Dataset);
        assert_eq!(classify(&d[2]), Classification::Dataset);
        let (alg, hashes) = d[1].digests.clone().unwrap();
        assert_eq!(alg, HashAlgorithm::Md5);
        assert_eq!(hashes.len(), 3);
        let (ds, cov) = datasets(&d);
        assert_eq!(ds.len(), 3, "the fallback joins the same element");
        assert_eq!(ds[0].element, "WaybillFixtureA/waybill-fixture-m1");
        assert_eq!(ds[0].url, "https://example.invalid/MM/WaybillFixtureA/waybill-fixture-m1.tar.gz");
        assert_eq!(ds[2].digest, Some((HashAlgorithm::Md5, "cccccccccccccccccccccccccccccccc".into())));
        assert_eq!(cov, DownloadCoverage { read: 3, unresolved: 1, not_classified: 0, datasets: 3, digest_mismatches: 0 });
    }

    #[test]
    fn in_lists_and_literal_items_resolve_without_digests() {
        let src = "set(L waybill-fixture-a waybill-fixture-b)\nforeach(x IN LISTS L)\n  file(DOWNLOAD https://e.invalid/${x}.bin x)\nendforeach()\nforeach(y waybill-fixture-c)\n  file(DOWNLOAD https://e.invalid/${y}.bin y)\nendforeach()\n";
        let d = parse_downloads(src, "CMakeLists.txt");
        assert_eq!(d[0].resolution, Resolution::ListLoop { var: "x".into(), elements: vec!["waybill-fixture-a".into(), "waybill-fixture-b".into()] });
        assert_eq!(d[1].resolution, Resolution::ListLoop { var: "y".into(), elements: vec!["waybill-fixture-c".into()] });
        // No paired digest list: resolved, not a dataset.
        assert!(d.iter().all(|x| classify(x) == Classification::NotClassified));
    }

    #[test]
    fn a_literal_download_is_resolved_but_not_classified() {
        let d = parse_downloads("file(DOWNLOAD https://e.invalid/tool-installer.sh ${CMAKE_BINARY_DIR}/i.sh)\n", "CMakeLists.txt");
        assert_eq!(d[0].resolution, Resolution::Literal("https://e.invalid/tool-installer.sh".into()));
        assert_eq!(classify(&d[0]), Classification::NotClassified);
    }

    #[test]
    fn what_never_resolves() {
        for src in [
            "file(DOWNLOAD ${URL_BASE}/a.tgz a)\n",
            "set(URL_BASE https://e.invalid CACHE STRING \"\")\nfile(DOWNLOAD ${URL_BASE}/a.tgz a)\n",
            "function(f u)\n file(DOWNLOAD ${u} a)\nendfunction()\n",
            "set(L a b)\nforeach(x IN LISTS L)\n file(DOWNLOAD https://${HOST}/${x} a)\nendforeach()\n",
            "set(L a)\nset(L b)\nforeach(x IN LISTS L)\n file(DOWNLOAD https://e.invalid/${x} a)\nendforeach()\n",
            "foreach(x IN LISTS L)\n file(DOWNLOAD https://e.invalid/${x} a)\n",
        ] {
            let d = parse_downloads(src, "f.cmake");
            assert_eq!(d.len(), 1, "{src}");
            assert_eq!(classify(&d[0]), Classification::Unresolved, "{src}");
        }
    }

    #[test]
    fn unequal_lists_pair_no_digest_and_count() {
        let src = ROCSPARSE_SHAPE.replace("  CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC\n", "");
        let d = parse_downloads(&src, "m.cmake");
        assert!(d[1].digests.is_none() && d[1].digest_length_mismatch);
        let (ds, cov) = datasets(&d);
        assert!(ds.is_empty());
        assert_eq!(cov.digest_mismatches, 2, "the primary and the fallback call each count");
    }

    #[test]
    fn two_files_one_element_and_disagreeing_digests() {
        let a = parse_downloads(ROCSPARSE_SHAPE, "cmake/a.cmake");
        let b = parse_downloads(&ROCSPARSE_SHAPE.replace("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "dddddddddddddddddddddddddddddddd"), "cmake/b.cmake");
        let all: Vec<DeclaredDownload> = a.into_iter().chain(b).collect();
        let (ds, cov) = datasets(&all);
        assert_eq!(ds.len(), 3);
        assert_eq!(ds[0].files.len(), 2);
        assert_eq!(ds[0].digest, None, "a disagreement records no digest");
        assert_eq!(ds[1].digest.as_ref().map(|d| d.1.as_str()), Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
        assert_eq!(cov.digest_mismatches, 1);
    }

    #[test]
    fn comments_are_ignored() {
        let d = parse_downloads("# file(DOWNLOAD https://e.invalid/x a)\nfile(DOWNLOAD https://e.invalid/y b)\n", "f.cmake");
        assert_eq!(d.len(), 1);
    }
}
