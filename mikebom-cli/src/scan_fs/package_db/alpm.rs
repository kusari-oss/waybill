//! Parse `/var/lib/pacman/local/*/desc` — the authoritative list of
//! installed packages on an Arch Linux / Manjaro / SteamOS /
//! EndeavourOS / CachyOS system. Milestone 135 (closes #429).
//!
//! Per-package directory layout (stable since pacman 4.0, ~2012):
//!
//! ```text
//! /var/lib/pacman/local/
//! ├── glibc-2.40-1/
//! │   ├── desc        // %KEY%-block metadata stanza
//! │   ├── files       // owned-file manifest (FR-007, US3)
//! │   ├── mtree       // per-file modes / hashes (not consumed v0.1)
//! │   └── install     // optional hook script (not consumed)
//! └── ...
//! ```
//!
//! The `desc` file format is a sequence of header lines `%KEY%` each
//! followed by one or more value lines and a blank-line terminator:
//!
//! ```text
//! %NAME%
//! glibc
//!
//! %VERSION%
//! 2.40-1
//!
//! %DEPENDS%
//! linux-api-headers>=4.10
//! tzdata
//! ...
//! ```
//!
//! Per Constitution Principle V audit (research §R1), the alpm reader
//! introduces NO `mikebom:*` annotation — the standards-native
//! `pkg:alpm/...` PURL (purl-spec `alpm` type) carries the full
//! identity. CycloneDX 1.6, SPDX 2.3, and SPDX 3.0.1 all consume PURLs
//! as first-class component-identity fields.

use std::collections::BTreeSet;
use std::path::Path;

use mikebom_common::types::license::SpdxExpression;
use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::PackageDbEntry;

/// Path to the pacman installed-package directory relative to a rootfs.
const ALPM_LOCAL_DIR: &str = "var/lib/pacman/local";

