//! Tier 1: PEP 376 venv site-packages walker + METADATA parser.
//!
//! Authoritative source for what's actually installed on disk: each
//! `<name>-<version>.dist-info/METADATA` file is parsed into a
//! [`PipDistInfoEntry`], converted to [`PackageDbEntry`] at the
//! module boundary, and emitted by [`super::read`] before the
//! lockfile and requirements-file tiers run.
//!
//! Also exports [`collect_record_owners`]: which `.dist-info` owns each
//! file a `RECORD` lists (spec 1536). Callers reach it via the re-export in
//! `pip/mod.rs`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use waybill_common::types::license::SpdxExpression;
use waybill_common::types::purl::Purl;

use super::super::PackageDbEntry;
use super::extras::{active_extras, pep508_name, ExtraEdge};
use super::{build_pypi_purl_str, normalize_pypi_name_for_purl, tokenise_requires_dist_name};

// -----------------------------------------------------------------------
// Tier 1: venv dist-info walker
// -----------------------------------------------------------------------

/// Spec 1536 (#1567): the files each `.dist-info/RECORD` lists, mapped to
/// the `.dist-info` that lists them.
///
/// These used to go into the claim set the OS package databases share, and
/// a claimed file skipped binary analysis entirely. A Python package is not
/// the binary it ships: a wheel vendors third-party native code, which the
/// binary scan must still see. So a `RECORD` claim is kept apart. It names an
/// owner, and the binary scan analyses the file and nests the result under
/// that owner (`scan_fs::binary::owned`).
///
/// Lookup mirrors `binary::is_path_claimed`: the walker's path, then its
/// canonical form, then (Unix) its device and inode.
#[derive(Debug, Default, Clone)]
pub struct RecordOwners {
    by_path: std::collections::BTreeMap<PathBuf, PathBuf>,
    #[cfg(unix)]
    by_inode: std::collections::BTreeMap<(u64, u64), PathBuf>,
    files_by_owner: std::collections::BTreeMap<PathBuf, BTreeSet<String>>,
}

