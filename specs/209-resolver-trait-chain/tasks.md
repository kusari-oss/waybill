# Tasks: Resolver Trait + Chain Refactor (m209)

**Branch**: `209-resolver-trait-chain`
**Feature**: [spec.md](./spec.md) | [plan.md](./plan.md)

## Task Format

Each task follows: `- [ ] T### [P?] [Story?] Description with file path`. `[P]` = parallelizable (no dependency on incomplete sibling task; different file). `[US#]` = maps to a user story from spec.md (US1 = add-ecosystem ergonomics; US2 = per-resolver test isolation; US3 = technique-signal preservation).

## Phase 1: Setup

- [X] T001 Create `mikebom-cli/src/resolve/resolvers/` subdirectory with an empty `mod.rs` (top-of-file doc-comment naming milestone 209 + the resolvers-live-here purpose from plan.md's Project Structure section)
- [X] T002 Create scaffold files at `mikebom-cli/src/resolve/resolver_trait.rs` + `mikebom-cli/src/resolve/resolver_chain.rs` (each with a top-of-file doc-comment naming milestone 209 + the file's purpose)
- [X] T003 Register the new `resolver_trait` + `resolver_chain` + `resolvers` submodules in `mikebom-cli/src/resolve/mod.rs` (add three `mod` declarations in alphabetical position)

## Phase 2: Foundational (blocking prerequisites)

- [X] T004 [P] Define `ResolverError` enum in `mikebom-cli/src/resolve/resolver_trait.rs` per data-model E4 — 3 variants (`Transient { resolver, source }`, `MalformedInput { resolver, reason }`, `Unavailable { resolver, reason }`) with `#[derive(Debug, thiserror::Error)]` + `#[error(...)]` messages per data-model
- [X] T005 [P] Define `ResolveInput<'a>` enum in `mikebom-cli/src/resolve/resolver_trait.rs` per data-model E2 — 2 variants (`Connection { connection, basename_to_hash }`, `FileOp(&FileAccessOperation)`) with lifetime `'a` on the borrows
- [X] T006 [P] Define `ResolveContext<'a>` struct in `mikebom-cli/src/resolve/resolver_trait.rs` per data-model E3 — fields `deb_codename: Option<&'a str>`, `skip_online_validation: bool`
- [X] T007 Define `Resolver` trait in `mikebom-cli/src/resolve/resolver_trait.rs` per data-model E1 + contract C-1 — signature exactly as C-1 locks (`name`, `priority`, `technique`, `confidence`, `handles`, `async fn resolve` returning `Result<Vec<ResolvedComponent>, ResolverError>`). Uses native RPITIT (Rust 1.75+ `async fn in trait`) per research R1; NO `async-trait` crate. `Send + Sync` supertrait bounds
- [X] T008 Add unit test module in `mikebom-cli/src/resolve/resolver_trait.rs` — smoke test: define a trivial `TestResolver` in the test module, assert its `name()` / `priority()` / etc. return the expected values; verify `async fn` in trait compiles + can be awaited (guards against RPITIT MSRV regression); use `#[cfg_attr(test, allow(clippy::unwrap_used))]` per Constitution Principle IV
- [X] T009 Define `RESOLVER_REGISTRY` const in `mikebom-cli/src/resolve/resolver_chain.rs` per data-model E6 — 10-entry `&[(&str, u32)]` array with the priority layout from contract C-5 (cargo=100, pypi=99, npm=98, golang=97, maven=96, rubygems=95, deb=94, deps_dev_hash=90, path=70, hostname_fallback=40)
- [X] T010 Implement `assert_registry_priorities_unique` `const fn` in `mikebom-cli/src/resolve/resolver_chain.rs` per data-model E6 + research R2 — nested `while` loop over the slice pair-comparing priorities; `panic!` with the file-pointer message from R2 on collision. Follow with `const _: () = assert_registry_priorities_unique(RESOLVER_REGISTRY);` module-scope invocation. Verify `cargo build` succeeds
- [X] T011 Define `ResolverChain` struct in `mikebom-cli/src/resolve/resolver_chain.rs` per data-model E5 — field `resolvers: Vec<Box<dyn Resolver>>`; `new_default()` placeholder returning empty `Vec` (filled in during Phase 3 as each resolver lands); `run(input, ctx)` placeholder returning empty `Vec<ResolvedComponent>` (implemented after all resolvers registered)
- [X] T012 Preserve pre-refactor `pipeline::resolve` verbatim as `mikebom-cli/src/resolve/pipeline_legacy_reference.rs` per research R6 + data-model E10 — copy the current `resolve` function body into a new file; gate the whole module with `#[cfg(test)]` at the top; add a top-of-file doc-comment naming m209 SC-001 as the reason + a scheduled-for-deletion note. Register the module in `resolve/mod.rs` with `#[cfg(test)] mod pipeline_legacy_reference;`. Verify `cargo test --no-run` compiles
- [X] T013 **PIVOT (recon finding)**: existing mikebom test infrastructure builds `InTotoStatement` in-code via helpers like `verify_dsse.rs::minimal_statement` (verified via grep); no vendored attestation JSON fixtures exist. Adopted the in-code pattern — SC-001 byte-identity harness (T030) constructs fixtures programmatically inside a unit-test module at `mikebom-cli/src/resolve/byte_identity_test.rs` where the `#[cfg(test)]`-gated legacy oracle is accessible. No on-disk `attestation_corpus/` directory populated; the `mikebom-cli/tests/fixtures/resolver_chain/` scaffold from T003 becomes docs-only. Documented in the Phase-2 impl(209) commit; no functional or coverage regression vs. the original T013 intent
- [X] T014 **PIVOT (paired with T013)**: byte-identity reference is captured in-process at test-time rather than persisted to `byte_identity_reference.json`. The T030 harness runs BOTH the legacy oracle AND the new chain over the same in-code fixtures in a single test invocation, comparing outputs directly. Advantage: no drift between oracle output + reference file. Trade-off documented in the Phase-2 impl(209) commit

**Checkpoint**: Foundational done. Run `cargo +stable clippy -p mikebom --all-targets -- -D warnings` — zero errors, zero warnings. Run `cargo +stable test -p mikebom` — no new test failures, no existing regressions. Compile-time priority-collision check active (verify by temporarily editing `RESOLVER_REGISTRY` to duplicate a priority — `cargo build` MUST fail; then revert).

## Phase 3: User Story 1 — Add a new ecosystem resolver without touching orchestration code (P1, MVP)

**Story goal**: A contributor can add a new `Resolver` impl in a single new file plus one registry line, and the pipeline dispatches to it without editing `pipeline.rs`.

**Independent test**: `mikebom-cli/tests/resolver_chain_byte_identity.rs` — replays every T013 fixture through the new `ResolverChain::new_default()` and asserts the resulting `Vec<ResolvedComponent>` matches T014's reference set exactly. Plus a `NugetResolver` proof-of-concept subtest (per SC-002) that verifies adding a new ecosystem requires exactly two file edits (one new file + one line in RESOLVER_REGISTRY + one line in resolvers/mod.rs), asserted via `git diff --name-only` against a captured pre-Nuget commit.

- [ ] T015 [P] [US1] Implement `CargoResolver` at `mikebom-cli/src/resolve/resolvers/cargo.rs` per data-model E7 — extract `resolve_cargo` from `mikebom-cli/src/resolve/url_resolver.rs:41-90ish` verbatim into an `async fn resolve` body wrapped in the `Resolver` trait impl (`name = "cargo"`, `priority = 100`, `technique = UrlPattern`, `confidence = 0.95`, `handles` checks hostname `crates.io` / `static.crates.io`). Returns `Ok(vec![component])` on match, `Ok(vec![])` on clean no-match, `Err(ResolverError::MalformedInput)` on unparseable input
- [ ] T016 [P] [US1] Implement `PypiResolver` at `mikebom-cli/src/resolve/resolvers/pypi.rs` — same pattern as T015, extract from `url_resolver::resolve_pypi`; `priority = 99`
- [ ] T017 [P] [US1] Implement `NpmResolver` at `mikebom-cli/src/resolve/resolvers/npm.rs` — same pattern; `priority = 98`
- [ ] T018 [P] [US1] Implement `GolangResolver` at `mikebom-cli/src/resolve/resolvers/golang.rs` — same pattern; `priority = 97`
- [ ] T019 [P] [US1] Implement `MavenResolver` at `mikebom-cli/src/resolve/resolvers/maven.rs` — same pattern; `priority = 96`
- [ ] T020 [P] [US1] Implement `RubyGemsResolver` at `mikebom-cli/src/resolve/resolvers/rubygems.rs` — same pattern; `priority = 95`
- [ ] T021 [P] [US1] Implement `DebResolver` at `mikebom-cli/src/resolve/resolvers/deb.rs` — same pattern but with `ctx.deb_codename` threaded into the extraction (matches `url_resolver::resolve_deb` signature that takes `deb_codename: Option<&str>`); `priority = 94`
- [ ] T022 [US1] Implement `DepsDevHashResolver` at `mikebom-cli/src/resolve/resolvers/deps_dev_hash.rs` per data-model E8 — struct wraps `super::super::hash_resolver::HashResolver`; `Resolver::handles` returns `false` when `ctx.skip_online_validation` OR input isn't `Connection` with a response `content_hash` (preserves FR-011); `Resolver::resolve` delegates to `self.inner.resolve(hash).await` and maps `anyhow::Error` returns to `ResolverError::Transient { resolver: "deps_dev_hash", source: e }`; `priority = 90`, `confidence = 0.90`, `technique = HashMatch`
- [ ] T023 [US1] Implement `PathResolver` at `mikebom-cli/src/resolve/resolvers/path.rs` per data-model E9 — thin wrapper around `super::super::path_resolver::resolve_path_with_context`; `handles` accepts BOTH `ResolveInput::Connection` (extracts URL basename) AND `ResolveInput::FileOp` (uses op.path); `priority = 70`, `confidence = 0.70`, `technique = FilePathHeuristic`
- [ ] T024 [US1] Implement `HostnameFallbackResolver` at `mikebom-cli/src/resolve/resolvers/hostname_fallback.rs` — thin wrapper around `super::super::hostname_resolver::resolve_hostname`; `handles` = `Connection` variant only; `priority = 40`, `confidence = 0.40`, `technique = HostnameHeuristic`
- [ ] T025 [US1] Register all 10 resolver modules in `mikebom-cli/src/resolve/resolvers/mod.rs` — add `pub(crate) mod cargo; pub(crate) mod pypi; ...` for each of the 10 files created in T015–T024. Alphabetical order
- [ ] T026 [US1] Implement `ResolverChain::new_default()` in `mikebom-cli/src/resolve/resolver_chain.rs` — instantiate one `Box::new(<T>)` per resolver (10 total), collect into `Vec<Box<dyn Resolver>>`, sort by `.priority()` descending, assert (with `debug_assert!`) that the sorted `.name()` sequence matches `RESOLVER_REGISTRY` in order. Panic with a clear message if any registered name has no live implementation
- [ ] T027 [US1] Implement `ResolverChain::run(input, ctx)` in `mikebom-cli/src/resolve/resolver_chain.rs` per research R4 + R5 — iterate `self.resolvers` in stored order (priority-descending); for each, check `resolver.handles(&input)` first; if true, spawn the `.await` via `tokio::task::spawn(async move { resolver.resolve(&input, ctx).await })` per R5; await the `JoinHandle` and match on `Ok(Ok(vec)) if !vec.is_empty()` → return vec (first-match-wins per R4); `Ok(Err(resolver_err))` → `tracing::warn!(resolver = resolver.name(), kind = "error", "{resolver_err}")` and continue; `Err(join_err) if join_err.is_panic()` → `tracing::warn!(resolver = resolver.name(), kind = "panic", "resolver panicked")` and continue; other JoinError variants → same warn + continue. Return `vec![]` if no resolver produced components
- [ ] T028 [US1] Rewire `mikebom-cli/src/resolve/pipeline.rs::ResolutionPipeline::resolve()` to iterate over the `ResolverChain` instead of the fixed function-call sequence — construct a `ResolverChain::new_default()` in `ResolutionPipeline::new`; in `resolve`, build the `basename_to_hash` map (existing logic, pipeline.rs:89-101 verbatim), extract `deb_codename` (existing logic, pipeline.rs:74-79 verbatim) into `ResolveContext`; for each connection AND each file-op, construct a `ResolveInput` variant and call `chain.run(input, &ctx).await`; extend `components` with the returned vec; per-input first-match-wins is enforced inside `chain.run`. Preserve the existing INFO logs at pipeline boundaries per FR-012
- [ ] T029 [US1] Delete `mikebom-cli/src/resolve/url_resolver.rs` per data-model E11 — the file's 7 `resolve_*` functions were extracted into T015–T021 verbatim. Remove `mod url_resolver;` from `resolve/mod.rs`. Verify `cargo build` succeeds (any missed callers of `url_resolver::resolve_url_with_context` must be updated to `ResolverChain` — grep first)
- [ ] T030 [US1] Write byte-identity integration test `mikebom-cli/tests/resolver_chain_byte_identity.rs` — for each fixture in T013's corpus: parse the attestation; run through `ResolverChain::new_default()` (post-refactor path); assert the resulting sorted-by-PURL `Vec<ResolvedComponent>` matches T014's reference exactly. Use `pretty_assertions::assert_eq!` for readable diffs on failure (workspace already has `pretty_assertions` in dev-deps — verify + use; if not, use `assert_eq!`). **Plus two additional subtests for analyze-finding coverage**: (a) `info_log_preservation_m209` (C1 / FR-012) — capture stderr from a single-fixture pipeline invocation using `tracing_subscriber::fmt::layer().with_writer(...)` redirected to a `Vec<u8>`; parse the captured lines and assert the expected INFO log fields (resolver name, component count) appear with equivalent shape to pre-refactor output; (b) `skip_online_validation_disables_deps_dev_end_to_end_m209` (C2 / FR-011) — run the SAME fixture (one that has a hash-hit deps.dev component in reference output) TWICE: once with `ResolveContext::skip_online_validation = false` (expect deps.dev components present) and once with `true` (expect ZERO components with `technique == HashMatch`); assert the diff matches the expected shape
- [ ] T031 [US1] Write SC-002 proof-of-concept subtest in `mikebom-cli/tests/resolver_chain_byte_identity.rs` — a `#[test]` that verifies the git-diff footprint of adding a hypothetical NuGet resolver is exactly 2 files (new `resolvers/nuget.rs` + edits to `resolvers/mod.rs` + `resolver_chain.rs` REGISTRY) OR skips with a diagnostic if the NuGet fixture files aren't present. **Alternative implementation** if git-diff assertion is fragile: hand-vendor a `mikebom-cli/tests/fixtures/resolver_chain/nuget_proof_of_concept/{nuget.rs,mod.rs.diff,registry.rs.diff}` set and assert the three files exist with expected contents — this is a paper-audit test rather than a compile-verifying test, but validates the same claim

**Checkpoint**: US1 done. `cargo build -p mikebom` clean. `cargo test -p mikebom --test resolver_chain_byte_identity` passes — post-refactor pipeline output byte-identical to legacy oracle across all fixtures. Run `./scripts/pre-pr.sh` — both clippy + test must be clean.

## Phase 4: User Story 2 — Unit-test resolvers in isolation (P2)

**Story goal**: Each resolver's unit test suite runs in isolation without loading fixtures or code paths from other resolvers; per-resolver test wall-clock under 100 ms per SC-003.

**Independent test**: `cargo test -p mikebom -- resolve::resolvers::cargo` runs only Cargo resolver tests, in under 100 ms, exercising every internal branch (matches API pattern, matches CDN pattern, rejects non-cargo hostname).

- [ ] T032 [P] [US2] Add per-resolver unit test module in `mikebom-cli/src/resolve/resolvers/cargo.rs::tests` — 5-8 tests covering: matches `/api/v1/crates/foo/1.2.3/download` → `pkg:cargo/foo@1.2.3`; matches CDN pattern `/crates/foo/foo-1.2.3.crate` → same PURL; rejects non-cargo hostname (returns `Ok(vec![])`); rejects malformed path (returns `Ok(vec![])`); URL segments requiring percent-encoding round-trip correctly. Use `#[cfg_attr(test, allow(clippy::unwrap_used))]`. **Every test MUST invoke `assert_sc003_timing_ok(start)` (from `resolvers/tests_common.rs` per T042) at exit — SC-003 hard-blocking wall-clock ≤ 100 ms per test**
- [ ] T033 [P] [US2] Same-shape per-resolver tests in `mikebom-cli/src/resolve/resolvers/pypi.rs::tests` — mirror T032 patterns for PyPI. **Inherits T032's SC-003 timing-assertion requirement (applies to T033–T041 uniformly)**
- [ ] T034 [P] [US2] Same-shape per-resolver tests in `mikebom-cli/src/resolve/resolvers/npm.rs::tests`
- [ ] T035 [P] [US2] Same-shape per-resolver tests in `mikebom-cli/src/resolve/resolvers/golang.rs::tests`
- [ ] T036 [P] [US2] Same-shape per-resolver tests in `mikebom-cli/src/resolve/resolvers/maven.rs::tests`
- [ ] T037 [P] [US2] Same-shape per-resolver tests in `mikebom-cli/src/resolve/resolvers/rubygems.rs::tests`
- [ ] T038 [P] [US2] Same-shape per-resolver tests in `mikebom-cli/src/resolve/resolvers/deb.rs::tests` — additionally verify `ctx.deb_codename` threads through to the emitted PURL's `distro` qualifier when `Some(...)` and is omitted when `None`
- [ ] T039 [P] [US2] Unit tests in `mikebom-cli/src/resolve/resolvers/deps_dev_hash.rs::tests` — 3-4 tests: `handles()` returns `false` when `ctx.skip_online_validation` is `true`; `handles()` returns `false` when input has no `content_hash`; `resolve()` maps upstream `anyhow::Error` to `ResolverError::Transient` correctly. Use `wiremock` (already a dev-dep per m055 research) OR a hand-rolled `httptest`-style fixture to stub the deps.dev endpoint — do NOT hit the real network
- [ ] T040 [P] [US2] Unit tests in `mikebom-cli/src/resolve/resolvers/path.rs::tests` — verify wrapper correctly threads `resolve_path_with_context` outputs through both `Connection` (via URL basename) AND `FileOp` (via op.path) input variants
- [ ] T041 [P] [US2] Unit tests in `mikebom-cli/src/resolve/resolvers/hostname_fallback.rs::tests` — verify wrapper correctly threads `resolve_hostname` outputs through the `Connection` variant only; `handles(FileOp)` returns `false`
- [ ] T042 [P] [US2] Add per-chain-behavior tests in `mikebom-cli/src/resolve/resolver_chain.rs::tests` — 4-5 tests: first-match-wins per-connection (highest-priority resolver's non-empty result wins, subsequent resolvers not invoked); chain skips resolver whose `handles()` returns `false`; chain catches `Err` return + logs WARN + continues; chain catches panic + logs WARN + continues (use a synthetic `PanickyResolver` in test module); chain returns `vec![]` when no resolver produces components. **SC-003 timing (C3 remediation — was informational; now BLOCKING)**: each per-resolver test in T032–T041 MUST record `let start = std::time::Instant::now();` at test entry and `assert!(start.elapsed() < std::time::Duration::from_millis(100), "SC-003 violation: test exceeded 100ms wall-clock: {:?}", start.elapsed());` at test exit. Wrap this pattern in a `#[track_caller] fn assert_sc003_timing_ok(start: Instant)` helper in a shared test-module `resolvers/tests_common.rs` so per-resolver tests can call it without boilerplate. Failing SC-003 timing = failing CI

**Checkpoint**: US2 done. All 11 per-resolver test modules exist; each runs in under 100 ms; each exercises its resolver's code paths in isolation.

## Phase 5: User Story 3 — Preserve `ResolutionEvidence.technique` signal (P3)

**Story goal**: Every downstream consumer of `ResolutionEvidence.technique` receives identical technique values before and after the refactor (SC-005 = 100 % preservation).

**Independent test**: `mikebom-cli/tests/resolver_chain_byte_identity.rs::technique_signal_preservation` — for each fixture in T013's corpus, extract every emitted component's `.evidence.technique` value from both the legacy oracle output AND the post-refactor chain output; assert both sequences are identical.

- [ ] T043 [US3] Add `technique_signal_preservation` test to `mikebom-cli/tests/resolver_chain_byte_identity.rs` — iterate the same T013 fixture corpus; for each fixture, run through both legacy oracle (`pipeline_legacy_reference::resolve`) AND new chain (`ResolverChain::new_default().run` orchestrated through `ResolutionPipeline`); extract `Vec<ResolutionTechnique>` from each side sorted by PURL; assert byte-equal. If T030's byte-identity test passes but T043 fails, the bug is in how the new chain assigns techniques — fix in the per-resolver `technique()` returns
- [ ] T044 [US3] Add contract-locked test in `mikebom-cli/src/resolve/resolver_chain.rs::tests::technique_mapping_matches_contract` — for each entry in `RESOLVER_REGISTRY`, look up the corresponding resolver in `ResolverChain::new_default()`, call its `.technique()`, and assert it matches the `ResolutionTechnique` value locked in contract C-4. Fails compilation-adjacent if any resolver's technique drifts from the contract without an intentional update in the same PR

**Checkpoint**: US3 done. Technique-signal preservation locked at both fixture-corpus level (T043) AND per-resolver contract level (T044).

## Phase 6: Polish & Cross-Cutting

- [ ] T045 [P] Write compile-time-collision test at `mikebom-cli/tests/resolver_priority_collision.rs` per research R8 — first attempt: `///```compile_fail\nuse mikebom::resolve::resolver_chain::assert_registry_priorities_unique;\nconst _: () = assert_registry_priorities_unique(&[("a", 1), ("b", 1)]);\n///```` doctest on a public re-export of the assertion function. If doctest can't verify the specific panic message, fall back to R8's subprocess-fixture approach (create a minimal `mikebom-cli/tests/fixtures/resolver_collision_fixture/` with a duplicate-priority REGISTRY + `Cargo.toml`; test invokes `cargo build --manifest-path <fixture-toml>` as a subprocess, asserts non-zero exit + stderr contains the panic message). Document the choice in the file's top-of-file comment
- [ ] T046 [P] Add SC-004 perf regression test at `mikebom-cli/tests/resolver_chain_perf.rs` — `#[ignore]`-gated per m094 convention; runs the m083 knative-func audit fixture (`mikebom-cli/tests/fixtures/transitive_parity/cargo/`) through both the legacy oracle AND the new chain, captures wall-clock for each in a single process, asserts `post_refactor_ms <= 1.05 * baseline_ms`. Also assert `post_refactor_ms > 0` (guards against fixture-not-loading false-pass). Write baseline to `mikebom-cli/tests/fixtures/resolver_chain/perf_baseline.json` on first run (with `MIKEBOM_UPDATE_M209_PERF_BASELINE=1`); read + assert against on subsequent runs
- [ ] T047 [P] Write resolver-authoring guide at `docs/architecture/resolvers.md` per FR-016 — sections: overview of the trait chain model; step-by-step "adding a new resolver" recipe (mirror quickstart.md Path 2 verbatim); the `RESOLVER_REGISTRY` compile-time collision-check semantics; per-resolver testing patterns; panic + error semantics (link back to FR-013 + Q1 clarification). Link from `docs/design-notes.md` under an "Architecture references" section
- [ ] T048 [P] Add an ADR-style entry documenting the m209 refactor's shape decisions — either a new `docs/adr/209-resolver-trait-chain.md` OR (if the project doesn't use per-milestone ADRs — verify) inline in the existing `docs/design-notes.md` under a new "Milestone 209: Resolver Trait + Chain" heading. Cite R1 (RPITIT choice), R2 (compile-time collision), R5 (tokio::spawn panic-catch), R6 (legacy oracle preservation)
- [ ] T049 Verify pre-PR gate passes clean — run `./scripts/pre-pr.sh` from repo root; both `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace` must exit 0 with zero warnings and every suite reporting `ok. N passed; 0 failed`
- [ ] T050 Verify byte-identity of pre-m209 scan-mode goldens — grep for any golden-fixture test that regenerated during the refactor; if any regenerated, the SC-001 defensive-default in Phase 3 is broken and MUST be fixed. Expected outcome: zero golden regens
- [ ] T051 Open PR against main — title `impl(209): resolver trait + chain refactor (closes #601)`; body cites the pre-PR gate output line-by-line + notes zero-golden-regen result + notes SC-002 proof-of-concept via T031

## Dependencies

**Phase → Phase**: 1 → 2 → (3, 4, 5 mostly parallel but 3 blocks 4+5) → 6.

**Within Phase 2 (Foundational)**:
- T004, T005, T006 are all `[P]` — different types in the same file but independent (can be added in one commit).
- T007 requires T004 + T005 + T006 (trait references all three).
- T008 requires T007.
- T009 has no dependencies (standalone const).
- T010 requires T009 (references RESOLVER_REGISTRY).
- T011 requires T007 + T010 (chain holds `Vec<Box<dyn Resolver>>`).
- T012 has no dependencies (preserves existing code verbatim).
- T013 has no dependencies (fixture vendoring).
- T014 requires T012 + T013 (runs legacy oracle against fixtures).

**Within Phase 3 (US1)**:
- T015–T021 (7 URL-family resolvers) are ALL `[P]` — different files, no cross-dependencies.
- T022 (DepsDevHashResolver) requires foundational types only.
- T023, T024 (PathResolver, HostnameFallbackResolver) require foundational types only.
- T025 (register modules) requires T015–T024 to have created their files.
- T026 (ResolverChain::new_default) requires T025 (needs the pub types to instantiate).
- T027 (ResolverChain::run) requires T026 (needs to iterate the constructed chain).
- T028 (rewire pipeline) requires T027 + T012 (must not break the legacy reference which is still compiled).
- T029 (delete url_resolver.rs) requires T015–T021 + T028 (all callers migrated).
- T030 (byte-identity test) requires T028 + T014 (compares chain output vs. captured reference).
- T031 (SC-002 proof-of-concept) requires T030 or is independent (paper-audit variant).

**Within Phase 4 (US2)**:
- T032–T042 are all `[P]` (11 different files).

**Within Phase 5 (US3)**:
- T043, T044 both require Phase 3 done. Independent of each other; can be `[P]` if worked simultaneously, but both edit different files, so mark T044 `[P]` relative to T043.

**Within Phase 6 (Polish)**:
- T045, T046, T047, T048 all `[P]`.
- T049 requires everything else.
- T050 requires T049.
- T051 requires T049 + T050.

## Parallel Execution Examples

**Phase 2 type definitions** (same file, different types):
```text
T004 [P] ResolverError enum
T005 [P] ResolveInput enum
T006 [P] ResolveContext struct
```

**Phase 3 per-ecosystem resolver extraction** (7 different files):
```text
T015 [P] [US1] CargoResolver
T016 [P] [US1] PypiResolver
T017 [P] [US1] NpmResolver
T018 [P] [US1] GolangResolver
T019 [P] [US1] MavenResolver
T020 [P] [US1] RubyGemsResolver
T021 [P] [US1] DebResolver
```

**Phase 4 per-resolver test suites** (11 different files):
```text
T032 [P] [US2] cargo tests
T033 [P] [US2] pypi tests
… through T042 [P] [US2] chain tests
```

**Phase 6 polish** (independent):
```text
T045 [P] compile-fail test
T046 [P] perf regression test
T047 [P] resolvers.md docs
T048 [P] design-notes ADR entry
```

## Implementation Strategy

- **MVP scope**: Phase 1 + Phase 2 + Phase 3 (US1) = 31 tasks. Delivers the refactored chain with byte-identical output on the fixture corpus + the SC-002 proof-of-concept for extensibility. US2 (per-resolver test isolation) + US3 (technique-signal preservation lock) build on top without touching MVP surface.
- **Incremental delivery**: after MVP, US2 (T032–T042) adds per-resolver test modules — pure additive tests, no code changes; can ship in the same PR or a follow-up. US3 (T043–T044) adds two locking regression tests; ~2 hours of work.
- **Sequencing recommendation**: One PR for the whole milestone. Total ~2500 LOC counting tests. If the PR is too large to review, natural split boundaries are: (A) Setup + Foundational (Phases 1+2, T001–T014); (B) US1 (T015–T031); (C) US2 + US3 (T032–T044); (D) Polish (T045–T051). Split A pre-emerges the trait scaffold without touching the pipeline; A→B is where semantic change happens; C+D are additive.
- **Rollback plan if byte-identity breaks**: the T012 legacy oracle stays in place for at least 2 releases post-merge; if a regression surfaces, revert `pipeline.rs` to invoke `pipeline_legacy_reference::resolve` as a one-line emergency fix while root-causing the chain divergence.

## Task count

- **Setup**: 3 (T001–T003)
- **Foundational**: 11 (T004–T014)
- **US1 (P1, MVP)**: 17 (T015–T031)
- **US2 (P2)**: 11 (T032–T042)
- **US3 (P3)**: 2 (T043–T044)
- **Polish**: 7 (T045–T051)

**Total**: 51 tasks
