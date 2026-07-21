//! OCI Distribution Spec v1.1 Referrers API — SBOM discovery + descriptor
//! filter (milestone 186, #442).
//!
//! When the operator passes `--sbom-source referrer|either`, waybill queries
//! `/v2/<repo>/referrers/<manifest-digest>` for descriptors advertising
//! attached artifacts (SBOMs, attestations, signatures). This module carries
//! the SBOM media-type filter + priority-ordering picker used by
//! `super::try_fetch_referrer_sbom`.
//!
//! The referrer's blob body is emitted BYTE-IDENTICALLY to the operator's
//! `--output` path — no re-parse, no re-encode, no PURL rewriting, no
//! transcoding. This preserves any upstream signer's Cosign / in-toto DSSE
//! byte-identity contract (spec.md §Deferred defers signed-verification to a
//! follow-up milestone).

use oci_spec::image::{Descriptor, ImageIndex};

/// SBOM media types recognized by the referrer filter.
///
/// Kept in-sync with `media_type_for_mikebom_format` — the "Tier 2 CDX-first
/// fallback" iter order in `pick_sbom_descriptor` is derived from this list's
/// ordering: CDX+JSON, SPDX+JSON, CDX+XML. Extending the list (e.g., SPDX 3
/// once the ecosystem's SPDX 3 referrer media type is stable) is additive
/// and does not break m186's contract.
pub(super) const SBOM_MEDIA_TYPES: &[&str] = &[
    "application/vnd.cyclonedx+json",
    "application/spdx+json",
    "application/vnd.cyclonedx+xml",
];

/// Map waybill's `--format` value to the referrer descriptor media type it
/// corresponds to.
///
/// Used by [`pick_sbom_descriptor`] for the format-match preference (Tier 1
/// per research.md Decision 2). Returns `None` for formats without a
/// well-known referrer media type equivalent (SPDX 3 currently, since the
/// ecosystem hasn't converged on a stable `application/vnd.spdx+json`
/// registration for SPDX 3 as of m186's landing).
pub(super) fn media_type_for_mikebom_format(fmt: &str) -> Option<&'static str> {
    match fmt {
        "cyclonedx-json" => Some("application/vnd.cyclonedx+json"),
        "spdx-2.3-json" => Some("application/spdx+json"),
        _ => None,
    }
}

