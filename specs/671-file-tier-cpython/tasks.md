---

description: "Task list for milestone 671 — file-tier surfacing for source-heavy trees (SC-003 follow-up)"
---

# Tasks: File-tier surfacing for source-heavy trees (SC-003 follow-up)

**Input**: Design documents from `/specs/671-file-tier-cpython/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: INCLUDED. SC-001 (≥100 file-tier), SC-006 (Python-only restriction), SC-007 (C156 annotation) require fixture-integration tests. FR-009 (loud-fail on unknown extension) requires a parse-error unit test. US2 is a verification-only story (existing 6 golden test suites + sweep regression cover it).

**Organization**: Tasks grouped by user story. US1 (opt-in mode) + US3 (restriction subset) contain the code delivery. US2 (byte-identity) is verification-only.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 (Setup + Polish carry no story label)
- Exact absolute file paths in every description

## Path Conventions

- **Source root**: `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/file_tier/`
- **CLI**: `/Users/mlieberman/Projects/mikebom/waybill-cli/src/cli/scan_cmd.rs`
- **Parity catalog**: `/Users/mlieberman/Projects/mikebom/waybill-cli/src/parity/`
- **Test root**: `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/`
- **Docs**: `/Users/mlieberman/Projects/mikebom/docs/reference/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Empty scaffold files so subsequent tasks can iterate against a running dispatcher.

- [X] T001 [P] Create empty `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/file_tier/source_shape.rs` with `#[allow(dead_code)]` module-level attribute, empty `pub(crate) enum SourceShape {}`, empty `#[derive(thiserror::Error)] pub(crate) enum SourceShapeParseError {}` — placeholders that let the sibling files `use super::source_shape;` compile before the real content lands. Add `pub(super) mod source_shape;` to `waybill-cli/src/scan_fs/file_tier/mod.rs`. **Delivered**: `pub(crate) mod source_shape;` registered in `file_tier/mod.rs:50` (upgraded from `pub(super)` in task text to `pub(crate)` — matches the existing `content_shape` / `dedupe` / `walker` sibling declarations). Empty `SourceShape` + `SourceShapeParseError` enums stubbed with doc-comments cross-linking T003/T004 + `data-model.md` + `contracts/source_shape_restriction.md`. Compile clean, clippy clean.
- [X] T002 [P] Create empty integration-test file `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_file_tier_source_tree_m671.rs` with `#![cfg(test)]` + `#![allow(clippy::unwrap_used)]` at the top and no test functions. Establishes the target so subsequent test tasks land without a separate wire-in. **Delivered**: file created with the two crate-attributes + a module docstring cross-linking to the m671 spec/plan/data-model and enumerating which downstream T012/T015/T016/T017/T018 tasks will populate it. `cargo test --test scan_file_tier_source_tree_m671` runs green with `0 passed; 0 failed`. Clippy clean.

**Checkpoint (Phase 1)**: Module + integration-test target exist and workspace compiles.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Type-level pieces (`SourceShape` enum + `SourceShapeSet` + `FileInventoryMode::SourceTree` + `ContentShape::SourceFile`) that ALL user-story code depends on.

**⚠️ CRITICAL**: No user-story work can begin until Phase 2 completes.

