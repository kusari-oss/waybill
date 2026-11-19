# Feature Specification: A package search's conditions do not mark a shipped library non-runtime

**Feature Branch**: `1617-installed-record-scope`
**Created**: 2026-11-19
**Status**: Draft
**Input**: #1623. In an installed tree, a library the tree ships is marked non-runtime because the tree's own CMake config files search for it only under a condition, or only from shipped test projects.

## Background

Spec 1614 (#1591, #1605) reads package searches (`find_package`, `pkg_check_modules`, `pkg_search_module`) from every CMake file and pairs each with the component it names. For an installed-library record (spec 1531: a pkg-config `.pc` file or a CMake package-version file), the pairing does two things:
- **It gives the record its edges.** That part is correct.
- **It decides the record's scope.** Spec 1614 extended spec 1543's rule so that a record is Optional (C122 `cmake-conditional`) when every link (C235) and every search of it is non-runtime, and nothing else scopes or targets it. Its conditions go in C236, when there are any.

In an installed tree, a shipped config file's conditional `find_dependency` says when a *consumer* of the tree needs the library. It does not say whether the tree ships it.

**Measured on the ROCm TheRock SDK install tree.** `therock-dist-linux-gfx1150-7.14.0a20260612`, measure-full run 44616350723, waybill `main` `857e542e`; the SDK was scanned only in CI. Of 313 named, versioned installed-library records, 9 are non-runtime, all through this rule.

| Record | Ships | Why non-runtime |
|---|---|---|
| `hsa-runtime64@1.21.0`, the core ROCm runtime | `lib/libhsa-runtime64.so*` | searched under `else: rocprofiler-sdk_BUILD_TREE AND EXISTS …` |
| `amd_smi@26.5.0.99999` | `lib/libamd_smi.so*`, `libamd_smi_static.a` | `else: TARGET amd_smi OR TARGET amd_smi_static` |
| `zstd@1.5.7` | `librocm_sysdeps_zstd.so.1.5.7` | `LLVM_ENABLE_ZSTD` |
| `absl@20250512` | 88 `libabsl_*.a` | `NOT TARGET absl::strings` |
| `openblas@0.3.31.dev`, `openblas64@0.3.31.dev` | `librocm-openblas{,64}.so.0.3` | `NOT TARGET cblas` / `NOT TARGET cblas64` |
| `rocprofiler-sdk-rocattach@1.3.1` | `lib/librocprofiler-sdk-rocattach.so*` | searched only from the shipped test project `share/rocprofiler-sdk/tests/rocattach/` |
| `rocmcmakebuildtools@0.14.0` | CMake modules only | `NOT GPU_TARGETS AND NOT AMDGPU_TARGETS` |
| `rocprofiler-sdk-tests@1.3.1` | no library file | searched only from `share/rocprofiler-sdk/tests/unit-tests/` |

**Traced in the source** (`main` `857e542e`, `scan_fs/cmake_search_pair.rs`):
- **What counts as conditional.** A pairing is conditional when the search component has any lifecycle scope, Test included (`conditional: c.lifecycle_scope.is_some()`).
- **Searches from a test context.** Such a search is Test-scoped and carries no `if()` conditions. It makes the record **Optional** (not Test), and `apply_scope` writes C236 only when there are conditions. That is why the two `rocprofiler-sdk` records are `cmake-conditional` with no C236.
- **The link half (C235, spec 1543).** No record on the SDK has C235, so on the SDK every one of the 9 comes from the search half.
- **TheRock's source tree.** Spec 1614 paired 0 searches with installed records there (C237 `paired.installed` = 0 in run 44477517009).

**Inference, not measured:** C236 and the search half exist only since spec 1614, so the records it scopes were required before it. No pre-1614 SDK document was compared.

**A scan-type gate would be too blunt.** Spec 1533's "installed tree" classification (`InstalledTree::Recognised`) is set whenever the scan holds an installed library that no OS package owns. A source tree with one vendored prebuilt library qualifies. A rule gated on that classification would also change source trees.

## Clarifications

### Session 2026-11-19

- Q: Which records may the search conditions no longer make Optional? → A: Any installed record whose own evidence includes a library file it ships, decided per record (FR-001).
- Q: How is a record scoped when it ships no library file and is searched only from test contexts? → A: Test, with the C62 derivation its searches carry (FR-004).
- Q (at implementation, T012): a record searched from a tests directory and also under a condition. Spec 1614's name rule scopes every search of that name Test. Should the record follow it, or stay Optional? → A: Follow it: Test. The paired record takes its searches' own scope. That spec 1614's rule lets Test beat Optional, against spec 1269's order, is a separate issue.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A library an SDK ships is a runtime component (Priority: P1)

An SBOM consumer reviewing an SDK sees its core runtime, `hsa-runtime64`, marked as an optional dependency. The reason is that a config file elsewhere in the SDK looks for it inside an `else()` branch. A vulnerability triage that skips non-runtime components would skip the runtime.

**Why this priority**: It mislabels what ships, which is the direction waybill treats as most dangerous (spec 1269).

**Independent Test**: Scan an installed prefix with a library, its version file, and a config file that searches for it under `if(NOT TARGET …)`. The record is required, and C236 still records the condition.

**Acceptance Scenarios**:

1. **Given** an installed prefix shipping `libwaybill-fixture-lib.so` with its CMake version file, and a shipped config file that searches for it only under `if(NOT TARGET waybill_fixture_lib)`, **When** it is scanned, **Then** the record is required, with C236 `["NOT TARGET waybill_fixture_lib"]` and an edge from the searching package.
2. **Given** the same library searched only in an `else()` branch, **When** it is scanned, **Then** the same.
3. **Given** it searched unconditionally, **When** it is scanned, **Then** it is required with no C236, as today.

---

### User Story 2 - A library searched only from a shipped test project is still shipped (Priority: P1)

An SDK ships test projects next to its libraries. A library that only those test projects search for is currently marked optional with no stated condition, although the SDK ships it.

**Why this priority**: The same mislabel, with no condition recorded to explain it.

**Independent Test**: Scan an installed prefix shipping a library whose only search is in `share/<pkg>/tests/x/CMakeLists.txt`. The record is runtime.

**Acceptance Scenarios**:

1. **Given** a shipped library whose only searches are in a shipped test project, **When** it is scanned, **Then** the record is required.
2. **Given** a record that ships no library file and is searched only from a test project (like `rocprofiler-sdk-tests`), **When** it is scanned, **Then** it is Test, with its searches' C62 derivation (FR-004).

---

### User Story 3 - What the tree does not ship stays optional (Priority: P1)

A conditional search for a library that the tree does not contain is still an optional dependency of the tree, as spec 1614 made it.

**Why this priority**: The fix must not flip the other half of spec 1614's rule.

**Independent Test**: A conditional search that pairs with no installed record stays Optional with C236.

**Acceptance Scenarios**:

1. **Given** `if(WITH_X) find_package(waybill_fixture_absent)` and no such library in the tree, **When** it is scanned, **Then** the search component is Optional with C236 `["WITH_X"]`, as today.

---

### Edge Cases

- **Test payload.** A library that spec 1533 scopes as test payload (under `tests/` in an installed tree, declared by CTest, or exported by a test package) keeps that scope.
- **One unconditional search.** If any search is unconditional, the record is required, as today.
- **Something else already scopes the record.** A record with its own reader-assigned scope keeps it, as today.
- **A conditional link (C235).** It keeps spec 1543's behaviour unless FR-001's decision covers links as well. Measured: 0 records with C235 on the SDK.
- **Evidence.** C236 and C235 keep their values. On a shipped record, C236 is recorded without making the record Optional, so a required record can carry it. Only the scope, C62 and C122 change.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The search half of the record-scope rule MUST NOT make Optional any installed record whose own evidence includes a library file it ships (`.so`, `.so.*`, `.a`, `.dylib`, `.dll`, `.lib`). This is decided per record, not by how the whole scan is classified: a library present in the tree is runtime content of it. A vendored prebuilt library in a source tree is treated the same way.
- **FR-002**: A search MUST still pair with the installed record it names, give the record its edge, and record its conditions in C236, whatever FR-001 decides.
- **FR-003**: A conditional search that pairs with no installed record MUST keep spec 1614's behaviour (US3).
- **FR-004**: A record that ships no library file, and whose every search is Test-scoped by spec 1614's name rule, MUST be Test, with the C62 derivation its searches carry (`source-test-directory`, else `cmake-build-testing`), not Optional. The record takes the scope its searches have. Spec 1614 scopes a name Test whenever any of its searches is in a test directory or under `BUILD_TESTING`, including a name also searched under another condition (clarified 2026-11-19).
- **FR-005**: Spec 1543's link half (C235) MUST keep its behaviour. This feature changes only the search half.
- **FR-006**: The emitted catalog rows MUST keep their values: C235, C236 and C237 (`paired`, `read`, `conditional`, …). Only C42, C62 and C122 may change on affected records, plus the edges' relationship types that follow from scope.
- **FR-007**: A tree whose installed records are not searched only conditionally or from test contexts MUST produce output byte-identical to `main`.
- **FR-008**: No new Cargo dependencies.

### Key Entities

- **Installed-library record**: a component from spec 1531's readers (pkg-config, CMake package-version). Its evidence lists the files it covers, including the library files it ships.
- **Record-scope rule**: spec 1543's link half plus spec 1614's search half, which decide whether a record is Optional.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the TheRock SDK, `hsa-runtime64`, `amd_smi`, `zstd`, `absl`, `openblas`, `openblas64` and `rocprofiler-sdk-rocattach` are required, each still carrying C236 where it has one today. `rocprofiler-sdk-tests` is Test, and `rocmcmakebuildtools` (no library file; a non-test conditional search) stays Optional with C236.
- **SC-002**: On the SDK, every other change in scope is traced to a record FR-001 or FR-004 covers. No component gains a non-runtime scope, and none is added or removed.
- **SC-003**: TheRock's source tree is unchanged against `main`.
- **SC-004**: Every public-corpus golden change (from a regeneration) and every quality-corpus change against a `main` control is traced component by component.
- **SC-005**: The three formats agree on C42, C62, C122, C235 and C236 (`waybill sbom parity-check` shows no gap `main` does not). No duplicate `bom-ref`s; repeat scans are identical across two runners.
- **SC-006**: SDK scan time stays within 1.05× `main`'s median, measured with measure-full.
- **SC-007**: Each rule this feature changes (FR-001, FR-002's C236 on shipped records, FR-004, FR-005) has a mutation that a test catches. FR-003 is behaviour this feature does not touch; it is guarded by a regression test (I-8).

## Assumptions

- **The SDK measurements** are from waybill's own document of run 44616350723. The cause of each non-runtime record was traced in that document's properties and in the source.
- **"Ships a library file"** is read from the record's own evidence (`waybill:source-files`), which already lists the library files spec 1531 and spec 1563 attributed to it.
- **TheRock is measured only in CI.** Fixtures use synthetic `waybill-fixture-*` names.
