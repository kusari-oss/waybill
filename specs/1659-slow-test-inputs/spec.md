# Feature Specification: Cheaper inputs for the two costliest tests (#1716, #1725)

**Feature Branch**: `1659-slow-test-inputs`
**Created**: 2026-11-25
**Status**: Draft
**Input**: #1719, third slice: #1716 and #1725. Refs #1719, #1716, #1725.

## Context

Since spec 1658, tests run in parallel under nextest. On Linux CI the test phase is throughput-bound: 1,475 s of summed test time over about 370 s of wall time on 4 cores (run 45728714707). A test's cost divided by about 4 is what it adds to Linux wall time.

The two costliest inputs:

- **#1725:** `sbom_edit_filter::i1454_an_edit_that_drops_components_drops_the_openvex_reference` takes 115 s on Linux CI and 34.7 s locally. Locally, 24.4 s of that is `spdx3-validate` run on the 8.3 MB edited moat SPDX 3 document. The edits take about 4 s each, the schema check 0.5 s.
- **#1716:** four tests each copy the debug `waybill` binary (125 MB on macOS) into a temporary directory and scan it. They take about 20 s each on Linux CI, and 4.8–5.0 s each alone locally.

### What the code shows

These findings are from reading the code, and they settle the questions the description asked.

**#1716.** All four tests make the same scan: a directory holding only the debug `waybill` binary, `sbom scan --path <dir> --output <file> --no-deep-hash`, CycloneDX.
- Three are negative controls whose claim is about waybill's own bytes:
  - `cpe_binary_id::mikebom_self_scan_emits_no_spurious_openssl_cpe`: no `openssl:openssl` CPE.
  - `binary_id_enrich::mikebom_itself_does_not_emit_spurious_version_strings`: no `pkg:generic/{openssl,zlib,curl,libcurl,sqlite,libxml2}@…`.
  - `binary_id_enrich::mikebom_itself_does_not_emit_spurious_symbol_fingerprints`: no version-less `pkg:generic/…` symbol-fingerprint match. Its comment adds a second purpose: catching waybill starting to link one of those libraries.
- **These three need this specific binary.** A different binary would test a different claim.
- The fourth, `binary_build_provenance::mikebom_self_scan_emits_build_provenance_properties`, is a positive check: a host-format binary yields build-provenance properties (ELF compiler stamps, Mach-O build version, PE fields). It needs some host-built binary. waybill's is the one at hand.
- **All four read the same scan output**, so one scan serves all four.
- **In-process sharing won't work.** nextest runs each test in its own process, so a lazily shared value can't carry one scan across four tests.

**#1725.**
- The nix-closure goldens are the only corpus SPDX 3 documents that carry an OpenVEX sidecar reference: moat, openai, shellcheck, slack-web and slacklinker, each 6.9 to 15.3 MB. Every other golden is 2.1 MB or less.
- Four other tests run `spdx3-validate` on edited documents:
  - `sbom_edit_filter.rs:47` and `:294` drop development and test scope from small fixtures and from a golden;
  - `sbom_edit_redact.rs:79` redacts;
  - `closure_runtime_scope.rs:2337` (`us4_drop_scope_excluded_yields_valid_runtime_document`) drops `scope=excluded` from a stub closure document.
- **What only i1454's validator run checks:** that an SPDX 3 document is still conformant after the edit both drops components and removes the OpenVEX sidecar reference. No other validated input is known to carry that reference.
  - `us4` runs the same recipe on a small closure document. Its doc comment says SPDX's OpenVEX sidecar "is not edited", which predates #1454's reference removal. Whether its input carries a reference has not been checked; the plan's research must settle it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The OpenVEX-reference edit is validated on a small document (Priority: P1)

A contributor's test run no longer spends about 24 s locally, and about 100 s of Linux CPU, validating an 8 MB document. The conformance of the reference-removal edit is still checked.

**Why this priority**: it's the slowest test on Linux CI and the single largest saving.

**Independent Test**: i1454's moat edits still run, along with their JSON schema check. The conformance check after the reference is removed runs on a small document that carries an OpenVEX reference and goes through the same edit. A mutation that makes reference removal leave the document non-conformant fails the new check.

**Acceptance Scenarios**:

