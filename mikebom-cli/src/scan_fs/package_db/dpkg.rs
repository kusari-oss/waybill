//! Parse `/var/lib/dpkg/status` — the authoritative list of installed
//! packages on a Debian/Ubuntu system.
//!
//! The file is a sequence of RFC-822-style stanzas, each describing one
//! package, separated by blank lines. The fields we consume:
//! - `Package`, `Version`, `Architecture` — identity triplet
//! - `Status` — must contain `install ok installed` for the entry to
//!   count as actually installed (everything else is `deinstall`,
//!   `half-installed`, `config-files`, etc.)
//! - `Depends` — comma-separated dependency list, tokens may include
//!   version constraints `(>= 1.0)` and alternatives `libjq1 | libonig5`

use std::path::Path;

use anyhow::{Context, Result};
use mikebom_common::types::purl::Purl;

use super::PackageDbEntry;

/// The dpkg status path relative to a rootfs (legacy single-file
/// layout, written by a real dpkg daemon).
const DPKG_STATUS_PATH: &str = "var/lib/dpkg/status";

/// Per-package status directory used by minimal-image builds
/// (rules-distroless, chainguard apko, Bazel-shaped images that
/// don't include a real dpkg daemon). Each `<pkgname>` file is
/// exactly one stanza in the same RFC-822-style format the legacy
/// `status` file uses; `<pkgname>.md5sums` and other companions
/// also live here but parse as no-ops.
const DPKG_STATUS_D_DIR: &str = "var/lib/dpkg/status.d";

/// Read and parse the dpkg status file beneath `rootfs`. Returns an
/// empty vector when the file is absent; returns an error only when
/// the file is present but malformed.
///
/// Feature 005 US2/US3:
/// * `namespace` — the deb PURL namespace segment (e.g. `"debian"`,
///   `"ubuntu"`). Derived from `/etc/os-release::ID` by the caller
///   (`package_db::read_all`) with `"debian"` as the fallback when ID
///   is absent. Used as-is; no internal rewrite table (derivatives like
///   `kali` stay as `kali`, per FR-011).
/// * `distro_version` — optional `VERSION_ID` (e.g. `"12"`, `"24.04"`).
///   When `Some(non_empty)`, emitted as `&distro=<namespace>-<version>`
///   on every generated PURL. When `None` or empty, the qualifier is
///   omitted entirely.
pub fn read(
    rootfs: &Path,
    namespace: &str,
    distro_version: Option<&str>,
) -> Result<Vec<PackageDbEntry>> {
    // Two metadata sources, processed in priority order:
    //   1. /var/lib/dpkg/status — the legacy single-file layout
    //      (full dpkg-managed images: debian:*, ubuntu:*, etc.)
    //   2. /var/lib/dpkg/status.d/<pkg> — per-package files used
    //      by minimal-image builds (distroless, chainguard apko,
    //      rules-distroless, etc.)
    //
    // Real-world images use ONE or the OTHER, never both — but we
    // dedup defensively. When both sources contribute an entry for
    // the same purl, the status.d/ entry wins (FR-003): the modern
    // per-package layout is the source-of-truth in pathological
    // mixed images.
    let status_path = rootfs.join(DPKG_STATUS_PATH);
    let mut from_status: Vec<PackageDbEntry> = if status_path.is_file() {
        let text = std::fs::read_to_string(&status_path)
            .with_context(|| format!("reading {}", status_path.display()))?;
        let source = status_path.to_string_lossy().into_owned();
        parse(&text, &source, namespace, distro_version)
    } else {
        Vec::new()
    };
    let from_status_d = read_status_d_dir(rootfs, namespace, distro_version);

    if from_status_d.is_empty() {
        return Ok(from_status);
    }
    if from_status.is_empty() {
        return Ok(from_status_d);
    }

    // Dedup: drop any status entry whose purl also appears in
    // status.d/. status.d/ wins.
    let status_d_purls: std::collections::HashSet<&str> =
        from_status_d.iter().map(|e| e.purl.as_str()).collect();
    from_status.retain(|e| !status_d_purls.contains(e.purl.as_str()));
    drop(status_d_purls);
    from_status.extend(from_status_d);
    Ok(from_status)
}