impl RecordOwners {
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    /// The `.dist-info` whose `RECORD` lists `walker_path`.
    pub fn owner_of(&self, walker_path: &Path) -> Option<&Path> {
        if let Some(o) = self.by_path.get(walker_path) {
            return Some(o);
        }
        if let Ok(canonical) = std::fs::canonicalize(walker_path) {
            if let Some(o) = self.by_path.get(&canonical) {
                return Some(o);
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(meta) = std::fs::metadata(walker_path) {
                if let Some(o) = self.by_inode.get(&(meta.dev(), meta.ino())) {
                    return Some(o);
                }
            }
        }
        None
    }

    /// Whether `owner`'s `RECORD` lists a file whose name is `file_name`.
    /// Internal links are matched this way: auditwheel and delocate point a
    /// vendored library at its sibling through an rpath, which the linkage
    /// probe does not follow, and the sibling is always in the same `RECORD`.
    pub fn owner_lists_file_named(&self, owner: &Path, file_name: &str) -> bool {
        self.files_by_owner.get(owner).is_some_and(|names| names.contains(file_name))
    }

    /// First insert wins. Callers insert `.dist-info` directories in sorted
    /// order, so two `RECORD`s listing one file resolve to the one that sorts
    /// first, whatever the scan order (FR-010).
    fn insert(
        &mut self,
        owner: &Path,
        abs_path: PathBuf,
        parent_cache: &mut std::collections::HashMap<PathBuf, Option<PathBuf>>,
    ) {
        if let (Some(parent), Some(basename)) = (abs_path.parent(), abs_path.file_name()) {
            let canonical_parent = parent_cache
                .entry(parent.to_path_buf())
                .or_insert_with(|| std::fs::canonicalize(parent).ok())
                .clone();
            if let Some(canonical_parent) = canonical_parent {
                let canonical = canonical_parent.join(basename);
                if canonical != abs_path {
                    self.by_path.entry(canonical).or_insert_with(|| owner.to_path_buf());
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(meta) = std::fs::metadata(&abs_path) {
                self.by_inode.entry((meta.dev(), meta.ino())).or_insert_with(|| owner.to_path_buf());
            }
        }
        if let Some(name) = abs_path.file_name().and_then(|n| n.to_str()) {
            self.files_by_owner.entry(owner.to_path_buf()).or_default().insert(name.to_string());
        }
        self.by_path.entry(abs_path).or_insert_with(|| owner.to_path_buf());
    }
}

/// Read every `RECORD` under every candidate `site-packages` directory into
/// a [`RecordOwners`].
///
/// RECORD format (PEP 376 / PEP 627): CSV whose first column is a path
/// relative to `site-packages`, with forward slashes. Entries may escape it
/// with `../` (scripts installed to `bin/`); that form is kept. An entry
/// naming a file that is not there is skipped. Malformed lines are tolerated.
pub fn collect_record_owners(rootfs: &Path) -> RecordOwners {
    let mut owners = RecordOwners::default();
    let mut parent_cache: std::collections::HashMap<PathBuf, Option<PathBuf>> =
        std::collections::HashMap::new();
    let mut dist_infos: Vec<(PathBuf, PathBuf)> = Vec::new();
    for site_packages in candidate_site_packages_roots(rootfs) {
        let Ok(read_dir) = std::fs::read_dir(&site_packages) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("dist-info") {
                dist_infos.push((p, site_packages.clone()));
            }
        }
    }
    dist_infos.sort();
    dist_infos.dedup();
    for (dist_info, site_packages) in dist_infos {
        let Ok(content) = std::fs::read_to_string(dist_info.join("RECORD")) else {
            continue;
        };
        for line in content.lines() {
            let Some(rel) = line.split(',').next().map(str::trim) else {
                continue;
            };
            if rel.is_empty() {
                continue;
            }
            let abs = site_packages.join(rel);
            if !abs.is_file() {
                continue;
            }
            owners.insert(&dist_info, abs, &mut parent_cache);
        }
    }
    owners
}

/// Walk candidate `site-packages/` locations under `rootfs` and parse
/// every `<name>-<version>.dist-info/METADATA` we find.
pub(super) fn read_venv_dist_info(rootfs: &Path) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    for root in candidate_site_packages_roots(rootfs) {
        if !root.is_dir() {
            continue;
        }
        let Ok(read_dir) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut names: Vec<PathBuf> = read_dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|ext| ext == "dist-info")
            })
            .collect();
        names.sort(); // determinism
        // #1163 — one site-packages directory is one installed set, so the
        // extras it activates are resolved across all of it before any
        // package's edges are built.
        let parsed: Vec<(PathBuf, PipDistInfoEntry)> = names
            .into_iter()
            .filter_map(|dist_info| {
                let metadata_path = dist_info.join("METADATA");
                let bytes = std::fs::read(&metadata_path).ok()?;
                Some((metadata_path, parse_metadata_bytes(&bytes)))
            })
            .collect();
        let graph = parsed
            .iter()
            .map(|(_, e)| {
                let edges = e.requires_dist.iter().filter_map(|r| ExtraEdge::from_pep508(r)).collect();
                (normalize_pypi_name_for_purl(&e.name), edges)
            })
            .collect();
        // No seeds: METADATA does not record which extras the top-level
        // install asked for, only those one installed package asks of another.
        let active = active_extras([], &graph);
        // #1529: the only package at the scan root itself (an unpacked
        // wheel) is the project, so it is the main module (C40).
        let is_project = root == rootfs && parsed.len() == 1;
        for (metadata_path, entry) in parsed {
            let own = active.get(&normalize_pypi_name_for_purl(&entry.name)).cloned();
            if let Some(mut entry) =
                entry.into_package_db_entry(metadata_path.to_string_lossy().into_owned(), own.as_ref())
            {
                if is_project {
                    entry.extra_annotations.insert(
                        "waybill:component-role".to_string(),
                        serde_json::Value::String("main-module".to_string()),
                    );
                }
                out.push(entry);
            }
        }
    }
    out
}

/// Enumerate likely `site-packages/` locations. Covers:
/// - `<root>/.venv/lib/python*/site-packages/`
/// - `<root>/venv/lib/python*/site-packages/`
/// - `<root>/usr/lib/python*/dist-packages/` (Debian system python)
/// - `<root>/usr/lib/python*/site-packages/`
/// - `<root>/usr/local/lib/python*/site-packages/`
/// - `<root>/opt/app/.venv/lib/python*/site-packages/`
/// - `<root>` itself, when it directly holds a `*.dist-info` directory: an
///   unpacked wheel has a site-packages layout (#1529).
pub(crate) fn candidate_site_packages_roots(rootfs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if holds_dist_info(rootfs) {
        out.push(rootfs.to_path_buf());
    }
    for venv in [".venv", "venv"] {
        out.extend(find_site_packages_under(&rootfs.join(venv).join("lib")));
    }
    for sys in ["usr/lib", "usr/local/lib", "opt/app/.venv/lib"] {
        out.extend(find_site_packages_under(&rootfs.join(sys)));
    }
    out
}

fn holds_dist_info(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.path().extension().is_some_and(|ext| ext == "dist-info") && e.path().is_dir()
        })
    })
}

