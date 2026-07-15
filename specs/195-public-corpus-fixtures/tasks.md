---
description: "Task list for m195 — Public SBOM Regression Corpus (6 public targets, hybrid two-layer invariants, nightly + workflow_dispatch CI)"
---

# Tasks: Public SBOM Regression Corpus

**Input**: Design documents from `/specs/195-public-corpus-fixtures/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/corpus-harness.md, quickstart.md

**Tests**: Included per mikebom idiom — every corpus target IS a test.

**Organization**: 5 user stories (3 P1: regression guardrails / cross-ecosystem coverage / public-only sources; 2 P2: reproducible pinning / opt-in execution). Foundational infra (data types + cache + harness) is shared across all stories. Per-target scan+assert work is per-story (US1 seeds the harness with the Go target; US2 adds the remaining 5 targets).

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (distinct files, no dependency on incomplete tasks)
- **[Story]**: US1 / US2 / US3 / US4 / US5
- File paths absolute where non-obvious; workspace root is `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

- **mikebom-cli crate tests**: `mikebom-cli/tests/public_corpus.rs`, `mikebom-cli/tests/public_corpus/*.rs`
- **Golden fixtures**: `mikebom-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`
- **Scripts**: `scripts/corpus/`
- **CI**: `.github/workflows/public-corpus.yml`
- **Feature spec dir**: `specs/195-public-corpus-fixtures/`

---

## Phase 1: Setup

**Purpose**: Sink the directory scaffolding + workflow file skeleton before any implementation. All follow-up phases can proceed against a stable layout.

- [X] T001 Create `mikebom-cli/tests/public_corpus/` sub-directory (will hold `mod.rs`, `manifest.rs`, `layer1_assertions.rs`, `layer2_golden.rs`, `cache.rs`, `harness.rs`).
- [X] T002 [P] Create `mikebom-cli/tests/fixtures/public_corpus/` directory with a `.gitkeep` placeholder (per-target sub-dirs created later at T020 / T024-T030 / T032).
- [X] T003 [P] Create `scripts/corpus/` directory with a `.gitkeep` placeholder (scripts added at T039 and T042).
- [X] T004 Create `mikebom-cli/tests/public_corpus.rs` file with a minimal `#[test]` that always passes (skeleton — will get its real tests added per US1 / US2). This ensures `cargo test --test public_corpus` finds the target from the very first commit.
- [X] T005 Verify baseline: `cargo test --test public_corpus` — MUST pass with 1 skeleton test.

**Checkpoint**: Directory tree in place, empty-but-valid test binary compiles.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Types + helpers used by every user story. All Layer 1 assertion functions (US1 + US2) depend on the `EmittedSboms` and `AssertionFailure` types; every target's `#[test]` depends on the env-gate + cache + binary-invocation helpers.

⚠️ **CRITICAL**: Must complete before Phase 3 (US1). Blocks all user story work.

### Types (data-model.md Entities 1, 2, 3, 5)

- [X] T006 In `mikebom-cli/tests/public_corpus/manifest.rs` (new file): declare the `CorpusTarget`, `SourceKind`, `PinnedRef`, `Ecosystem` types per data-model.md Entity 1. Include the `pub const TARGETS: &[CorpusTarget] = &[]` empty slice placeholder — filled in per-target at T017 / T023 / T025 / T027 / T029 / T031.
- [X] T007 [P] In `mikebom-cli/tests/public_corpus/harness.rs` (new file): declare the `EmittedSboms` + `EmittedPaths` types per data-model.md Entity 2.
- [X] T008 [P] In `mikebom-cli/tests/public_corpus/harness.rs`: declare the `AssertionFailure` + `FailureFormat` types per data-model.md Entity 3, including the `Display` impl per contracts/corpus-harness.md "Diagnostic Output Format".
- [X] T009 [P] In `mikebom-cli/tests/public_corpus/harness.rs`: declare the `CorpusInfraError` enum per data-model.md Entity 5, including the 5 variants (`GitClone`, `OciPull`, `SbomEmission`, `CacheIo`, `OciToolMissing`). **AND** implement `Display for CorpusInfraError` matching the contracts/corpus-harness.md "Diagnostic Output Format" `underlying error: <stderr excerpt, capped at 500 chars>` block shape. Per FR-012, this Display is what distinguishes corpus-infra failure output from mikebom-regression failure output (which is rendered via `AssertionFailure::Display` at T008).

### Cache (data-model.md Entity 4, research §R3)

