# Feature Specification: A parity verdict per row, a test helper that can fail, and the closure parity gaps

**Feature Branch**: `1654-parity-verdict`
**Created**: 2026-11-24
**Status**: Draft
**Input**: #1695. Ten integration test files assert parity with a helper that cannot fail, and closure scans carry parity gaps nothing reports. Refs #1688.

## Background (measured)

**The verdict is not in the per-row report.**
- `waybill sbom parity-check --format json` reports, per row, each format's `kind` (`present` or `absent`) and `value_count`, plus `universal_parity`.
- `universal_parity` is the catalog's **expectation**: whether the row should appear in every format (`classification.is_universal_parity()`, `cli/parity_cmd.rs`). It is not the result.
- The result, under each row's directionality rule, is computed and then only counted: `summary.parity_gaps`.

**Ten test files read the expectation as the result.** Each treats a row as a gap only when `universal_parity == false`, so their parity assertions cannot fail:
- `binary_kind_technique`, `closure_resolved_tier`, `hpack_include_merge`, `library_identity`, `merged_binary_evidence`;
- `project_discovery_haskell`, `sbom_edit_retier`, `version_string_anchored`, `version_string_boundary`, `version_string_static`.

Spec 1653 found it: removing one row from SPDX 3 alone left the row `universal_parity: true` and failed no test (`specs/1653-boot-library-edges/measurements/after.md`, M5). `tests/boot_library_edges.rs` carries a stopgap: it compares presence and counts, and allows three known rows.

**Every corpus golden reports at least one parity gap.** These are `summary.parity_gaps` per golden at `main` `614e3c2`: 80 in total across 28 targets. The rows listed are those whose presence or count differ between the formats, written as CycloneDX / SPDX 2.3 / SPDX 3:

| Targets | `parity_gaps` | Rows that differ |
|---|---|---|
| 5 closure targets | 5 each | A7 (declared license, e.g. 26/31/31), **B1** (runtime edges, e.g. 596 / 13,182 / 13,182 on shellcheck), **B2** (dev edges, e.g. 12,587 / 1 / 1), D1 (identity evidence, presence-only, all present), **E1** (completeness claim, CycloneDX absent), G1 |
| go-opentelemetry, haskell-aeson, haskell-language-server, haskell-security-advisories, hpack-hs-opentelemetry ×2, hpack-ogmios ×2, maven-guice, python-flask | 3–4 | B1, B2, sometimes A7 |
| hpack-hs-temporal-sdk, hpack-lootbox ×2, image-postgres16 | 2–3 | A7, A8 (image), G1 |
| 9 others (go-cobra, npm-express, rust-ripgrep, 6 pants targets) | 1 each | G1 only |

**Which differing rows fail cannot be read from today's report.** That is FR-002.
- A row can differ in count and still pass. **D1** (presence-only) passes when every format has it. **A12 (CPE)** passes when CycloneDX's set is a subset of each SPDX set.
- But the totals show A12 failing on some targets. maven-guice reports 4 gaps, and differs only on A12, B1, B2 and G1.

**G1 differs in all 28 goldens** (tool name and version: SPDX 2.3 absent). A live scan of milestone 966's stub closure fixture shows no G1 gap. So G1 is probably an artefact of the goldens' masking, not of the emitters. That is inference; the plan confirms it.

**On a live scan of the stub closure fixture** (`tests/fixtures/nix_closure_boot_versions`, `--nix-closure`), `parity_gaps` is 3: B1 9/12/12, B2 4/1/1, E1 absent/1/1. Without the closure it is 0.

## Clarifications

### Session 2026-11-24

