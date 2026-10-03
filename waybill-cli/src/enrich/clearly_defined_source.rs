//! ClearlyDefined enrichment source.
//!
//! Mirrors `depsdev_source.rs` — async, in-memory cache (per scan),
//! offline-aware, error-tolerant. The post-scan pipeline calls
//! [`enrich_components`] once with the full component list; CD
//! responses populate `ResolvedComponent.concluded_licenses` so the
//! CDX serializer emits them with `acknowledgement: "concluded"`.
//!
//! Definitions are fetched in bulk (`POST /definitions`, 100 per
//! request) and only fall back to one `GET` per coordinate for what a
//! batch could not answer (#933).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tracing::{debug, info, warn};

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::license::SpdxExpression;

use super::clearly_defined_client::{BulkError, CdDefinition, ClearlyDefinedClient};
use super::clearly_defined_coord::{cd_coord_for, CdCoord};
use super::clearly_defined_disk_cache::CdDiskCache;

const DEFAULT_TIMEOUT_SECS: u64 = 5;
/// In-flight cap for the per-coordinate fallback. Mirrors
/// `deps_dev_graph::CONCURRENT_REQUESTS` (8).
const CONCURRENT_REQUESTS: usize = 8;

/// Coordinates per `POST /definitions`. Measured (#933): 1000 is refused
/// with HTTP 400, 250 succeeded once and later hit an origin timeout,
/// 100 is consistently accepted — the same size deps.dev settled on.
const BULK_BATCH_SIZE: usize = 100;
/// Bulk requests in flight. Measured 2026-10-02 on 7 warm batches of 100:
/// 1 → serial, 2 → 1.67 s, 4 → 0.97 s, 7 → 0.69 s. Four takes most of the
/// gain without opening seven connections to a service whose variance is
/// the reason #930 exists.
const BULK_CONCURRENCY: usize = 4;
/// Per bulk request. Measured (specs/923-enrich-batch-default/measurements/
/// clearly-defined-bulk/): every batch that succeeded answered in 0.18–0.71 s,
/// cold or warm, and every failure ran 30 s or more (to the client limit, or
/// a 502 after 31–112 s). A longer timeout buys no successes; it only sets
/// what a stall costs. At 30 s, 4 of 20 scans of a one-batch project
/// stalled, and one stalled on the retry too: 65 s, failing the Windows
/// smoke test's 60 s limit. At 5 s, the same as a single-coordinate GET, a
/// stall costs 5 s and a double stall 10 s. The retry stays: it answered in
/// 3 of those 4 stalls.
const BULK_TIMEOUT_SECS: u64 = 5;

/// Owns the HTTP client + in-memory + disk caches + offline flag.
/// Cheap to clone — the cache and disk cache are `Arc`-shared so
/// concurrent fetch tasks see the same `HashMap` and the same
/// disk-handle.
#[derive(Clone)]
pub struct ClearlyDefinedSource {
    client: ClearlyDefinedClient,
    offline: bool,
    /// `Some(def)` when CD answered with a definition (license may be
    /// None inside it); `None` for confirmed misses (404). Either way
    /// caching prevents the same coord from being re-fetched in a scan.
    cache: Arc<Mutex<HashMap<CdCoord, Option<CdDefinition>>>>,
    /// Cross-scan persistent cache. The disk-cache layer is
    /// no-op-tolerant: disabled / unwritable home dirs are silently
    /// treated as "no disk cache available."
    disk_cache: Arc<CdDiskCache>,
}

impl ClearlyDefinedSource {
    pub fn new(offline: bool) -> Self {
        Self::with_parts(
            ClearlyDefinedClient::new(Duration::from_secs(DEFAULT_TIMEOUT_SECS)),
            offline,
            CdDiskCache::open(),
        )
    }

    fn with_parts(client: ClearlyDefinedClient, offline: bool, disk_cache: Arc<CdDiskCache>) -> Self {
        Self {
            client,
            offline,
            cache: Arc::new(Mutex::new(HashMap::new())),
            disk_cache,
        }
    }

