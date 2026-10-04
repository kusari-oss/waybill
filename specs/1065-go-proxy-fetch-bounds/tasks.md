# Tasks: Bound the Go proxy-fetch tier

**Input**: `specs/1065-go-proxy-fetch-bounds/` (spec.md, plan.md, research.md, data-model.md, contracts/coverage-reason.md, quickstart.md, measurements/)

**Tests**: requested. Spec SC-001…SC-005 and US1–US3 independent tests define them. Write each test first and confirm it fails before implementing.

Paths are relative to the repository root. `golang/` means `waybill-cli/src/scan_fs/package_db/golang/`.

## Phase 1: Setup

- [X] T001 Confirm the baseline: run `cargo test -p waybill --bin waybill golang::` and `cargo test -p waybill --test go_transitive_coverage --test go_transitive_edges`. Record that all pass before changes. Note in tasks.md if anything is already red.

**Implementation notes (deviations recorded):**
- **New file:** the per-scan state, budget and outcome live in a new `golang/proxy_bounds.rs` rather than in the 2,300-line `graph_resolver.rs`. The tests for them sit in `proxy_bounds.rs` and in a `m1065_bounds_tests` module in `graph_resolver.rs`.
- **Fetch-path code written with the foundation:** T011, T012, T017 and T018 were written alongside Phase 2 because they touch the same functions. To keep test-first meaningful, each bound-enforcement test was then shown to fail with the bounds switched off: unit 007a/007b/009/014/015 and binary T010/T010e/T016 fail, while the "must not engage" tests pass either way.
- **Half-open hold (research R2 correction):** the first after-measurement showed a dead proxy cost two timeouts (20.3 s for 64 modules), and T007(b)'s bound (1.2 s) was loose enough to allow that. T007(b) was tightened to 0.75 s and ≤ 16 requests, which reproduced the problem; the hold fixed it.
- **Skips are silent per module:** a module refused only by tripped entries, or not admitted by the budget, returns `Unavailable` and gets no per-module "fetch failed" warning. Its outcome is in the one FR-008 warning per bound.
- **Reporting only when something was lost:** the C110/C111 contribution appears only when a bound left at least one module to go.sum (FR-006 amended). The warning appears for every tripped entry.
- **T010(e) narrowed to this feature's outputs:** the documents and the FR-008 warning. Two pre-existing log lines print the raw `GOPROXY` URL (#1110; the second site was found by this test and added to the issue).

## Phase 2: Foundational (shared by US1 and US2)

**Purpose**: the per-scan state and the reporting path. Both bounds use them; neither can be tested without them.

- [X] T002 Add `entry_label(url: &url::Url) -> String` in `golang/proxy_fetch.rs` returning `scheme://host[:port]` (port only when explicit). Never include userinfo, path, query or fragment (research R6). Add unit tests: plain URL; URL with `user:secret@`, asserting `secret` and `user` are absent; URL with path `/goproxy/`; explicit port.
- [X] T003 Add the per-scan state types in `golang/graph_resolver.rs` per data-model.md:
  - **`EntryHealth`:** `responded: bool`, `consecutive_network_failures: usize`, `tripped: Option<ErrorClass>`, `affected: usize`, `label: String`.
  - **`ProxyFetchBudget`:** `started: Option<Instant>` set on the first request, `budget: Duration`, `exhausted: bool`, `not_attempted: usize`.
  - **`ProxyFetchBounds`:** `Mutex`-guarded, holding `Vec<EntryHealth>` indexed by chain position plus the budget. Shared via `Arc` so the 16 worker threads can update it.
  - **Placement:** put `ProxyFetchBounds` on `GraphResolver`, so one instance spans every workspace of a scan (R4).
