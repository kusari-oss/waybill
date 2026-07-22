---

description: "Task list for m215 — auto-split monorepo SBOM into per-subproject SBOMs via `--split` flag"
---

# Tasks: Auto-split monorepo SBOM into per-subproject SBOMs

**Input**: Design documents from `/specs/215-sbom-auto-split/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Test tasks INCLUDED. The user stories are essentially verification perspectives on shared infrastructure; the tests are what prove each perspective is satisfied. Integration tests + golden fixtures dominate.

**Organization**: 4 user stories from spec.md. US1 + US2 (both P1) exercise the same core BFS-projection code path over different fixture shapes (single-ecosystem workspace vs multi-ecosystem project). US3 (P2) validates the manifest as an index. US4 (P2) validates shared-dep duplication semantics.

**Solo-dev sequencing note**: US1 through US4 all depend on the SAME Phase 2 foundational infrastructure. Once Foundational is done, US1/US2/US3/US4 are largely test-tasks and can be tackled in any order — the recommended order is US1 → US3 → US4 → US2 because US2 needs a new fixture that takes longer to author.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1, US2, US3, US4)
- File paths absolute-from-repo-root

---

## Phase 1: Setup

**Purpose**: Sanity checks + branch verification.

- [X] T001 Verify branch `215-sbom-auto-split` is checked out and up-to-date with `main` post-m214 merge. Confirm HEAD is the plan-phase commit via `git log -1 --oneline`.
- [X] T002 Verify pre-feature single-SBOM emit path is green: `cargo test -p waybill --test cdx_regression --test spdx_regression --test spdx3_regression`. Should print `11 passed 0 failed` × 3 suites. Split-mode work must not regress these.
- [X] T003 Read `waybill-cli/src/generate/root_selector.rs` (m127) and `waybill-cli/src/scan_fs/mod.rs:2367+` (m201 workspace-root disambiguation) to understand the existing `waybill:is-workspace-root` annotation flow. Note where the annotation is written — this is the boundary-enumeration input source per research R1.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The core `SubprojectRoot` enumeration + `SplitProjection` BFS + `SplitManifest` types + `--split` CLI flag. Every user story depends on these. NO user-story work can begin until Phase 2 completes.

**⚠️ CRITICAL**: `cargo check --workspace` MUST pass at Phase 2 completion.

- [X] T004 Add `--split` boolean flag to `ScanArgs` in `waybill-cli/src/cli/scan_cmd.rs` per contracts/cli-flag.md. Include `#[arg(long)]` derive + help-text matching the contract. Add the interaction-matrix validation: `--split` + `--output <file>` → hard error at CLI parse-time with the exact error text from contracts/cli-flag.md.
- [X] T005 [P] Add `SplitManifest` + `SplitEntry` types in `waybill-cli/src/generate/split_manifest.rs` (NEW file, ~100 LOC) per data-model.md E3. Use `serde::{Serialize, Deserialize}` derives. Use `BTreeMap<String, String>` for `SplitEntry.files` (deterministic key ordering). Include the `$schema` v1 URL constant. Unit tests: round-trip serde + byte-identity under fixed inputs.
- [X] T006 [P] Add `SubprojectRoot` + `SplitProjection` internal types in `waybill-cli/src/generate/split.rs` (NEW file, foundational-only ~50 LOC of type defs + module skeleton — the BFS + enumeration logic land in later tasks) per data-model.md E1 + E2. Types are `pub(crate)`.
- [X] T007 Register the new `split` + `split_manifest` modules in `waybill-cli/src/generate/mod.rs`. Add `pub(crate) mod split; pub(crate) mod split_manifest;`. Do NOT yet wire the emit-dispatch through them; that's US1's T012.
- [X] T008 Implement `enumerate_workspace_roots(resolved_components: &[ResolvedComponent]) -> Vec<SubprojectRoot>` in `waybill-cli/src/generate/split.rs` per research R1. Filter to components with `extra_annotations["waybill:is-workspace-root"] == true`, project into `SubprojectRoot` structs, sort lexicographically by derived `subproject_id`. Include the placeholder-PURL filter (skip synthetic-empty-PURL roots).
- [X] T009 Implement `project_for_root(root: &SubprojectRoot, components: &[ResolvedComponent], relationships: &[Relationship]) -> SplitProjection` per research R2. BFS from root's `bomref` over dep-edge `RelationshipType` variants. Include the root itself in `components[0]`. Filter `relationships` to those where both endpoints are in the reachable set (self-contained per FR-007). `shared_deps_count` initialized to 0 (populated post-hoc in T010).
- [X] T010 Implement `compute_shared_deps(projections: &mut [SplitProjection])` — one linear pass over the union of all component PURLs; for each PURL, count how many projections contain it; write `shared_deps_count` into each projection reflecting how many of ITS components are also in another projection. Returns `(total_unique_components, aggregate_shared_dep_count)` for manifest emission (T014).
- [X] T011 Implement `filename_for(root: &SubprojectRoot, format: OutputFormat, collisions: &BTreeMap<String, Vec<PathBuf>>) -> String` per contracts/filename-convention.md. Handles slug derivation (namespace-prefix + char substitution + truncation + lowercase), ecosystem-name mapping, format-extension mapping, collision-fallback via SHA-8-char, filesystem-safety guards (reserved Windows names, ASCII-only). Unit tests cover all rows of the example-filenames table + reserved-name edge cases.

