# Implementation Plan: Record which readers found a merged component (C56)

**Branch**: `1639-also-detected-via` | **Date**: 2026-11-21 | **Spec**: [spec.md](spec.md)

## Summary

#1558: when two readers find one component, waybill emits one component and discards the record that a second reader agreed. C56 was designed by milestone 105 and never connected, and the CycloneDX carrier it chose is schema-invalid (#1442).

**The approach** (research R1–R8):
- **One helper** replaces milestone 105's unused precedence module (`scan_fs/dedup.rs`), with two functions:
  - `record(site, subject, survivor, absorbed)` unions the absorbed C55 (and any C56 it carries) into the survivor's C56;
  - `normalise` drops the survivor's own C55, and drops C56 when it is empty or when the survivor has no C55.
- **Nine merge sites call it** (S1–S9, research R3): the general deduplicator and its Maven fold, the design-source reconciler, the CMake reader's R5, R4, R9 and vendored fold, and the two folds in `cmake_search_pair`. The sites that cannot join different C55 values are listed with the reason.
- **Survivor rule unchanged** (FR-003). No C55 changes.
- **Carriers.** C56 is a CDX component property, after a native-field audit against the vendored 1.6 schema found no carrier (R1). The schema-invalid `methods[]` key and its `build_evidence` parameters are removed. SPDX keeps its annotation. All three extractors use the annotation macros (R8).
- **The fingerprint collision** moves to a new row, C242 `waybill:fingerprint-also-matched`, with no alias (R7).
- **Census first.** The helper lands log-only, output byte-identical, and a CI census counts merges by site and mechanism pair before C56 is written (R6).

**One planning amendment** refines the spec (recorded in its Clarifications). R5: C56 is recorded only when the survivor carries C55. This keeps FR-012, since the reconciler never folds annotations.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- Code:
  - `resolve/deduplicator.rs` (`deduplicate`, `fold_declared_not_cached`);
  - `resolve/reconciler.rs`;
  - `scan_fs/package_db/cmake.rs` (`fold_vendored_into_projects`);
  - `scan_fs/package_db/cmake/fetches.rs` (`first_declaration_wins`, `one_per_purl`, `merge_into_existing`);
  - `scan_fs/cmake_search_pair.rs` (`pair`);
  - `generate/cyclonedx/evidence.rs` (`build_evidence`) and its callers in `builder.rs` and `metadata.rs`;
  - `scan_fs/binary/{entry.rs, symbol_fingerprint.rs, fingerprints/matcher.rs}`;
  - `parity/extractors/{mod,cdx,spdx2,spdx3}.rs`.
- Crates: `serde_json`, `tracing`.

**Zero new Cargo dependencies** (FR-014).

**Storage**: N/A. In process per scan.

**Testing**:
- **Integration:** `waybill-cli/tests/also_detected_via.rs`, with cases A-1..A-10, A-ctl and A-rep (`contracts/wire.md`):
  - parity agreement through `waybill::parity::extractors::EXTRACTORS` on C55, C56 and C242;
  - CycloneDX schema validation through `tests/common/cdx_schema.rs`;
  - unique bom-refs;
  - 10 repeat scans per case.

  Fixtures reuse `common::{git_fixture, cmake_fetch, installed_lib}` and the opkg/manifest shape of `cross_reader_dedup_ecosystem_107.rs`. A-ctl's golden is generated from `main`'s binary before any source change.
- **Existing tests updated:**
  - `cross_reader_dedup_ecosystem_107.rs` now asserts C56;
  - `dedup_precedence_determinism.rs`: its header and the spec 1530 case assert C56 where the merge has two mechanisms;
  - `evidence.rs` unit tests for the removed parameters are deleted.
- **Unit tests:**
  - the helper: union, self-exclusion, the anchor rule, empty-absent, sort and dedup, and transitivity;
  - the S1 fold skipping C56;
  - F-1 (C242 mapping) and F-2 (C242 through the emitters).