- [X] T004 Add `proxy_fetch_budget: Duration` to `GraphResolverConfig` in `golang/graph_resolver.rs`. The default is 60 s (R3); the test-only override is `WAYBILL_GO_PROXY_FETCH_BUDGET_MS`, read the same way as `mod_why.rs::BudgetTracker::from_env`. Extend the existing `GraphResolverConfig::default()` test (the one asserting `fetch_concurrency == 16`) to assert 60 s.
- [X] T005 Add `BoundOutcome` and `GraphResolver::bound_outcome(&self) -> BoundOutcome` in `golang/graph_resolver.rs`:
  - **Breaker:** one entry per tripped chain entry, in chain order, carrying `(label, class, affected)`.
  - **Budget:** `Some((budget_secs, not_attempted))` iff exhausted.
  - **`BoundOutcome::coverage(&self) -> Option<GoTransitiveCoverage>`:** emits exactly the contract strings in `contracts/coverage-reason.md`. Breaker → `Unknown`, budget only → `Partial`, both → `Unknown` with breaker fragments first joined by `; `.
  - **Tests:** unit tests for the string forms against the contract examples.
- [X] T006 In `golang/legacy.rs`, after the per-workspace `resolver.resolve` loop (around `legacy.rs:1915-1960`), merge `resolver.bound_outcome().coverage()` into `signals.go_transitive_coverage` using the existing `merge_coverage`. When none exists yet (`None`), use it directly. With no bound tripped, `coverage()` returns `None` and nothing changes (FR-009).

**Checkpoint**: compiles; all existing tests still pass; nothing trips yet because nothing sets the state.

## Phase 3: User Story 1 — Dead or hanging proxy does not stall the scan (P1) 🎯 MVP

**Goal**: a chain entry that never answers trips after one batch, and later modules skip it.

**Independent test**: `GOPROXY` at a closed local port with 64 go.sum modules. Proxy-step time is about one batch; C110 is `unknown`; C111 starts with `proxy-unreachable:`.

### Tests for User Story 1