**Checkpoint**: `cargo check -p waybill --all-targets` clean; new modules registered but not yet wired into emit dispatch.

---

## Phase 3: User Story 1 - Monorepo owner emits per-workspace-member SBOMs (Priority: P1) 🎯 MVP

**Goal**: Single-ecosystem workspace (e.g., Cargo workspace with N members) with `--split` emits N sub-SBOMs + 1 manifest. Each sub-SBOM's root is the specific member; component set is BFS-scoped. Union across sub-SBOMs equals pre-feature single-SBOM component set (SC-004).

**Independent Test**: On the m212 `two_binaries_diverge` cargo-workspace fixture (4 members): `waybill sbom scan --path <fixture> --split --output-dir <tmp>` emits exactly 4 sub-SBOMs + 1 manifest. Golden diff-check against `waybill-cli/tests/fixtures/golden/split/cargo-workspace/{4 CDX files, split-manifest.json}`.

### Implementation for User Story 1

- [X] T012 [US1] Wire the emit-dispatch in `waybill-cli/src/generate/mod.rs` to fan out on `args.split == true`: enumerate roots (T008), project each (T009), compute shared-dep counts (T010), then for each projection × each requested format, invoke the existing emit function with narrowed component + relationship inputs. Write each output via `filename_for` (T011). When N == 0 workspace roots, fall through to pre-feature single-SBOM emit + WARN log line per R8.
- [ ] T013 [US1] Wire the sub-SBOM serial-number deterministic path per R5. When `WAYBILL_FIXED_TIMESTAMP` is set, sub-SBOM `serialNumber` becomes `urn:uuid:<sha256(subproject_root_purl + fixed_ts)>[..32]`. When absent, fresh UUID per sub-SBOM (matching pre-feature single-SBOM behavior for the whole-repo case). Apply in the CDX, SPDX 2.3, and SPDX 3 emit paths — likely one shared helper `sub_sbom_serial(root_purl: &Purl) -> String` in `waybill-cli/src/generate/split.rs`.
- [X] T014 [US1] Implement `SplitManifest` emission in `waybill-cli/src/generate/split.rs`. After all N × M sub-SBOMs are written, build a `SplitManifest` struct (E3) from the projections + emitted-file list, serialize as pretty JSON via `serde_json::to_string_pretty`, write to `<output-dir>/split-manifest.json`. Under `WAYBILL_FIXED_TIMESTAMP`, `generated_at` uses the fixed value; otherwise scan-start UTC RFC 3339.

