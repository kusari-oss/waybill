//! Tier 4: requirements*.txt parser (legacy / heterogeneous).
//!
//! Reads any `requirements*.txt` at the project root. Lower-tier source
//! than venv / lockfile because range specs may resolve to different
//! versions at install time; entries get `sbom_tier = "design"` when
//! the version is unpinned.

use std::path::{Path, PathBuf};
use waybill_common::types::purl::Purl;

use waybill_common::types::hash::{ContentHash, HashAlgorithm};

use super::super::PackageDbEntry;
use super::{build_pypi_purl_str, tokenise_requires_dist_name};

/// Milestone 670 T013 — FR-005a lifecycle-scope classification for a
/// discovered `requirements*.txt` file. Filename + parent-directory
/// heuristic; parent-dir signal wins over filename when both match.
///
/// Priority (per `contracts/requirements_txt.md` §Scope-tag derivation):
/// 1. Parent-dir name ∈ {`docs`, `doc`, `documentation`, `test`, `tests`,
///    `ci`, `.ci`} → Optional with parent-mapped scope name.
/// 2. Filename matches `requirements-<scope>*.txt` OR `<scope>-
///    requirements*.txt` where `<scope>` ∈ {`dev`, `test`, `docs`, `ci`}
///    → Optional with the matched scope.
/// 3. Otherwise → Main.
///
/// Parent-dir + filename matching is case-insensitive. Standard Python
/// convention (`docs/`, `tests/`) uses lowercase, but cpython ships
/// `Doc/requirements.txt` (capital D) — case-insensitive matching
/// covers both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequirementsScope {
    Main,
    /// Static scope-name string — one of `"dev"`, `"test"`, `"docs"`, `"ci"`.
    Optional(&'static str),
}

impl RequirementsScope {
    pub(crate) fn as_optional_name(&self) -> Option<&'static str> {
        match self {
            RequirementsScope::Optional(name) => Some(name),
            RequirementsScope::Main => None,
        }
    }
}

/// FR-005a classifier — see [`RequirementsScope`] for the priority order.
pub(crate) fn classify_requirements_scope(path: &Path) -> RequirementsScope {
    // Priority 1: parent-directory signal (case-insensitive).
    if let Some(parent) = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
    {
        let parent_lower = parent.to_ascii_lowercase();
        match parent_lower.as_str() {
            "docs" | "doc" | "documentation" => return RequirementsScope::Optional("docs"),
            "test" | "tests" => return RequirementsScope::Optional("test"),
            "ci" | ".ci" => return RequirementsScope::Optional("ci"),
            _ => {}
        }
    }
    // Priority 2: filename signal.
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    for scope in ["dev", "test", "docs", "ci"] {
        if matches_scope_filename(filename, scope) {
            return RequirementsScope::Optional(scope);
        }
    }
    RequirementsScope::Main
}

/// True when `filename` matches `requirements-<scope>*.txt` OR
/// `<scope>-requirements*.txt`, case-insensitive.
///
/// Word-boundary shape — `special` does NOT match `ci` scope even
/// though `special` contains the substring `ci`. Only recognized
/// tokens are `requirements-<scope>` or `<scope>-requirements`
/// followed by end-of-stem, another `-`, or `.` (before `.txt`).
fn matches_scope_filename(filename: &str, scope: &str) -> bool {
    let f = filename.to_ascii_lowercase();
    let stem = f.strip_suffix(".txt").unwrap_or(&f);
    // Shape 1: requirements-<scope>[-...]
    let prefix1 = format!("requirements-{scope}");
    if let Some(rest) = stem.strip_prefix(&prefix1) {
        if rest.is_empty() || rest.starts_with('-') || rest.starts_with('.') {
            return true;
        }
    }
    // Shape 2: <scope>-requirements[-...]
    let prefix2 = format!("{scope}-requirements");
    if let Some(rest) = stem.strip_prefix(&prefix2) {
        if rest.is_empty() || rest.starts_with('-') || rest.starts_with('.') {
            return true;
        }
    }
    false
}

