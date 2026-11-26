# Tasks: Cheaper inputs for the two costliest tests (#1716, #1725)

**Input**: `specs/1659-slow-test-inputs/` (spec, plan, research, data-model, quickstart).

**Tests**: this feature changes tests. The spec requires each changed check to be proven with a mutation (FR-007).

**Rules (CLAUDE.md):**
- Timings are min/median/max over 3 runs on an idle machine.
- Revert mutations with `cp` and check with `cmp`.
- Never pipe a check through `head`.
- Don't run `cargo fmt`.

## Phase 1: Setup

- [x] T001 Create `specs/1659-slow-test-inputs/measurements/` with a `runs.tsv` header (test, phase, run, seconds), and do a warm build with `cargo nextest run --workspace --no-run`.

## Phase 2: Foundational

- [x] T002 Baseline. Run each affected test alone 3 times with `cargo nextest run -p waybill --color never -E 'test(=NAME)'`, and record nextest's per-test seconds in `runs.tsv` as phase `before`. The tests:
  - `sbom_edit_filter::i1454_an_edit_that_drops_components_drops_the_openvex_reference`
  - `closure_runtime_scope::us4_drop_scope_excluded_yields_valid_runtime_document`
  - `cpe_binary_id::mikebom_self_scan_emits_no_spurious_openssl_cpe`
  - `binary_id_enrich::mikebom_itself_does_not_emit_spurious_version_strings`
  - `binary_id_enrich::mikebom_itself_does_not_emit_spurious_symbol_fingerprints`
  - `binary_build_provenance::mikebom_self_scan_emits_build_provenance_properties`

## Phase 3: User Story 1, OpenVEX-reference conformance on a small document (P1)

**Goal**: i1454 no longer validates the 8.3 MB moat output. `us4` carries that check, with explicit reference controls.

**Independent test**: `us4` passes. The R2 mutation makes it fail. i1454 passes with no `spdx3-validate` call.

- [x] T003 [US1] In `waybill-cli/tests/closure_runtime_scope.rs`, inside `us4_drop_scope_excluded_yields_valid_runtime_document`'s loop, add these for the `spdx23` and `spdx3` labels:
  - **Control:** before the edit, the input carries exactly one OpenVEX sidecar reference. Use a format-aware helper that reads `externalDocumentRefs[].spdxDocument` ending in `.openvex.json` (SPDX 2.3), or the `SpdxDocument`'s `externalRef[]` with `externalRefType == "vulnerabilityExploitabilityAssessment"` (SPDX 3). Move `openvex_refs` from `sbom_edit_filter.rs:502` into `waybill-cli/tests/sbom_edit_support/mod.rs` as `pub fn`, and use it from both files. Both already include that module; don't copy it.
  - **After the edit:** the output carries none, and `e.stderr` contains `OpenVEX sidecar reference removed`.
  - **Doc comment:** add one sentence, that the edit removes the SPDX document's *reference* to the sidecar (#1454), while the sidecar file itself is not edited.
- [x] T004 [US1] Mutation R2. In `waybill-cli/src/edit/spdx3.rs` `remove_openvex_references`, delete the `if refs.is_empty() { … obj.remove("externalRef") … }` block with an exact replacement, then run `us4` and record which check fails (schema or `spdx3-validate`) in `measurements/mutations.md`.
  - **If it passes**, the empty array is conformant. Try leaving a locator-less `ExternalRef` instead (retain the entry but strip its `locator`), and record that.
  - **Revert** with `cp`, check with `cmp`, and rebuild.
  - **Record which check fired.** If it was the JSON schema check, that shows the schema check catches it, not `spdx3-validate`. So also run a mutation that passes the JSON schema but should fail SHACL: for example, leave the OpenVEX `ExternalRef` in place but change its `externalRefType` to a value outside the SPDX 3 vocabulary. Confirm the schema check passes and `spdx3-validate` fails. If no such mutation can be found, record that the moat validator run guarded nothing the schema check didn't already catch.
  - **Also record** a test-side mutation: delete T003's "carries the reference" assertion, inject an input without the reference, and confirm the control fails.
- [x] T005 [US1] In `waybill-cli/tests/sbom_edit_filter.rs` i1454, remove the `if file == "spdx-3.json" { spdx3_validate_or_skip(&e.path); }` block. Leave a one-line comment pointing to `us4` as the conformance check for a reference-removing edit. If `spdx3_validate_or_skip`'s import becomes unused in this file, check it: lines 47 and 294 still use it, so it should stay used.

**Checkpoint**: US1 is done when `us4` and i1454 pass and T004's mutation is recorded as failing.

## Phase 4: User Story 2, one scan for four self-scan guards (P1)