/// Walk `<rootfs>/var/lib/dpkg/status.d/` and return one
/// [`PackageDbEntry`] per file that parses as a single dpkg
/// stanza marked `install ok installed`.
///
/// Files are processed in sorted-by-name order so the output is
/// deterministic across runs (read_dir's order is filesystem-
/// dependent). Files that don't parse — including the documented
/// companion files like `<pkg>.md5sums`, `<pkg>.conffiles`, etc.
/// — produce zero entries and are silently skipped, since
/// `parse_stanza` only returns `Some` for files matching the
/// dpkg-stanza grammar with `Status: install ok installed`.
///
/// IO errors per-file log at `tracing::debug` and skip the
/// affected file; this never propagates an error. Same posture as
/// [`collect_claimed_paths`] — a malformed status.d/ shouldn't
/// fail the whole scan.
fn read_status_d_dir(
    rootfs: &Path,
    namespace: &str,
    distro_version: Option<&str>,
) -> Vec<PackageDbEntry> {
    let dir = rootfs.join(DPKG_STATUS_D_DIR);
    let read_dir = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::debug!(
                dir = %dir.display(),
                error = %e,
                "could not read dpkg status.d/ directory; treating as empty"
            );
            return Vec::new();
        }
    };

    // Collect file paths first, then sort, then process — gives
    // stable output order across filesystems.
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
        if !is_file {
            continue;
        }
        paths.push(path);
    }
    paths.sort();

    let mut out = Vec::new();
    for path in paths {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %e,
                    "skipping unreadable file in dpkg status.d/"
                );
                continue;
            }
        };
        let source = path.to_string_lossy().into_owned();
        // status.d/ stanzas in real distroless / chainguard images
        // legitimately omit the `Status:` field (the file's existence
        // is the installation marker). Use the relaxed parser so
        // those entries surface as installed components.
        let entries = parse_relaxed(&text, &source, namespace, distro_version);
        out.extend(entries);
    }
    out
}

/// Iterate every `<pkg>.list` under `<rootfs>/var/lib/dpkg/info/` and
/// insert every listed absolute path (rootfs-joined) into `claimed`.
///
/// Drives the binary walker's skip gate — files owned by a dpkg
/// package shouldn't also produce `pkg:generic/<filename>` file-level
/// components. Milestone 004 post-ship fix.
///
/// No-op when the dpkg info directory is absent. Malformed `.list`
/// files are tolerated (non-empty non-path lines silently ignored);
/// this function never errors — a failed claim-collection just means
/// more binaries might emit redundant file-level components, not a
/// scan failure.
pub fn collect_claimed_paths(
    rootfs: &Path,
    claimed: &mut std::collections::HashSet<std::path::PathBuf>,
    #[cfg(unix)] claimed_inodes: &mut std::collections::HashSet<(u64, u64)>,
) {
    let info_dir = rootfs.join("var/lib/dpkg/info");
    let Ok(entries) = std::fs::read_dir(&info_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("list") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in content.lines() {
            let line = line.trim();
            if !line.starts_with('/') {
                continue;
            }
            let stripped = line.strip_prefix('/').unwrap_or(line);
            let joined = rootfs.join(stripped);
            super::insert_claim_with_canonical(
                claimed,
                #[cfg(unix)]
                claimed_inodes,
                joined,
            );
        }
    }
}

fn parse(
    text: &str,
    source_path: &str,
    namespace: &str,
    distro_version: Option<&str>,
) -> Vec<PackageDbEntry> {
    super::control_file::parse_stanzas(text)
        .into_iter()
        .filter_map(|stanza| {
            parse_stanza_inner(&stanza, source_path, namespace, distro_version, true)
        })
        .collect()
}

/// Like [`parse`] but does NOT require the `Status:` field. Used by
/// [`read_status_d_dir`] for the per-package layout (distroless /
/// chainguard / Bazel-built minimal images): those files legitimately
/// omit `Status:` because the image has no dpkg daemon to maintain
/// install state — the file's existence IS the installation marker.
/// When `Status:` IS present (rare in status.d/), a non-installed
/// value still filters the entry out.
fn parse_relaxed(
    text: &str,
    source_path: &str,
    namespace: &str,
    distro_version: Option<&str>,
) -> Vec<PackageDbEntry> {
    super::control_file::parse_stanzas(text)
        .into_iter()
        .filter_map(|stanza| {
            parse_stanza_inner(&stanza, source_path, namespace, distro_version, false)
        })
        .collect()
}

