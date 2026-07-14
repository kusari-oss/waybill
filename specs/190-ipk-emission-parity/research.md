# Research: m190 ipk Emission Parity

**Date**: 2026-07-13
**Purpose**: Resolve the technical unknowns identified in `plan.md` Technical Context by locating exact code paths, verifying assumptions from the spec, and picking a single implementation approach per decision.

## R1 — Root cause of #550 (CDX emits raw BitBake operators)

**Decision**: Add a preprocessing pass `normalize_bitbake_license_operators(&str) -> String` in `ipk_file.rs` that runs BEFORE the existing `SpdxExpression::try_canonical(raw)` call at `ipk_file.rs:826`.

**Rationale**: Verified pipeline via direct code inspection:

1. Raw License string arrives at `ipk_file.rs:824-840`.
2. `SpdxExpression::try_canonical(raw)` at line 826 delegates to `spdx::Expression::parse` (per `mikebom-common/src/types/license.rs:135`). The `spdx` crate's parser does NOT recognize `&`/`|` as operators — it only recognizes SPDX-legal `AND`/`OR`/`WITH`. So canonicalization fails on any BitBake-compound input.
3. The fallback at line 832 (`SpdxExpression::new(raw)`) is the LENIENT constructor; it stores the raw string verbatim (`impl` at `mikebom-common/src/types/license.rs:104-133`).
4. That raw string then flows through `entry.licenses[]` → `ResolvedComponent.licenses[]` → all three format emitters.
5. **CDX emitter** at `mikebom-cli/src/generate/cyclonedx/builder.rs:876-940` reads `component.licenses` and emits as-is (no re-normalization). Raw `&` appears in `.components[].licenses[].expression`. THIS IS #550.
6. **SPDX 2.3 emitter** at `mikebom-cli/src/generate/spdx/packages.rs` reads the same field but has a m152/153-era `LicenseRef-<hex>` fallback that converts the raw text into a hashed LicenseRef when it cannot canonicalize. So the SPDX 2.3 output on #550's example emits `LicenseRef-<hex>` — technically SPDX-conformant, but NOT literally `GPL-2.0-only AND MIT`. #550's report language ("SPDX 2.3 canonicalizes") is *approximately* correct — it avoids raw operators — but the emitted form is a LicenseRef, not the intended AND-joined SPDX expression.

