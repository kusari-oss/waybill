# Research: a package search's conditions do not mark a shipped library non-runtime

Traced in the source at `main` `857e542e`. SDK figures come from waybill's own document of measure-full run 44616350723.

## R1. The rule and the order it runs in

**Finding.** In `scan_fs/mod.rs` (~1555–1590), the passes run in this order:
1. `cmake_search_pair::pair` turns searches into pairings;
2. `cmake_link_attach::attach` collects links;
3. `cmake_search_pair::record_scopes(components, pairings, linked, link_conditional, targeted_before)` returns `{required, optional}`;
4. `cmake_link_attach::apply_scope` applies the link half (spec 1543, C235) to records not in `required`;
5. `cmake_search_pair::apply_scope` applies the search half (C122 `cmake-conditional`, plus C236 when there are conditions).

`record_scopes` makes a record Optional when:
- every pairing is `conditional`;
- it is not unconditionally linked, not already scoped, and not the target of an existing edge.

A pairing is `conditional` when the search component has **any** lifecycle scope (`c.lifecycle_scope.is_some()`), Test included.

## R2. What a record's evidence holds when the rule runs

**Finding.** `installed_lib::resolve` (spec 1531, FR-010) adds each record's matching shared-library files to its evidence. That covers `lib<m>.so`, `lib<m>.so.<N…>`, `lib<m>.dylib` and `lib<m>.<N…>.dylib` (`names_library`). Spec 1563's exported-targets locations (`IMPORTED_LOCATION`, static archives included) are read inside the same function. `resolve` runs at `scan_fs/mod.rs` ~686, long before step 3.

So whether a record ships a library is already in its own evidence when the rule runs. On the SDK the documents show it:
- `amd_smi` has `lib/libamd_smi.so*` and `libamd_smi_static.a`;
- `absl` has 88 `libabsl_*.a`.

T001 confirms that the `.a` files are present at step 3 and not added later.

**Decision (FR-001).** A record *ships a library file* when any path in its `evidence.source_file_paths` has a library file name:
- `*.so`, `*.so.<digits…>`;
- `*.a`;
- `*.dylib`;
- `*.dll`, `*.lib`.

The test is by extension, not by module name: the evidence was already attributed to this record by `resolve`, so its name is not in question here.

## R3. Making the search half skip a shipping record, and leaving the link half alone

**Decision.**
- In `record_scopes`, a record that ships a library file joins **neither** `optional` nor `required`.
- **The search half therefore never scopes it (FR-001).** Its searches still pair, give it its edge, and add their files to its evidence (FR-002).
- **The link half is untouched (FR-005).** The record is not in `required`, so `cmake_link_attach::apply_scope` still makes it Optional when every link is conditional.
- Measured: 0 records on the SDK have C235, so the link half changes nothing there.

**Alternative rejected:** putting a shipping record in `required`. That would also switch off the link half for it, which FR-005 keeps.

**C236 (FR-006).** It is written today only by `apply_scope`, for the records it makes Optional. A shipping record that this feature keeps required would lose it. To keep C236's values, a shipping record that **today's rule would have made Optional** still has its conditions written as C236, without the scope. A shipping record that today's rule would not have scoped (one with an unconditional link, an existing scope, or an incoming edge) gets no C236, exactly as today. A required record then carries C236, which is new. It still means "the conditions under which this was searched", which is what the row says (`docs/reference/sbom-format-mapping.md` C236).

## R4. Records searched only from test contexts (FR-004)

**Finding.**
- A search in a test directory is Test with C62 `source-test-directory`; one under `if(BUILD_TESTING)` is Test with C62 `cmake-build-testing` (`searches::scopes`).
- Both make a pairing `conditional` (R1), so a record searched only from test contexts becomes Optional. A test directory gives no conditions, so there is no C236.
- That is `rocprofiler-sdk-rocattach` and `rocprofiler-sdk-tests` on the SDK. Their CMake evidence is `share/rocprofiler-sdk/tests/{rocattach,unit-tests}/CMakeLists.txt`.

**Decision.**
- **Each pairing carries its search's scope and C62 derivation.**
- **A shipping record is decided by R3 first.** `rocprofiler-sdk-rocattach` ships `lib/librocprofiler-sdk-rocattach.so*`, so it is required.
- **A non-shipping record whose every pairing is Test becomes Test,** in a new `RecordScopes::test` set, with the derivation `source-test-directory` if any pairing has it, else `cmake-build-testing` (the same precedence as `searches::scopes`). C236 is written when the conditions are non-empty, as `apply_scope` does today.
- **A non-shipping record with at least one non-test conditional pairing** stays Optional, with C236 from all its pairings, as today. Mixed test and optional searches give Optional, as `searches::scopes` does for a name.

## R5. Spec 1533's test scope

**Finding.** `test_scope::apply` runs later, in `scan_cmd`, on installed trees, and scopes installed-library records among other kinds (`kind()`: `pkg-config`, `cmake-package-version`). It does not touch an already-scoped component. A shipping record that this feature keeps runtime can still be Test-scoped there, when it is test payload: under `tests/`, declared by CTest, or exported by a test package.

**Decision.** No change. The edge case "test payload keeps its scope" holds through spec 1533.

## R6. Predicted effect on the SDK (a prediction; SC-001 measures it)

| Record | Today | After |
|---|---|---|
| `hsa-runtime64`, `amd_smi`, `zstd`, `absl`, `openblas`, `openblas64` | Optional, C236 | required, C236 kept (R3) |
| `rocprofiler-sdk-rocattach` | Optional, no C236 | required (R3) |
| `rocprofiler-sdk-tests` | Optional, no C236 | Test, `source-test-directory` (R4) |
| `rocmcmakebuildtools` | Optional, C236 | unchanged: no library file, non-test conditional search |

**Edges.** Scope rewriting types the edges into these records, so they go from optional to plain dependency (`apply_lifecycle_scope_to_edges`).

**Propagation.** Spec 1269 propagation from a now-runtime record is unchanged, because a runtime record seeds nothing.

**TheRock's source tree:** 0 installed pairings (C237), so no change is predicted.

## R7. Scan time

`record_scopes` gains one extension check per evidence path of each paired installed record: 313 records on the SDK. Budget ≤ 1.05× `main` (SC-006).
