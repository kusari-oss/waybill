# Tasks: GHC boot libraries carry their dependency edges, or the document says they don't

**Input**: `specs/1653-boot-library-edges/` — plan.md, spec.md, research.md (R1–R7), data-model.md, contracts/wire.md, quickstart.md, measurements/.
**Tests**: requested (SC-001 to SC-005, mutations). Each story's tests are written first and must fail before its implementation.

**Rules that apply to every task:**
- Fixtures use `waybill-fixture-*` names only.
- No new Cargo dependencies.
- Never run `cargo fmt`.
- Mutations are reverted with `cp` from a backup, then rebuilt.
- Python runs as `python3 -I -B`.
- Never pipe a check through `head`.
- Test modules carry `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- Goldens are regenerated only by `gh workflow run public-corpus.yml -R kusari-oss/waybill --ref main -f branch=1653-boot-library-edges -f regen_goldens=true`.

## Phase 1: Setup (the table)

- [X] T001 Write `scripts/ghc-boot-dependencies.py`, modelled on `scripts/ghc-boot-libraries.py` (header, stdlib only, `-I -B`):
  - Merge `specs/1653-boot-library-edges/measurements/{conf_deps,bindist}.py`: stream each release's `ghc-<v>-x86_64-deb10-linux.tar.xz` and read both layouts, `package.conf.d/*.conf` `depends:`, or `package-data.mk` `_DEP_NAMES` plus `compiler/stage2/package-data.mk` and `rts/package.conf.in`.
  - Take the release list from `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv` (numeric first column only).
  - Emit the TSV in data-model.md's format: header lines (observed date, source URL pattern, layouts, regenerate command, release and row counts), then rows sorted by release then library.
  - Exit non-zero if any release fails, or yields no libraries.
- [X] T002 Run T001's probe into `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_dependencies.tsv`. Then check:
  - 36 releases and 1,371 rows (`measurements/baseline.md`);
  - 9.10.3 `base` is `ghc-internal ghc-prim`;
  - 9.6.6 `base` is `ghc-bignum ghc-prim rts`;
  - every `rts` row is `-`.

  Commit the TSV.

## Phase 2: Foundational

- [X] T003 Write the failing unit tests in a new `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/boot_dependencies.rs` test module:
  - the table parses;
  - the release set equals `boot_table::table()`'s (SC-002b);
  - each release's library set equals milestone 966's, excluding Windows-only rows;
  - versions are equal where both are non-empty;
  - `depends("9.10.3","base") == ["ghc-internal","ghc-prim"]`;
  - `depends("9.6.6","base") == ["ghc-bignum","ghc-prim","rts"]`;
  - `depends(_, "rts") == Some(&[])`;
  - an absent release and an absent library give `None`.
- [X] T004 Implement `BootDependencyTable` in `boot_dependencies.rs` (data-model.md): `include_str!` of the TSV, a `OnceLock` parse, and `observed()`, `releases()` and `depends()`. Register it in `haskell_packages/mod.rs`. A malformed row is a test failure, never a silent skip: mirror `boot_table.rs`'s parse discipline. Make T003 green.

## Phase 3: User Story 1 — A boot library's dependencies are in the graph (Priority: P1)

**Goal:** edges between boot libraries of the same compiler, with C245 saying where they came from (FR-002, FR-003, FR-004).

**Independent test:** a stub closure on milestone 966's fixtures (compiler 9.10.3) emits `base` → `ghc-prim`, with C245, in three formats with parity.

- [X] T005 [P] [US1] Write the failing unit tests for the pass, in `waybill-cli/src/resolve/boot_library_edges.rs`'s test module, on hand-built `ResolvedComponent`s carrying C218. Boot-library names are GHC's own (`base`, `ghc-prim`, `rts`), as `tests/closure_resolved_tier.rs` already uses them; anything else is `waybill-fixture-*`.
  - two compilers (`ghc-9.10.3`, `ghc-binary-9.8.4`), each with `base` and `ghc-prim`: edges only within a compiler;
  - an existing edge is not duplicated;
  - a dependency absent from the components gives no edge, and C245 `absent` lists it, sorted;
  - a compiler release not in the table gives `{"reason":"release-not-in-table"}`, and a library not in the release gives `{"reason":"library-not-in-table"}`;
  - `rts` present gives C245 with no `absent`;
  - components without C218 are untouched.
- [X] T006 [P] [US1] Write the failing integration tests in a new `waybill-cli/tests/boot_library_edges.rs` (`#![cfg(unix)]`, `mod common;`), using `common::nix_stub::NixStub` over `tests/fixtures/nix_closure_boot_versions/` as `tests/closure_resolved_tier.rs` does:
  - **The positive control (SC-004):** `pkg:hackage/base@<9.10.3's>` → `pkg:hackage/ghc-prim@<9.10.3's>` in CycloneDX `dependencies`, SPDX 2.3 `DEPENDS_ON` and SPDX 3 `dependsOn`.
  - C245 is on every C218 component, and `waybill sbom parity-check` reports no gap.
  - **SC-002a:** the same scan with `NixStub.store_dir` pointing at a tempdir holding a fake compiler output, whose `package.conf.d` declares a different `base` dependency, gives identical edges and C245. This guards against a future change that reads the store: T017's store-reading mutation must fail it.
  - Without `--nix-closure`, no C245 and no new edges.
- [X] T007 [US1] Implement `pub(crate) fn apply(components: &mut [ResolvedComponent], relationships: &mut Vec<Relationship>) -> BootEdgeSummary` in `waybill-cli/src/resolve/boot_library_edges.rs`, per research R4:
  - group by the C218 value, whose release is the value with `ghc-` or `ghc-binary-` stripped;
  - look up `depends()`;
  - add `DependsOn` with provenance `{source:"ghc-boot-dependencies", data_type:"dependency"}`;
  - stamp C245 `waybill:boot-library-dependencies` as a canonical JSON string (`serde_json::json!`, sorted keys).

  Register it in `waybill-cli/src/resolve/mod.rs`. A module doc gives the measured facts: 4 of 4 nixpkgs agreement, 36 of 36 milestone 966 agreement, and 9.10's `base` → `ghc-internal`.
- [X] T008 [US1] Call it in `waybill-cli/src/cli/scan_cmd.rs` immediately after `closure_resolved_tier::retier` (around line 5596), on the same closure-only branch, with one INFO line: edges added, complete / incomplete / unknown counts.
- [X] T009 [US1] Add C245 in the same change, because the gate is in the lib target:
  - extractors `c245_cdx`, `c245_spdx23` and `c245_spdx3`, using the component-scope annotation macros, in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`;
  - registered in `parity/extractors/mod.rs` as `SymmetricEqual`, `order_sensitive: false`;
  - the row in `docs/reference/sbom-format-mapping.md`, after C244, with the Principle V audit from research R5.

  Run T005 and T006 until green.

## Phase 4: User Story 2 — Unknown or incomplete boot libraries are not presented as complete (Priority: P1)

**Goal:** a closed-vocabulary reason, and `compositions[]` agreeing with it (FR-005).

**Independent test:** a stub closure whose components include `ghc-prim` and not `rts` reads `partial`, with `boot-library-edges-incomplete`.

- [X] T010 [P] [US2] Write the failing unit tests:
  - `ReasonCode::BootLibraryEdgesIncomplete { incomplete: 2, unknown: 1 }` renders `boot-library-edges-incomplete: 3 hackage boot librar(ies), 2 with dependencies not in the document, 1 without dependency data`, with no `;`, in `generate/graph_completeness/reason_codes.rs`;
  - the classifier fires on components carrying C245 with `absent` or `reason`, and does not fire when all are complete, in `generate/graph_completeness/mod.rs`;
  - `degraded_ecosystems` adds `hackage`, in `generate/cyclonedx/compositions.rs`.
- [X] T011 [US2] Implement the variant and its rendering. Add `classify_boot_library_edges_incomplete(components)`, modelled on `classify_go_transitive_coverage_degraded`, which reads C245 from each component, and push it on the classified path. On the empty-set path, file-tier-only components never carry C245, so a comment there says why no push is needed. Add the explicit `compositions` arm.
- [X] T012 [US2] Integration test in `waybill-cli/tests/boot_library_edges.rs`: the stub scan reads C104 `partial`, and C105 contains `boot-library-edges-incomplete:` in all three formats. No CycloneDX `compositions[]` record with aggregate `complete` names a `pkg:hackage/` component, except the root's own dependency-list claim, as spec 1652's test excludes it.

## Phase 5: User Story 3 — Nothing else changes (Priority: P2)

- [X] T013 [US3] Run `cargo test -p waybill --test closure_resolved_tier --test nix_closure_boot_versions` (and every other test file that drives `NixStub`; grep `nix_stub` under `waybill-cli/tests/`). Update an assertion only where the new edges, C245 or the reason are the explanation, and record each one in the test's doc comment.
- [X] T014 [US3] Byte identity without `--nix-closure`: build `main` in a worktree, scan two non-closure fixtures in three formats with both binaries, mask UUIDs and timestamps, and `diff -r` to 0 differences. Add a positive control: the stub closure scan differs. Record it in `specs/1653-boot-library-edges/measurements/after.md`.

## Phase 6: Polish & cross-cutting

- [X] T015 [P] Update C105's row in `docs/reference/sbom-format-mapping.md` to the 12-code vocabulary with the new template. Update the nix closure documentation that describes milestone 966 (grep `boot-library-compiler` under `docs/`) with the edges and C245.
- [X] T016 [P] Add a CHANGELOG entry under Unreleased → Fixed. It covers:
  - the edges and their source;
  - the corrected premise: 9.10's `base` depends on `ghc-internal`;
  - C245;
  - the new reason;
  - that every closure document now reads `partial`, and why (`rts`, `ghc-internal`).

  Refs #1688.
- [X] T017 Mutations, each via a `cp` backup, rebuild, the targeted tests (which must fail), restore and rebuild. Recorded in `measurements/after.md`:
  - `depends()` returns `None`;
  - the compiler key is ignored (match by name only, across compilers);
  - the classifier never pushes;
  - the `compositions` arm is removed;
  - C245 is not emitted in SPDX 3;
  - `absent` is never filled;
  - the pass reads the fake store output's `package.conf.d` when present (it must fail the SC-002a test).
- [X] T018 Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` (no `+toolchain`). Confirm `GATE_RESULT=pass`, the expected line, and every per-target `test result: ok. N passed; 0 failed`.
- [X] T019 Push, then dispatch the corpus with `regen_goldens=true`. Enumerate every job's conclusion and confirm the run scanned the branch head. Download the regenerated goldens and commit them. Trace every golden diff with a normalised, sorted diff, as SC-005 requires:
  - only the five closure goldens change;
  - only by edges between C218 components, C245, C104/C105 and `compositions[]`, plus the orphan classification that follows: `waybill:orphan-reason` on a boot library that gains an incoming edge, and C105's orphan count. Each such change is traced to a specific new edge (spec US3 scenario 2).

  Also assert and record:
  - **SC-002:** for each golden and compiler, `base`'s `dependsOn` equals the table's `depends` for that release, filtered to components present.
  - **The `ghc` library:** its edges before are a subset of its edges after, and no `dependsOn` entry is duplicated.

  Record per target the boot libraries with edges and the edge counts, before and after (SC-001), against research R2's prediction, in `measurements/after.md`.
- [X] T020 Dispatch a verification run without regeneration, and confirm every golden passes. File the follow-up issue for emitting each compiler's full boot-library set (research R3), with the public footer. Mark the tasks and checklist complete, verifying each claim against the diff. Open the PR with `Refs #1688` and the footer.

## Dependencies

- T001 → T002 → T003 → T004. The table blocks everything.
- US1 (T005–T009) → US2 (T010–T012). The classifier reads C245, which US1 writes.
- US3 (T013, T014) comes after US1 and US2.
- Polish comes last. T017 comes before T018, and T018 before T019 and T020.

## Parallel opportunities

- T005 ∥ T006.
- T010's three files can be written in parallel.
- T015 ∥ T016.

## Implementation strategy

- **MVP = US1:** the edges and C245. `base` → `ghc-prim` exists on every project compiler.
- **Then US2:** the verdict stops claiming `complete`.
- **US3 and Polish:** nothing else moved, and the goldens are traced.
