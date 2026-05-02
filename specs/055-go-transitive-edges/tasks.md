---

description: "Tasks for milestone 055 — Go transitive dependency edges, anchored on go.sum"
---

# Tasks: Go transitive dependency edges, anchored on `go.sum`

**Input**: Design documents from `/specs/055-go-transitive-edges/`
**Prerequisites**: [plan.md](plan.md) (required), [spec.md](spec.md) (user stories), [research.md](research.md), [data-model.md](data-model.md), [contracts/resolver-api.md](contracts/resolver-api.md), [quickstart.md](quickstart.md)

**Tests**: Tests are explicitly required by the spec — FR-011 mandates unit tests for each ladder step, FR-012 mandates the integration test for the issue #102 reproduction. Test tasks below are NOT optional.

**Organization**: Grouped by user story per `/speckit.tasks` convention. US1 is the MVP (offline/no-go transitive edges via proxy-fetch); US2 adds the `go mod graph` preferred path; US3 is the realistic-project regression assertion.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Parallelizable — different files, no dependency on incomplete tasks at the same level
- **[Story]**: `[US1]` / `[US2]` / `[US3]` — maps to spec.md user story (Setup/Foundational/Polish phases have NO story label)
- All paths are absolute or relative-to-repo-root; CLAUDE.md pre-PR gate (`./scripts/pre-pr.sh`) MUST pass after every commit cluster

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add the one new dev-dep and prep the integration-test fixture directory. Both can proceed in parallel.

- [X] T001 [P] Add `wiremock = "0.6"` to `[dev-dependencies]` in `mikebom-cli/Cargo.toml` (NOT a workspace dep — keeps blast radius minimal per plan Complexity Tracking row 2). Verify `cargo +stable check -p mikebom --tests` resolves the new dep.

- [X] T002 [P] Create directory `tests/fixtures/go/argo-style-no-cache/proxy-mock/` (will hold synthesized `.mod` files for the integration test in T026; populated incrementally during US1).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish the `golang/` submodule scaffold + type system. Every user story depends on these types existing. Implementations live in story phases.

**⚠️ CRITICAL**: No story tasks may begin until T003–T010 complete and the workspace builds clean.

