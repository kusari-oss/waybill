// Milestone 055 — Go transitive-edge resolver: 4-step ladder.
//
// Module-level `#[allow(dead_code)]`: this module exposes the documented
// resolver API surface from contracts/resolver-api.md, but several
// items are reachable only via paths not yet wired:
//   - `ResolutionStep::GoModGraph` is constructed by step 1 wiring
//     in T033 (US2).
//   - `ModuleGraphEntry.source` is documented for external consumers
//     (debugging, ladder-summary introspection) but the production
//     `read()` path consumes only `requires()`.
//   - `ModuleGraphMap::{entry, iter, len, is_empty}` mirror the
//     contract's public accessors; not all are exercised by the
//     internal call sites.
//   - `WorkspaceContext.{excludes, gomodcache}` are populated for
//     future use (transitive `replace`/`exclude` audit, GOMODCACHE
//     introspection) — not consumed by the current orchestrator.
// All of these are part of the documented public-API surface of the
// resolver and removing them would be a regression. The allow is
// removed once T033 (step 1) lands AND the integration tests in
// `tests/go_transitive_edges.rs` start consuming `entry()` /
// `summary()` etc. for assertions.
#![allow(dead_code)]

//
// This module is the orchestrator for spec FR-002's resolution ladder:
//
//   1. `go mod graph` (when `go` is on PATH and `--offline` not set)
//   2. `$GOMODCACHE` walk (existing 053 behavior; reuses
//      `legacy::cache_lookup_depends`)
//   3. Proxy fetch from `$GOPROXY` (per the Go module proxy protocol)
//   4. Graceful no-edges fallthrough (component still emits with empty
//      `depends`; FR-009 ladder summary names the count)
//
// All edges are intersected with the workspace's `go.sum` per FR-003
// (`go.sum` is canonical for what's installed). Workspace-level `replace`
// directives are applied per FR-006.
//
// See specs/055-go-transitive-edges/spec.md and
// specs/055-go-transitive-edges/contracts/resolver-api.md for the
// full contract.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::scan_fs::package_db::golang::goprivate::{
    parse_private_patterns, parse_proxy_chain, PrivatePatterns, ProxyChain,
};
use crate::scan_fs::package_db::golang::legacy::{
    parse_go_mod, GoModCache, GoModDocument, GoSumEntry, GoSumKind,
};
use crate::scan_fs::package_db::golang::module_id::ModuleId;
use crate::scan_fs::package_db::golang::proxy_fetch::{build_http_client, fetch_module_mod};

// --------------------------------------------------------------------
// Resolution-step taxonomy
// --------------------------------------------------------------------

/// Which step of the 4-step ladder supplied this module's transitive
/// requires (per FR-002 / FR-009 ladder summary).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionStep {
    /// Step 1: `go mod graph` subprocess.
    GoModGraph,
    /// Step 2: `$GOMODCACHE` walk (the milestone 053 codepath).
    GoModCache,
    /// Step 3: HTTP fetch from `$GOPROXY`.
    Proxy,
    /// Step 4: graceful fallthrough — no edges produced for this module.
    None,
}

/// Per-module record after resolution. `requires` is post-replace, post-
/// intersection-with-go.sum.
#[derive(Clone, Debug)]
pub struct ModuleGraphEntry {
    pub module: ModuleId,
    pub requires: Vec<ModuleId>,
    pub source: ResolutionStep,
}

// --------------------------------------------------------------------
// Top-level resolver output
// --------------------------------------------------------------------

/// The complete module graph for a single scan. Keyed by `ModuleId`.
/// Consulted by `legacy::read()` to populate each `PackageDbEntry`'s
/// `depends` field.
#[derive(Clone, Debug, Default)]
pub struct ModuleGraphMap {
    entries: HashMap<ModuleId, ModuleGraphEntry>,
    summary: LadderSummary,
}

impl ModuleGraphMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn requires(&self, m: &ModuleId) -> &[ModuleId] {
        self.entries
            .get(m)
            .map(|e| e.requires.as_slice())
            .unwrap_or(&[])
    }

    pub fn entry(&self, m: &ModuleId) -> Option<&ModuleGraphEntry> {
        self.entries.get(m)
    }

    pub fn summary(&self) -> &LadderSummary {
        &self.summary
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ModuleId, &ModuleGraphEntry)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // --- mutating API used internally by GraphResolver ---

    pub(crate) fn insert(&mut self, entry: ModuleGraphEntry) {
        self.entries.insert(entry.module.clone(), entry);
    }

    pub(crate) fn contains(&self, m: &ModuleId) -> bool {
        self.entries.contains_key(m)
    }

    pub(crate) fn summary_mut(&mut self) -> &mut LadderSummary {
        &mut self.summary
    }

    pub(crate) fn entries_mut(&mut self) -> &mut HashMap<ModuleId, ModuleGraphEntry> {
        &mut self.entries
    }
}

// --------------------------------------------------------------------
// FR-009 ladder summary
// --------------------------------------------------------------------

/// Counters behind the FR-009 per-scan `tracing::info` summary line.
#[derive(Clone, Debug, Default)]
pub struct LadderSummary {
    pub graph_count: usize,
    pub cache_count: usize,
    pub proxy_count: usize,
    pub missing_count: usize,
    pub fetch_errors: HashMap<String, usize>,
}

impl fmt::Display for LadderSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "go transitive edges: ladder=[graph:{}, cache:{}, proxy:{}, missing:{}]",
            self.graph_count, self.cache_count, self.proxy_count, self.missing_count
        )
    }
}

// --------------------------------------------------------------------
// Workspace context
// --------------------------------------------------------------------

