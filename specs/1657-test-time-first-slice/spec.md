# Feature Specification: cut serial test time, first slice

**Feature Branch**: `1657-test-time-first-slice`
**Created**: 2026-11-25
**Status**: Draft
**Input**: #1719, first slice: #1711, #1712 and #1715, before any runner change (#1718 is a separate trial and needs #1711 first). Refs #1719, #1711, #1712, #1715.

## Background (measured)

The test suite takes about 25 min on the macOS CI lane, about 15 min on Linux, and about 30 min locally (`pre-pr.sh`). Most of that is test binaries running one after another (#1719). This slice removes three costs:

1. **#1711: `waybill sbom edit` is quadratic on SPDX 3.**
   - **Cause:** `Spdx3::components()` (`waybill-cli/src/edit/spdx3.rs`) calls `annotations_about()` per component and field, at 8 call sites. Each call walks the whole `@graph` and re-parses every Annotation's statement.
   - **Cost on the nix-closure-moat golden (debug build):** an edit takes 79.4 s on SPDX 3 and 4.21 s on SPDX 2.3.
   - **Cost to the suite:** `sbom_edit_filter::i1454_an_edit_that_drops_components_drops_the_openvex_reference` takes 150.8 s alone. Its binary is the slowest on every CI lane: 143 s on Linux and 192 s on macOS (run 45613291165).
2. **#1712: duplicated harness tests.**
   - 17 integration-test files include `tests/corpus_harness_195/mod.rs` through `#[path]` (`measurements/harness-includers.txt`). Its 145 `#[test]` functions therefore run in 17 binaries.
   - `holistic_parity.rs:16` declares `mod dual_format_perf`, so those 2 tests run twice.
3. **#1715: scan tests call deps.dev live.**
   - A split scan takes 7.06, 7.16 and 6.86 s online, and 0.06 s with `--offline` (empty HOME).
   - 38 test files run a scan with no `--offline` anywhere in them (`measurements/scan-without-offline.txt`). That list is a grep heuristic, not a reviewed list.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - SPDX 3 edits take linear time (Priority: P1)

An operator edits a large SPDX 3 closure document with `waybill sbom edit`. The edit takes about as long as the same edit of the SPDX 2.3 document, not 20 times longer.

**Why this priority**: it is a user-facing product defect. It is also the single slowest test, and it sets the floor for any parallel runner (#1718).

**Independent Test**: edit the moat SPDX 3 golden with the test's recipe. The output is byte-identical to today's, and the time falls within 2× of the SPDX 2.3 edit.

**Acceptance Scenarios**:

1. **Given** every input the test suite edits, and the five closure goldens under the i1454 recipe, **When** they are edited before and after the change, **Then** the outputs are byte-identical.
2. **Given** the moat goldens, **When** each is edited three times, **Then** the SPDX 3 median is no more than 2× the SPDX 2.3 median. The min, median and max are recorded, in both debug and release builds.

### User Story 2 - each test runs once (Priority: P2)

**Independent Test**: list every test binary. The `corpus_harness_195::` tests appear in exactly one binary, and the `dual_format_perf::` tests in exactly one, with totals of 145 and 2.

**Acceptance Scenarios**:

1. **Given** the 17 consumers, **When** each binary's tests are listed, **Then** none contains `corpus_harness_195::` tests except the one dedicated binary.
2. **Given** `holistic_parity`, **When** its tests are listed, **Then** it contains no `dual_format_perf::` test, and `dual_format_perf`'s own binary contains both.

### User Story 3 - scan tests stay off the network (Priority: P3)

**Independent Test**: every scan test that is not about enrichment runs with `--offline` or against a mock. Each of the 38 candidates has a recorded disposition.

**Acceptance Scenarios**:

1. **Given** a candidate file, **When** it is reviewed, **Then** `measurements/offline-review.md` records it as one of:
   - offline;
   - mocked;
   - deliberately online, with the reason;
   - not a scan, for example a help-text test.
2. **Given** the shared scan helpers (`run_split_scan`, the nix-stub helper, and any others like them), **When** they run a scan, **Then** they pass `--offline` unless the test is about enrichment.
3. **Given** the enforcement decision (FR-007), **When** it is adopted, **Then** a positive control proves it fires on a scan that reaches the live service.

### Edge Cases

- A test that depends on enrichment output but is not about enrichment: switching it to offline changes its input. It needs a mock or a fixture-cached answer, plus a mutation showing it still fails on the defect it guards.
- A harness item the 17 consumers use that is currently private to a test module: it moves to the shared non-test part, with no behaviour change.
- An SPDX 3 document with annotations about an absent subject, or with malformed statements: the index preserves today's handling of both exactly. Byte-identity of the output is the check.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `sbom edit` on SPDX 3 MUST read the annotations about all subjects in one pass over `@graph`, shared with the document-level annotation read.
- **FR-002**: Edited output MUST be byte-identical before and after, for every input the test suite edits and for the five closure goldens under the i1454 recipe.
- **FR-003**: The moat SPDX 3 edit's median MUST be within 2× the SPDX 2.3 median (3 runs each). The release-build figures are recorded beside it.
- **FR-004**: The corpus-harness self-tests MUST compile into exactly one test binary. The 17 consumers include only its non-test code.
- **FR-005**: `dual_format_perf`'s tests MUST run only in their own binary. The helpers `holistic_parity` needs move to `tests/common`.
- **FR-006**: Shared scan helpers MUST pass `--offline` unless the test is about enrichment. An enrichment test MUST use a mock server rather than the live service.
- **FR-007**: The plan MUST decide, after measuring, whether to add an enforcement point that makes a scan test fail when it reaches the live service. If it is adopted, a positive control proves it fires.
- **FR-008**: Every one of the 38 candidates MUST have a recorded disposition, reviewed by hand.
- **FR-009**: No test may lose coverage. A test whose input changes needs a mutation showing it still fails on the defect it guards. Total test counts change only by the duplicates removed.
- **FR-010**: Each affected binary's wall time is measured before and after (min, median and max over 3 runs, idle machine). The summed serial run time of the full suite, from one `cargo test --workspace` log each side, is reported with no pre-committed target.
- **FR-011**: No corpus golden changes. Zero new Cargo dependencies. One gate, one PR.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The i1454 test's binary takes a small fraction of its 150.8 s today. It is reported against the before figure, measured under the same conditions.
- **SC-002**: `corpus_harness_195::` tests run once per suite run instead of 17 times, and `dual_format_perf::` tests once instead of twice.
- **SC-003**: No reviewed scan test reaches deps.dev live unless its recorded reason says it must.
- **SC-004**: The before and after per-binary and summed suite times are recorded in `measurements/after.md`.

## Assumptions

- The SPDX 2.3 edit path is not quadratic. It is the baseline the 2× ratio is measured against.
- Removing duplicate test executions drops no coverage, because each removed copy is identical to the one kept.

## Out of Scope

These are filed already:
- #1716: self-scans of the debug binary, beyond adding `--offline`;
- #1713: redundant integration-test candidates;
- #1714: consolidating the integration-test binaries;
- #1718: the cargo-nextest trial.