pub(super) fn read_requirements_files(rootfs: &Path) -> Option<Vec<PackageDbEntry>> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(rootfs) else {
        return None;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.starts_with("requirements") && n.ends_with(".txt"))
        })
        .collect();
    paths.sort();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let source_path = path.to_string_lossy().into_owned();
        // Milestone 670 T013 — classify the file's lifecycle scope
        // per FR-005a. All entries parsed from this file share the
        // same scope tag.
        let scope = classify_requirements_scope(&path);
        let parsed = parse_requirements_file_text(&text);
        for entry in parsed {
            if let Some(pdb) = entry.into_package_db_entry(&source_path, scope) {
                out.push(pdb);
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// One line from a `requirements.txt`-style file, normalised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RequirementsTxtEntry {
    pub name: String,
    /// Only populated for exactly-pinned (`==`) requirements. For
    /// ranges / unpinned / URL refs, left empty.
    ///
    /// Milestone 670 T012: also populated with the git rev extracted
    /// from `pkg @ git+https://...@<rev>` when the URL fragment
    /// carries a resolvable rev.
    pub version: String,
    /// Original raw line (including operators, extras, hash flags).
    /// Emitted as `waybill:requirement-range` on the component.
    pub range_spec: String,
    /// Non-registry source kind: `"url"` for `https://...`, `"local"`
    /// for `file:...`, `"git"` for `git+...`. None for registry-named
    /// requirements.
    pub source_type: Option<String>,
    /// Per-component content hashes from `--hash=alg:hex` flags. pip
    /// allows multiple `--hash=` flags per requirement (one per
    /// distribution file — sdist + per-platform wheels) and CDX
    /// `components[].hashes[]` is array-shaped, so all are emitted.
    pub hashes: Vec<ContentHash>,
    /// Milestone 670 T012 — PEP 508 direct-URL metadata for entries of
    /// shape `pkg @ git+https://...@rev` or `pkg @ https://...tar.gz`.
    /// `None` for registry-named requirements. When `Some`, the
    /// component emits a `waybill:direct-url-source` annotation with
    /// `{url, kind, resolved_rev}` matching pip's PEP 610
    /// `direct_url.json` metadata shape.
    pub direct_url: Option<DirectUrlRef>,
}

/// Milestone 670 T012 — captured PEP 508 direct-URL source metadata.
///
/// Populated when a requirements line matches the `pkg @ URL` shape.
/// Emitted as the `waybill:direct-url-source` component annotation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirectUrlRef {
    /// The raw URL string as written in the requirements file.
    pub url: String,
    /// Classification: `"git"` for `git+...` URLs, `"url"` for plain
    /// HTTP/HTTPS URLs pointing at archives (tarballs, wheels).
    pub kind: &'static str,
    /// Statically extractable git rev, when the URL is a VCS reference
    /// with `@<rev>` in the path segment. `None` for archive URLs and
    /// git URLs without a rev suffix.
    pub resolved_rev: Option<String>,
}

impl DirectUrlRef {
    /// Attempt to parse a URL as a PEP 508 direct-URL source. Returns
    /// `None` if the URL doesn't match one of the recognized shapes
    /// (`git+...`, `http://`, `https://`, `hg+...`, `svn+...`, `bzr+...`,
    /// `file://`). VCS URLs with `@<rev>` in the path have the rev
    /// extracted; archive URLs are captured verbatim.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let url = raw.trim();
        if url.is_empty() {
            return None;
        }
        // VCS URL prefixes per PEP 440 §Direct references + pip docs.
        // Order matters: `git+https://...` starts with `git+`, not `http`.
        for vcs_prefix in ["git+", "hg+", "svn+", "bzr+"] {
            if url.starts_with(vcs_prefix) {
                let rev = extract_git_rev_from_url(url);
                return Some(DirectUrlRef {
                    url: url.to_string(),
                    kind: "git",
                    resolved_rev: rev,
                });
            }
        }
        if url.starts_with("http://") || url.starts_with("https://") {
            return Some(DirectUrlRef {
                url: url.to_string(),
                kind: "url",
                resolved_rev: None,
            });
        }
        if url.starts_with("file://") {
            return Some(DirectUrlRef {
                url: url.to_string(),
                kind: "local",
                resolved_rev: None,
            });
        }
        None
    }
}

/// Extract the `@<rev>` suffix from a `git+URL@rev` reference.
///
/// PEP 440 §Direct references allow `git+https://host/repo.git@rev`
/// where `<rev>` is a branch, tag, or commit SHA. The rev is the
/// substring between the LAST `@` in the URL path (excluding the
/// scheme separators like `git+ssh://user@host`) and either the URL
/// fragment (`#egg=`) or the URL end.
///
/// Returns `None` when no rev suffix is present.
fn extract_git_rev_from_url(url: &str) -> Option<String> {
    // Strip fragment (`#egg=...`) first so it doesn't confuse the
    // rev extraction.
    let before_frag = url.split_once('#').map(|(a, _)| a).unwrap_or(url);
    // Strip the scheme prefix (`git+`, then `https://` etc) so the
    // `user@host` colon in ssh URLs doesn't get mistaken for a rev.
    let after_scheme = before_frag
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(before_frag);
    // The LAST `@` in the remaining path is the rev separator per
    // PEP 440. If it's absent (or if the only `@` is the ssh user
    // separator `user@host`, which comes BEFORE any `/`), no rev.
    let last_at = after_scheme.rfind('@')?;
    // Guard: if `last_at` is in the host portion (before the first
    // `/`), it's the ssh user separator, not a rev. Only accept when
    // the `@` sits inside the path segment.
    let first_slash = after_scheme.find('/')?;
    if last_at < first_slash {
        return None;
    }
    let rev = after_scheme[last_at + 1..].trim();
    if rev.is_empty() {
        None
    } else {
        Some(rev.to_string())
    }
}

