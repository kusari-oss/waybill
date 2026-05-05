---
description: "Task list for milestone 073 — identifiers (built-in + user-defined; auto-detect repo: from git origin; dedicated --repo/--image-id/--attestation/--id flags)"
---

> **Post-implementation rename note (2026-05-03, before merge)**: this milestone shipped under the renamed concept "identifiers" (not "source identifiers") with a dedicated-flag CLI (`--repo` / `--git-ref` / `--image-id` / `--attestation` / `--id <scheme>=<value>`) instead of the originally-drafted `--with-source <scheme>:<value>`. See spec.md's prepended note for the full rename scope. The task list below uses the original draft's terminology — treat references to `--with-source` and `source_identifiers` / `mikebom:source-identifiers` / `source_identifiers_*.rs` / `docs/reference/source-identifiers.md` as historical references to the equivalents renamed pre-merge: `--repo`/`--image-id`/`--id`, `identifiers` / `mikebom:identifiers` / `identifiers_*.rs` / `docs/reference/identifiers.md`.

# Tasks: Source identifiers — built-in + user-defined

**Input**: Design documents from `/specs/073-source-identifiers/`
**Prerequisites**: spec.md ✅ (with 3 clarifications), plan.md ✅, research.md ✅, data-model.md ✅, contracts/{identifier-shape,source-identifiers-annotation}.md ✅, quickstart.md ✅

## Format: `[ID] [P?] [Story?] Description`

## Phase 1: Setup

- [X] T001 Confirm working tree clean and on branch `073-source-identifiers`. Confirm `cargo +stable test --workspace` passes baseline before any edits (so any new failure is attributable to this milestone).
- [X] T002 Create the new submodule skeleton at `mikebom-cli/src/binding/identifiers/{mod.rs,auto_detect.rs,validators.rs}` with `pub mod ...` declarations in `mod.rs` and `pub mod identifiers;` added to `mikebom-cli/src/binding/mod.rs`. Each file is empty stubs at this stage. Verify `cargo +stable check -p mikebom` builds clean.

## Phase 2: Foundational (blocking prerequisites for all user stories)

- [X] T003 [P] Implement the data types in `mikebom-cli/src/binding/identifiers/mod.rs` per data-model.md: `Identifier`, `SchemeName` (newtype with `^[a-z][a-z0-9_-]*$` regex validator per FR-004), `IdentifierValue` (newtype, non-empty validation), `IdentifierKind { Builtin(BuiltinScheme), UserDefined }`, `BuiltinScheme { Repo, Git, Image, Attestation }` enum with `from_scheme_name(&SchemeName) -> Option<Self>`, `cdx_external_reference_type(self) -> &'static str` (per research.md §2 mapping), `spdx23_reference_category(self) -> &'static str` (always `"PERSISTENT-ID"`), and `IdentifierError` (thiserror enum per data-model.md). Implement `Identifier::parse(raw: &str) -> Result<Self, IdentifierError>` that splits on the FIRST `:` only (per VR-003). Add inline unit tests covering: (a) `SchemeName::new` accepts/rejects per FR-004 regex; (b) `IdentifierValue::new` rejects empty; (c) `Identifier::parse` splits on first `:` only with values containing additional `:`; (d) `BuiltinScheme::from_scheme_name` recognizes the 4 built-ins and returns `None` for user-defined; (e) `BuiltinScheme::cdx_external_reference_type` per scheme.

- [X] T004 [P] Implement per-built-in-scheme validators in `mikebom-cli/src/binding/identifiers/validators.rs` per research.md §1. One `validate_<scheme>(value: &str) -> Result<(), IdentifierError>` function per built-in scheme: `validate_repo` (URL or git-style ssh URL), `validate_git` (URL + optional `#<fragment>`), `validate_image` (per the Q3 canonical regex `^([a-zA-Z0-9.\-_/]+/)?[a-zA-Z0-9.\-_/]+(:[a-zA-Z0-9.\-_]+)?(@sha256:[a-fA-F0-9]{64})?$`), `validate_attestation` (any RFC 3986 URI). Wire into `Identifier::parse` so a built-in scheme with an invalid value emits a `tracing::warn!` and downgrades the `kind` to `IdentifierKind::UserDefined` per VR-005. Add unit tests for each validator's accept/reject cases plus the soft-fail downgrade path.