- [X] T003 Create `mikebom-cli/src/scan_fs/package_db/golang/module_id.rs` with the `ModuleId` newtype per [data-model.md](data-model.md#moduleid) — `path: String`, `version: String`, `Clone + Debug + Eq + Hash + PartialEq + Ord + PartialOrd` derives, `new()`, `path()`, `version()`, `Display` impl rendering `<path>@<version>` matching `go mod graph` format. Add `#[cfg(test)] mod tests { ... }` covering: pretty-printing, equality, ordering, hashing into `HashSet`. Tests must use `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.

- [X] T004 Create `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs` with the type scaffold from [data-model.md](data-model.md): `ResolutionStep` enum (4 variants), `ModuleGraphEntry`, `ModuleGraphMap`, `LadderSummary` (with `Default` derive), `WorkspaceContext` struct, `GraphResolverConfig` struct (with default values: 30 s `go_mod_graph_timeout`, 10 s `fetch_connect_timeout`, 30 s `fetch_total_timeout`, 16 `fetch_concurrency`), `GraphResolverError` (`thiserror`-derived enum per [contracts/resolver-api.md](contracts/resolver-api.md)), `StepResult<T>` enum (`Ok(T) / Unavailable / Failed(StepError)`), `StepError { class, detail }`, `ErrorClass` enum per R14 taxonomy. Plus `GraphResolver` struct and `GraphResolver::new(config)` constructor — `resolve()` body returns `unimplemented!()` for now (filled in T022). Depends on T003 (uses `ModuleId`).

- [X] T005 [P] Create `mikebom-cli/src/scan_fs/package_db/golang/goprivate.rs` with type scaffolds — `ProxyChain { entries: Vec<ProxyEntry> }`, `ProxyEntry` enum (`Url { url: Url, fall_through_on_404_only: bool } / Direct / Off`), `PrivatePattern { segments: Vec<PatternSegment> }`, `PatternSegment` enum (`Literal(String) / Glob(String)`), `PrivatePatterns { patterns: Vec<PrivatePattern> }`, plus function signatures `pub fn parse_proxy_chain(env_value: Option<&str>) -> Result<ProxyChain, ProxyParseError>` and `pub fn parse_private_patterns(env_value: &str) -> PrivatePatterns` and `impl PrivatePatterns { pub fn matches(&self, module_path: &str) -> bool }` — bodies are `unimplemented!()` stubs. Depends on T003 (file is independent but the build needs T003 done for module imports).

- [X] T006 [P] Create `mikebom-cli/src/scan_fs/package_db/golang/proxy_fetch.rs` with function signatures only — `pub fn escape_module_path(path: &str) -> Result<String, EscapeError>`, `pub fn build_proxy_url(proxy_base: &reqwest::Url, target: &ModuleId) -> Result<reqwest::Url, EscapeError>`, `pub async fn fetch_module_mod(client: &reqwest::Client, proxy_chain: &ProxyChain, target: &ModuleId) -> StepResult<String>`. Define `EscapeError` enum (`InvalidByte { byte: u8, position: usize }`). Bodies = `unimplemented!()`. Depends on T003.

- [X] T007 [P] Create `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs` with function signatures — `pub async fn run_go_mod_graph(workspace_root: &Path, timeout: Duration) -> StepResult<HashMap<ModuleId, Vec<ModuleId>>>` and a private `fn parse_go_mod_graph(stdout: &str) -> HashMap<ModuleId, Vec<ModuleId>>`. Bodies = `unimplemented!()`. Depends on T003.

- [X] T008 **Promote `golang.rs` to a directory module** in a single commit: (a) `git mv mikebom-cli/src/scan_fs/package_db/golang.rs mikebom-cli/src/scan_fs/package_db/golang/legacy.rs` — body unchanged; (b) create `mikebom-cli/src/scan_fs/package_db/golang/mod.rs` declaring the submodules (`pub mod legacy; pub mod module_id; pub mod graph_resolver; pub mod goprivate; pub mod proxy_fetch; pub mod go_mod_graph;`) and re-exporting the public API (`pub use legacy::*; pub use graph_resolver::{GraphResolver, GraphResolverConfig, ModuleGraphMap, ResolutionStep}; pub use module_id::ModuleId;`); (c) update every external `use crate::scan_fs::package_db::golang::<Item>` import in the workspace — most should resolve unchanged because of the `pub use legacy::*` re-export, but verify by running `cargo +stable build -p mikebom`. **Why a single commit**: Rust's module system disallows `golang.rs` AND `golang/` as siblings, so the rename and the new submodule files must land together. Depends on T004–T007.

- [X] T009 Create `mikebom-cli/tests/go_transitive_edges.rs` integration-test scaffold — `#[tokio::test] async fn ladder_step3_only_argo_fixture() { todo!() }` body. Confirm `cargo +stable test -p mikebom --test go_transitive_edges` compiles (test will be `panic!("not yet implemented")` until T026). Depends on T001.

- [X] T010 Wire `--offline` plumbing through to the resolver call site. Locate the `mikebom sbom scan` codepath in `mikebom-cli/src/cli/scan_cmd.rs:583` (the `offline: bool` parameter) and confirm it threads to the Go ecosystem reader's entry function. Add a TODO marker at the call site noting "T024 will plumb this into `GraphResolver::resolve` via `WorkspaceContext::offline`." No behavior change in T010 — verification only.

**Checkpoint**: workspace builds, `cargo +stable check -p mikebom --all-targets` passes with no `unimplemented!()` reachable from any test or release path. Story phases may now begin.

---

## Phase 3: User Story 1 — Transitive edges without `go` and without cache (Priority: P1) 🎯 MVP

**Goal**: Per spec US1 — when `go` is not on PATH and `$GOMODCACHE` is empty, mikebom emits transitive `dependsOn` edges between every component sourced from `go.sum` via the proxy-fetch ladder step. Edges accurately reflect what `go build` would install.

**Independent Test**: Run `cargo +stable test -p mikebom --test go_transitive_edges -- --nocapture`. The test (T026) sets `PATH` to exclude `go`, sets `GOMODCACHE` to an empty `tempdir()`, points `$GOPROXY` at a `wiremock::MockServer`, and asserts the SBOM contains transitive edges for ≥ 90% of `go.sum` modules whose `.mod` files declare requires (per SC-001).

### Tests for User Story 1 (REQUIRED — FR-011, FR-012)

- [X] T011 [P] [US1] In `mikebom-cli/src/scan_fs/package_db/golang/proxy_fetch.rs::tests`, write unit tests for `escape_module_path()` covering Go's documented escape rules from R6: `github.com/Azure/azure-sdk-for-go` → `github.com/!azure/azure-sdk-for-go`, `gopkg.in/yaml.v2` → `gopkg.in/yaml.v2` (unchanged), `github.com/SAP/go-hdb` → `github.com/!s!a!p/go-hdb`, plus `Err(EscapeError::InvalidByte)` for module paths containing `?`, space, or non-ASCII bytes. Tests must FAIL (the impl is `unimplemented!()` until T015).

- [X] T012 [P] [US1] In `mikebom-cli/src/scan_fs/package_db/golang/goprivate.rs::tests`, write unit tests for `parse_private_patterns()` + `PrivatePatterns::matches()` per R5 — Go's `MatchPrefixPatterns` semantics. Cover: empty input → matches nothing; `github.com/our-org/*` matches `github.com/our-org/foo`, `github.com/our-org/foo/bar`, but NOT `github.com/other-org/foo`; `*.corp.example.com` matches `internal.corp.example.com` but NOT `corp.example.com.evil.com`; comma-separated multi-pattern; invalid pattern logs warn and produces non-matcher (does not panic). Tests must FAIL.

- [X] T013 [P] [US1] In `mikebom-cli/src/scan_fs/package_db/golang/goprivate.rs::tests`, write unit tests for `parse_proxy_chain()` per R7 — covering: `None`/empty → default chain `[Url(proxy.golang.org, ,), Direct]`; `"off"` → `Off` only; `"direct"` → `Direct` only; `"https://internal,direct"` → `[Url(internal, ,), Direct]`; `"https://a|https://b"` → both `Url` entries with pipe semantics; `"http://insecure"` parses but emits `tracing::warn` (capture log via `tracing-test` or assert on a side-channel); invalid URL returns `Err(ProxyParseError::InvalidUrl)`. Tests must FAIL.

- [X] T014 [US1] In `mikebom-cli/src/scan_fs/package_db/golang/proxy_fetch.rs::tests`, write a unit test for `fetch_module_mod()` using `wiremock::MockServer`: stub a 200 response with a synthetic `.mod` body, assert the function returns `StepResult::Ok(body)`; stub a 404 against a `,`-separated chain entry, assert fall-through to the next entry; stub a timeout (delay > total timeout), assert `StepResult::Failed(StepError { class: ErrorClass::Timeout, .. })`; stub HTML response (not a valid `.mod`), assert `StepResult::Failed` with `class: Parse`. Test must FAIL.

- [X] T015 [P] [US1] Implement `escape_module_path()` in `mikebom-cli/src/scan_fs/package_db/golang/proxy_fetch.rs` per R6: lowercase ASCII / digits / `.-_~/+` pass through; uppercase `X` → `!x`; other bytes → `Err(EscapeError::InvalidByte)`. Verify T011 passes.

- [X] T016 [P] [US1] Implement `parse_private_patterns()` + `PrivatePatterns::matches()` in `mikebom-cli/src/scan_fs/package_db/golang/goprivate.rs` per R5 — segment-wise glob with `*` matching any chars except `/`, comma-separated multi-pattern, fail-open on bad pattern with `tracing::warn`. Verify T012 passes.

- [X] T017 [P] [US1] Implement `parse_proxy_chain()` in `mikebom-cli/src/scan_fs/package_db/golang/goprivate.rs` per R7 — handle default, `off`, `direct`, comma/pipe-separated URL chains, http-warn, invalid URL → `Err`. Verify T013 passes.

- [X] T018 [P] [US1] Implement `WorkspaceContext::from_workspace(root: &Path, offline: bool) -> Result<WorkspaceContext, GraphResolverError>` in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs` — reads `go.sum` and `go.mod` from `root`, parses replaces/excludes via the existing `apply_replace_and_exclude` infra (now under `golang/legacy.rs::apply_replace_and_exclude`), reads `$GOMODCACHE` (with `$GOPATH/pkg/mod` and the OS default fallbacks), `$GOPROXY` (via `parse_proxy_chain`), `$GOPRIVATE` (via `parse_private_patterns`). Stores all in a `WorkspaceContext`. No tests needed beyond a smoke-build; integration test T026 covers behavior.

- [X] T019 [US1] Implement `build_proxy_url()` in `mikebom-cli/src/scan_fs/package_db/golang/proxy_fetch.rs` — constructs `<base>/<escaped_path>/@v/<escaped_version>.mod`, escaping path AND version per `escape_module_path` (the version uses the same escape function). Add unit tests covering: `(github.com/Azure/azure-sdk-for-go, v1.2.3)` against `https://proxy.golang.org` → `https://proxy.golang.org/github.com/!azure/azure-sdk-for-go/@v/v1.2.3.mod`. Depends on T015.

- [X] T020 [US1] Implement `fetch_module_mod()` async in `mikebom-cli/src/scan_fs/package_db/golang/proxy_fetch.rs` — walks the `proxy_chain` per R7 separator semantics, builds URL via T019, issues `client.get(url).timeout(...).send().await`, classifies errors per R14, returns `StepResult<String>`. Honors `GOPRIVATE` short-circuit (caller's responsibility — `fetch_module_mod` itself is unaware of GOPRIVATE; the orchestrator at T021 filters before calling). Verify T014 passes. Depends on T017, T019.

- [X] T021 [US1] Implement `step3_proxy_fetch_for_each_missing()` private fn in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs` — for each `ModuleId` in `ctx.go_sum_modules` not yet in `entries` AND not matched by `ctx.goprivate` AND `ctx.goproxy != Off` AND `!ctx.offline`: spawn an async task acquiring a permit from a `tokio::sync::Semaphore::new(16)`, call `fetch_module_mod`, parse the body via `parse_go_mod()` (existing parser at `golang/legacy.rs:159`), apply `ctx.replaces` to the module's requires, build a `ModuleGraphEntry { source: ResolutionStep::Proxy }`, insert into `entries`. Use `tokio::task::JoinSet` to await all tasks; on individual failures, increment `LadderSummary::fetch_errors[error_class]` and leave the entry absent (step 4 will fill it in). Depends on T016, T020.

- [X] T022 [US1] Implement `step2_cache_walk_for_each_missing()` private fn in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs` — wraps the existing `cache_lookup_depends()` logic from `golang/legacy.rs:570`, returning `StepResult<Vec<ModuleId>>` per `ModuleId`. For each `ModuleId` in `ctx.go_sum_modules` not yet in `entries`: try cache lookup; on success, build a `ModuleGraphEntry { source: ResolutionStep::GoModCache }` and insert. Note: this is a refactor — preserve 053's behavior bit-for-bit. The existing fn stays in `legacy.rs` as a private helper; new code wraps it.

- [X] T023 [US1] Implement `intersect_with_go_sum()` and `apply_replaces()` private fns in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs` per [contracts/resolver-api.md](contracts/resolver-api.md). Plus the step 4 fallthrough (insert empty `ModuleGraphEntry { source: ResolutionStep::None }` for any `ModuleId` in `ctx.go_sum_modules` still missing from `entries`; increment `LadderSummary::missing_count`).

- [X] T024 [US1] Implement `GraphResolver::resolve()` async in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs` — orchestrates the ladder for US1's no-`go` path: step 2 (T022) → step 3 (T021) → step 4 (T023 fallthrough) → intersect (T023) → apply_replaces (T023) → emit `tracing::info!` summary line per FR-009. Step 1 wiring is deferred to US2 (T031). Returns `Result<ModuleGraphMap, GraphResolverError>`. Depends on T021, T022, T023.

- [X] T025 [US1] Update `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs::read()` (formerly `golang.rs::read()` at line ~826) to: (a) build `WorkspaceContext` via `WorkspaceContext::from_workspace(root, offline)` once per scan, (b) call `GraphResolver::new(GraphResolverConfig::default()).resolve(&ctx).await?` once per scan, (c) populate each `PackageDbEntry::depends` field by `ModuleGraphMap::requires(&module_id)` lookup. Replace the per-entry call to the old `cache_lookup_depends()` (the new resolver subsumes it). Verify `cargo +stable test -p mikebom` passes for the unit-test layer.

- [X] T026 [US1] Create `tests/fixtures/go/argo-style-no-cache/proxy-mock/<escaped-mod>/<ver>.mod` files for every module in `tests/fixtures/go/argo-style-no-cache/argo-workflows/go.sum` (synthesized minimal `go.mod` content — `module <path>\n` plus `require` blocks chosen to produce ≥ 90% of `go.sum` modules with at least one outgoing edge per SC-001). Use a shell script in the same directory to (re)generate these from the actual upstream `.mod` files when the fixture is regenerated. Depends on T002.

- [X] T027 [US1] Implement the integration test in `mikebom-cli/tests/go_transitive_edges.rs` (FR-012) per quickstart.md and R11: start a `wiremock::MockServer`, register stubs for every `<escaped-mod>/@v/<ver>.mod` URL serving the synthesized files from T026, set environment (`PATH=/usr/bin:/bin`, `GOMODCACHE=<empty tempdir>`, `GOPROXY=<mock URI>`, `GOPRIVATE=""`), invoke the Go ecosystem reader against `tests/fixtures/go/argo-style-no-cache/argo-workflows/`, parse the resulting `PackageDbEntry`s, assert ≥ 90% of go.sum modules have at least one outgoing edge AND every emitted edge target is itself in `go.sum` (per FR-003 / SC-006) AND the FR-009 summary `tracing::info` line was emitted with `proxy:N>0` (capture tracing output via `tracing-subscriber::fmt::Layer` writing to a `Vec<u8>` buffer during the scan, post-scan-parse with regex on the buffer, assert non-zero proxy count — closes SC-007 for the proxy-step branch). Depends on T024, T025, T026.

- [X] T028 [US1] Add a unit test in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs::tests` covering the ladder fall-through (FR-011) — set up an in-memory `WorkspaceContext` with: empty cache dir, mock proxy returning 404 for every module, and assert the resolver returns a `ModuleGraphMap` where every entry has `ResolutionStep::None` and `LadderSummary::missing_count == |go.sum|`. Verifies graceful step-4 fallthrough.

**Checkpoint**: US1 is complete. The argo-style-no-cache fixture scans cleanly without `go` installed and without a populated `$GOMODCACHE`, emitting transitive edges via proxy fetch. `./scripts/pre-pr.sh` MUST pass.

---

## Phase 4: User Story 2 — `go mod graph` preferred path (Priority: P2)

**Goal**: Per spec US2 — when `go` IS on PATH, mikebom invokes `go mod graph` once and uses its output as the primary edge source. Output matches `go mod graph`'s edges (intersected with `go.sum`) with zero divergence on the committed fixtures.

**Independent Test**: On a host with `go` on PATH, run `cargo +stable test -p mikebom -- golang::go_mod_graph`. The unit suite asserts the parser correctly handles `go mod graph`'s line format AND a behavioral test asserts that when both step 1 and step 2 would succeed, step 1's data wins.

### Tests for User Story 2 (REQUIRED — FR-011)

- [X] T029 [P] [US2] In `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs::tests`, write unit tests for `parse_go_mod_graph()` covering: empty input → empty map; single-line `main-module child@v1.0.0` → map with one entry; multi-line input with whitespace variants; lines with no `@version` on the parent (main module) parsed correctly; malformed lines (1 field, 3+ fields) skipped with `tracing::debug` (assert via captured logs that we don't panic). Tests must FAIL initially.

- [X] T030 [P] [US2] In `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs::tests`, write unit test for `run_go_mod_graph()` — case: stub `go` binary via a small shell script in a `tempdir()` PATH that emits a known multi-line graph, assert `StepResult::Ok(map)` with the parsed contents; case: stub returns non-zero exit, assert `StepResult::Failed`; case: stub sleeps longer than the timeout, assert `StepResult::Failed { class: Timeout }`. (Use `tempfile` + `chmod +x` to create the stub.) Tests must FAIL initially.

### Implementation for User Story 2

- [X] T031 [P] [US2] Implement `parse_go_mod_graph()` in `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs` — per [contracts/resolver-api.md](contracts/resolver-api.md): split each line on whitespace, validate exactly 2 fields, split each field on `@` (parent's `@<version>` is optional — main module has none), construct `ModuleId`s, accumulate into `HashMap<ModuleId, Vec<ModuleId>>`. Verify T029 passes.

- [X] T032 [US2] Implement `run_go_mod_graph()` async in `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs` — uses `tokio::process::Command::new("go").args(["mod", "graph"]).current_dir(workspace_root)` wrapped in `tokio::time::timeout(timeout, ...)`. Probe `go` availability first via `Command::new("go").arg("version").output().await` (R4) — `Err(io::Error{kind:NotFound})` returns `StepResult::Unavailable` immediately. Non-zero exit, timeout, or unparseable stdout → `StepResult::Failed`. Otherwise → `StepResult::Ok(parse_go_mod_graph(stdout))`. Verify T030 passes. Depends on T031.

- [X] T033 [US2] Wire step 1 at the head of `GraphResolver::resolve()` in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs` per FR-002: BEFORE step 2, if `!ctx.offline`, call `run_go_mod_graph(&ctx.root_dir, config.go_mod_graph_timeout).await`; on `Ok(map)`, populate `entries` with `ResolutionStep::GoModGraph` for every parent in the map; then proceed to step 2 only for `ModuleId`s still missing. On `Unavailable` or `Failed`, fall through to step 2 directly with `tracing::warn` for `Failed`. Update the `LadderSummary` counters accordingly. Depends on T024 (resolve scaffold), T032.

- [ ] T034 [US2] Add a unit test in `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs::tests` covering "step 1 wins when present" — set up a context where step 1 mock returns a known map, step 2 cache mock returns DIFFERENT data, assert the resolver's output uses step 1's data and never invokes step 2 for the modules step 1 covered. Verifies precedence ordering per FR-002.

- [X] T035 [US2] Add an integration test in `mikebom-cli/tests/go_transitive_edges.rs` named `step1_matches_real_go_mod_graph_simple_module` (gated `#[cfg(feature = "real-go")]` or an env-based skip when `go` is not on PATH) — runs `go mod graph` against the existing `tests/fixtures/go/simple-module/` fixture, runs mikebom against the same fixture, asserts the edge sets match exactly after intersection with `go.sum` (per SC-002). Skip cleanly with a `tracing::warn` when `go` is unavailable so the test suite stays green on non-Go runners.

**Checkpoint**: US1 + US2 both functional. Local dev machines (where `go` is installed) get fast canonical resolution; CI runners without `go` still get full edges via the proxy path. `./scripts/pre-pr.sh` MUST pass.

---

## Phase 5: User Story 3 — Realistic-project regression assertion (Priority: P3)

**Goal**: Per spec US3 — the realistic-project CI job introduced in milestone 054 (which already scans knative/func) gains a transitive-edge-count assertion. A future regression that drops edge emission to zero fails CI with a clear diagnostic.

**Independent Test**: Trigger the realistic-project CI job in a PR. Verify it reports the per-fixture edge count and fails on a synthetic regression (e.g., a one-line change to `step3_proxy_fetch_for_each_missing` that returns empty).

- [ ] T036 [US3] Locate the milestone 054 realistic-project CI job (likely `.github/workflows/realistic-projects.yml` or an extension of `ci.yml`). Add a post-scan step that parses the resulting SBOM (CDX or SPDX, whichever the job already emits) and counts transitive edges among `pkg:golang` components. Assert ≥ 200 for `knative/func @ knative-v1.22.0` per SC-003. On assertion failure, log: `"Go transitive edges regressed: knative/func produced N edges (expected ≥ 200)."`

- [ ] T037 [US3] Document the expected edge-count threshold in `specs/055-go-transitive-edges/realistic-project-baseline.md` (small new file in the spec dir) — captures the SC-003 baseline, when it was measured, on which platform, and the upstream tag pinned for measurement. Future tag bumps (knative/func updates) update this file alongside the assertion threshold.

**Checkpoint**: All three stories functional. The CI matrix protects against regressions.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T038 [P] Update `specs/055-go-transitive-edges/checklists/requirements.md` with implementation evidence — for each FR, link the implementing task ID and any commit SHA.

- [X] T039 [P] Run `./scripts/pre-pr.sh` end-to-end and capture full output. Both `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace` MUST report clean. Per CLAUDE.md, NEVER cite a passing per-crate `cargo test -p mikebom` as evidence of CI-readiness — only the workspace-wide commands count. Paste the full output at the bottom of the PR description. **Also confirms SC-004 (no perf regression) via T043's `go_resolver_no_regression` test running as part of the workspace test suite, and SC-005 (offline → no network) via T044's `offline_makes_no_network_calls` integration test.**

- [X] T040 [P] Update `docs/design-notes.md` (if it exists per the project-wide reference convention) with a short "Go transitive resolver" section describing the 4-step ladder + capability matrix from spec.md. Cross-link the data-model + contracts. Skip if `docs/design-notes.md` doesn't exist.

- [ ] T041 [P] Manual smoke check via quickstart.md — exercise each ladder step (step 1, step 2, step 3, step 4) against the `simple-module/` and `argo-style-no-cache/` fixtures and verify the FR-009 summary line matches expectations per the quickstart's "Common debugging hooks" matrix.

- [X] T042 Verify the `agent-context` update by `update-agent-context.sh` from the plan phase — confirm `CLAUDE.md` has a milestone-055 entry under "Recent Changes" and that no pre-existing entries were clobbered.

- [X] T043 Add SC-004 perf-regression smoke check. In `tests/dual_format_perf.rs`, add `#[test] fn go_resolver_no_regression()` that scans `tests/fixtures/go/simple-module/` and `tests/fixtures/go/argo-style-no-cache/argo-workflows/` 5 times each with `--offline` (avoids any network variance), captures the median wall-clock per fixture, and asserts the median is ≤ 1.15 × a baseline constant committed in the test source. Baseline constants are measured on main HEAD immediately before 055 lands and recorded in the PR description. Per milestone 054's noise-allowance convention, the threshold becomes 2.0 × on macOS-latest (gate via `cfg!(target_os = "macos")`). Smoke check only — not a full benchmark; full perf tracking is out of scope for 055.

- [X] T044 [P] Add SC-005 offline-no-network verification. In `mikebom-cli/tests/go_transitive_edges.rs`, add `#[tokio::test] async fn offline_makes_no_network_calls()` that: (a) starts a `wiremock::MockServer` and registers a single catch-all `Mock::given(any()).respond_with(ResponseTemplate::new(500))` that the test asserts is NEVER hit; (b) sets the test's `WorkspaceContext.goproxy` to point at the mock URL; (c) runs `GraphResolver::resolve(&ctx)` with `ctx.offline = true` against the `argo-style-no-cache/argo-workflows/` fixture; (d) at scan end, calls `mock_server.received_requests().await.expect("never None").len()` and asserts `== 0`. Cross-platform — no Linux-only `unshare` needed. Verifies FR-005 + SC-005 at the resolver layer.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001, T002 — independent, can run in parallel.
- **Foundational (Phase 2)**: T003 → {T004, T005, T006, T007} → T008 → {T009, T010}. Blocks all stories.
- **US1 (Phase 3)**: depends on Foundational. T011–T013 (tests) are independent of each other; T014 depends on the proxy_fetch impl (T015 → T019 → T020). T015–T017 are parallel (different files). T018–T024 are largely sequential within `graph_resolver.rs`. T025 depends on T024. T026 depends on T002. T027 depends on T024 + T025 + T026.
- **US2 (Phase 4)**: depends on Foundational + US1 task T024 (resolver scaffold). T029–T034 are intra-`go_mod_graph.rs` and `graph_resolver.rs`.
- **US3 (Phase 5)**: depends on US1 (so the realistic-project SBOM contains transitive edges to count). T036, T037 are independent.
- **Polish (Phase 6)**: depends on all desired stories.

### Within-Story Dependencies

**US1**:
- Tests-first order per CLAUDE.md project convention (write FAILING test, implement, verify GREEN):
  - T011 (test) → T015 (impl)
  - T012 (test) → T016 (impl)
  - T013 (test) → T017 (impl)
  - T014 (test) → T020 (impl)
- Type/scaffold dependencies:
  - T015, T019, T020 all in `proxy_fetch.rs` — sequential within the file
  - T016, T017 both in `goprivate.rs` — sequential
  - T018, T021–T024 all in `graph_resolver.rs` — sequential
- Integration dependency chain: T024 → T025 → T027

**US2**:
- T029, T030 (tests) → T031, T032 (impl) → T033 (wire) → T034 (test wire)
- T035 is end-to-end and depends on T033

### Parallel Opportunities

- **Phase 1**: T001 ∥ T002
- **Phase 2 after T003**: T005 ∥ T006 ∥ T007 (different files)
- **Phase 3 tests-write phase**: T011 ∥ T012 ∥ T013 (different files); T014 sequenced after T015/T019 because it tests them
- **Phase 3 impl phase across files**: T015 ∥ T016 ∥ T017 (different files); within each file, sequential
- **Phase 4 tests phase**: T029 ∥ T030 (same file but no shared state — can sequence by writer convention)
- **Phase 6**: T038 ∥ T039 ∥ T040 ∥ T041 ∥ T044 (independent verifications); T043 not [P] because it edits `tests/dual_format_perf.rs` which is also touched by milestone 054 baseline updates — sequence behind those if any are in flight

---

## Parallel Example: Phase 2 Foundational

```bash
# After T003 (ModuleId) is committed:
Task: "T005 — Create golang/goprivate.rs scaffold"
Task: "T006 — Create golang/proxy_fetch.rs scaffold"
Task: "T007 — Create golang/go_mod_graph.rs scaffold"
# All three are different files; they all need ModuleId from T003 but nothing else.
# After all three land, T008 (mod.rs declarations) commits next.
```

## Parallel Example: US1 implementations

```bash
# After Foundational (T010) completes:
Task: "T011 — escape_module_path tests in proxy_fetch.rs"
Task: "T012 — parse_private_patterns tests in goprivate.rs"
Task: "T013 — parse_proxy_chain tests in goprivate.rs (different #[test] fns)"
# Then implement each:
Task: "T015 — escape_module_path impl in proxy_fetch.rs"
Task: "T016 — parse_private_patterns impl in goprivate.rs"
Task: "T017 — parse_proxy_chain impl in goprivate.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Complete Phase 1 (Setup) — T001, T002.
2. Complete Phase 2 (Foundational) — T003–T010. **Critical gate**: workspace builds clean; `cargo +stable check -p mikebom --all-targets` zero warnings.
3. Complete Phase 3 (US1) — T011–T028. Run `./scripts/pre-pr.sh` after each test→impl pair.
4. **STOP and VALIDATE**: integration test T027 passes. `tracing::info` ladder summary observably emits `proxy:N>0` on the argo-style-no-cache fixture. Open a draft PR.
5. This is shippable on its own — closes the issue #102 residual gap.

### Incremental Delivery

1. Setup + Foundational → Foundation ready (workspace builds, types in place)
2. US1 → Ship MVP (offline transitive edges via proxy, the headline)
3. US2 → Ship faster local-dev path (`go mod graph`)
4. US3 → Ship CI regression backstop
5. Each story is independently mergeable; reviewers can land them as separate PRs if desired (or one bundled PR — author's call).

### Parallel Team Strategy (if multiple contributors)

After Phase 2 (Foundational):
- Contributor A: US1 (the meat of the work; T011–T028)
- Contributor B: US2 once US1's T024 (resolver orchestration scaffold) lands; T029–T035 are otherwise independent
- Contributor C: US3 (T036, T037) — can start immediately after US1 lands because it only consumes the SBOM

---

## Notes

- **Pre-PR gate**: ALWAYS run `./scripts/pre-pr.sh` (clippy + test, both workspace-wide) before any push. Both must report clean. CLAUDE.md project file makes this mandatory; the constitution Pre-PR Verification table v1.4.0 makes it constitutional.
- **No `.unwrap()` in production code**: Constitution Principle IV. Test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the `mikebom-cli/src/trace/` convention.
- **No new `mikebom:*` properties**: Constitution Principle V; spec FR-010. The `dependsOn` channel in CDX / SPDX 2.3 / SPDX 3 is the native construct.
- **No on-disk cache** for fetched `.mod` files: spec Q3 clarification — in-memory per-scan only.
- **No CLI flag** for fetch concurrency or proxy-fetch toggle: spec Q1 (default-on with `--offline`) + Q2 (fixed 16) clarifications. Resist the temptation to add knobs prematurely.
- **wiremock is a dev-dep, not a workspace dep**: keeps blast radius minimal per plan Complexity Tracking row 2. If a reviewer rejects the dev-dep, fall back to a hand-rolled `tokio::net::TcpListener` mock (~150 LOC of test infra).
- **Foundational refactor in T008** (renaming `golang.rs` → `golang/legacy.rs`) is the riskiest refactor in the plan. Land it as a standalone PR if reviewer prefers, then layer the rest.