1. **Given** a small SPDX 3 document with an OpenVEX sidecar reference and components marked `scope=excluded`, **When** it's edited with `--drop scope=excluded`, **Then** the reference is gone, the edit report says so, and `spdx3-validate` accepts the output.
2. **Given** that input, **When** the test runs, **Then** a control confirms beforehand that the input carries the reference and that the edit drops at least one component, so the check can fail.
3. **Given** i1454 after the change, **When** it runs, **Then** it still checks the moat edits, the reference removal and the JSON schema for both formats. It no longer runs `spdx3-validate` on the moat output.

---

### User Story 2 - waybill's binary is scanned once for its four guards (Priority: P1)

The four self-scan guards run on one scan of the debug binary, not four.

**Why this priority**: about 60 s of Linux CPU saved, with each guard's claim and input unchanged.

**Independent Test**: one test scans the binary once and evaluates all four guards. It reports every guard that fails, by name, not just the first.

**Acceptance Scenarios**:

1. **Given** the debug `waybill` binary, **When** the combined test runs, **Then** it makes exactly one scan and checks all four claims against that output.
2. **Given** a mutation that breaks one guard (for example, a CPE table entry that matches waybill), **When** the test runs, **Then** it fails and names that guard.
3. **Given** two guards broken at once, **When** the test runs, **Then** the failure names both.
4. **Given** the provenance guard's skip case (no file-level binary component), **When** it applies, **Then** the skip is reported for that guard only, and the other three are still checked.

---

### Edge Cases

- **If no small document carrying an OpenVEX reference exists**, research must say so. The P1 input is then built for the test from the stub closure path that `us4` uses. It must not be a hand-edited copy of a golden.
- **Running the combined test on its own** (`-E 'test(=…)'`) gives the same result as running it in the suite.
- **The four original test names disappear.** CI history and issue references to them are noted in the PR.
- **The provenance guard branches on host format** (ELF, Mach-O, PE). The combined test keeps every branch.

## Requirements *(mandatory)*

### Functional Requirements

**#1725**

- **FR-001**: i1454 MUST keep its moat edits, its controls, its OpenVEX-reference assertions and its JSON schema checks, for both formats.
- **FR-002**: The `spdx3-validate` check after a reference-removing edit MUST run on a document carrying an OpenVEX sidecar reference whose `--drop scope=excluded` edit removes it. Its size MUST be at most 2.1 MB, the size of the largest corpus golden without a sidecar reference.
  - It may be an existing test's input, if research shows that input carries the reference.
  - Otherwise it is a small input generated in the test, not committed.
- **FR-003**: A positive control MUST confirm, before the edit, that the input carries the reference and holds droppable components.

**#1716**

- **FR-004**: The three negative guards and the provenance guard MUST be evaluated against one scan of the debug `waybill` binary, using the same scan arguments as today.
- **FR-005**: The combined test MUST evaluate every guard before failing, and name each failing guard with the message its original test gave.
- **FR-006**: No on-disk cache. A single test is the sharing mechanism, so there is no invalidation rule to get wrong.

**Coverage and measurement**

- **FR-007**: Every changed check MUST be shown to fail under a mutation that reintroduces the defect it guards. The mutation is applied with an exact replacement, reverted with `cp`, and checked with `cmp`.
- **FR-008**: Each affected test MUST be timed before and after, min/median/max over 3 runs, locally and idle. The Linux CI summed test time and `Tests` step MUST be reported over 2 runs before and 2 after.
- **FR-009**: No corpus golden changes, and zero new Cargo dependencies.

### Key Entities

- **Guard**: a named claim about a scan output (CPE, version strings, symbol fingerprints, build provenance), with its pass/fail result and message.
- **Reference-carrying input**: an SPDX 3 document with an OpenVEX sidecar reference and components `--drop scope=excluded` removes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The time locally spent in i1454 plus the new reference-removal conformance check falls from 34.7 s to at most 12 s (median of 3). The edits and schema checks alone measured about 9.5 s.
- **SC-002**: Four scans of the debug binary become one. The summed local time of the four guards falls from about 4 × 5 s to about 5 s (median of 3).
- **SC-003**: Linux CI summed test time and the `Tests` step are reported over 2 runs before and 2 after, with no pre-committed target.
- **SC-004**: Every changed check fails under its mutation, and its positive control shows the input exercises the path.

## Assumptions

- `spdx3-validate`'s cost grows with document size. This is checked during implementation by timing the validator on the small input.
- **Out of scope:** #1713, #1714, and CI compile and cache time, which is the next slice and needs measuring first.
- One gate, one PR.