- [X] T010 In `mikebom-cli/tests/public_corpus/cache.rs` (new file): implement `CorpusCacheKey` + `CorpusCacheDir` per data-model.md Entity 4. Include `source_id_short(url)` helper (first 16 hex chars of sha256).
- [X] T011 In `mikebom-cli/tests/public_corpus/cache.rs`: implement `ensure_hydrated(&CorpusTarget) -> Result<PathBuf, CorpusInfraError>` per data-model.md Entity 4 state machine. Handle both `SourceKind::Git` (git clone + checkout + `.corpus-pin-verified` marker) and `SourceKind::OciImage` (docker pull; marker only — image lives in Docker daemon storage). Honor `MIKEBOM_CORPUS_CACHE_DIR` override per contracts/corpus-harness.md.

### Harness (research §R2, contracts/corpus-harness.md)

- [X] T012 In `mikebom-cli/tests/public_corpus/harness.rs`: implement `scan_target(&CorpusTarget) -> Result<EmittedSboms, CorpusInfraError>`: (a) call `ensure_hydrated`, (b) spawn `env!("CARGO_BIN_EXE_mikebom")` with the correct args per target's source kind (git → `--path <clone-dir>`; image → `--image <ref>@<digest>`), (c) pass `--format cyclonedx-json,spdx-2.3-json,spdx-3-json --output <fmt>=<tmpdir>/<name>.<ext>` per format, (d) parse each output into `serde_json::Value`, (e) return `EmittedSboms`.
- [X] T013 [P] In `mikebom-cli/tests/public_corpus/harness.rs`: implement `env_gate() -> bool` — returns `false` (skip) when `MIKEBOM_RUN_PUBLIC_CORPUS != "1"`. Include a `skip_oci_gate() -> bool` for the `MIKEBOM_CORPUS_SKIP_OCI` behavior per contracts/corpus-harness.md.

### Layer 2 golden diff (research §R5)

- [X] T014 In `mikebom-cli/tests/public_corpus/layer2_golden.rs` (new file): implement `compare_golden(target_name, format, actual_path, golden_path) -> Result<(), AssertionFailure>` reusing the existing masking helpers from `mikebom-cli/tests/cdx_regression.rs` / `spdx_regression.rs` / `spdx3_regression.rs`. When `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is set, write the golden instead of comparing. Emit `.actual.json` sibling file on diff failure per contracts/corpus-harness.md.

### Wire it up

- [X] T015 In `mikebom-cli/tests/public_corpus.rs`: expand from the T004 skeleton to a proper test entry that (a) declares `mod public_corpus;` (referring to `tests/public_corpus/mod.rs`), (b) creates a `mod.rs` re-exporting the manifest and harness. This is the compilation gate — after T015 the crate compiles with all foundational types + helpers wired.

**Checkpoint**: `cargo test --test public_corpus --no-run` compiles clean. All shared infra ready. No corpus targets yet.

---

## Phase 3: User Story 1 — Class-of-bug regression guardrails (MVP) (Priority: P1)

**Goal**: One corpus target (`go-cobra`) end-to-end catches an intentional revert of m194 US1 (Go stdlib edge synthesis). Proves the harness works; every subsequent target is straightforward duplication.

**Independent Test**: `git revert --no-commit` the m194-US1 stdlib-edge commit; run `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus corpus_go_cobra`; observe an `AssertionFailure` naming `stdlib-edge-present` per data-model.md Entity 3 and quickstart Reproducer 6.

### Implementation for User Story 1

- [X] T016 [US1] In `mikebom-cli/tests/public_corpus/layer1_assertions.rs` (new file): implement `go_cobra_layer1(sboms) -> Result<(), AssertionFailure>` covering: (a) `mikebom:graph-completeness == "complete"` across all three formats, (b) at least one edge from `pkg:golang/github.com/spf13/cobra@v*` → `pkg:golang/stdlib@v*` in CDX `.dependencies[]` (per m194 US1), (c) main-module PURL present. Include structured `AssertionFailure` returns per research §R4.
- [X] T017 [US1] In `mikebom-cli/tests/public_corpus/manifest.rs`: append the `go-cobra` `CorpusTarget` const entry — `source: Git { clone_url: "https://github.com/spf13/cobra" }`, `pinned: Sha { hex: <v1.9.1 SHA> }`, `ecosystem: Ecosystem::Go`, `layer1: go_cobra_layer1`. Resolve the actual SHA via `git ls-remote --tags https://github.com/spf13/cobra v1.9.1^{}` at authoring time and pin it verbatim.
- [X] T018 [US1] In `mikebom-cli/tests/public_corpus.rs`: add the `#[test] fn corpus_go_cobra()` test function — early-return if `env_gate()` returns false; otherwise `scan_target(&TARGETS[<index>])?`, then apply Layer 1, then Layer 2 for each format.
- [X] T019 [US1] Regen `go-cobra` goldens: `MIKEBOM_RUN_PUBLIC_CORPUS=1 MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1 cargo test --test public_corpus corpus_go_cobra` — commits under `mikebom-cli/tests/fixtures/public_corpus/go-cobra/{cdx,spdx-2.3,spdx-3}.json`.
- [ ] T020 [US1] Verify SC-001 (m194 US1 revert trip): manually revert the m194 US1 stdlib-edge commit locally, rebuild `mikebom` release binary, re-run the go-cobra corpus test, observe the expected `stdlib-edge-present` failure. Reset the working tree after. Record findings in `specs/195-public-corpus-fixtures/scratch/us1-verification.txt`.

