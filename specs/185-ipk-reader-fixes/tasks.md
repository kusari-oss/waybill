# Tasks: ipk reader bug fixes (m185)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md) · **Data model**: [data-model.md](./data-model.md)

**Delivery slice** (per plan.md): both USs ship in one PR. Per-USR independence (different files, no shared code between the two fixes) — either US could ship alone. Estimated ~22 tasks across 5 phases.

**Zero new production Cargo dependencies** — reuses stdlib `rsplitn` (US1) and rpm_file.rs's existing normalization helpers via `pub(crate)` visibility bumps (US2). C122 docstring at `parity/extractors/cdx.rs:867` — NOT touched (m185 has zero impact on the C122 optional-derivation value-set).

## Phase 1: Setup

- [X] T001 Verify current branch is `185-ipk-reader-fixes` and working tree is clean at `/Users/mlieberman/Projects/mikebom`; confirm base is main HEAD post-m184 merge (commit `5f1b29d` / `impl(184): Maven + Gradle optional-dependency classification (#540)`)
- [X] T002 Verify pre-existing helpers m185 will reuse: `grep -n 'fn normalize_bitbake_license_operators\|fn preserve_known_operands_with_license_ref\|fn sanitize_to_license_ref_idstring' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/rpm_file.rs` — expect 3 private `fn` matches (to be promoted to `pub(crate)` in T006). Also verify `stanza.license()` accessor at `control_file.rs:71` via `grep -n 'fn license' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/control_file.rs` — expected one match

## Phase 2: User Story 1 — ipk filename fallback multi-underscore fix (P1)

**Goal**: `parse_ipk_filename` at `ipk_file.rs:609` correctly handles ipk filenames whose version field contains embedded underscores (BitBake `SRCPV` shape). Zero behavior change on canonical 2-underscore filenames.

**Independent Test**: Create a synthetic `.ipk` file with a multi-underscore version filename (e.g., `test-pkg_1.0+git0+abc_def-r0_all.ipk`), scan the containing directory, verify the emitted component has valid `name`/`version`/`arch`/`purl` per contracts/parser-decision-matrix.md row 3.

### 2a. Parser semantic change