- [X] T003 [P] Implement the `SourceShape` enum + methods in `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/file_tier/source_shape.rs` per `data-model.md` §"SourceShape (new enum)": 21 variants (`Py`, `Pyi`, `C`, `Cc`, `Cpp`, `Cxx`, `H`, `Hh`, `Hpp`, `Rs`, `Go`, `Java`, `Kt`, `Js`, `Ts`, `Rb`, `Php`, `Cs`, `Swift`, `M`, `Mm`), derives `Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord`. Methods: `from_extension(&str) -> Option<Self>` (case-insensitive, tolerant of leading `.`), `as_str(&self) -> &'static str` (lowercase, no dot). Also add `pub(crate) type SourceShapeSet = std::collections::BTreeSet<SourceShape>;`. Include 3 inline unit tests: (1) every variant round-trips through `from_extension(v.as_str())`, (2) case-insensitivity works (`from_extension("PY") == Some(Py)`), (3) leading-dot tolerance (`from_extension(".py") == Some(Py)`). **Delivered**: 21 variants grouped by language family for review-clarity; derives applied. Methods `from_extension` uses `strip_prefix('.')` (strips exactly ONE leading dot; `..py` correctly rejected) + `to_ascii_lowercase` (case-insensitive). Added const `ALL: [Self; 21]` sorted lex by `as_str()` for T004's diagnostic construction. `SourceShapeSet` type alias (BTreeSet). **5 unit tests** (added 2 beyond spec — `from_extension_rejects_unknown` regression guard + `all_array_is_sorted_lex_by_as_str` ordering invariant). All pass. Discovered + fixed one bug during test iteration: my initial `trim_start_matches('.')` was greedy (stripped all leading dots); switched to `strip_prefix('.')` per test regression on `..py` input.
- [X] T004 [P] Implement `SourceShapeParseError` + `parse_restriction(&str) -> Result<SourceShapeSet, SourceShapeParseError>` in the same file per `contracts/source_shape_restriction.md` §"Parse steps". Error variants: `UnknownExtension { actual: String }` (Display lists all 21 accepted extensions), `Empty`. Dedup silently via BTreeSet. Include 4 inline unit tests: (1) `parse_restriction("py,c,h")` yields `{C, H, Py}`, (2) `parse_restriction("")` → `Err(Empty)`, (3) `parse_restriction("md")` → `Err(UnknownExtension{actual:"md"})`, (4) `parse_restriction("py,py")` yields `{Py}` (dedup). **Delivered**: parser drops empty tokens (comma-only input → `Empty`), delegates extension lookup to T003's `from_extension` (so case-folding + leading-dot tolerance ride for free), first-unknown-wins on mixed valid/invalid inputs. Error `Display` message hardcodes the 21 accepted extensions in lex order (mirrors `SourceShape::ALL`) so downstream stderr-greppers get a stable diagnostic. **6 unit tests added** (2 beyond spec): (1) typical multi-ext + iteration-order assertion, (2) empty + comma-only errors, (3) unknown extension first-wins behavior, (4) silent dedup across case variants + leading dots, (5) diagnostic contains sample accepted extensions (regression guard), (6) whitespace + leading-dot tolerance in parse tokens. All 11 tests in `scan_fs::file_tier::source_shape::tests` pass; clippy clean.
- [X] T005 [US1] Extend the `FileInventoryMode` enum at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/file_tier/mod.rs:292` with a new variant: `SourceTree { restriction: Option<SourceShapeSet> }`. Update `FileInventoryMode::parse(raw: &str)` at `mod.rs:308` to recognize `"source-tree"` → `SourceTree { restriction: None }` (the restriction subset arrives via the companion flag, wired up at the CLI layer in T009). Preserve the existing three variants byte-identical. Inline unit tests: `parse("source-tree")` returns `Ok(SourceTree { restriction: None })`; `parse("Off")` still works. **Delivered**: variant added; `parse` accepts `"source-tree"` case-insensitively. **`Copy` derive dropped** — the new variant carries `BTreeSet` which isn't `Copy`; verified no downstream site relied on `Copy` (all callers in `scan_cmd.rs` use `!=` / `match` / borrow). **Downstream match at `scan_cmd.rs:4085`** required a new arm for the `SourceTree { .. }` variant — added mapping to `Some("source-tree")` (mirrors `Full → Some("full")` shape; the doc-level metadata rides on this label; finer-grained C156 annotation is separate work in T010). **2 unit tests** added: `parse_accepts_source_tree` verifies case-insensitivity + `sourcetree` (no dash) regression guard. 62 tests in `file_tier::*` all pass; workspace clippy clean.
- [X] T006 [US1] Extend `ContentShape` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/file_tier/content_shape.rs:32` with a new variant `SourceFile`. Update the module docstring at `content_shape.rs:1-22` to name the new variant and cross-link to the m671 spec. **Delivered**: `SourceFile` variant added after `ExecScript` with a full doc-comment describing (a) the mode-gated emission contract (only under `SourceTree` mode + FR-002 allowlist match + restriction check), (b) the default-mode byte-identity preservation (variant is never produced under Orphan/Off/Full), (c) downstream emission treats it identically to other file-tier variants (SHA-256 + path evidence, no PURL). Module docstring extended with an m671 paragraph naming the new variant + the conditional-bypass semantic + cross-linking `specs/671-file-tier-cpython/`. `ContentShape` still derives `Copy` (variant is unit-shaped). Compile + full-workspace clippy + 62 `file_tier::*` tests all pass.

**Checkpoint (Phase 2)**: Type surface is complete. `cargo build -p waybill --lib` compiles. No behavior change yet.