**Checkpoint**: One target working end-to-end. US1 acceptance scenarios all pass. Harness proven.

---

## Phase 4: User Story 2 — Cross-ecosystem coverage (Priority: P1)

**Goal**: 5 more targets (rust-ripgrep, npm-express, python-flask, maven-guice, image-postgres16) round out ecosystem coverage per FR-002 and SC-002. Each is independent and can proceed in parallel.

**Independent Test**: List corpus targets; confirm at least one per ecosystem in `{Go, Rust, Npm, Python, JavaMaven, PolyglotImage}`. Breaking any single reader (e.g., dropping cargo main-module emission) causes exactly that ecosystem's target to fail; others pass.

### Rust — ripgrep

- [X] T021 [P] [US2] In `layer1_assertions.rs`: implement `rust_ripgrep_layer1` — assertions: graph-completeness `complete`; main-module PURL `pkg:cargo/ripgrep@<v>`; at least one cargo-workspace-peer edge; at least one dev-dependency scope tag on a test-only dep.
- [X] T022 [P] [US2] In `manifest.rs`: append `rust-ripgrep` `CorpusTarget` — source `https://github.com/BurntSushi/ripgrep`, pinned to `14.1.1` SHA, ecosystem `Rust`.
- [ ] T023 [US2] In `public_corpus.rs`: add `#[test] fn corpus_rust_ripgrep()` mirroring T018. Regen goldens.

### npm — express

- [X] T024 [P] [US2] In `layer1_assertions.rs`: implement `npm_express_layer1` — assertions: graph-completeness `complete`; main-module PURL `pkg:npm/express@<v>`; at least one lockfile-tier component; at least one `TestDependsOn`-typed relationship in SPDX 2.3 for a test dep.
- [X] T025 [US2] In `manifest.rs`: append `npm-express` `CorpusTarget` — source `https://github.com/expressjs/express`, pinned to `5.1.0` SHA, ecosystem `Npm`.
- [ ] T026 [US2] In `public_corpus.rs`: add `#[test] fn corpus_npm_express()`. Regen goldens.

### Python — flask

- [X] T027 [P] [US2] In `layer1_assertions.rs`: implement `python_flask_layer1` — assertions: graph-completeness `complete`; main-module PURL `pkg:pypi/flask@<v>`; at least one `optional`-lifecycle-scope dep from `[project.optional-dependencies]` (m183).
- [X] T028 [US2] In `manifest.rs`: append `python-flask` `CorpusTarget` — source `https://github.com/pallets/flask`, pinned to `3.1.2` SHA, ecosystem `Python`.
- [ ] T029 [US2] In `public_corpus.rs`: add `#[test] fn corpus_python_flask()`. Regen goldens.

### Java/Maven — guice

- [X] T030 [P] [US2] In `layer1_assertions.rs`: implement `maven_guice_layer1` — assertions: graph-completeness `complete`; multiple main-modules (one per Maven module: core + extensions) per m070; at least one Maven `optional` dep per m184.
- [X] T031 [US2] In `manifest.rs`: append `maven-guice` `CorpusTarget` — source `https://github.com/google/guice`, pinned to `7.0.0` SHA, ecosystem `JavaMaven`.
- [ ] T032 [US2] In `public_corpus.rs`: add `#[test] fn corpus_maven_guice()`. Regen goldens.

### Polyglot image — postgres:16