- **Mutations** (restore with `cp`, rebuild after the last one, a `^error\[E…\]` guard so a compile failure is not counted as a kill):
  - M1: S1's `record` call removed;
  - M2: S6's `record` call removed;
  - M3: `normalise` keeping the survivor's own C55;
  - M4: the anchor rule removed (A-8 must fail);
  - M5: C242 written back under the old key;
  - M6: the CDX extractor reading `methods[]`;
  - M7: the sort removed (A-rep must fail);
  - M8: S9's `record` call removed.
- **CI:**
  - the census (`census-merges.yml`, fixture repo, R6) at the log-only commit;
  - TheRock measure-source against `main`;
  - the public corpus (`regen_goldens`), twice: log-only (0 differing expected) and final;
  - the quality corpus against a `main` control.

**Target Platform**: the waybill CLI on Linux, macOS and Windows.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-006, at most 1.05× `main` on TheRock's source tree (median of at least 3). `record` is constant work per absorbed member.

**Constraints**:
- FR-010: which components merge is unchanged;
- FR-003: no C55 changes;
- FR-012: byte-identical output where no merge joins two mechanisms and no collision fires;
- FR-011: CDX schema-valid.

**Scale/Scope**: counted by the census (SC-001, T001). The spec quotes no figure in advance.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies. |
| III. Fail closed | ✅ C56 lists only mechanisms a reader actually recorded; nothing is inferred. No C56 without an anchoring C55 (R5). |
| IV. Type-driven correctness | ✅ One helper owns C56's rules; every site calls it, so no site re-implements the union or the order. The unused precedence enum is removed rather than left to drift (FR-008). |
| V. Specification compliance | ✅ Native-field audit performed against the vendored CycloneDX 1.6 schema (R1); the property is a documented parity bridge. The schema-invalid `methods[]` key is removed (#1442). The new row C242 has its audit and lands with its extractors. |
| VII. Test isolation | ✅ Synthetic `waybill-fixture-*` trees in temp dirs; TheRock and the quality corpus only in CI. |
| VIII. Completeness | ✅ Evidence that is currently discarded is recorded. |
| IX. Accuracy | ✅ One key, one meaning (FR-007). False comments and test headers are corrected (FR-009). |
| X. Transparency | ✅ A consumer can see which independent readers agreed on a component. |

Post-design re-check: unchanged. The only new `waybill:*` key is C242, which is a rename of an existing emission into its own row, with its audit in `contracts/wire.md`.

## Project Structure

### Documentation (this feature)

```text
specs/1639-also-detected-via/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/          (T001 census, mutations, gate log, CI traces)
```

### Source Code (repository root)

```text
waybill-cli/src/
├── scan_fs/also_detected.rs              (new; replaces scan_fs/dedup.rs)
├── scan_fs/dedup.rs                      (removed after T001)
├── resolve/deduplicator.rs               (S1, S2)
├── resolve/reconciler.rs                 (S3)
├── scan_fs/package_db/cmake.rs           (S7)
├── scan_fs/package_db/cmake/fetches.rs   (S4, S5, S6)
├── scan_fs/cmake_search_pair.rs          (S8, S9)
├── scan_fs/binary/entry.rs, symbol_fingerprint.rs, fingerprints/matcher.rs  (C242)
├── generate/cyclonedx/evidence.rs, builder.rs, metadata.rs                  (methods[] key removed)
└── parity/extractors/{mod,cdx,spdx2,spdx3}.rs                               (C56 macros, C242)
waybill-cli/tests/also_detected_via.rs    (new)
waybill-cli/tests/fixtures/also_detected_via/  (A-ctl golden)
docs/reference/sbom-format-mapping.md     (C56, C58, C207, C233, new C242)
docs/reference/identifiers.md §11.5, docs/reference/reading-a-mikebom-sbom.md, docs/ecosystems.md
CHANGELOG.md                              (Fixed: C56; Changed: C242 rename, a wire change)
```

**Structure Decision**: the existing single workspace. The fixture-repo workflow `census-merges.yml` and its script live in `kusari-sandbox/waybill-fixture-therock-sdk`, as spec 1638's census did.

## Complexity Tracking

None. Nine call sites of one helper is the scope FR-004 sets; each call is one line at the point where the site already removes the absorbed member.