### Test tasks for User Story 1

- [X] T015 [P] [US1] Add unit tests in `waybill-cli/src/generate/split.rs::tests`: (a) `enumerate_workspace_roots` filters correctly on `is-workspace-root` annotation, (b) `project_for_root` BFS reaches root's transitive closure, (c) `project_for_root` excludes unrelated sibling-member components, (d) `compute_shared_deps` counts correctly across 3 projections with known overlaps, (e) `sub_sbom_serial` deterministic under fixed timestamp.
- [X] T016 [P] [US1] Add unit tests in `waybill-cli/src/generate/split_manifest.rs::tests`: (a) round-trip serde (SplitManifest → JSON → parse), (b) empty `entries` case serializes as `entries: []` not null, (c) `files` map key-order deterministic (BTreeMap invariant), (d) `$schema` URL matches contract v1.
- [X] T017 [P] [US1] Add unit tests in `waybill-cli/src/generate/split.rs::tests` for filename generation: covers every row of the contracts/filename-convention.md example table + collision resolution + reserved-name (`con.cargo.cdx.json` → `wb-con.cargo.cdx.json`) + long-slug truncation.
- [X] T018 [US1] Add integration test at `waybill-cli/tests/scan_split_basic.rs` — first scenario: `cargo_workspace_split_emits_one_sbom_per_member`. Uses the m212 `two_binaries_diverge` fixture (4 cargo workspace members). Invokes `waybill sbom scan --path <fixture> --split --output-dir <tmp>` via `Command`. Asserts: 4 CDX files + 1 manifest exist; each SBOM's `metadata.component.purl` matches a distinct member; union of components across the 4 SBOMs equals the pre-feature single-SBOM component set (SC-004 verification). **Also validates SC-006 (per C4 analyze finding)**: each emitted sub-SBOM independently passes the CDX 1.6 JSON schema check via the existing `jsonschema = "0.46"` dev-dep (reuse the loader pattern from `waybill-cli/tests/cdx_regression.rs`). Any sub-SBOM that fails schema validation fails the test — catches split-mode-specific structural bugs that per-format `*_regression` tests don't cover.
- [X] T018a [US1] Add integration test `split_on_single_package_falls_back_to_one_sbom` (verifies FR-009 per C2 analyze finding). Fixture: single-package `Cargo.toml` (no `[workspace]` block, one `[package]` entry). Invokes `waybill sbom scan --path <fixture> --split --output-dir <tmp>`. Asserts: exactly 1 CDX file emitted (slug matches root's PURL name), NO `split-manifest.json` written, exit code 0, WARN log line present in stderr matching pattern `no workspace boundaries detected`.
- [ ] T018b [US1] Add integration test `split_multi_format_emits_N_times_M_files` (verifies FR-008 per C1 analyze finding). Uses the cargo-workspace fixture with 4 members. Invokes `waybill sbom scan --path <fixture> --split --output-dir <tmp> --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json`. Asserts: 4 × 3 = 12 sub-SBOM files exist (4 `.cdx.json` + 4 `.spdx.json` + 4 `.spdx3.json`); each SBOM passes its format's schema validation (CDX 1.6, SPDX 2.3, SPDX 3.0.1); manifest's `entries[].files{}` map contains all 3 format entries per subproject. Per-format schema validation reuses the same `jsonschema` loader as T018.
- [X] T019 [US1] Extend the same integration test file with `cargo_workspace_split_manifest_lists_all_emitted_files`. Asserts: parse `split-manifest.json`, verify `entries` count == 4, each entry's `subproject_id` unique, each entry's `files["cyclonedx-json"]` filename exists on disk, `total_unique_components` equals pre-feature count.
- [ ] T020 [US1] Extend with `cargo_workspace_split_fixed_timestamp_deterministic`. Runs split twice back-to-back under `WAYBILL_FIXED_TIMESTAMP="2026-01-01T00:00:00Z"`; asserts byte-identical output across both runs (all 4 CDX + manifest).
- [ ] T021 [US1] Golden fixture regeneration setup: add `WAYBILL_UPDATE_SPLIT_GOLDENS=1` env-var support in the integration test at `waybill-cli/tests/scan_split_basic.rs` per research R9. Under the env var, the test writes the emitted files as goldens under `waybill-cli/tests/fixtures/golden/split/cargo-workspace/`; without it, byte-diff against the goldens.
- [ ] T022 [US1] Run `WAYBILL_UPDATE_SPLIT_GOLDENS=1 cargo test -p waybill --test scan_split_basic cargo_workspace_split` to generate the initial cargo-workspace golden set. Commit the resulting 4 CDX files + 1 manifest under `waybill-cli/tests/fixtures/golden/split/cargo-workspace/`. Verify diff-purity: goldens contain no absolute paths (use `<WORKSPACE>` placeholder per existing normalize.rs pattern).

**Checkpoint**: Single-ecosystem cargo-workspace split works end-to-end. `cargo test -p waybill --test scan_split_basic` passes green.

---

## Phase 4: User Story 2 - Heterogeneous multi-ecosystem project (Priority: P1)

**Goal**: Project with distinct ecosystem manifests at different subdirs (e.g., `frontend/package.json` + `backend/pyproject.toml`) with `--split` emits one sub-SBOM per detected ecosystem-root. Multi-manifest-per-directory case (npm + pypi in same dir) emits TWO sub-SBOMs per clarification Q2.

**Independent Test**: On a new fixture at `waybill-cli/tests/fixtures/split_heterogeneous/` with npm + pypi + swift subdirs, split scan emits 3 sub-SBOMs with correct ecosystem-appropriate root PURLs.

### Implementation for User Story 2

Nothing new — US2 uses the same code paths as US1. This phase is fixture + test authoring.

### Test tasks for User Story 2

- [X] T023 [US2] Author fixture `waybill-cli/tests/fixtures/split_heterogeneous/` with three subdirs: `frontend/` (npm — `package.json` + `package-lock.json` with 2-3 minimal deps), `backend/` (python — `pyproject.toml` + `poetry.lock` or `uv.lock` with 2-3 deps), `mobile-ios/` (swift — `Package.swift` + `Package.resolved` with 2-3 deps). Fixture kept small (<200 lines total) to keep goldens reviewable.
- [X] T024 [US2] Add integration test `heterogeneous_split_emits_one_sbom_per_ecosystem` in `waybill-cli/tests/scan_split_basic.rs`. Asserts: 3 sub-SBOMs emitted (`frontend.npm.cdx.json`, `backend.pypi.cdx.json`, `mobile-ios.swift.cdx.json`); each SBOM's root PURL uses the ecosystem-appropriate type (`pkg:npm/…`, `pkg:pypi/…`, `pkg:swift/…`); manifest lists all 3.
- [X] T025 [US2] Add integration test `multi_manifest_per_dir_emits_one_sbom_per_ecosystem` in `waybill-cli/tests/scan_split_basic.rs`. Fixture: one directory containing BOTH `package.json` AND `pyproject.toml` (with distinct package names). Asserts: 2 sub-SBOMs emitted (one npm, one pypi); per Clarification Q2 hard-decided as "one SBOM per ecosystem manifest".
- [ ] T026 [US2] Run `WAYBILL_UPDATE_SPLIT_GOLDENS=1 cargo test -p waybill --test scan_split_basic heterogeneous` to generate the heterogeneous golden set. Commit 3 CDX + 1 manifest under `waybill-cli/tests/fixtures/golden/split/heterogeneous/`.
- [X] T026a [US2] Add nested-workspace integration test `split_nested_workspace_emits_all_boundaries` (verifies FR-010 per C3 analyze finding). Fixture: cargo workspace with 2 members, where one member contains an npm sub-workspace with 2 packages. Reuse fixture-authoring pattern from T023. Asserts: 4 total sub-SBOMs emitted (2 outer cargo members + 2 inner npm packages under one of them); manifest lists all 4; no subprojects "swallowed" by outer boundary. **Scope guard**: this test is small — reusing the T023 fixture-authoring pattern with a minimal 4-manifest layout. If fixture authoring exceeds 1 hour of work, defer to a follow-up issue rather than blocking m215 merge.

**Checkpoint**: Both P1 stories delivered. Any P1 user (monorepo owner, multi-ecosystem team) can now use `--split` productively. **This is the mergeable MVP.**

---

## Phase 5: User Story 3 - Downstream consumer receives an index (Priority: P2)

**Goal**: The split-manifest.json is well-formed, discoverable, and downstream-parseable per contracts/split-manifest-schema.md.

**Independent Test**: Manifest JSON validates against the v1 schema; parses cleanly with `jq`; contains stable `subproject_id` primary keys; downstream tool can route SBOMs to consumers using only `entries[].{subproject_id, root_purl, files}`.

### Implementation for User Story 3

Manifest emission itself is US1 T014 — the code is done. US3 adds schema validation and richer manifest content per contract.

### Test tasks for User Story 3

- [ ] T027 [P] [US3] Ship the JSON schema `waybill-cli/contracts/split-manifest-v1.schema.json` per contracts/split-manifest-schema.md. Defines the v1 shape (required fields, types, array-item shapes). Pinned in-repo for schema-validation test (T028).
- [ ] T028 [US3] Add integration test `split_manifest_validates_against_schema` in `waybill-cli/tests/split_manifest_schema.rs` (new file). Runs a small split fixture through the emit pipeline, loads the generated `split-manifest.json`, validates against the v1 schema using the existing `jsonschema = "0.46"` dev-dep (already used for SPDX 2.3 conformance per milestone 010). Fails if manifest drifts from schema.
- [ ] T029 [P] [US3] Extend the manifest emission (T014) to include the `waybill_version` field from `env!("CARGO_PKG_VERSION")`, `scan_root` with the same `<WORKSPACE>` normalization the goldens use (per common/normalize.rs pattern), and the `total_unique_components` + `shared_dep_count` document-level aggregates. Unit test in `split_manifest.rs::tests` validates each field.
- [ ] T030 [US3] Add integration test `split_manifest_subproject_ids_are_stable_and_unique` in `waybill-cli/tests/scan_split_basic.rs`. Runs the cargo-workspace split TWICE (without `WAYBILL_FIXED_TIMESTAMP` — just stability of subproject_id derivation); asserts every `entries[].subproject_id` appears exactly once and matches its filename prefix.

**Checkpoint**: Manifest is a well-formed operator-facing artifact with schema-validation coverage.

---

## Phase 6: User Story 4 - Shared transitive deps handled consistently (Priority: P2)

**Goal**: Two subprojects both depending on `serde 1.0.219` both list `serde` in their sub-SBOM (per FR-007 self-contained per Clarification Q1 duplicate default). Manifest reports shared-dep counts as diagnostic.

**Independent Test**: Fixture where multiple workspace members share transitive deps. Split emits sub-SBOMs each containing the shared deps. Manifest's `shared_dep_count` reflects the shared count.

### Implementation for User Story 4

Nothing new — US1's T009 (BFS) already produces self-contained projections. T010 (shared-deps computation) already populates the counts. This phase is verification testing.

### Test tasks for User Story 4

- [ ] T031 [US4] Extend the existing cargo-workspace integration test with `cargo_workspace_split_duplicates_shared_transitive_deps`. Verifies at least one component present in >1 sub-SBOM (via PURL exact-match). Asserts: for each shared component, it appears in all subprojects whose dep-graph reaches it (per FR-007).
- [ ] T032 [US4] Add assertion in the same file: `manifest_shared_dep_count_accurate`. Parses the emitted manifest, cross-references with the union of `components[*].purl` across all sub-SBOMs, verifies `shared_dep_count` equals the count of PURLs appearing in ≥ 2 SBOMs, and each `entries[].shared_deps_count` reflects that entry's overlap with siblings.

**Checkpoint**: Shared-dep semantics verified. All 4 user stories complete.

---

## Phase 7: Polish + docs + PR

**Purpose**: Docs update, pre-PR gate, PR open.

- [ ] T033 [P] Update `docs/user-guide/cli-reference.md` — add a `## Split mode (`--split`)` section documenting the flag, interaction matrix per contracts/cli-flag.md, and pointer to the split-manifest schema. Cross-link to `docs/user-guide/split-manifest.md` (T034).
- [ ] T034 [P] Create `docs/user-guide/split-manifest.md` — operator-facing guide to consuming `split-manifest.json`. Includes: field reference (mirroring contracts/split-manifest-schema.md), common jq queries (from quickstart.md), CI-integration example (from quickstart.md), downstream tool integration patterns.
- [ ] T035 Pre-PR gate per CLAUDE.md: `./scripts/pre-pr.sh` — `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero warnings) + `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`). If any dead-code warnings appear on non-linux hosts (splitter code is cross-platform pure Rust so this should NOT fire — but the pattern from m212 counters.rs is `#[cfg_attr(...)]` guard if needed).
- [ ] T036 Verify the m214 CI grep gate stays green: run the local mirror `BADHITS=$(grep -rE '\bmikebom\b' waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml Dockerfile.ebpf-test scripts 2>/dev/null | grep -v '^Binary file' | grep -vE 'mikebom-test-fixtures' || true)`; expects zero output.
- [ ] T037 Push branch `git push origin 215-sbom-auto-split`.
- [ ] T038 Open PR against `main` titled `impl(215): --split flag for auto-splitting SBOMs by workspace member (closes #TBD-if-issue-filed)`. PR body includes: (a) summary + link to spec/plan, (b) Test Plan enumerating CI matrix + golden diff-purity checks + schema-validation test, (c) migration/backward-compat note ("`--split` is strictly opt-in; existing single-SBOM invocations are byte-identical to pre-feature per SC-007"), (d) follow-up cross-reference to [waybill#627](https://github.com/kusari-oss/waybill/issues/627) for the merge/split/edit interop docs.

### Final gates

- [ ] T039 CI-side verification: all 4 Lint+test lanes (linux-x86_64 default + ebpf-tracing, macOS, Windows) + Kusari Inspector + 15 rootfs/language scanners MUST pass. Merge blocked until all 20 checks green. Any FR-018-style scope-guard concern — this milestone should NOT introduce any behavioral change to non-split scan; if a pre-existing regression test drifts, that's a rename bug not a bundling opportunity.

---

## Post-merge (out of this PR's scope)

- [ ] T040 File an issue if `--split` reveals gaps in the existing `waybill:is-workspace-root` annotation coverage — some ecosystem readers may not emit it consistently. Follow-up work extends the annotation to any reader that supports workspace-member detection today but doesn't currently emit the annotation.
- [ ] T041 Consider a follow-up spec for manual boundary override (`.waybill/split.yml` config file) once auto-detection gets real-world usage data on where it fails.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: no dependencies — starts immediately.
- **Foundational (Phase 2)**: depends on Setup. T004-T007 parallelizable (different files). T008-T011 sequential (all in `split.rs`).
- **US1 (Phase 3)**: depends on Foundational. T012-T014 sequential (all touching `generate/mod.rs` + `split.rs`). T015-T017 parallelizable unit tests. T018-T022 sequential integration test authoring + golden regen.
- **US2 (Phase 4)**: depends on Foundational + US1 (uses the fully-wired emit pipeline). T023-T026 sequential (fixture → test → regen).
- **US3 (Phase 5)**: depends on Foundational + US1 T014 (manifest emission). T027-T030 mostly parallelizable.
- **US4 (Phase 6)**: depends on Foundational + US1 (verifies duplicate semantics on same fixture). T031-T032 sequential.
- **Polish (Phase 7)**: depends on all preceding phases. T033-T034 parallelizable docs work. T035-T038 sequential release-prep steps.

### Cross-story parallelism

- After Phase 2 completes: US1 code (T012-T014) is the critical path; US2/US3/US4 mostly test-authoring so can start once T012 lands.
- T015 || T016 || T017 (three different test files under US1)
- T027 || T029 (US3 schema + manifest-enrichment work in different files)
- T033 || T034 (US7 polish docs in different files)

### Within each user story

- Foundational modules (T005/T006) written FIRST — types before logic.
- Implementation tasks (T008-T014) sequential within the same file group.
- Test tasks after their implementation targets exist.
- Golden regeneration LAST within each user story (needs the code to produce the golden output).

---

## Parallel Example: Foundational Phase

```bash
# After T004 (CLI flag) completes, launch T005 + T006 + T007 in parallel:
Task: "Add SplitManifest + SplitEntry types in waybill-cli/src/generate/split_manifest.rs"     # T005
Task: "Add SubprojectRoot + SplitProjection type defs in waybill-cli/src/generate/split.rs"    # T006
Task: "Register split + split_manifest modules in waybill-cli/src/generate/mod.rs"             # T007
```

---

## Implementation Strategy

### MVP (US1 + US2 — both P1)

1. Complete Phase 1 (Setup).
2. Complete Phase 2 (Foundational — types + boundary enumeration + BFS + filename convention).
3. Complete Phase 3 (US1 — single-ecosystem cargo-workspace split, goldens).
4. Complete Phase 4 (US2 — heterogeneous fixture + tests + goldens).
5. **STOP + VALIDATE**: manual `waybill sbom scan --path <monorepo> --split --output-dir <tmp>` end-to-end works; goldens byte-identical across two runs.
6. This is the earliest mergeable point.

### Full delivery (US1 + US2 + US3 + US4)

7. Complete Phase 5 (US3 — manifest schema + validation).
8. Complete Phase 6 (US4 — shared-dep duplication verified).
9. Complete Phase 7 (Polish + PR).

### Solo-dev sequencing (recommended)

T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017 → T018 → T018a → T018b → T019 → T020 → T021 → T022 → T027 → T029 → T028 → T030 → T031 → T032 → T023 → T024 → T025 → T026 → T026a → T033 → T034 → T035 → T036 → T037 → T038 → T039.

(US3 T027-T030 pulled forward before US2 fixture-authoring because manifest validation is smaller/quicker; US4 T031-T032 slot in adjacent to US1 tests they build on.)

---

## Notes

- [P] tasks = different files, no dependencies.
- Golden regeneration under `WAYBILL_UPDATE_SPLIT_GOLDENS=1` is the standard pattern established by m212/m213/m214.
- Fixture authoring (US2 T023) can consume real time — allocate half a day even for a small fixture. Small, hand-authored `package.json` / `pyproject.toml` / `Package.swift` files with minimal but non-trivial dep-graphs.
- **Cross-format multiplication (FR-008)**: implicitly tested via the golden set — regenerate with multiple `--format` values and verify N × M files land. Explicit multi-format integration test optional (adds coverage but not required for MVP).
- **Constitution Principle V audit**: no new `waybill:*` on wire SBOMs; manifest is Waybill-side operator namespace under `waybill.dev/schema/`. Passes clean.
- **CI grep gate**: T036 verifies m214 rename-completeness gate stays green — this milestone introduces zero `mikebom` references.
- Skip local `./scripts/pre-pr.sh` on this branch if it's slow — the golden regen is faster than the full compile cache invalidation from a version bump (m214 pattern doesn't apply here since no Cargo.toml version changes).
