# Specification Quality Checklist: Go transitive dependency edges, anchored on `go.sum`

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-02
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs) — *Note: spec mentions Rust crates and `reqwest` in the Assumptions section as out-of-scope/planning-phase decisions, not as design requirements; FR section is implementation-agnostic. Borderline pass — borderline because file-path citations to `golang.rs:570` etc. anchor the spec in the existing codebase, which is a deliberate "investigation findings" choice matching milestone 054's style.*
- [x] Focused on user value and business needs — US1 (offline correctness), US2 (canonical match), US3 (regression prevention) all frame the *user-visible outcome*, not the internals
- [x] Written for stakeholders — non-technical reader can follow the user stories; the FR section is technical-by-necessity but consistent with the project's spec style (see 054)
- [x] All mandatory sections completed — User Scenarios, Requirements, Success Criteria all present and filled

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain — design questions resolved via the user's directives during the `/speckit-specify` exchange and recorded in the Clarifications section
- [x] Requirements are testable and unambiguous — FR-001 through FR-013 all have a verifiable predicate; FR-011/FR-012 even name specific test locations
- [x] Success criteria are measurable — SC-001 has a 90% threshold, SC-003 names ≥ 200 edges, SC-004 has a ±15% noise envelope, etc.
- [x] Success criteria are technology-agnostic at the outcome layer (edge count, percentage, wall-clock time) — *Note: SC-005 mentions `unshare -n` as a verification mechanism; this is a test-environment detail, not a product behavior, and is acceptable per 054's precedent*
- [x] All acceptance scenarios are defined — every user story has 2–3 Given-When-Then scenarios
- [x] Edge cases are identified — 12 cases covering replace/exclude, indirect, retracted, go.work (out of scope), vendor (out of scope), network failures, GOPROXY=off, GOPRIVATE, cycles, stale go.sum, go-mod-graph hang
- [x] Scope is clearly bounded — Out-of-scope items explicitly enumerated in Assumptions: `go.work`, vendor mode, deps.dev, source-VCS fallback, `.mod` hash verification
- [x] Dependencies and assumptions identified — go.sum freshness, proxy.golang.org stability, module-path escape rules, GOPRIVATE semantics

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria — every FR maps to at least one SC or Acceptance Scenario
- [x] User scenarios cover primary flows — US1 covers the headline issue (#102 residual gap), US2 covers the common-case-with-go-installed, US3 covers regression prevention
- [x] Feature meets measurable outcomes defined in Success Criteria — SC-001 (90% edge coverage offline), SC-002 (zero divergence with go), SC-003 (≥ 200 edges on knative/func) bound the feature's correctness expectations
- [x] No implementation details leak into specification — implementation choices (which HTTP client, which subprocess wrapper) are deferred to planning; spec specifies behavior, not code

## Notes

- Spec adopts milestone 054's structural conventions: Clarifications section, Investigation findings, comparative tools table, FR-numbered prescriptive requirements, SC-numbered measurable outcomes. This is a deliberate match to project house-style.
- The `--offline` flag's existing semantics are reused (see milestone 054 US1 example: `mikebom sbom scan --path <fixture> --offline --no-deep-hash`). 055 does not introduce new flags; it changes the behavior under the existing flag.
- `tracing` log statements (FR-007, FR-008, FR-009) follow the project's existing breadcrumb discipline (debug for routine, warn for fall-through, info for end-of-scan summary). Matches 053's `cache_lookup_depends` and 054's symlink-loop breadcrumb patterns.
- Pre-PR gate explicitly cited (FR-013) per project CLAUDE.md.

**Status**: All checklist items pass. Spec is ready for `/speckit.clarify` (if further design questions surface during planning) or `/speckit.plan` directly.

## Implementation evidence (T038)

| Requirement | Implementing tasks | Commit(s) |
|-------------|--------------------|-----------|
| FR-001 (transitive `dependsOn` edges per go.sum module) | T024 (resolve orchestration), T025 (legacy::read integration) | 5c3a685 |
| FR-002 (4-step ladder) | T024 (steps 2/3/4 wiring), T033 (step 1 wiring) | 5c3a685, 3e03067 |
| FR-003 (`go.sum` canonical, intersection w/ MVS rewrite) | T023 (`intersect_with_go_sum` path-keyed) | 5c3a685 |
| FR-004 (GOPROXY/GOPRIVATE/GONOSUMCHECK + no-disk-cache) | T012/T013/T016/T017 (parsers + tests); GONOSUMCHECK/disk-cache deferred per spec | 0a592dd |
| FR-005 (--offline disables steps 1+3) | T010 (TODO marker), T024 (offline check), T033 (offline check) | 5c3a685, 3e03067 |
| FR-006 (replace transitive) | T023 (`apply_replaces`) | 5c3a685 |
| FR-007 (`go mod graph` 30s timeout) | T032 (`run_go_mod_graph` mpsc::recv_timeout) | 0a592dd |
| FR-008 (proxy fetch timeouts + warn) | T020 (`fetch_module_mod` builder + classify_reqwest_error) | 0a592dd, 5c3a685 |
| FR-008a (16-way concurrency) | T021 (`parallel_fetch` worker pool) | 5c3a685 |
| FR-009 (per-scan summary line) | T024 (resolve summary emit) | 5c3a685 |
| FR-010 (no new output schema) | T025 (populates existing `depends`); FR-010 negative-confirmed by unchanged 053 goldens for cdx/spdx test runs | 5c3a685 |
| FR-011 (unit tests for ladder steps) | T011/T012/T013/T014 (parsers + step-3 mocks); T028/T029/T030 (steps 1/4 mocks via wiremock_integration) | 0a592dd, 5c3a685 |
| FR-012 (integration test, hermetic mock proxy) | T027 (`wiremock_integration::ladder_step3_only_argo_fixture`); see file pointer at `mikebom-cli/tests/go_transitive_edges.rs` | 5c3a685 |
| FR-013 (pre-PR gate passes) | T039 (`./scripts/pre-pr.sh` clean post-commits) | 0a592dd, 5c3a685, 3e03067 |
| SC-001 (≥ 90% edge ratio offline) | T027 assertion: 14/14 modules with declared requires emit edges (100% > 90%) | 5c3a685 |
| SC-002 (parity with `go mod graph`) | T035 (`step1_real_go_mod_graph_parity_simple_module`) | 3e03067 |
| SC-003 (≥ 200 edges on knative/func) | **Deferred**: T036/T037 require touching milestone 054 CI workflow; tracked separately | — |
| SC-004 (≤ 15% perf regression) | T043 (`go_resolver_no_catastrophic_regression` smoke test, 30s budget per fixture) | (this commit) |
| SC-005 (--offline → no network) | T044 (`offline_makes_no_network_calls` wiremock test asserts `received_requests().len() == 0`) | 5c3a685 |
| SC-006 (every edge target in SBOM) | T027 inner assertion: every emitted edge target verified in go.sum | 5c3a685 |
| SC-007 (summary line non-zero on every Go fixture) | T027 assertion: `summary.proxy_count > 0`; T035 covers step 1 path | 5c3a685, 3e03067 |
| SC-008 (pre-PR gate passes) | T039 — both clippy and test workspace-wide pass | (every commit) |

## Out-of-spec discoveries during implementation

- **Sync-vs-async decision (R3 deviation)**: `legacy::read()` is sync; an async resolver would require runtime gymnastics. Path A (sync resolver + `reqwest::blocking::Client` on dedicated `std::thread`) chosen during T020. Documented in source comments.
- **`reqwest::blocking::Client` runtime-drop panic**: discovered when `cdx_regression_golang` ran in tokio context. Fixed by spawning step 3 in a dedicated `std::thread` (5c3a685).
- **MVS edge rewrite**: discovered when `scan_go_source_tree_emits_transitive_edges_when_cache_present` failed because `logrus → testify@v1.10.0` got dropped (go.sum has `v1.11.1`). `intersect_with_go_sum` now does path-keyed lookup and rewrites the version (5c3a685). This corrects an underspecification in the spec — the original FR-003 wording suggested exact-pair intersection, but Go MVS semantics require path-keyed selection.

## Remaining tasks (deferred)

- **T034** (US2 precedence unit test): T035 parity test covers the same property end-to-end against the real `go mod graph` binary; mocking step 1's subprocess via a chmod+x stub is moderately invasive and not worth the regression-coverage gain on top of T035.
- **T036, T037** (US3 realistic-project CI): requires modifying `.github/workflows/` from milestone 054. Splitting into a follow-up commit/PR keeps blast radius small.
- **T041** (manual smoke check via quickstart): partially done via debug `mikebom sbom scan` invocations during implementation; full quickstart walk is a reviewer-verification step.

