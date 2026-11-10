# Tasks: the Go modules and Rust crates inside a Nix-built binary

**Input**: Design documents from `/specs/1378-closure-binary-contents/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/
**Issue**: #1396

**Tests**: included. The spec gives each story an independent test and the
repository gate requires them.

Paths are relative to the repository root. `CL` abbreviates
`waybill-cli/src/scan_fs/package_db/nix/closure`.

> **Superseded on rebase (2026-11-10).** While this was being built, main
> landed milestone 1377 (#1405, issues #1398 and #1394): the package a closure
> scan evaluated is always emitted and is the document's subject, identified
> as `graph.evaluated` — the closure's sole unconsumed derivation. That is the
> root-as-component work this feature had taken on as "the minimum of #1398".
> On rebase the feature's own mechanism (a `root` value for
> `waybill:closure-role`, `DerivationRole::Root`, `graph::sole_root`) was
> **removed** in favour of main's. Text below that describes it is history.
> Main also took row C212, so this feature's rows are **C213, C214, C215**.

## Phase 1: Setup

- [X] T001 Re-verify research.md's line-numbered seams against the working tree (R1–R4) and correct research.md where they moved: `CL/graph.rs` `root_of`, `CL/emit.rs` `emits_as_component`, `waybill-cli/src/scan_fs/package_db/go_binary.rs` `emit_entries_from_info`, `waybill-cli/src/scan_fs/binary/entry.rs` `cargo_auditable_packages_to_entries`, `waybill-cli/src/cli/scan_cmd.rs` closure block
- [X] T002 Create closure JSON fixtures in `waybill-cli/tests/fixtures/nix_closure_contents/` — `app.json` (a root `waybill-fixture-app 1.2.3` depending on one library and one build tool, bare-basename output paths) — using only synthetic `waybill-fixture-*` names

## Phase 2: Foundational (blocks every story)

- [X] T003 Add `DerivationRole::Root` (wire `root`) in `CL/classify.rs`: assign it in `classify` to the closure root when no other signal gives a role; extend `merge`, `wire` and the wire-form test; make `root_of` `pub(crate)` in `CL/graph.rs`
- [X] T004 Admit `Root` in `role_alone_emits` in `CL/emit.rs` and handle the new variant at every exhaustive match in `CL/emit.rs` and `CL/summary.rs`
- [X] T005 Add an explicit `"root"` arm (classifies nothing) in `waybill-cli/src/resolve/closure_role_scope.rs` with a unit test that a `root` component keeps `artifact_presence == None`
- [X] T006 Unit tests in `CL/classify.rs` and `CL/emit.rs`: the root of a fixture closure gets `Root`, emits as a component with C182 `root`, and `graph::relationships` returns `unanchored == 0` (FR-019, FR-020)
- [X] T007 Expose a dependency-entries helper in `waybill-cli/src/scan_fs/package_db/go_binary.rs` (`pub(crate) fn dependency_entries(info: &GoBinaryInfo) -> Vec<PackageDbEntry>`) that reuses `emit_entries_from_info` with the main module withheld; unit test that it yields deps + `stdlib` and no main module
- [X] T008 Create `waybill-cli/src/scan_fs/binary/embedded.rs` with `EmbeddedContents`, `SelfDescription`, `SelfEcosystem` and `pub(crate) fn embedded_contents(path: &Path) -> EmbeddedContents` per research R2; register the module in `waybill-cli/src/scan_fs/binary/mod.rs`
- [X] T009 Unit tests in `waybill-cli/src/scan_fs/binary/embedded.rs`: a directory with no binaries; a single-file path; a Go binary (deps recovered, main module returned as self-description, `(devel)` → `None` version); a cargo-auditable binary (non-root crates recovered with `parent_purl == None`, root crate returned as self-description); a directory holding a `Cargo.lock` yields nothing
- [X] T010 Create `CL/contents.rs` with `ReadState`, `ContentsRecord` (+ `wire()`), `BinaryIdentity`, `IdentityCandidate`, `ContentsOutcome`, `store_dir()` (honours `NIX_STORE_DIR`) per data-model.md; register in `CL/mod.rs`

**Checkpoint**: the root is a component; a path can be asked for its embedded modules.

## Phase 3: User Story 1 — see what is compiled into the package I scanned (P1) 🎯 MVP

**Goal**: recovered modules appear as dependencies of the member whose output holds them.
**Independent test**: integration scan of the fixture closure with a binary placed in a temp store; module count equals what the reader reports for that binary.

- [X] T011 [US1] Implement `contents::read(classified, components, store_dir) -> ContentsOutcome` in `CL/contents.rs`: group emitted derivations by `(pname, version)`, resolve outputs, call `embedded_contents`, convert entries to `ResolvedComponent`s (mirroring the field propagation in `waybill-cli/src/scan_fs/mod.rs` `scan_path`), deduplicate by PURL, build member→module `DependsOn` relationships (FR-001, FR-005 to FR-009)
- [X] T012 [US1] Set `artifact_presence` on each recovered module from the roles of the members that reach it, shipped winning over build-only, in `CL/contents.rs` (FR-010)
- [X] T013 [US1] Wire the pass into `waybill-cli/src/cli/scan_cmd.rs` after `run_closure_ecosystem_identity` and before `graph::relationships`, as a named function `run_closure_contents`; extend `components` and `relationships`; log the `nix-closure: contents` INFO line
- [X] T014 [P] [US1] Unit tests in `CL/contents.rs`: one member with a present output; a module reached from two members is one component with two edges; a module reached from a build-only and a shipped member is not marked absent; a build-only-only module is `NotInArtifact`; one module at two versions under two members is two components
- [X] T015 [US1] Field-parity test in `CL/contents.rs`: for one binary, the component the pass builds equals what the standalone conversion builds on `purl`, `name`, `version`, `sbom_tier`, `evidence_kind`, `confidence` and `extra_annotations` (FR-005)
- [X] T016 [US1] Create `waybill-cli/tests/closure_binary_contents.rs` (file-scope `#![cfg(unix)]`) with the stub-`nix` harness and a temp `NIX_STORE_DIR`; test that the three formats carry the same recovered PURLs and the same member→module edges, and that `metadata.component` and the root's CPE are what they were without the store (FR-006, FR-018, FR-021, SC-006)

