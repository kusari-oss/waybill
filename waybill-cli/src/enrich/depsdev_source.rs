use std::collections::HashMap;
use std::sync::Mutex;

use tracing::{debug, info};

use waybill_common::resolution::{DepsDevMatch, Relationship, ResolvedComponent};
use waybill_common::types::license::SpdxExpression;

use super::deps_dev_client::{DepsDevClient, VersionInfo};
use waybill_common::resolution::{normalize_external_references, ExternalReference};

/// Milestone 776 (FR-014b) — enrichment links that produced no
/// reference, split by reason.
///
/// Kept separate rather than summed: the two numbers call for
/// opposite responses. `unmapped_label` rising means the upstream
/// vocabulary moved and waybill should consider mapping a new label.
/// `malformed_url` rising means upstream data quality degraded and
/// waybill should not.
#[derive(Debug, Default, Clone, Copy)]
pub struct LinkMappingSkips {
    pub unmapped_label: usize,
    pub malformed_url: usize,
}
use super::deps_dev_system::deps_dev_system_for;
use super::source::EnrichmentSource;

/// An enrichment source backed by the deps.dev v3 API.
///
/// Covers ecosystems deps.dev actually indexes (cargo, npm, pypi, go,
/// maven, nuget). Components with ecosystems outside that set (deb,
/// apk, generic, …) are skipped silently — no API call, no error.
///
/// Behaves as a strict enhancement layer: failures (404s, timeouts,
/// unparseable SPDX strings) are logged at `debug` / `warn` and the
/// component is left exactly as it was. Never fails the enclosing
/// scan.
pub struct DepsDevSource {
    client: DepsDevClient,
    offline: bool,
    /// In-memory cache keyed by (system, name, version). `None` caches
    /// the "API returned 404 / error" result so we don't re-hit the
    /// same miss for every duplicate component in a single scan.
    cache: Mutex<HashMap<(String, String, String), Option<VersionInfo>>>,
}