- Q: What is in scope once the shared helper can fail? → A: Fix every gap that the eleven test files' own fixtures show, plus the closure gaps and anything sharing their cause. Gaps seen only in corpus goldens are measured and filed. There is no per-test exclusion.
- Q (at T007): A7, the declared licence, fails on the five closure goldens through five licence-representation shapes it shares with 11 non-closure goldens (research R11). Fix it here? → A: No. It is its own spec. A7 is the one closure-golden row this spec leaves failing. It is filed as an issue carrying R11's measurement, and SC-003 is scoped accordingly. No test file's fixture fails A7, so the eleven files are unaffected.
- Q (at T022): After the second regeneration, nix-closure-slack-web keeps B1/B2 on 9 edges into `optional`-scoped Hackage components. The same `optional` mismatch fails B1/B2 on 6 non-closure goldens (python-flask, maven-guice, haskell-language-server, both hpack-hs-opentelemetry variants). The formats spell `optional` three ways. Fix it here? → A: No. It is filed with the non-closure B1/B2 issue, and slack-web's B1/B2 is SC-003's second exception.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer can read each row's parity verdict (Priority: P1)

Someone running `parity-check --format json` (a test, a CI step, a person) can see, for each row, whether it passed under its directionality, without recomputing the rule.

**Why this priority**: the absence of this field is how ten test files came to assert on the wrong one.

**Independent Test**: on a document set where one row is dropped from one format, that row's verdict is a failure, every other row is unchanged, and the summary count equals the number of failing rows.

**Acceptance Scenarios**:

1. **Given** any scan's three documents, **When** `parity-check --format json` runs, **Then** each row carries a verdict, and the number of failing universal rows equals `summary.parity_gaps`.
2. **Given** the same run, **When** compared with today's output, **Then** every existing field of every row, the summary and the human-readable table are unchanged; only the verdict is added.
3. **Given** a row that is not universal (format-restricted), **When** reported, **Then** its verdict says it was not checked, distinct from passing.

---

### User Story 2 - Test parity assertions can fail (Priority: P1)

Every integration test that claims parity uses one shared helper, which fails when the verdict says a row it exercises failed.

**Why this priority**: today the claim "checked with parity-check" is false in ten files.

**Acceptance Scenarios**:

1. **Given** each of the ten files and `tests/boot_library_edges.rs`, **When** their parity helper is replaced by the shared one, **Then** they pass. Each failure it exposes is traced to its cause, and the gap is fixed in the emitter or extractor, or the row is made format-restricted with a reason. No test excludes a row.
2. **Given** each of those eleven files, **When** a mutation drops one format's value of a row that file exercises, **Then** a test in that file fails.
3. **Given** the shared helper, **When** a scan has a gap that this spec has not resolved, **Then** the helper fails. No allowlist of known gaps remains, including the one in `tests/boot_library_edges.rs`.

---

### User Story 3 - Closure scans agree across formats (Priority: P1)

A `--nix-closure` document says the same thing in CycloneDX, SPDX 2.3 and SPDX 3, or the catalog records why a row cannot.

**Why this priority**: a closure document whose formats disagree about its own dependency edges is wrong in at least one format, and that is the output compared against other Nix SBOM tools.

**Acceptance Scenarios**:

1. **Given** each of B1, B2 and E1 on closure scans, and A7 where it differs on the closure goldens, **When** the three formats' values are diffed on the stub fixture and on the five closure goldens, **Then** the cause of each difference is named: an emitter that is wrong, an extractor that reads differently per format, or something a format cannot carry.
2. **Given** a wrong emitter or a mismatched extractor, **When** it is fixed, **Then** that row reports no gap on the stub fixture or the five closure goldens.
3. **Given** a value a format genuinely cannot carry, **When** recorded, **Then** the catalog marks the row format-restricted for that format, with the reason, and no test allowlists it.
4. **Given** G1, **When** traced, **Then** it is established whether the goldens' masking or an emitter causes it, and the measurement or the emitter is fixed accordingly.

---

### User Story 4 - Nothing else changes (Priority: P2)

**Acceptance Scenarios**:

1. **Given** any scan, **When** compared with `main`, **Then** documents are byte-identical, unless an FR-005 fix applies to them. Each such change is traced and listed.
2. **Given** existing consumers of `parity-check --format json`, **When** run, **Then** they read every field they read today, unchanged.

### Edge Cases