- [X] T005 [US-foundational] Add a new `ParityExtractor` row at `mikebom-cli/src/parity/extractors/mod.rs` for `mikebom:source-identifiers` (row id `C47` — next free C-section after milestone-072's C46). Directionality `SymmetricEqual` per `contracts/source-identifiers-annotation.md` C-3. Per-format extractors at `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` reuse the existing macros (`cdx_anno!`, `spdx23_anno!`, `spdx3_anno!`) — same shape as milestone-072's C46 row. Confirm the catalog ordering invariant `extractors_table_is_sorted_by_row_id` still passes by inserting at the alphabetical position. Add a row to `docs/reference/sbom-format-mapping.md`'s parity-catalog table.

## Phase 3: User Story 1 — Auto-detected `repo:` identifier from git checkout (P1) 🎯 MVP

### Auto-detection logic (research.md §4)

- [X] T006 [US1] Implement `auto_detect_repo_identifier(scan_root: &Path) -> Option<Identifier>` in `mikebom-cli/src/binding/identifiers/auto_detect.rs` per research.md §4. Three-step fallback (per Q1 clarification): (1) try `git remote get-url origin`; if that fails, (2) try `git remote get-url upstream`; if that fails, (3) `git remote` to list all remotes alphabetically, take the first, then `git remote get-url <first>`. Subprocess invocations use `Command::new("git").args(["-C", scan_root, ...])` mirroring milestone 053's `git describe` pattern. Failure → `tracing::info!` log naming the reason, return `None`. Set the resulting `Identifier.source_label` to `"auto-detected from git remote `<name>`"` (with conditional suffix when `origin`/`upstream` are absent — per FR-007). Add unit tests using a `tempfile::tempdir()` + `git init` + `git remote add origin/upstream <url>` fixture covering: (a) `origin` only → uses `origin`; (b) `upstream` only → uses `upstream` with the conditional suffix; (c) third remote only → uses first-listed alphabetical with the conditional suffix; (d) no `.git/` dir → returns `None`; (e) `.git/` dir but no remotes → returns `None` with info log.

### Plumbing through ScanArtifacts

- [X] T007 [US1] Add `pub source_identifiers: Vec<Identifier>` field to `ScanArtifacts<'a>` at `mikebom-cli/src/generate/mod.rs` per data-model.md. Default to `vec![]`. Update existing `ScanArtifacts` construction sites in `mikebom-cli/src/cli/scan_cmd.rs::execute`, `mikebom-cli/src/cli/trace_cmd.rs::execute`, and any other callers (search via `grep -rn 'ScanArtifacts {' mikebom-cli/src/`) to add `source_identifiers: vec![]` to the struct-update syntax. Verify `cargo +stable check -p mikebom` builds clean.

### Per-format emission

- [X] T008 [P] [US1] Wire built-in-identifier emission into CDX `metadata.component.externalReferences[]` at `mikebom-cli/src/generate/cyclonedx/metadata.rs`. Iterate `artifacts.source_identifiers`, filter `is_builtin()`, emit one `externalReferences[]` entry per identifier with `type` mapped via `BuiltinScheme::cdx_external_reference_type`, `url` = the identifier value, `comment` = the `source_label` (or `"manual --with-source"` when source_label is absent). Order: auto-detected entries first, then manual in supply order (per FR-009 / VR-008). Add a unit test with a synthesized `ScanArtifacts` carrying 2 built-in identifiers asserting correct CDX shape per `contracts/source-identifiers-annotation.md` C-1.

- [X] T009 [P] [US1] Wire built-in-identifier emission into the SPDX 2.3 dual carrier (per Q2 clarification): (a) main-module `Package.externalRefs[]` at `mikebom-cli/src/generate/spdx/packages.rs` — one `externalRefs[]` entry per built-in identifier with `referenceCategory: "PERSISTENT-ID"`, `referenceType: <scheme-name>`, `referenceLocator: <value>`, optional `comment: <source_label>`; (b) `creationInfo.creators[]` redundant text at `mikebom-cli/src/generate/spdx/document.rs` — one entry per built-in identifier as `"Tool: mikebom-<version> source: <full-identifier>"`. Order matches T008. Add a unit test asserting both carriers fire for a 2-identifier scan.

- [X] T010 [P] [US1] Wire identifier emission into SPDX 3 `Element.externalIdentifier[]` on the `SpdxDocument` element at `mikebom-cli/src/generate/spdx/v3_document.rs`. Per `contracts/source-identifiers-annotation.md` C-1 SPDX 3 section: emit ONE `externalIdentifier[]` entry per identifier (built-in AND user-defined — SPDX 3's open-typed model handles both natively, no separate annotation needed). `externalIdentifierType` = scheme name verbatim; `identifier` = value; `comment` = source_label. Add a unit test asserting both built-in and user-defined identifiers appear in the SPDX 3 carrier.

### US1 integration test

- [X] T011 [US1] Create `mikebom-cli/tests/source_identifiers_emission.rs`. Test the auto-detection happy path end-to-end: `tempfile::tempdir()` → `git init` → `git remote add origin git@github.com:test/foo.git` → write a minimal `Cargo.toml` + `Cargo.lock` (so the scan has something to scan) → run `mikebom sbom scan --path <tempdir>` via `Command::new(env!("CARGO_BIN_EXE_mikebom"))` → parse the emitted CDX/SPDX 2.3/SPDX 3 SBOMs → assert each carries the auto-detected `repo:git@github.com:test/foo.git` identifier in the right slot per `contracts/source-identifiers-annotation.md`. Test in all 3 formats. Plus a no-git-dir variant asserting no identifier is emitted (no error). Include the 3-step fallback sub-tests: origin-only, upstream-only, third-remote-only.

## Phase 4: User Story 2 — Manual `--with-source` flag (P1)

### CLI flag wiring

- [X] T012 [US2] Add `--with-source <Vec<String>>` flag to `ScanArgs` at `mikebom-cli/src/cli/scan_cmd.rs:65` (the `ScanArgs` struct). Repeatable per FR-002 — clap field `pub with_source: Vec<String>` with `#[arg(long = "with-source", value_name = "SCHEME:VALUE")]`. Help text documents the syntax and mentions `--with-source repo:git@... --with-source acme_corp_id:abc123` as a worked example. Default is empty vec. Reject malformed scheme prefix at parse time by parsing each value through `Identifier::parse` in a clap `value_parser`; surface `IdentifierError::InvalidSchemeName` as a clap parse error so the scan exits non-zero before any work begins (per the spec edge case "Empty `--with-source` value: malformed flag — clap rejects at parse time").

### Identifier resolution pipeline

- [X] T013 [US2] Implement the resolution pipeline in `mikebom-cli/src/cli/scan_cmd.rs::execute` near the top of the function (before the per-component scan walk): (1) call `auto_detect_repo_identifier(scan_root)` and capture the optional `Identifier`; (2) parse each `args.with_source` entry into an `Identifier` via `Identifier::parse`; (3) merge the lists per FR-006 + FR-009 override-position rule (per analyze F1 fix): start with `[auto_detected, manual_in_supply_order]`, then for each manual entry that deduplicates against an auto-detected entry on `(scheme, value)`, REPLACE the auto-detected entry IN PLACE with the manual entry (the manual entry inherits the auto-detected position, NO shift; the auto-detected `source_label` is replaced with the manual entry's). For each manual entry whose `(scheme, value)` is NEW (no auto-detected match), append in supply order. For manual-vs-manual collision on `(scheme, value)`, first-supplied wins. Info-log records both URLs when an override fires. (4) Set `ScanArtifacts.source_identifiers` to the resolved Vec. Add unit tests for the resolution function alone covering: (a) auto-detected-only; (b) manual-only; (c) manual entry inherits auto-detected position when scheme+value match; (d) manual entry of different value drops auto-detected entry (collapsed, manual follows in supply order); (e) two manual entries with same `(scheme, value)` deduplicate.

### User-defined annotation emission

- [X] T014 [US2] Wire user-defined identifier emission into the `mikebom:source-identifiers` annotation per `contracts/source-identifiers-annotation.md` C-2. Three sites: (a) CDX `metadata.properties[]` at `mikebom-cli/src/generate/cyclonedx/metadata.rs` — JSON-encode the array per the contract; (b) SPDX 2.3 document-level `annotations[]` at `mikebom-cli/src/generate/spdx/document.rs` — wrap in the `MikebomAnnotationCommentV1` envelope (reuse the existing `build_annotation` helper from milestone 071); (c) SPDX 3 — NO separate annotation; user-defined identifiers ride T010's `Element.externalIdentifier[]` natively (per contract C-1's note: SPDX 3 multi-identifier model handles both built-in and user-defined). The annotation array is sorted lex by `(scheme, value)` per FR-009 / contract C-4. Emit ONLY when the user-defined entry set is non-empty per VR-007 (preserves cross-format byte-identity for non-user-defined-namespace scans). Add a unit test asserting empty user-defined set → no annotation emitted; non-empty → annotation emitted with sorted-lex array.

### US2 integration test

- [X] T015 [US2] Create `mikebom-cli/tests/source_identifiers_manual.rs`. Test cases: (a) `tempdir` (NOT a git checkout) + `--with-source repo:git@...` → identifier emits in the standards-native VCS slot; (b) `--with-source acme_corp_id:abc123 --with-source internal_ticket:PROJ-456` → both emit under `mikebom:source-identifiers` annotation in CDX + SPDX 2.3 (and natively in SPDX 3 per T010); (c) git checkout + `--with-source repo:git@<different-url>` → manual override wins, info-log captured, manual entry inherits auto-detected position per FR-006 override-position rule; (d) duplicate `--with-source repo:<same-url>` twice → deduplicated; (e1) **empty value** — `--with-source repo:` (scheme followed by empty value) → clap parse error citing `IdentifierError::EmptyValue` per VR-002, exit non-zero before any scan work; (e2) **malformed scheme** (per analyze F2 fix) — `--with-source NOT_VALID:value` (uppercase scheme fails the FR-004 regex `^[a-z][a-z0-9_-]*$`) → clap parse error citing `IdentifierError::InvalidSchemeName`, exit non-zero before any scan work. The two error paths produce DIFFERENT error messages and MUST be asserted separately. (f) malformed value for built-in scheme `--with-source repo:obviously_invalid` → soft-fail to opaque per VR-005, identifier appears under `mikebom:source-identifiers` not the VCS slot, warn log captured.

## Phase 5: User Story 3 — Same mechanism on `mikebom trace` + image-tier (P2)

### Trace command flag

- [X] T016 [US3] Add `--with-source <Vec<String>>` flag to `TraceArgs` at `mikebom-cli/src/cli/trace_cmd.rs` (mirror T012's pattern). Wire the resolution pipeline (T013's logic) into `trace_cmd::execute` BUT skip the auto-detection step — build-tier scans don't auto-detect (per FR-008). Manual flags only. Plumb through to `ScanArtifacts.source_identifiers` for the build-tier emission path. Add a unit test asserting trace_cmd accepts the flag and threads identifiers through.

### Image-tier auto-detection

- [X] T017 [US3] Implement `image_reference_to_identifier(resolved: &ResolvedImage) -> Option<Identifier>` in `mikebom-cli/src/binding/identifiers/auto_detect.rs` per research.md §3. Synthesize the `image:<registry>/<name>:<tag>@sha256:<digest>` form from the `ResolvedImage` struct's fields (omitting registry / digest as documented per the Q3 clarified shape). Find `ResolvedImage` (or equivalent) in the existing image-resolution code via `grep -rn 'ResolvedImage' mikebom-cli/src/scan_fs/` — it's the post-pull/post-load output struct. Wire the call into `cli/scan_cmd.rs::execute` for `--image` mode (same pipeline as T013 but with `image:` instead of `repo:`). Add unit tests for the synthesis function covering: full form (registry+name+tag+digest), tarball-only (no registry), pre-distribution-spec (no digest), each producing the canonical shape per `contracts/identifier-shape.md` C-3.

### US3 integration test

- [X] T018 [US3] Create `mikebom-cli/tests/source_identifiers_per_tier.rs`. Three test cases: (a) `mikebom trace --with-source repo:git@... -- echo 'fake build'` (using a no-op build command so the test doesn't actually exercise eBPF — feature-gated; if `ebpf-tracing` not enabled, mock the trace harness similarly to existing trace_cmd tests) → build-tier SBOM emits with the manual identifier; (b) `mikebom sbom scan --image <local-tarball>` → image-tier SBOM emits with auto-detected `image:` identifier in the canonical shape; (c) cross-tier consistency check — same scheme used on path / image / trace tiers ALL ride the same per-format carriers (no tier-specific divergence per FR-008).

## Phase 6: User Story 4 — Forward-looking determinism handshake (P3)

- [X] T019 [US4] Create `mikebom-cli/tests/source_identifiers_determinism.rs` testing the FR-009 / SC-002 determinism contract. Two test cases: (a) Run the same scan twice with identical inputs → both emitted SBOMs (in all 3 formats) have byte-identical identifier slots; (b) An external walker (implemented inline in the test as a JSON-traversal function — not the published Python; this is the Rust analog) extracts every identifier from each format and produces a sorted `(scheme, value)` list — the lists from CDX, SPDX 2.3, and SPDX 3 of the same scan MUST be equal (cross-format consistency). This satisfies SC-002 + the milestone-074 forward-looking handshake (SC-005) — proves emission is parseable + deterministic + cross-format-consistent enough that 074's resolution layer can key off it.

## Phase 7: Polish

- [X] T020 Author `docs/reference/source-identifiers.md` per FR-010 — published reference for external SBOM consumers writing their own identifier extractors. Sections per the published-doc model from milestone-072's `cross-tier-binding.md` and milestone-071's `conformance-harness-guide.md`: §1 wire format (`<scheme>:<value>`, first-`:`-only split, FR-004 regex); §2 the 4 built-in schemes + their per-format carrier table (mirror `contracts/source-identifiers-annotation.md`); §3 the user-defined-passthrough rule + `mikebom:source-identifiers` envelope shape; §4 auto-detection (3-step git fallback for `repo:`, `image:` synthesis from resolved reference); §5 determinism contract; §6 a runnable `jq` decode recipe per format mirroring quickstart.md Recipe 7; §7 stability commitment; §8 pointer back to `cross-tier-binding.md` (milestone 074 will use these identifiers as resolution keys). Mirror the milestone-071/072 published-guide pattern — external auditor MUST be able to write a working extractor from this doc alone (SC-006 bar).

- [X] T021 Update `docs/design-notes.md` with a new section "Source identifiers (milestone 073)" pointing to the new `source-identifiers.md` and explaining the operator-visible behavior: when does auto-detection fire, what schemes are recognized, what the user-defined-passthrough does, and how it sets up milestone 074's `--bind-to-source <identifier>` resolution path. Brief — ~30-50 lines like milestone-072's design-notes addition.

- [X] T022 Regenerate the byte-identity goldens for git-tracked source-tier fixtures (per FR-012). The fixtures with `.git/` directories (cargo-workspace, maven-multi-module-reactor, possibly others — find via `find mikebom-cli/tests/fixtures -name ".git" -type d`) WILL get one additive identifier slot per format. Non-git-tracked fixtures MUST stay byte-identical. Run `MIKEBOM_UPDATE_CDX_GOLDENS=1 / SPDX_GOLDENS=1 / SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test {cdx,spdx,spdx3}_regression`. Verify the diff is purely additive — no existing fields removed; only new `externalReferences[type:vcs]` / `creationInfo.creators` text line / `externalIdentifier[]` entries appear. If any golden has unexpected non-additive churn, halt and investigate.

- [X] T023 CHANGELOG.md `[Unreleased]` entry for milestone 073 — sections: **Added** (auto-detected `repo:` from git origin remote with 3-step fallback; `--with-source <scheme>:<value>` repeatable flag on scan + trace; auto-detected `image:` on `--image`; 4 built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) with per-format native carriers; user-defined-namespace passthrough via `mikebom:source-identifiers` annotation; published `source-identifiers.md` guide; parity catalog row C47). **Migration**: scans on git-tracked fixtures get one additive identifier emit (no existing fields removed); non-git scans byte-identical to alpha.15. **Out of scope**: identifier-keyed `--bind-to-source` resolution (milestone 074).

- [X] T024 Run `./scripts/pre-pr.sh` end-to-end and confirm clippy clean + every test target reports `ok. N passed; 0 failed`. Per the memory rule, show the full per-target output, NOT a failure-grep.

- [ ] T025 Open PR via `gh pr create` with title `feat(073): source identifiers — built-in + user-defined (auto-detect repo: from git origin; --with-source flag)`. Body cites SC-001..SC-007 measurement targets, the 3 spec clarifications baked into the design, the alpha.15 byte-identity golden delta (additive only on git-tracked fixtures), and the milestone-074 forward-looking handshake.

## Dependencies

```text
T001 (Setup)
   │
   ├── T002 (module skeleton)
   │      ↓
   ├── T003, T004 (Foundational types + validators — parallel)
   │      ↓
   └── T005 (parity catalog row + docs/sbom-format-mapping.md)
          ↓
          ├── T006 (US1 auto-detect logic)
          │      ↓
          ├── T007 (ScanArtifacts.source_identifiers field)
          │      ↓
          ├── T008,T009,T010 (US1 per-format emission — parallel)
          │      ↓
          │      T011 (US1 integration test)
          │
          ├── T012 (US2 --with-source flag)
          │      ↓
          │      T013 (US2 resolution pipeline)
          │      ↓
          │      T014 (US2 mikebom:source-identifiers annotation emission)
          │      ↓
          │      T015 (US2 integration test)
          │
          ├── T016 (US3 trace --with-source) and T017 (US3 image: auto-detect) — parallel
          │      ↓
          │      T018 (US3 per-tier integration test)
          │
          ├── T019 (US4 determinism + cross-format consistency test, depends on T008-T014 emit being final)
          │
          └── T020,T021,T023 (Polish docs — parallel)
                 ↓
                 T022 (golden regen, depends on all emission code being final)
                 ↓
                 T024 (pre-PR gate)
                 ↓
                 T025 (PR)
```

## Format validation

All 25 tasks follow the required checklist format. Setup (T001-T002), Foundational (T003-T005), US1 (T006-T011), US2 (T012-T015), US3 (T016-T018), US4 (T019), Polish (T020-T025). Every US-phase task carries the `[US#]` story label; `[P]`-marked tasks are genuinely parallelizable.

## MVP scope

**US1 alone is the MVP.** It delivers:
- Auto-detected `repo:` identifier in source-tier scans (the zero-config win — every git-tracked CI scan gets a stable identifier with no flag changes).
- All 3 per-format carriers (CDX `externalReferences[]`, SPDX 2.3 dual, SPDX 3 native).
- The cross-format-parity infrastructure registration (C47 row).

US2 adds the manual escape hatch + user-defined-namespace passthrough. US3 extends to image-tier and trace-tier. US4 verifies determinism. All four together = the milestone goal, but US1 alone is independently shippable if scope had to compress.

## Parallel execution opportunities

- **T003 + T004** (Foundational data types + validators) — independent files, can land together.
- **T008 + T009 + T010** (US1 per-format emission, three different format sub-trees) — fully parallel after T007 lands.
- **T016 + T017** (US3 trace flag + image-tier auto-detect) — independent surfaces.
- **T020 + T021 + T023** (Polish docs) — three independent files.
- **Per-US tests** (T011, T015, T018, T019) — independent test files; can be authored TDD-style alongside their respective US implementations.

## Independent test criteria (per user story)

- **US1**: T011 emission test produces SBOMs with auto-detected `repo:` identifier in all 3 formats; the 3-step fallback (origin → upstream → first-listed) is exercised across sub-tests.
- **US2**: T015 covers (a) manual flag emission, (b) user-defined-namespace passthrough, (c) override-wins, (d) dedup, (e) parse-time malformed rejection, (f) soft-fail-to-opaque on built-in-validator failure.
- **US3**: T018 confirms the same mechanism on path / image / trace tiers; auto-detected `image:` form matches the Q3 canonical shape.
- **US4**: T019's determinism + cross-format consistency check passes on byte-identical inputs.

## Closing context

Smaller and tighter than milestone 072 (~25 tasks vs 35 in 072). Foundation reuses milestone-071's parity catalog + canonicalization helper, and milestone-072's cross-tier-binding emit-site patterns. The forward-looking handshake (SC-005) sets up milestone 074's `--bind-to-source <identifier>` resolution path with no additional emission-side work needed at that point.
