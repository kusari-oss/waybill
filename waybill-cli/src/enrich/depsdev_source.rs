use std::collections::HashMap;
use std::sync::Mutex;

use tracing::{debug, info, warn};

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
use super::degradation::{DegradationMode, DegradationRecord};
use super::deps_dev_graph::CONCURRENT_REQUESTS;
use super::progress::ProgressReporter;
use super::request_key::EnrichmentKey;
use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// Milestone 839 (FR-017a): transport failures, counted apart from
    /// genuine 404s. The fetch path collapses both into `None` so
    /// callers can treat them uniformly — which is exactly why a
    /// degraded scan is invisible today. This counter keeps the
    /// distinction the return type discards.
    transport_errors: AtomicUsize,
    /// Milestone 839 (FR-002) — opt in to the bulk path. Off by
    /// default: `GetVersionBatch` is on deps.dev's `v3alpha` surface,
    /// documented as liable to change incompatibly.
    batch: bool,
    /// Milestone 839 (FR-017a) — batch chunks that failed and fell
    /// back to the per-component path. Speed degraded, coverage did
    /// not; recorded so a slower scan is explicable rather than
    /// mysterious.
    batch_fallbacks: AtomicUsize,
    /// Milestone 839 (FR-011) — survives across scans, unlike the
    /// in-memory cache above which dies with the process.
    disk: std::sync::Arc<super::deps_dev_disk_cache::DepsDevDiskCache>,
    /// Milestone 839 — coordinates that actually required a network
    /// lookup, i.e. neither cache could serve them. This is the
    /// quantity SC-005 is about; wall time alone cannot distinguish a
    /// cache that worked from a fast network.
    network_lookups: AtomicUsize,
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
            transport_errors: AtomicUsize::new(0),
            batch: false,
            batch_fallbacks: AtomicUsize::new(0),
            // Default OFF. A constructor that reaches $HOME makes
            // every test that builds a source touch the developer's
            // real cache — shared mutable state across the whole
            // suite (Constitution VII). The caller that wants
            // persistence opts in via `with_disk_cache`, which is
            // exactly one site: the scan command.
            disk: super::deps_dev_disk_cache::DepsDevDiskCache::open(false, None),
            network_lookups: AtomicUsize::new(0),
        }
    }

    /// Milestone 839 (FR-002) — enable the bulk path for this source.
    pub fn with_batch(mut self, batch: bool) -> Self {
        self.batch = batch;
        self
    }

    /// Milestone 839 (FR-011/FR-012b/FR-014) — configure the on-disk
    /// cache. `enabled = false` is `--enrich-no-cache`: every read
    /// misses and every write is dropped, with no call site needing to
    /// know.
    pub fn with_disk_cache(mut self, enabled: bool, override_max_age: Option<u64>) -> Self {
        self.disk = super::deps_dev_disk_cache::DepsDevDiskCache::open(enabled, override_max_age);
        self
    }

    /// Milestone 839 (FR-001) — resolve one chunk through the bulk
    /// endpoint, following pagination.
    ///
    /// Returns `None` on any failure, which the caller turns into a
    /// per-component fallback for that chunk (C-4.1). A chunk is 100
    /// keys, so a failure costs 100 lookups' worth of speed, not the
    /// scan (C-4.0 / FR-005c).
    async fn fetch_chunk_batched(
        &self,
        chunk: &[EnrichmentKey],
    ) -> Option<(Vec<Option<VersionInfo>>, Option<u64>)> {
        // Match by the echoed request, never by position (C-3.1).
        // The echo is uncanonicalized, and deps.dev normalises names
        // per ecosystem, so the index is built from what we SENT.
        let mut index: HashMap<(String, String, String), usize> = HashMap::new();
        for (i, k) in chunk.iter().enumerate() {
            index.insert(
                (k.system.to_uppercase(), k.name.clone(), k.version.clone()),
                i,
            );
        }
        let mut out: Vec<Option<VersionInfo>> = vec![None; chunk.len()];
        let mut token: Option<String> = None;
        let mut max_age: Option<u64> = None;
        loop {
            let (page, page_max_age) = match self.client.get_version_batch(chunk, token.clone()).await {
                Ok(p) => p,
                Err(e) => {
                    warn!(error = %e, "deps.dev batch request failed — falling back");
                    return None;
                }
            };
            // Pages of one logical request share a policy; keep the
            // first seen so a later page cannot silently extend the
            // bound this chunk is stored under.
            max_age = max_age.or(page_max_age);
            for entry in &page.responses {
                let vk = &entry.request.version_key;
                let probe = (
                    vk.system.to_uppercase(),
                    vk.name.clone(),
                    vk.version.clone(),
                );
                match index.get(&probe) {
                    Some(&i) => out[i] = entry.version.clone(),
                    // An entry we did not ask for. Dropping it is
                    // right; treating it as positional would corrupt
                    // a neighbour.
                    None => debug!(
                        system = %vk.system, name = %vk.name, version = %vk.version,
                        "deps.dev batch returned an entry that was not requested",
                    ),
                }
            }
            // C-2.1/C-2.2: continue while the token is NON-EMPTY. The
            // service sends "" rather than omitting it, so a presence
            // check would never terminate.
            if !page.has_more() {
                break;
            }
            token = Some(page.next_page_token.clone());
        }
        Some((out, max_age))
    }

    /// Milestone 839 (FR-003a) — resolve many keys, concurrently.
    ///
    /// Cache is consulted on this task before anything is spawned, so
    /// the workers only ever fetch genuine misses and never contend on
    /// the cache mutex. Results come back in the order the keys were
    /// given, because the caller pairs them positionally with the
    /// components they apply to.
    ///
    /// Concurrency is bounded at `CONCURRENT_REQUESTS`, the same
    /// ceiling the dep-graph path uses. deps.dev publishes no rate
    /// limit and no documented throttling semantics, so the bound is
    /// chosen conservatively rather than tuned up against an
    /// advertised allowance (FR-003b).
    async fn fetch_many(
        &self,
        keys: &[EnrichmentKey],
        progress: &mut ProgressReporter,
    ) -> Vec<Option<VersionInfo>> {
        let mut out: Vec<Option<VersionInfo>> = vec![None; keys.len()];
        let mut misses: Vec<usize> = Vec::new();
        {
            let cache = self.cache.lock().expect("deps.dev cache mutex poisoned");
            for (i, k) in keys.iter().enumerate() {
                let ck = (k.system.to_string(), k.name.clone(), k.version.clone());
                match cache.get(&ck) {
                    Some(hit) => out[i] = hit.clone(),
                    None => misses.push(i),
                }
            }
        }

        // FR-012: disk before network. A hit here is still a hit even
        // under `--offline` (C-5.2) — reading a local file is not a
        // request.
        if self.disk.is_enabled() && !misses.is_empty() {
            let mut still: Vec<usize> = Vec::with_capacity(misses.len());
            let mut cache = self.cache.lock().expect("deps.dev cache mutex poisoned");
            for i in misses {
                match self.disk.get(&keys[i]) {
                    Some(record) => {
                        cache.insert(
                            (
                                keys[i].system.to_string(),
                                keys[i].name.clone(),
                                keys[i].version.clone(),
                            ),
                            record.clone(),
                        );
                        out[i] = record;
                    }
                    None => still.push(i),
                }
            }
            misses = still;
        }

        self.network_lookups
            .fetch_add(misses.len(), Ordering::Relaxed);

        // FR-015 / C-5.1: under `--offline` nothing is issued — not
        // batch, not per-component, not a revalidation. Whatever the
        // caches could serve has already been served above; the rest
        // stays unenriched rather than triggering a fetch (C-5.2).
        if self.offline {
            return out;
        }

        // FR-001: bulk path first when enabled. Chunks run
        // concurrently under the same ceiling; a chunk that fails
        // falls through to the per-component path below (C-4.2),
        // which is concurrent, so a degraded run is slower rather
        // than a return to the original defect.
        let mut done = keys.len() - misses.len();
        if self.batch && !misses.is_empty() {
            let mut still_missing: Vec<usize> = Vec::new();
            let chunks: Vec<Vec<usize>> = misses
                .chunks(super::deps_dev_batch::BATCH_SIZE)
                .map(|c| c.to_vec())
                .collect();
            let mut batched_ok = 0usize;
            for group in chunks.chunks(CONCURRENT_REQUESTS) {
                let mut results = Vec::new();
                for idxs in group {
                    let ks: Vec<EnrichmentKey> =
                        idxs.iter().map(|&i| keys[i].clone()).collect();
                    results.push(async move { (idxs.clone(), self.fetch_chunk_batched(&ks).await) });
                }
                for fut in results {
                    let (idxs, got) = fut.await;
                    match got {
                        Some((vals, max_age)) => {
                            let bound = self.disk.effective_max_age(max_age);
                            let mut cache =
                                self.cache.lock().expect("deps.dev cache mutex poisoned");
                            for (&i, v) in idxs.iter().zip(vals) {
                                cache.insert(
                                    (
                                        keys[i].system.to_string(),
                                        keys[i].name.clone(),
                                        keys[i].version.clone(),
                                    ),
                                    v.clone(),
                                );
                                self.disk.put(&keys[i], &v, bound);
                                out[i] = v;
                            }
                            batched_ok += idxs.len();
                            done += idxs.len();
                        }
                        None => {
                            self.batch_fallbacks.fetch_add(1, Ordering::Relaxed);
                            still_missing.extend(idxs);
                        }
                    }
                }
                progress.tick(done, Instant::now());
            }
            info!(
                batched = batched_ok,
                fell_back = still_missing.len(),
                "deps.dev bulk enrichment complete",
            );
            misses = still_missing;
            if misses.is_empty() {
                return out;
            }
        }

        let client = std::sync::Arc::new(self.client.clone());

        // A sliding window, not `chunks(N)`.
        //
        // Chunking imposes a barrier: each group of N waits for its
        // slowest member before the next group starts, so the cost is
        // max-of-N per group rather than mean-of-N. Measured, that
        // gave 3.2x on an 8-way ceiling where the request layer alone
        // sustains far more. Keeping N continuously in flight — spawn
        // a replacement the moment one finishes — removes the
        // head-of-line blocking a chunk barrier reintroduces.
        let mut pending = misses.iter().copied();
        let mut set = tokio::task::JoinSet::new();
        let spawn_one = |set: &mut tokio::task::JoinSet<_>, i: usize| {
            let c = client.clone();
            let k = keys[i].clone();
            set.spawn(async move {
                let r = c.get_version(k.system, &k.name, &k.version).await;
                (i, k, r)
            });
        };
        for _ in 0..CONCURRENT_REQUESTS {
            match pending.next() {
                Some(i) => spawn_one(&mut set, i),
                None => break,
            }
        }
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok((i, k, result)) => {
                    let (info, max_age) = match result {
                        Ok((info, max_age)) => (Some(info), max_age),
                        Err(e) => {
                            self.transport_errors.fetch_add(1, Ordering::Relaxed);
                            debug!(
                                system = %k.system,
                                name = %k.name,
                                version = %k.version,
                                error = %e,
                                "deps.dev get_version failed — caching as miss"
                            );
                            (None, None)
                        }
                    };
                    self.disk
                        .put(&k, &info, self.disk.effective_max_age(max_age));
                    self.cache
                        .lock()
                        .expect("deps.dev cache mutex poisoned")
                        .insert(
                            (k.system.to_string(), k.name.clone(), k.version.clone()),
                            info.clone(),
                        );
                    out[i] = info;
                }
                // A panicked worker costs one lookup, not the scan.
                Err(e) => warn!(error = %e, "deps.dev licence worker task panicked"),
            }
            done += 1;
            progress.tick(done, Instant::now());
            if let Some(i) = pending.next() {
                spawn_one(&mut set, i);
            }
        }
        out
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
    // Milestone 839 (C-5.2): offline no longer short-circuits the
    // whole phase. Reading a local cache file is not a network
    // request, and the `--offline` flag's own documentation names
    // air-gapped scanners as the use case — which is precisely where a
    // pre-warmed cache is worth having. `fetch_many` still issues
    // nothing; it serves what is on disk and leaves the rest
    // unenriched.
    if source.offline && !source.disk.is_enabled() {
        debug!("deps.dev enrichment skipped — offline with no cache to serve from");
        return (0, LinkMappingSkips::default());
    }
    let phase_start = Instant::now();
    let mut enriched_count = 0usize;
    // Milestone 776 (FR-014b): counted separately on purpose. A rising
    // unmapped count means the upstream label vocabulary moved and a
    // label should be mapped; a rising malformed count means upstream
    // data quality degraded and it should not. Conflating them would
    // obscure both.
    let mut unmapped_label_skips = 0usize;
    let mut malformed_url_skips = 0usize;

    // Milestone 839: resolve every key before fetching anything.
    //
    // Two reasons, and the second is why it is worth a separate pass.
    // Progress must report completed against a total (US2), and a
    // total discovered as the loop runs is not a total — it would show
    // a denominator that grows, which reads as the work getting longer
    // the longer you wait. And the batch path needs exactly this
    // shape: the set of lookups, known up front, independent of the
    // components they will be applied to.
    //
    // The key is built by the one helper the batch path and the cache
    // also use. Three call sites deriving identity independently will
    // eventually derive it differently, and the symptom is a cache
    // that silently splits — one path writing entries another never
    // finds. `from_purl_parts` also absorbs the ecosystem-unsupported
    // and incomplete-coordinate skips that used to sit inline.
    let planned: Vec<(usize, EnrichmentKey)> = components
        .iter()
        .enumerate()
        .filter_map(|(idx, c)| {
            EnrichmentKey::from_purl_parts(
                c.purl.ecosystem(),
                c.purl.namespace(),
                &c.name,
                &c.version,
            )
            .map(|k| (idx, k))
        })
        .collect();
    let attempted = planned.len();
    let mut progress = ProgressReporter::new(attempted);

    // Fetch concurrently, then apply serially. The split matters:
    // applying needs `&mut` on each component, which cannot be held
    // across an await, and mixing the two would serialise the fetches
    // back into the shape this is replacing.
    let keys: Vec<EnrichmentKey> = planned.iter().map(|(_, k)| k.clone()).collect();
    let fetched = source.fetch_many(&keys, &mut progress).await;

    for ((idx, key), info) in planned.into_iter().zip(fetched) {
        let component = &mut components[idx];
        let licenses_before = component.licenses.len();
        if let Some(info) = info {
            let (unmapped, malformed) =
                DepsDevSource::apply_version_info(component, key.system, &info);
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
    // Milestone 839: time the phase explicitly rather than leaving it
    // to be inferred by differencing whole-scan runs. A scan's other
    // network work is not stable enough to subtract — on this repo the
    // non-deps.dev floor moved between 5s and 23s across runs — so a
    // subtraction-based measurement of this phase is unreliable by
    // construction. It is also what an operator wants to know when a
    // scan feels slow.
    let network_lookups = source.network_lookups.load(Ordering::Relaxed);
    info!(
        elapsed_ms = phase_start.elapsed().as_millis() as u64,
        attempted,
        network_lookups,
        cache_hits = attempted.saturating_sub(network_lookups),
        enriched = enriched_count,
        "deps.dev licence enrichment complete",
    );

    // Milestone 839 (FR-017a): Principles XI and XII.3 require a
    // transparency signal when an enrichment source degrades. Only the
    // total-failure case is classifiable here — a *partial* transport
    // failure has no mode in the spec's closed vocabulary, so it is
    // deliberately not invented. Surfaced in the log for now; the
    // document-scope SBOM annotation lands with T008, which needs the
    // record threaded through the emission pipeline.
    let transport_errors = source.transport_errors.load(Ordering::Relaxed);
    let mut degradation = DegradationRecord::new();
    if source.batch_fallbacks.load(Ordering::Relaxed) > 0 {
        degradation.record(DegradationMode::BatchUnavailable);
    }
    if attempted > 0 && transport_errors >= attempted {
        degradation.record(DegradationMode::WhollyUnavailable);
        degradation.add_unenriched(attempted);
    }
    if let Some(value) = degradation.annotation_value() {
        warn!(
            degradation = %value,
            "deps.dev enrichment degraded — SBOM emitted with reduced enrichment"
        );
    } else if transport_errors > 0 {
        debug!(
            transport_errors,
            attempted,
            "deps.dev enrichment saw transport errors but completed",
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
        VersionInfo { licenses: vec![], links }
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod concurrency_tests {
    use super::*;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Milestone 839 (FR-003a / T016).
    ///
    /// Concurrency introduces exactly one new way to be wrong: results
    /// arriving out of request order and being applied to the wrong
    /// component. A serial loop cannot do that, so no existing test
    /// covers it. This makes later requests finish FIRST, then asserts
    /// every licence landed on its own package.
    #[tokio::test]
    async fn out_of_order_completion_still_pairs_results_correctly() {
        let server = MockServer::start().await;

        // 12 packages — more than the concurrency ceiling of 8, so the
        // work spans two chunks and the second chunk's results
        // interleave with nothing left of the first.
        let names: Vec<String> = (0..12).map(|i| format!("pkg{i:02}")).collect();

        for (i, n) in names.iter().enumerate() {
            // Descending delay: pkg00 waits longest, pkg07 returns
            // almost immediately. Completion order is the reverse of
            // request order within each chunk.
            let delay = std::time::Duration::from_millis(((12 - i) * 20) as u64);
            Mock::given(method("GET"))
                .and(path_regex(format!(r".*/packages/{n}/versions/.*")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_delay(delay)
                        .set_body_json(serde_json::json!({
                            // The licence names the package, so a
                            // mispairing is visible rather than subtle.
                            "licenses": [format!("LicenseRef-{n}")],
                            "links": [],
                        })),
                )
                .mount(&server)
                .await;
        }

        let client = DepsDevClient::new(std::time::Duration::from_secs(10))
            .with_base_url(format!("{}/v3", server.uri()));
        let source = DepsDevSource::new(client, false);

        let keys: Vec<EnrichmentKey> = names
            .iter()
            .map(|n| EnrichmentKey::from_purl_parts("cargo", None, n, "1.0.0").unwrap())
            .collect();
        let mut progress = ProgressReporter::new(keys.len());
        let got = source.fetch_many(&keys, &mut progress).await;

        assert_eq!(got.len(), keys.len());
        for (k, info) in keys.iter().zip(got) {
            let info = info.unwrap_or_else(|| panic!("{} should have resolved", k.name));
            assert_eq!(
                info.licenses,
                vec![format!("LicenseRef-{}", k.name)],
                "result for {} was paired with the wrong request",
                k.name,
            );
        }
    }

    /// A cached entry and a fetched one must be indistinguishable in
    /// the output, and the cache must not shift the positions of the
    /// entries around it.
    #[tokio::test]
    async fn cache_hits_and_network_misses_interleave_without_shifting() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(r".*/packages/fetched/versions/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "licenses": ["MIT"], "links": [],
            })))
            .mount(&server)
            .await;
        // Anything not explicitly mocked 404s, standing in for a
        // package deps.dev genuinely does not carry.
        let client = DepsDevClient::new(std::time::Duration::from_secs(10))
            .with_base_url(format!("{}/v3", server.uri()));
        let source = DepsDevSource::new(client, false);

        let mk = |n: &str| EnrichmentKey::from_purl_parts("cargo", None, n, "1.0.0").unwrap();
        let keys = vec![mk("cached"), mk("fetched"), mk("absent")];

        source.cache.lock().unwrap().insert(
            ("cargo".to_string(), "cached".to_string(), "1.0.0".to_string()),
            Some(VersionInfo { licenses: vec!["Apache-2.0".into()], links: vec![] }),
        );

        let mut progress = ProgressReporter::new(keys.len());
        let got = source.fetch_many(&keys, &mut progress).await;

        assert_eq!(got[0].as_ref().unwrap().licenses, vec!["Apache-2.0"], "cache hit");
        assert_eq!(got[1].as_ref().unwrap().licenses, vec!["MIT"], "network hit");
        assert!(got[2].is_none(), "absent package stays absent, and does not fail the scan");
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod batch_tests {
    use super::*;
    use wiremock::matchers::{body_string_contains, method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn src(server: &MockServer, batch: bool) -> DepsDevSource {
        let client = DepsDevClient::new(std::time::Duration::from_secs(10))
            .with_base_url(format!("{}/v3", server.uri()));
        DepsDevSource::new(client, false).with_batch(batch)
    }
    fn keys(names: &[&str]) -> Vec<EnrichmentKey> {
        names
            .iter()
            .map(|n| EnrichmentKey::from_purl_parts("cargo", None, n, "1.0.0").unwrap())
            .collect()
    }
    fn entry(name: &str, licence: Option<&str>) -> serde_json::Value {
        let mut e = serde_json::json!({
            "request": {"versionKey": {"system": "CARGO", "name": name, "version": "1.0.0"}}
        });
        if let Some(l) = licence {
            e["version"] = serde_json::json!({"licenses": [l], "links": []});
        }
        e
    }

    /// T022 / C-2.4. At the shipped batch size this path never runs in
    /// production, so it must be forced by a fixture — an unexercised
    /// defensive path rots, and this one exists only because the page
    /// size is undocumented and may move.
    #[tokio::test]
    async fn pagination_is_followed_to_the_end() {
        let server = MockServer::start().await;
        // Second page first: it matches only when the request body
        // carries the continuation token, so it cannot capture the
        // initial request. Returns "" — the shape that traps a
        // presence check.
        Mock::given(method("POST"))
            .and(path("/v3alpha/versionbatch"))
            .and(body_string_contains("PAGE2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "responses": [entry("b", Some("Apache-2.0"))],
                "nextPageToken": "",
            })))
            .mount(&server)
            .await;
        // First page: matches any batch POST, but only once, so the
        // follow-up request falls through to the mock above.
        Mock::given(method("POST"))
            .and(path("/v3alpha/versionbatch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "responses": [entry("a", Some("MIT"))],
                "nextPageToken": "PAGE2",
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let s = src(&server, true);
        let k = keys(&["a", "b"]);
        let mut p = ProgressReporter::new(k.len());
        let got = s.fetch_many(&k, &mut p).await;

        // `b` only exists on page two. If pagination stopped early —
        // which a presence check on nextPageToken would do, since the
        // final token is "" and not absent — this is None.
        assert_eq!(got[0].as_ref().unwrap().licenses, vec!["MIT"]);
        assert_eq!(
            got[1].as_ref().unwrap().licenses,
            vec!["Apache-2.0"],
            "second page was not consumed — enrichment stopped early and would have \
             produced a plausible-looking but under-enriched SBOM",
        );
    }

    /// T024 / C-3.1, C-3.3. Responses reordered relative to the
    /// request, one entry carrying no `version` at all.
    #[tokio::test]
    async fn responses_match_by_identity_not_position() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v3alpha/versionbatch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                // Deliberately reversed, with the middle one absent.
                "responses": [
                    entry("z", Some("BSD-3-Clause")),
                    entry("y", None),
                    entry("x", Some("MIT")),
                ],
                "nextPageToken": "",
            })))
            .mount(&server)
            .await;

        let s = src(&server, true);
        let k = keys(&["x", "y", "z"]);
        let mut p = ProgressReporter::new(k.len());
        let got = s.fetch_many(&k, &mut p).await;

        assert_eq!(got[0].as_ref().unwrap().licenses, vec!["MIT"], "x");
        assert!(got[1].is_none(), "y has no data upstream and must stay unenriched");
        assert_eq!(got[2].as_ref().unwrap().licenses, vec!["BSD-3-Clause"], "z");
    }

    /// T027 / C-4.1, C-4.2, C-4.4. A failing batch must not fail the
    /// scan, and must produce the same content the per-component path
    /// would have.
    #[tokio::test]
    async fn batch_failure_falls_back_and_content_is_unchanged() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v3alpha/versionbatch"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r".*/packages/.*/versions/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "licenses": ["MIT"], "links": [],
            })))
            .mount(&server)
            .await;

        let k = keys(&["a", "b", "c"]);
        let batched = {
            let s = src(&server, true);
            let mut p = ProgressReporter::new(k.len());
            s.fetch_many(&k, &mut p).await
        };
        let direct = {
            let s = src(&server, false);
            let mut p = ProgressReporter::new(k.len());
            s.fetch_many(&k, &mut p).await
        };

        for (i, r) in batched.iter().enumerate() {
            assert_eq!(
                r.as_ref().map(|v| v.licenses.clone()),
                direct[i].as_ref().map(|v| v.licenses.clone()),
                "fallback content must equal the per-component path for key {i}",
            );
        }
        assert!(batched.iter().all(|r| r.is_some()), "the scan completes with full enrichment");
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod offline_tests {
    use super::*;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// T039 / C-5.1 + C-5.2. Under `--offline` a populated cache is
    /// still served — a local file read is not a network request — but
    /// nothing is issued, and an entry the cache lacks simply stays
    /// unenriched instead of triggering a fetch.
    #[tokio::test]
    async fn offline_serves_cache_and_issues_nothing() {
        let server = MockServer::start().await;
        // Any request at all is a failure of C-5.1. Mounting a mock
        // that would succeed makes the assertion meaningful: if a
        // request were issued, `absent` would come back enriched.
        Mock::given(method("GET"))
            .and(path_regex(r".*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "licenses": ["SHOULD-NOT-APPEAR"], "links": [],
            })))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("WAYBILL_DEPS_DEV_CACHE_DIR", dir.path());
        let client = DepsDevClient::new(std::time::Duration::from_secs(5))
            .with_base_url(format!("{}/v3", server.uri()));
        let source = DepsDevSource::new(client, true).with_disk_cache(true, None);

        let cached = EnrichmentKey::from_purl_parts("cargo", None, "cached", "1.0.0").unwrap();
        let absent = EnrichmentKey::from_purl_parts("cargo", None, "absent", "1.0.0").unwrap();
        source.disk.put(
            &cached,
            &Some(VersionInfo { licenses: vec!["MIT".into()], links: vec![] }),
            3600,
        );

        let keys = vec![cached, absent];
        let mut p = ProgressReporter::new(keys.len());
        let got = source.fetch_many(&keys, &mut p).await;
        std::env::remove_var("WAYBILL_DEPS_DEV_CACHE_DIR");

        assert_eq!(
            got[0].as_ref().unwrap().licenses,
            vec!["MIT"],
            "a cached entry is served under --offline",
        );
        assert!(
            got[1].is_none(),
            "an uncached entry stays unenriched — a cache miss under --offline must not \
             become a fetch",
        );
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            0,
            "C-5.1: no request of any kind may be issued under --offline",
        );
    }
}