- [X] T007 [P] [US1] Unit tests in `golang/graph_resolver.rs` (test module guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`). Each drives `parallel_fetch` through the resolver with a `ProxyChain` and a small `GraphResolverConfig`.
  - **(a) Closed port:** bind a `TcpListener` to `127.0.0.1:0`, take the port, drop it. With 64 targets, the entry trips, `affected == 64`, and fewer than 64 connection attempts are made. Count attempts via a test-visible counter on `ProxyFetchBounds`.
  - **(b) Hang:** a listener that accepts and never writes, with `fetch_total_timeout = 300ms` and 48 targets. Elapsed time is under 2 × 300 ms plus slack, not 3 × 300 ms.
  - **(c) Any response keeps the entry reachable:** a wiremock server answering 404 to everything never trips the breaker, and every module is attempted. Repeat with 503 for the 5xx edge case.
  - **(d) Mixed:** wiremock answers the first request with 200 and then delays past the timeout. It never trips (`responded == true`).
- [X] T008 [P] [US1] Unit test in `golang/graph_resolver.rs` for a `|` chain: the first entry is a closed port, the second is wiremock returning valid `.mod` bodies. The first trips, every module resolves via the second entry, and no C111 fragment names the second. For a `,` chain with the same entries, the trip ends fetching and modules fall to go.sum (FR-002).
- [X] T009 [P] [US1] Unit test in `golang/graph_resolver.rs`: a two-workspace scan through one `GraphResolver`. Workspace 1 has 20 modules against a closed port and trips; workspace 2 has 30 modules and makes zero connection attempts. `affected == 50` (spec edge case: per scan, not per workspace).
- [X] T010 [US1] Binary-level test, new file `waybill-cli/tests/go_proxy_fetch_bounds.rs`. Follow `go_transitive_coverage.rs`'s harness: set `WAYBILL_NO_GO_MOD_WHY=1`, use `--no-deps-dev`, put a `PATH` without `go`, use an empty `GOMODCACHE` and `HOME` in a tempdir, and set `GOPROXY=http://127.0.0.1:<closed-port>`. The synthetic repo has 40 go.sum modules (generator as in `measurements/probe_e2e.sh`). Assert:
  - **(a)** the scan succeeds;
  - **(b)** CycloneDX, SPDX 2.3 and SPDX 3 all carry C110 `unknown` and C111 equal to `proxy-unreachable: http://127.0.0.1:<port> failed at the network level (connection); 40 modules resolved from go.sum only`;
  - **(c)** all 40 modules are components with `waybill:go-transitive-source = go-sum-fallback`;
  - **(d)** no new per-component annotation exists (FR-005): the property-name set on Go components matches a no-proxy (`GOPROXY=off`) scan of the same repo;
  - **(e) no credentials:** a second run with `GOPROXY=http://user:secret@127.0.0.1:<closed-port>`. `secret` and `user` appear in neither stderr nor any of the three documents, and C111 names `http://127.0.0.1:<port>` (FR-006, FR-008).

### Implementation for User Story 1

- [X] T011 [US1] Change `fetch_module_mod` in `golang/proxy_fetch.rs` to take `&ProxyFetchBounds` (or a narrow trait / closure pair to keep it testable).
  - **Before each `ProxyEntry::Url` request:** if the entry at this index is tripped, skip sending and treat it as a network failure of its `tripped` class, then follow the existing fall-through rules (`|` continues, `,` returns `Failed`). Increment `affected`.
  - **After a response (any status):** set `responded = true`.
  - **After a network-level error** (`Connection | Timeout | Dns | Tls` from `classify_reqwest_error`): if the entry has not responded, increment `consecutive_network_failures`, add 1 to `affected`, and trip when the count reaches `fetch_concurrency` (R2).
- [X] T012 [US1] Thread the bounds through the ladder in `golang/graph_resolver.rs`: `step3_proxy_fetch` → `parallel_fetch` → workers, each worker cloning the `Arc`.
  - **At trip time:** emit only `tracing::debug!`.
  - **FR-008 warning:** in `golang/legacy.rs` after the workspace loop (T006's site), emit one warning per tripped entry from `bound_outcome()`: `tracing::warn!(proxy = %label, class, modules = affected, "Go module proxy unreachable; skipped it for the rest of the scan and used go.sum for these modules")`. Use the label only, never the URL (FR-006, FR-008).
- [X] T013 [US1] Run T007–T010 until green. Then run quickstart §1 and §2 with `measurements/probe_e2e.sh` against `10.255.255.1` and the hang server. Record the 16-vs-64 wall times in `measurements/after.txt` and check them against SC-001/SC-002 (ratio ≤ 1.5).

**Checkpoint**: US1 complete; a dead proxy costs one batch.

## Phase 4: User Story 2 — Many missing modules have a time limit (P2)

**Goal**: no new proxy request starts after the per-scan budget. The document records how many modules were not attempted.

**Independent test**: with `WAYBILL_GO_PROXY_FETCH_BUDGET_MS` small and a slow-404 proxy, the step ends near the budget, C110 is `partial`, and C111 has `proxy-fetch-budget-exhausted:`.

### Tests for User Story 2

- [X] T014 [P] [US2] Unit test in `golang/graph_resolver.rs`: wiremock answers 404 after a 200 ms delay; budget 500 ms; 160 targets. Assert:
  - fewer than 160 attempts;
  - `not_attempted > 0`;
  - elapsed < 500 ms + `fetch_total_timeout` + slack;
  - `BoundOutcome::coverage()` is `Partial` with the contract string.
- [X] T015 [P] [US2] Unit test in `golang/graph_resolver.rs`: both bounds. A `|` chain of a closed port followed by a slow 404 server, with a small budget. Coverage is `Unknown`, and the reason has the breaker fragment first, then the budget fragment (contract example 2).
- [X] T016 [US2] Binary-level test in `waybill-cli/tests/go_proxy_fetch_bounds.rs`: a local wiremock-equivalent slow-404 server. Use a `std::net::TcpListener` thread that writes `HTTP/1.1 404` after 200 ms, so no async runtime is needed in an integration test. Set `WAYBILL_GO_PROXY_FETCH_BUDGET_MS=500` and use 160 modules. Assert C110 `partial` and that the C111 prefix is identical in all three formats.

### Implementation for User Story 2

- [X] T017 [US2] In `parallel_fetch` / the worker loop in `golang/graph_resolver.rs`, before each request:
  - set `budget.started` if it is `None`;
  - if the elapsed time is at or past the budget, mark `exhausted`, increment `not_attempted`, and return the module as a failed fetch without sending. It then falls to go.sum like any failure (FR-005);
  - never interrupt requests already in flight (FR-003).
- [X] T018 [US2] In `golang/legacy.rs` at T012's site, emit the FR-008 budget warning once: `tracing::warn!(budget_secs, modules = not_attempted, "Go proxy-fetch budget exhausted; remaining modules used go.sum")`.
- [X] T019 [US2] Run T014–T016 until green. Run quickstart §3 and record the result in `measurements/after.txt` (SC-003).

## Phase 5: User Story 3 — Healthy scans unchanged (P1)

**Goal**: byte-identity when nothing trips (FR-009, SC-005).

- [X] T020 [P] [US3] Unit test in `golang/graph_resolver.rs`: wiremock serves a valid `.mod` (one `require` each) for every target. Assert:
  - `bound_outcome().coverage()` is `None`;
  - every module's entry has `source = Proxy` and exactly the `require` its body declared, compared against an expected map built from the bodies.
- [X] T021 [US3] Binary-level test in `waybill-cli/tests/go_proxy_fetch_bounds.rs` for FR-010. A local server answers 404 to every module (no delay), with 40 modules and the default budget. Assert C110 `complete`, `waybill:go-transitive-fallback-count = 40`, and no C111, in all three formats. This is today's behaviour, kept.
- [X] T022 [US3] Binary-level test in `waybill-cli/tests/go_proxy_fetch_bounds.rs`: `--no-go-proxy-fetch` with `GOPROXY` at a closed port gives no C111 and no breaker warning on stderr. The bounds only govern fetches that would happen (spec edge case).
- [X] T023 [US3] Run quickstart §4 (`measurements/probe_healthy_real.sh`). Expect `failed-fetches=0`, `proxy-fetch=197` and no C111. Record in `measurements/after.txt`.

## Phase 6: Polish & Cross-Cutting

- [X] T024 Update C111's row in `docs/reference/sbom-format-mapping.md`: add `proxy-unreachable` and `proxy-fetch-budget-exhausted` to the code vocabulary with their C110 values (contract), and note that entry labels never include credentials. Reference m1065 (#853).
- [X] T025 [P] Add an Unreleased entry to `CHANGELOG.md`. Cover the breaker, the 60 s budget, the two C111 codes, and the end of a dead proxy reporting `complete`. Note the test-only env var is not an operator setting.
- [X] T026 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo EXIT=$?`. Require `EXIT=0`, the `>>> all pre-PR checks passed.` line, and no `test result` line without ` 0 failed`.
- [ ] T027 Push. Dispatch a read-only public-corpus run (`gh workflow run "Public corpus regression" -f branch=1065-go-proxy-fetch-bounds`). Watch it **by run ID**, since dispatched runs are recorded under `main`. Expect every target to pass with no golden change (SC-005, R7). If any golden changes, stop and attribute it before regenerating.
- [ ] T028 Open the PR (closes #853; references #1110). After merge, run `cargo clean`.

## Dependencies & Execution Order

- **Phase order:** Phase 1 → Phase 2 (T002–T006) → US1 (Phase 3) → US2 (Phase 4) → US3 (Phase 5) → Polish.
- **Why US2 follows US1:** both edit the same worker loop (T011/T012 and T017), so they are sequential.
- **US3** can run any time after Phase 2, and must run again after US2.
- **Within each story,** tests come first and must fail before the implementation tasks.

### Parallel opportunities

- **Phase 2:** T002 (`proxy_fetch.rs`) ∥ T003–T005 (`graph_resolver.rs`); T006 after T005.
- **US1:** T007, T008 and T009 are separate test functions in one file. Write them together, then T010 in its own file.
- **US2:** T014 ∥ T015; T016 in the separate integration file.
- **Polish:** T024 ∥ T025.

## Implementation Strategy

- **MVP = Phase 2 + US1:** removes the minutes-long cases on its own.
- **US2** adds the budget on top.
- **Ship:** both together in one PR (plan: Delivery), because they share the state and the C111 change.