/// Inputs the resolver needs from the workspace + environment.
/// Constructed once per scan.
#[derive(Clone, Debug)]
pub struct WorkspaceContext {
    pub root_dir: PathBuf,
    pub go_sum_modules: HashSet<ModuleId>,
    pub replaces: HashMap<ModuleId, ModuleId>,
    pub excludes: HashSet<ModuleId>,
    pub offline: bool,
    pub gomodcache: PathBuf,
    pub goproxy: ProxyChain,
    pub goprivate: PrivatePatterns,
}

// --------------------------------------------------------------------
// Step-result + error taxonomy
// --------------------------------------------------------------------

/// Outcome of a single ladder-step invocation. The orchestrator decides
/// whether to fall through based on this value.
#[derive(Clone, Debug)]
pub enum StepResult<T> {
    /// Step succeeded; data attached.
    Ok(T),
    /// Step is unavailable (precondition not met) — fall through silently.
    /// e.g., `go` not on PATH, `--offline` set, `GOPROXY=off`.
    Unavailable,
    /// Step attempted and failed — fall through with a `tracing::warn`.
    Failed(StepError),
}

#[derive(Clone, Debug)]
pub struct StepError {
    pub class: ErrorClass,
    pub detail: String,
}

/// Operator-friendly error classification for `tracing::warn` lines per
/// research.md R14. Stable string names (`error_class="timeout"`, etc.)
/// are used in the summary's `fetch_errors` map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorClass {
    Timeout,
    Http4xx,
    Http404,
    Http5xx,
    Dns,
    Connection,
    Tls,
    Parse,
    Other,
}

impl ErrorClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Http4xx => "http_4xx",
            Self::Http404 => "http_404",
            Self::Http5xx => "http_5xx",
            Self::Dns => "dns",
            Self::Connection => "connection",
            Self::Tls => "tls",
            Self::Parse => "parse",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// --------------------------------------------------------------------
// Resolver config + error
// --------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct GraphResolverConfig {
    pub go_mod_graph_timeout: Duration,
    pub fetch_connect_timeout: Duration,
    pub fetch_total_timeout: Duration,
    pub fetch_concurrency: usize,
}