    /// What the caches already know about `coord`: the in-memory cache
    /// first, then the disk cache (written through to memory). `None`
    /// means neither has an answer and the network must be asked.
    fn cached(&self, coord: &CdCoord) -> Option<Option<CdDefinition>> {
        if let Some(hit) = self
            .cache
            .lock()
            .expect("cd cache mutex poisoned")
            .get(coord)
        {
            return Some(hit.clone());
        }
        let disk_hit = self.disk_cache.get(coord)?;
        self.cache
            .lock()
            .expect("cd cache mutex poisoned")
            .insert(coord.clone(), disk_hit.clone());
        Some(disk_hit)
    }

    /// Record an answer for the rest of the scan. Only an answer CD
    /// actually gave is persisted: a timeout or 5xx said nothing about
    /// the package, and writing it to the 7-day disk cache would withhold
    /// the package's licence from every scan in that window.
    fn record(&self, coord: &CdCoord, result: Option<CdDefinition>, answered: bool) {
        if answered {
            self.disk_cache.put(coord, &result);
        }
        self.cache
            .lock()
            .expect("cd cache mutex poisoned")
            .insert(coord.clone(), result);
    }

    async fn fetch_definition(&self, coord: &CdCoord) -> Option<CdDefinition> {
        if let Some(hit) = self.cached(coord) {
            return hit;
        }
        let (result, answered) = match self.client.get_definition(&coord.url_path()).await {
            Ok(def) => (def, true),
            Err(e) => {
                debug!(
                    coord = %coord.url_path(),
                    error = %e,
                    "ClearlyDefined fetch failed — not cached across scans"
                );
                (None, false)
            }
        };
        self.record(coord, result.clone(), answered);
        result
    }

    /// Apply one CD definition to a component. Pushes the curated
    /// SPDX expression onto `concluded_licenses`, deduped against
    /// any existing entry. SPDX-parse failures are logged + skipped.
    fn apply_definition(component: &mut ResolvedComponent, def: &CdDefinition) -> bool {
        let Some(ref raw) = def.declared_license else {
            return false;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return false;
        }
        let expr = match SpdxExpression::try_canonical(trimmed) {
            Ok(e) => e,
            Err(e) => {
                debug!(
                    raw = %trimmed,
                    error = %e,
                    "CD returned a non-canonical SPDX expression — skipping"
                );
                return false;
            }
        };
        let canonical = expr.as_str().to_string();
        if component
            .concluded_licenses
            .iter()
            .any(|existing| existing.as_str() == canonical)
        {
            return false;
        }
        component.concluded_licenses.push(expr);
        true
    }
}

/// Enrich every supported component against ClearlyDefined.
///
/// Skips offline mode entirely. Components in unsupported ecosystems
/// (deb / apk / rpm / generic / etc.) are silently passed through.
/// Returns the number of components that picked up at least one
/// concluded license — useful for an INFO-level log line.
///
/// Coords are deduplicated, cache hits are served without the network,
/// and the rest go to [`bulk_fetch`]. Whatever bulk could not answer is
/// fetched one coordinate at a time. Apply runs sequentially after all
/// fetches complete — keeps the mutation site simple and the ordering
/// deterministic.
pub async fn enrich_components(
    source: &ClearlyDefinedSource,
    components: &mut [ResolvedComponent],
) -> usize {
    if source.offline {
        debug!("ClearlyDefined enrichment skipped — offline mode active");
        return 0;
    }

    // 1. Compute the coord per component (None means "skip this row").
    let coords_by_index: Vec<Option<CdCoord>> = components
        .iter()
        .map(cd_coord_for)
        .collect();

    // 2. Deduplicate coords. Multiple components could share a coord
    //    (e.g., the same maven artifact appearing in two places).
    let mut seen = std::collections::HashSet::new();
    let unique_coords: Vec<CdCoord> = coords_by_index
        .iter()
        .filter_map(|c| c.as_ref())
        .filter(|c| seen.insert((*c).clone()))
        .cloned()
        .collect();

    if unique_coords.is_empty() {
        return 0;
    }

    let source_arc = Arc::new(source.clone());
    let misses: Vec<CdCoord> = unique_coords
        .iter()
        .filter(|c| source_arc.cached(c).is_none())
        .cloned()
        .collect();

    info!(
        unique_coords = unique_coords.len(),
        cache_hits = unique_coords.len() - misses.len(),
        bulk_batches = misses.len().div_ceil(BULK_BATCH_SIZE),
        "ClearlyDefined enrichment starting",
    );

    // 3. Bulk first; per-coordinate for whatever it left.
    let leftover = bulk_fetch(&source_arc, &misses).await;
    if !leftover.is_empty() {
        info!(
            coords = leftover.len(),
            "ClearlyDefined bulk could not answer some coordinates — fetching them individually"
        );
        per_coordinate_fetch(&source_arc, &leftover).await;
    }

    // 4. Apply cached results to each component.
    let mut enriched = 0usize;
    for (component, coord) in components.iter_mut().zip(coords_by_index) {
        let Some(coord) = coord else { continue };
        let def_opt = source_arc
            .cache
            .lock()
            .expect("cd cache mutex poisoned")
            .get(&coord)
            .cloned()
            .unwrap_or(None);
        if let Some(def) = def_opt {
            if ClearlyDefinedSource::apply_definition(component, &def) {
                enriched += 1;
            }
        }
    }

    if enriched > 0 {
        info!(
            count = enriched,
            "ClearlyDefined enriched components with concluded licenses"
        );
    }
    enriched
}

