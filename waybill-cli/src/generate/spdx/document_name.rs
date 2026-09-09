//! Source-scan document labels, independent of package identity and output paths.

use std::collections::{BTreeSet, HashMap, HashSet};

use waybill::binding::identifiers::{BuiltinScheme, IdentifierKind};
use waybill_common::attestation::metadata::GenerationContext;

use super::{document::SpdxDocument, relationships::SpdxRelationshipType};
use crate::generate::ScanArtifacts;

pub(super) fn derive(
    document: &SpdxDocument,
    scan: &ScanArtifacts<'_>,
    placeholder_root: bool,
) -> String {
    derive_with_roots(scan, scan.target_name, || {
        (!placeholder_root)
            .then(|| described_name(document))
            .flatten()
    })
}

pub(super) fn derive_v3(
    root_iris: &[String],
    packages: &[serde_json::Value],
    scan: &ScanArtifacts<'_>,
    placeholder_root: bool,
) -> String {
    let legacy_name = if scan.root_override.is_active() {
        scan.root_override
            .name
            .as_deref()
            .unwrap_or(scan.target_name)
    } else {
        scan.target_name
    };
    derive_with_roots(scan, legacy_name, || {
        if placeholder_root {
            return None;
        }
        let packages: HashMap<_, _> = packages
            .iter()
            .filter(|p| p["type"] == "software_Package")
            .filter_map(|p| Some((p["spdxId"].as_str()?, p)))
            .collect();
        // Use the exact rootElement IRIs selected by the emitter, never
        // graph/package position. The label policy is shared with SPDX 2.3.
        format_root_names(root_iris.iter().map(|iri| {
            let package = packages.get(iri.as_str())?;
            Some((
                package["name"].as_str()?,
                package["software_packageVersion"].as_str()?,
            ))
        }))
    })
}

fn derive_with_roots(
    scan: &ScanArtifacts<'_>,
    legacy_name: &str,
    root_name: impl FnOnce() -> Option<String>,
) -> String {
    if let Some(name) = &scan.user_metadata.scan_target_name {
        return name.clone();
    }
    if scan.generation_context != GenerationContext::FilesystemScan {
        return legacy_name.to_string();
    }
    if let Some(name) = root_name() {
        return name;
    }
    if let Some(name) = repository_name(scan) {
        return name;
    }
    tracing::warn!(
        "Cannot derive SPDX document name: no reliable root name/version or repository identity; \
         supply --root-name and --root-version, --repo and --git-ref, or --scan-target-name. \
         Using 'Waybill source scan (identity unavailable)'; checkout directory names are not used."
    );
    "Waybill source scan (identity unavailable)".to_string()
}

fn described_name(document: &SpdxDocument) -> Option<String> {
    let roots: HashSet<_> = document
        .document_describes
        .iter()
        .chain(
            document
                .relationships
                .iter()
                .filter(|r| {
                    r.source == document.spdx_id && r.kind == SpdxRelationshipType::Describes
                })
                .map(|r| &r.target),
        )
        .collect();
    if roots.is_empty() {
        return None;
    }
    let packages: HashMap<_, _> = document.packages.iter().map(|p| (&p.spdx_id, p)).collect();
    format_root_names(roots.into_iter().map(|id| {
        let package = packages.get(id)?;
        Some((package.name.as_str(), package.version_info.as_str()))
    }))
}

fn format_root_names<'a>(
    roots: impl IntoIterator<Item = Option<(&'a str, &'a str)>>,
) -> Option<String> {
    let mut names = BTreeSet::new();
    for root in roots {
        let (name, version) = root?;
        let name = usable_metadata(name)?;
        let version = usable_metadata(version)?;
        names.insert(format!("{name} {version}"));
    }
    (!names.is_empty()).then(|| names.into_iter().collect::<Vec<_>>().join(", "))
}

fn usable_metadata(value: &str) -> Option<&str> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    let temporary = lower.split(['/', '\\']).any(|part| {
        matches!(part, "tmp" | "temp")
            || part.starts_with("tmp.")
            || part.starts_with(".tmp")
            || part.starts_with("tmp-")
            || part.starts_with("temp.")
            || part.starts_with("temp-")
    });
    (!value.is_empty()
        && !matches!(
            lower.as_str(),
            "noassertion"
                | "none"
                | "unknown"
                | "filesystem-scan"
                | "v0.0.0-unknown"
                | "0.0.0-unknown"
        )
        && !temporary)
        .then_some(value)
}

fn repository_name(scan: &ScanArtifacts<'_>) -> Option<String> {
    // Manual identifiers take precedence over auto-detection. Prefer a
    // revision-bearing git identifier within each group; sort ties so the
    // document label never depends on identifier discovery order.
    let mut candidates = Vec::new();
    for id in scan.identifiers {
        let (repo, revision) = match id.kind {
            IdentifierKind::Builtin(BuiltinScheme::Git) => {
                let (repo, revision) = id
                    .value
                    .as_str()
                    .split_once('#')
                    .map_or((id.value.as_str(), None), |(repo, rev)| {
                        (repo, usable_metadata(rev))
                    });
                (repo, revision)
            }
            IdentifierKind::Builtin(BuiltinScheme::Repo) => (id.value.as_str(), None),
            _ => continue,
        };
        let Some(identity) = repository_identity(repo) else {
            continue;
        };
        let name = match revision {
            Some(revision) => format!("{identity} {revision}"),
            None => identity,
        };
        candidates.push((id.source_label.is_some(), revision.is_none(), name));
    }
    candidates.sort();
    let (_, missing_revision, name) = candidates.into_iter().next()?;
    if missing_revision {
        tracing::warn!(
            "SPDX document name uses repository identity without a revision; \
             supply --git-ref to identify the scanned ref"
        );
    }
    Some(name)
}

fn repository_identity(repo: &str) -> Option<String> {
    let url = if repo.contains("://") {
        url::Url::parse(repo).ok()?
    } else {
        let (host, path) = repo.split_once(':')?;
        url::Url::parse(&format!("ssh://{host}/{path}")).ok()?
    };
    if !matches!(url.scheme(), "http" | "https" | "ssh" | "git") || url.host_str().is_none() {
        return None;
    }
    let path = url.path().trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    usable_metadata(path).map(str::to_string)
}
