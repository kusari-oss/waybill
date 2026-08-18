---
description: "Task list for m663 — local-cache-probe resolver tier"
---

# Tasks: Local-cache-probe resolver tier

**Input**: Design documents from `/specs/663-cache-probe-resolver/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/probe-interface.md, contracts/per-ecosystem-cache-shapes.md

**Tests**: Per-ecosystem unit tests + cross-ecosystem integration test + parity test. Required by FR-009, FR-010, FR-011, SC-001–SC-007.

**Organization**: 3 user stories (US1 Maven+Go / US2 Cargo+Ruby / US3 npm+Python). Setup + Foundational phases block all user stories.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Parallelizable — different files, no dependencies on incomplete tasks
- **[Story]**: US1 / US2 / US3

## Path Conventions

Two-crate change: `waybill-cli/` (resolver + tests + parity) and `waybill-common/` (single enum-variant addition).

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Locate existing pipeline substrate + verify m209 trait chain state.

- [ ] T001 Grep `waybill-cli/src/resolve/resolver_chain.rs` to confirm the `RESOLVER_REGISTRY` shape matches plan.md R1 (priorities 100-40 with a gap at 91-93 between rubygems=95 and deps_dev_hash=90). Record the exact free-slot number in a scratch note for T004.

- [ ] T002 Grep `waybill-common/src/resolution.rs` for the `ResolutionTechnique` enum. Confirm existing variants (`UrlPattern`, `HashMatch`, `PackageDatabase`, `FilePathPattern`, `HostnameFallback`) match plan.md R2. If a `LocalCacheHit` variant already exists, note the branch — this milestone's variant addition may already be partially landed.

- [ ] T003 Create the module scaffold: `mkdir -p waybill-cli/src/resolve/resolvers/cache_probe/`. Create empty `mod.rs` + one empty file per ecosystem probe (`maven.rs`, `golang.rs`, `cargo.rs`, `rubygems.rs`, `npm.rs`, `pypi.rs`). Each file contains only a `use super::*;` line for now; concrete impls land in the US1-US3 phases. Also touch `waybill-cli/src/resolve/resolvers/cache_probe.rs` (the resolver-struct file — parent of the module).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add the `LocalCacheHit` variant + register the resolver in `RESOLVER_REGISTRY` + wire the parity extractor. Once these land, US1-US3 probes can plug in without pipeline surgery.

- [ ] T004 Add `LocalCacheHit` variant to `ResolutionTechnique` in `waybill-common/src/resolution.rs`. Position: after `FilePathPattern`, before `HostnameFallback`. Docstring per data-model.md ("Confidence 0.92 — cache-path structure names the coord unambiguously; higher than deps.dev because artifact IS on this machine"). Add the serde `snake_case` (implicit via enum-level attribute) which yields `"local_cache_hit"` on the wire.

- [ ] T005 Add corresponding `LocalCacheHit` mapping in any `ResolutionTechnique::confidence()` or similar helper functions in `waybill-common/src/resolution.rs`. Return `0.92`.

- [ ] T006 [P] Add the `cache_probe` entry to `RESOLVER_REGISTRY` in `waybill-cli/src/resolve/resolver_chain.rs` at priority 92 (from T001's free-slot verification). Position: after `("deb", 94)`, before `("deps_dev_hash", 90)`. Verify `assert_registry_priorities_unique` still passes at compile time.

- [ ] T007 [P] Add the `CacheProbeResolver` struct + `Resolver` trait impl skeleton in `waybill-cli/src/resolve/resolvers/cache_probe.rs`. Fields per data-model.md (`probes: Vec<EcosystemProbe>`). Trait methods: `name() = "cache_probe"`, `priority() = 92`, `technique() = ResolutionTechnique::LocalCacheHit`, `confidence() = 0.92`, `handles()` returns true for path-shaped inputs, `resolve()` iterates probes first-match-wins and returns empty when none match.

- [ ] T008 [P] Add the `EcosystemProbe` enum in `waybill-cli/src/resolve/resolvers/cache_probe/mod.rs` with all 6 variants (Maven, Golang, Cargo, RubyGems, NpmPnpm, PyPi). Add `try_match(&self, path: &Path) -> Option<Purl>` dispatch method routing to the per-file `try_match_<eco>` functions (all stubbed to return `None` for now — they land in US1-US3).

- [ ] T009 [P] Register `CacheProbeResolver` in `ResolverChain::new_default()` at `waybill-cli/src/resolve/resolver_chain.rs`. Follow the existing per-resolver instantiation pattern.

- [ ] T010 [P] Add the parity extractor triple + registry entry for `waybill:resolver-tier` at component scope. Files:
  - `waybill-cli/src/parity/extractors/cdx.rs`: `cdx_anno!(cN_cdx, "waybill:resolver-tier", component);`
  - `waybill-cli/src/parity/extractors/spdx2.rs`: `spdx23_anno!(cN_spdx23, "waybill:resolver-tier", component);`
  - `waybill-cli/src/parity/extractors/spdx3.rs`: `spdx3_anno!(cN_spdx3, "waybill:resolver-tier", component);`
  - `waybill-cli/src/parity/extractors/mod.rs`: register in EXTRACTORS array with next-available C-number (grep for the highest existing row; likely C152).

- [ ] T011 Add the C-row to `docs/reference/sbom-format-mapping.md` for `waybill:resolver-tier`. Value enum: `"url_pattern" / "local_cache_hit" / "hash_match" / "package_database" / "file_path_pattern" / "hostname_fallback"`. Standards-native audit: KEEP-NO-NATIVE (no CDX/SPDX carrier for "which resolver tier produced this component's identity"). Cross-reference to `ResolutionTechnique::as_wire_str()` as the source of truth.

- [ ] T012 Wire the `waybill:resolver-tier` annotation emission into the `ResolvedComponent → PackageDbEntry` conversion in the resolver pipeline (search for where `ResolutionEvidence.technique` is set — the same emit path adds `extra_annotations.insert("waybill:resolver-tier", technique.as_wire_str())`). This wires the annotation for ALL resolvers, not just cache-probe (design-model.md universal-annotation decision).

- [ ] T013 Verify parity gate passes with C-next-available registered: `cargo +stable test -p waybill --lib every_catalog_row_has_an_extractor` MUST pass. Also verify the compile-time `assert_registry_priorities_unique` still passes.

**Checkpoint**: After Phase 2 completes, the resolver is registered (priority 92, technique `LocalCacheHit`, confidence 0.92) but has zero probe matches — all US1-US3 inputs fall through to deps.dev. Ready for per-ecosystem probe landings.

---

## Phase 3: User Story 1 — Maven + Go cache-hit resolution (Priority: P1) 🎯 MVP

**Story goal**: Attestation paths under `~/.m2/repository/` and `$GOMODCACHE` extract the correct Maven GAV / Go module coord at confidence 0.92.

**Independent test criteria**: 2 per-ecosystem unit tests (Maven + Go) + a partial integration test using the 2 ecosystems.

### Maven probe (T014–T015)

- [ ] T014 [US1] Implement `try_match_maven(path: &Path) -> Option<Purl>` in `waybill-cli/src/resolve/resolvers/cache_probe/maven.rs` per `contracts/per-ecosystem-cache-shapes.md#maven-us1`. Cache root: `env::var_os("M2_HOME").map(|p| p.join("repository"))` else `dirs::home_dir()?.join(".m2/repository")`. Extraction: split path segments after cache root into `[g1, g2, ..., artifact, version, filename]`; construct `pkg:maven/<g1.g2.>/<artifact>@<version>`. Per Q1 clarification: if the path can't be split cleanly (fewer than 3 segments after root), log `tracing::warn!` naming the path + reason and return `None`.