/// `<base>/python3.X/site-packages/` OR `<base>/python3.X/dist-packages/`.
/// Glob-equivalent for `python*/site-packages/` and `python*/dist-packages/`.
fn find_site_packages_under(base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !base.is_dir() {
        return out;
    }
    let Ok(read_dir) = std::fs::read_dir(base) else {
        return out;
    };
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.starts_with("python") {
            continue;
        }
        for leaf in ["site-packages", "dist-packages"] {
            let candidate = entry.path().join(leaf);
            if candidate.is_dir() {
                out.push(candidate);
            }
        }
    }
    out
}

// -----------------------------------------------------------------------
// Tier 1 support: PipDistInfoEntry + METADATA parser
// -----------------------------------------------------------------------

/// Parsed content of a single `dist-info/METADATA` file. Intermediate
/// representation; converted to [`PackageDbEntry`] at the module
/// boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PipDistInfoEntry {
    pub name: String,
    pub version: String,
    pub license_expression: Option<String>, // PEP 639 `License-Expression:`
    pub license_raw: Option<String>,        // legacy `License:`
    pub classifiers: Vec<String>,           // `Classifier: License :: ...`
    pub requires_dist: Vec<String>,         // raw PEP 508 strings
    pub author: Option<String>,
    pub author_email: Option<String>,
}

impl PipDistInfoEntry {
    /// Convert to the scan-pipeline's `PackageDbEntry` shape.
    /// `active` is the set of this package's extras that are requested
    /// within its installed set (`extras::active_extras`).
    fn into_package_db_entry(
        self,
        source_path: String,
        active: Option<&BTreeSet<String>>,
    ) -> Option<PackageDbEntry> {
        if self.name.is_empty() || self.version.is_empty() {
            return None;
        }
        let purl_str = build_pypi_purl_str(&self.name, &self.version);
        let purl = Purl::new(&purl_str).ok()?;

        // Author / Author-email concatenated into supplier text.
        let supplier = match (&self.author, &self.author_email) {
            (Some(n), Some(e)) if !e.is_empty() => Some(format!("{n} <{e}>")),
            (Some(n), _) => Some(n.clone()),
            (None, Some(e)) if !e.is_empty() => Some(e.clone()),
            _ => None,
        };

        // Depends: bare names from Requires-Dist (PEP 508 tokenised). #1163:
        // an extras-gated entry is a dependency when its extra is active.
        let depends = self
            .requires_dist
            .iter()
            .filter_map(|raw| match ExtraEdge::from_pep508(raw) {
                Some(edge) if !edge.gated_by.is_empty() => {
                    edge.is_active(active).then(|| pep508_name(raw).to_string())
                }
                _ => tokenise_requires_dist_name(raw),
            })
            .collect();

        let licenses = extract_license(&self);

        Some(PackageDbEntry {
            extra_source_paths: Vec::new(),
            depends_ecosystem: None,
            build_inclusion: None,
            purl,
            name: self.name,
            version: self.version,
            arch: None,
            source_path,
            depends,
            maintainer: supplier,
            licenses,
            lifecycle_scope: None, // venv dist-info doesn't carry a dev/prod marker
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
            extra_annotations: Default::default(),
            binary_role: None,
        })
    }
}

// -----------------------------------------------------------------------
// Tier 1 support: license precedence ladder (PEP 639 → legacy → classifier)
// -----------------------------------------------------------------------

/// Resolve license(s) from a `PipDistInfoEntry` per research.md R2.
/// Precedence: `License-Expression:` (PEP 639) → `License:` (legacy
/// free-form, passed through the copyright-shorthand normaliser) →
/// `Classifier: License ::` trove list. Returns at most one SPDX
/// expression from the first tier that produces a canonicalisable
/// value; empty Vec when nothing resolves.
pub(crate) fn extract_license(entry: &PipDistInfoEntry) -> Vec<SpdxExpression> {
    // Tier 1: PEP 639 — authoritative, SPDX by construction.
    if let Some(ref raw) = entry.license_expression {
        if let Ok(spdx) = SpdxExpression::try_canonical(raw.trim()) {
            return vec![spdx];
        }
    }
    // Tier 2: legacy `License:` free-form. Try canonical first.
    if let Some(ref raw) = entry.license_raw {
        let cleaned = raw.trim();
        if !cleaned.is_empty() {
            if let Ok(spdx) = SpdxExpression::try_canonical(cleaned) {
                return vec![spdx];
            }
        }
    }
    // Tier 3: classifier trove → SPDX lookup.
    for c in &entry.classifiers {
        if let Some(spdx) = classifier_to_spdx(c) {
            if let Ok(expr) = SpdxExpression::try_canonical(spdx) {
                return vec![expr];
            }
        }
    }
    Vec::new()
}