- [X] T033 [P] [US2] In `layer1_assertions.rs`: implement `image_postgres16_layer1` — assertions per research §R8: graph-completeness `partial` with reason `TransitiveEdgesUnresolvable { ecosystems: ["generic", "golang"] }`; at least one `pkg:deb/*` component; at least one `pkg:golang/*` component (Go bin extraction from `gosu`); `file-tier` components present but excluded from orphan accounting per m194 US3.
- [X] T034 [US2] In `manifest.rs`: append `image-postgres16` `CorpusTarget` — source `docker.io/library/postgres:16` (resolved to `sha256:<digest>` at authoring time), ecosystem `PolyglotImage`.
- [ ] T035 [US2] In `public_corpus.rs`: add `#[test] fn corpus_image_postgres16()` — respects `MIKEBOM_CORPUS_SKIP_OCI` per contracts/corpus-harness.md. Regen goldens (may take ~10 min per SC-005 image slice).

### Cross-ecosystem coverage assertion

- [X] T036 [US2] In `mikebom-cli/tests/public_corpus/manifest.rs`: add `#[test] fn cross_ecosystem_coverage_check()` — asserts every `Ecosystem` enum variant appears in `TARGETS` at least once (satisfies FR-002 / SC-002 in-tree).

**Checkpoint**: 6 targets operational; ecosystem coverage verified by unit test.

---

## Phase 5: User Story 3 — Public-only source of truth (Priority: P1)

**Goal**: Enforce the no-Kusari-hostnames constraint in-tree so the constraint can't silently regress via future manifest edits.

**Independent Test**: Add a fake `kusari` substring to the manifest, run the audit test, observe failure. Remove the fake substring, re-run, observe pass.

### Implementation for User Story 3