/// Errors the alpm reader can raise. All current failure modes are
/// non-fatal at the scan level — per-package issues warn-and-skip
/// (FR-009). The enum exists for symmetry with `dpkg::DpkgError` and
/// future fatal-error introduction.
#[derive(Debug, thiserror::Error)]
pub enum AlpmError {
    #[error("alpm reader I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Reader-private parser intermediate for a single `desc` file.
/// Mirrors data-model.md's `PacmanDescStanza` shape.
///
/// Several fields (`description`, `homepage`, `conflicts`, `replaces`,
/// `provides`, `install_reason`) are parsed for completeness but not
/// surfaced to the wire in this milestone — see data-model.md's field
/// mapping table for the deferred-emission entries (e.g. `%URL%` per
/// FR-012 deferral, optdepends per spec edge case).
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PacmanDescStanza {
    name: String,
    version: String,
    arch: String,
    description: Option<String>,
    homepage: Option<String>,
    licenses: Vec<String>,
    packager: Option<String>,
    depends: Vec<String>,
    optdepends: Vec<String>,
    conflicts: Vec<String>,
    replaces: Vec<String>,
    provides: Vec<String>,
    install_reason: Option<u8>,
}

/// Parse a `desc` file's text into a `PacmanDescStanza`. Returns
/// `None` when any required field (`%NAME%`, `%VERSION%`, `%ARCH%`)
/// is missing or empty — caller warn-and-skips per FR-009.
fn parse_desc(text: &str) -> Option<PacmanDescStanza> {
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut arch: Option<String> = None;
    let mut description: Option<String> = None;
    let mut homepage: Option<String> = None;
    let mut licenses: Vec<String> = Vec::new();
    let mut packager: Option<String> = None;
    let mut depends: Vec<String> = Vec::new();
    let mut optdepends: Vec<String> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    let mut replaces: Vec<String> = Vec::new();
    let mut provides: Vec<String> = Vec::new();
    let mut install_reason: Option<u8> = None;

    let mut current_key: Option<String> = None;
    let mut current_values: Vec<String> = Vec::new();

    let flush =
        |key: &Option<String>,
         values: &mut Vec<String>,
         name: &mut Option<String>,
         version: &mut Option<String>,
         arch: &mut Option<String>,
         description: &mut Option<String>,
         homepage: &mut Option<String>,
         licenses: &mut Vec<String>,
         packager: &mut Option<String>,
         depends: &mut Vec<String>,
         optdepends: &mut Vec<String>,
         conflicts: &mut Vec<String>,
         replaces: &mut Vec<String>,
         provides: &mut Vec<String>,
         install_reason: &mut Option<u8>| {
            let Some(k) = key.as_deref() else {
                values.clear();
                return;
            };
            match k {
                "%NAME%" => {
                    *name = values.first().filter(|s| !s.is_empty()).cloned();
                }
                "%VERSION%" => {
                    *version = values.first().filter(|s| !s.is_empty()).cloned();
                }
                "%ARCH%" => {
                    *arch = values.first().filter(|s| !s.is_empty()).cloned();
                }
                "%DESC%" => {
                    *description = if values.is_empty() {
                        None
                    } else {
                        Some(values.join("\n"))
                    };
                }
                "%URL%" => {
                    *homepage = values.first().filter(|s| !s.is_empty()).cloned();
                }
                "%LICENSE%" => {
                    licenses.extend(values.iter().filter(|s| !s.is_empty()).cloned());
                }
                "%PACKAGER%" => {
                    *packager = values.first().filter(|s| !s.is_empty()).cloned();
                }
                "%DEPENDS%" => {
                    depends.extend(values.iter().filter(|s| !s.is_empty()).cloned());
                }
                "%OPTDEPENDS%" => {
                    optdepends.extend(values.iter().filter(|s| !s.is_empty()).cloned());
                }
                "%CONFLICTS%" => {
                    conflicts.extend(values.iter().filter(|s| !s.is_empty()).cloned());
                }
                "%REPLACES%" => {
                    replaces.extend(values.iter().filter(|s| !s.is_empty()).cloned());
                }
                "%PROVIDES%" => {
                    provides.extend(values.iter().filter(|s| !s.is_empty()).cloned());
                }
                "%REASON%" => {
                    *install_reason = values.first().and_then(|s| s.parse::<u8>().ok());
                }
                _ => {
                    // Unknown key — ignore (forward-compatibility for
                    // future pacman header additions).
                }
            }
            values.clear();
        };

    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.starts_with('%') && trimmed.ends_with('%') {
            // Header line — flush the previous block, start a new one.
            flush(
                &current_key,
                &mut current_values,
                &mut name,
                &mut version,
                &mut arch,
                &mut description,
                &mut homepage,
                &mut licenses,
                &mut packager,
                &mut depends,
                &mut optdepends,
                &mut conflicts,
                &mut replaces,
                &mut provides,
                &mut install_reason,
            );
            current_key = Some(trimmed.to_string());
        } else if trimmed.is_empty() {
            // Blank line — terminates the current block.
            flush(
                &current_key,
                &mut current_values,
                &mut name,
                &mut version,
                &mut arch,
                &mut description,
                &mut homepage,
                &mut licenses,
                &mut packager,
                &mut depends,
                &mut optdepends,
                &mut conflicts,
                &mut replaces,
                &mut provides,
                &mut install_reason,
            );
            current_key = None;
        } else {
            current_values.push(trimmed.to_string());
        }
    }
    // EOF: flush the final block in case the file didn't end with a
    // blank line.
    flush(
        &current_key,
        &mut current_values,
        &mut name,
        &mut version,
        &mut arch,
        &mut description,
        &mut homepage,
        &mut licenses,
        &mut packager,
        &mut depends,
        &mut optdepends,
        &mut conflicts,
        &mut replaces,
        &mut provides,
        &mut install_reason,
    );

    Some(PacmanDescStanza {
        name: name?,
        version: version?,
        arch: arch?,
        description,
        homepage,
        licenses,
        packager,
        depends,
        optdepends,
        conflicts,
        replaces,
        provides,
        install_reason,
    })
}