fn parse_stanza_inner(
    stanza: &super::control_file::ControlStanza,
    source_path: &str,
    namespace: &str,
    distro_version: Option<&str>,
    require_status: bool,
) -> Option<PackageDbEntry> {
    let get = |name: &str| -> Option<&str> { stanza.get(name) };

    // Only count entries whose status is fully installed. dpkg's Status
    // is three space-separated tokens; we want the exact phrase
    // `install ok installed`. The `require_status` flag controls
    // whether absence of the Status field is treated as installed
    // (status.d/ layout) or as a filter-out (legacy status file).
    let status = get("status");
    match (require_status, status) {
        // Legacy file: Status MUST be present and "install ok installed".
        (true, Some(s)) if !s.contains("install ok installed") => return None,
        (true, None) => return None,
        // Per-package layout: Status absence is fine; presence with a
        // non-installed value still filters.
        (false, Some(s)) if !s.contains("install ok installed") => return None,
        _ => {}
    }

    let name = get("package")?.to_string();
    let raw_version = get("version")?.to_string();
    let arch = get("architecture").map(|s| s.to_string());
    if name.is_empty() || raw_version.is_empty() {
        return None;
    }

    // Milestone 197 US1 (#562): split epoch prefix out of the raw
    // `Version:` field before PURL construction. Epoch flows into the
    // PURL as `?epoch=<N>` qualifier per purl-spec; naked version
    // (without the `<N>:` prefix) becomes the PURL version segment.
    let (epoch, version) = parse_deb_version_with_epoch(&raw_version);

    let purl_str = build_deb_purl(&name, &version, arch.as_deref(), namespace, distro_version, epoch);
    let purl = Purl::new(&purl_str).ok()?;

    let depends = get("depends")
        .map(parse_depends)
        .unwrap_or_default();

    // CycloneDX `component.supplier.name` is free-form text, so the
    // raw "Name <email>" form is fine and useful — it preserves the
    // contact path that distros publish without us having to parse the
    // angle-bracket address out.
    let maintainer = get("maintainer")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version,
        arch,
        source_path: source_path.to_string(),
        depends,
        maintainer,
        // dpkg status records what's INSTALLED on the rootfs — deployed
        // tier per research.md R13. dpkg doesn't carry a dev/prod
        // distinction or range spec, and the source is always the
        // registry (never local/git/url).
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_range: None,
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

/// Build a deb PURL. Matches the shape that
/// `resolve::path_resolver::resolve_deb_path` emits so the deduplicator
/// merges entries from both sources when they describe the same
/// installed package. The name + version are run through the shared
/// PURL encoder so `+` → `%2B` per the packageurl reference impl.
///
/// Feature 005 US2/US3:
/// * `namespace` is the PURL path segment (the "vendor" per
///   `deb-definition.json`): `debian`, `ubuntu`, or any other distro ID
///   from `/etc/os-release`. The caller derives it; we use it verbatim
///   (no rewrite table).
/// * `distro_version` is `/etc/os-release::VERSION_ID`. When
///   `Some(non_empty)`, the PURL carries `&distro=<namespace>-<ver>`
///   (e.g. `debian-12`, `ubuntu-24.04`, `alpine-3.20`). When `None` or
///   empty, the qualifier is omitted entirely.
fn build_deb_purl(
    name: &str,
    version: &str,
    arch: Option<&str>,
    namespace: &str,
    distro_version: Option<&str>,
    epoch: Option<u32>,
) -> String {
    // Encode `+` in the name too (e.g. `libstdc++6` → `libstdc%2B%2B6`)
    // and in the version — both use the same rules per reference impl.
    let encoded_name = mikebom_common::types::purl::encode_purl_segment(name);
    let encoded_version = mikebom_common::types::purl::encode_purl_segment(version);
    let mut s = format!("pkg:deb/{namespace}/{encoded_name}@{encoded_version}");
    let mut have_qualifier = false;
    if let Some(a) = arch {
        if !a.is_empty() {
            s.push_str(&format!("?arch={a}"));
            have_qualifier = true;
        }
    }
    if let Some(v) = distro_version {
        if !v.is_empty() {
            s.push(if have_qualifier { '&' } else { '?' });
            s.push_str("distro=");
            s.push_str(namespace);
            s.push('-');
            s.push_str(v);
            have_qualifier = true;
        }
    }
    // Milestone 197 US1 (#562): epoch qualifier — omitted when None or
    // Some(0) per purl-spec convention (mirrors the m190 opkg-side fix
    // pattern at `ipk_file.rs::build_opkg_purl`).
    if let Some(e) = epoch {
        if e != 0 {
            s.push(if have_qualifier { '&' } else { '?' });
            s.push_str(&format!("epoch={e}"));
        }
    }
    s
}

/// Milestone 197 US1 (#562): extract an optional epoch prefix from a
/// Debian-style version string. `<digits>:<upstream-version>-<release>`
/// splits to `(Some(digits), <upstream-version>-<release>)`; strings
/// without a leading digit-run + `:` return `(None, raw)`.
///
/// Mirrors the m190 opkg-side helper at
/// `ipk_file.rs::parse_opkg_version_with_epoch` — the deb / opkg /
/// apk version grammars all inherit the same epoch shape.
fn parse_deb_version_with_epoch(raw: &str) -> (Option<u32>, String) {
    if let Some(colon_pos) = raw.find(':') {
        let (prefix, rest) = raw.split_at(colon_pos);
        if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(v) = prefix.parse::<u32>() {
                return (Some(v), rest[1..].to_string());
            }
        }
    }
    (None, raw.to_string())
}

