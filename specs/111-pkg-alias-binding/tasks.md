---
description: "Task list for milestone 111 — Operator-supplied PURL alias for cross-tier binding"
---

# Tasks: Operator-supplied PURL alias for cross-tier binding (Option A of issue #225)

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/111-pkg-alias-binding/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Test tasks are INCLUDED because (a) the constitution's Pre-PR Verification section mandates `cargo +stable test --workspace` passing clean, (b) the spec's SC-004 byte-identity regression for no-alias scans is test-mandatory (a golden-comparison check is the only way to enforce it), and (c) the binding-correctness path is precision-critical per Principle IX (Accuracy) and warrants TDD discipline.

**Organization**: Tasks are grouped by user story. Story implementation order is US1 → US2 → US3, matching spec priority (US1 P1; US2/US3 both P2 — US3 depends on US1's emission of aliased SBOMs).

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: User story label (US1–US3); omitted on Setup / Foundational / Polish phases
- Every task names exact file paths

## Path Conventions (per plan.md)

- All implementation in `mikebom-cli/src/binding/`, `mikebom-cli/src/cli/`, `mikebom-cli/src/parity/extractors/`.
- Integration tests in `mikebom-cli/tests/`.
- Test fixtures in `mikebom-cli/tests/fixtures/pkg_alias_binding/`.
- No new crates per Constitution Principle VI.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm clean branch baseline and create the per-feature test-fixture directory tree.

- [X] T001 Run `./scripts/pre-pr.sh` on the clean `111-pkg-alias-binding` branch to verify the pre-PR gate is green BEFORE any code changes; record the baseline test count + clippy warning count (expected: zero warnings) for later regression checks.
- [X] T002 [P] Create the test-fixture directory tree at `mikebom-cli/tests/fixtures/pkg_alias_binding/` with an empty `.gitkeep` so the tree exists for fixture-generating tasks downstream.

**Checkpoint**: Branch baseline confirmed; fixture tree exists.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define the alias type system + extend the milestone-072 envelope. No user-story work can begin until these compile.

**⚠️ CRITICAL**: All user-story tasks below depend on Phase 2 completing.

### Type definitions (data-model.md → code)

- [X] T003 Create `mikebom-cli/src/binding/alias.rs` (NEW file) with module skeleton: `#![cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]` test-mod fence, public type stubs for `PurlAlias`, `AliasMap`, `AliasError` (bodies empty `// TODO T004-T006`). (Establishes the file as the foundational substrate; T004–T006 fill it.) **Implementation note:** consolidated with T004-T006 — the file landed fully implemented in one pass.
- [X] T004 [P] Implement `PurlAlias` newtype + constructor in `mikebom-cli/src/binding/alias.rs` per data-model.md. Constructor takes raw `&str` slices for LHS and RHS, runs both through `Purl::canonical()`, returns `Result<PurlAlias, AliasError>`. Rejects `LHS == RHS` with `AliasError::LhsEqualsRhs`. **Implementation note:** `Purl::canonical()` does not exist as a separate method — `Purl::new()` canonicalizes internally and the canonical form is accessed via `as_str()`. Used that pattern.
- [X] T005 [P] Implement `AliasError` thiserror enum in `mikebom-cli/src/binding/alias.rs` per data-model.md (variants: `MissingSeparator`, `MalformedLhs`, `MalformedRhs`, `ConflictingRhs`, `LhsEqualsRhs`). Each variant's `Display` message satisfies SC-003's single-line-actionable contract per contracts/cli-flags.md. **Implementation note:** `ConflictingRhs` and `LhsEqualsRhs` variants are boxed (carry `Box<ConflictingRhsPayload>` and `Box<Purl>` respectively) to satisfy `clippy::result_large_err`.
- [X] T006 Implement `AliasMap` (BTreeMap-backed) with `insert`, `get`, `iter`, `is_empty` in `mikebom-cli/src/binding/alias.rs`. `insert` rejects same-LHS-different-RHS with `AliasError::ConflictingRhs`; same-LHS-same-RHS is idempotent. **Depends on T004, T005.** (Foundational; not parallel because same file.) **Implementation note:** chose `Vec<PurlAlias>` over `BTreeMap` — `Purl` does not implement `Ord`, and for N<10 the linear-scan constant beats any tree/hash setup cost; insertion order gives deterministic SBOM emission.

### Envelope extension (binding/mod.rs)

- [X] T007 Extend `SourceDocumentBinding` in `mikebom-cli/src/binding/mod.rs` with two new optional fields `alias_from: Option<Purl>` and `alias_to: Option<Purl>`, both `#[serde(default, skip_serializing_if = "Option::is_none")]`. Per data-model.md + contracts/binding-envelope-v1.1.md. `algo` field STAYS `"v1"` — additive metadata, not algorithm change (research.md §2). Add a paired-presence invariant check in any constructor / mutator.

### Module wiring

- [X] T008 Update `mikebom-cli/src/binding/mod.rs` to declare `pub(crate) mod alias;` and re-export `PurlAlias`, `AliasMap`, `AliasError`. **Depends on T003–T006.**

### Foundational tests

- [X] T009 Add wire-compatibility unit test in `mikebom-cli/src/binding/mod.rs::tests` that round-trips a `SourceDocumentBinding` through `serde_json` BOTH with `alias_from`/`alias_to` both `None` (expecting byte-identity to pre-feature output — SC-004 prerequisite) AND with both `Some(purl)` (expecting both fields appear in the serialized JSON). **Depends on T007.** (Foundational because every user story depends on the envelope serializing correctly; unlabeled per format spec.)

**Checkpoint**: `cargo +stable check --workspace` passes; envelope round-trip test green. No behavioral change yet; all user stories can build on this foundation.

---

## Phase 3: User Story 1 — Bind a single primary binary to its source-tier ecosystem PURL (Priority: P1) 🎯 MVP

**Goal**: An operator scanning a single-binary Rust project image with `--pkg-alias "pkg:generic/baz=pkg:cargo/baz@1.0.0"` against a `--bind-to-source` source SBOM gets binding strength `verified` or `weak` for the aliased component, NOT `unknown { reason: "source-not-found-in-bind-target" }`. The applied alias is persisted in the emitted SBOM via the extended envelope.

**Independent Test**: `cargo +stable test --workspace --test pkg_alias_binding_us1`. Loads pre-built fixture source SBOM + pre-built fixture image SBOM, invokes the binder with `--pkg-alias`, asserts the affected component's `SourceDocumentBinding.strength != Unknown` AND `alias_from/alias_to` are both populated with the operator-declared values.

### Tests for User Story 1 (FAIL FIRST per TDD)

- [X] T010 [P] [US1] Build US1 fixture source SBOM at `mikebom-cli/tests/fixtures/pkg_alias_binding/source-baz.cdx.json`. Hand-author a minimal CDX 1.6 document containing a single component with PURL `pkg:cargo/baz@1.0.0` and a pre-computed milestone-072 `binding-result-v1` envelope property. Validate via `jq` that the JSON parses.
- [X] T011 [P] [US1] Build US1 fixture image SBOM at `mikebom-cli/tests/fixtures/pkg_alias_binding/image-baz.cdx.json`. Hand-author a minimal CDX 1.6 document containing a single component with PURL `pkg:generic/baz` (no version) and the milestone-072 binding-result envelope showing `strength: unknown, reason: "source-not-found-in-bind-target"`. This is the PRE-feature baseline used both as the byte-identity golden AND as the binder input. **Implementation note:** the golden is generated (not hand-authored) from the real no-alias `--image` scan output of a synthetic docker-save tarball, normalized via `normalize_cdx_for_golden` — bindings only attach on image scans, so the "binder input" is the scan itself, not this file. The component PURL is `pkg:generic/baz?file-sha256=<hex>` (the binary walker's file-level shape). Regen with `MIKEBOM_UPDATE_CDX_GOLDENS=1`.
- [X] T012 [P] [US1] Add `pkg_alias_binding_us1.rs` integration test in `mikebom-cli/tests/` that exercises the binder programmatically via `env!("CARGO_BIN_EXE_mikebom")` with `--bind-to-source <source-fixture>` + `--pkg-alias` and asserts (a) emitted component carries `alias_from = "pkg:generic/baz"` + `alias_to = "pkg:cargo/baz@1.0.0"`, (b) `strength != "unknown"`, (c) reason is NOT `source-not-found-in-bind-target`. Test should FAIL initially because the CLI flag + binder integration doesn't exist yet. **Implementation note:** the first run surfaced a real parser bug — `parse_pkg_alias` split on the first `=`, which lands inside the LHS's `?file-sha256=` qualifier. Fixed with a qualifier-aware split (first `=` whose right side starts with `pkg:` and both sides canonicalize); contracts/cli-flags.md updated.

### Implementation for User Story 1

- [X] T013 [P] [US1] Implement `parse_pkg_alias` clap `value_parser` function in `mikebom-cli/src/binding/alias.rs` (per research.md §5). Returns `Result<PurlAlias, String>` where the error is `AliasError::to_string()` (clap calls `.to_string()` on parser errors). Handles `MissingSeparator`, `MalformedLhs`, `MalformedRhs`, `LhsEqualsRhs` — but NOT `ConflictingRhs` (that's an AliasMap-insert-time check, not parse-time).
- [X] T014 [US1] Add `--pkg-alias` CLI flag declaration on `ScanArgs` in `mikebom-cli/src/cli/scan_cmd.rs` per contracts/cli-flags.md: `#[arg(long = "pkg-alias", value_name = "LHS=RHS", value_parser = binding::alias::parse_pkg_alias, action = clap::ArgAction::Append)] pub pkg_alias: Vec<PurlAlias>`. Add a default value `vec![]` entry in the `ScanArgsForTest`-helper at scan_cmd.rs:~2622 (the `enrich_args` helper) so existing tests still construct. **Depends on T013** — the clap derive references `binding::alias::parse_pkg_alias`, which T013 declares.
- [X] T015 [US1] In `mikebom-cli/src/cli/scan_cmd.rs::execute`, accumulate the parsed `Vec<PurlAlias>` into an `AliasMap` (handling `AliasError::ConflictingRhs` via `anyhow::bail!`), then thread the `AliasMap` into the existing milestone-072 binding-construction path. Locate the call to `binding::verify::*` or the `--bind-to-source` flow (around `scan_cmd.rs:1838`) and pass the map as a new function parameter. **Depends on T014.**
- [X] T016 [US1] Extend the per-component binding-compute call site in `mikebom-cli/src/binding/verify.rs::ComponentBinding::binding_for_purl` (or equivalent at `binding/verify.rs:520`) to accept the `AliasMap` and consult it: when the LHS PURL matches a configured alias, perform the source-side lookup against the RHS; populate `SourceDocumentBinding.alias_from = Some(lhs_canonical)` + `alias_to = Some(rhs)` regardless of strength outcome (so the alias declaration is visible even when the RHS isn't found). **Depends on T007, T015.** **Implementation note:** the scan-time per-component binding loop actually lives in `scan_cmd.rs::attach_bindings_to_components`, not `verify.rs` — the alias consult + envelope stamping were implemented there. `verify.rs` remains the verify-time path (US3 / T032).
- [X] T017 [US1] Add the new `unknown` reason value `"alias-target-not-found-in-bind-target"` to the binding-build logic in `mikebom-cli/src/binding/verify.rs`: when an alias was applied (`alias_from.is_some()`) AND the RHS PURL was not found in `--bind-to-source`, emit `strength = Unknown` with this reason instead of the existing `source-not-found-in-bind-target`. **Depends on T016.** **Implementation note:** implemented in `scan_cmd.rs::attach_bindings_to_components` (see T016 note); the verify-time analog is T033.
- [X] T018 [US1] Emit the FR-010 warning in `mikebom-cli/src/cli/scan_cmd.rs::execute` when `args.pkg_alias` is non-empty AND `args.bind_to_source` is `None`: `tracing::warn!(count = ..., "--pkg-alias declared but --bind-to-source was not supplied; aliases have no effect ...")` per contracts/cli-flags.md. The AliasMap is dropped silently in this case (no alias persisted in the SBOM per FR-010).
- [X] T019 [US1] Emit the FR-011 info log for unused aliases: track which LHS PURLs the binder actually matched against during the per-component loop; after the loop, log unmatched LHSes at info level per contracts/cli-flags.md. **Depends on T016.**
- [X] T020 [P] [US1] Add unit tests for `parse_pkg_alias`, `AliasMap::insert` conflict detection, and `PurlAlias::canonical()` round-trip in `mikebom-cli/src/binding/alias.rs::tests`. Cover the 5 `AliasError` variants and the idempotent same-LHS-same-RHS insert. **Include one explicit non-`pkg:generic` LHS case** (e.g., `pkg:deb/debian/foo@1.0=pkg:github/foo/foo@1.0`) to assert spec.md's edge case "alias whose LHS is not in the generic PURL namespace" — the parser MUST accept any PURL scheme, not just `pkg:generic/*`.
- [X] T021 [US1] Run `cargo +stable test --workspace --test pkg_alias_binding_us1`; the test from T012 should now PASS. If it doesn't, diff the emitted envelope against the expected shape — likely candidates are missing `Purl::canonical()` on input or the alias not being threaded through to the binding-compute call site.
- [X] T022 [US1] Add a byte-identity regression test in `mikebom-cli/tests/pkg_alias_binding_us1.rs` (SC-004): invoke the binder against the same fixtures with NO `--pkg-alias` flag and assert the emitted SBOM is byte-identical (post-canonicalization) to the pre-feature golden at `mikebom-cli/tests/fixtures/pkg_alias_binding/image-baz.cdx.json`. Also asserts the envelope shape directly: `strength: unknown`, `reason: source-not-found-in-bind-target`, and NO `alias_from`/`alias_to` keys.
- [X] T023 [US1] Run the full `./scripts/pre-pr.sh` and confirm zero new clippy warnings + all existing tests still pass + the new US1 tests pass.

**Checkpoint**: US1 acceptance scenarios 1, 2, 3 pass. SC-001 (one-flag move from Unknown to Verified/Weak) and SC-004 (byte-identity for no-alias path) are verifiable. MVP shippable here as a standalone PR if desired.

---

## Phase 4: User Story 2 — Bind multiple primary binaries in a workspace project (Priority: P2)

**Goal**: An operator with two primary binaries in a Cargo workspace can declare two `--pkg-alias` flags (or one `MIKEBOM_PKG_ALIAS` env var with comma-separated entries) and both components bind correctly. Env-var and repeated-flag forms produce identical SBOMs.

**Independent Test**: `cargo +stable test --workspace --test pkg_alias_binding_us2`. Loads a two-binary fixture; runs the binder twice — once with two repeated `--pkg-alias` flags, once with the env-var form — asserts both runs produce identical aliased bindings on both components.

### Tests for User Story 2 (FAIL FIRST)

- [ ] T024 [P] [US2] Build US2 workspace fixtures at `mikebom-cli/tests/fixtures/pkg_alias_binding/`:
  - `workspace-multi.cdx.json` — source-tier with two components `pkg:cargo/baz@1.0.0` + `pkg:cargo/baz-debug@1.0.0` AND a sibling image-tier `image-multi.cdx.json` with two `pkg:generic/baz` + `pkg:generic/baz-debug` components (the basic two-binary case);
  - `workspace-five.cdx.json` + `image-five.cdx.json` — five-binary source-tier and image-tier pair (`pkg:cargo/baz@1.0.0` through `pkg:cargo/baz5@1.0.0` / `pkg:generic/baz` through `pkg:generic/baz5`) for the SC-005 literal-threshold assertion;
  - `image-multi-same-rhs.cdx.json` — same-source-different-image fixture with two distinct image-tier components (`pkg:generic/baz-cli` and `pkg:generic/baz-daemon`) both intended to alias to the SAME source-tier `pkg:cargo/baz@1.0.0`, for the U1 non-collapse assertion in T025.
- [ ] T025 [US2] Add `pkg_alias_binding_us2.rs` integration test in `mikebom-cli/tests/` covering: (a) two repeated `--pkg-alias` flags against the two-binary fixture, both components bind; (b) one `MIKEBOM_PKG_ALIAS=entry1,entry2` env var, identical result to (a); (c) cross-check identity of emitted SBOM bytes between (a) and (b) so future operators trust the two surfaces are interchangeable; (d) **SC-005 literal-threshold assertion** — five aliases via a single `MIKEBOM_PKG_ALIAS` env var against `image-five.cdx.json`, all five components bind with `alias_from`/`alias_to` populated, scan invocation exits with status 0 and no per-binary CLI ergonomics penalties (no required-config files, no separate scan invocations, no `--pkg-alias-file` deferred-feature dependency); (e) **U1 non-collapse assertion** — two distinct LHS aliases targeting the SAME RHS against `image-multi-same-rhs.cdx.json`, both LHS image-tier components retain their distinct identities in the emitted SBOM (verified by asserting both `pkg:generic/baz-cli` and `pkg:generic/baz-daemon` appear as separate `components[]` entries, each with its own `alias_from`/`alias_to` populated to the shared RHS). Test should FAIL because the env-var-parse path doesn't exist yet.

### Implementation for User Story 2

- [X] T026 [US2] Implement `MIKEBOM_PKG_ALIAS` env-var parsing in `mikebom-cli/src/cli/scan_cmd.rs`: read the env var via `std::env::var("MIKEBOM_PKG_ALIAS")`, split on `,`, trim whitespace per entry, run each entry through `binding::alias::parse_pkg_alias`, append to the CLI-derived `Vec<PurlAlias>`. Per contracts/cli-flags.md. Empty entries (`,,`) silently skipped. Same `ConflictingRhs` detection applies across the union. **Implementation note:** landed early in the US1 wire-up PR as `scan_cmd.rs::build_pkg_alias_map` (CLI flags first, then env entries, shared conflict detection). US2 fixtures + integration tests (T024/T025/T027) remain.
- [ ] T027 [US2] Run `cargo +stable test --workspace --test pkg_alias_binding_us2`; iterate until all three scenarios PASS. Debug the env-var path by adding `tracing::debug!` on the parsed entries during dev.
- [ ] T028 [US2] Re-run `./scripts/pre-pr.sh` to confirm no regression in US1 (single-binary path) caused by US2's env-var addition.

**Checkpoint**: US2 acceptance scenarios 1 and 2 pass. SC-005 (workspace-of-5 aliases without ergonomic penalty) is verified DIRECTLY at N=5 by T025 scenario (d); operator-of-N for arbitrary larger N extrapolates from the linear composition. U1 non-collapse invariant (same RHS aliased by multiple LHSes retains distinct image-tier identities) verified by T025 scenario (e).

---

## Phase 5: User Story 3 — Verify-binding consumes alias-bearing SBOMs without re-supplying the alias (Priority: P2)

**Goal**: After a scan persists an alias-bearing SBOM, an auditor running `mikebom verify-binding image.cdx.json source.cdx.json` (no CLI alias supplied) reproduces the same binding strength AND surfaces the alias via a new `applied_alias: "<LHS> → <RHS>"` sibling field in the output. Same applies to `trace-binding`.

**Independent Test**: `cargo +stable test --workspace --test pkg_alias_binding_us3`. Reuses the US1 emitted SBOM as input. Runs `verify-binding` programmatically; asserts the output JSON contains `applied_alias = "pkg:generic/baz → pkg:cargo/baz@1.0.0"` AND `binding.strength` matches the original scan-time result.

### Tests for User Story 3 (FAIL FIRST)

- [ ] T029 [US3] Add `pkg_alias_binding_us3.rs` integration test in `mikebom-cli/tests/` covering: (a) round-trip — alias-bearing SBOM produced by the US1 binder, run through verify-binding, asserts `applied_alias` sibling field present + binding strength matches; (b) RHS-missing scenario — same image SBOM run against a different source SBOM whose component list does NOT include the aliased RHS, asserts `binding.strength = "unknown"` AND `binding.reason = "alias-target-not-found-in-bind-target"`. Test should FAIL because the `applied_alias` output field doesn't exist yet.

### Implementation for User Story 3

- [ ] T030 [P] [US3] Add `applied_alias: Option<String>` field to the per-component verify-binding output struct in `mikebom-cli/src/cli/verify_binding_cmd.rs`. Populated as `format!("{} → {}", alias_from, alias_to)` (UTF-8 `→` U+2192 per contracts/binding-envelope-v1.1.md) when the envelope's `alias_from` and `alias_to` are both `Some`. `None` otherwise; `#[serde(skip_serializing_if = "Option::is_none")]` so non-aliased outputs stay byte-identical to pre-feature.
- [ ] T031 [P] [US3] Add the same `applied_alias` field to the trace-binding output struct in `mikebom-cli/src/cli/trace_binding_cmd.rs` (it's a separate command but emits a related per-component object). Mirror the population logic from T030.
- [ ] T032 [US3] Update the binding-builder in `mikebom-cli/src/binding/verify.rs::ComponentBinding::binding_for_purl` (line ~520) to honor a recorded alias when reading an alias-bearing input SBOM: when the input envelope has `alias_from` + `alias_to`, perform the source-side lookup against `alias_to` instead of the component's own PURL. This is the verify-time analog of T016's scan-time logic.
- [ ] T033 [US3] Add the FR-007 `alias-target-not-found-in-bind-target` reason to the verify-binding path as well: when the input envelope had an alias but the RHS isn't in the supplied source SBOM at verify time, emit `strength = Unknown` with this reason (distinct from `source-not-found-in-bind-target`).
- [ ] T034 [US3] Run `cargo +stable test --workspace --test pkg_alias_binding_us3`; iterate until both scenarios PASS.
- [ ] T035 [US3] Re-run `./scripts/pre-pr.sh` to confirm no regression in US1 (scan-time path) or US2 (multi-binary path).

**Checkpoint**: US3 acceptance scenarios 1 and 2 pass. SC-002 (verify-binding reproduces scan-time strength without CLI re-supply) is verifiable.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Format-parity coverage, docs, CHANGELOG, full pre-PR gate.

- [ ] T036 [P] Update CDX parity extractor at `mikebom-cli/src/parity/extractors/cdx.rs` to surface `alias_from` / `alias_to` from the existing binding-result envelope into the parity-comparator's normalized form. The C56 row's parity check must continue to pass when both fields are present (round-trip CDX ↔ SPDX 2.3 ↔ SPDX 3 must preserve the alias context).
- [ ] T037 [P] Update SPDX 2.3 parity extractor at `mikebom-cli/src/parity/extractors/spdx2.rs` with the same envelope-field extraction. The `MikebomAnnotationCommentV1` wrapper already carries the JSON-encoded envelope; the extractor just needs to surface the two new fields.
- [ ] T038 [P] Update SPDX 3 parity extractor at `mikebom-cli/src/parity/extractors/spdx3.rs` with the same. The `Annotation.statement` envelope carries the same shape as SPDX 2.3.
- [ ] T039 [P] Add a cross-format parity round-trip test in `mikebom-cli/tests/` (extend an existing parity test rather than adding a new file): emit the same aliased component in CDX 1.6 + SPDX 2.3 + SPDX 3, run the existing parity comparator, assert zero invariant violations. Verifies SC-006.
- [ ] T040 [P] Update `docs/reference/sbom-format-mapping.md` C56 row with a note: "Envelope MAY additionally carry `alias_from` + `alias_to` fields when an operator-supplied `--pkg-alias` was applied (milestone 111). Both fields are paired-presence (both populated or both absent)." No new C-row.
- [ ] T041 [P] Add a CHANGELOG entry under `CHANGELOG.md`'s `## [Unreleased]` section: "Operator-supplied PURL alias for cross-tier binding (milestone 111, Option A of #225). New `--pkg-alias LHS=RHS` flag on `mikebom sbom scan` (and `MIKEBOM_PKG_ALIAS` env var) lets operators declare that a binary-tier PURL should be treated as a source-tier PURL during `--bind-to-source` binding. Aliased components reach `Verified`/`Weak` strength instead of `Unknown`. Verify-binding output gains an `applied_alias: \"<LHS> → <RHS>\"` sibling field. See `specs/111-pkg-alias-binding/spec.md`."
- [ ] T042 Run the full `cargo +stable test --workspace` one more time + manually exercise the quickstart.md scenarios 1–3 against the test fixtures to confirm operator-facing UX matches the documented contract.
- [ ] T043 Final pre-PR pass: `./scripts/pre-pr.sh` MUST exit zero. Inspect every clippy line in the output even at zero-warning state — sometimes new code triggers nit-level lints that warrant a `#[allow(...)]` with a comment.

---

## Dependencies

Sequential dependencies (each phase blocks the next):

```text
Phase 1 (Setup) ──▶ Phase 2 (Foundational) ──▶ Phase 3 (US1 MVP)
                                              │
                                              └─▶ Phase 4 (US2) ─┐
                                                                 ├─▶ Phase 6 (Polish)
                                              └─▶ Phase 5 (US3) ─┘
```

US3 depends on US1 (verify-binding needs aliased SBOMs to verify; US1 produces them). US2 is mostly orthogonal to US3 (env-var parsing is independent of verify-binding output). Phases 4 and 5 may proceed in parallel after Phase 3 completes.

Parallel opportunities:
- Within Phase 2: T004, T005 are independent type definitions in the same file (parallel-conceptual; serialize the actual edits if same-file conflict matters).
- Within Phase 3: T010, T011, T013, T014, T020 parallelize across distinct files. T015–T019 are sequential due to file overlap in `cli/scan_cmd.rs` + `binding/verify.rs`. T012 (integration test scaffold) parallelizes with the implementation tasks but should be written FIRST (TDD discipline).
- Within Phase 5: T030 and T031 parallelize (different files: `verify_binding_cmd.rs` vs `trace_binding_cmd.rs`).
- Within Phase 6: T036–T041 are all parallel (different files).

## Implementation Strategy

**MVP scope** (most operator-visible value with minimum risk):

The MVP is **US1 alone** (Phases 1, 2, 3). It delivers the textbook source-to-image binding workflow — exactly the issue #225 problem statement. Operators who only need the single-primary-binary case can ship MVP scope first.

US2 (env-var + multi-flag) is a separable second PR — the underlying machinery is identical; only the input-parsing surface grows.

US3 (verify-binding output) is a separable third PR — the persistence is already done in US1; US3 just makes the existing data operator-visible at verify time.

**Suggested commit cadence** (each commit independently passes pre-PR gate):

1. Phase 1 + Phase 2 (foundational types, envelope extension, no behavioral change) — 1 PR.
2. Phase 3 (US1 MVP) — 1 PR. CI gate green; SC-001 + SC-004 verified.
3. Phase 4 (US2 env-var + multi-flag) — 1 PR. Quick wrap on the input surface.
4. Phase 5 (US3 verify-binding output) — 1 PR.
5. Phase 6 (parity-extractors + docs + CHANGELOG) — 1 PR.

Five PRs is the conservative max. Phases 4 + 5 could combine into one PR if review bandwidth is tight; Phase 6 could combine with whichever phase ships last. Three PRs is the floor (foundation + MVP+US1, US2+US3 combined, polish).

## Format validation

All tasks conform to the `- [ ] TaskID [P?] [Story?] Description with file path` format per the speckit-tasks rules:
- 43 total tasks (T001–T043).
- 14 of those are `[P]` marked (T014's `[P]` removed during /speckit-analyze remediation O1 — it depends on T013's parser signature).
- 25 carry a `[Story]` label (US1: 14, US2: 5, US3: 6); the remaining 18 are Setup / Foundational / Polish with no story label per the format spec.
- Every task names exact file paths or scripts.
- Post-analyze remediations applied: C1 (T024+T025 extended to N=5 fixture + assertion), C2 (T020 extended with non-`pkg:generic` LHS case), O1 (T014 `[P]` removed + T013 dependency noted), U1 (T024+T025 extended with multi-LHS-same-RHS non-collapse fixture + assertion).