- [ ] T015 [US1] Add unit test in `waybill-cli/src/resolve/resolvers/cache_probe/maven.rs::mod tests`. Use `tempfile::tempdir()` for the fake `.m2/repository/` tree. Set `M2_HOME` for the test scope (unset after via `env_guard` helper if it exists; else use `std::env::remove_var` in a scoped teardown). Assert `try_match_maven` returns `Some(purl_str == "pkg:maven/com.example.waybillfixture/waybill-fixture-lib@1.0.0")` for path `<tempdir>/repository/com/example/waybillfixture/waybill-fixture-lib/1.0.0/waybill-fixture-lib-1.0.0.jar`. Test name: `m663_maven_cache_hit_extracts_gav`.

### Golang probe (T016–T017)

- [ ] T016 [US1] Implement `try_match_golang(path: &Path) -> Option<Purl>` in `waybill-cli/src/resolve/resolvers/cache_probe/golang.rs` per `contracts/per-ecosystem-cache-shapes.md#go-us1`. Cache roots: check `env::var_os("GOMODCACHE")`, else `env::var_os("GOPATH").map(|p| p.join("pkg/mod"))`, else `dirs::home_dir()?.join("go/pkg/mod")`. Extraction: walk segments after cache root; find segment matching `<name>@v<version>`. Pre-`@` segments join into `namespace`. Emit `pkg:golang/<namespace>/<name>@<version>`. Q1 decline: no `@` found or version doesn't start with `v` → warn + None.