/// Build a `pkg:alpm/<namespace>/<name>@<version>?arch=<arch>[&distro=<ns>-<verid>]`
/// PURL per the purl-spec `alpm` type. Wire format per
/// `specs/135-arch-alpm-reader/contracts/alpm-component-purl.md`.
fn build_alpm_purl(
    namespace: &str,
    name: &str,
    version: &str,
    arch: &str,
    distro_qualifier: Option<&str>,
) -> Option<Purl> {
    let encoded_ns = encode_purl_segment(namespace);
    let encoded_name = encode_purl_segment(name);
    let encoded_version = encode_purl_segment(version);
    let encoded_arch = encode_purl_segment(arch);
    let purl_str = match distro_qualifier {
        Some(distro) if !distro.is_empty() => format!(
            "pkg:alpm/{encoded_ns}/{encoded_name}@{encoded_version}?arch={encoded_arch}&distro={}",
            encode_purl_segment(distro),
        ),
        _ => format!(
            "pkg:alpm/{encoded_ns}/{encoded_name}@{encoded_version}?arch={encoded_arch}",
        ),
    };
    Purl::new(&purl_str).ok()
}

/// Split a pacman dep spec into its name half. Pacman dep specs may
/// include version constraints (e.g., `glibc>=2.40`). The dep graph
/// only uses the name; the constraint is informational.
fn strip_dep_constraint(spec: &str) -> &str {
    for op in ["<=", ">=", "=", "<", ">"] {
        if let Some(idx) = spec.find(op) {
            return spec[..idx].trim();
        }
    }
    spec.trim()
}

/// Convert a `PacmanDescStanza` into a `PackageDbEntry` per
/// data-model.md's field mapping table.
fn stanza_to_entry(
    stanza: PacmanDescStanza,
    namespace: &str,
    distro_qualifier: Option<&str>,
    source_path: String,
) -> Option<PackageDbEntry> {
    let purl = build_alpm_purl(
        namespace,
        &stanza.name,
        &stanza.version,
        &stanza.arch,
        distro_qualifier,
    )?;

    // Dep names — strip version constraints; deduplicate via
    // BTreeSet for stable order.
    let dep_names: Vec<String> = stanza
        .depends
        .iter()
        .map(|s| strip_dep_constraint(s).to_string())
        .filter(|s| !s.is_empty())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();

    // Licenses — canonicalize each line; fall back to verbatim string
    // when canonicalization fails (milestone-012 LicenseRef discipline).
    let licenses: Vec<SpdxExpression> = stanza
        .licenses
        .iter()
        .filter_map(|raw| SpdxExpression::try_canonical(raw).ok())
        .collect();

    Some(PackageDbEntry {
        purl,
        name: stanza.name,
        version: stanza.version,
        arch: Some(stanza.arch),
        source_path,
        depends: dep_names,
        maintainer: stanza.packager,
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: Some("alpm".to_string()),
        licenses,
        buildinfo_status: None,
        sbom_tier: Some("deployed".to_string()),
        evidence_kind: Some("alpm-local-db".to_string()),
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
        shade_relocation: None,
        extra_annotations: std::collections::BTreeMap::new(),
        binary_role: None,
        build_inclusion: None,
    })
}