**Goal**: the debug binary is scanned once, and four guards are evaluated with every failure reported.

**Independent test**: `self_scan_guards` passes, and each R4 mutation fails it with the guard's name.

- [x] T006 [P] [US2] Create `waybill-cli/tests/self_scan_guards.rs` with one test, `waybill_self_scan_guards`.
  - **The scan:** copy `env!("CARGO_BIN_EXE_waybill")` into a tempdir as `waybill-under-test`, then run `sbom scan --path <dir> --output <dir>/out.cdx.json --no-deep-hash`, the same arguments as all four originals. Parse the output once.
  - **The guards:** four functions taking `&Value` and returning `Result<Option<String>, String>`, where `Ok(Some(note))` is a skip. Each carries over its original test's logic and failure message exactly:
    - `guard_no_openssl_cpe`, from `cpe_binary_id.rs`;
    - `guard_no_spurious_version_strings` and `guard_no_spurious_symbol_fingerprints`, from `binary_id_enrich.rs`;
    - `guard_build_provenance`, from `binary_build_provenance.rs`, including its ELF, Mach-O and PE branches and its skip.
  - **Reporting:** collect `(name, result)`, print any skips with `eprintln!`, then one `assert!` over all failures that names each.
  - **Helpers:** copy `property_value` and `find_file_level` from the originals. Add the `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention only if it's needed; integration test crates follow the existing files' style.
- [x] T007 [US2] Mutations R4, recorded in `measurements/mutations.md`. In the test, temporarily insert defects into the parsed SBOM before evaluation, and run once per mutation:
  - **One per guard:** a CPE `cpe:2.3:a:openssl:openssl:…` on any component; a component with PURL `pkg:generic/zlib@1.2.13`; a component with PURL `pkg:generic/openssl`; the format-specific provenance property removed from the file-level component. Each must fail and name its guard.
  - **Two together:** must give one failure naming both.
  - **Product side:** first read how a CPE reaches a component in a binary scan, and record the path in `mutations.md`. Then pick the mutation on that path that makes the real scan of waybill emit `openssl:openssl`: for example, loosening the `openssl` version-string anchor so it matches a string in the debug binary, found with `strings`. The test must fail. If no single change in waybill can produce it, record why, and rely on the injected-defect mutations for this guard.
  - **Revert** everything with `cp`, check with `cmp`, and rebuild.
- [x] T008 [US2] Delete the four original tests:
  - `cpe_binary_id.rs`: `mikebom_self_scan_emits_no_spurious_openssl_cpe`;
  - `binary_id_enrich.rs`: the two `mikebom_itself_*` tests;
  - `binary_build_provenance.rs`: `mikebom_self_scan_emits_build_provenance_properties`.

  Then remove helpers and imports left unused in those files. Clippy (`-D warnings`) will name them. A file left with no tests is deleted, and a module comment is moved into `self_scan_guards.rs` if it explains a guard. Check by name in `cargo nextest list` that each removed test is gone and `self_scan_guards::waybill_self_scan_guards` is present.

**Checkpoint**: US2 is done when `self_scan_guards` passes, every T007 mutation fails as recorded, and the originals are gone.

## Phase 5: Measure and ship

- [x] T009 After timings. Run `self_scan_guards::waybill_self_scan_guards`, i1454 and `us4` alone 3 times each, and record them as phase `after`. Compare with SC-001 (i1454 plus `us4` at 12 s or less, median) and SC-002 (one scan: the combined test against the sum of the four originals).
- [ ] T010 Run the gate: `./scripts/gate.sh` with both `--expect` patterns from CLAUDE.md, around `prepr -- ./scripts/pre-pr.sh`. Check `GATE_RESULT=pass`, the summary line, and the test count. Expected: before − 4 + 1, with no other change; explain any other difference.
- [ ] T011 Write `measurements/after.md`, commit, and open the PR (Refs #1719, #1716, #1725).
- [ ] T012 CI: collect the `Tests` step and nextest summed time for Linux, from 2 PR runs against 2 recent `main` runs. Use `specs/1658-nextest-trial/measurements/ci_steps.py` and the per-test sum method from spec 1658. Add them to `after.md` and the PR. Enumerate every job's conclusion.

## Dependencies

- **Order:** T001 → T002 → US1 (T003 → T004 → T005) and US2 (T006 → T007 → T008) → T009 → T010 → T011 → T012.
- **US1 and US2 are independent.** T006 can be written while US1 runs.
- **Within US2,** T008 comes after T007, so the originals remain as a reference until the mutations are recorded.

## Implementation strategy

US1 first: it's the single largest saving, and the smallest change. Then US2. One gate, one PR.