impl RequirementsTxtEntry {
    fn into_package_db_entry(
        self,
        source_path: &str,
        scope: RequirementsScope,
    ) -> Option<PackageDbEntry> {
        if self.name.is_empty() {
            return None;
        }
        // PURL for empty version: `pkg:pypi/<name>` (no @). packageurl
        // crate accepts this.
        let purl_str = build_pypi_purl_str(&self.name, &self.version);
        let purl = Purl::new(&purl_str).ok()?;
        // Tier: `source` when the requirement is exactly pinned
        // (`==` gives us a concrete version); `design` for ranges /
        // unpinned / URL refs where we kept the raw range string
        // but have no resolved version. A project that exclusively
        // pins its deps is authoritative for the pypi ecosystem —
        // `complete_ecosystems` keys off this.
        let tier = if self.version.is_empty() {
            "design"
        } else {
            "source"
        };
        // Milestone 236 (C151): on the design-tier branch, tag with
        // `waybill:unresolved-reason` naming the resolution boundary.
        // The requirement has no `==X.Y` pin and no lockfile fallback
        // resolved it upstream.
        let mut extra_annotations: std::collections::BTreeMap<
            String,
            serde_json::Value,
        > = Default::default();
        if tier == "design" {
            // Milestone 670 T012: differentiate the m236 reason for PEP
            // 508 direct-URL entries — the resolution boundary is "URL
            // present but no rev extractable," which is diagnostically
            // distinct from "no version specifier on a registry entry."
            let reason = if self.direct_url.is_some() && self.version.is_empty() {
                "PEP 508 direct-URL entry; no rev extractable from URL"
            } else {
                "no version specifier in requirements.txt; no uv.lock / poetry.lock fallback"
            };
            extra_annotations.insert(
                "waybill:unresolved-reason".to_string(),
                serde_json::Value::String(reason.to_string()),
            );
        }
        // Milestone 670 T013 — FR-005a scope-heuristic annotation.
        // Present ONLY when the file's classification yielded an
        // Optional scope (dev/test/docs/ci). Runtime files (bare
        // `requirements.txt` at project-standard locations) omit the
        // annotation. Catalog row C158 registered by T016.
        if let Some(scope_name) = scope.as_optional_name() {
            extra_annotations.insert(
                "waybill:python-req-file-scope".to_string(),
                serde_json::Value::String(scope_name.to_string()),
            );
        }
        // Milestone 670 T012: `waybill:direct-url-source` annotation
        // for PEP 508 direct-URL entries. Carries `{url, kind, resolved_rev}`
        // matching pip's PEP 610 `direct_url.json` metadata shape.
        // Catalog row C154 registered by T016.
        if let Some(direct) = &self.direct_url {
            let mut obj = serde_json::Map::new();
            obj.insert("url".to_string(), serde_json::Value::String(direct.url.clone()));
            obj.insert("kind".to_string(), serde_json::Value::String(direct.kind.to_string()));
            obj.insert(
                "resolved_rev".to_string(),
                match &direct.resolved_rev {
                    Some(rev) => serde_json::Value::String(rev.clone()),
                    None => serde_json::Value::Null,
                },
            );
            extra_annotations.insert(
                "waybill:direct-url-source".to_string(),
                serde_json::Value::Object(obj),
            );
        }
        // Milestone 670 T013 — populate lifecycle_scope from FR-005a
        // classification. Optional-scope files (dev/test/docs/ci) mark
        // every emitted entry as `LifecycleScope::Optional`. Main-scope
        // files leave it None (Runtime by absence of tag; interacts
        // cleanly with m183's `is_none()` guard in
        // `apply_optional_derivation_annotation`).
        let lifecycle_scope = match scope {
            RequirementsScope::Main => None,
            RequirementsScope::Optional(_) => {
                Some(waybill_common::resolution::LifecycleScope::Optional)
            }
        };
        Some(PackageDbEntry {
            build_inclusion: None,
            purl,
            name: self.name,
            version: self.version,
            arch: None,
            source_path: source_path.to_string(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope,
            requirement_ranges: vec![self.range_spec],
            source_type: self.source_type,
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
            hashes: self.hashes,
            sbom_tier: Some(tier.to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        })
    }
}

/// Parse raw `requirements.txt` text. Tolerates:
/// - `# comments` (full-line or trailing).
/// - Blank lines.
/// - `-r <other.txt>` includes (ignored this milestone; follow-up to recurse).
/// - `--hash=sha256:...` flags on their own line or trailing.
/// - URL refs (`https://...`, `git+...`, `file:...`).
/// - Pinned (`==`) and ranged (`>=`, `<`, `~=`, `!=`) requirements.
pub(crate) fn parse_requirements_file_text(text: &str) -> Vec<RequirementsTxtEntry> {
    let mut out = Vec::new();
    // Deal with line continuations: a trailing backslash joins to the
    // next line. Common in pinned-with-hash blocks.
    let joined = text.replace("\\\n", " ");
    for raw in joined.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Strip full-line comments.
        if line.starts_with('#') {
            continue;
        }
        // Strip trailing comments (but only when clearly after a space).
        let line = match line.split_once(" #") {
            Some((before, _)) => before.trim(),
            None => line,
        };
        // Skip `-r`, `-c`, `--index-url`, etc. lines — meta-commands.
        if line.starts_with('-') {
            continue;
        }

        if let Some(entry) = parse_requirements_line(line) {
            out.push(entry);
        }
    }
    out
}

/// Parse a single non-blank, non-comment, non-meta requirements line.
fn parse_requirements_line(line: &str) -> Option<RequirementsTxtEntry> {
    // Split off `--hash=alg:hex` flags. pip allows MULTIPLE per
    // requirement (one for sdist + one per platform wheel) so collect
    // all of them. Each flag has form `--hash=<alg>:<hex>`.
    let body = line.split("--hash").next().unwrap_or(line).trim();
    let hashes = parse_hash_flags(line);

    // Milestone 670 T012: PEP 508 direct-URL entry (`pkg @ URL` shape).
    // PEP 508 mandates spaces around the `@` when the URL follows a name;
    // the presence of ` @ ` distinguishes this from `name[extras]` or
    // `name==version`. Handled BEFORE the bare-URL branches so the name-
    // prefix supersedes the `egg=` fragment fallback.
    if let Some((name_part, url_part)) = body.split_once(" @ ") {
        if let Some(direct) = DirectUrlRef::parse(url_part) {
            if let Some(name) = tokenise_requires_dist_name(name_part) {
                let version = direct.resolved_rev.clone().unwrap_or_default();
                let source_type = direct.kind.to_string();
                return Some(RequirementsTxtEntry {
                    name,
                    version,
                    range_spec: body.to_string(),
                    source_type: Some(source_type),
                    hashes,
                    direct_url: Some(direct),
                });
            }
        }
    }

    // URL-style sources (bare, without `pkg @ ` prefix).
    //
    // Milestone 670 T012 note: we DO populate `direct_url` for bare-URL
    // entries so the `waybill:direct-url-source` annotation surfaces
    // consistently across both shapes. Version stays empty on this
    // branch (design-tier) to preserve pre-m670 `pkg:pypi/<name>` PURL
    // shape — the git rev from `@ref` in a bare-git-URL is *branch/tag*
    // metadata, not a resolved version. The name-prefixed shape (`pkg
    // @ git+URL@rev`) treats the rev AS the resolved version because
    // the PEP 508 syntax makes that intent explicit.
    if body.starts_with("git+") {
        // e.g. `git+https://github.com/foo/bar.git@rev#egg=bar`
        let name = egg_fragment(body).unwrap_or_else(|| "unknown".to_string());
        let direct = DirectUrlRef::parse(body);
        return Some(RequirementsTxtEntry {
            name,
            version: String::new(),
            range_spec: body.to_string(),
            source_type: Some("git".to_string()),
            hashes,
            direct_url: direct,
        });
    }
    if body.starts_with("http://") || body.starts_with("https://") {
        let name = egg_fragment(body).unwrap_or_else(|| "unknown".to_string());
        let direct = DirectUrlRef::parse(body);
        return Some(RequirementsTxtEntry {
            name,
            version: String::new(),
            range_spec: body.to_string(),
            source_type: Some("url".to_string()),
            hashes,
            direct_url: direct,
        });
    }
    if body.starts_with("file:") || body.starts_with('.') || body.starts_with('/') {
        let name = egg_fragment(body).unwrap_or_else(|| "unknown".to_string());
        return Some(RequirementsTxtEntry {
            name,
            version: String::new(),
            range_spec: body.to_string(),
            source_type: Some("local".to_string()),
            hashes,
            direct_url: None,
        });
    }

    // Registry-style: `name[extras] OP version, OP version; marker`.
    // Reuse the PEP 508 tokeniser for the name; detect `==` for pinning.
    let name = tokenise_requires_dist_name(body)?;

    // Look for a single `==` pin to populate `version`.
    let version = pinned_version_from(body).unwrap_or_default();

    Some(RequirementsTxtEntry {
        name,
        version,
        range_spec: body.to_string(),
        source_type: None,
        hashes,
        direct_url: None,
    })
}

/// Extract every `--hash=<alg>:<hex>` flag from a requirements line.
/// Tolerates both `--hash=sha256:abc` and `--hash sha256:abc` shapes
/// (pip accepts the latter via getopt-style spacing). Unknown
/// algorithms are silently dropped (not just sha256/512/1 — md5
/// also gets through ContentHash::with_algorithm but pip docs only
/// list sha256/384/512).
fn parse_hash_flags(line: &str) -> Vec<ContentHash> {
    let mut out = Vec::new();
    // Iterate on `--hash` substring; each occurrence is followed by
    // either `=` or ` ` then `<alg>:<hex>`.
    let mut rest = line;
    while let Some(idx) = rest.find("--hash") {
        let after = &rest[idx + "--hash".len()..];
        // Skip the separator (`=` or whitespace).
        let after = after.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
        // Take up to the next whitespace or end.
        let token_end = after
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after.len());
        let token = &after[..token_end];
        if let Some((alg_str, hex)) = token.split_once(':') {
            if let Some(alg) = parse_hash_alg(alg_str) {
                if let Ok(hash) = ContentHash::with_algorithm(alg, hex) {
                    if !out.contains(&hash) {
                        out.push(hash);
                    }
                }
            }
        }
        rest = &after[token_end..];
    }
    out
}