/// Tokenise a `Depends:` field value into plain package names.
///
/// Input shape: `libc6 (>= 2.34), libjq1 (= 1.6-2.1+deb12u1) | libonig5`
/// Output:      `["libc6", "libjq1", "libonig5"]`
///
/// We drop version constraints (the string inside parentheses) because
/// CycloneDX `dependsOn[]` is a flat reference list — constraints are
/// validated by the package manager at install time, and by the time
/// we're scanning, those edges are already resolved.
///
/// For alternatives (`a | b`), we keep **all** alternates and let the
/// scan orchestrator drop the ones that don't resolve to another entry
/// in this scan. That produces the correct outcome for both cases:
/// - "libjq1 | libonig5" when only libjq1 is installed → edge to libjq1
/// - "libcurl4 | libcurl3-gnutls" when only libcurl4 is installed → edge
///   to libcurl4.
fn parse_depends(raw: &str) -> Vec<String> {
    // dpkg field values can span multiple lines via continuation; a
    // newline between commas is equivalent to a space.
    let flattened: String = raw.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    let mut out = Vec::new();
    for group in flattened.split(',') {
        for alt in group.split('|') {
            let name = alt.trim();
            // Strip version constraint: "name (>= 1.0)" → "name"
            let name = match name.split_once('(') {
                Some((before, _)) => before.trim(),
                None => name,
            };
            // Strip architecture qualifier (multiarch): "name:any" → "name"
            let name = match name.split_once(':') {
                Some((before, _)) => before.trim(),
                None => name,
            };
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const SOURCE: &str = "/var/lib/dpkg/status";

    #[test]
    fn parses_single_installed_package() {
        let text = "\
Package: jq
Status: install ok installed
Version: 1.6-2.1+deb12u1
Architecture: arm64
Maintainer: Debian Jq Maintainers <pkg-jq-maintainers@alioth-lists.debian.net>
Depends: libc6 (>= 2.34), libjq1 (= 1.6-2.1+deb12u1)
Description: command-line JSON processor
";
        let entries = parse(text, SOURCE, "debian", Some("12"));
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.name, "jq");
        // Typed accessors hold the human-readable literal form.
        assert_eq!(e.version, "1.6-2.1+deb12u1");
        assert_eq!(e.arch.as_deref(), Some("arm64"));
        // Canonical PURL encodes `+` as `%2B` per the packageurl-python
        // reference implementation; `:` stays literal. Both the name
        // and version segments get the same treatment (the name rule
        // kicks in for packages like `libstdc++6`, covered in its
        // own test below).
        assert_eq!(
            e.purl.as_str(),
            "pkg:deb/debian/jq@1.6-2.1%2Bdeb12u1?arch=arm64&distro=debian-12"
        );
        assert_eq!(e.depends, vec!["libc6", "libjq1"]);
        assert_eq!(e.source_path, SOURCE);
        // Supplier extracted as the raw "Name <email>" string — the
        // angle-bracket address is preserved because CycloneDX's
        // `supplier.name` is free-form and downstream tooling commonly
        // wants the contact path intact.
        assert_eq!(
            e.maintainer.as_deref(),
            Some("Debian Jq Maintainers <pkg-jq-maintainers@alioth-lists.debian.net>")
        );
    }

    #[test]
    fn missing_maintainer_field_is_none() {
        let text = "\
Package: minimal
Status: install ok installed
Version: 1.0
Architecture: amd64
";
        let entries = parse(text, SOURCE, "debian", None);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].maintainer.is_none());
    }

    #[test]
    fn empty_maintainer_field_is_none() {
        // dpkg occasionally writes `Maintainer: ` (trailing whitespace
        // only). Treat that as absent — we don't want a blank supplier.
        let text = "\
Package: weird
Status: install ok installed
Version: 1.0
Architecture: amd64
Maintainer:
";
        let entries = parse(text, SOURCE, "debian", None);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].maintainer.is_none());
    }

    #[test]
    fn skips_non_installed_status() {
        let text = "\
Package: ghostie
Status: deinstall ok config-files
Version: 1.0
Architecture: amd64
";
        let entries = parse(text, SOURCE, "debian", None);
        assert!(entries.is_empty());
    }

    #[test]
    fn multiple_stanzas_separated_by_blank_lines() {
        let text = "\
Package: a
Status: install ok installed
Version: 1.0
Architecture: amd64

Package: b
Status: install ok installed
Version: 2.0
Architecture: amd64
";
        let entries = parse(text, SOURCE, "debian", None);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a");
        assert_eq!(entries[1].name, "b");
    }

    #[test]
    fn depends_handles_alternatives_and_version_constraints() {
        let out = parse_depends("libc6 (>= 2.34), libjq1 (= 1.6-2.1+deb12u1) | libonig5");
        assert_eq!(out, vec!["libc6", "libjq1", "libonig5"]);
    }

    #[test]
    fn depends_handles_multiarch_qualifier() {
        let out = parse_depends("libc6:any, libdl:amd64");
        assert_eq!(out, vec!["libc6", "libdl"]);
    }

    #[test]
    fn depends_handles_empty_field() {
        assert!(parse_depends("").is_empty());
    }

    #[test]
    fn continuation_lines_extend_preceding_field() {
        // The Description is multi-line; it must not eat subsequent
        // fields. `Depends` that follows the continuation must still
        // parse cleanly.
        let text = "\
Package: foo
Status: install ok installed
Version: 1.0
Architecture: all
Description: first line
 second continuation line
 third continuation line
Depends: libc6, libm6
";
        let entries = parse(text, SOURCE, "debian", None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].depends, vec!["libc6", "libm6"]);
    }

    #[test]
    fn dpkg_name_with_plus_plus_encodes_to_percent_2b() {
        // `libstdc++6` is the canonical example of a deb package whose
        // name carries `++`. Per the packageurl-python reference impl,
        // both `+` must be percent-encoded in the name segment.
        let text = "\
Package: libstdc++6
Status: install ok installed
Version: 12.2.0-14
Architecture: arm64
";
        let entries = parse(text, SOURCE, "debian", Some("12"));
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        // Typed accessors keep the literal form.
        assert_eq!(e.name, "libstdc++6");
        // Canonical PURL encodes `++` → `%2B%2B`.
        assert_eq!(
            e.purl.as_str(),
            "pkg:deb/debian/libstdc%2B%2B6@12.2.0-14?arch=arm64&distro=debian-12"
        );
    }

    #[test]
    fn omits_distro_qualifier_when_codename_absent() {
        let text = "\
Package: foo
Status: install ok installed
Version: 1.0
Architecture: amd64
";
        let entries = parse(text, SOURCE, "debian", None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:deb/debian/foo@1.0?arch=amd64");
    }

    #[test]
    fn missing_required_fields_skip_entry() {
        // No Version — entry is dropped, not fatal.
        let text = "\
Package: onlyname
Status: install ok installed
Architecture: amd64
";
        let entries = parse(text, SOURCE, "debian", None);
        assert!(entries.is_empty());
    }

    #[test]
    fn handles_dpkg_status_that_ends_without_trailing_newline() {
        // Last stanza shouldn't be dropped just because the file lacks
        // a terminating blank line.
        let text = "\
Package: foo
Status: install ok installed
Version: 1.0
Architecture: amd64";
        let entries = parse(text, SOURCE, "debian", None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "foo");
    }

    #[test]
    fn read_function_returns_empty_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        // rootfs with no var/lib/dpkg/status — return empty.
        let out = read(dir.path(), "debian", None).unwrap();
        assert!(out.is_empty());
    }

    // Milestone 100: `#[cfg(unix)]` — dpkg is Debian/Ubuntu-only;
    // tests exercising the reader's filesystem layout (rootfs paths
    // like `/var/lib/dpkg/status`) don't apply on Windows.
    #[cfg(unix)]
    #[test]
    fn read_function_reads_from_rootfs_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(DPKG_STATUS_PATH);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(
            &p,
            "\
Package: curl
Status: install ok installed
Version: 8.0.0
Architecture: arm64
",
        )
        .unwrap();
        let out = read(dir.path(), "debian", Some("12")).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "curl");
        assert!(out[0].source_path.ends_with("/var/lib/dpkg/status"));
    }

    // ---- Milestone 037: status.d/ per-package layout ---------------------

    /// Helper: write a single dpkg stanza to <rootfs>/var/lib/dpkg/status.d/<name>.
    fn write_status_d_stanza(rootfs: &std::path::Path, name: &str, stanza: &str) {
        let dir = rootfs.join("var/lib/dpkg/status.d");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), stanza).unwrap();
    }

    /// FR-005 case 1: status.d/-only layout (distroless / chainguard).
    /// `read()` must discover both stanzas.
    ///
    /// Milestone 100: `#[cfg(unix)]` — dpkg is Linux-only.
    #[cfg(unix)]
    #[test]
    fn parses_status_d_only_layout() {
        let dir = tempfile::tempdir().unwrap();
        write_status_d_stanza(
            dir.path(),
            "foo",
            "Package: foo\n\
             Status: install ok installed\n\
             Version: 1.0\n\
             Architecture: amd64\n",
        );
        write_status_d_stanza(
            dir.path(),
            "bar",
            "Package: bar\n\
             Status: install ok installed\n\
             Version: 2.0\n\
             Architecture: amd64\n",
        );
        let out = read(dir.path(), "debian", Some("12")).unwrap();
        let names: std::collections::BTreeSet<&str> =
            out.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(out.len(), 2);
        assert!(names.contains("foo"));
        assert!(names.contains("bar"));
        // source_path must point at the per-package file, not the
        // monolithic status file.
        for e in &out {
            assert!(
                e.source_path.contains("/status.d/"),
                "expected status.d/ provenance, got {}",
                e.source_path
            );
        }
    }

    /// FR-005 case 2: mixed status + status.d/. Both contribute.
    #[test]
    fn parses_mixed_status_and_status_d() {
        let dir = tempfile::tempdir().unwrap();
        // Legacy status file with pkg-a.
        let status_path = dir.path().join(DPKG_STATUS_PATH);
        std::fs::create_dir_all(status_path.parent().unwrap()).unwrap();
        std::fs::write(
            &status_path,
            "Package: pkg-a\n\
             Status: install ok installed\n\
             Version: 1.0\n\
             Architecture: amd64\n",
        )
        .unwrap();
        // status.d/ with pkg-b.
        write_status_d_stanza(
            dir.path(),
            "pkg-b",
            "Package: pkg-b\n\
             Status: install ok installed\n\
             Version: 2.0\n\
             Architecture: amd64\n",
        );
        let out = read(dir.path(), "debian", Some("12")).unwrap();
        let names: std::collections::BTreeSet<&str> =
            out.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(out.len(), 2);
        assert!(names.contains("pkg-a"));
        assert!(names.contains("pkg-b"));
    }

    /// FR-005 case 3: Status filter applies to status.d/ entries too.
    /// A `deinstall ok config-files` stanza must NOT appear.
    #[test]
    fn status_d_filters_non_installed() {
        let dir = tempfile::tempdir().unwrap();
        write_status_d_stanza(
            dir.path(),
            "keep",
            "Package: keep\n\
             Status: install ok installed\n\
             Version: 1.0\n\
             Architecture: amd64\n",
        );
        write_status_d_stanza(
            dir.path(),
            "drop",
            "Package: drop\n\
             Status: deinstall ok config-files\n\
             Version: 0.9\n\
             Architecture: amd64\n",
        );
        let out = read(dir.path(), "debian", Some("12")).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "keep");
    }

    /// FR-005 case 4: companion files (`<pkg>.md5sums`) parse as
    /// no-ops. The package file alongside still produces an entry.
    #[test]
    fn status_d_skips_companion_files() {
        let dir = tempfile::tempdir().unwrap();
        write_status_d_stanza(
            dir.path(),
            "real-pkg",
            "Package: real-pkg\n\
             Status: install ok installed\n\
             Version: 1.0\n\
             Architecture: amd64\n",
        );
        // Drop a non-stanza companion file alongside (md5sums-style).
        std::fs::write(
            dir.path().join("var/lib/dpkg/status.d/real-pkg.md5sums"),
            "abc123  /usr/bin/real-pkg\n",
        )
        .unwrap();
        let out = read(dir.path(), "debian", Some("12")).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "real-pkg");
    }

    /// FR-005 case 5: empty status.d/ directory yields zero entries
    /// without erroring.
    #[test]
    fn status_d_empty_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("var/lib/dpkg/status.d")).unwrap();
        let out = read(dir.path(), "debian", Some("12")).unwrap();
        assert!(out.is_empty());
    }

    /// FR-003: when both sources contribute the same purl, the
    /// status.d/ entry wins (defensive against pathological mixed
    /// images).
    ///
    /// Milestone 100: `#[cfg(unix)]` — dpkg is Linux-only.
    #[cfg(unix)]
    #[test]
    fn status_d_wins_over_status_on_purl_collision() {
        let dir = tempfile::tempdir().unwrap();
        // Both sources mention `colliding 1.0 amd64` → same PURL.
        let status_path = dir.path().join(DPKG_STATUS_PATH);
        std::fs::create_dir_all(status_path.parent().unwrap()).unwrap();
        std::fs::write(
            &status_path,
            "Package: colliding\n\
             Status: install ok installed\n\
             Version: 1.0\n\
             Architecture: amd64\n",
        )
        .unwrap();
        write_status_d_stanza(
            dir.path(),
            "colliding",
            "Package: colliding\n\
             Status: install ok installed\n\
             Version: 1.0\n\
             Architecture: amd64\n",
        );
        let out = read(dir.path(), "debian", Some("12")).unwrap();
        assert_eq!(out.len(), 1, "duplicate purls should dedup");
        assert!(
            out[0].source_path.contains("/status.d/"),
            "status.d/ should win; got source {}",
            out[0].source_path
        );
    }

    // ---- Feature 005 US2/US3 --------------------------------------------

    /// T028 — build_deb_purl stamps `<ID>-<VERSION_ID>` into the distro
    /// qualifier when both are present.
    #[test]
    fn build_deb_purl_stamps_id_version_qualifier() {
        let purl = build_deb_purl("libc6", "2.36-9", Some("amd64"), "debian", Some("12"), None);
        assert_eq!(
            purl,
            "pkg:deb/debian/libc6@2.36-9?arch=amd64&distro=debian-12"
        );
    }

    /// T029 — `distro_version = None` means no qualifier at all.
    #[test]
    fn build_deb_purl_omits_qualifier_when_distro_version_none() {
        let purl = build_deb_purl("libc6", "2.36-9", Some("amd64"), "debian", None, None);
        assert_eq!(purl, "pkg:deb/debian/libc6@2.36-9?arch=amd64");
    }

    /// T030 — `Some("")` is the same as `None` (empty VERSION_ID
    /// shouldn't produce `distro=debian-` with a trailing dash).
    #[test]
    fn build_deb_purl_omits_qualifier_when_distro_version_empty() {
        let purl = build_deb_purl("libc6", "2.36-9", Some("amd64"), "debian", Some(""), None);
        assert_eq!(purl, "pkg:deb/debian/libc6@2.36-9?arch=amd64");
    }

    /// T033 — the `namespace` parameter drives the PURL path segment
    /// (this is the US3 guarantee). No internal rewrite table — the
    /// caller's value is used verbatim.
    #[test]
    fn build_deb_purl_uses_namespace_parameter() {
        let purl =
            build_deb_purl("libssl3", "3.0.13", Some("amd64"), "ubuntu", Some("24.04"), None);
        assert_eq!(
            purl,
            "pkg:deb/ubuntu/libssl3@3.0.13?arch=amd64&distro=ubuntu-24.04"
        );
    }

    /// T034 — Derivative distros (Kali, Pop!_OS, etc.) must pass
    /// through without silent rewrite to `debian`. The contract is
    /// that whatever `/etc/os-release::ID` says, that's what we put in
    /// the PURL.
    #[test]
    fn build_deb_purl_preserves_raw_id_no_lookup_rewrite() {
        let purl =
            build_deb_purl("foo", "1.0", Some("amd64"), "kali", Some("2024.1"), None);
        assert_eq!(
            purl,
            "pkg:deb/kali/foo@1.0?arch=amd64&distro=kali-2024.1"
        );
    }

    // -----------------------------------------------------------------
    // Milestone 197 US1 (#562): epoch qualifier emission tests
    // -----------------------------------------------------------------

    #[test]
    fn parse_deb_version_with_epoch_extracts_epoch() {
        assert_eq!(parse_deb_version_with_epoch("1:2.0-r0"), (Some(1), "2.0-r0".to_string()));
        assert_eq!(parse_deb_version_with_epoch("2:1.6-2.1+deb12u1"), (Some(2), "1.6-2.1+deb12u1".to_string()));
    }

    #[test]
    fn parse_deb_version_with_epoch_preserves_explicit_zero() {
        // Explicit `0:` is unusual but valid Debian syntax; preserved
        // faithfully (build_deb_purl drops the qualifier when epoch=0).
        assert_eq!(parse_deb_version_with_epoch("0:1.0"), (Some(0), "1.0".to_string()));
    }

    #[test]
    fn parse_deb_version_with_epoch_no_epoch_returns_raw() {
        assert_eq!(parse_deb_version_with_epoch("1.0-r0"), (None, "1.0-r0".to_string()));
        assert_eq!(parse_deb_version_with_epoch("2.36-9"), (None, "2.36-9".to_string()));
    }

    #[test]
    fn parse_deb_version_with_epoch_graceful_on_non_digit_prefix() {
        // `foo:bar` is not epoch syntax — return raw.
        assert_eq!(parse_deb_version_with_epoch("not-a-number:1.0"), (None, "not-a-number:1.0".to_string()));
    }

    #[test]
    fn build_deb_purl_emits_epoch_qualifier_when_nonzero() {
        let purl = build_deb_purl("test-pkg", "2.0-r0", Some("amd64"), "debian", Some("12"), Some(1));
        assert_eq!(purl, "pkg:deb/debian/test-pkg@2.0-r0?arch=amd64&distro=debian-12&epoch=1");
    }

    #[test]
    fn build_deb_purl_omits_epoch_qualifier_when_zero() {
        // Per purl-spec convention (mirrored from m190 opkg): explicit
        // epoch=0 is dropped from the emitted PURL.
        let purl = build_deb_purl("test-pkg", "1.0", Some("amd64"), "debian", Some("12"), Some(0));
        assert_eq!(purl, "pkg:deb/debian/test-pkg@1.0?arch=amd64&distro=debian-12");
    }

    #[test]
    fn build_deb_purl_omits_epoch_qualifier_when_none() {
        // Non-epoch packages are unchanged from pre-m197 output.
        let purl = build_deb_purl("test-pkg", "1.0", Some("amd64"), "debian", Some("12"), None);
        assert_eq!(purl, "pkg:deb/debian/test-pkg@1.0?arch=amd64&distro=debian-12");
    }

    #[test]
    fn scan_epoch_versioned_dpkg_stanza_emits_qualifier_purl() {
        // Full parse-path integration: synthetic dpkg stanza with
        // Version: 1:2.0-r0 must emit `pkg:deb/debian/test-pkg@2.0-r0?arch=amd64&distro=debian-12&epoch=1`.
        // Regression test for m197 US1 / #562.
        let text = "\
Package: test-pkg
Status: install ok installed
Version: 1:2.0-r0
Architecture: amd64
Maintainer: Test <test@example.com>
Description: epoch-versioned test package
";
        let entries = parse(text, SOURCE, "debian", Some("12"));
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:deb/debian/test-pkg@2.0-r0?arch=amd64&distro=debian-12&epoch=1"
        );
    }
}