impl Default for GraphResolverConfig {
    fn default() -> Self {
        Self {
            go_mod_graph_timeout: Duration::from_secs(30), // FR-007
            fetch_connect_timeout: Duration::from_secs(10), // FR-008
            fetch_total_timeout: Duration::from_secs(30),  // FR-008
            fetch_concurrency: 16,                          // FR-008a
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum GraphResolverError {
    #[error("workspace go.sum missing or unreadable: {0}")]
    GoSumMissing(#[source] std::io::Error),

    #[error("workspace go.mod missing or unreadable: {0}")]
    GoModMissing(#[source] std::io::Error),
}

// --------------------------------------------------------------------
// Resolver
// --------------------------------------------------------------------

/// 4-step ladder orchestrator. `resolve()` is the single public entry
/// point and is consumed by `legacy::read()` once per scan.
///
/// In milestone 055, the body of `resolve()` is split into private
/// step-N functions implemented across this file and the sibling
/// `proxy_fetch` / `go_mod_graph` modules. The orchestration is
/// implemented incrementally by tasks T021–T024 (US1) and T031–T033 (US2).
pub struct GraphResolver {
    config: GraphResolverConfig,
}

impl GraphResolver {
    pub fn new(config: GraphResolverConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &GraphResolverConfig {
        &self.config
    }

    /// Resolve the transitive module graph for a single workspace
    /// (one `go.mod` + `go.sum` pair).
    ///
    /// Walks the 4-step ladder per FR-002 in order, accumulating
    /// `(module, requires)` records into a `ModuleGraphMap` keyed by
    /// `ModuleId`. After the ladder, applies workspace-level `replace`
    /// directives (FR-006) and intersects with `go.sum` (FR-003,
    /// dropping dangling targets), then emits the FR-009 ladder summary
    /// at `tracing::info`.
    ///
    /// Returns `Ok(map)` even when individual steps fail — graceful
    /// degradation is the spec's contract. The returned map covers
    /// every `ModuleId` in `ctx.go_sum_modules` (entries for which no
    /// step succeeded carry `ResolutionStep::None` and an empty
    /// `requires`).
    pub fn resolve(
        &self,
        ctx: &WorkspaceContext,
        cache: &GoModCache,
    ) -> Result<ModuleGraphMap, GraphResolverError> {
        let mut map = ModuleGraphMap::new();

        // Step 1 — `go mod graph`. Preferred when `go` is on PATH and
        // `--offline` is not set: one subprocess gives the entire
        // resolved DAG with MVS + replace + exclude already applied.
        if !ctx.offline {
            self.step1_go_mod_graph(&mut map, ctx);
        }

        // Step 2 — `$GOMODCACHE` walk. Reuses the existing
        // milestone 053 `cache_lookup_depends` codepath via
        // GoModCache::read_mod_file.
        self.step2_cache_walk(&mut map, ctx, cache);

        // Step 3 — proxy fetch (parallel via std::thread + bounded
        // mpsc). Disabled if `--offline` set, `GOPROXY=off`, or the
        // chain is empty. Per-module fetches that are GOPRIVATE-matched
        // are skipped to avoid leaking private module names to the
        // public proxy (FR-004).
        if !ctx.offline {
            self.step3_proxy_fetch(&mut map, ctx);
        }

        // Step 4 — graceful empty fallthrough. Every go.sum module
        // that didn't pick up edges from steps 1–3 still emits a
        // record (with empty requires) so downstream code can iterate
        // uniformly.
        self.step4_empty_fallthrough(&mut map, ctx);

        // Apply workspace-level `replace` directives to every edge
        // (FR-006). Transitive modules' OWN replaces are ignored,
        // matching Go's semantics (R12).
        apply_replaces(&mut map, &ctx.replaces);

        // Intersect with go.sum: any edge whose target isn't in
        // `ctx.go_sum_modules` is dropped silently (FR-003).
        intersect_with_go_sum(&mut map, &ctx.go_sum_modules);

        // FR-009 summary line. Tracing fields are flattened so
        // structured-log consumers can group by ladder step.
        let s = map.summary();
        tracing::info!(
            graph_count = s.graph_count,
            cache_count = s.cache_count,
            proxy_count = s.proxy_count,
            missing_count = s.missing_count,
            "{}",
            s
        );

        Ok(map)
    }

    // -- private step bodies ---------------------------------------

    fn step1_go_mod_graph(&self, map: &mut ModuleGraphMap, ctx: &WorkspaceContext) {
        use crate::scan_fs::package_db::golang::go_mod_graph::run_go_mod_graph;
        // run_go_mod_graph internally probes `go version`; if `go` is
        // not on PATH, returns Unavailable and we fall through silently.
        match run_go_mod_graph(&ctx.root_dir, self.config.go_mod_graph_timeout) {
            StepResult::Ok(parsed_map) => {
                // The parsed map is keyed by parent ModuleId. The
                // workspace's main-module entry has an empty version
                // and represents a project we never want to add as a
                // component (it's the workspace itself); skip it.
                for (parent, children) in parsed_map {
                    if parent.version().is_empty() {
                        continue;
                    }
                    if !ctx.go_sum_modules.contains(&parent) {
                        // Ignore parents not in our go.sum scope —
                        // could be a workspace member from a `go.work`
                        // file (out of scope) or stale `go mod graph`
                        // output.
                        continue;
                    }
                    if map.contains(&parent) {
                        continue;
                    }
                    map.insert(ModuleGraphEntry {
                        module: parent.clone(),
                        requires: children,
                        source: ResolutionStep::GoModGraph,
                    });
                    map.summary_mut().graph_count += 1;
                }
            }
            StepResult::Unavailable => {}
            StepResult::Failed(err) => {
                tracing::warn!(
                    error_class = err.class.as_str(),
                    detail = err.detail,
                    "`go mod graph` failed; falling through to cache walk + proxy fetch"
                );
            }
        }
    }

    fn step2_cache_walk(
        &self,
        map: &mut ModuleGraphMap,
        ctx: &WorkspaceContext,
        cache: &GoModCache,
    ) {
        if cache.is_empty() {
            return;
        }
        for module in &ctx.go_sum_modules {
            if map.contains(module) {
                continue;
            }
            let Some(text) = cache.read_mod_file(module.path(), module.version()) else {
                continue;
            };
            let doc = parse_go_mod(&text);
            let requires = doc
                .requires
                .into_iter()
                .map(|r| ModuleId::new(r.path, r.version))
                .collect();
            map.insert(ModuleGraphEntry {
                module: module.clone(),
                requires,
                source: ResolutionStep::GoModCache,
            });
            map.summary_mut().cache_count += 1;
        }
    }

    fn step3_proxy_fetch(&self, map: &mut ModuleGraphMap, ctx: &WorkspaceContext) {
        if ctx.goproxy.is_off() || ctx.goproxy.is_empty() {
            return;
        }

        // Collect targets still missing from the map AND not GOPRIVATE-matched.
        let to_fetch: Vec<ModuleId> = ctx
            .go_sum_modules
            .iter()
            .filter(|m| !map.contains(m))
            .filter(|m| !ctx.goprivate.matches(m.path()))
            .cloned()
            .collect();

        if to_fetch.is_empty() {
            return;
        }

        // `reqwest::blocking::Client` spawns and OWNS its own internal
        // tokio runtime. Constructing or dropping it from within an
        // async context (which is what `mikebom sbom scan` runs in,
        // because main.rs is #[tokio::main]) panics with
        // `Cannot drop a runtime in a context where blocking is not
        // allowed`. Wrapping the entire client-lifecycle block in a
        // dedicated OS thread isolates the blocking runtime from any
        // surrounding tokio context. The thread joins synchronously
        // before this function returns, preserving the resolver's
        // sync API.
        let config = self.config.clone();
        let proxy_chain = ctx.goproxy.clone();
        let concurrency = self.config.fetch_concurrency;
        let results = std::thread::spawn(move || {
            let client = match build_http_client(&config) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "failed to build HTTP client for proxy fetch; skipping step 3"
                    );
                    return Vec::new();
                }
            };
            parallel_fetch(&client, &proxy_chain, &to_fetch, concurrency)
        })
        .join()
        .unwrap_or_default();

        for (module, result) in results {
            match result {
                StepResult::Ok(text) => {
                    let doc = parse_go_mod(&text);
                    let requires = doc
                        .requires
                        .into_iter()
                        .map(|r| ModuleId::new(r.path, r.version))
                        .collect();
                    map.insert(ModuleGraphEntry {
                        module,
                        requires,
                        source: ResolutionStep::Proxy,
                    });
                    map.summary_mut().proxy_count += 1;
                }
                StepResult::Failed(err) => {
                    tracing::warn!(
                        module = %module,
                        error_class = err.class.as_str(),
                        detail = err.detail,
                        "go-mod proxy fetch failed"
                    );
                    *map.summary_mut()
                        .fetch_errors
                        .entry(err.class.as_str().to_string())
                        .or_insert(0) += 1;
                }
                StepResult::Unavailable => {
                    // Chain reported it cannot help (Off / Direct-only /
                    // empty). Don't emit a warn — that's intentional
                    // configuration.
                }
            }
        }
    }

    fn step4_empty_fallthrough(&self, map: &mut ModuleGraphMap, ctx: &WorkspaceContext) {
        for module in &ctx.go_sum_modules {
            if map.contains(module) {
                continue;
            }
            map.insert(ModuleGraphEntry {
                module: module.clone(),
                requires: Vec::new(),
                source: ResolutionStep::None,
            });
            map.summary_mut().missing_count += 1;
        }
    }
}

// --------------------------------------------------------------------
// Workspace-level replace + intersection
// --------------------------------------------------------------------

/// Rewrite each edge target via `replaces` (workspace-level). Used after
/// all ladder steps so step 1 (`go mod graph`, which already applies
/// replaces) and steps 2/3 (which return raw requires) end up with
/// identical edge sets.
fn apply_replaces(map: &mut ModuleGraphMap, replaces: &HashMap<ModuleId, ModuleId>) {
    if replaces.is_empty() {
        return;
    }
    for entry in map.entries_mut().values_mut() {
        for child in entry.requires.iter_mut() {
            if let Some(replacement) = replaces.get(child) {
                *child = replacement.clone();
            }
        }
    }
}

/// Resolve every edge target to the version actually installed per
/// `go.sum`, dropping edges whose path doesn't appear at all.
///
/// FR-003 + Go MVS semantics: a `go.mod` may declare `require X v1.0.0`
/// but the workspace's MVS-selected version (recorded in `go.sum`) might
/// be `X v2.0.0`. The actually-installed edge is `→ X v2.0.0`. Without
/// this rewrite, the resolver would drop the edge because
/// `(X, v1.0.0)` isn't a key in `go_sum_modules`. With it, we rewrite
/// the edge target's version to whatever `go.sum` says is installed
/// for that path. Edges whose path is wholly absent from `go.sum`
/// (e.g., test-only deps that didn't make the install set) are still
/// dropped.
fn intersect_with_go_sum(map: &mut ModuleGraphMap, go_sum_modules: &HashSet<ModuleId>) {
    // Build a path → version index of go.sum once. Go convention:
    // exactly one version of each module path appears in a workspace's
    // go.sum (MVS picks one); duplicates would indicate an ill-formed
    // sum file. We take whichever version appears (HashSet iteration
    // order doesn't matter — there should only be one per path).
    let mut sum_by_path: HashMap<&str, &str> = HashMap::new();
    for m in go_sum_modules {
        sum_by_path.insert(m.path(), m.version());
    }

    for entry in map.entries_mut().values_mut() {
        let parent_str = entry.module.to_string();
        entry.requires.retain_mut(|child| {
            match sum_by_path.get(child.path()) {
                Some(installed_version) => {
                    if *installed_version != child.version() {
                        tracing::debug!(
                            parent = %parent_str,
                            child_path = child.path(),
                            declared = child.version(),
                            installed = installed_version,
                            "rewriting edge target to MVS-selected version per go.sum"
                        );
                        *child = ModuleId::new(child.path(), *installed_version);
                    }
                    true
                }
                None => {
                    tracing::debug!(
                        parent = %parent_str,
                        child_path = child.path(),
                        "dropping edge to module path not in go.sum (FR-003)"
                    );
                    false
                }
            }
        });
    }
}

// --------------------------------------------------------------------
// Parallel proxy fetcher
// --------------------------------------------------------------------

/// Fetch all `targets` via the proxy chain using a fixed `concurrency`-way
/// worker-thread pool. Returns one `(ModuleId, StepResult)` per target.
fn parallel_fetch(
    client: &reqwest::blocking::Client,
    chain: &ProxyChain,
    targets: &[ModuleId],
    concurrency: usize,
) -> Vec<(ModuleId, StepResult<String>)> {
    if targets.is_empty() {
        return Vec::new();
    }
    let n = targets.len();
    let workers = concurrency.max(1).min(n);

    // Bounded queue: workers can't get ahead of producers, but the
    // bound is small (workers * 1) so memory stays O(workers).
    let (job_tx, job_rx) = mpsc::sync_channel::<ModuleId>(workers);
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (result_tx, result_rx) = mpsc::channel();

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let job_rx = Arc::clone(&job_rx);
        let result_tx = result_tx.clone();
        let client = client.clone();
        let chain = chain.clone();
        let h = std::thread::spawn(move || loop {
            let job = {
                let rx = match job_rx.lock() {
                    Ok(g) => g,
                    Err(_) => break, // poisoned mutex — stop worker
                };
                match rx.recv() {
                    Ok(j) => j,
                    Err(_) => break, // channel closed → no more work
                }
            };
            let r = fetch_module_mod(&client, &chain, &job);
            if result_tx.send((job, r)).is_err() {
                break; // collector dropped
            }
        });
        handles.push(h);
    }
    drop(result_tx); // last clone so result_rx terminates after workers do

    // Producer: enqueue all jobs then signal end.
    for target in targets {
        if job_tx.send(target.clone()).is_err() {
            break; // workers all gone
        }
    }
    drop(job_tx);

    // Collect results.
    let mut results = Vec::with_capacity(n);
    while let Ok(pair) = result_rx.recv() {
        results.push(pair);
    }

    // Reap workers.
    for h in handles {
        let _ = h.join();
    }

    results
}

// --------------------------------------------------------------------
// WorkspaceContext builder
// --------------------------------------------------------------------

impl WorkspaceContext {
    /// Build a `WorkspaceContext` from a parsed `go.mod` document and
    /// its `go.sum` entries.
    ///
    /// `project_root` is the directory containing both files. `offline`
    /// is plumbed from the global `--offline` CLI flag. Environment
    /// variables `$GOPROXY` and `$GOPRIVATE` are read at this point;
    /// if mikebom is invoked from a CI runner that wants a fixed proxy,
    /// the user sets it in the environment.
    pub fn from_parts(
        project_root: PathBuf,
        doc: &GoModDocument,
        sums: &[GoSumEntry],
        offline: bool,
    ) -> Self {
        // Belt-and-suspenders for callers that haven't yet plumbed
        // `--offline` through their call chain (T010 noted that
        // threading it through `scan_path` → `read_all` → `golang::read`
        // is a multi-test-fixture refactor we deferred for milestone
        // 055). main.rs sets `MIKEBOM_OFFLINE=1` when `cli.offline`
        // is true; we OR that into the explicit param here so any
        // call site that hard-codes `false` still respects the user's
        // intent.
        let offline = offline || std::env::var("MIKEBOM_OFFLINE").is_ok();

        let go_sum_modules: HashSet<ModuleId> = sums
            .iter()
            .filter(|s| s.kind == GoSumKind::Module)
            .map(|s| ModuleId::new(s.module.clone(), s.version.clone()))
            .collect();

        let replaces: HashMap<ModuleId, ModuleId> = doc
            .replaces
            .iter()
            .map(|((op, ov), (np, nv))| {
                (
                    ModuleId::new(op.clone(), ov.clone()),
                    ModuleId::new(np.clone(), nv.clone()),
                )
            })
            .collect();

        let excludes: HashSet<ModuleId> = doc
            .excludes
            .iter()
            .map(|(p, v)| ModuleId::new(p.clone(), v.clone()))
            .collect();

        // Resolve $GOMODCACHE (informational — the resolver hands the
        // real cache via the separate `&GoModCache` parameter for
        // multi-root discovery). Stored here for future use / tracing.
        let gomodcache = std::env::var_os("GOMODCACHE")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("GOPATH")
                    .map(|p| PathBuf::from(p).join("pkg/mod"))
            })
            .unwrap_or_else(|| PathBuf::from(""));

        let goproxy = parse_proxy_chain(std::env::var("GOPROXY").ok().as_deref())
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "failed to parse $GOPROXY; using default chain");
                ProxyChain::default()
            });

        let goprivate = parse_private_patterns(
            std::env::var("GOPRIVATE").ok().as_deref().unwrap_or(""),
        );

        Self {
            root_dir: project_root,
            go_sum_modules,
            replaces,
            excludes,
            offline,
            gomodcache,
            goproxy,
            goprivate,
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn ladder_summary_renders_canonical_format() {
        // FR-009: exact tracing line format.
        let s = LadderSummary {
            graph_count: 12,
            cache_count: 3,
            proxy_count: 27,
            missing_count: 1,
            ..Default::default()
        };
        assert_eq!(
            s.to_string(),
            "go transitive edges: ladder=[graph:12, cache:3, proxy:27, missing:1]"
        );
    }

    #[test]
    fn error_class_has_stable_string_repr() {
        // Used as a HashMap key in LadderSummary.fetch_errors and as the
        // `error_class` field in tracing::warn — stability matters.
        assert_eq!(ErrorClass::Timeout.as_str(), "timeout");
        assert_eq!(ErrorClass::Http404.as_str(), "http_404");
        assert_eq!(ErrorClass::Http4xx.as_str(), "http_4xx");
        assert_eq!(ErrorClass::Http5xx.as_str(), "http_5xx");
        assert_eq!(ErrorClass::Dns.as_str(), "dns");
        assert_eq!(ErrorClass::Connection.as_str(), "connection");
        assert_eq!(ErrorClass::Tls.as_str(), "tls");
        assert_eq!(ErrorClass::Parse.as_str(), "parse");
        assert_eq!(ErrorClass::Other.as_str(), "other");
    }

    #[test]
    fn resolver_config_defaults_match_spec() {
        // FR-007, FR-008, FR-008a hard-coded values.
        let cfg = GraphResolverConfig::default();
        assert_eq!(cfg.go_mod_graph_timeout, Duration::from_secs(30));
        assert_eq!(cfg.fetch_connect_timeout, Duration::from_secs(10));
        assert_eq!(cfg.fetch_total_timeout, Duration::from_secs(30));
        assert_eq!(cfg.fetch_concurrency, 16);
    }

    #[test]
    fn module_graph_map_default_is_empty() {
        let m = ModuleGraphMap::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert_eq!(m.summary().graph_count, 0);
    }

    #[test]
    fn module_graph_map_requires_returns_empty_for_unknown() {
        let m = ModuleGraphMap::new();
        let unknown = ModuleId::new("github.com/never/seen", "v0.0.0");
        assert!(m.requires(&unknown).is_empty());
    }

    // --- Workspace-level helpers ---

    #[test]
    fn intersect_drops_dangling_targets() {
        // FR-003: edge whose target isn't in go.sum gets dropped.
        let mut map = ModuleGraphMap::new();
        let parent = ModuleId::new("github.com/foo/bar", "v1.0.0");
        let in_sum = ModuleId::new("github.com/in/sum", "v1.0.0");
        let dangling = ModuleId::new("github.com/dangling/x", "v0.1.0");
        map.insert(ModuleGraphEntry {
            module: parent.clone(),
            requires: vec![in_sum.clone(), dangling.clone()],
            source: ResolutionStep::GoModCache,
        });
        let mut go_sum: HashSet<ModuleId> = HashSet::new();
        go_sum.insert(parent.clone());
        go_sum.insert(in_sum.clone());
        intersect_with_go_sum(&mut map, &go_sum);
        let entry = map.entry(&parent).expect("parent kept");
        assert_eq!(entry.requires, vec![in_sum]);
    }

    #[test]
    fn apply_replaces_rewrites_edge_targets() {
        // FR-006: workspace replace map applies to every edge.
        let mut map = ModuleGraphMap::new();
        let parent = ModuleId::new("github.com/parent", "v1.0.0");
        let original = ModuleId::new("github.com/old", "v1.0.0");
        let replacement = ModuleId::new("github.com/new", "v2.0.0");
        map.insert(ModuleGraphEntry {
            module: parent.clone(),
            requires: vec![original.clone()],
            source: ResolutionStep::GoModCache,
        });
        let mut replaces = HashMap::new();
        replaces.insert(original, replacement.clone());
        apply_replaces(&mut map, &replaces);
        assert_eq!(map.entry(&parent).unwrap().requires, vec![replacement]);
    }
}