/// Fetch `coords` in batches of [`BULK_BATCH_SIZE`], [`BULK_CONCURRENCY`]
/// in flight, and return the coordinates no batch answered.
///
/// A transiently failed batch is retried once, from the back of the
/// queue: measured against the live service, batches that stalled or
/// returned 502 answered in under a second minutes later. A 4xx means the
/// request shape itself was refused, so it stops bulk for the rest of the
/// scan rather than repeating the refusal per batch (#933).
async fn bulk_fetch(source: &Arc<ClearlyDefinedSource>, coords: &[CdCoord]) -> Vec<CdCoord> {
    let batches: Vec<Vec<CdCoord>> = coords.chunks(BULK_BATCH_SIZE).map(<[_]>::to_vec).collect();
    let mut queue: VecDeque<(usize, bool)> = (0..batches.len()).map(|i| (i, false)).collect();
    let mut settled = vec![false; batches.len()];
    let mut leftover: Vec<CdCoord> = Vec::new();
    let mut rejected = false;
    let timeout = Duration::from_secs(BULK_TIMEOUT_SECS);

    let mut set = tokio::task::JoinSet::new();
    let spawn = |set: &mut tokio::task::JoinSet<_>, i: usize, retried: bool| {
        let s = Arc::clone(source);
        let keys: Vec<String> = batches[i].iter().map(CdCoord::bulk_key).collect();
        set.spawn(async move { (i, retried, s.client.post_definitions(&keys, timeout).await) });
    };
    while set.len() < BULK_CONCURRENCY {
        let Some((i, retried)) = queue.pop_front() else { break };
        spawn(&mut set, i, retried);
    }

    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((i, _, Ok(answers))) => {
                settled[i] = true;
                for coord in &batches[i] {
                    match answers.get(&coord.bulk_key()) {
                        Some(def) => source.record(coord, Some(def.clone()), true),
                        None => leftover.push(coord.clone()),
                    }
                }
            }
            Ok((i, _, Err(BulkError::Rejected(status)))) => {
                settled[i] = true;
                if !rejected {
                    warn!(
                        status,
                        "ClearlyDefined bulk endpoint refused a request — using per-coordinate lookups for the rest of the scan"
                    );
                    rejected = true;
                }
                leftover.extend(batches[i].iter().cloned());
            }
            Ok((i, retried, Err(BulkError::Transient(e)))) => {
                if retried || rejected {
                    debug!(batch = i, error = %e, "ClearlyDefined bulk batch failed twice");
                    settled[i] = true;
                    leftover.extend(batches[i].iter().cloned());
                } else {
                    debug!(batch = i, error = %e, "ClearlyDefined bulk batch failed — retrying later");
                    queue.push_back((i, true));
                }
            }
            // A panicked worker leaves its batch unsettled; it is picked
            // up below with everything else nothing answered.
            Err(e) => warn!(error = %e, "ClearlyDefined bulk worker task panicked"),
        }
        if rejected {
            for (i, _) in queue.drain(..) {
                settled[i] = true;
                leftover.extend(batches[i].iter().cloned());
            }
        }
        while set.len() < BULK_CONCURRENCY {
            let Some((i, retried)) = queue.pop_front() else { break };
            spawn(&mut set, i, retried);
        }
    }

    for (i, done) in settled.iter().enumerate() {
        if !done {
            leftover.extend(batches[i].iter().cloned());
        }
    }
    leftover
}

