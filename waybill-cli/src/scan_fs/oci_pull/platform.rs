//! Manifest-list → platform-specific manifest selection
//! (milestone 031, extracted as a submodule by milestone 032,
//! cross-arch override added by milestone 035).
//!
//! When a registry returns an image index (multi-arch manifest
//! list), we pick the entry matching `linux/<arch>[/<variant>]`.
//! The arch defaults to the host machine's; users can override via
//! `--image-platform`.

use anyhow::{anyhow, bail, Result};

/// User-supplied platform parsed from `--image-platform <linux/ARCH[/VARIANT]>`.
///
/// `os` is constrained to `"linux"` — waybill's package-DB readers
/// (dpkg / apk / rpm) are linux-rootfs-shaped, so cross-OS image
/// scans are rejected upfront with a clear error rather than
/// producing an empty SBOM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ParsedPlatform {
    pub os: String,
    pub architecture: String,
    pub variant: Option<String>,
}

/// Parse a `<os>/<arch>[/<variant>]` platform string. The OCI
/// image-spec uses 2- or 3-segment platform identifiers; we accept
/// both shapes and reject anything else with a clear error.
pub(super) fn parse_platform_string(s: &str) -> Result<ParsedPlatform> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        bail!("--image-platform value must not be empty");
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() < 2 || parts.len() > 3 {
        bail!(
            "--image-platform must be `<os>/<arch>` or `<os>/<arch>/<variant>` \
             (got `{trimmed}`); examples: linux/amd64, linux/arm64, linux/arm/v7"
        );
    }
    if parts.iter().any(|p| p.is_empty()) {
        bail!(
            "--image-platform components must be non-empty \
             (got `{trimmed}`); examples: linux/amd64, linux/arm64, linux/arm/v7"
        );
    }
    let os = parts[0].to_string();
    if os != "linux" {
        bail!(
            "--image-platform `{trimmed}`: only `linux/<arch>` is supported. \
             waybill's package-database readers don't apply to non-Linux container \
             images, so cross-OS scans would yield empty SBOMs."
        );
    }
    Ok(ParsedPlatform {
        os,
        architecture: parts[1].to_string(),
        variant: if parts.len() == 3 {
            Some(parts[2].to_string())
        } else {
            None
        },
    })
}

/// Select the digest of the manifest matching `linux/<target_arch>`
/// (and optionally `target_variant`) from a list of `ManifestListEntry`s.
///
/// Variant matching: when `target_variant` is `None`, any entry with
/// matching arch+os matches regardless of variant — this preserves
/// the milestone-031 default behavior, since most images don't set
/// variant. When `target_variant` is `Some`, the entry's variant
/// must exactly match (entry variant=None won't satisfy a user
/// variant=Some request — variant=None means "unspecified", which
/// is a different statement than "matches anything the user asks
/// for").
///
/// Returns the matching digest. Errors when no entry matches,
/// listing the available platforms in the message so the user can
/// see what they could pass.
pub(super) fn resolve_manifest_list_to_linux<I>(
    entries: I,
    target_arch: &str,
    target_variant: Option<&str>,
) -> Result<String>
where
    I: IntoIterator<Item = ManifestListEntry>,
{
    let entries: Vec<ManifestListEntry> = entries.into_iter().collect();
    if let Some(entry) = entries.iter().find(|e| {
        e.os == "linux"
            && e.architecture == target_arch
            && match target_variant {
                None => true,
                Some(v) => e.variant.as_deref() == Some(v),
            }
    }) {
        return Ok(entry.digest.clone());
    }
    let available: Vec<String> = entries
        .iter()
        .map(|e| match e.variant.as_deref() {
            Some(v) => format!("{}/{}/{}", e.os, e.architecture, v),
            None => format!("{}/{}", e.os, e.architecture),
        })
        .collect();
    let target = match target_variant {
        Some(v) => format!("linux/{target_arch}/{v}"),
        None => format!("linux/{target_arch}"),
    };
    Err(anyhow!(
        "no manifest in image index matches {target}; \
         available: [{}]. Pass `--image-platform <linux/arch[/variant]>` \
         to pick one of the available entries.",
        available.join(", ")
    ))
}

/// One entry in a manifest list / image index, in a form
/// decoupled from any specific crate's types. Owned strings —
/// the platform-resolver runs once per pull, so allocation cost
/// is negligible vs. the borrowed-view variant that complicated
/// the milestone-031 → 032 type-conversion path.
#[derive(Clone, Debug)]
pub(super) struct ManifestListEntry {
    pub digest: String,
    pub architecture: String,
    pub os: String,
    pub variant: Option<String>,
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn entry(digest: &str, arch: &str, os: &str) -> ManifestListEntry {
        ManifestListEntry {
            digest: digest.to_string(),
            architecture: arch.to_string(),
            os: os.to_string(),
            variant: None,
        }
    }

    fn entry_with_variant(
        digest: &str,
        arch: &str,
        os: &str,
        variant: &str,
    ) -> ManifestListEntry {
        ManifestListEntry {
            digest: digest.to_string(),
            architecture: arch.to_string(),
            os: os.to_string(),
            variant: Some(variant.to_string()),
        }
    }

