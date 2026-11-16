# Mutations (T022)

**Method:** `mutate.py` applied each mutation. `mutate-run.sh` then ran:
- the unit tests (`root_edges`, `declared_attach`, `s1537`);
- `tests/declared_requirements.rs`, all 15 cases;
- `tests/transitive_parity_pip_poetry.rs`.

Every touched file was restored with `cp`, and `git diff --quiet` confirmed it matched the committed tree. No mutation stopped compilation.

| Mutation | What it breaks | Failing tests |
|---|---|---|
| M1 | anchors close the gate (CycloneDX and SPDX 2.3 at `main`, #1566) | 3: `an_anchor_does_not_close_the_gate`, `the_root_is_never_its_own_target_and_a_missing_anchor_is_dropped`, `an_anchored_root_with_no_edges_is_complete` |
| M2 | the fallback applies only to a synthetic root (SPDX at `main`) | 2: `transitive_edges_match_baseline` (poetry: 89 → 64), `c9_os_packages_with_a_placeholder_root` |
| M3 | a lockfile-resolve anchor closes the gate (#894) | 1: `a_resolve_anchor_does_not_close_the_gate` |
| M4 | nested components count as graph-tops | 2: `a_nested_component_is_not_a_graph_top`, `every_target_is_declared_anchored_or_an_open_gate_graph_top` |
| M5 | completeness ignores anchors | 7: C-3, C-5, C-6, C-7, C-8b and the classifier's `a_declared_edge_closes_the_gate…`, among others |
| M6 | the owner walk reaches the scan root (FR-002) | 1: `a_file_at_the_scan_root_is_a_root_anchor_even_beside_a_main_module` (run alone after the redefinition below) |
| M7 | precedence reversed: a submodule outranks a main module | 2: C-6, `a_submodules_own_python_project_outranks_the_submodule` |
| M8 | the walk stops at the file's own directory | 1: `the_walk_continues_to_the_nearest_enclosing_owner` |
| M9 | the pip duplicate merge drops the second declaring file | 2: C-7, `s1537_a_package_pinned_in_two_files_keeps_both_paths` |
| M10 | the pass runs after the first lifecycle typing | **0: equivalent**, see below |
| M11 | declared root anchors are not installed | 5: C-3, C-5, C-6, C-7, C-8b |
| M12 | a non-pip (cget) `requirements.txt` component is accepted | 1: `only_pypi_requirements_declarations_count` |
| M13 (FR-006) | SPDX 2.3 bypasses `root_targets` | 14: every C-case through I-1 |
| M14 (FR-009) | top-level submodule anchors are not installed | 1: C-6 |

## Two mutations survived the first run. Both are now explained.

### M6: the first form exercised dead code

M6 first removed `OwnerIndex::build`'s "the scan root is not an owner" exclusion, and nothing failed. `owner_of` stops its walk below the scan root (`take_while(|d| !d.as_os_str().is_empty())`), so a scan-root entry in the index is never looked up. The exclusion was a second guard that could not act.

It is deleted, and M6 now removes the walk's stop. That is the guard FR-002 actually rests on, and the mutation is caught.

### M10: an equivalent mutation

Moving the pass after `scan_fs`'s `apply_lifecycle_scope_to_edges` changes no output. C-8b, an owner edge from `tools/requirements-dev.txt`, still comes out `OPTIONAL_DEPENDENCY_OF`.

The reason: `cli/scan_cmd.rs:5434` re-runs the same typing, unconditionally, after scope propagation, and it types every edge that is still plain `DependsOn`. FR-010 therefore holds wherever the pass sits. The code comment and the data model said the order was required; both were corrected to say it is conventional.

C-8b stays as the end-to-end check that owner edges carry the declaring file's scope.

## Run 2: after the corpus fixes (`e3dfd637`, research R15)

The corpora found two defects, and the corrections redefine two mutations. Read in that light, run 1's table records the defects.

**M6 was killed by one test, and it asserted the wrong rule.** That test was `a_file_at_the_scan_root_is_a_root_anchor_even_beside_a_main_module`.

**M11 was killed by C-3, C-5, C-6, C-7 and C-8b.** Each has a scan-root project. Under the original FR-002, a scan-root file in those cases reached the root only through a declared anchor, beside a project that should have owned it.

**Changes since run 1:**
- **M6 now re-adds the stop below the scan root,** that is, the original FR-002.
- **M8 now reads** "the walk stops at the file's own directory", anchored on the new walk.
- **M15 is new (FR-002a):** the pip reader ignores `# via`.
- **M16 is new (FR-002a):** every pin of a file is direct.
- **M12 is re-anchored.** The attach loop's pip/non-main filter was a redundant second copy of `members_by_file`'s, so it is deleted. M12 mutates the one that acts.
- **C-14, C-15 and C-16 are added,** for 18 contract cases.

| Mutation | Failing tests (run 2) |
|---|---|
| M1 | 3 |
| M2 | 2: `c9_os_packages_with_a_placeholder_root`, `transitive_edges_match_baseline` |
| M3 | 1 |
| M4 | 2 |
| M5 | 3, including C-6 |
| M6 (original FR-002) | 8, including C-14 and C-15 |
| M7 | 2 |
| M8 | 4, including C-14 and `a_nested_file_with_no_owner_of_its_own_walks_up_to_the_scan_root_project` |
| M9 | 2 |
| M10 | **0: equivalent**, as in run 1 |
| M11 | 0 in the full run. That run predated C-16. With C-16 added, M11 was re-applied, C-16 was run, and the file was restored by `cp`: C-16 fails, and the root reaches only `tools`, with `a` and `b` stranded. That is #1566's shape, with a project instead of a submodule. |
| M12 | 1: `only_pypi_requirements_declarations_count` |
| M13 | 11 C-cases |
| M14 | 1: C-6 |
| M15 | 3: C-15, `s1537_via_becomes_exact_purl_edges_between_pins_of_one_file`, and one other `s1537` reader test |
| M16 | 3: C-15, `a_compiled_file_gives_its_project_only_the_pins_no_sibling_reaches`, and one other `declared_attach` test |

**Why M11 survived the full run without C-16:**
- Every remaining case that needs a declared root anchor has a root whose fallback gate is open.
- A submodule anchor and a supplement anchor are anchors, not relationships, so they leave the gate open.
- The fallback then attaches the same packages as graph-tops.

C-16 closes the gate with a real edge, the retained nested project under `--root-name`. That is the only shape in which the anchor is the sole path.