/// Walk `<rootfs>/var/lib/pacman/local/*/desc` and emit one
/// `PackageDbEntry` per installed package.
///
/// Returns `Ok(vec![])` cleanly when the pacman DB is absent or
/// empty (FR-008 — no-op, no warnings logged).
///
/// Per-package parse failures emit `tracing::warn!` naming the
/// offending package and continue (FR-009 — partial output is more
/// valuable than no output).
///
/// * `namespace` — the alpm PURL namespace segment (e.g., `"arch"`,
///   `"manjaro"`, `"steamos"`). Derived by the caller (`read_all`)
///   from `/etc/os-release::ID` with `"arch"` as the fallback.
/// * `distro_version` — optional `VERSION_ID`. When `Some(non_empty)`,
///   emitted as `&distro=<namespace>-<version>` on every PURL.
///   Rolling-release Arch (no VERSION_ID) gets unqualified PURLs.
pub fn read(
    rootfs: &Path,
    namespace: &str,
    distro_version: Option<&str>,
) -> Result<Vec<PackageDbEntry>, AlpmError> {
    let local_dir = rootfs.join(ALPM_LOCAL_DIR);
    if !local_dir.is_dir() {
        return Ok(Vec::new());
    }

    let distro_qualifier: Option<String> =
        distro_version.and_then(|v| {
            if v.is_empty() {
                None
            } else {
                Some(format!("{namespace}-{v}"))
            }
        });

    // Iterate package directories in sorted order for deterministic
    // output across runs (mirrors dpkg's status.d ordering discipline).
    let mut pkg_dirs: Vec<std::path::PathBuf> = match std::fs::read_dir(&local_dir) {
        Ok(rd) => rd
            .filter_map(|r| r.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(e) => {
            return Err(AlpmError::Io {
                path: local_dir,
                source: e,
            })
        }
    };
    pkg_dirs.sort();

    let mut out: Vec<PackageDbEntry> = Vec::with_capacity(pkg_dirs.len());
    for pkg_dir in pkg_dirs {
        let desc_path = pkg_dir.join("desc");
        if !desc_path.is_file() {
            // Some entries in local/ (e.g., the `ALPM_DB_VERSION` file
            // at the directory root) aren't package directories.
            continue;
        }
        let text = match std::fs::read_to_string(&desc_path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    path = %desc_path.display(),
                    error = %e,
                    "pacman: failed to read desc file, skipping package",
                );
                continue;
            }
        };
        let Some(stanza) = parse_desc(&text) else {
            tracing::warn!(
                path = %desc_path.display(),
                "pacman: desc missing required %NAME%/%VERSION%/%ARCH%, skipping package",
            );
            continue;
        };
        let source_path = desc_path.to_string_lossy().into_owned();
        let Some(entry) = stanza_to_entry(
            stanza,
            namespace,
            distro_qualifier.as_deref(),
            source_path,
        ) else {
            tracing::warn!(
                path = %desc_path.display(),
                "pacman: failed to construct PURL for stanza, skipping package",
            );
            continue;
        };
        out.push(entry);
    }

    if !out.is_empty() {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
            namespace,
            "parsed pacman local database",
        );
    }
    Ok(out)
}