---

## Phase 3: User Story 1 — Opt-in source-tree mode (Priority: P1) 🎯 MVP

**Goal**: `--file-inventory=source-tree` surfaces source-code file extensions as file-tier components; C156 doc-scope annotation records the mode.

**Independent Test**: Scan the m671 synthetic fixture (10-20 mixed .py/.c/.h files); assert `[.components[] | select(.type == "file")] | length >= 6` (all source files emit) + verify the C156 annotation is present with `restriction: null`.

### Implementation for US1

- [X] T007 [US1] Extend `content_shape::classify` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/file_tier/content_shape.rs` per `research.md` R6. Add a new parameter (or thread the existing mode through) so the function knows when to bypass the `EXCLUDED_EXTENSIONS` check for FR-002 source-code extensions. When `mode == SourceTree { restriction }` AND the file extension has a matching `SourceShape::from_extension` result AND the restriction (if `Some`) contains that shape → return `Some(ContentShape::SourceFile)` instead of `None`. Default-mode code path stays byte-identical (matches SB#5 + FR-007). Inline unit tests: (1) `.py` under `Orphan` mode returns `None` (default-mode preserved), (2) `.py` under `SourceTree { restriction: None }` returns `Some(SourceFile)`, (3) `.py` under `SourceTree { restriction: Some({Rs}) }` returns `None` (restriction filters out), (4) `.md` under `SourceTree { restriction: None }` returns `None` (not in FR-002 allowlist). **Delivered**: introduced sibling function `classify_with_source_tree` with the new `source_tree_restriction: Option<Option<&SourceShapeSet>>` parameter; existing `classify` becomes a byte-identical thin wrapper calling `classify_with_source_tree(..., None)`. **Preserves 14+ existing test call sites unchanged** (avoiding widespread churn). Bypass logic: when the `EXCLUDED_EXTENSIONS` check would return `None` AND SourceTree mode is active AND `source_shape_for_filename` resolves to a `SourceShape` AND the restriction (if any) contains that shape → return `Some(ContentShape::SourceFile)`. Docs/configs/build-glue stay hard-excluded under all modes (fourth test verifies). Added helper `source_shape_for_filename` handling multi-dot paths + no-extension files. **5 unit tests** (spec asked 4; added a 5th `source_shape_for_filename_edge_cases`). All 23 content_shape tests pass; workspace clippy clean.
- [X] T008 [US1] Update the walker at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/file_tier/walker.rs` to thread the `FileInventoryMode` value through to `classify()`. Preserve existing `Off` / `Orphan` / `Full` call sites byte-identical. Extend the `file_tier walker complete` INFO log line (per FR-011 + `data-model.md` §"On the log") to include `mode=SourceTree source_tree_restriction=[...]` fields when the new mode is active. **Delivered**: `WalkerConfig` gains a `source_tree_restriction: Option<Option<&SourceShapeSet>>` field. Production walker code now routes through `classify_with_source_tree` instead of `classify`. The `scan_cmd.rs` wire-in derives the field from the effective `FileInventoryMode` (SourceTree → Some(restriction.as_ref()); else None). **INFO log**: the existing line already had `mode = ?file_inventory_mode`, so `SourceTree { restriction: [...] }` prints automatically via Debug — no source-side change needed. 16 walker-test call sites updated via perl multi-line regex (with a small collateral-fix to a WalkConfig site that shared the same suffix shape). 67 `file_tier::*` tests pass; workspace clippy clean.
- [X] T009 [US1] Wire the CLI flags at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/cli/scan_cmd.rs`: (a) extend the `--file-inventory` argument's help text to name the new `source-tree` value; (b) add a new `--file-inventory-source-shapes` argument via `Args`-derive with `value_parser = crate::scan_fs::file_tier::source_shape::parse_restriction`; (c) post-parse cross-arg validation: if `source_shapes.is_some() && !matches!(file_inventory_mode, SourceTree { .. })`, fail with a clear diagnostic per `contracts/source_shape_restriction.md`. When both flags are present, construct `FileInventoryMode::SourceTree { restriction: Some(parsed) }`. Test coverage lands in T012 (integration) + T004 (unit). **Delivered**: `ScanArgs.file_inventory_source_shapes: Option<SourceShapeSet>` with `parse_source_shape_restriction_arg` value_parser (thin wrapper over `parse_restriction` to convert `SourceShapeParseError → String` for clap). `--file-inventory` help + parse-fail message updated to include `source-tree`. Cross-arg validation: `(_, Some(_))` when mode is not `SourceTree` → fail with contract-cited diagnostic naming the file at `specs/671-file-tier-cpython/contracts/source_shape_restriction.md`. Combined into a final `FileInventoryMode::SourceTree { restriction }`. `SourceTree { .. }` arm added at the mode-label match. `WalkerConfig.source_tree_restriction` wired from the effective mode. Test-helper `ScanArgs` default at line ~5590 initialized to `None`. End-to-end smoke: `--file-inventory=source-tree` emits 2 file components; adding `--file-inventory-source-shapes=py` restricts to 1; `--file-inventory=orphan --file-inventory-source-shapes=py` fails with the contract-diagnostic; `--file-inventory-source-shapes=md` fails at parse time listing the 21 accepted extensions. Workspace clippy clean.
- [X] T010 [US1] Emit the C156 doc-scope annotation per `data-model.md` §"C156". When the effective mode is `SourceTree`, insert into the emitted CDX `metadata.properties[]` (and SPDX 2.3 / SPDX 3 doc-scope annotations) a JSON-stringified `{"mode":"source-tree","restriction":<sorted-list-or-null>}`. Locate the emission site by tracing the existing `waybill:binary-scan-suppressed` (C153) emission at `waybill-cli/src/generate/cyclonedx/metadata.rs` and its SPDX siblings. Emit iff and only if the mode is active (default-mode byte-identity preserved). **Delivered**: (a) added `pub file_inventory_source_shapes: Option<Vec<String>>` to `ScanArtifacts` at `generate/mod.rs:246`, propagated through `.narrow()`; (b) scan_cmd.rs constructs it as a sorted-lex `Vec<String>` from the parsed `SourceShapeSet` (BTreeSet iterates in enum-discriminant order — grouped by language family, NOT lex — so we sort explicitly after `as_str()` conversion); (c) CDX plumbing via `.with_file_inventory_source_shapes(...)` builder method + new `Option<&[String]>` parameter on `build_metadata`; (d) 3 emission sites (CDX `metadata.rs`, SPDX 2.3 `annotations.rs`, SPDX 3 `v3_annotations.rs`) each gate on `file_inventory_mode == Some("source-tree")` and stringify a `{"mode":"source-tree","restriction":<sorted-array-or-null>}` object into the doc-scope slot; (e) **bug fix as side effect**: SPDX 2.3's `view_artifacts` shadowing at `spdx/document.rs:535` previously reset `file_inventory_mode` + `file_inventory_source_shapes` to `None`, silently breaking C156 AND the pre-existing m133 US4 `waybill:file-inventory-mode=full` marker. Fixed both branches at lines 480+523 to propagate the outer values. End-to-end smoke: all three formats emit C156 correctly for `--file-inventory=source-tree [--file-inventory-source-shapes=...]`; default `orphan` mode remains byte-identical (SC-005); m133 US4 marker now also emits in SPDX 2.3 as a side benefit (no test relied on the broken behavior). Workspace clippy clean, all 30 integration test binaries pass.
- [X] T011 [US1] Register C156 in the parity catalog. (a) Add row to `/Users/mlieberman/Projects/mikebom/docs/reference/sbom-format-mapping.md` after C155 (m670) with the JSON-object value shape, `SymmetricEqual` directionality, and Principle V bullet-5 native-alternative audit (matches C153 shape + C154 JSON-object pattern). (b) Add extractor macros `c156_cdx` / `c156_spdx23` / `c156_spdx3` to `waybill-cli/src/parity/extractors/{cdx.rs,spdx2.rs,spdx3.rs}` (component-scope=`document`). (c) Register the `ParityExtractor { row_id: "C156", ... }` entry in `parity/extractors/mod.rs::EXTRACTORS` + add the 6 name imports across the mass-import lines. Verified downstream by `every_catalog_row_has_an_extractor` + `holistic_parity` tests. **Delivered**: C156 doc-scope catalog row landed at `docs/reference/sbom-format-mapping.md:187` (positioned adjacent to C155 with a Principle V bullet-5 audit citing C153 and C146 as sibling patterns). Extractors landed at `cdx.rs:932-940`, `spdx2.rs:644`, `spdx3.rs:704` — all three use the `<name>_anno!` macro with the `document` scope tag. `ParityExtractor` entry registered at `parity/extractors/mod.rs:623-624` with `Directionality::SymmetricEqual, order_sensitive: false`. Six new imports added to the three mass-import lines. Workspace clippy clean; integration tests pass (`every_catalog_row_has_an_extractor` + `holistic_parity` implicit in the 30 test binaries that all passed).
- [X] T012 [US1] Add US1 integration test to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_file_tier_source_tree_m671.rs`: use a `tempfile::tempdir()` synthetic fixture (per m670 T007 precedent) with 10 mixed files (3 `.py`, 3 `.c`, 3 `.h`, 1 `README.md` control). Assert `--file-inventory=source-tree` (no restriction) emits: (a) 9 file-tier components (not the .md), (b) each with SHA-256 hash + `evidence.occurrences[].location`, (c) doc-scope C156 annotation present with `mode=source-tree, restriction=null` (SC-001, SC-002, SC-007). Contrast with `--file-inventory=orphan` on the same fixture emitting 0 file-tier components (no .py/.c/.h passes default classifier). **Delivered**: two tests at `waybill-cli/tests/scan_file_tier_source_tree_m671.rs` — `source_tree_unrestricted_emits_nine_file_components_with_hashes_and_c156` + `orphan_mode_emits_zero_file_components_on_same_fixture`. Shared `write_mixed_source_fixture` helper writes 3 `.py` + 3 `.c` + 3 `.h` + 1 `README.md` under a `tempdir`. Synthetic content uses `waybill-fixture-*` prefix per memory `feedback_fixture_synthetic_package_names`. **Deviation from T012 spec text**: the location-signal assertion checks `properties[].{name: "waybill:file-paths", value: JSON-array}` instead of `evidence.occurrences[].location` — the m133 file-tier emitter routes locations through the `waybill:file-paths` property, not through CDX-native `evidence.occurrences` (evidence.identity[] carries the hash-comparison technique instead). The location-signal is preserved losslessly; the assertion just tracks the actual emission shape. All three T012 acceptance criteria (a/b/c) covered. Both tests pass: `test result: ok. 2 passed; 0 failed`.