## Phase 4: User Story 2 — know when the contents could not be read (P1)

**Goal**: unread members are explicit at component and document scope; strict opt-in.
**Independent test**: scan with an empty store directory.

- [X] T017 [US2] Compute per-member `ReadState` and the `ContentsRecord` in `contents::read`; stamp C214 `waybill:closure-contents` on every member component (FR-013, FR-014)
- [X] T018 [US2] Thread C213 through `ScanArtifacts` (`waybill-cli/src/generate/mod.rs`) and every struct literal that builds one (`generate/spdx/{packages,mod,document,relationships}.rs`, `generate/openvex/mod.rs`); populate it in `waybill-cli/src/cli/scan_cmd.rs`
- [X] T019 [US2] Emit C213 in CycloneDX (`generate/cyclonedx/{builder,mod,metadata}.rs`), SPDX 2.3 (`generate/spdx/annotations.rs`) and SPDX 3 (`generate/spdx/v3_annotations.rs`)
- [X] T020 [US2] Withhold the CycloneDX root-completeness claim when the record has unread members, beside the existing `read_outcome.has_gap()` test in `generate/cyclonedx/builder.rs` (FR-016)
- [X] T021 [US2] Add catalog rows C213 and C214 to `docs/reference/sbom-format-mapping.md` and their extractors to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs` in the same change
- [X] T022 [US2] Add `--nix-closure-require-root-contents` to `ScanArgs` in `waybill-cli/src/cli/scan_cmd.rs` (`requires = "nix_closure"`); fail before any document is written when the root is not `read` or the tier degraded; WARN when unread without the flag (FR-015 to FR-015b)
- [X] T023 [P] [US2] Unit tests in `CL/contents.rs`: `absent`, `partial`, `failed`, `no-outputs`, read-and-empty stays `read`; record counts sum to `members`; a single-file output is read
- [X] T024 [US2] Integration tests in `waybill-cli/tests/closure_binary_contents.rs`: empty store → exit 0, zero recovered modules, C213 `read: 0` and `root: "absent"`, C214 `absent` on members (SC-004), and the only differences from the present-store document are explained by C213/C214 (SC-009); strict flag → non-zero exit and no output file; strict flag with a present root that embeds nothing → exit 0 (SC-004a)

## Phase 5: User Story 3 — one identity per package (P2)

**Goal**: the program appears once; its ecosystem identity is promoted only when confirmed.
**Independent test**: count components describing the program; stub the confirmation both ways.

- [X] T025 [US3] In `contents::read`, derive an `IdentityCandidate` per member from its self-descriptions per research R5 (one distinct self-description, Go `v` prefix, no closure version → none, several → `ambiguous`) in `CL/contents.rs`
- [X] T026 [US3] In `run_closure_contents` (`waybill-cli/src/cli/scan_cmd.rs`), confirm candidates through `DepsDevSource::confirm`; on `Found` swap the member's `purl`; stamp C215 `waybill:closure-binary-identity` in every case (FR-011 to FR-012b)
- [X] T027 [US3] Add catalog row C215 to `docs/reference/sbom-format-mapping.md` and its extractors to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs`
- [X] T028 [P] [US3] Unit tests in `CL/contents.rs`: Go candidate gets the `v` prefix; `(devel)` yields no `binary-version`; a differing self-reported version is recorded; two distinct self-descriptions → `ambiguous`; no closure version → `no-version`
- [X] T029 [US3] Tests beside the existing mock-deps.dev tests in `waybill-cli/src/cli/scan_cmd.rs`: `Found` promotes and C215 says `confirmed`; `Absent` leaves the PURL and says `declined`; disabled says `unchecked`/`disabled`; in all cases exactly one component describes the program (SC-005)