- [ ] T017 [US1] Add unit test `m663_golang_cache_hit_extracts_module_coord` in `golang.rs`. Fixture: `<tempdir>/pkg/mod/example.com/waybill/fixture@v2.0.0/main.go`. Assert PURL `pkg:golang/example.com/waybill/fixture@v2.0.0`.

### US1 integration (T018–T019)

- [ ] T018 [US1] Enable the `Maven` and `Golang` variants in `EcosystemProbe::try_match()` dispatch — remove the stubbed `None` returns for these two, route to their real `try_match_*` functions.

- [ ] T019 [US1] Cross-ecosystem integration test in `waybill-cli/tests/cache_probe_universal.rs` (create the file). US1 subset: scan a synthetic attestation containing 1 Maven path + 1 Go path. Assert both emit at `confidence == 0.92`, technique `LocalCacheHit`, `waybill:resolver-tier == "local_cache_hit"`. Also assert deps.dev is NOT called for these paths (mock/spy on the deps.dev resolver's `resolve()` call count).

### US1 checkpoint

- [ ] T020 [US1] Run `cargo +stable test -p waybill --bin waybill m663_` — confirm 2 unit tests pass. Run `cargo +stable test -p waybill --test cache_probe_universal` — confirm the US1 integration test passes. Combined: 3 passing tests.

**Checkpoint**: US1 (MVP) is shippable — Maven + Go cache-hit resolution live end-to-end.

---

## Phase 4: User Story 2 — Cargo + Ruby cache-hit resolution (Priority: P2)

**Story goal**: Attestation paths under `~/.cargo/registry/` and `~/.gem/specs/` (or Bundler's `vendor/bundle`) extract Cargo / Ruby coords at confidence 0.92.

### Cargo probe (T021–T022)

- [ ] T021 [US2] Implement `try_match_cargo(path: &Path) -> Option<Purl>` in `waybill-cli/src/resolve/resolvers/cache_probe/cargo.rs` per `contracts/per-ecosystem-cache-shapes.md#cargo-us2`. Support BOTH variants (crate cache + src extraction). Cache root: `env::var_os("CARGO_HOME").map(|p| p.join("registry"))` else `dirs::home_dir()?.join(".cargo/registry")`. Extraction: for `cache/<hash>/<name>-<version>.crate` or `src/<hash>/<name>-<version>/`, split filename/dirname stem on LAST `-` before a semver-shaped suffix (regex `-(\d+\.\d+\.\d+.*)$`). Emit `pkg:cargo/<name>@<version>`. Q1 decline: no semver suffix found → warn + None.

- [ ] T022 [US2] Unit test `m663_cargo_cache_hit_extracts_crate_coord` in `cargo.rs`. Test BOTH variants: (a) `<tempdir>/registry/cache/github.com-1ecc.../waybill-fixture-crate-1.2.3.crate` → `pkg:cargo/waybill-fixture-crate@1.2.3`. (b) `<tempdir>/registry/src/github.com-1ecc.../waybill-fixture-crate-1.2.3/Cargo.toml` → same PURL.

### RubyGems probe (T023–T024)

- [ ] T023 [US2] Implement `try_match_rubygems(path: &Path) -> Option<Purl>` in `waybill-cli/src/resolve/resolvers/cache_probe/rubygems.rs` per `contracts/per-ecosystem-cache-shapes.md#rubygems-us2`. Support BOTH variants:
  - Variant A: `env::var_os("GEM_HOME").map(|p| p.join("specs/rubygems.org%443"))` else `dirs::home_dir()?.join(".gem/specs/rubygems.org%443")` — filename `<name>-<version>.gemspec`.
  - Variant B: any path segment `vendor/bundle/ruby/<x>/gems/<name>-<version>/`.
  - Both variants: split on last `-` before semver (mirrors Cargo helper). Emit `pkg:gem/<name>@<version>`. Q1 decline: no split → warn + None.

- [ ] T024 [US2] Unit test `m663_rubygems_cache_hit_extracts_gem_coord` in `rubygems.rs`. Test both variants with synthetic `waybill-fixture-gem-1.2.3` names.

### US2 integration (T025–T026)

- [ ] T025 [US2] Enable `Cargo` and `RubyGems` variants in `EcosystemProbe::try_match()` dispatch.

- [ ] T026 [US2] Extend `cache_probe_universal.rs` cross-ecosystem test to include Cargo + Ruby paths. Assert 4 emissions total (Maven + Go + Cargo + Ruby) at 0.92 confidence.

### US2 checkpoint

- [ ] T027 [US2] Run `cargo +stable test -p waybill --bin waybill m663_` — confirm 4/4 unit tests pass. Combined with US1: 5/5 tests pass.

---

## Phase 5: User Story 3 — npm/pnpm + Python cache-hit resolution (Priority: P3)

**Story goal**: Attestation paths under `node_modules/*/package.json` (MVP scope; pnpm content-addressed store deferred per plan R3) and Python `site-packages` / wheel cache extract coords at confidence 0.92. This tier introduces the bounded-metadata-read pattern with Q1 decline-on-failure semantics.

### NpmPnpm probe (T028–T030)

- [ ] T028 [US3] Implement `try_match_npm_pnpm(path: &Path) -> Option<Purl>` in `waybill-cli/src/resolve/resolvers/cache_probe/npm.rs` per `contracts/per-ecosystem-cache-shapes.md#npm--pnpm-us3`. Scope: `node_modules/<name>/package.json` OR `node_modules/@<scope>/<name>/package.json` variant only. Extraction: bounded read (max 64 KiB) of the `package.json`, parse via `serde_json::from_slice`, extract `.version` field. Emit `pkg:npm/<name>@<version>` or `pkg:npm/%40<scope>/<name>@<version>` (URL-encoded scope prefix per PURL spec). Q1 decline: file unreadable OR JSON parse error OR `.version` missing/non-string → `tracing::warn!` naming path + reason + return None. **Pnpm content-addressed store variant explicitly out of scope** for MVP; add a TODO comment referencing plan R3 deferral.

- [ ] T029 [US3] Unit test `m663_npm_package_json_extracts_purl` in `npm.rs`. Fixture: `<tempdir>/node_modules/waybill-fixture-npm/package.json` containing `{"name": "waybill-fixture-npm", "version": "1.0.0"}`. Assert PURL `pkg:npm/waybill-fixture-npm@1.0.0`. Also test the scoped variant (`@waybillfixture/scoped-lib`) and the URL-encoded emission (`pkg:npm/%40waybillfixture/scoped-lib@2.0.0`).

- [ ] T030 [US3] Unit test `m663_npm_metadata_failure_declines` in `npm.rs`. Two fixtures: (a) `package.json` with malformed JSON — assert `try_match_npm_pnpm` returns None + emits a warn. (b) `package.json` with `{"name": "foo"}` (missing version) — same expectation.

### PyPi probe (T031–T033)

- [ ] T031 [US3] Implement `try_match_pypi(path: &Path) -> Option<Purl>` in `waybill-cli/src/resolve/resolvers/cache_probe/pypi.rs` per `contracts/per-ecosystem-cache-shapes.md#python-us3`. Support BOTH variants:
  - Variant A (`dist-info/METADATA`): parse RFC-822-shape line-by-line looking for `Version:` header. Read at most 64 KiB.
  - Variant B (wheel cache): filename stem `<name>-<version>-py3-none-any.whl` — split on first `-` (name) + regex-extract semver.
  - Both variants: normalize name to lowercase-hyphens per PyPI convention. Emit `pkg:pypi/<name>@<version>`. Q1 decline paths per Variant.

- [ ] T032 [US3] Unit test `m663_pypi_dist_info_extracts_purl` in `pypi.rs`. Fixture: `<tempdir>/site-packages/waybill_fixture_pip-1.0.0.dist-info/METADATA` containing `Version: 1.0.0` line. Assert PURL `pkg:pypi/waybill-fixture-pip@1.0.0` (note underscore→hyphen normalization).

- [ ] T033 [US3] Unit test `m663_pypi_wheel_cache_extracts_purl` in `pypi.rs`. Fixture: `<tempdir>/wheels/.../waybill_fixture_pip-1.0.0-py3-none-any.whl`. Assert PURL `pkg:pypi/waybill-fixture-pip@1.0.0`. Also add `m663_pypi_metadata_failure_declines` covering missing `Version:` header.

### US3 integration (T034–T035)

- [ ] T034 [US3] Enable `NpmPnpm` and `PyPi` variants in `EcosystemProbe::try_match()` dispatch.

- [ ] T035 [US3] Extend `cache_probe_universal.rs` cross-ecosystem test to include npm + Python paths. Total 6 emissions (Maven + Go + Cargo + Ruby + npm + Python) at 0.92 confidence.

### US3 checkpoint

- [ ] T036 [US3] Run `cargo +stable test -p waybill --bin waybill m663_` — confirm 8/8 unit tests pass (adds npm×2 + pypi×3 to prior 4). Combined: 8 unit tests + 1 integration test = 9 passing tests.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: SC-006 microbenchmark + SC-007 CI matrix + spec close-out + memory ref.

### Cross-cutting tests (T037–T039)

- [ ] T037 SC-004 env-var-override test in `cache_probe_universal.rs`: set `GOMODCACHE=/opt/gomod` before scan; fixture attestation uses a path under `/opt/gomod`. Assert the resolver correctly extracts coord from the non-default location.

- [ ] T038 SC-005 no-cache-path fall-through test in `cache_probe_universal.rs`: fixture attestation names a path that doesn't match ANY known cache prefix. Assert cache-probe returns empty, deps.dev is called, and the emitted component's `technique` is `HashMatch` (NOT `LocalCacheHit`).

- [ ] T039 SC-006 microbenchmark. Add a `#[test]` with `Instant::now()` timing per-path overhead across ≥100k warm-filesystem paths with the cache-probe resolver enabled. Compute p95 latency; assert **p95 ≤ 5 ms**. Location: `waybill-cli/tests/cache_probe_perf.rs`. Note: 100k paths is the minimum sample size for meaningful p95; larger samples improve stability but pre-PR gate limits argue for the minimum.

### Docs + close-out (T040–T043)

- [ ] T040 [P] Update `docs/ecosystems.md` — add a new subsection on cache-probe resolver-tier semantics (which resolvers run when + which cache paths are matched). Cross-reference `contracts/per-ecosystem-cache-shapes.md`.

- [ ] T041 [P] Run pre-PR gate: `./scripts/pre-pr.sh`. MUST pass clean (zero clippy warnings; every test suite green).

- [ ] T042 Open PR titled `feat(m663): local-cache-probe resolver tier (closes #605)`. Body includes: summary linking spec + plan + tasks + issue #605; per-ecosystem cache-shape table (from contracts/per-ecosystem-cache-shapes.md); test plan checklist; deferred section (pnpm content-addressed store).

- [ ] T043 Add spec close-out note to `specs/663-cache-probe-resolver/spec.md` under a new `## Close-out (post-implementation)` section: (a) list of 6 covered ecosystems with confirmed cache-root logic; (b) link to merged PR; (c) SC verification pass/fail per SC. Add `memory/reference_cache_probe_resolver.md` auto-memory entry linking the milestone's SoT paths + confidence contract.

---

## Dependencies

**Phase order** (blocking):

1. Phase 1 Setup → Phase 2 Foundational → Phase 3+ User Stories (parallel-optional)
2. Phase 2 blocks all user stories (registry + variant + trait impl must exist before per-ecosystem probes plug in)
3. Phase 6 requires US1 + US2 + US3 complete (integration test needs all 6 ecosystems)

**Task-level**:

- T004+T005 (variant + confidence mapping) block T006–T013
- T006+T007+T008 can run in parallel (different files); T009+T010 depend on T007 + T004
- T010 depends on T004 (`LocalCacheHit` variant must exist)
- T013 depends on T004+T006+T010+T011 (parity gate needs full registration)
- T014–T019 (US1) depend on T013 (foundational infra ready)
- T021–T026 (US2) depend on T013
- T028–T035 (US3) depend on T013
- T037+T038+T039 depend on all US1+US2+US3 tasks complete
- T042 depends on T041 pre-PR gate

Fixture creation within each US phase can run in parallel with the reader-modification tasks (different files).

## Parallel execution examples

### Phase 2 foundational

T006 + T007 + T008 all in parallel (registry entry, resolver struct, dispatch enum — 3 files).

```bash
task T006 & task T007 & task T008
wait

# Sequential:
task T004
task T005
task T009
task T010
task T011
task T012
task T013
```

### Per-user-story probe batches

Within a story, per-ecosystem impl + test tasks all touch distinct files → all [P].

- **US1**: T014 + T016 in parallel (Maven + Go impl); T015 + T017 in parallel (their unit tests). T018 + T019 sequential.
- **US2**: T021 + T023 parallel; T022 + T024 parallel. T025 + T026 sequential.
- **US3**: T028 + T031 parallel; T029 + T030 + T032 + T033 parallel. T034 + T035 sequential.

## Implementation strategy — MVP scope

**MVP = Phase 1 + Phase 2 + Phase 3 (US1)**

Ships:
- `LocalCacheHit` variant + parity extractor
- `CacheProbeResolver` registered at priority 92
- Maven + Golang probes with unit tests
- US1-scoped cross-ecosystem integration test
- 3 passing tests total

**Incremental delivery**:

- **PR 1 (MVP)**: T001–T020 (Setup + Foundational + US1). ~20 tasks, ~250 LOC.
- **PR 2 (US2)**: T021–T027. ~7 tasks, ~150 LOC.
- **PR 3 (US3)**: T028–T036. ~9 tasks, ~250 LOC (metadata reads add complexity).
- **PR 4 (Polish)**: T037–T043. ~7 tasks, ~200 LOC (integration + benchmark + docs).

## Task summary

| Phase | Count | Purpose |
|---|---|---|
| Phase 1 Setup | 3 | Substrate verification + module scaffold |
| Phase 2 Foundational | 10 | Variant + registry + trait skel + parity + emission wire-up |
| Phase 3 US1 (P1) | 7 | Maven + Go probes + tests + integration |
| Phase 4 US2 (P2) | 7 | Cargo + Ruby probes + tests + integration |
| Phase 5 US3 (P3) | 9 | npm + Python (metadata reads) + tests + integration |
| Phase 6 Polish | 7 | Cross-cutting tests + benchmark + docs + close-out |
| **Total** | **43** | |

## Format validation

- ✅ Every task starts with `- [ ]` markdown checkbox
- ✅ Every task has a sequential T-ID (T001–T043)
- ✅ Every task in Phase 3+ has a `[US1]` / `[US2]` / `[US3]` label
- ✅ Setup + Foundational + Polish tasks have NO story label
- ✅ Parallelizable tasks marked `[P]`
- ✅ Every task includes an exact file path