fn parse_hash_alg(s: &str) -> Option<HashAlgorithm> {
    match s.to_ascii_lowercase().as_str() {
        "sha256" => Some(HashAlgorithm::Sha256),
        "sha512" => Some(HashAlgorithm::Sha512),
        "sha1" => Some(HashAlgorithm::Sha1),
        // sha384 not in HashAlgorithm yet; pip uses sha256/384/512.
        // md5 is supported by ContentHash but pip rejects md5 hashes.
        _ => None,
    }
}

/// Extract `egg=<name>` from a URL-style requirement, if present.
fn egg_fragment(url: &str) -> Option<String> {
    let frag = url.split_once('#')?.1;
    for part in frag.split('&') {
        if let Some(value) = part.strip_prefix("egg=") {
            let clean = value.split('[').next().unwrap_or(value);
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

/// If the requirement has a single `==` pin (and no disjunction like
/// `==1.0 || ==2.0`, which pip doesn't support anyway), return that
/// version string. Returns None for ranges.
fn pinned_version_from(body: &str) -> Option<String> {
    // Drop any env marker first.
    let head = body.split_once(';').map(|x| x.0).unwrap_or(body);
    // Look for `==` as an exact operator. Ignore `!=` and `~=`.
    for part in head.split(',') {
        let p = part.trim();
        if let Some(rest) = p.strip_prefix("==") {
            return Some(rest.trim().to_string());
        }
        // `name == 1.0` form: find `==` after some whitespace.
        if let Some(idx) = p.find("==") {
            // Ensure it's an `==` operator, not a substring of `===` etc.
            let before = &p[..idx];
            let after = &p[idx + 2..];
            if !before.ends_with('=') && !after.starts_with('=') {
                return Some(after.trim().trim_start_matches(' ').to_string());
            }
        }
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    #[test]
    fn requirements_txt_pinned_populates_version_and_range() {
        let entries = parse_requirements_file_text("requests==2.31.0\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(entries[0].version, "2.31.0");
        assert_eq!(entries[0].range_spec, "requests==2.31.0");
        assert!(entries[0].source_type.is_none());
    }

    #[test]
    fn requirements_txt_ranged_leaves_version_empty() {
        let entries = parse_requirements_file_text("requests>=2,<3\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert!(entries[0].version.is_empty());
        assert_eq!(entries[0].range_spec, "requests>=2,<3");
    }

    #[test]
    fn requirements_txt_bare_name_empty_version() {
        let entries = parse_requirements_file_text("requests\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert!(entries[0].version.is_empty());
    }

    #[test]
    fn requirements_txt_strips_comments_and_blank_lines() {
        let text = "\
# top comment
requests==2.31.0  # trailing comment
# another

urllib3>=2  # with space before hash
";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(entries[1].name, "urllib3");
    }

    #[test]
    fn requirements_txt_skips_meta_commands() {
        let text = "\
-r other.txt
--index-url https://pypi.org/simple/
requests==2.31.0
";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
    }

    #[test]
    fn requirements_txt_strips_hash_flags() {
        let text = "\
requests==2.31.0 --hash=sha256:abc123 --hash=sha256:def456
urllib3>=2 \\
    --hash=sha256:zzz
";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(entries[1].name, "urllib3");
    }

    #[test]
    fn parse_hash_flags_captures_single_sha256() {
        let line = "requests==2.31.0 --hash=sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f";
        let hashes = parse_hash_flags(line);
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0].algorithm, HashAlgorithm::Sha256);
        assert_eq!(
            hashes[0].value.as_str(),
            "58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f"
        );
    }

    #[test]
    fn parse_hash_flags_captures_multiple() {
        // pip allows multiple --hash= flags (sdist + per-platform wheel).
        let sha256 = "a".repeat(64);
        let sha512 = "b".repeat(128);
        let line = format!(
            "requests==2.31.0 --hash=sha256:{sha256} --hash=sha512:{sha512}"
        );
        let hashes = parse_hash_flags(&line);
        assert_eq!(hashes.len(), 2);
        // Order preserved (first --hash flag → first slot).
        assert_eq!(hashes[0].algorithm, HashAlgorithm::Sha256);
        assert_eq!(hashes[1].algorithm, HashAlgorithm::Sha512);
    }

    #[test]
    fn parse_hash_flags_dedups_identical_entries() {
        // Pathological: same hash specified twice. Only emit once.
        let line = "x==1 --hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa --hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hashes = parse_hash_flags(line);
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn parse_hash_flags_drops_unknown_algorithm() {
        let line = "x==1 --hash=md4:dead --hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hashes = parse_hash_flags(line);
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0].algorithm, HashAlgorithm::Sha256);
    }

    #[test]
    fn parse_hash_flags_returns_empty_when_absent() {
        let line = "requests==2.31.0";
        assert!(parse_hash_flags(line).is_empty());
    }

    #[test]
    fn parse_hash_flags_handles_space_separator() {
        // pip also accepts `--hash sha256:abc` (getopt-style).
        let line = "x==1 --hash sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hashes = parse_hash_flags(line);
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn requirements_txt_threads_hashes_through_to_entry() {
        let text = "requests==2.31.0 --hash=sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hashes.len(), 1);
        let pdb = entries[0]
            .clone()
            .into_package_db_entry("/req.txt", RequirementsScope::Main)
            .expect("converts");
        assert_eq!(pdb.hashes.len(), 1);
        assert_eq!(pdb.hashes[0].algorithm, HashAlgorithm::Sha256);
    }

    #[test]
    fn requirements_txt_git_url_source_type() {
        let text = "git+https://github.com/psf/requests.git@main#egg=requests\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(entries[0].source_type.as_deref(), Some("git"));
        assert!(entries[0].version.is_empty());
    }

    #[test]
    fn requirements_txt_https_url_source_type() {
        let text = "https://example.com/pkg/foo-1.0.tar.gz#egg=foo\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "foo");
        assert_eq!(entries[0].source_type.as_deref(), Some("url"));
    }

    #[test]
    fn requirements_txt_file_ref_source_type() {
        let text = "file:./local/pkg#egg=local-pkg\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "local-pkg");
        assert_eq!(entries[0].source_type.as_deref(), Some("local"));
    }

    #[test]
    fn requirements_txt_pinned_produces_source_tier() {
        // Exact pin (`==`) means the requirement IS authoritative for
        // the version — same semantics as a cargo.lock line.
        // `complete_ecosystems` keys off source/deployed tier, so
        // pinned requirements.txt entries mark the pypi ecosystem
        // complete and drive sbomqs `sbom_completeness_declared`.
        let entry = RequirementsTxtEntry {
            name: "requests".into(),
            version: "2.31.0".into(),
            range_spec: "requests==2.31.0".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("source"));
        assert_eq!(pdb.requirement_ranges.as_slice(), &["requests==2.31.0".to_string()]);
    }

    #[test]
    fn requirements_txt_unpinned_stays_design_tier() {
        // Range / unpinned requirements have no resolved version —
        // tier stays `design`, same as a package.json dependency
        // block without a lockfile.
        let entry = RequirementsTxtEntry {
            name: "requests".into(),
            version: "".into(),
            range_spec: "requests>=2.0".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("design"));
    }

    #[test]
    fn m236_pip_design_tier_carries_unresolved_reason() {
        // Milestone 236 (C151): unpinned requirement (design-tier)
        // MUST carry `waybill:unresolved-reason` naming pip's
        // resolution boundary.
        let entry = RequirementsTxtEntry {
            name: "waybill-fixture-pip".into(),
            version: String::new(),
            range_spec: "waybill-fixture-pip>=2.0".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("design"));
        let reason = pdb
            .extra_annotations
            .get("waybill:unresolved-reason")
            .expect("C151 annotation present on design-tier pip component");
        assert_eq!(
            reason.as_str().unwrap(),
            "no version specifier in requirements.txt; no uv.lock / poetry.lock fallback",
        );
    }

    #[test]
    fn m236_pip_source_tier_does_not_carry_unresolved_reason() {
        // FR-004 negative assertion: source-tier components MUST NOT
        // carry the C151 annotation.
        let entry = RequirementsTxtEntry {
            name: "waybill-fixture-pip".into(),
            version: "1.2.3".into(),
            range_spec: "waybill-fixture-pip==1.2.3".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("source"));
        assert!(
            !pdb.extra_annotations.contains_key("waybill:unresolved-reason"),
            "source-tier components MUST NOT carry C151 annotation",
        );
    }

    #[test]
    fn requirements_txt_empty_version_purl_well_formed() {
        let entry = RequirementsTxtEntry {
            name: "requests".into(),
            version: String::new(),
            range_spec: "requests>=2".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        // packageurl-rs accepts `pkg:pypi/<name>` without @version.
        assert!(pdb.purl.as_str().starts_with("pkg:pypi/requests"));
    }

    // -----------------------------------------------------------------
    // Milestone 670 T012: PEP 508 direct-URL entry (`pkg @ URL` shape)
    // -----------------------------------------------------------------

    #[test]
    fn m670_pep508_direct_url_git_with_rev_populates_version() {
        // PEP 508 `pkg @ git+URL@<rev>` — the rev is the resolved
        // version per Q5 clarification. source_type=git, direct_url
        // annotation with resolved_rev populated.
        let text =
            "pygments @ git+https://github.com/pygments/pygments.git@2cad2642\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pygments");
        assert_eq!(entries[0].version, "2cad2642");
        assert_eq!(entries[0].source_type.as_deref(), Some("git"));

        let direct = entries[0]
            .direct_url
            .as_ref()
            .expect("direct_url populated");
        assert_eq!(direct.kind, "git");
        assert_eq!(direct.resolved_rev.as_deref(), Some("2cad2642"));

        // Emission check: waybill:direct-url-source annotation on the
        // emitted component carries the {url, kind, resolved_rev}
        // structure per pip's PEP 610 direct_url.json metadata shape.
        let pdb = entries[0]
            .clone()
            .into_package_db_entry("/req.txt", RequirementsScope::Main)
            .expect("converts");
        assert_eq!(pdb.purl.as_str(), "pkg:pypi/pygments@2cad2642");
        let anno = pdb
            .extra_annotations
            .get("waybill:direct-url-source")
            .expect("m670 T012 annotation present");
        assert_eq!(anno["kind"].as_str(), Some("git"));
        assert_eq!(anno["resolved_rev"].as_str(), Some("2cad2642"));
        assert!(anno["url"].as_str().unwrap().contains("pygments.git"));
    }

    #[test]
    fn m670_pep508_direct_url_git_without_rev_leaves_version_empty() {
        // git+URL without `@rev` suffix — no rev extractable, tier
        // stays design, m236 reason uses the T012-locked
        // "PEP 508 direct-URL entry; no rev extractable" string.
        let text = "somepkg @ git+https://github.com/foo/somepkg.git\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "somepkg");
        assert!(entries[0].version.is_empty());
        assert_eq!(entries[0].source_type.as_deref(), Some("git"));
        let direct = entries[0]
            .direct_url
            .as_ref()
            .expect("direct_url populated");
        assert_eq!(direct.kind, "git");
        assert!(direct.resolved_rev.is_none());

        let pdb = entries[0]
            .clone()
            .into_package_db_entry("/req.txt", RequirementsScope::Main)
            .expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("design"));
        assert_eq!(
            pdb.extra_annotations
                .get("waybill:unresolved-reason")
                .and_then(|v| v.as_str()),
            Some("PEP 508 direct-URL entry; no rev extractable from URL"),
            "T012 locked reason string differentiates direct-URL from unpinned-requirements case",
        );
    }

    #[test]
    fn m670_pep508_direct_url_https_tarball_no_rev() {
        // Real cpython case: `pygments @ https://.../pygments-<sha>.tar.gz`.
        // No git rev semantics; version stays empty; annotation records
        // the URL for provenance.
        let text = "pygments @ https://github.com/pygments/pygments/archive/2cad2642.tar.gz\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pygments");
        assert!(entries[0].version.is_empty());
        assert_eq!(entries[0].source_type.as_deref(), Some("url"));
        let direct = entries[0]
            .direct_url
            .as_ref()
            .expect("direct_url populated");
        assert_eq!(direct.kind, "url");
        assert!(direct.resolved_rev.is_none());
        assert!(direct.url.ends_with(".tar.gz"));
    }

    #[test]
    fn m670_pep508_direct_url_invalid_url_falls_back_to_registry_parse() {
        // If the `@` separator is present but the following string
        // doesn't match a supported URL scheme, the direct-URL detector
        // returns None and the parser falls through to the registry-
        // shape branch. The tokeniser stops at `@`, so the name is
        // still extracted correctly and the component emits as a
        // design-tier registry entry.
        let text = "requests @ not-a-real-scheme\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        // Falls through to registry-style — no direct_url populated,
        // source_type None, no version pin.
        assert!(entries[0].direct_url.is_none());
        assert_eq!(entries[0].source_type, None);
        assert!(entries[0].version.is_empty());
    }

    // -----------------------------------------------------------------
    // Milestone 670 T013: FR-005a scope-heuristic tests
    // -----------------------------------------------------------------

    #[test]
    fn m670_scope_classify_parent_docs_wins_regardless_of_filename() {
        let path = std::path::PathBuf::from("/proj/docs/requirements.txt");
        assert_eq!(
            classify_requirements_scope(&path),
            RequirementsScope::Optional("docs"),
        );
        // Case-insensitive: cpython ships `Doc/requirements.txt`.
        let path = std::path::PathBuf::from("/proj/Doc/requirements.txt");
        assert_eq!(
            classify_requirements_scope(&path),
            RequirementsScope::Optional("docs"),
        );
        // `documentation/` also maps to `docs`.
        let path = std::path::PathBuf::from("/proj/documentation/requirements.txt");
        assert_eq!(
            classify_requirements_scope(&path),
            RequirementsScope::Optional("docs"),
        );
    }

    #[test]
    fn m670_scope_classify_parent_tests_and_ci_variants() {
        for (dirname, expected) in [
            ("tests", "test"),
            ("test", "test"),
            ("Tests", "test"),
            ("ci", "ci"),
            (".ci", "ci"),
        ] {
            let path = std::path::PathBuf::from(format!("/proj/{dirname}/requirements.txt"));
            assert_eq!(
                classify_requirements_scope(&path),
                RequirementsScope::Optional(expected),
                "parent-dir `{dirname}` should map to scope `{expected}`",
            );
        }
    }

    #[test]
    fn m670_scope_classify_filename_signals() {
        for (fname, expected) in [
            ("requirements-dev.txt", Some("dev")),
            ("requirements-test.txt", Some("test")),
            ("requirements-docs.txt", Some("docs")),
            ("requirements-ci.txt", Some("ci")),
            ("dev-requirements.txt", Some("dev")),
            ("test-requirements.txt", Some("test")),
            ("docs-requirements.txt", Some("docs")),
            // Suffix-with-more-tokens after scope name still matches.
            ("requirements-dev-frozen.txt", Some("dev")),
            // Bare `requirements.txt` (no scope token) → Main.
            ("requirements.txt", None),
            // Word-boundary check — `requirements-hypothesis.txt` doesn't
            // match any scope keyword.
            ("requirements-hypothesis.txt", None),
            // Word-boundary check — `special` contains substring `ci`
            // but MUST NOT match the ci scope (regression guard for
            // the substring-based first draft).
            ("requirements-special.txt", None),
        ] {
            let path = std::path::PathBuf::from(format!("/proj/{fname}"));
            let got = classify_requirements_scope(&path);
            let expected_scope = match expected {
                Some(name) => RequirementsScope::Optional(name),
                None => RequirementsScope::Main,
            };
            assert_eq!(
                got, expected_scope,
                "filename `{fname}` classified as {got:?}, expected {expected_scope:?}",
            );
        }
    }

    #[test]
    fn m670_scope_classify_parent_wins_over_filename() {
        // Parent-dir `docs/` beats filename `requirements-dev.txt`.
        // (Contrived — a real project wouldn't mix signals — but the
        // priority order is contract-locked.)
        let path = std::path::PathBuf::from("/proj/docs/requirements-dev.txt");
        assert_eq!(
            classify_requirements_scope(&path),
            RequirementsScope::Optional("docs"),
        );
    }

    #[test]
    fn m670_optional_scope_emits_lifecycle_scope_optional_and_annotation() {
        let entry = RequirementsTxtEntry {
            name: "sphinx".into(),
            version: "".into(),
            range_spec: "sphinx>=7".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry
            .into_package_db_entry("/proj/docs/requirements.txt", RequirementsScope::Optional("docs"))
            .expect("converts");
        assert_eq!(
            pdb.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional),
            "docs-scoped entries emit as Optional",
        );
        assert_eq!(
            pdb.extra_annotations
                .get("waybill:python-req-file-scope")
                .and_then(|v| v.as_str()),
            Some("docs"),
            "annotation carries the derived scope name",
        );
    }

    #[test]
    fn m670_main_scope_omits_annotation_and_lifecycle_scope() {
        // FR-005a: bare `requirements.txt` at project-standard location →
        // Main → no annotation, lifecycle_scope stays None (Runtime-by-
        // absence-of-tag, matching pre-m670 behavior).
        let entry = RequirementsTxtEntry {
            name: "requests".into(),
            version: "2.31.0".into(),
            range_spec: "requests==2.31.0".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry
            .into_package_db_entry("/proj/requirements.txt", RequirementsScope::Main)
            .expect("converts");
        assert_eq!(pdb.lifecycle_scope, None);
        assert!(
            !pdb.extra_annotations
                .contains_key("waybill:python-req-file-scope"),
            "Main-scope entries omit the annotation",
        );
    }
}