- **A row whose formats are all absent** is unexercised. Its verdict says so, and it is not a failure, as today's rule treats it.
- **CycloneDX-subset rows (A12)** pass when CycloneDX's set is a subset of each SPDX set, whatever the counts.
- **Presence-only rows (D1, E1)** pass when every format is present or every format is absent.
- **A gap on a non-closure target that shares a cause with a closure gap** (B1 and B2 also differ on Go, Haskell, Maven and Python targets) is fixed with it, and the change is traced.
- **A gap in a fixture one of the eleven test files scans** is in scope, whatever the target (Clarification 1).
- **A gap seen only in a corpus golden,** with a cause of its own (A8 on the postgres image, for example), is measured and filed, not fixed here.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (measure first)**: Before the design is fixed, the work MUST record:
  - for every corpus golden, per row, which rows fail under their directionality;
  - for B1, B2, E1, A7 and G1 on the stub closure fixture and the five closure goldens, the diff of the three formats' value sets, naming each cause;
  - whether G1 is an artefact of golden masking, by comparing a live scan with its golden.
- **FR-002 (verdict)**: `parity-check --format json` MUST give each row a verdict computed by the same rule that counts toward `summary.parity_gaps`, distinguishing passed, failed and not-checked. No existing field, summary value, or the human-readable table may change.
- **FR-003 (shared helper)**: The integration tests MUST share one parity helper that fails on any failing verdict. All eleven files named in User Story 2 adopt it, and no allowlist of known gaps remains.
- **FR-004 (each file can fail)**: For each of the eleven files, a mutation that drops one format's value of a row the file exercises MUST fail a test in that file.
- **FR-005 (gaps in scope)**: Every universal row MUST pass, or be recorded in the catalog as format-restricted for the format that cannot carry it, with the reason. This applies on:
  - the stub closure fixture and the five closure goldens;
  - every fixture the eleven test files scan.

  Fixes are to emitters or extractors, whichever FR-001 shows is wrong. Gaps seen only in other corpus goldens are measured, and filed as issues with the per-row measurement.

  **Exception, A7 (clarified at T007):** the closure goldens' declared-licence gap is not closure-specific (research R11). It is filed as its own issue and left failing here.
- **FR-006 (corpus before and after)**: The per-target, per-row gap list MUST be recorded for every corpus golden before and after, and every changed golden traced. Goldens are regenerated only by the CI corpus dispatch.
- **FR-007 (unchanged otherwise)**: Existing `parity-check` consumers and documents MUST be unchanged, except where FR-005 fixes a gap, and every such change is traced.
- **FR-008 (no new dependencies)**: Zero new Cargo dependencies.

### Key Entities

- **Row verdict**: per catalog row and scan: passed, failed, or not checked (not universal, or no format carries the row).
- **Format-restricted row**: a catalog row recorded as not carried by a named format, with the reason.
- **Shared parity helper**: the one test-side check every integration test uses.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (verdict consistency)**: On every corpus golden and every test fixture scanned in the suite, the number of rows whose verdict is a failure equals `summary.parity_gaps`.
- **SC-002 (helpers can fail)**: Eleven of eleven files adopt the shared helper. Eleven of eleven mutations, one per file, are killed.
- **SC-003 (parity where it is tested)**: The stub closure fixture and every fixture the eleven test files scan report 0 parity gaps. The five closure goldens report 0 apart from A7, and apart from slack-web's B1/B2 on `optional` edges (both FR-005 exceptions, clarified at T007 and T022; masking makes G1 a measurement artefact, R9). Any row that would otherwise fail is format-restricted in the catalog with a stated reason. Every remaining gap in other corpus goldens is in a filed issue.
- **SC-004 (the counts)**: The table of gaps per golden and per row is recorded before (80 in total today) and after, and every golden whose documents change is traced.
- **SC-005 (nothing else)**: Existing JSON fields and the table output are unchanged. Every document that changes does so because of an FR-005 fix, traced.

## Assumptions

- **Scope of the fixes** (Clarification 1): closure scans, every fixture the eleven test files scan, and anything sharing a cause with either. Gaps found only in other corpus goldens are measured and filed.
- **Rules and catalog:** the per-row rule is today's per-directionality rule, unchanged. A row moves to format-restricted only with a stated reason in the catalog.
- **Masking:** the goldens are masked documents, so a masking artefact found by FR-001 is fixed in how parity is measured on goldens, not in the emitters.