/// One `GET` per coordinate, [`CONCURRENT_REQUESTS`] in flight.
///
/// A sliding window, not `chunks(N)`: a chunk barrier makes each group
/// wait for its slowest member, which `depsdev_source.rs` measured at 3.2x
/// on the same 8-way ceiling (#933).
async fn per_coordinate_fetch(source: &Arc<ClearlyDefinedSource>, coords: &[CdCoord]) {
    let mut pending = coords.iter();
    let mut set = tokio::task::JoinSet::new();
    let spawn = |set: &mut tokio::task::JoinSet<()>, coord: &CdCoord| {
        let s = Arc::clone(source);
        let coord = coord.clone();
        set.spawn(async move {
            s.fetch_definition(&coord).await;
        });
    };
    for coord in pending.by_ref().take(CONCURRENT_REQUESTS) {
        spawn(&mut set, coord);
    }
    while let Some(joined) = set.join_next().await {
        if let Err(e) = joined {
            warn!(error = %e, "ClearlyDefined worker task panicked");
        }
        if let Some(coord) = pending.next() {
            spawn(&mut set, coord);
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{
        ResolutionEvidence, ResolutionTechnique,
    };
    use waybill_common::types::purl::Purl;

    fn make_component(purl: &str) -> ResolvedComponent {
        let p = Purl::new(purl).expect("valid purl");
        ResolvedComponent {
            build_inclusion: None,
            name: p.name().to_string(),
            version: p.version().unwrap_or("").to_string(),
            purl: p,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
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

    /// #933 — the bulk path against a mock CD.
    mod bulk {
        use super::*;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn source(server: &MockServer, disk: Arc<CdDiskCache>) -> ClearlyDefinedSource {
            ClearlyDefinedSource::with_parts(
                ClearlyDefinedClient::with_base_url(Duration::from_secs(5), &server.uri()),
                false,
                disk,
            )
        }

        fn body(declared: Option<&str>) -> serde_json::Value {
            match declared {
                Some(d) => serde_json::json!({ "licensed": { "declared": d } }),
                None => serde_json::json!({ "described": {}, "licensed": {} }),
            }
        }

        /// One component per (npm name, declared licence).
        const FIXTURE: &[(&str, Option<&str>)] = &[
            ("a", Some("MIT")),
            ("b", Some("MIT OR Apache-2.0")),
            ("c", None),
        ];

        fn components() -> Vec<ResolvedComponent> {
            FIXTURE
                .iter()
                .map(|(n, _)| make_component(&format!("pkg:npm/{n}@1.0.0")))
                .collect()
        }

        fn coord_path(name: &str) -> String {
            format!("npm/npmjs/-/{name}/1.0.0")
        }

        fn bulk_body() -> serde_json::Value {
            let map: serde_json::Map<String, serde_json::Value> = FIXTURE
                .iter()
                .map(|(n, d)| (coord_path(n), body(*d)))
                .collect();
            serde_json::Value::Object(map)
        }

        async fn mount_gets(server: &MockServer) {
            for (n, d) in FIXTURE {
                Mock::given(method("GET"))
                    .and(path(format!("/definitions/{}", coord_path(n))))
                    .respond_with(ResponseTemplate::new(200).set_body_json(body(*d)))
                    .mount(server)
                    .await;
            }
        }

        fn licences(comps: &[ResolvedComponent]) -> Vec<Vec<String>> {
            comps
                .iter()
                .map(|c| c.concluded_licenses.iter().map(|l| l.as_str().to_string()).collect())
                .collect()
        }

        fn requests(server_requests: &[wiremock::Request], verb: &str) -> usize {
            server_requests.iter().filter(|r| r.method.as_str() == verb).count()
        }

        #[tokio::test]
        async fn both_paths_produce_the_same_content() {
            let bulk_server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/definitions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(bulk_body()))
                .mount(&bulk_server)
                .await;
            let mut via_bulk = components();
            enrich_components(&source(&bulk_server, CdDiskCache::disabled()), &mut via_bulk).await;

            let single_server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/definitions"))
                .respond_with(ResponseTemplate::new(503))
                .mount(&single_server)
                .await;
            mount_gets(&single_server).await;
            let mut via_single = components();
            enrich_components(&source(&single_server, CdDiskCache::disabled()), &mut via_single).await;

            assert_eq!(
                licences(&via_bulk),
                vec![vec!["MIT".to_string()], vec!["MIT OR Apache-2.0".to_string()], vec![]],
            );
            assert_eq!(licences(&via_bulk), licences(&via_single));
            let bulk_reqs = bulk_server.received_requests().await.unwrap();
            assert_eq!(requests(&bulk_reqs, "GET"), 0, "bulk answered everything");
            let single_reqs = single_server.received_requests().await.unwrap();
            assert_eq!(requests(&single_reqs, "POST"), 2, "one attempt and one retry");
            assert_eq!(requests(&single_reqs, "GET"), FIXTURE.len());
        }

        #[tokio::test]
        async fn a_transient_batch_failure_is_retried_before_falling_back() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/definitions"))
                .respond_with(ResponseTemplate::new(502))
                .up_to_n_times(1)
                .with_priority(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/definitions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(bulk_body()))
                .mount(&server)
                .await;
            let mut comps = components();
            enrich_components(&source(&server, CdDiskCache::disabled()), &mut comps).await;

            assert_eq!(licences(&comps)[0], vec!["MIT".to_string()]);
            let reqs = server.received_requests().await.unwrap();
            assert_eq!(requests(&reqs, "POST"), 2);
            assert_eq!(requests(&reqs, "GET"), 0, "the retry answered; nothing fell back");
        }

        #[tokio::test]
        async fn a_refused_request_stops_bulk_for_the_scan() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/definitions"))
                .respond_with(ResponseTemplate::new(400))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body(Some("MIT"))))
                .mount(&server)
                .await;
            let batches = 9;
            let mut comps: Vec<ResolvedComponent> = (0..BULK_BATCH_SIZE * batches)
                .map(|i| make_component(&format!("pkg:npm/p{i}@1.0.0")))
                .collect();
            let enriched =
                enrich_components(&source(&server, CdDiskCache::disabled()), &mut comps).await;

            let reqs = server.received_requests().await.unwrap();
            assert_eq!(
                requests(&reqs, "POST"),
                BULK_CONCURRENCY,
                "only the batches already in flight when the refusal arrived may be sent"
            );
            assert_eq!(enriched, comps.len(), "every coordinate still resolved, one at a time");
        }

        #[tokio::test]
        async fn a_coordinate_missing_from_the_response_is_fetched_individually() {
            let server = MockServer::start().await;
            let mut partial = bulk_body();
            partial.as_object_mut().unwrap().remove(&coord_path("b"));
            Mock::given(method("POST"))
                .and(path("/definitions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(partial))
                .mount(&server)
                .await;
            mount_gets(&server).await;
            let mut comps = components();
            enrich_components(&source(&server, CdDiskCache::disabled()), &mut comps).await;

            assert_eq!(licences(&comps)[1], vec!["MIT OR Apache-2.0".to_string()]);
            let reqs = server.received_requests().await.unwrap();
            assert_eq!(requests(&reqs, "GET"), 1);
        }

        #[tokio::test]
        async fn bulk_matches_a_go_incompatible_version() {
            // Measured: bulk keys are literal, so this coordinate only hits
            // with a raw `+`. Sent percent-encoded, CD answers with nothing.
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/definitions"))
                .and(wiremock::matchers::body_string_contains("v2.0.1+incompatible"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "go/golang/github.com%2Fyudai/pp/v2.0.1+incompatible":
                        { "licensed": { "declared": "MIT" } }
                })))
                .mount(&server)
                .await;
            let mut comps = vec![make_component(
                "pkg:golang/github.com/yudai/pp@v2.0.1+incompatible",
            )];
            enrich_components(&source(&server, CdDiskCache::disabled()), &mut comps).await;

            assert_eq!(licences(&comps)[0], vec!["MIT".to_string()]);
            let reqs = server.received_requests().await.unwrap();
            assert_eq!(requests(&reqs, "GET"), 0);
        }

        #[tokio::test]
        async fn only_answers_reach_the_disk_cache() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/definitions"))
                .respond_with(ResponseTemplate::new(503))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/definitions/{}", coord_path("a"))))
                .respond_with(ResponseTemplate::new(200).set_body_json(body(Some("MIT"))))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(500))
                .mount(&server)
                .await;
            let dir = tempfile::tempdir().unwrap();
            let mut comps = components();
            enrich_components(&source(&server, CdDiskCache::at(dir.path().to_path_buf())), &mut comps)
                .await;

            let disk = CdDiskCache::at(dir.path().to_path_buf());
            let coord = |n: &str| cd_coord_for(&make_component(&format!("pkg:npm/{n}@1.0.0"))).unwrap();
            assert!(disk.get(&coord("a")).is_some(), "an answer is persisted");
            assert!(
                disk.get(&coord("b")).is_none(),
                "a 500 said nothing about the package and must not be cached for 7 days"
            );
        }
    }

    #[tokio::test]
    async fn offline_mode_is_a_noop() {
        let source = ClearlyDefinedSource::new(true);
        let mut comps = vec![make_component("pkg:npm/express@4.18.2")];
        let n = enrich_components(&source, &mut comps).await;
        assert_eq!(n, 0);
        assert!(comps[0].concluded_licenses.is_empty());
    }

    #[tokio::test]
    async fn unsupported_ecosystems_skipped_silently() {
        let source = ClearlyDefinedSource::new(true);
        let mut comps = vec![
            make_component("pkg:deb/ubuntu/curl@7.88.1"),
            make_component("pkg:rpm/fedora/bash@5.2.15-1.fc40"),
            make_component("pkg:generic/cpython@3.11"),
        ];
        let n = enrich_components(&source, &mut comps).await;
        assert_eq!(n, 0);
        for c in &comps {
            assert!(c.concluded_licenses.is_empty());
        }
    }

    #[test]
    fn apply_definition_adds_canonical_spdx() {
        let mut c = make_component("pkg:npm/express@4.18.2");
        let def = CdDefinition {
            declared_license: Some("MIT".to_string()),
        };
        let added = ClearlyDefinedSource::apply_definition(&mut c, &def);
        assert!(added);
        assert_eq!(c.concluded_licenses.len(), 1);
        assert_eq!(c.concluded_licenses[0].as_str(), "MIT");
    }

    #[test]
    fn apply_definition_dedups() {
        let mut c = make_component("pkg:npm/express@4.18.2");
        let def = CdDefinition {
            declared_license: Some("MIT".to_string()),
        };
        assert!(ClearlyDefinedSource::apply_definition(&mut c, &def));
        // Second apply with same value should be a no-op.
        assert!(!ClearlyDefinedSource::apply_definition(&mut c, &def));
        assert_eq!(c.concluded_licenses.len(), 1);
    }

    #[test]
    fn apply_definition_skips_non_canonical_spdx() {
        let mut c = make_component("pkg:npm/foo@1.0.0");
        let def = CdDefinition {
            declared_license: Some("Some Random License".to_string()),
        };
        let added = ClearlyDefinedSource::apply_definition(&mut c, &def);
        assert!(!added);
        assert!(c.concluded_licenses.is_empty());
    }

    #[test]
    fn apply_definition_skips_when_declared_is_none() {
        let mut c = make_component("pkg:npm/foo@1.0.0");
        let def = CdDefinition {
            declared_license: None,
        };
        assert!(!ClearlyDefinedSource::apply_definition(&mut c, &def));
        assert!(c.concluded_licenses.is_empty());
    }

    #[test]
    fn apply_definition_compound_expression_preserved() {
        let mut c = make_component("pkg:cargo/anyhow@1.0.80");
        let def = CdDefinition {
            declared_license: Some("MIT OR Apache-2.0".to_string()),
        };
        assert!(ClearlyDefinedSource::apply_definition(&mut c, &def));
        assert_eq!(c.concluded_licenses[0].as_str(), "MIT OR Apache-2.0");
    }
}