/// Map a `Classifier: License :: ...` trove string to the closest
/// canonical SPDX identifier. Table covers >95% of real-world PyPI
/// packages per research R2. Unknown classifiers return None.
pub(crate) fn classifier_to_spdx(classifier: &str) -> Option<&'static str> {
    // Classifiers look like "License :: OSI Approved :: MIT License".
    // Care about the last segment only.
    let last = classifier.rsplit("::").next()?.trim();
    Some(match last {
        "MIT License" => "MIT",
        "Apache Software License" => "Apache-2.0",
        "BSD License" => "BSD-3-Clause",
        "BSD 3-Clause" | "BSD 3-Clause License" => "BSD-3-Clause",
        "BSD 2-Clause" | "BSD 2-Clause License" => "BSD-2-Clause",
        "Mozilla Public License 2.0 (MPL 2.0)" => "MPL-2.0",
        "ISC License (ISCL)" => "ISC",
        "zlib/libpng License" => "Zlib",
        "Python Software Foundation License" => "Python-2.0",
        "GNU General Public License v2 (GPLv2)" => "GPL-2.0-only",
        "GNU General Public License v2 or later (GPLv2+)" => "GPL-2.0-or-later",
        "GNU General Public License v3 (GPLv3)" => "GPL-3.0-only",
        "GNU General Public License v3 or later (GPLv3+)" => "GPL-3.0-or-later",
        "GNU Lesser General Public License v2 (LGPLv2)" => "LGPL-2.0-only",
        "GNU Lesser General Public License v2 or later (LGPLv2+)" => "LGPL-2.0-or-later",
        "GNU Lesser General Public License v3 (LGPLv3)" => "LGPL-3.0-only",
        "GNU Lesser General Public License v3 or later (LGPLv3+)" => "LGPL-3.0-or-later",
        "GNU Affero General Public License v3" => "AGPL-3.0-only",
        "GNU Affero General Public License v3 or later (AGPLv3+)" => "AGPL-3.0-or-later",
        "Artistic License" => "Artistic-2.0",
        "The Unlicense (Unlicense)" => "Unlicense",
        _ => return None,
    })
}

/// Parse a METADATA file body. Tolerates non-UTF-8 by falling back to
/// lossy decoding (consistent with `copyright.rs` convention).
pub(crate) fn parse_metadata_bytes(bytes: &[u8]) -> PipDistInfoEntry {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    };
    parse_metadata_text(&text)
}

/// Parse METADATA as RFC-822-style stanzas with continuation-line
/// support: a line starting with a space/tab continues the previous
/// key's value. Multiple occurrences of the same key collect into a
/// list (e.g. `Classifier:` appears many times).
pub(crate) fn parse_metadata_text(text: &str) -> PipDistInfoEntry {
    let mut out = PipDistInfoEntry::default();

    // Only the header block matters; stop at the first blank line
    // which separates headers from the payload (description / README).
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    let flush = |key_opt: &mut Option<String>, value: &mut String, out: &mut PipDistInfoEntry| {
        if let Some(key) = key_opt.take() {
            let v = std::mem::take(value).trim().to_string();
            match key.as_str() {
                "Name" => out.name = v,
                "Version" => out.version = v,
                "License" if !v.is_empty() => out.license_raw = Some(v),
                "License-Expression" if !v.is_empty() => out.license_expression = Some(v),
                "Classifier" if !v.is_empty() => out.classifiers.push(v),
                "Requires-Dist" if !v.is_empty() => out.requires_dist.push(v),
                "Author" if !v.is_empty() => out.author = Some(v),
                "Author-email" if !v.is_empty() => out.author_email = Some(v),
                _ => {}
            }
        }
    };

    for line in text.lines() {
        if line.is_empty() {
            // End of headers; stop parsing.
            break;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line — append to current value with a space.
            if current_key.is_some() {
                if !current_value.is_empty() {
                    current_value.push(' ');
                }
                current_value.push_str(line.trim());
            }
            continue;
        }
        // Fresh header.
        flush(&mut current_key, &mut current_value, &mut out);
        if let Some(idx) = line.find(':') {
            let (key, rest) = line.split_at(idx);
            current_key = Some(key.to_string());
            current_value = rest[1..].trim().to_string();
        }
    }
    flush(&mut current_key, &mut current_value, &mut out);
    out
}


#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
#[cfg_attr(test, allow(clippy::field_reassign_with_default))]
mod tests {
    use std::fs;

    use super::super::read;
    use super::*;