- [X] T037 [US3] In `mikebom-cli/tests/public_corpus/manifest.rs`: add `#[test] fn public_only_audit()` — iterates every `CorpusTarget` in `TARGETS`, extracts source URL / image ref via `SourceKind` match, asserts none contain `kusari` (case-insensitive) per FR-003. Fails with a clear diagnostic naming any offending target.
- [X] T038 [US3] In `mikebom-cli/tests/public_corpus/manifest.rs`: add `#[test] fn public_hostname_allowlist()` — parses each source URL / image ref, asserts hostname matches one of: `github.com`, `docker.io`, `registry-1.docker.io`, `ghcr.io`. Fails with a diagnostic when an unexpected hostname sneaks in.
- [X] T038a [US3] In `mikebom-cli/tests/public_corpus/manifest.rs`: add `#[test] fn no_credentials_required()` — for each `Git` target, spawn `git ls-remote <clone_url>` with `GIT_TERMINAL_PROMPT=0`, `GIT_ASKPASS=/bin/false`, `SSH_ASKPASS=/bin/false`, and no `HOME` inheritance (empty temp dir as `HOME`); assert the process exits `0`. Provides direct evidence for FR-004 (no auth credentials required) — the hostname allowlist in T038 is a necessary-but-not-sufficient check because a public hostname could still gate specific repos behind auth. Gated behind the same `MIKEBOM_RUN_PUBLIC_CORPUS=1` env var per contracts/corpus-harness.md. `OciImage` targets skip this check (public Docker Hub images don't require credentials by definition when pulled by digest).

**Checkpoint**: Manifest hygiene enforced by tests. Future edits that introduce private hostnames trip the audit.

---

## Phase 6: User Story 4 — Reproducible pinning (Priority: P2)

**Goal**: Two consecutive corpus runs against the same pinned manifest produce byte-identical SBOMs (SC-006). Pin refresh is a documented human-in-the-loop workflow, not auto-applied.

**Independent Test**: Run corpus twice in succession, compare per-target emitted SBOMs byte-identically after masking — MUST match.

### Implementation for User Story 4

- [X] T039 [US4] In `scripts/corpus/refresh-pins.sh` (new file): implement the refresh-pins helper per research §R7 + contracts/corpus-harness.md "Refresh Helper Contract". Reads `manifest.rs` const table via a `grep`-based parser (targets are declared with a stable syntax; this doesn't need a full Rust parser). For each git target, runs `git ls-remote --tags <url> <tag>^{}` and extracts SHA. For each OCI target, runs `docker manifest inspect <image>:<tag>` and extracts digest. Prints a unified diff of proposed manifest changes to stdout. Does NOT auto-commit.
- [X] T040 [US4] Make `scripts/corpus/refresh-pins.sh` executable (`chmod +x`) and add a short usage-comment header referencing FR-008.
- [X] T041 [P] [US4] Add `#[test] fn byte_identity_across_two_runs()` in `mikebom-cli/tests/public_corpus/mod.rs` — gated by an additional env var `MIKEBOM_RUN_BYTE_IDENTITY_SUITE=1` (opt-in-within-opt-in — this test doubles corpus wall-clock). Runs each target twice, compares byte-for-byte after masking. Provides SC-006 evidence.

**Checkpoint**: Pin refresh is a scripted-but-reviewed workflow. Byte-identity opt-in test provides evidence for SC-006 on demand.

---

## Phase 7: User Story 5 — Opt-in execution (Priority: P2)

**Goal**: The corpus MUST NOT run on the default `cargo test` / `./scripts/pre-pr.sh` flow. Nightly CI + manual dispatch is the primary invocation path per Q2 clarification.

**Independent Test**: `./scripts/pre-pr.sh` completes with unchanged wall-clock (SC-004). `cargo test` without `MIKEBOM_RUN_PUBLIC_CORPUS=1` skips every corpus test with a `println!`. `gh workflow run public-corpus.yml --ref my-pr-branch` triggers the workflow against a branch.

### Implementation for User Story 5

- [X] T042 [US5] In `.github/workflows/public-corpus.yml` (new file): implement the workflow per research §R6 + contracts/corpus-harness.md "CI Workflow Contract". Triggers: `schedule: cron: '17 6 * * *'` + `workflow_dispatch: inputs.branch: type: string, default: main`. Runner: `ubuntu-latest`. Steps: SHA-pinned `actions/checkout` (matches memory `feedback_sha_pin_before_dependabot`), `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, `cargo build --release -p mikebom`, `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus --release -- --nocapture --test-threads=3`, upload-artifact for summary (always) and emitted-sboms (on failure only).
- [X] T043 [US5] In `mikebom-cli/tests/public_corpus.rs`: add `#[test] fn env_gate_skips_when_unset()` — a paranoia test that mocks `MIKEBOM_RUN_PUBLIC_CORPUS` unset and verifies `env_gate()` returns false. Prevents the gate silently drifting.
- [ ] T044 [US5] Verify SC-004 (delta ≤ 5s): measure `./scripts/pre-pr.sh` wall-clock pre-feature (from a clean `git stash` of these changes) vs post-feature. Record both timings in `specs/195-public-corpus-fixtures/scratch/scaffolding-cost.txt` per the SC's baseline-storage requirement. Any regression > 5s in the pre-PR gate blocks the PR per the tightened SC-004 threshold.
- [X] T045 [US5] Create `scripts/corpus/regen-goldens.sh` (new file) — one-liner wrapper: `MIKEBOM_RUN_PUBLIC_CORPUS=1 MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1 cargo test --test public_corpus --release -- --nocapture "$@"`. Makes goldens-regen discoverable and consistent per quickstart Reproducer 4.

**Checkpoint**: CI wired up; opt-in gates verified; regen ergonomics documented.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, CLAUDE.md refresh, pre-PR gate.

- [X] T046 [P] Update `CLAUDE.md` "Recent Changes" section (via `.specify/scripts/bash/update-agent-context.sh claude` if not already run during /speckit-plan) to reference 195-public-corpus-fixtures.
- [X] T047 [P] Add a short README section to `mikebom-cli/tests/public_corpus/mod.rs` (top-of-file `//!` doc comment) referencing spec + quickstart + contracts, so `cargo doc --tests` produces browseable inline docs for the corpus.
- [ ] T048 [P] Cross-reference from `mikebom-cli/CLAUDE.md` (if exists) OR from `mikebom-cli/tests/README.md` (create if needed) to the public-corpus contract, explaining "these tests are opt-in; see specs/195".
- [X] T049 Run `./scripts/pre-pr.sh` — MUST pass clean per memory `feedback_prepr_gate_full_output`. Confirms US5 opt-in gate works (corpus tests skip when unset).
- [ ] T050 Run one full corpus invocation end-to-end: `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus --release -- --nocapture` — MUST pass all 6 targets. Provides evidence for SC-005 wall-clock (< 30 min cold, < 5 min warm).

**Checkpoint**: All 5 user stories delivered; docs updated; pre-PR gate green; full corpus green.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T002/T003/T004 [P] → T005. No dependencies.
- **Foundational (Phase 2)**: T006 → T007/T008/T009 [P] → T010 → T011 → T012 → T013 [P] → T014 → T015. Depends on Setup. **BLOCKS all user story phases.**
- **US1 (Phase 3)**: Depends on Foundational. Sequential internally (single Go target).
- **US2 (Phase 4)**: Depends on Foundational. Per-target 3-task chains (layer1 + manifest-add + wire+regen) are internally sequential but ecosystems are parallel across each other.
- **US3 (Phase 5)**: Depends on Foundational + at least one target in place (US1 or US2). Two independent tests.
- **US4 (Phase 6)**: Depends on Foundational + at least one target (for the byte-identity test to have something to run against).
- **US5 (Phase 7)**: Depends on Foundational; T042 (CI workflow) depends on the manifest+wire being complete for whichever targets have shipped so far (can ship CI incrementally).
- **Polish (Phase 8)**: Depends on all user story phases done.

### User Story Dependencies

- **US1 ⊂ US2**: US1 is the first target implementation; US2 adds the remaining 5. US2 tasks reuse US1's harness patterns.
- **US3 / US4 / US5 all depend on Foundational + at least US1** — but do NOT depend on each other. Can proceed in parallel across those three stories once US1 is done.

### Parallel Opportunities

- Phase 1: T002/T003/T004 all `[P]` after T001.
- Phase 2: T007/T008/T009 all `[P]` (all in the same file `harness.rs` — but they're distinct type declarations, no code sharing).
- Phase 4: Per-ecosystem 3-task chains (T021+T022+T023, T024+T025+T026, T027+T028+T029, T030+T031+T032, T033+T034+T035) — 5 chains that are parallel across each other.
- Phase 5: T037 / T038 `[P]` (both are new tests in the same file — parallel-authorable).
- Phase 6: T041 `[P]` with US5 work.
- Phase 8: T046 / T047 / T048 all `[P]`.

---

## Parallel Example: US2 in parallel (5 ecosystem lanes)

```bash
# After Foundational (Phase 2) completes, 5 devs work in parallel:
Developer A: T021 → T022 → T023 (Rust — ripgrep)
Developer B: T024 → T025 → T026 (npm — express)
Developer C: T027 → T028 → T029 (Python — flask)
Developer D: T030 → T031 → T032 (Maven — guice)
Developer E: T033 → T034 → T035 (Image — postgres:16)

# Then converge on:
T036 (cross-ecosystem coverage check)

# Then US3 / US4 / US5 in parallel:
US3: T037 / T038
US4: T039 → T040 → T041
US5: T042 → T043 → T044 → T045

# Then Polish (Phase 8) in parallel:
T046 / T047 / T048 → T049 → T050
```

---

## Implementation Strategy

### MVP (US1 only — first landable slice)

1. Phase 1 (Setup) → Phase 2 (Foundational) → Phase 3 (US1).
2. MVP delivers **one** working corpus target (`go-cobra`). Proves the harness. Ships as a standalone PR.
3. Subsequent PRs add each remaining ecosystem target (US2 lanes A–E). Each is independently reviewable.

### Single-PR delivery (matches m190 / m191 / m192 / m194 shape)

Land Phases 1–8 in a single PR titled `impl(195): public SBOM regression corpus (6 targets, hybrid two-layer invariants)`. Commit granularity per phase, reviewer digestibility per US.

**Trade-off consideration**: Single PR = higher review load; 6-PR split = clean history but more coordination. Since m194 shipped as a single PR with 4 user stories worth of work, this milestone follows the same convention.

### Incremental CI wiring (optional)

If the initial PR ships with only US1's target, CI wiring (T042) can defer to a follow-up PR that adds it once US2 targets are in place. This keeps the first PR under 500 LOC diff.

---

## Notes

- Total tasks: 51 across 8 phases (T038a added post-analyze for FR-004 credentials coverage).
- US1: 5 tasks (T016–T020). US2: 16 tasks (T021–T036). US3: 3 tasks (T037, T038, T038a). US4: 3 tasks. US5: 4 tasks. Setup/Foundational/Polish: 20 tasks.
- Every `[P]` task edits a distinct file OR distinct sections of the same file that don't overlap.
- **Zero new Cargo dependencies** (FR: research §R2/§R5 audit satisfied).
- **Zero new `mikebom:*` annotations** (research §R2 audit satisfied — corpus consumes, doesn't extend).
- **Public-only constraint** enforced in-tree by T037 + T038 (US3) — future manifest edits that violate FR-003 are blocked at test time.
- **Opt-in gate** enforced in-tree by T043 (US5) — the gate can't silently regress to always-on.
- **Pin-refresh workflow** is human-in-the-loop by design (T039 script + FR-008) — no auto-commit paths exist.