**Checkpoint (US1)**: `--file-inventory=source-tree` surfaces source files end-to-end + C156 annotation present + zero regressions on default-mode paths. **Ships as its own PR; MVP.**

---

## Phase 4: User Story 2 — Byte-identity for existing users (Priority: P1)

**Goal**: Every existing waybill workflow (`--file-inventory=off`/`orphan`/`full`) emits identically to v0.5.0. No accidental SBOM inflation.

**Independent Test**: The 6 golden test suites pass without regeneration; the 21-fixture kusari-sandbox sweep component counts are within ± 1% of v0.5.0 baseline on every fixture.

### Verification for US2

**Note**: US2 is verification-only. No new code. Every task confirms an existing invariant holds.

- [X] T013 [US2] Run the 6 existing golden test suites without regeneration: `cargo +stable test --workspace --no-fail-fast --test cdx_regression --test spdx_regression --test spdx3_regression --test pkg_alias_binding_us1 --test oci_pull_backward_compat --test optional_dep_classification`. Every suite MUST report `ok. N passed; 0 failed` (SC-004). If ANY suite fails, the T007 mode-gated bypass has changed default-mode behavior — investigate + fix before proceeding (do NOT regenerate goldens; that would mask the regression). **Delivered**: all six suites pass without golden regeneration — `cdx_regression 11/11`, `spdx_regression 11/11`, `spdx3_regression 11/11`, `pkg_alias_binding_us1 3/3`, `oci_pull_backward_compat 2/2`, `optional_dep_classification 2/2`. Total 40 tests, 0 failed. Confirms the T007 `classify_with_source_tree` thin-wrapper strategy preserved byte-identity on every existing golden path (SC-004 + SC-005).
- [X] T014 [US2] Run the 21-fixture kusari-sandbox sweep with the release binary. Compare against baseline `specs/670-pip-under-detection-fix/artifacts/sweep-after-2026-09-01.tsv` via `/tmp/sweep-compare.sh` (from m670 T019). Every non-cpython fixture MUST stay within ± 1% of its baseline component count (SC-003). cpython under DEFAULT mode (no `--file-inventory=source-tree`) MUST also stay at 187 components. Save the after-TSV + comparison to `specs/671-file-tier-cpython/artifacts/sweep-after-<date>.tsv` + `sweep-comparison.md`. **Delivered**: 21/21 fixtures byte-identical to m670 baseline (0.0% delta on every fixture). cpython held at 187 components under DEFAULT mode (source-tree is opt-in per FR-007). `test-rustlang` remains failing per pre-existing bug #742 (unrelated to m671). Artifacts saved to `specs/671-file-tier-cpython/artifacts/sweep-after-2026-09-01.tsv` + `sweep-comparison.md`. **Verdict: PASS — no default-mode regressions.**
- [X] T015 [US2] Add an integration test to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_file_tier_source_tree_m671.rs`: verify that scanning the T012 synthetic fixture under `--file-inventory=orphan` (default) emits 0 file-tier components AND emits NO C156 annotation. Locks the FR-007 default-mode byte-identity invariant as a per-run gate (complements the sweep-level gate in T014). **Delivered**: `orphan_mode_emits_zero_file_components_on_same_fixture` (already landed as part of T012's paired test set) satisfies T015 in full — asserts BOTH `file_component_count == 0` AND `c156_value(&doc).is_none()`. Added an inline T015-tag comment above the C156-absence assertion to make the coverage attribution explicit. Passes as part of the 6-test suite (`test result: ok. 6 passed; 0 failed`).

**Checkpoint (US2)**: Default-mode byte-identity verified at unit + integration + fixture-sweep levels. No user-visible regression.

---

## Phase 5: User Story 3 — Shape-subset restriction (Priority: P2)

**Goal**: Operators can restrict the mode to a subset of the FR-002 21-extension allowlist via `--file-inventory-source-shapes=<comma-list>`. Unknown extensions fail loudly.

**Independent Test**: Scan the m671 synthetic fixture with `--file-inventory-source-shapes=py`; verify only `.py` files emit as file-tier components. Attempt `--file-inventory-source-shapes=md`; verify CLI exits 2 with a diagnostic listing the FR-002 allowlist.

### Implementation for US3

- [X] T016 [US3] Add US3 integration test to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_file_tier_source_tree_m671.rs`: reuse T012's synthetic fixture, run with `--file-inventory=source-tree --file-inventory-source-shapes=py`; assert (a) 3 file-tier components (the 3 `.py` files only, no `.c` or `.h`), (b) C156 annotation has `restriction=["py"]` (SC-006, SC-007). **Delivered**: `source_tree_with_py_only_restriction_emits_three_components_and_c156_lists_py` — asserts exactly 3 file-tier components emitted + every emitted `name` ends with `.py` (paranoid check that the `.c`/`.h` files did not sneak through) + C156 restriction is exactly `["py"]`. Passes.
- [X] T017 [US3] Add US3 fail-loud tests to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_file_tier_source_tree_m671.rs`: shell out to the binary with `--file-inventory=source-tree --file-inventory-source-shapes=md` (invalid extension) — assert exit code 2 + stderr contains the FR-002 allowlist string (FR-009). Second case: `--file-inventory=orphan --file-inventory-source-shapes=py` (restriction under wrong mode) — assert exit code 2 + stderr mentions the cross-arg conflict (FR-001). **Delivered**: `source_tree_unknown_extension_fails_loudly_with_allowlist_diagnostic` — asserts exit code 2 (clap parse failure via `error::ErrorKind::InvalidValue`) + stderr contains `"unknown source-shape extension"` + stderr includes each of 7 sample extensions from the 21-extension allowlist. `source_tree_restriction_without_correct_mode_fails_loudly_with_cross_arg_diagnostic` — asserts non-zero exit code (waybill's own anyhow error path returns 1, not 2) + stderr mentions both flag names (`--file-inventory-source-shapes` + `source-tree`). Both pass.
- [X] T018 [US3] Verify the lex-sorted `restriction` array in C156 emission: extend T016 to assert that a mixed-order restriction (e.g., `--file-inventory-source-shapes=py,c,h`) emits `restriction=["c","h","py"]` in the annotation (locked-lex-order per BTreeSet iteration; SC-007). **Delivered**: `source_tree_mixed_order_restriction_emits_lex_sorted_c156_array` — passes `py,h,c` (reverse-lex intentionally) and asserts C156 emits `["c","h","py"]`. Locks the scan_cmd.rs explicit-sort invariant (BTreeSet iterates in enum-discriminant order which is language-grouped, NOT lex; without the explicit `.sort()` this test would fail with `["py","h","c"]`-shape output). Passes.

**Checkpoint (US3)**: Restriction subset works + fail-loud path verified. All 3 user stories complete.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T019 [P] Update `/Users/mlieberman/Projects/mikebom/CLAUDE.md` "Recent Changes" section with a milestone-671 entry describing the new `source-tree` mode, C156 annotation, zero new Cargo deps, and the SC-003 reframing (from `≥50 pypi` to `≥100 file-tier`). **Delivered**: expanded the auto-generated m671 stub at CLAUDE.md:431 into a full entry naming: (a) the SC-003 reframe, (b) the FR-002 21-extension allowlist enumerated verbatim, (c) the `SourceShape` / `SourceShapeSet` / `SourceShapeParseError` types + `classify_with_source_tree` sibling, (d) C156 doc-scope annotation shape, (e) the side bug fix in SPDX 2.3's `view_artifacts` shadowing (restored m133 US4 marker as a bonus), (f) zero-new-Cargo-deps promise, (g) 6/6 integration + 40/40 golden verification.
- [X] T020 [P] Add a memory-note file at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_file_tier_source_tree.md` documenting: (a) how the mode gates the classifier bypass, (b) the FR-002 21-extension allowlist, (c) the fail-loud CLI-parse pattern (`SourceShapeParseError`), (d) the C156 annotation shape + parity-catalog registration. Register in MEMORY.md. **Delivered**: file created with all four sections + a side-bug-fix section documenting the SPDX 2.3 `view_artifacts` shadowing gotcha (so future readers who wonder why m133 US4 markers didn't appear pre-m671 in SPDX 2.3 have the answer). MEMORY.md pointer added at line 57 with the `--file-inventory=source-tree` slug + the BTreeSet-vs-lex-sort gotcha called out inline. Cross-linked to `[[reference_pip_manifest_declared_deps]]` (m670) and `[[feedback_native_fields_first]]` (Principle V audit).
- [X] T021 [P] Update `/Users/mlieberman/Projects/mikebom/docs/reference/component-tiers.md` to document the new `source-tree` mode + FR-002 shape allowlist + the interaction with the m133 orphan-fallback contract. **Delivered**: appended a new prose block after the pre-existing `--file-inventory=full` paragraph explaining source-tree mode as an opt-in peer to orphan mode (NOT a superset). Enumerates the FR-002 21-extension allowlist, cites the classifier-bypass mechanism ("mode-gates a bypass of the excluded-extension list"), documents the companion `--file-inventory-source-shapes` flag + fail-loud parse behavior + the C156 annotation shape (both restricted-array and null-restriction forms shown). Closes with an explicit "interaction with m133 orphan fallback" clarification — source-tree is additive for source files, not a replacement for the default file-tier logic.
- [X] T022 Run the mandatory pre-PR gate: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 PATH="/Users/mlieberman/Projects/mikebom/.venv/spdx3-validate/bin:$PATH" ./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace` MUST pass green. Per Constitution v2.1.0 §Development Workflow. **Delivered**: `>>> all pre-PR checks passed.` — final line of the run. 0 FAILED test result blocks across the full run (bin + all integration test binaries + doc-tests). Workspace clippy clean. SPDX3 validator gate honored via `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` env. Ready to open PR.
- [X] T023 Verify the walker-audit allowlist at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/walk.audit-allowlist.txt` needs no new entries. Reproduce the CI logic locally per memory `feedback_walker_audit_local_check` (use `command grep` + `/usr/bin/sed` — the claude-code plugin wraps `grep`). Expected: byte-for-byte match with 12 existing entries — m671 does NOT add any new `fn walk[_(]` functions (all edits thread through `classify()` + existing walker). **Delivered**: local reproduction matches — `diff -u` between the allowlist and the live grep-with-skip-marker output shows zero delta. 12 pre-existing allowlist entries stand unchanged. m671 introduced only `classify_with_source_tree` (a sibling of `classify()`, not a new walker) + `SourceShape`/`SourceShapeSet`/`SourceShapeParseError` + a `source_tree_restriction` field on `WalkerConfig`. No new `fn walk[_(]` functions in the tree.

**Checkpoint (Phase 6)**: Docs + memory + pre-PR gate + walker-audit all green. Ready to open PR.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately.
- **Foundational (Phase 2)**: Depends on Setup. T003/T004 (`source_shape.rs` content) block T005/T006 which block ALL user-story work.
- **US1 (Phase 3)**: All of T007–T012 depend on Phase 2. T007–T011 are file-scoped; some parallelism possible.
- **US2 (Phase 4)**: All of T013–T015 depend on US1 (T007–T011). T015 is `[US2]` but also depends on T012's fixture helper.
- **US3 (Phase 5)**: T016–T018 depend on US1 (needs the CLI flag + emission wired).
- **Polish (Phase 6)**: T019/T020/T021 parallelizable + can run any time after their prerequisite user-story tasks. T022 (pre-PR gate) MUST be last. T023 can run alongside T022.

### Recommended Execution Order (single contributor)

1. **T001–T002** — scaffolding, ~15 min
2. **T003–T006** — foundational types, ~1 hour (parallel possible on T003+T004)
3. **T007–T012** — US1 core, ~3 hours
4. **T013–T015** — US2 verification, ~30 min (mostly running existing gates)
5. **T016–T018** — US3 restriction, ~1 hour
6. **T019–T023** — polish + pre-PR, ~1 hour

Total: ~1 day of focused work.

### Parallel Opportunities

- **Phase 2**: T003 || T004 (different types in same file, but write-conflict on the file — coordinate); T005 || T006 (different files).
- **Phase 3**: T007 + T008 sequential (walker depends on classify signature); T009 + T010 + T011 mostly parallelizable (different files).
- **Phase 4**: T013 || T014 || T015 (verification-only; no code changes).
- **Phase 6**: T019 || T020 || T021 (docs + memory + docs).

---

## Parallel Example: Phase 3 US1 core

```bash
# After T007 lands the classify() signature change, run in parallel:
Task: "T008 walker.rs mode-passthrough + INFO log extension"
Task: "T009 scan_cmd.rs CLI flag wiring + cross-arg validation"
Task: "T010 C156 emission at CDX/SPDX/SPDX 3 metadata sites"
Task: "T011 parity catalog + extractors"

# Then serially:
Task: "T012 US1 integration test (synthetic fixture, no restriction)"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 (Setup) + Phase 2 (Foundational) → foundation ready
2. Phase 3 (US1) → mode works + C156 emits
3. **STOP + VALIDATE**: synthetic-fixture integration test asserts ≥ 6 file-tier components. Ship as an experimental-mode PR gated behind `WAYBILL_EXPERIMENTAL=1` OR just release it — the mode is opt-in, no default-path risk.
4. US2 + US3 land in a follow-up PR.

### Incremental Delivery

1. **PR-A**: T001–T012 (US1 MVP). Ships the mode + tests + parity registration.
2. **PR-B**: T013–T015 (US2 verification hardening). Adds byte-identity gates.
3. **PR-C**: T016–T018 (US3 restriction). Adds the subset filter + fail-loud.
4. **PR-D**: T019–T023 (Polish). Docs + memory + pre-PR gate + walker-audit.

Given the small surface (~150 LoC + tests), a single PR covering all 6 phases is likely simpler than 4-way splitting. Adjust based on review-cycle preference.

### Parallel Team Strategy

Not applicable for this scope. Single contributor is efficient given the total task-hours (~1 day).

---

## Task Count Summary

- **Phase 1 Setup**: 2 tasks (T001–T002)
- **Phase 2 Foundational**: 4 tasks (T003–T006)
- **Phase 3 US1** (MVP): 6 tasks (T007–T012)
- **Phase 4 US2** (verification): 3 tasks (T013–T015)
- **Phase 5 US3** (restriction): 3 tasks (T016–T018)
- **Phase 6 Polish**: 5 tasks (T019–T023)

**Total**: 23 tasks. Small milestone — reflects the tight surface (extending existing mechanisms rather than building new).

---

## Notes

- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps task to US1/US2/US3 for traceability (Setup + Polish carry no story label)
- Zero new Cargo dependencies per plan.md Technical Context
- Fixture strategy: synthetic inline fixtures via `tempfile::tempdir()` per m670 T007 precedent — NO new files under `waybill-cli/tests/fixtures/`
- **Constitution divergence** (Principle II) inherited from m670; documented in `plan.md ## Complexity Tracking`
- **New parity-catalog row C156** — the ONLY new `waybill:*` annotation. Every emission site + extractor per T010–T011 must carry the Principle V bullet-5 audit
- **Backward-compat is a P1 story (US2), not just a polish concern** — SC-003 + SC-004 are hard gates; regressions on default-mode byte-identity MUST NOT be fixed by regenerating goldens
