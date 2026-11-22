# T016: mutations (SC-006)

`measurements/mutate.py` applies each mutation as one exact replacement, runs its killing test, and restores the file with `cp`. The binary was rebuilt after the last one. A `^error[E` line would count as a compile error, not a kill; none occurred.

**Result: 8 of 8 killed.**

| id | mutation | killed by |
|---|---|---|
| M1 | drop R2's source-path swap | `o3_a_merged_main_module_keeps_its_own_directory` |
| M2 | apply R2 to installed entries too | `o8_an_installed_entry_keeps_its_path_first` |
| M3 | drop the lockfile path from the merged entry | `o3_a_merged_main_module_keeps_its_own_directory` |
| M4 | drop FR-003's non-runtime key | unit `s1642_a_runtime_candidate_beats_a_non_runtime_one` |
| M5 | drop the anchor seeds | `a1_a_root_declared_package_is_runtime` |
| M6 | drop the declaring-file filter (every requirements anchor a seed) | `a3_a_dev_requirements_anchor_is_not_a_seed` |
| M7 | drop the anchor's own-scope filter | unit `s1642_a_non_runtime_anchor_is_not_a_seed` |
| M8 | own a directory by the first source path only (no `project_path`) | unit `s1642_a_main_module_owns_the_directory_its_paths_name` |

## M8: why only a unit test kills it

M8 first survived O-4, then two versions of O-13. Probing it by hand, with the census events, showed why:
- The owner index is built **twice** per scan. Requirements edges come from the first build, before the deduplicator merges same-PURL entries. There a main module's paths are unsorted, and R2 puts its manifest's directory first.
- Spec 1542's sort (`deduplicator.rs`) applies only to the merged component, which the second build sees. That build decides root anchors (`declared_root_anchors`), and CMake fetch, link and search anchors.
- The sort compares raw paths. `path+file://…/tool/devtool` sorts before `tool/uv.lock` but after `app/uv.lock`, so the lockfile is first only under some directory names.
- In every tree I built (O-4; O-13 under `tool/`, under `app/`, and with a manifest-less subdirectory; O-14), the second build's ownership does not change what is anchored. So M8 produced the same document.

`project_path` therefore guards the post-dedup index, and its unit test observes it directly. O-13 and O-14 are kept: each fails on `main` (the dev package owns the directory) and passes on the branch.

## Integration cases added at T016

- **O-13:** a merged dev package under `app/`, with a requirements file in a manifest-less subdirectory.
- **O-14:** a lockfile-only directory. On `main` its requirements attach to the dev package; on the branch they are anchored to the root.

Both were checked against `main`'s binary: each fails there on its stated assertion.