impl DepsDevSource {
    /// Create a new deps.dev enrichment source. When `offline` is true
    /// the source skips every API call (serves as a cheap no-op) —
    /// useful when the global `--offline` flag is set or tests want to
    /// exercise the enrichment path without hitting the network.
    pub fn new(client: DepsDevClient, offline: bool) -> Self {
        Self {
            client,
            offline,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Look up `(system, name, version)` in deps.dev, returning the
    /// cached result when available. Errors are converted to `None` so
    /// the caller can treat "not found" and "API transiently broken"
    /// uniformly.
    async fn fetch_version_info(
        &self,
        system: &str,
        name: &str,
        version: &str,
    ) -> Option<VersionInfo> {
        let key = (system.to_string(), name.to_string(), version.to_string());
        if let Some(cached) = self.cache.lock().expect("deps.dev cache mutex poisoned").get(&key) {
            return cached.clone();
        }
        let result = match self.client.get_version(system, name, version).await {
            Ok(info) => Some(info),
            Err(e) => {
                debug!(
                    system = %system,
                    name = %name,
                    version = %version,
                    error = %e,
                    "deps.dev get_version failed — caching as miss"
                );
                None
            }
        };
        self.cache.lock().expect("deps.dev cache mutex poisoned").insert(key, result.clone());
        result
    }

    /// Milestone 776 (FR-004) — accept a URL only if it is a
    /// well-formed absolute URL.
    ///
    /// Rejects empty strings, relative paths, and scheme-less forms.
    /// Enrichment metadata is best-effort upstream data; emitting an
    /// unusable reference is worse than emitting none (Principle IX).
    fn is_valid_absolute_url(url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        match url::Url::parse(url) {
            Ok(parsed) => parsed.has_host() && !parsed.scheme().is_empty(),
            Err(_) => false,
        }
    }

    /// Milestone 776 (FR-001, FR-002, FR-002a) — map a deps.dev link
    /// label onto a CycloneDX-native `externalReference.type`.
    ///
    /// The kind is chosen from the LABEL ONLY. The URL's shape must
    /// never influence it: a `HOMEPAGE` pointing at a repository host
    /// is still `website`, not `vcs`. Inferring kind from URL shape is
    /// exactly the guess FR-003 exists to prevent, and real packages
    /// exercise this constantly — many set their homepage to their
    /// repository page.
    ///
    /// `ORIGIN` is deliberately absent. It appears on essentially
    /// every component, but its semantics are not determinable from
    /// the label, and assigning a kind would be a guess. It is
    /// treated as any other unmapped label — skipped AND counted —
    /// so the FR-014a summary reflects reality rather than hiding it.
    /// See `specs/776-component-source-refs/` Clarifications Q1.
    fn ref_type_for_link_label(label: &str) -> Option<&'static str> {
        match label {
            "SOURCE_REPO" => Some("vcs"),
            "ISSUE_TRACKER" => Some("issue-tracker"),
            "DOCUMENTATION" => Some("documentation"),
            "HOMEPAGE" => Some("website"),
            "ATTESTATION" => Some("attestation"),
            _ => None,
        }
    }

    /// Apply one deps.dev `VersionInfo` payload to a component. Adds any
    /// SPDX-canonical licenses to `component.licenses` (de-duped against
    /// what's already there) and stamps the `deps_dev_match` evidence
    /// field so downstream consumers see where the enrichment came from.
    /// Milestone 776: also maps `info.links` onto
    /// `component.external_references`. Returns
    /// `(unmapped_label_skips, malformed_url_skips)` so the caller can
    /// aggregate them for the FR-014a summary — the two are counted
    /// SEPARATELY because they call for opposite responses (FR-014b):
    /// a rising unmapped count means the upstream vocabulary moved and
    /// a label should be mapped; a rising malformed count means
    /// upstream data quality degraded and it should not.
    fn apply_version_info(
        component: &mut ResolvedComponent,
        system: &str,
        info: &VersionInfo,
    ) -> (usize, usize) {
        for raw in &info.licenses {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let expr = match SpdxExpression::try_canonical(trimmed) {
                Ok(e) => e,
                Err(e) => {
                    debug!(
                        raw = %trimmed,
                        error = %e,
                        "deps.dev returned a non-canonical SPDX expression"
                    );
                    continue;
                }
            };
            let canonical = expr.as_str().to_string();
            if !component
                .licenses
                .iter()
                .any(|existing| existing.as_str() == canonical)
            {
                component.licenses.push(expr);
            }
        }
        // Milestone 776 (FR-001..FR-004, FR-007): map the `links[]`
        // array that arrives in this SAME already-fetched payload.
        // Pre-m776 it was deserialized and discarded. No additional
        // network request is made here — that is the whole point.
        let mut unmapped_skips = 0usize;
        let mut malformed_skips = 0usize;
        for link in &info.links {
            let Some(ref_type) = Self::ref_type_for_link_label(&link.label) else {
                // Unrecognized label (including ORIGIN). Skip without
                // failing and without per-occurrence output (FR-003);
                // the count surfaces in the scan summary instead.
                unmapped_skips += 1;
                continue;
            };
            if !Self::is_valid_absolute_url(&link.url) {
                malformed_skips += 1;
                continue;
            }
            component.external_references.push(ExternalReference {
                ref_type: ref_type.to_string(),
                url: link.url.clone(),
            });
        }
        // FR-006 + FR-013 over the combined set — derived references
        // only; operator-supplied references live in a separate
        // annotation and are structurally out of reach (research R9).
        normalize_external_references(&mut component.external_references);

        component.evidence.deps_dev_match = Some(DepsDevMatch {
            system: system.to_string(),
            name: component.name.clone(),
            version: component.version.clone(),
        });
        (unmapped_skips, malformed_skips)
    }
}

impl EnrichmentSource for DepsDevSource {
    fn name(&self) -> &str {
        "deps.dev"
    }

    fn enrich_relationships(
        &self,
        components: &[ResolvedComponent],
    ) -> anyhow::Result<Vec<Relationship>> {
        info!(
            component_count = components.len(),
            "deps.dev relationship enrichment (not implemented — licenses + CPE only)"
        );
        // Relationship enrichment via deps.dev's GetDependencies endpoint
        // is tracked as its own follow-up. The current round only
        // populates metadata.
        Ok(vec![])
    }