**Implication**: The preprocessing fix in the reader is strictly better than any downstream normalization — it produces a canonicalized SPDX expression in all three formats (US1 acceptance criterion #4: "semantically equivalent across formats"), instead of the current state where SPDX 2.3 emits a LicenseRef and CDX emits raw text.

**Alternatives considered**:
- (A) Extend `SpdxExpression::try_canonical` in `mikebom-common` to accept `&`/`|`: rejected because it widens the newtype's behavior for every reader (dpkg, apk, rpm, etc.), some of which may legitimately treat `&` as literal text.
- (B) Fix only in the CDX emitter path (route through a new normalizer): rejected because it leaves SPDX 2.3 emitting LicenseRef and doesn't fix #551 either.
- (C) Fix only in the SPDX 3 emitter: rejected because it doesn't fix #550.
- (D — **CHOSEN**) Preprocess in the ipk reader before try_canonical: single-point fix, fixes all three formats simultaneously, matches Q2's decision (option A: string-level substitution).

**Operator handling per Q1 (session 2026-07-13)**: Substitution order MUST be long-form before short-form to avoid partial-token overlap:
- `&&` (with any surrounding whitespace) → ` AND `
- `||` (with any surrounding whitespace) → ` OR `
- `&`  (with any surrounding whitespace) → ` AND `
- `|`  (with any surrounding whitespace) → ` OR `

Regex-per-form OR a single hand-rolled state-machine: pick the state-machine for correctness (avoids partial-match hazards when regex engines lazy-match `&&` as `&` `&`). Alternative: 4 sequential `str::replace` calls in the long-first order — simpler and correct for these tokens because `&&` fully consumes both `&` bytes before the `&` substitution runs. **Chosen**: 4 sequential `str::replace` calls, one per operator form, long-form first. Justifies its own doc-comment noting the ordering invariant.

**References**:
- `mikebom-cli/src/scan_fs/package_db/ipk_file.rs:823-840` — current license routing.
- `mikebom-common/src/types/license.rs:104-215` — SpdxExpression newtype.
- `mikebom-cli/src/generate/cyclonedx/builder.rs:876-940` — CDX license emit.
- `mikebom-cli/src/generate/spdx/v3_licenses.rs:141-161` — SPDX 3 license reduction.

## R2 — Root cause of #551 (SPDX 3 emits no license fields)

**Decision**: Investigation task deferred to implementation phase; the R1 preprocessing fix alone SHOULD resolve #551 assuming the `ResolvedComponent.licenses` Vec is non-empty and the ipk component is present in `package_iri_by_purl`. If US2 acceptance still fails after R1, the follow-up investigation is: (a) verify `ResolvedComponent.licenses` is populated from the parsed `PackageDbEntry`, (b) verify the ipk PURL is present in `package_iri_by_purl`, (c) audit the pipeline between `ipk_file.rs::parse_entry` and `spdx/v3_licenses.rs::build_license_elements_and_relationships`.

**Rationale**: Code inspection of `spdx/v3_licenses.rs:40-119` shows the emitter loops over every `ResolvedComponent`, reads `c.licenses`, and emits a `simplelicensing_LicenseExpression` for any non-empty list. Two hypothesized failure modes:

1. **Empty `licenses` Vec**: The lenient constructor `SpdxExpression::new(raw)` at `ipk_file.rs:832` could fail on some inputs (currently only fails on empty/whitespace-only per `license.rs:253`). Not the actual root cause for compound BitBake inputs — those pass the lenient constructor.
2. **PURL lookup miss** at `v3_licenses.rs:54-56`: If the ipk PURL isn't in `package_iri_by_purl`, the entire iteration `continue`s and no license element is emitted for that component. This would be a wiring bug orthogonal to license normalization.

Per issue #551's evidence sample, SPDX 2.3 output for the same scan DOES emit license info (via LicenseRef- fallback), which proves the ipk reader IS producing components with non-empty license fields. That rules out hypothesis 1. Hypothesis 2 remains plausible; also plausible is a subtler routing bug where the ipk component's PURL is present but the emission short-circuits earlier.

**Implementation approach**: Ship the R1 fix (preprocessing) and add a US2 integration test that asserts `simplelicensing_LicenseExpression` presence for a compound-license ipk. If the test passes → US2 is transitively fixed by R1. If the test fails → drill into the pipeline. The added test is required regardless (it's how we prove US2's acceptance criterion #4).

**Alternatives considered**:
- (A) Front-load the investigation before writing any fix code: rejected because R1 is a certain-fix for #550 that also plausibly closes #551; the empirical path is faster.
- (B) Add a runtime assertion that every ipk component with a non-empty license produces a SPDX 3 license element: rejected because it's a debugging tool, not a production feature; the integration test does the same job.

## R3 — Empty / missing License field (Q3 answer B)

**Decision**: Verify what each of the three emitters currently does on empty `component.licenses`, then align to the format-idiomatic convention chosen in Q3:
- SPDX 2.3 → `licenseDeclared: "NOASSERTION"`
- CDX 1.6 → `licenses: []` (empty array) or omit the field entirely (both are format-legal; pick omit-field to preserve pre-fix byte-identity where possible)
- SPDX 3 → omit the `hasDeclaredLicense` relationship + `simplelicensing_LicenseExpression` element for that Package

**Rationale**: `spdx/v3_licenses.rs:58-84` already respects this convention — when `reduce_license_vec` returns `None` for an empty input, the block at line 61 (`if let Some(expr) = &declared_expr`) skips emission entirely. So the SPDX 3 side is ALREADY correct per Q3.

For SPDX 2.3, verify at implementation time: if the current rpm-side behavior emits `licenseDeclared: "NOASSERTION"` on empty input, the ipk side inherits the same treatment through the shared `packages.rs` emitter. If NOT, this becomes an additional task to align both readers.

For CDX 1.6: verify at implementation time. Preferred outcome — the emitter already handles empty `licenses` by omitting the field (matches Q3 answer B). If it emits an empty `licenses: []` array today, that's ALSO format-legal per CDX 1.6 §5.3.2 and byte-identical to existing goldens; leave unchanged.

**Alternatives considered**:
- (A) Introduce a shared `LicenseAbsentBehavior` policy struct that all three emitters consult: rejected as over-engineering for what is effectively a static per-format constant.

**Investigation deliverable**: Add a task to Phase 3 that grep-verifies the three emitters' current empty-license behavior and captures the finding in a code comment referencing this research note.

## R4 — ipk epoch extraction pattern (US3, #552)

**Decision**: Add a helper `parse_opkg_version_with_epoch(raw: &str) -> (Option<u32>, String)` in `ipk_file.rs`. Regex: `^(\d+):(.*)$`. Store the parsed epoch on a new `epoch: Option<u32>` field on the parsed control record; use it in `build_opkg_purl` to emit `?epoch=N` per Q4's decision.

**Rationale**: The rpm reader's implementation at `rpm_file.rs:397-411` is the canonical reference:

```rust
// EPOCH goes in the `&epoch=N` qualifier, NEVER inline in the version
...
// Omit epoch=0; treat 0 as semantically "no epoch" (matches the
// purl-spec convention where epoch is implicit-zero).
let epoch_seg = match epoch {
    Some(v) if v != 0 => format!("&epoch={v}"),
    _ => String::new(),
};
```

The rpm-side pattern:
- Uses `rpm::PackageMetadata::get_epoch()` (via the `rpm` crate) — ipk has no such crate; we parse the version string ourselves.
- Formats as `&epoch={v}` — because it's appended AFTER an existing `?arch=<arch>` qualifier (the `&` is the qualifier separator, not the initial `?`).
- Explicitly skips zero-valued epochs per purl-spec convention.

Adapted for ipk:
- Source of truth: the control-file `Version:` field. Fallback: filename (per FR-012). Filename form: `pkg_<epoch>:<version>-<release>_<arch>.ipk`.
- Parse: regex `^(\d+):(.*)$` applied to the `Version:` value; on match, epoch = capture 1 parsed as u32, naked-version = capture 2.
- Emit into PURL after arch: existing `build_opkg_purl` already appends `?arch=<arch>` (and optionally `&distro=<tag>`); insert `&epoch=<N>` immediately after `?arch=<arch>` (before `&distro=` if present) — this matches purl-spec §5.6's alphabetical qualifier-key ordering (`arch` < `distro` < `epoch` is false — `epoch` starts with `e`, distro with `d`, arch with `a`, so alphabetical is `arch < distro < epoch`). Correction: emit after distro to keep alphabetical. Alphabetical order per purl-spec: `arch, distro, epoch`.

**Purl-spec conformance check**: The purl-spec's opkg type definition (spec.md link) explicitly permits `?epoch=<N>` per the deb/rpm precedent. Verified by cross-referencing the current rpm reader's emission format (`pkg:rpm/fedora/epochy@2.0-1?arch=noarch&epoch=7` per `rpm_file.rs:1080`) — the same qualifier-ordering applies here.

**Alternatives considered**:
- (A) Store epoch as `Option<NonZeroU32>` (encodes "zero == absent" in the type): rejected as overkill; `Option<u32>` + explicit `if v != 0` check is simpler and matches the rpm pattern verbatim.
- (B) Detect epoch from the filename only (skip control-file source): rejected because filename detection alone is fragile — some real-world Yocto builds emit ipks with the epoch elided from the filename but present in control (per Yocto's `PACKAGE_ARCH` / `PACKAGE_ARCH_KERNEL` rules).
- (C) Push epoch into PURL as inline `@<epoch>:<version>-<release>`: rejected — that's the current buggy behavior we're fixing.

**References**:
- `mikebom-cli/src/scan_fs/package_db/rpm_file.rs:397-411` — canonical epoch-qualifier emission.
- `mikebom-cli/src/scan_fs/package_db/rpm_file.rs:1062-1080` — canonical epoch test.
- `mikebom-cli/src/scan_fs/package_db/ipk_file.rs:1087-1130` — current `build_opkg_purl` (to be extended).

## R5 — SPDX 3 CustomLicense sweep for vendor licenses (US2 acceptance #3)

**Decision**: The existing `sweep_custom_licenses` at `v3_licenses.rs:248-288` handles emission of `simplelicensing_CustomLicense` elements for any `LicenseRef-*` token appearing in a `simplelicensing_LicenseExpression` expression string. As long as the ipk reader's lenient-constructor path produces a `LicenseRef-<hex>` value for vendor licenses (which per `ipk_file.rs:829-830` it does via the m152 fallback), the CustomLicense element WILL be emitted downstream.

**Rationale**: Direct code inspection — the sweep at `v3_licenses.rs:266-284` matches capture-group-1 of the `LicenseRef-[a-zA-Z0-9.-]+` regex on every emitted expression string, dedups by idstring, and emits one `simplelicensing_CustomLicense` per unique LicenseRef. This is byte-identical to the rpm path (which was the original m154 sweep target).

**Implication**: No new code for US2 acceptance criterion #3. The R1 preprocessing fix + the confirmed SPDX 3 emission path together produce CustomLicense elements for vendor operands as a side effect. Acceptance criterion #3 is verified by an integration test.

## R6 — spdx3-validate integration (US2 acceptance #5, FR-007)

**Decision**: Extend the existing `spdx3-validate==0.0.5` CI gate (per memory `reference_spdx3_validator`) to include a new fixture that is an ipk directory with compound + vendor licenses. Reuse the existing test-helper that shells out to `.venv/spdx3-validate/bin/spdx3-validate`.

**Rationale**: The tool is already installed and CI-gated (`MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1`). Adding a new positive fixture is a single-file test addition, not a tooling change. Version pinning stays at `0.0.5`.

**References**:
- Memory `reference_spdx3_validator`.

## R7 — Test fixture strategy (m187 pattern reuse)

**Decision**: Synthesize new ipk fixtures at test time via the existing ar-format builder helper introduced in m187 (`mikebom-cli/tests/ipk_file_reader.rs` uses a hand-rolled BSD ar-format writer). Six new fixtures needed:

| Fixture | Control File Content (relevant fields) | Verifies |
|---|---|---|
| `ipk_license_mit.ipk` | `License: MIT` | US1 acceptance #1 baseline (byte-identity of no-op canonical) |
| `ipk_license_bitbake_and.ipk` | `License: GPL-2.0-only & MIT` | US1 #1, US2 #2, FR-013 cross-format equality |
| `ipk_license_bitbake_or.ipk` | `License: MIT \| Apache-2.0` | US1 #2 |
| `ipk_license_double_ops.ipk` | `License: MIT && Apache-2.0 \|\| BSD-2-Clause` | Q1 double-form coverage |
| `ipk_license_vendor.ipk` | `License: SomeVendorLicense` | US2 #3, US1 #5 |
| `ipk_license_grouped.ipk` | `License: (GPL-2.0-only & MIT) \| Apache-2.0` | US1 #3 grouping preservation |
| `ipk_epoch_positive.ipk` | filename `pkg_1:2.0-r0_all.ipk` + `Version: 1:2.0-r0` | US3 #1 |
| `ipk_epoch_zero.ipk` | `Version: 0:1.0-r0` | US3 #3 |
| `ipk_epoch_none.ipk` | `Version: 2.0-r0` | US3 #2 byte-identity |

**Total footprint**: ~9 fixtures × ~30 KB each = ~270 KB. Consistent with the m187 fixture footprint (~500 KB reported at Feature 003 clarification). Well under the "keep in-repo per memory `project_test_fixture_stayset`" bar.

**Rationale**: The m187 fixture-builder helper is proven (all m187 tests pass); reusing it avoids introducing a second fixture-generation path. Fixture content is synthesized at test-init time from string literals — no binary blobs to check in, matches Constitution Principle I (Pure Rust).

## R8 — Backward-compatibility with existing goldens (FR-014, SC-006)

**Decision**: Grep every golden `.cdx.json` and `.spdx.json` for BitBake operator characters (`&` outside quoted strings; `|` outside quoted strings) and for `\d+:` version-prefix patterns in ipk-typed components. Any hit → those specific goldens MUST be regenerated as part of this milestone. Any golden without a hit → byte-identity MUST be preserved (SC-006 gate).

**Rationale**: Deterministic identification of drift set. Prevents surprise byte-identity failures at PR time.

**Regen strategy** (per memory `feedback_release_bump_regen_all_golden_tests`):
1. Identify affected goldens via grep.
2. Run `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test -p mikebom` on the identified test files only.
3. Diff-review the regenerated files: every diff MUST be either (a) a `&`/`|` → `AND`/`OR` transformation, (b) a `<digits>:<version>` → `<version>?epoch=<digits>` transformation, or (c) a version-string change resulting from PURL canonicalization. No other diffs permitted.
4. Commit the regen alongside the fix, cite the exact drift class in the commit message.

**Alternatives considered**:
- (A) Nuclear regen of ALL six golden test files: rejected because it discards SC-006's byte-identity gate as a regression signal; we WANT to see byte-identity failures on unrelated goldens if they occur, because that indicates a hidden regression.

## R9 — Constitution Principle V audit (native-first)

**Decision**: NO new `mikebom:*` annotation is introduced. Every fix uses standards-native fields:

| Fix | Native Field Used | Format |
|---|---|---|
| US1 CDX license normalization | `components[].licenses[].expression` | CycloneDX 1.6 §5.3.2 |
| US2 SPDX 3 license emission | `simplelicensing_LicenseExpression` + `simplelicensing_CustomLicense` | SPDX 3.0.1 § simplelicensing_License Expression |
| US3 epoch qualifier | PURL `?epoch=<N>` qualifier | purl-spec §5.6 opkg type |

**Rationale**: Direct audit result satisfying Principle V's "spec authors MUST cite the audit result in the spec's Functional Requirements". Recorded in FR-015. Reviewers can reject any implementation-time drift that introduces a `mikebom:*` alternative.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Where to fix operator normalization | ipk reader (before try_canonical) | CDX emitter shim; extend SpdxExpression | Single-point fix; benefits all 3 emitters |
| Operator-substitution mechanism | 4 sequential `str::replace` (long-form first) | Regex; state machine | Simpler + correct given ordering invariant |
| Empty-license semantics | Per-format idiomatic (NOASSERTION / `[]` / omit) | Uniform NOASSERTION | Matches downstream consumer expectations |
| Epoch parser location | New helper in ipk_file.rs | Extend `Purl` crate; put in mikebom-common | Reader-local; matches rpm pattern |
| Epoch qualifier position | Alphabetical after arch, distro (per purl-spec §5.6) | End-of-qualifiers | purl-spec conformance |
| Scope | opkg only; follow-up for dpkg + apk | Sweep all 3 | Per Q4; keeps milestone bounded |
| SPDX 3 CustomLicense | Reuse existing `sweep_custom_licenses` | New sweep | Already handles LicenseRef-* tokens verbatim |
| Test-fixture strategy | Reuse m187 ar-format builder | Check in real Yocto ipks | No binary blobs; pure Rust generation |
| Golden regen strategy | Targeted (grep-identified) | Nuclear (all six test files) | Preserves byte-identity signal on unrelated goldens |
| New Cargo deps | None | Add SPDX-mode-specific crate | Zero-new-deps posture confirmed |