    /// Spec 1536 (#1567): a site-packages tree with `.dist-info`s, each
    /// `(dir name, RECORD body)`, plus files to create (relative to
    /// site-packages, `../` allowed).
    fn record_tree(dist_infos: &[(&str, &str)], files: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let sp = dir.path().join(".venv/lib/python3.12/site-packages");
        fs::create_dir_all(&sp).unwrap();
        for (name, record) in dist_infos {
            fs::create_dir_all(sp.join(name)).unwrap();
            fs::write(sp.join(name).join("RECORD"), record).unwrap();
        }
        for f in files {
            let p = sp.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, f.as_bytes()).unwrap();
        }
        (dir, sp)
    }

    #[test]
    fn record_owners_map_each_listed_file_to_its_dist_info() {
        let (dir, sp) = record_tree(
            &[("waybill_fixture_a-1.0.dist-info", "waybill_fixture_a/.dylibs/libz.dylib,sha256=x,1\n")],
            &["waybill_fixture_a/.dylibs/libz.dylib", "waybill_fixture_a/unlisted.so"],
        );
        let owners = collect_record_owners(dir.path());
        let owner = sp.join("waybill_fixture_a-1.0.dist-info");
        assert_eq!(owners.owner_of(&sp.join("waybill_fixture_a/.dylibs/libz.dylib")), Some(owner.as_path()));
        assert_eq!(owners.owner_of(&sp.join("waybill_fixture_a/unlisted.so")), None);
        assert!(owners.owner_lists_file_named(&owner, "libz.dylib"));
        assert!(!owners.owner_lists_file_named(&owner, "unlisted.so"));
    }

    /// FR-010: two `RECORD`s listing one file resolve to the `.dist-info`
    /// that sorts first, whichever is read first.
    #[test]
    fn record_owners_first_sorted_dist_info_wins() {
        let rec = "shared/libshared.so,,\n";
        let (dir, sp) = record_tree(
            &[("waybill_fixture_b-1.0.dist-info", rec), ("waybill_fixture_a-1.0.dist-info", rec)],
            &["shared/libshared.so"],
        );
        let owners = collect_record_owners(dir.path());
        assert_eq!(
            owners.owner_of(&sp.join("shared/libshared.so")),
            Some(sp.join("waybill_fixture_a-1.0.dist-info").as_path())
        );
    }

    /// pip installs scripts through `../`-relative entries; they are owned.
    #[test]
    fn record_owners_keep_entries_outside_the_package() {
        let (dir, sp) = record_tree(
            &[("waybill_fixture_a-1.0.dist-info", "../../../bin/waybill-fixture-tool,,\n")],
            &["../../../bin/waybill-fixture-tool"],
        );
        let owners = collect_record_owners(dir.path());
        assert_eq!(
            owners.owner_of(&dir.path().join(".venv/bin/waybill-fixture-tool")),
            Some(sp.join("waybill_fixture_a-1.0.dist-info").as_path()),
            "the walker reaches the script by its own path, not the ../ form"
        );
    }

    #[test]
    fn record_owners_skip_empty_and_missing_entries() {
        let (dir, _sp) = record_tree(
            &[("waybill_fixture_a-1.0.dist-info", ",,\n  ,x,y\nwaybill_fixture_a/gone.so,,\n")],
            &[],
        );
        let owners = collect_record_owners(dir.path());
        assert!(owners.is_empty(), "an empty path and a missing file are both skipped");
    }

    #[cfg(unix)]
    #[test]
    fn record_owners_match_by_canonical_path_and_by_inode() {
        let (dir, sp) = record_tree(
            &[("waybill_fixture_a-1.0.dist-info", "waybill_fixture_a/libz.so,,\n")],
            &["waybill_fixture_a/libz.so"],
        );
        let owner = sp.join("waybill_fixture_a-1.0.dist-info");
        // A symlinked parent: the walker's path differs from the RECORD's.
        std::os::unix::fs::symlink(&sp, dir.path().join("sp-link")).unwrap();
        // A hard link elsewhere: only the inode matches.
        fs::hard_link(sp.join("waybill_fixture_a/libz.so"), dir.path().join("libz-hard.so")).unwrap();
        let owners = collect_record_owners(dir.path());
        assert_eq!(owners.owner_of(&dir.path().join("sp-link/waybill_fixture_a/libz.so")), Some(owner.as_path()));
        assert_eq!(owners.owner_of(&dir.path().join("libz-hard.so")), Some(owner.as_path()));
    }
    fn make_venv_rootfs(
        packages: &[(&str, &str, &str)],
    ) -> tempfile::TempDir {
        // packages: [(name, version, metadata_body)]
        let dir = tempfile::tempdir().expect("tempdir");
        let sp = dir.path().join(".venv/lib/python3.12/site-packages");
        fs::create_dir_all(&sp).unwrap();
        for (name, version, body) in packages {
            let subdir = sp.join(format!("{name}-{version}.dist-info"));
            fs::create_dir_all(&subdir).unwrap();
            fs::write(subdir.join("METADATA"), body).unwrap();
        }
        dir
    }

    #[test]
    fn parse_metadata_single_package_basics() {
        let body = "\
Metadata-Version: 2.1
Name: requests
Version: 2.31.0
Summary: Python HTTP for Humans.
Home-page: https://requests.readthedocs.io
Author: Kenneth Reitz
Author-email: me@kennethreitz.org
License: Apache 2.0
Requires-Dist: charset-normalizer<4,>=2
Requires-Dist: idna<4,>=2.5
Requires-Dist: urllib3<3,>=1.21.1
Requires-Dist: certifi>=2017.4.17
Classifier: License :: OSI Approved :: Apache Software License
Classifier: Programming Language :: Python :: 3
";
        let e = parse_metadata_text(body);
        assert_eq!(e.name, "requests");
        assert_eq!(e.version, "2.31.0");
        assert_eq!(e.author.as_deref(), Some("Kenneth Reitz"));
        assert_eq!(e.author_email.as_deref(), Some("me@kennethreitz.org"));
        assert_eq!(e.license_raw.as_deref(), Some("Apache 2.0"));
        assert_eq!(e.requires_dist.len(), 4);
        assert_eq!(e.classifiers.len(), 2);
    }

    #[test]
    fn parse_metadata_with_continuation_lines() {
        let body = "\
Name: multiline-pkg
Version: 0.1.0
License: Some custom license
 that spans multiple lines
 with indentation.
";
        let e = parse_metadata_text(body);
        assert_eq!(e.name, "multiline-pkg");
        let raw = e.license_raw.expect("license present");
        assert!(raw.contains("Some custom license"));
        assert!(raw.contains("multiple lines"));
    }

    #[test]
    fn parse_metadata_pep_639_license_expression_wins() {
        let body = "\
Name: modern-pkg
Version: 2.0.0
License-Expression: Apache-2.0 OR MIT
License: legacy-free-form
";
        let e = parse_metadata_text(body);
        assert_eq!(e.license_expression.as_deref(), Some("Apache-2.0 OR MIT"));
        assert_eq!(e.license_raw.as_deref(), Some("legacy-free-form"));
    }

    #[test]
    fn parse_metadata_stops_at_first_blank_line() {
        let body = "\
Name: pkg
Version: 1.0

This is the description; it should NOT be parsed as a header.
License: Ignored
";
        let e = parse_metadata_text(body);
        assert_eq!(e.name, "pkg");
        assert!(e.license_raw.is_none());
    }

    #[test]
    fn parse_metadata_non_utf8_author_falls_back_losslessly() {
        // Mixed UTF-8 + an invalid byte in the author field.
        let mut body = b"Name: q\nVersion: 1.0\nAuthor: Jose \xE9 Surname\nLicense: MIT\n".to_vec();
        let _ = &mut body; // keep mutable reference simple
        let e = parse_metadata_bytes(&body);
        assert_eq!(e.name, "q");
        assert_eq!(e.license_raw.as_deref(), Some("MIT"));
        // Author present (lossy-decoded); we don't care about the exact bytes.
        assert!(e.author.is_some());
    }

    #[test]
    fn venv_walk_finds_packages_in_standard_location() {
        let dir = make_venv_rootfs(&[
            ("requests", "2.31.0", "Name: requests\nVersion: 2.31.0\nLicense: Apache 2.0\n"),
            ("urllib3", "2.0.7", "Name: urllib3\nVersion: 2.0.7\nLicense: MIT\n"),
        ]);
        let out = read(dir.path(), false, &Default::default());
        assert_eq!(out.len(), 2);
        // Deterministic alphabetical order by dist-info dir name.
        assert_eq!(out[0].name, "requests");
        assert_eq!(out[1].name, "urllib3");
        assert!(out
            .iter()
            .all(|e| e.sbom_tier.as_deref() == Some("deployed")));
    }

    #[test]
    fn pyproject_only_project_emits_main_module_and_manifest_declared_deps() {
        // Bare pyproject.toml; no venv, no lockfile, no requirements.
        //
        // Timeline of this test's expected shape:
        //   * Pre-m068: emitted zero components (m018 "pyproject-only =
        //     bloat" policy).
        //   * m068 through m669: emitted exactly one main-module component
        //     for the project itself; declared deps stayed suppressed.
        //   * m670 (PR-1): reverses the m018 policy — declared deps in
        //     `[project.dependencies]` ARE emitted as design-tier
        //     components with `version = "unresolved"` and a m236
        //     `waybill:unresolved-reason` annotation surfacing the
        //     boundary. Total emitted = 1 main-module + N declared deps.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"myapp\"\nversion = \"0.1.0\"\ndependencies = [\"requests\"]\n",
        )
        .unwrap();
        let out = read(dir.path(), false, &Default::default());
        assert_eq!(
            out.len(),
            2,
            "m670: main-module + one declared dep (`requests`)"
        );
        let main_module = out
            .iter()
            .find(|e| e.name == "myapp")
            .expect("main-module component present");
        assert_eq!(main_module.purl.as_str(), "pkg:pypi/myapp@0.1.0");
        assert_eq!(
            main_module
                .extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
            "milestone 068: pyproject-only emits a C40-tagged main-module"
        );
        let requests = out
            .iter()
            .find(|e| e.name == "requests")
            .expect("m670: declared dep `requests` emitted as design-tier");
        assert_eq!(requests.purl.as_str(), "pkg:pypi/requests@unresolved");
        assert_eq!(requests.sbom_tier.as_deref(), Some("design"));
        assert_eq!(
            requests
                .extra_annotations
                .get("waybill:unresolved-reason")
                .and_then(|v| v.as_str()),
            Some(super::super::MANIFEST_UNRESOLVED_REASON)
        );
    }

    #[test]
    fn dist_info_with_mixed_case_name_emits_lowercase_purl() {
        // Real-world dist-info carries the declared name (may be mixed
        // case). The PURL must canonicalise but `component.name` (via
        // the entry's `name` field) preserves the declared form.
        let dir = make_venv_rootfs(&[(
            "Flask",
            "3.0.0",
            "Name: Flask\nVersion: 3.0.0\nLicense: BSD-3-Clause\n",
        )]);
        let out = read(dir.path(), false, &Default::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "Flask", "component.name keeps declared form");
        assert_eq!(
            out[0].purl.as_str(),
            "pkg:pypi/flask@3.0.0",
            "PURL must be lowercase per packageurl-python reference impl"
        );
    }

    #[test]
    fn license_pep_639_expression_wins() {
        let mut e = PipDistInfoEntry::default();
        e.license_expression = Some("Apache-2.0 OR MIT".into());
        e.license_raw = Some("Apache 2.0".into());
        e.classifiers.push("License :: OSI Approved :: MIT License".into());
        let out = extract_license(&e);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_str(), "Apache-2.0 OR MIT");
    }

    #[test]
    fn license_legacy_raw_tried_when_pep_639_absent() {
        let mut e = PipDistInfoEntry::default();
        e.license_raw = Some("MIT".into());
        let out = extract_license(&e);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_str(), "MIT");
    }

    #[test]
    fn license_classifier_fallback_maps_to_spdx() {
        let mut e = PipDistInfoEntry::default();
        e.classifiers.push("License :: OSI Approved :: Apache Software License".into());
        let out = extract_license(&e);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_str(), "Apache-2.0");
    }

    #[test]
    fn license_all_tiers_empty_returns_empty() {
        let e = PipDistInfoEntry::default();
        assert!(extract_license(&e).is_empty());
    }

    #[test]
    fn classifier_lookup_covers_common_licenses() {
        assert_eq!(
            classifier_to_spdx("License :: OSI Approved :: MIT License"),
            Some("MIT")
        );
        assert_eq!(
            classifier_to_spdx("License :: OSI Approved :: GNU General Public License v3 or later (GPLv3+)"),
            Some("GPL-3.0-or-later")
        );
        assert_eq!(
            classifier_to_spdx("License :: OSI Approved :: BSD License"),
            Some("BSD-3-Clause")
        );
    }

    #[test]
    fn classifier_lookup_returns_none_for_unknown() {
        assert_eq!(classifier_to_spdx("License :: Other/Proprietary License"), None);
        assert_eq!(classifier_to_spdx("Topic :: Software Development"), None);
    }

    /// #1163 — an extras-gated Requires-Dist is a dependency when another
    /// installed package requests the extra, and only then.
    #[test]
    fn requested_extra_makes_its_requires_dist_a_dependency() {
        let requests = "\
Name: requests
Version: 2.31.0
Requires-Dist: certifi >= 2017.4.17
Requires-Dist: PySocks!=1.5.7,>=1.5.6; extra == \"socks\"
Requires-Dist: chardet<6,>=3.0.2; extra == \"use-chardet-on-py3\"
";
        let alone = make_venv_rootfs(&[("requests", "2.31.0", requests)]);
        let out = read(alone.path(), false, &Default::default());
        assert_eq!(out[0].depends, vec!["certifi".to_string()], "nothing requests an extra");

        let httpx = "\
Name: docker
Version: 7.1.0
Requires-Dist: requests[Socks] >= 2.26.0
";
        let both = make_venv_rootfs(&[("requests", "2.31.0", requests), ("docker", "7.1.0", httpx)]);
        let out = read(both.path(), false, &Default::default());
        let req = out.iter().find(|e| e.name == "requests").unwrap();
        assert_eq!(req.depends, vec!["certifi".to_string(), "PySocks".to_string()]);
    }

    #[test]
    fn requires_dist_tokenisation_drives_depends() {
        let body = "\
Name: requests
Version: 2.31.0
Requires-Dist: urllib3 < 3, >= 1.21.1
Requires-Dist: certifi >= 2017.4.17
Requires-Dist: charset-normalizer < 4, >= 2
Requires-Dist: pytest ; extra == 'dev'
";
        let dir = make_venv_rootfs(&[("requests", "2.31.0", body)]);
        let out = read(dir.path(), false, &Default::default());
        assert_eq!(out.len(), 1);
        let e = &out[0];
        // extra-marker dep dropped, rest emitted.
        assert!(e.depends.contains(&"urllib3".to_string()));
        assert!(e.depends.contains(&"certifi".to_string()));
        assert!(e.depends.contains(&"charset-normalizer".to_string()));
        assert!(!e.depends.contains(&"pytest".to_string()));
    }

    /// #1529: an unpacked wheel has a site-packages layout. Scanned at its
    /// own root, its `.dist-info` identifies the package.
    #[test]
    fn s1529_unpacked_wheel_at_scan_root_is_read() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("waybill_fixture_pkg")).unwrap();
        fs::write(dir.path().join("waybill_fixture_pkg/__init__.py"), "").unwrap();
        let di = dir.path().join("waybill_fixture_pkg-1.2.3.dist-info");
        fs::create_dir_all(&di).unwrap();
        fs::write(
            di.join("METADATA"),
            "Metadata-Version: 2.1\nName: waybill-fixture-pkg\nVersion: 1.2.3\n",
        )
        .unwrap();
        fs::write(di.join("WHEEL"), "Wheel-Version: 1.0\n").unwrap();
        fs::write(
            di.join("RECORD"),
            "waybill_fixture_pkg/__init__.py,,\nwaybill_fixture_pkg-1.2.3.dist-info/METADATA,,\n",
        )
        .unwrap();
        let out = read(dir.path(), false, &Default::default());
        let pypi: Vec<_> = out.iter().filter(|e| e.purl.as_str().starts_with("pkg:pypi/")).collect();
        assert_eq!(pypi.len(), 1, "{out:?}");
        assert_eq!(pypi[0].purl.as_str(), "pkg:pypi/waybill-fixture-pkg@1.2.3");
        // The only package at the scan root is the project itself (C40).
        assert_eq!(
            pypi[0].extra_annotations.get("waybill:component-role").and_then(|v| v.as_str()),
            Some("main-module"),
        );
    }

    /// #1529 control: two `.dist-info` directories at the scan root (a
    /// site-packages directory scanned directly) are installed packages,
    /// and neither is the project.
    #[test]
    fn s1529_two_dist_infos_at_root_have_no_main_module() {
        let dir = tempfile::tempdir().unwrap();
        for (name, version) in [("waybill_fixture_a", "1.0.0"), ("waybill_fixture_b", "2.0.0")] {
            let di = dir.path().join(format!("{name}-{version}.dist-info"));
            fs::create_dir_all(&di).unwrap();
            fs::write(
                di.join("METADATA"),
                format!("Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n"),
            )
            .unwrap();
        }
        let out = read(dir.path(), false, &Default::default());
        let pypi: Vec<_> = out.iter().filter(|e| e.purl.as_str().starts_with("pkg:pypi/")).collect();
        assert_eq!(pypi.len(), 2, "{out:?}");
        assert!(pypi.iter().all(|e| !e.extra_annotations.contains_key("waybill:component-role")));
    }

    /// #1529 control: a `.dist-info` deeper than the scan root's own
    /// directory is not a site-packages root by this rule.
    #[test]
    fn s1529_nested_dist_info_outside_site_packages_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let di = dir.path().join("vendor/waybill_fixture_pkg-1.2.3.dist-info");
        fs::create_dir_all(&di).unwrap();
        fs::write(
            di.join("METADATA"),
            "Metadata-Version: 2.1\nName: waybill-fixture-pkg\nVersion: 1.2.3\n",
        )
        .unwrap();
        let out = read(dir.path(), false, &Default::default());
        assert!(out.iter().all(|e| !e.purl.as_str().starts_with("pkg:pypi/")), "{out:?}");
    }
}