/// Pick the best SBOM descriptor from a Referrers-API response.
///
/// Priority order per research.md Decision 2:
///   1. Explicit format match — the descriptor whose media type matches the
///      first `--format` value the operator requested.
///   2. CDX-first fallback — prefer CDX+JSON, then SPDX+JSON, then CDX+XML.
///   3. First-descriptor tiebreaker — the FIRST candidate that survived the
///      SBOM-media-type + size-cap filters (deterministic ordering per
///      Distribution Spec v1.1 §Referrers Response).
///
/// Returns `None` when the response contains zero SBOM-shaped descriptors OR
/// when every SBOM-shaped descriptor exceeds the size cap.
pub(super) fn pick_sbom_descriptor<'a>(
    index: &'a ImageIndex,
    requested_formats: &[&str],
    max_bytes: u64,
) -> Option<&'a Descriptor> {
    let candidates: Vec<&Descriptor> = index
        .manifests()
        .iter()
        .filter(|d| SBOM_MEDIA_TYPES.contains(&d.media_type().as_ref()))
        .filter(|d| {
            if d.size() > max_bytes {
                tracing::warn!(
                    digest = %d.digest(),
                    declared_size = d.size(),
                    cap = max_bytes,
                    "skipping oversize referrer descriptor — override via WAYBILL_REFERRER_MAX_BYTES env var if trusted"
                );
                false
            } else {
                true
            }
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }

    for fmt in requested_formats {
        if let Some(target_mt) = media_type_for_mikebom_format(fmt) {
            if let Some(d) = candidates
                .iter()
                .find(|d| d.media_type().as_ref() == target_mt)
            {
                return Some(*d);
            }
        }
    }

    for target_mt in SBOM_MEDIA_TYPES {
        if let Some(d) = candidates
            .iter()
            .find(|d| d.media_type().as_ref() == *target_mt)
        {
            return Some(*d);
        }
    }

    candidates.into_iter().next()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use oci_spec::image::{
        Digest, DescriptorBuilder, ImageIndexBuilder, MediaType, SCHEMA_VERSION,
    };
    use std::str::FromStr;

    fn descriptor(media_type: &str, digest: &str, size: u64) -> Descriptor {
        DescriptorBuilder::default()
            .media_type(MediaType::from(media_type))
            .digest(Digest::from_str(digest).unwrap())
            .size(size)
            .build()
            .unwrap()
    }

    fn image_index(manifests: Vec<Descriptor>) -> ImageIndex {
        ImageIndexBuilder::default()
            .schema_version(SCHEMA_VERSION)
            .manifests(manifests)
            .build()
            .unwrap()
    }

    const SHA_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const SHA_C: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn media_type_for_mikebom_format_maps_cdx_and_spdx23() {
        assert_eq!(
            media_type_for_mikebom_format("cyclonedx-json"),
            Some("application/vnd.cyclonedx+json")
        );
        assert_eq!(
            media_type_for_mikebom_format("spdx-2.3-json"),
            Some("application/spdx+json")
        );
        // Unmapped format ids return None (verifies the `_ =>` arm).
        // We deliberately do NOT reference specific unmapped format
        // identifiers here — the m013 leak-audit test in
        // `tests/spdx3_us3_acceptance.rs` scans this file's source for
        // format-specific tokens and would trip on those literals.
        assert_eq!(media_type_for_mikebom_format("gibberish"), None);
        assert_eq!(media_type_for_mikebom_format("cyclonedx-xml"), None);
    }

    #[test]
    fn pick_sbom_descriptor_prefers_format_match() {
        let idx = image_index(vec![
            descriptor("application/vnd.cyclonedx+json", SHA_A, 1024),
            descriptor("application/spdx+json", SHA_B, 1024),
        ]);
        let picked = pick_sbom_descriptor(&idx, &["spdx-2.3-json"], 100 * 1024 * 1024).unwrap();
        assert_eq!(picked.digest().to_string(), SHA_B);
    }

    #[test]
    fn pick_sbom_descriptor_cdx_first_fallback() {
        let idx = image_index(vec![
            descriptor("application/spdx+json", SHA_A, 1024),
            descriptor("application/vnd.cyclonedx+json", SHA_B, 1024),
        ]);
        let picked = pick_sbom_descriptor(&idx, &[], 100 * 1024 * 1024).unwrap();
        assert_eq!(
            picked.media_type().as_ref(),
            "application/vnd.cyclonedx+json"
        );
        assert_eq!(picked.digest().to_string(), SHA_B);
    }

    #[test]
    fn pick_sbom_descriptor_first_descriptor_tiebreaker() {
        // Two same-media-type descriptors; no format match; CDX-first pass
        // resolves to whichever CDX+JSON appears first via `Iterator::find`.
        let idx = image_index(vec![
            descriptor("application/vnd.cyclonedx+json", SHA_A, 1024),
            descriptor("application/vnd.cyclonedx+json", SHA_B, 1024),
        ]);
        let picked = pick_sbom_descriptor(&idx, &[], 100 * 1024 * 1024).unwrap();
        assert_eq!(picked.digest().to_string(), SHA_A);
    }

    #[test]
    fn pick_sbom_descriptor_returns_none_on_empty_index() {
        let idx = image_index(vec![]);
        assert!(pick_sbom_descriptor(&idx, &[], 100 * 1024 * 1024).is_none());
    }

    #[test]
    fn pick_sbom_descriptor_returns_none_on_non_sbom_types() {
        let idx = image_index(vec![
            descriptor("application/vnd.in-toto+json", SHA_A, 1024),
            descriptor(
                "application/vnd.dev.cosign.simplesigning.v1+json",
                SHA_B,
                1024,
            ),
        ]);
        assert!(pick_sbom_descriptor(&idx, &[], 100 * 1024 * 1024).is_none());
    }

    #[test]
    fn pick_sbom_descriptor_skips_attestation_envelopes() {
        // F8 remediation — attestation-envelope descriptors are filtered out
        // by the SBOM_MEDIA_TYPES membership check; the CDX descriptor wins.
        let idx = image_index(vec![
            descriptor("application/vnd.in-toto+json", SHA_A, 1024),
            descriptor("application/vnd.cyclonedx+json", SHA_B, 1024),
        ]);
        let picked = pick_sbom_descriptor(&idx, &[], 100 * 1024 * 1024).unwrap();
        assert_eq!(picked.digest().to_string(), SHA_B);
    }

    #[test]
    fn pick_sbom_descriptor_skips_oversize_descriptors() {
        // The oversize CDX is skipped; the smaller SPDX wins via CDX-first
        // fallback exhausting to SPDX+JSON.
        let idx = image_index(vec![
            descriptor("application/vnd.cyclonedx+json", SHA_A, 200 * 1024 * 1024),
            descriptor("application/spdx+json", SHA_B, 1024),
        ]);
        let cap = 100 * 1024 * 1024;
        let picked = pick_sbom_descriptor(&idx, &[], cap).unwrap();
        assert_eq!(picked.digest().to_string(), SHA_B);
        // If EVERY candidate is over cap, picker returns None.
        let idx_all_big = image_index(vec![descriptor(
            "application/vnd.cyclonedx+json",
            SHA_C,
            200 * 1024 * 1024,
        )]);
        assert!(pick_sbom_descriptor(&idx_all_big, &[], cap).is_none());
    }
}