- [X] T003 [US1] Rewrite `parse_ipk_filename` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/ipk_file.rs:609` per data-model.md §2.3. Full transformation:
    1. Replace `stem.split('_').collect::<Vec<&str>>()` + strict `parts.len() != 3` guard with a `stem.rsplitn(3, '_')` iterator.
    2. Extract via three `.next()?` calls in order: `arch` (rightmost segment), `version` (middle), `name` (all remaining leading text).
    3. Preserve the existing empty-field guard: `if name.is_empty() || version.is_empty() || arch.is_empty() { return None; }`.
    4. Convert `&str` slices to `String` at return time (`Some((name.to_string(), version.to_string(), arch.to_string()))`).

  Also update the docstring comment above the function to note the m185 change and reference issue #538. Keep the "we split only on `_`, never `-`" note.

### 2b. Unit tests

- [X] T004 [US1] Add 6 unit tests to `ipk_file.rs::tests` (colocate with existing `parse_ipk_filename` tests from m169) covering the contracts/parser-decision-matrix.md rows: `parse_ipk_filename_canonical_2underscore_still_parses` (row 1 — regression pin), `parse_ipk_filename_multi_underscore_version_now_parses` (row 3 — the fix), `parse_ipk_filename_yocto_kernel_module_shape` (row 4 — real BitBake shape), `parse_ipk_filename_no_ipk_suffix_still_none` (row 7 — extension missing), `parse_ipk_filename_no_underscores_still_none` (row 5 — 0-underscore stem returns None), `parse_ipk_filename_empty_field_still_none` (rows 8-10 — empty-guard). Each test's input string mirrors data-model.md §2.4 exactly
- [X] T005 [US1] Verify no regression on the existing m169 US1-US6 test suite: `cargo +stable test -p mikebom --bin mikebom -- scan_fs::package_db::ipk_file` — expect all pre-m185 ipk_file.rs tests (m169 archive-format parser, filename-fallback annotations, PURL-with-distro-tag, etc.) to continue passing byte-identically. Any breakage indicates the rsplitn refactor has an unintended side effect on the canonical-input path — investigate before advancing

## Phase 3: User Story 2 — opkg License extraction (P1)

**Goal**: `opkg::build_entry` at `opkg.rs:203` extracts `stanza.license()` and normalizes it through a 4-pass pipeline (3 reused rpm helpers + 1 opkg-only wholesale-wrap fallback per m185 FR-014). Every opkg-installed component whose stanza carries a valid or partially-parseable License field emits populated `licenses[]` instead of the pre-m185 `Vec::new()`.

**Independent Test**: Scan a synthetic opkg-status fixture with stanzas exercising all four passes: (a) canonical SPDX passes at Pass 2, (b) BitBake `&` operator passes at Pass 3 via LicenseRef wrap, (c) wholly unparseable stanza triggers Pass 4 wholesale-wrap, (d) absent License field produces `licenses: []` regression-pin. Verify CDX 1.6 output shape matches contracts/license-pipeline.md §Emitted format shape.

### 3a. rpm_file.rs helper visibility bumps

- [X] T006 [US2] Promote 3 helper visibilities in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/rpm_file.rs` per data-model.md §3.4:
    - Line 615: `fn normalize_bitbake_license_operators` → `pub(crate) fn`
    - Line 770: `fn sanitize_to_license_ref_idstring` → `pub(crate) fn`
    - Line 832: `fn preserve_known_operands_with_license_ref` → `pub(crate) fn`

  Add a docstring note above each promoted function: "Milestone 185 US2 (#539): promoted from private to `pub(crate)` so `opkg.rs::build_entry` can reuse this helper for its own License-field normalization pipeline. Zero behavior change for rpm.rs's existing call site — this is a visibility-only change."

  **Critical FR-011 pin**: verify via `cargo +stable check -p mikebom` that rpm_file.rs compiles clean AND that its call site at line 469-488 continues to reference the unqualified names (no shorthand `use super::...::` needed since they're in the same module). Zero behavior change on rpm

### 3b. opkg.rs pipeline wiring

- [X] T007 [US2] Modify `build_entry` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/opkg.rs:203` per data-model.md §3.2 code block. Full transformation:
    1. Import `mikebom_common::types::license::SpdxExpression` (verify current import list; if not present, add near the existing `use` block).
    2. Insert the 4-pass pipeline BEFORE the `Some(PackageDbEntry { ... })` construction. The pipeline is:
       - Pass 0: `stanza.license().filter(|l| !l.trim().is_empty())` — treat absent + whitespace-only as no License.
       - Pass 1: `super::rpm_file::normalize_bitbake_license_operators(raw)` — BitBake `&`/`|` normalization.
       - Pass 2: `SpdxExpression::try_canonical(&normalized).ok()` — strict SPDX parse. On success, `return Some(expr)`.
       - Pass 3: `super::rpm_file::preserve_known_operands_with_license_ref(&normalized).and_then(|wrapped| SpdxExpression::try_canonical(&wrapped).ok())` — per-operand LicenseRef wrap. On success, `return Some(expr)`.
       - Pass 4 (m185 US2 wholesale-wrap): `super::rpm_file::sanitize_to_license_ref_idstring(raw)?`, format as `LicenseRef-{sanitized}`, `SpdxExpression::try_canonical(&wrapped).ok()`. Emit `tracing::warn!` with `source_path`, `package = name`, `raw_license = raw`, `wrapped` for operator visibility.
    3. Collect the resulting `Option<SpdxExpression>` into `Vec<SpdxExpression>` (via `.into_iter().collect()`).
    4. Replace the `licenses: Vec::new(),` at line 289 with `licenses,` (consuming the local variable).

  Verify via `cargo +stable check -p mikebom` clean compile

### 3c. Unit tests

- [X] T008 [US2] Add 6 unit tests to `opkg.rs::tests` covering the contracts/license-pipeline.md pipeline: `build_entry_extracts_canonical_spdx_license` (Pass 2 success — `"GPL-2.0-only"` → `vec![SpdxExpression("GPL-2.0-only")]`), `build_entry_bitbake_operator_normalizes_and_wraps_unknown_operand` (Pass 3 success — `"GPLv2 & bzip2-1.0.4"` → `vec![SpdxExpression("GPL-2.0-only AND LicenseRef-bzip2-1.0.4")]`), `build_entry_absent_license_stays_empty` (regression pin — no License field → `licenses: Vec::new()`), `build_entry_whitespace_only_license_treated_as_absent` (whitespace-only edge → `Vec::new()`), `build_entry_unparseable_license_wholesale_wraps` (Pass 4 fires — `"!!! bad syntax &&& random"` → single `LicenseRef-` entry), `build_entry_unsanitizable_license_falls_through_to_empty` (defensive — a purely-symbol string like `"!!!"` sanitizes to empty → `Vec::new()`).

  Each test constructs a `ControlStanza` via the existing helper (grep for `ControlStanza::` construction in `opkg.rs::tests` for pattern) OR directly via inline `parse_stanzas(text)` on a `&str` fixture. Assert on the returned `PackageDbEntry.licenses` shape

## Phase 4: Polish & Cross-Cutting Concerns

### 4a. Golden regeneration (SC-005, SC-006, FR-011)

- [X] T009 Regenerate CDX 1.6 goldens: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test --workspace`. Expected drift: (a) additive changes on any Yocto/opkg fixture that exercises License extraction (populated `licenses[]` entries), (b) potentially additive changes on any ipk fixture that happens to include multi-underscore filenames (fixed name/version/PURL); (c) ZERO drift on rpm.cdx.json per research Decision 3 rpm-invariant; (d) ZERO drift on every non-Yocto golden per SC-005. Verify (c)+(d) via `git diff --stat mikebom-cli/tests/fixtures/golden/cyclonedx/` post-regen — rpm files + non-Yocto files MUST show `0 changed`
- [X] T010 Regenerate SPDX 2.3 goldens: `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test --workspace`. Expected drift: (a) additive `licenseDeclared` + `hasExtractedLicensingInfos` entries on any opkg fixture, (b) ZERO drift on rpm.spdx.json per FR-011. Verify via `git diff`
- [X] T011 Regenerate SPDX 3.0.1 goldens: `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --workspace`. Expected drift: additive `simplelicensing_CustomLicense` entries on opkg fixtures with LicenseRef-wrapped operands; ZERO drift on rpm.spdx3.json per FR-011

### 4b. Documentation

- [X] T012 Update `/Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md` — locate the section documenting opkg / ipk emission (grep for `opkg` / `ipk` / `pkg:opkg`) and add a brief post-m185 note: (a) the ipk filename fallback now handles multi-underscore versions per BitBake `SRCPV`, (b) opkg installed-package components carry `licenses[]` normalized through the same pipeline as rpm — pointing to `LicenseRef-<sanitized>` fallbacks for unknown operands + wholesale-wrap fallback for unparseable strings. Cross-reference #538 and #539 as the driving issue reports

### 4c. Verification gates

- [X] T013 Run walker-audit allow-list check locally per project memory `feedback_walker_audit_local_check` (bash block identical to m184 T019 body) — m185 introduces ZERO new walker functions (all changes are string-parser + license-extraction, no filesystem walking); expected exit 0
- [X] T014 Run the mandatory pre-PR gate: `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` — MUST report `>>> all pre-PR checks passed.` before commit. Per project memory `feedback_prepr_gate_full_output`, capture per-target `N passed; 0 failed` lines as verification evidence

### 4d. rpm-byte-identity verification (research Decision 3 — m185's most important invariant)

- [X] T015 [P] After T009-T011 land, verify the rpm-invariant explicitly (research Decision 3 + SC-005/SC-006 rpm-goldens subset): `git diff --stat mikebom-cli/tests/fixtures/golden/ | grep -i rpm` — every rpm.cdx.json / rpm.spdx.json / rpm.spdx3.json (and any adjacent rpm-labeled goldens) MUST show `0 changed`. If ANY rpm golden drifts, this is a CRITICAL failure — investigate immediately. Likely indicates the m185 4th-pass wholesale-wrap accidentally landed on the rpm call site (contradicting research.md Decision 3), or that the visibility bumps in T006 accidentally modified helper behavior. Note: FR-011 in spec.md is about m107 opkg regression tests continuing (NOT rpm-byte-identity — corrected per /speckit-analyze I1)

### 4e. Non-Yocto byte-identity verification (SC-005/SC-006)

- [X] T016 [P] After T009-T011 land, verify SC-005 + SC-006: `git diff --stat mikebom-cli/tests/fixtures/golden/` — every non-Yocto / non-opkg / non-ipk golden MUST show `0 changed`. Non-Yocto ecosystems (Cargo, npm, pip, Maven, Gradle, deb, apk) MUST be byte-identical to pre-m185. If any non-Yocto golden shows drift, investigate — likely indicates cross-reader contamination

### 4f. Filter-parity + license coverage verification (SC-001/SC-002/SC-003/SC-004)

- [X] T017 [P] Verify SC-001 (US1 individual case): construct a synthetic tempdir with a single `.ipk` file matching `test-pkg_1.0+git0+abc_def-r0_all.ipk`, run `mikebom sbom scan --path <tempdir> --format cyclonedx-json --output /tmp/m185-sc001.cdx.json`, assert (a) the emitted component has `purl = "pkg:opkg/test-pkg@1.0%2Bgit0%2Babc_def-r0?arch=all"`, (b) `name = "test-pkg"`, (c) `version = "1.0+git0+abc_def-r0"`. Codified as an integration test in `mikebom-cli/tests/ipk_multi_underscore_filename.rs` (new file) using `env!("CARGO_BIN_EXE_mikebom")` — matches the m182 T016 wiremock/tempdir end-to-end pattern
- [X] T018 [P] Verify SC-003 (US2 individual case): construct a synthetic opkg-status fixture in a tempdir with a stanza `Package: busybox\nVersion: 1.36.1-r0\nArchitecture: core2-64\nLicense: GPLv2 & bzip2-1.0.4\n`, run mikebom against it, assert the emitted CDX 1.6 component's `licenses[]` array contains the expected two-operand structure per contracts/license-pipeline.md §Emitted format shape. Codified as an integration test in `mikebom-cli/tests/opkg_license_extraction.rs` (new file)
- [X] T019 [P] Verify SC-002 + SC-004 aggregate targets: SC-002 (0 null-PURL components from filename-fallback path when scanning a fixture containing the 4-kernel-module shape) and SC-004 (≥80% components carry `licenses[]`) require either a real Yocto image fixture OR a substantial synthesized fixture. **Deferred to T020** — the real-Yocto verification is post-merge validation; T017/T018 unit-level coverage is sufficient to prove the classifier semantics work per SC-001/SC-003 individual gates

### 4g. Manual post-merge validation

- [~] T020 Deferred: post-merge validation against the yocto-test testbed's `003-ipk-package-format` feature (the same testbed run that surfaced #538 + #539). This closes the SC-002 + SC-004 aggregate gates against a real Yocto `core-image-minimal` scan. Not required for m185 merge — the T017/T018 integration tests + the golden regen coverage are sufficient. Coordinator: user re-runs the yocto-test testbed against alpha.59 (post-release) and confirms 9 → 0 null-PURL components + ≥80% license coverage. Filed as the closing verification step of the two issue reports

### 4h. Zero-new-dep verification (SC-009 / FR-012 explicit gate)

- [X] T021 [P] Verify SC-009 explicitly per the m184 T026 pattern. Command: `git -C /Users/mlieberman/Projects/mikebom stash && cargo tree -p mikebom | wc -l > /tmp/m185-tree-pre.txt && git stash pop && cargo tree -p mikebom | wc -l > /tmp/m185-tree-post.txt && diff /tmp/m185-tree-pre.txt /tmp/m185-tree-post.txt`. Expected: identical line counts. If nonzero delta, investigate — expected to be zero because m185 only touches source files inside `mikebom-cli/src/scan_fs/package_db/{ipk_file,opkg,rpm_file}.rs`; no `Cargo.toml` edit is proposed in any m185 task

## Dependencies

- **T001 → T002** (Setup) MUST complete before any other work.
- **T003 → T004** (US1 parser + tests — sequential; T004 depends on T003's parser semantic).
- **T006 → T007** (US2 helpers must be `pub(crate)` before opkg.rs can call them).
- **T007 → T008** (US2 pipeline + tests — sequential; T008 depends on T007's pipeline wiring).
- **T003, T004** (US1) can run entirely in parallel with **T006, T007, T008** (US2) — different files, no shared code.
- **T009 → T010 → T011** (golden regens — sequential per project convention).
- **T012** (docs) — independent, can land any time after T007.
- **T013** (walker audit) — independent, can run any time.
- **T014** (pre-PR gate) — requires ALL preceding tasks to have landed.
- **T015, T016** (FR-011 + SC-005/006 byte-identity gates) — after T009-T011.
- **T017, T018, T019** (integration tests + deferred aggregate gate) — parallel with each other; T017/T018 are additive integration tests that don't affect golden regen.
- **T020** (deferred manual validation) — post-merge.
- **T021** (zero-new-dep) — independent, can run any time.

## Parallel Execution Examples

**Phase 2 (US1) and Phase 3 (US2) can run entirely in parallel — no shared code**:
- US1 series: T003 → T004 (touches `ipk_file.rs` only)
- US2 series: T006 → T007 → T008 (touches `rpm_file.rs` + `opkg.rs`, disjoint from `ipk_file.rs`)
- The two series are file-independent and can be developed simultaneously by different implementers or in parallel commits.

**Phase 4 polish**:
- T009 → T010 → T011 (golden regens) — sequential
- T012 (docs), T013 (walker audit) — parallel with each other
- T015-T019 (verification gates) — parallel with each other; must run AFTER T009-T011
- T021 (zero-new-dep) — independent, can run any time

## Implementation Strategy

**MVP scope (this milestone)**: Both USs + polish = 21 tasks (T001-T021, with T020 deferred). Both ship in one PR per plan.md. Per-USR independence means either US could ship alone.

**Recommended commit cadence** — ~4-5 small commits on the branch:
1. T001-T002 (setup)
2. T003-T004 (US1 filename fallback — parser semantic + 6 unit tests)
3. T006-T008 (US2 opkg License extraction — visibility bumps + pipeline + 6 unit tests)
4. T009-T012 (polish: goldens + docs)
5. T013-T021 (verification + pre-PR)

**Fallback** (if implementation surprises arise): US1 and US2 can land in separate commits or even separate PRs. Per-USR independence + zero shared code makes the split trivial.

## Success Criteria Coverage

| SC | Gate | Task(s) |
|----|------|---------|
| SC-001 (US1 individual filename fix) | T017 integration test | T003 (implementation), T004 (unit), T017 (integration) |
| SC-002 (US1 aggregate — 9 → 0 null-PURLs) | T020 deferred manual validation | T017 (proves individual case works; aggregate deferred) |
| SC-003 (US2 individual License extraction) | T018 integration test | T007 (implementation), T008 (unit), T018 (integration) |
| SC-004 (US2 aggregate — ≥80% coverage) | T020 deferred manual validation | T018 (proves individual case works; aggregate deferred) |
| SC-005 (non-Yocto CDX byte-identity) | T016 backward-compat pin | T009, T016 |
| SC-006 (non-Yocto SPDX 3 byte-identity) | T016 backward-compat pin | T011, T016 |
| SC-007 (C122-adjacent cross-format parity) | Inherited from `SpdxExpression` emission pipeline | T009, T010, T011 (golden regen exercises all three formats) |
| SC-008 (existing tests continue) | Pre-PR gate | T014 |
| SC-009 (zero new Cargo dep) | `cargo tree` line-count diff | T021 |
| **Decision 3 (rpm-byte-identity invariant)** | **T015 rpm-byte-identity gate** | **T015 (critical — the most important invariant)** — Note: NOT FR-011 (which is the m107 opkg regression pin per spec.md; corrected per /speckit-analyze I1) |