    fn enrich_metadata(
        &self,
        _component: &mut ResolvedComponent,
    ) -> anyhow::Result<()> {
        // The sync trait contract doesn't match deps.dev's async client.
        // Callers that want the real enrichment use
        // [`enrich_components`] (defined below) instead — it takes the
        // full set in one async pass so we can batch + cache.
        Ok(())
    }
}

/// Enrich a whole vector of components against deps.dev. Offline-aware
/// and lossy-friendly: components in unsupported ecosystems are left
/// untouched, API errors cache as misses without failing the scan.
///
/// Returns the number of components that received at least one new
/// license or CPE candidate from deps.dev. Useful for a post-scan log
/// line.
/// Returns `LinkMappingSkips` alongside the enriched-component count
/// so the caller can report the milestone-776 FR-014a summary. Skips
/// never reach the emitted document, so they cannot be recovered by
/// counting components later — unlike the per-kind reference counts,
/// which are derived from the final component set precisely so they
/// cannot drift from the document (research R9).
pub async fn enrich_components(
    source: &DepsDevSource,
    components: &mut [ResolvedComponent],
) -> (usize, LinkMappingSkips) {
    if source.offline {
        debug!("deps.dev enrichment skipped — offline mode active");
        return (0, LinkMappingSkips::default());
    }
    let mut enriched_count = 0usize;
    // Milestone 776 (FR-014b): counted separately on purpose. A rising
    // unmapped count means the upstream label vocabulary moved and a
    // label should be mapped; a rising malformed count means upstream
    // data quality degraded and it should not. Conflating them would
    // obscure both.
    let mut unmapped_label_skips = 0usize;
    let mut malformed_url_skips = 0usize;
    for component in components.iter_mut() {
        let ecosystem = component.purl.ecosystem();
        let Some(system) = deps_dev_system_for(ecosystem) else {
            continue;
        };
        if component.name.is_empty() || component.version.is_empty() {
            continue;
        }
        // deps.dev keys Maven packages by `group:artifact` (and Go by
        // module path, npm scoped by `@scope/name`). `component.name`
        // is just the artifact / short name, which for Maven/Go/npm
        // produces 404s. Format through the helper so the URL is
        // correct for every supported ecosystem.
        let name =
            super::deps_dev_system::deps_dev_package_name(
                ecosystem,
                component.purl.namespace(),
                &component.name,
            );
        let licenses_before = component.licenses.len();
        if let Some(info) = source
            .fetch_version_info(system, &name, &component.version)
            .await
        {
            let (unmapped, malformed) =
                DepsDevSource::apply_version_info(component, system, &info);
            unmapped_label_skips += unmapped;
            malformed_url_skips += malformed;
            if component.licenses.len() > licenses_before {
                enriched_count += 1;
            }
        }
    }
    if enriched_count > 0 {
        info!(
            count = enriched_count,
            "deps.dev enriched components with new licenses"
        );
    }
    let skips = LinkMappingSkips {
        unmapped_label: unmapped_label_skips,
        malformed_url: malformed_url_skips,
    };
    (enriched_count, skips)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
    use waybill_common::types::purl::Purl;
    use std::time::Duration;

    pub(super) fn make_component(purl_str: &str) -> ResolvedComponent {
        let purl = Purl::new(purl_str).expect("valid purl");
        ResolvedComponent {
            build_inclusion: None,
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.9,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: Vec::new(),
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    #[tokio::test]
    async fn offline_mode_skips_api_and_leaves_components_untouched() {
        // Pointing at a deliberately-unreachable URL would prove the
        // skip, but using offline=true is safer: the client is never
        // invoked at all so there's no network dependency in the test.
        let client = DepsDevClient::new(Duration::from_secs(1));
        let source = DepsDevSource::new(client, /*offline=*/ true);
        let mut components = vec![make_component("pkg:cargo/serde@1.0.197")];
        let (n, skips) = enrich_components(&source, &mut components).await;
        assert_eq!(n, 0);
        // m776: no links were consulted, so both skip counters stay 0.
        assert_eq!((skips.unmapped_label, skips.malformed_url), (0, 0));
        assert!(components[0].licenses.is_empty());
        assert!(components[0].evidence.deps_dev_match.is_none());
    }

    #[tokio::test]
    async fn unsupported_ecosystems_are_skipped_without_cache_entry() {
        let client = DepsDevClient::new(Duration::from_secs(1));
        let source = DepsDevSource::new(client, /*offline=*/ false);
        let mut components = vec![
            make_component("pkg:deb/debian/jq@1.6-2.1"),
            make_component("pkg:apk/alpine/musl@1.2.4-r2"),
        ];
        let (n, skips) = enrich_components(&source, &mut components).await;
        assert_eq!(n, 0);
        // m776: unsupported ecosystems are skipped before any link
        // handling, so neither counter moves.
        assert_eq!((skips.unmapped_label, skips.malformed_url), (0, 0));
        // Cache must stay empty — we never looked these up.
        assert!(source.cache.lock().unwrap().is_empty());
    }

    #[test]
    fn apply_version_info_deduplicates_licenses() {
        // Pre-seed with MIT; deps.dev returns MIT + Apache-2.0 — only
        // Apache-2.0 should be appended.
        let mut c = make_component("pkg:cargo/foo@1.0.0");
        c.licenses.push(SpdxExpression::try_canonical("MIT").unwrap());
        let info = VersionInfo {
            licenses: vec!["MIT".into(), "Apache-2.0".into()],
            advisory_keys: vec![],
            links: vec![],
        };
        DepsDevSource::apply_version_info(&mut c, "cargo", &info);
        assert_eq!(c.licenses.len(), 2);
        assert!(c.licenses.iter().any(|l| l.as_str() == "MIT"));
        assert!(c.licenses.iter().any(|l| l.as_str() == "Apache-2.0"));
        assert!(c.evidence.deps_dev_match.is_some());
    }

    #[test]
    fn apply_version_info_rejects_unparseable_license_strings() {
        let mut c = make_component("pkg:cargo/foo@1.0.0");
        let info = VersionInfo {
            licenses: vec!["Not a real SPDX token $%^".into()],
            advisory_keys: vec![],
            links: vec![],
        };
        DepsDevSource::apply_version_info(&mut c, "cargo", &info);
        assert!(c.licenses.is_empty());
        // We still stamp the deps_dev_match (we did look it up,
        // successfully — the payload just happened to be garbage).
        assert!(c.evidence.deps_dev_match.is_some());
    }
}

/// Milestone 776 — enrichment link → externalReference mapping tests.
///
/// These exercise the mapping, validation, and normalization directly
/// on in-memory payloads: no network, no toolchain, no privilege
/// (Constitution Principle VII).
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod m776_link_mapping_tests {
    use super::*;
    use waybill_common::resolution::M776_DERIVED_REF_TYPES;

    fn link(label: &str, url: &str) -> super::super::deps_dev_client::Link {
        super::super::deps_dev_client::Link {
            label: label.to_string(),
            url: url.to_string(),
        }
    }

    fn info(links: Vec<super::super::deps_dev_client::Link>) -> VersionInfo {
        VersionInfo { licenses: vec![], advisory_keys: vec![], links }
    }

    fn component() -> ResolvedComponent {
        let mut c = super::tests::make_component("pkg:pypi/flask@3.0.0");
        c.external_references.clear();
        c
    }

    /// Contract 2 / FR-001 + FR-002: each mapped label yields its kind.
    #[test]
    fn m776_each_mapped_label_yields_its_kind() {
        for (label, expected) in [
            ("SOURCE_REPO", "vcs"),
            ("ISSUE_TRACKER", "issue-tracker"),
            ("DOCUMENTATION", "documentation"),
            ("HOMEPAGE", "website"),
            ("ATTESTATION", "attestation"),
        ] {
            let mut c = component();
            let (unmapped, malformed) = DepsDevSource::apply_version_info(
                &mut c, "pypi", &info(vec![link(label, "https://example.com/x")]),
            );
            assert_eq!(unmapped, 0, "{label} must map");
            assert_eq!(malformed, 0);
            assert_eq!(c.external_references.len(), 1, "{label}");
            assert_eq!(c.external_references[0].ref_type, expected, "{label}");
        }
    }

    /// Contract 2 / FR-002a: ORIGIN is unmapped — and COUNTED, not
    /// silently special-cased, so the summary reflects reality.
    #[test]
    fn m776_origin_is_unmapped_and_counted() {
        let mut c = component();
        let (unmapped, malformed) = DepsDevSource::apply_version_info(
            &mut c, "npm", &info(vec![link("ORIGIN", "https://registry.npmjs.org/x")]),
        );
        assert!(c.external_references.is_empty(), "ORIGIN must not produce a reference");
        assert_eq!(unmapped, 1, "ORIGIN must be COUNTED as an unmapped skip");
        assert_eq!(malformed, 0, "ORIGIN is not a malformed-URL skip");
    }

    /// FR-003: an unknown future label is skipped without failing.
    #[test]
    fn m776_unknown_label_skipped_without_failing() {
        let mut c = component();
        let (unmapped, _) = DepsDevSource::apply_version_info(
            &mut c, "pypi", &info(vec![link("SOME_FUTURE_LABEL", "https://example.com/y")]),
        );
        assert!(c.external_references.is_empty());
        assert_eq!(unmapped, 1);
    }

    /// Contract 2's binding constraint: the kind comes from the LABEL
    /// ONLY. A HOMEPAGE pointing at a repository host stays `website`.
    /// Real packages exercise this constantly.
    #[test]
    fn m776_kind_is_label_driven_not_url_driven() {
        let mut c = component();
        DepsDevSource::apply_version_info(
            &mut c, "pypi", &info(vec![link("HOMEPAGE", "https://github.com/pallets/flask")]),
        );
        assert_eq!(c.external_references.len(), 1);
        assert_eq!(
            c.external_references[0].ref_type, "website",
            "a repository-host URL under HOMEPAGE must NOT be inferred as vcs",
        );
    }

    /// Contract 4 / FR-004 + NFR-002: malformed URLs are skipped,
    /// counted as malformed (not unmapped), and the component survives
    /// with its other references intact.
    #[test]
    fn m776_malformed_urls_skipped_and_counted_separately() {
        let mut c = component();
        let (unmapped, malformed) = DepsDevSource::apply_version_info(
            &mut c, "pypi",
            &info(vec![
                link("SOURCE_REPO", ""),
                link("ISSUE_TRACKER", "not-a-url"),
                link("DOCUMENTATION", "/relative/path"),
                link("HOMEPAGE", "https://good.example/ok"),
            ]),
        );
        assert_eq!(malformed, 3, "empty, scheme-less, and relative are all malformed");
        assert_eq!(unmapped, 0, "these are mapped labels — the URL is the problem");
        assert_eq!(c.external_references.len(), 1, "the valid one survives");
        assert_eq!(c.external_references[0].ref_type, "website");
    }

    /// Contract 3 / FR-005 + SC-009: every kind the mapping can emit is
    /// CycloneDX-native. No `waybill:*` property carries source
    /// provenance.
    #[test]
    fn m776_all_emitted_kinds_are_cdx_native() {
        for label in ["SOURCE_REPO", "ISSUE_TRACKER", "DOCUMENTATION", "HOMEPAGE", "ATTESTATION"] {
            let mut c = component();
            DepsDevSource::apply_version_info(
                &mut c, "pypi", &info(vec![link(label, "https://example.com/z")]),
            );
            for r in &c.external_references {
                assert!(
                    M776_DERIVED_REF_TYPES.contains(&r.ref_type.as_str()),
                    "kind `{}` is not in the verified CDX-native set", r.ref_type,
                );
                assert!(!r.ref_type.starts_with("waybill:"));
            }
        }
    }

    /// FR-006 + FR-013 applied at the mapping site: duplicates collapse
    /// and ordering is input-order independent.
    #[test]
    fn m776_result_is_deduped_and_deterministically_ordered() {
        let mut a = component();
        DepsDevSource::apply_version_info(&mut a, "pypi", &info(vec![
            link("SOURCE_REPO", "https://github.com/a/b"),
            link("HOMEPAGE", "https://a.example"),
            link("SOURCE_REPO", "https://github.com/a/b"),
        ]));
        let mut b = component();
        DepsDevSource::apply_version_info(&mut b, "pypi", &info(vec![
            link("SOURCE_REPO", "https://github.com/a/b"),
            link("SOURCE_REPO", "https://github.com/a/b"),
            link("HOMEPAGE", "https://a.example"),
        ]));
        assert_eq!(a.external_references.len(), 2, "the duplicate pair collapses");
        assert_eq!(a.external_references, b.external_references, "order is input-independent");
    }

    /// FR-008: a payload with no links produces no references and does
    /// not fail — the same shape the offline path produces.
    #[test]
    fn m776_empty_links_produces_nothing() {
        let mut c = component();
        let (unmapped, malformed) =
            DepsDevSource::apply_version_info(&mut c, "pypi", &info(vec![]));
        assert!(c.external_references.is_empty());
        assert_eq!((unmapped, malformed), (0, 0));
    }
}