    #[test]
    fn picks_linux_amd64_when_present() {
        let entries = vec![
            entry("sha256:amd64", "amd64", "linux"),
            entry("sha256:arm64", "arm64", "linux"),
        ];
        let digest = resolve_manifest_list_to_linux(entries, "amd64", None).unwrap();
        assert_eq!(digest, "sha256:amd64");
    }

    #[test]
    fn picks_first_matching_when_multiples() {
        let entries = vec![
            entry("sha256:first", "amd64", "linux"),
            entry("sha256:second", "amd64", "linux"),
        ];
        let digest = resolve_manifest_list_to_linux(entries, "amd64", None).unwrap();
        assert_eq!(digest, "sha256:first");
    }

    #[test]
    fn errors_when_target_unavailable_and_lists_what_is() {
        let entries = vec![
            entry("sha256:arm64", "arm64", "linux"),
            entry("sha256:s390x", "s390x", "linux"),
        ];
        let err = resolve_manifest_list_to_linux(entries, "amd64", None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("linux/amd64"), "missing target in message: {msg}");
        assert!(msg.contains("linux/arm64"), "missing available platform: {msg}");
        assert!(msg.contains("linux/s390x"), "missing available platform: {msg}");
    }

    #[test]
    fn skips_non_linux_os_entries() {
        let entries = vec![
            entry("sha256:darwin-arm64", "arm64", "darwin"),
            entry("sha256:linux-arm64", "arm64", "linux"),
        ];
        let digest = resolve_manifest_list_to_linux(entries, "arm64", None).unwrap();
        assert_eq!(digest, "sha256:linux-arm64");
    }

    #[test]
    fn variant_none_matches_entry_variant_some() {
        // User asks for "linux/arm64" (no variant) — should pick the
        // single arm64 entry even though it carries variant=v8.
        let entries = vec![entry_with_variant("sha256:arm64v8", "arm64", "linux", "v8")];
        let digest = resolve_manifest_list_to_linux(entries, "arm64", None).unwrap();
        assert_eq!(digest, "sha256:arm64v8");
    }

    #[test]
    fn variant_some_requires_exact_match() {
        // arm/v6 + arm/v7 entries; user asks for v7 — must pick v7.
        let entries = vec![
            entry_with_variant("sha256:armv6", "arm", "linux", "v6"),
            entry_with_variant("sha256:armv7", "arm", "linux", "v7"),
        ];
        let digest = resolve_manifest_list_to_linux(entries, "arm", Some("v7")).unwrap();
        assert_eq!(digest, "sha256:armv7");
    }

    #[test]
    fn variant_some_does_not_match_entry_variant_none() {
        // User asks for v7; only entry has no variant. No match.
        let entries = vec![entry("sha256:arm-no-variant", "arm", "linux")];
        let err = resolve_manifest_list_to_linux(entries, "arm", Some("v7")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("linux/arm/v7"), "target should appear: {msg}");
        assert!(msg.contains("linux/arm"), "available should appear: {msg}");
    }

    #[test]
    fn error_message_lists_variants() {
        let entries = vec![
            entry_with_variant("sha256:armv6", "arm", "linux", "v6"),
            entry_with_variant("sha256:armv7", "arm", "linux", "v7"),
        ];
        let err = resolve_manifest_list_to_linux(entries, "amd64", None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("linux/arm/v6"), "missing v6 in available: {msg}");
        assert!(msg.contains("linux/arm/v7"), "missing v7 in available: {msg}");
    }

    // -------- parse_platform_string --------

    #[test]
    fn parse_accepts_linux_amd64() {
        let p = parse_platform_string("linux/amd64").unwrap();
        assert_eq!(p.os, "linux");
        assert_eq!(p.architecture, "amd64");
        assert_eq!(p.variant, None);
    }

    #[test]
    fn parse_accepts_linux_arm_v7() {
        let p = parse_platform_string("linux/arm/v7").unwrap();
        assert_eq!(p.os, "linux");
        assert_eq!(p.architecture, "arm");
        assert_eq!(p.variant.as_deref(), Some("v7"));
    }

    #[test]
    fn parse_accepts_linux_arm64_v8() {
        let p = parse_platform_string("linux/arm64/v8").unwrap();
        assert_eq!(p.architecture, "arm64");
        assert_eq!(p.variant.as_deref(), Some("v8"));
    }

    #[test]
    fn parse_trims_whitespace() {
        let p = parse_platform_string("  linux/amd64  ").unwrap();
        assert_eq!(p.architecture, "amd64");
    }

    #[test]
    fn parse_rejects_empty_string() {
        assert!(parse_platform_string("").is_err());
        assert!(parse_platform_string("   ").is_err());
    }

    #[test]
    fn parse_rejects_single_segment() {
        assert!(parse_platform_string("amd64").is_err());
    }

    #[test]
    fn parse_rejects_too_many_segments() {
        assert!(parse_platform_string("linux/amd64/v7/extra").is_err());
    }

    #[test]
    fn parse_rejects_empty_components() {
        assert!(parse_platform_string("linux/").is_err());
        assert!(parse_platform_string("/amd64").is_err());
        assert!(parse_platform_string("linux//v7").is_err());
    }

    #[test]
    fn parse_rejects_non_linux_os() {
        let err = parse_platform_string("windows/amd64").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("only `linux/<arch>`"),
            "expected linux-only error: {msg}"
        );
    }
}