// --------------------------------------------------------------------
// Wiremock-backed integration tests for the resolver (FR-011 / FR-012 /
// SC-001 / SC-005 / SC-007 / FR-009).
//
// These live inside the resolver's source file rather than under
// `mikebom-cli/tests/` because exposing the entire `scan_fs` tree via
// the library crate would cascade-require lib-exposing every other
// binary-internal module (`trace`, `generate`, `resolve`, ...) — too
// large a change for milestone 055. Functionally these test the same
// properties FR-012 specifies (resolver reaches modules via proxy
// fetch when cache+toolchain are absent); the location is the only
// thing that differs.
// --------------------------------------------------------------------
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod wiremock_integration {
    use super::*;
    use crate::scan_fs::package_db::golang::legacy::{parse_go_sum, GoSumKind};
    use std::collections::HashMap as StdHashMap;
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const ARGO_FIXTURE_REL: &str =
        "../tests/fixtures/go/argo-style-no-cache/argo-workflows";

    /// Synthesized minimal `go.mod` bodies for every module in the
    /// argo-style-no-cache fixture's `go.sum`. Transitive requires
    /// link back to other go.sum modules so the FR-003 intersection
    /// step actually has work to do.
    fn synth_mod_bodies() -> StdHashMap<&'static str, &'static str> {
        let mut m = StdHashMap::new();
        m.insert(
            "github.com/spf13/cobra/@v/v1.3.0.mod",
            "module github.com/spf13/cobra\n\
             go 1.18\n\
             require (\n\
               \tgithub.com/spf13/viper v1.10.1\n\
               \tgithub.com/stretchr/testify v1.7.0\n\
             )\n",
        );
        m.insert(
            "github.com/spf13/viper/@v/v1.10.1.mod",
            "module github.com/spf13/viper\n\
             go 1.18\n\
             require (\n\
               \tgithub.com/sirupsen/logrus v1.8.1\n\
               \tgopkg.in/yaml.v3 v3.0.1\n\
               \tgithub.com/stretchr/testify v1.7.0\n\
             )\n",
        );
        m.insert(
            "github.com/stretchr/testify/@v/v1.7.0.mod",
            "module github.com/stretchr/testify\n\
             go 1.13\n\
             require (\n\
               \tgithub.com/pkg/errors v0.9.1\n\
               \tgopkg.in/yaml.v3 v3.0.1\n\
             )\n",
        );
        m.insert(
            "github.com/sirupsen/logrus/@v/v1.8.1.mod",
            "module github.com/sirupsen/logrus\n\
             go 1.13\n\
             require github.com/stretchr/testify v1.7.0\n",
        );
        m.insert(
            "github.com/golang/protobuf/@v/v1.5.2.mod",
            "module github.com/golang/protobuf\n\
             go 1.9\n\
             require google.golang.org/protobuf v1.36.11\n",
        );
        m.insert(
            "github.com/google/uuid/@v/v1.3.0.mod",
            "module github.com/google/uuid\n\
             go 1.12\n",
        );
        m.insert(
            "github.com/pkg/errors/@v/v0.9.1.mod",
            "module github.com/pkg/errors\n\
             go 1.13\n",
        );
        m.insert(
            "github.com/prometheus/client_golang/@v/v1.12.1.mod",
            "module github.com/prometheus/client_golang\n\
             go 1.13\n\
             require (\n\
               \tgithub.com/golang/protobuf v1.5.2\n\
               \tgithub.com/sirupsen/logrus v1.8.1\n\
             )\n",
        );
        m.insert(
            "google.golang.org/grpc/@v/v1.80.0.mod",
            "module google.golang.org/grpc\n\
             go 1.20\n\
             require (\n\
               \tgithub.com/golang/protobuf v1.5.2\n\
               \tgoogle.golang.org/protobuf v1.36.11\n\
             )\n",
        );
        m.insert(
            "google.golang.org/protobuf/@v/v1.36.11.mod",
            "module google.golang.org/protobuf\n\
             go 1.20\n\
             require github.com/golang/protobuf v1.5.2\n",
        );
        m.insert(
            "gopkg.in/yaml.v2/@v/v2.4.0.mod",
            "module gopkg.in/yaml.v2\n\
             go 1.15\n\
             require github.com/stretchr/testify v1.7.0\n",
        );
        m.insert(
            "gopkg.in/yaml.v3/@v/v3.0.1.mod",
            "module gopkg.in/yaml.v3\n\
             go 1.13\n\
             require github.com/stretchr/testify v1.7.0\n",
        );
        m.insert(
            "k8s.io/api/@v/v0.24.3.mod",
            "module k8s.io/api\n\
             go 1.16\n\
             require github.com/golang/protobuf v1.5.2\n",
        );
        m.insert(
            "k8s.io/client-go/@v/v0.24.3.mod",
            "module k8s.io/client-go\n\
             go 1.16\n\
             require (\n\
               \tk8s.io/api v0.24.3\n\
               \tgoogle.golang.org/grpc v1.80.0\n\
             )\n",
        );
        m
    }

    fn argo_fixture_dir() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push(ARGO_FIXTURE_REL);
        p
    }

    async fn start_mock_proxy() -> MockServer {
        let server = MockServer::start().await;
        for (suffix, body) in synth_mod_bodies() {
            Mock::given(method("GET"))
                .and(path(format!("/{suffix}")))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .mount(&server)
                .await;
        }
        server
    }

    /// Build a `WorkspaceContext` against the argo fixture, with
    /// `$GOPROXY` overridden to the mock URL (rather than reading from
    /// the live process env, which would be racy across tests).
    fn build_argo_context(mock_url: &str, offline: bool) -> WorkspaceContext {
        use crate::scan_fs::package_db::golang::goprivate::parse_proxy_chain;
        use crate::scan_fs::package_db::golang::legacy::parse_go_mod;

        let fixture = argo_fixture_dir();
        let go_mod_text = std::fs::read_to_string(fixture.join("go.mod")).unwrap();
        let go_sum_text = std::fs::read_to_string(fixture.join("go.sum")).unwrap();
        let doc = parse_go_mod(&go_mod_text);
        let sums = parse_go_sum(&go_sum_text);

        let mut ctx = WorkspaceContext::from_parts(fixture, &doc, &sums, offline);
        ctx.goproxy = parse_proxy_chain(Some(mock_url)).unwrap();
        ctx
    }

    fn argo_go_sum_modules() -> Vec<ModuleId> {
        let go_sum_text =
            std::fs::read_to_string(argo_fixture_dir().join("go.sum")).unwrap();
        parse_go_sum(&go_sum_text)
            .into_iter()
            .filter(|s| s.kind == GoSumKind::Module)
            .map(|s| ModuleId::new(s.module, s.version))
            .collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn ladder_step3_only_argo_fixture() {
        // SC-001 + SC-006 + SC-007 + FR-012: empty cache, no `go`,
        // proxy fetch supplies edges. Asserts ≥ 90% of expected-with-
        // edges modules emit at least one outgoing edge, every edge
        // target is in go.sum, and the FR-009 summary's proxy_count > 0.
        let server = start_mock_proxy().await;
        let mock_url = server.uri();

        let ctx = build_argo_context(&mock_url, false);
        // GoModCache::discover reads $GOMODCACHE / $GOPATH / $HOME env
        // vars and would pick up the host machine's real cache, which
        // would let step 2 fill edges before step 3 even runs. Using
        // Default explicitly gives an empty-roots cache so step 3 is
        // the only path that can supply edges (the spec's US1 case).
        let cache = crate::scan_fs::package_db::golang::legacy::GoModCache::default();
        assert!(cache.is_empty(), "Default GoModCache must be empty");

        let resolver = GraphResolver::new(GraphResolverConfig::default());
        let map = tokio::task::spawn_blocking(move || {
            resolver.resolve(&ctx, &cache).unwrap()
        })
        .await
        .unwrap();

        let summary = map.summary();
        eprintln!(
            "ladder summary: graph={} cache={} proxy={} missing={}",
            summary.graph_count,
            summary.cache_count,
            summary.proxy_count,
            summary.missing_count
        );

        let go_sum_modules = argo_go_sum_modules();

        // Modules whose synthesized .mod has NO requires (leaves) are
        // excluded from the SC-001 ratio's denominator — there's no edge
        // to expect for them. Per synth_mod_bodies(), the leaves are
        // `github.com/google/uuid` and `github.com/pkg/errors`.
        let expected_with_edges: Vec<&ModuleId> = go_sum_modules
            .iter()
            .filter(|m| {
                !matches!(
                    m.path(),
                    "github.com/google/uuid" | "github.com/pkg/errors"
                )
            })
            .collect();
        let actual_with_edges: usize = expected_with_edges
            .iter()
            .filter(|m| !map.requires(m).is_empty())
            .count();
        let ratio = actual_with_edges as f64 / expected_with_edges.len() as f64;
        assert!(
            ratio >= 0.90,
            "SC-001: expected ≥ 90% of expected-with-edges modules to emit edges, got {actual_with_edges}/{} = {:.1}%",
            expected_with_edges.len(),
            ratio * 100.0
        );

        // FR-003 / SC-006: every emitted edge target is in go.sum.
        let go_sum_set: HashSet<&ModuleId> = go_sum_modules.iter().collect();
        for (parent, entry) in map.iter() {
            for child in &entry.requires {
                assert!(
                    go_sum_set.contains(child),
                    "FR-003: edge {parent} → {child} target not in go.sum",
                );
            }
        }

        // SC-007: proxy step contributed.
        assert!(
            summary.proxy_count > 0,
            "SC-007: expected proxy_count > 0, got 0",
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn offline_makes_no_network_calls() {
        // SC-005 / FR-005: --offline disables ALL network. wiremock
        // catch-all 500 stub asserts no requests reach it.
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let mock_url = server.uri();

        let ctx = build_argo_context(&mock_url, true);
        // GoModCache::discover reads $GOMODCACHE / $GOPATH / $HOME env
        // vars and would pick up the host machine's real cache, which
        // would let step 2 fill edges before step 3 even runs. Using
        // Default explicitly gives an empty-roots cache so step 3 is
        // the only path that can supply edges (the spec's US1 case).
        let cache = crate::scan_fs::package_db::golang::legacy::GoModCache::default();

        let resolver = GraphResolver::new(GraphResolverConfig::default());
        let _map = tokio::task::spawn_blocking(move || {
            resolver.resolve(&ctx, &cache).unwrap()
        })
        .await
        .unwrap();

        let received = server.received_requests().await.unwrap_or_default();
        assert_eq!(
            received.len(),
            0,
            "SC-005: expected zero HTTP requests when offline=true, got {}",
            received.len()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn step1_real_go_mod_graph_parity_simple_module() {
        // T035: when `go` is available, mikebom's edge set against the
        // simple-module fixture matches `go mod graph` (intersected
        // with go.sum). Skips cleanly when `go` is not on PATH so CI
        // runners without the toolchain stay green.
        let go_present = std::process::Command::new("go")
            .arg("version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .is_some();
        if !go_present {
            eprintln!("skipping T035: `go` not on PATH");
            return;
        }

        // Run `go mod graph` against the simple-module fixture.
        let mut fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        fixture.push("../tests/fixtures/go/simple-module");
        let output = std::process::Command::new("go")
            .args(["mod", "graph"])
            .current_dir(&fixture)
            .output()
            .expect("go mod graph runs");
        if !output.status.success() {
            eprintln!(
                "skipping T035: `go mod graph` exited non-zero ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let go_graph = crate::scan_fs::package_db::golang::go_mod_graph::parse_go_mod_graph(
            &stdout,
        );

        // Run mikebom's resolver against the same fixture (no `--offline`,
        // so step 1 is allowed). MIKEBOM_OFFLINE may be set by other
        // tests in the same binary — ensure it's cleared for this test.
        std::env::remove_var("MIKEBOM_OFFLINE");
        let go_mod_text = std::fs::read_to_string(fixture.join("go.mod")).unwrap();
        let go_sum_text = std::fs::read_to_string(fixture.join("go.sum")).unwrap();
        let doc = crate::scan_fs::package_db::golang::legacy::parse_go_mod(&go_mod_text);
        let sums = crate::scan_fs::package_db::golang::legacy::parse_go_sum(&go_sum_text);
        let ctx = WorkspaceContext::from_parts(fixture.clone(), &doc, &sums, false);
        let cache = crate::scan_fs::package_db::golang::legacy::GoModCache::default();
        let resolver = GraphResolver::new(GraphResolverConfig::default());
        let map = tokio::task::spawn_blocking(move || resolver.resolve(&ctx, &cache).unwrap())
            .await
            .unwrap();

        // Build the comparison sets. Both are intersected with go.sum
        // and indexed by parent path. The MVS rewrite means edge target
        // versions in mikebom's output match go.sum's; `go mod graph`
        // emits the declared (pre-MVS) versions, so we compare on path.
        let go_sum_paths: HashSet<&str> = sums
            .iter()
            .filter(|s| s.kind == GoSumKind::Module)
            .map(|s| s.module.as_str())
            .collect();

        // Exclude main-module edges from the comparison: milestone 053
        // emits those via `build_main_module_entry`, not via the
        // resolver. The resolver's step 1 explicitly skips parents
        // with empty version (the main-module sentinel) per the
        // implementation.
        // Also exclude parents that aren't `Module`-kind entries in
        // `go.sum` — they're "Mod"-kind entries that contribute to MVS
        // resolution but aren't installed as components, so the
        // resolver has no parent to attach edges to.
        let main_module_path = doc.module_path.as_deref().unwrap_or("");
        let mut go_graph_edges: HashSet<(String, String)> = HashSet::new();
        for (parent, children) in go_graph.iter() {
            if parent.path() == main_module_path {
                continue;
            }
            if !go_sum_paths.contains(parent.path()) {
                continue;
            }
            let parent_path = parent.path().to_string();
            for child in children {
                if go_sum_paths.contains(child.path()) {
                    go_graph_edges.insert((parent_path.clone(), child.path().to_string()));
                }
            }
        }

        let mikebom_edges: HashSet<(String, String)> = map
            .iter()
            .flat_map(|(parent, entry)| {
                let parent_path = parent.path().to_string();
                entry
                    .requires
                    .iter()
                    .map(move |child| (parent_path.clone(), child.path().to_string()))
            })
            .collect();

        // Mikebom may emit edges from go.sum modules that go mod graph
        // doesn't list (e.g., a module's go.mod requires that didn't
        // make MVS — go mod graph would prune them). The other
        // direction is the meaningful check: every go-mod-graph edge
        // between go.sum modules SHOULD appear in mikebom's output.
        for edge in &go_graph_edges {
            assert!(
                mikebom_edges.contains(edge),
                "SC-002: missing edge in mikebom output: {} → {}",
                edge.0, edge.1
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn ladder_fall_through_with_404_proxy() {
        // FR-008 + FR-009 + spec Edge Case ("network failure"): every
        // proxy fetch returns 404 → resolver falls through to step 4
        // gracefully; missing_count > 0; fetch_errors records http_404.
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let mock_url = server.uri();

        let ctx = build_argo_context(&mock_url, false);
        // GoModCache::discover reads $GOMODCACHE / $GOPATH / $HOME env
        // vars and would pick up the host machine's real cache, which
        // would let step 2 fill edges before step 3 even runs. Using
        // Default explicitly gives an empty-roots cache so step 3 is
        // the only path that can supply edges (the spec's US1 case).
        let cache = crate::scan_fs::package_db::golang::legacy::GoModCache::default();

        let resolver = GraphResolver::new(GraphResolverConfig::default());
        let map = tokio::task::spawn_blocking(move || {
            resolver.resolve(&ctx, &cache).unwrap()
        })
        .await
        .unwrap();

        let summary = map.summary();
        assert!(
            summary.missing_count > 0,
            "FR-009: expected missing_count > 0 when every proxy fetch 404s",
        );
        assert_eq!(summary.proxy_count, 0);
        assert!(
            summary.fetch_errors.contains_key("http_404"),
            "FR-008: expected http_404 in fetch_errors, got {:?}",
            summary.fetch_errors.keys().collect::<Vec<_>>()
        );
    }
}