## Phase 6: User Story 4 — the same shape whichever language (P3)

**Goal**: Go and Rust contents relate to their member identically; nothing nested.
**Independent test**: compare the two in one document.

- [X] T030 [US4] Integration test in `waybill-cli/tests/closure_binary_contents.rs`: with one Go and one cargo fixture binary in the store, both sets are top-level components related by `dependsOn`, and no recovered crate has a composite bom-ref or sits in a nested `components[]` (FR-007)

## Phase 7: Polish and verification

- [X] T031 [P] Update `docs/reference/sbom-format-mapping.md` C182 row with the `root` value and `docs/ecosystems.md` closure section per contracts/documentation.md
- [X] T032 [P] Help text for `--nix-closure` and the new flag in `waybill-cli/src/cli/scan_cmd.rs`
- [X] T033 Run the walker-audit grep locally (`.github/workflows/ci.yml` walker-audit step) to confirm no new `fn walk` trips it
- [X] T034 Run quickstart.md steps 1–5 against the gh and ripgrep wrapper flakes and record results in `specs/1378-closure-binary-contents/measurements/FINDINGS.md` (SC-001 to SC-006); include wall time with and without a populated store (SC-008) and one run against the substituted aarch64-linux outputs
- [X] T035 Run quickstart.md step 6 on a closure with no Go or Rust member and record the diff (SC-007)
- [X] T036 Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` and read the per-target result lines
- [ ] T037 Regenerate the `nix-closure-moat` corpus goldens via the `public-corpus.yml` dispatch and read the diff — **cannot be done locally**; hand to the maintainer

## Notes from implementation

- T008: the entry point makes one read and one `.dep-v0` section lookup per
  binary (`scan::dep_v0_manifest`), not a call to the full binary scanner as
  research R2 said. The scanner was most of the pass's time (16 s → ~5 s on
  97 outputs) and logged a line per binary. Universal Mach-O still takes the
  scanner's path.
- T020: the completeness claim had to be withheld in SPDX 3 as well
  (`generate/spdx/v3_document.rs`); the task named only the CycloneDX builder,
  and the first implementation made the two formats disagree. Caught by
  `m1280_t056`.
- T003–T006 were implemented, then **removed on rebase**: main's milestone
  1377 emits the evaluated package itself. The contents pass reads
  `graph.evaluated` instead.
- T016/T024: `tests/closure_edges.rs`' harness now points `NIX_STORE_DIR` at
  a directory that does not exist, so those tests no longer stat the host's
  store. The other closure test files still do; their fixture basenames do
  not exist there.
- T035: run as a before/after on gh and ripgrep over non-module components,
  not on a closure without Go or Rust (no pre-feature binary was kept).
- T036: passed on the third run — gate exit 0, the expected line found, 372 test targets ok, 0 failed, 7,522 tests passed, 42 ignored. The first run died at clippy and ran no tests; the second found the SPDX 3 disagreement above.
- T037 remains open and cannot be done locally.

## Dependencies

- Phase 2 blocks everything. T003 → T004 → T006; T007 → T008 → T009; T010 before Phase 3.
- US1 (T011–T016) is the MVP and blocks US2–US4, which all extend `contents::read`.
- US2 and US3 are independent of each other after US1; both touch `CL/contents.rs` and `scan_cmd.rs`, so they are sequential in practice.
- US4 is a test over US1's output.
- T021 and T027 must land with their extractors (lib-target gate).

## Parallel opportunities

- T014 with T016 (different files).
- T023 with T024.
- T031 with T032.
- Few others: most tasks share `CL/contents.rs` or `scan_cmd.rs`.

## Implementation strategy

1. Phases 1–2, then US1: the document gains modules. Demonstrable on gh.
2. US2: the document says what it did not read. Do not ship US1 without it.
3. US3: one identity. US4: the shape test.
4. Phase 7, with T034 as the acceptance measurement.