/// Milestone 004 file-claim integration (US3 / FR-007). Walks
/// `<rootfs>/var/lib/pacman/local/*/files`, parses the `%FILES%`
/// block, and inserts every non-directory path into the shared
/// claim sets so the binary walker skips emission of `pkg:generic/*`
/// components for paths owned by a pacman package.
///
/// Per-file resolve errors are warn-and-skip per FR-009.
pub fn collect_claimed_paths(
    rootfs: &Path,
    claimed: &mut std::collections::HashSet<std::path::PathBuf>,
    #[cfg(unix)] claimed_inodes: &mut std::collections::HashSet<(u64, u64)>,
) {
    let local_dir = rootfs.join(ALPM_LOCAL_DIR);
    if !local_dir.is_dir() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(&local_dir) else {
        return;
    };
    for entry in rd.filter_map(|r| r.ok()) {
        let pkg_dir = entry.path();
        if !pkg_dir.is_dir() {
            continue;
        }
        let files_path = pkg_dir.join("files");
        let Ok(text) = std::fs::read_to_string(&files_path) else {
            // Missing `files` manifest — warn-and-continue. Component
            // identity still emits via `read()`; binary walker may
            // produce duplicates as a soft regression.
            tracing::warn!(
                path = %files_path.display(),
                "pacman: missing files manifest, skipping file-claim registration",
            );
            continue;
        };
        let mut in_files_block = false;
        for line in text.lines() {
            let trimmed = line.trim_end();
            if trimmed == "%FILES%" {
                in_files_block = true;
                continue;
            }
            if trimmed.starts_with('%') && trimmed.ends_with('%') {
                in_files_block = false;
                continue;
            }
            if !in_files_block || trimmed.is_empty() {
                continue;
            }
            // Directory entries end with `/` — skip them.
            if trimmed.ends_with('/') {
                continue;
            }
            let resolved = rootfs.join(trimmed);
            #[cfg(unix)]
            {
                if let Ok(metadata) = std::fs::metadata(&resolved) {
                    use std::os::unix::fs::MetadataExt;
                    claimed_inodes.insert((metadata.dev(), metadata.ino()));
                }
            }
            claimed.insert(resolved);
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // -------- parse_desc --------

    #[test]
    fn parse_desc_minimal_stanza() {
        let text = "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n";
        let stanza = parse_desc(text).unwrap();
        assert_eq!(stanza.name, "bash");
        assert_eq!(stanza.version, "5.2.026-1");
        assert_eq!(stanza.arch, "x86_64");
        assert!(stanza.depends.is_empty());
        assert!(stanza.licenses.is_empty());
    }

    #[test]
    fn parse_desc_full_stanza() {
        let text = "\
%NAME%
curl

%VERSION%
8.5.0-1

%ARCH%
x86_64

%DESC%
URL retrieval utility and library

%URL%
https://curl.se/

%LICENSE%
MIT
GPL-3.0-or-later

%PACKAGER%
Levente Polyak <anthraxx@archlinux.org>

%DEPENDS%
glibc
brotli
zlib>=1.2

%OPTDEPENDS%
ca-certificates: TLS support
nghttp2: HTTP/2 support

%CONFLICTS%
curl-old

%REPLACES%
curl-deprecated

%PROVIDES%
libcurl.so=4-64

%REASON%
0
";
        let stanza = parse_desc(text).unwrap();
        assert_eq!(stanza.name, "curl");
        assert_eq!(stanza.version, "8.5.0-1");
        assert_eq!(stanza.arch, "x86_64");
        assert_eq!(stanza.description.as_deref(), Some("URL retrieval utility and library"));
        assert_eq!(stanza.homepage.as_deref(), Some("https://curl.se/"));
        assert_eq!(stanza.licenses, vec!["MIT".to_string(), "GPL-3.0-or-later".to_string()]);
        assert_eq!(
            stanza.packager.as_deref(),
            Some("Levente Polyak <anthraxx@archlinux.org>")
        );
        assert_eq!(stanza.depends.len(), 3);
        assert_eq!(stanza.optdepends.len(), 2);
        assert_eq!(stanza.conflicts, vec!["curl-old".to_string()]);
        assert_eq!(stanza.replaces, vec!["curl-deprecated".to_string()]);
        assert_eq!(stanza.provides, vec!["libcurl.so=4-64".to_string()]);
        assert_eq!(stanza.install_reason, Some(0));
    }

    #[test]
    fn parse_desc_missing_name_returns_none() {
        let text = "%VERSION%\n1.0\n\n%ARCH%\nx86_64\n";
        assert!(parse_desc(text).is_none());
    }

    #[test]
    fn parse_desc_missing_version_returns_none() {
        let text = "%NAME%\nfoo\n\n%ARCH%\nx86_64\n";
        assert!(parse_desc(text).is_none());
    }

    #[test]
    fn parse_desc_missing_arch_returns_none() {
        let text = "%NAME%\nfoo\n\n%VERSION%\n1.0\n";
        assert!(parse_desc(text).is_none());
    }

    #[test]
    fn parse_desc_noarch_any() {
        let text = "%NAME%\nterminfo\n\n%VERSION%\n6.4-3\n\n%ARCH%\nany\n";
        let stanza = parse_desc(text).unwrap();
        assert_eq!(stanza.arch, "any");
    }

    #[test]
    fn parse_desc_no_final_newline() {
        // Stanza without trailing blank line — EOF flush handles it.
        let text = "%NAME%\nfoo\n\n%VERSION%\n1.0\n\n%ARCH%\nx86_64";
        let stanza = parse_desc(text).unwrap();
        assert_eq!(stanza.name, "foo");
        assert_eq!(stanza.arch, "x86_64");
    }

    // -------- build_alpm_purl --------

    #[test]
    fn build_purl_stock_arch_no_distro_qualifier() {
        let purl =
            build_alpm_purl("arch", "bash", "5.2.026-1", "x86_64", None).unwrap();
        assert_eq!(purl.as_str(), "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64");
    }

    #[test]
    fn build_purl_steamos_with_distro_qualifier() {
        let purl = build_alpm_purl(
            "steamos",
            "bash",
            "5.2.026-1",
            "x86_64",
            Some("steamos-3.5.7"),
        )
        .unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:alpm/steamos/bash@5.2.026-1?arch=x86_64&distro=steamos-3.5.7"
        );
    }

    #[test]
    fn build_purl_noarch_any() {
        let purl =
            build_alpm_purl("arch", "terminfo", "6.4-3", "any", None).unwrap();
        assert_eq!(purl.as_str(), "pkg:alpm/arch/terminfo@6.4-3?arch=any");
    }

    #[test]
    fn build_purl_hyphenated_name() {
        let purl =
            build_alpm_purl("arch", "lib32-glibc", "2.40-1", "x86_64", None).unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:alpm/arch/lib32-glibc@2.40-1?arch=x86_64"
        );
    }

    // -------- strip_dep_constraint --------

    #[test]
    fn dep_constraint_bare_name() {
        assert_eq!(strip_dep_constraint("glibc"), "glibc");
    }

    #[test]
    fn dep_constraint_with_ge() {
        assert_eq!(strip_dep_constraint("glibc>=2.40"), "glibc");
    }

    #[test]
    fn dep_constraint_with_lt() {
        assert_eq!(strip_dep_constraint("foo<5"), "foo");
    }

    #[test]
    fn dep_constraint_with_eq() {
        assert_eq!(strip_dep_constraint("libcurl.so=4-64"), "libcurl.so");
    }

    // -------- stanza_to_entry --------

    #[test]
    fn stanza_to_entry_basic() {
        let stanza = PacmanDescStanza {
            name: "bash".to_string(),
            version: "5.2.026-1".to_string(),
            arch: "x86_64".to_string(),
            description: Some("GNU Bash".to_string()),
            homepage: Some("https://www.gnu.org/software/bash/".to_string()),
            licenses: vec!["GPL-3.0-or-later".to_string()],
            packager: Some("Levente <l@arch>".to_string()),
            depends: vec!["glibc".to_string(), "readline>=7.0".to_string()],
            optdepends: vec!["bash-completion: tab completion".to_string()],
            conflicts: vec![],
            replaces: vec![],
            provides: vec![],
            install_reason: Some(0),
        };
        let entry = stanza_to_entry(
            stanza,
            "arch",
            None,
            "/tmp/var/lib/pacman/local/bash-5.2.026-1/desc".to_string(),
        )
        .unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64");
        assert_eq!(entry.name, "bash");
        assert_eq!(entry.version, "5.2.026-1");
        assert_eq!(entry.arch.as_deref(), Some("x86_64"));
        // depends stripped + deduped: BTreeSet sorts lex
        assert_eq!(entry.depends, vec!["glibc".to_string(), "readline".to_string()]);
        assert_eq!(entry.source_type.as_deref(), Some("alpm"));
        assert_eq!(entry.sbom_tier.as_deref(), Some("deployed"));
        assert_eq!(entry.evidence_kind.as_deref(), Some("alpm-local-db"));
        assert_eq!(entry.maintainer.as_deref(), Some("Levente <l@arch>"));
        assert_eq!(entry.licenses.len(), 1);
    }

    #[test]
    fn stanza_to_entry_optdepends_excluded_from_depends() {
        let stanza = PacmanDescStanza {
            name: "x".to_string(),
            version: "1".to_string(),
            arch: "x86_64".to_string(),
            description: None,
            homepage: None,
            licenses: vec![],
            packager: None,
            depends: vec!["a".to_string()],
            optdepends: vec!["b: reason".to_string()],
            conflicts: vec![],
            replaces: vec![],
            provides: vec![],
            install_reason: None,
        };
        let entry =
            stanza_to_entry(stanza, "arch", None, String::new()).unwrap();
        assert_eq!(entry.depends, vec!["a".to_string()]);
        // b (optdep) MUST NOT appear in depends.
        assert!(!entry.depends.contains(&"b".to_string()));
    }

    // -------- read() — directory-walk happy path + edge cases --------

    fn write_pkg(rootfs: &std::path::Path, dir_name: &str, desc_body: &str) {
        let pkg_dir = rootfs.join("var/lib/pacman/local").join(dir_name);
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("desc"), desc_body).unwrap();
    }

    #[test]
    fn read_empty_rootfs_returns_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let entries = read(tmp.path(), "arch", None).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_no_pacman_dir_returns_zero_no_warn() {
        let tmp = tempfile::tempdir().unwrap();
        // Create only an unrelated path
        std::fs::create_dir_all(tmp.path().join("etc")).unwrap();
        let entries = read(tmp.path(), "arch", None).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_three_packages_arch_namespace() {
        let tmp = tempfile::tempdir().unwrap();
        write_pkg(
            tmp.path(),
            "bash-5.2.026-1",
            "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n\n%DEPENDS%\nglibc\n",
        );
        write_pkg(
            tmp.path(),
            "glibc-2.40-1",
            "%NAME%\nglibc\n\n%VERSION%\n2.40-1\n\n%ARCH%\nx86_64\n",
        );
        write_pkg(
            tmp.path(),
            "curl-8.5.0-1",
            "%NAME%\ncurl\n\n%VERSION%\n8.5.0-1\n\n%ARCH%\nx86_64\n\n%DEPENDS%\nglibc\nbrotli\n",
        );
        let entries = read(tmp.path(), "arch", None).unwrap();
        assert_eq!(entries.len(), 3);
        let purls: Vec<&str> = entries.iter().map(|e| e.purl.as_str()).collect();
        assert!(purls.contains(&"pkg:alpm/arch/bash@5.2.026-1?arch=x86_64"));
        assert!(purls.contains(&"pkg:alpm/arch/glibc@2.40-1?arch=x86_64"));
        assert!(purls.contains(&"pkg:alpm/arch/curl@8.5.0-1?arch=x86_64"));
    }

    #[test]
    fn read_steamos_namespace_with_distro_qualifier() {
        let tmp = tempfile::tempdir().unwrap();
        write_pkg(
            tmp.path(),
            "bash-5.2.026-1",
            "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n",
        );
        let entries = read(tmp.path(), "steamos", Some("3.5.7")).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:alpm/steamos/bash@5.2.026-1?arch=x86_64&distro=steamos-3.5.7"
        );
    }

    #[test]
    fn read_malformed_desc_warns_and_continues() {
        let tmp = tempfile::tempdir().unwrap();
        write_pkg(
            tmp.path(),
            "good-1.0-1",
            "%NAME%\ngood\n\n%VERSION%\n1.0-1\n\n%ARCH%\nx86_64\n",
        );
        // Missing %NAME%
        write_pkg(
            tmp.path(),
            "broken-1.0-1",
            "%VERSION%\n1.0-1\n\n%ARCH%\nx86_64\n",
        );
        let entries = read(tmp.path(), "arch", None).unwrap();
        // Only the good package survives.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "good");
    }

    // -------- collect_claimed_paths --------

    #[test]
    fn collect_claimed_paths_skips_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = tmp.path().join("var/lib/pacman/local/bash-5.2.026-1");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("desc"), "%NAME%\nbash\n").unwrap();
        std::fs::write(
            pkg_dir.join("files"),
            "%FILES%\nusr/\nusr/bin/\nusr/bin/bash\nusr/share/man/man1/bash.1.gz\n",
        )
        .unwrap();
        let mut claimed = std::collections::HashSet::new();
        #[cfg(unix)]
        let mut claimed_inodes = std::collections::HashSet::new();
        collect_claimed_paths(
            tmp.path(),
            &mut claimed,
            #[cfg(unix)]
            &mut claimed_inodes,
        );
        // Directories (trailing /) are NOT inserted; files ARE.
        assert!(claimed.contains(&tmp.path().join("usr/bin/bash")));
        assert!(claimed.contains(&tmp.path().join("usr/share/man/man1/bash.1.gz")));
        assert!(!claimed.contains(&tmp.path().join("usr")));
        assert!(!claimed.contains(&tmp.path().join("usr/bin")));
    }

    #[test]
    fn collect_claimed_paths_missing_files_manifest_continues() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = tmp.path().join("var/lib/pacman/local/x-1-1");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("desc"), "%NAME%\nx\n").unwrap();
        // No files manifest.
        let mut claimed = std::collections::HashSet::new();
        #[cfg(unix)]
        let mut claimed_inodes = std::collections::HashSet::new();
        collect_claimed_paths(
            tmp.path(),
            &mut claimed,
            #[cfg(unix)]
            &mut claimed_inodes,
        );
        assert!(claimed.is_empty());
    }

    #[test]
    fn collect_claimed_paths_no_pacman_dir_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let mut claimed = std::collections::HashSet::new();
        #[cfg(unix)]
        let mut claimed_inodes = std::collections::HashSet::new();
        collect_claimed_paths(
            tmp.path(),
            &mut claimed,
            #[cfg(unix)]
            &mut claimed_inodes,
        );
        assert!(claimed.is_empty());
    }
}
