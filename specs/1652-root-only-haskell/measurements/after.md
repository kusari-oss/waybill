# Spec 1652: after the change

**Binaries:**
- `main` at `317fbae6`, built from a `git worktree` at `main`.
- The branch, built from the working tree.

Both builds are debug builds, and every scan is `--offline` with an isolated `HOME`.

## T023: nothing else changes (FR-007, SC-004)

**What was scanned:** every fixture under `waybill-cli/tests/fixtures/project_discovery_haskell/` (f1–f6), by both binaries, in all three formats. Three ways:
- the default `all`;
- `--split=workspace`;
- `--split=directory`.

**How it was compared:** both trees were masked the same way, UUIDs to `<UUID>` and RFC 3339 timestamps to `<TS>`, rewritten with sorted keys, and compared with `diff -r`.

| | files | differing |
|---|---|---|
| `main` | 144 | — |
| branch | 144 | **0** |

**Positive control:** the same pipeline on f1 under `--project-discovery=root-only` reports 12 differing lines. These are:
- C104 `complete` → `partial`;
- the new C105 reason;
- C244;
- C200's new key.

So a zero above means equal, not unexamined.

## T027: mutations (SC-006)

**How each mutation was run:**
1. Applied from a `cp` backup of the one file it touches.
2. Ran `cargo test -p waybill --test project_discovery_haskell`, plus the unit tests named in the table.
3. Restored with `cp`, then rebuilt.

The harness is `mut/run.py`, and its definitions are `mut/muts.json`, both in the scratchpad (not committed). Results:

| # | Mutation | Killed | Integration tests that fail | Unit tests that fail |
|---|---|---|---|---|
| M1 | `root_dir` returns `source_dir` | yes | 8, incl. `us1_f2_root_only_keeps_chain_drops_independent`, `us1_f2_strict_keeps_chain`, `us1_unlisted_manifest_name_is_root_level`, `us2_f5_stack_yaml_sets_scope` | `a_root_package_yaml_is_root_level`, `root_dir_strips_a_*_manifest_file` |
| M2 | `split::project_for_root` skips nested main modules (never committed) | yes | `us1_f2_root_only_keeps_chain_drops_independent`, `us1_f2_strict_keeps_chain`, `us1_f6_npm_nested_by_path_kept`, `us3_f2_drop_one`, `us3_f6_no_drop_no_record` | `cycle_between_nested_main_modules_terminates_and_keeps_both`, `dropped_set_excludes_main_modules_reached_through_edges`, … |
| M3 | declared roots not added | yes | `us2_f3_*`, `us2_f4_*`, `us2_f5_*`, `us3_f3_unlisted_package_recorded` | `declared_members_are_roots_under_root_only` |
| M3a | `cabal.project` resolution removed | yes | `us2_f3_*`, `us2_f4_*`, `us3_f3_*` | 8 resolver tests |
| M3b | `stack.yaml` resolution removed | yes | `us2_f5_stack_yaml_sets_scope` | 4 resolver tests |
| M4 | declared roots applied under `strict` | yes | `us2_strict_ignores_declarations` | `strict_ignores_declared_members` |
| M5 | `root_dir` special-cased to `package.yaml` / `*.cabal` | yes | `us1_unlisted_manifest_name_is_root_level` | — |
| M6 | dropped set left empty | yes | `us2_strict_ignores_declarations`, `us3_f1_positive_control_records_drop`, `us3_f2_drop_one`, `us3_f3_*`, `us3_symlinked_root_c200_matches` | 7 filter tests |
| M7a | reason code not pushed | yes | `us3_f1_positive_control_records_drop` | 3 `s1652_scope_drop_tests` |
| M7b | verdict not made `partial` | yes | `us3_f1_positive_control_records_drop` | 2 `s1652_scope_drop_tests` |
| M8 | `compositions` arm removed | yes | — (no fixture's dropped ecosystem is otherwise complete) | `a_scope_dropped_ecosystem_is_degraded` |
| M9 | C200 key not inserted | yes | `us3_f1_positive_control_records_drop`, `us3_symlinked_root_c200_matches` | `amend_lists_the_dropped_subset_sorted_and_keeps_read` |
| M10 | C244 not emitted in SPDX 3 only | yes | 4, through `waybill sbom parity-check`, which every scan runs | — |
| M11a | C200 matched on the raw root only | yes | `us3_symlinked_root_c200_matches` | — |
| M11b | C200 matched on the canonical root only | **no** | — | — |

**M11b survived,** which showed that the raw-root candidate never matched anything: the reader joins C200's paths to the canonical root. The dead half was removed, so the code now matches on the canonical root only. M11a was re-run on the simplified code (raw root only) and is still killed by `us3_symlinked_root_c200_matches`.

**On the spec's "two paths" for the reason code:**
- The plan placed the reason on two classifier paths, the empty set and the classified set.
- As built, one post-step, `apply_scope_drop`, serves both.
- M7a and M7b mutate its two effects, the reason and the verdict.
- `scope_drop_on_an_empty_set_is_partial` and `scope_drop_on_a_complete_non_empty_graph_is_partial` cover the two inputs.

## T027 (continued): the two rules added after the corpus run

| # | Mutation | Killed | Integration test that fails | Unit test that fails |
|---|---|---|---|---|
| M12 | root directory not seeded for a declaration-only scope | yes | `us2_declaration_only_scope_keeps_root_level_components` | `declaration_only_scope_keeps_root_directory_components` |
| M13 | unattributable components dropped | yes | `us2_unattributable_snapshot_component_is_kept` | `unattributable_components_are_kept_attributable_ones_are_not` |

## T028: the corpus after (SC-005)

**What was run:** the 12 targets of `baseline.md`, at the same pins, plus `hpack-ogmios-with-root-package-json`. That is ogmios with a root `{"name":"waybill-fixture-root-js","version":"1.0.0"}` added, the case FR-001 asks for when a target has neither a root package nor a root declaration. The branch binary, `--offline`, with `count.py`. Rows for `all` are omitted: they are byte-identical to `baseline.md`'s (T023).

| target | mode | components | main modules (nested) | hackage | completeness | C244 count | log |
|---|---|---|---|---|---|---|---|
| go-cobra | root-only | 7 | 0 (0) | 0 | partial | - | root_main_modules=1 workspace_members_followed=2 nested_projects_ignored=0 |
| go-cobra | strict | 7 | 0 (0) | 0 | partial | - | root_main_modules=1 workspace_members_followed=2 nested_projects_ignored=0 |
| go-opentelemetry | root-only | 14 | 4 (3) | 0 | partial | 25 | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=25 |
| go-opentelemetry | strict | 14 | 4 (3) | 0 | partial | 25 | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=25 |
| haskell-aeson | root-only | 63 | 6 (5) | 61 | partial | - | root_main_modules=6 workspace_members_followed=0 nested_projects_ignored=0 |
| haskell-aeson | strict | 53 | 3 (2) | 51 | partial | 3 | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=3 |
| haskell-language-server | root-only | 167 | 6 (5) | 128 | partial | - | root_main_modules=6 workspace_members_followed=0 nested_projects_ignored=0 |
| haskell-language-server | strict | 167 | 6 (5) | 128 | partial | - | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=0 |
| haskell-security-advisories | root-only | 62 | 6 (6) | 59 | partial | - | root_main_modules=6 workspace_members_followed=3 nested_projects_ignored=0 |
| haskell-security-advisories | strict | 62 | 6 (6) | 59 | partial | - | FALLBACK |
| hpack-hs-opentelemetry | root-only | 155 | 37 (37) | 127 | partial | - | root_main_modules=37 workspace_members_followed=20 nested_projects_ignored=0 |
| hpack-hs-opentelemetry | strict | 155 | 37 (37) | 127 | partial | - | FALLBACK |
| hpack-hs-temporal-sdk | root-only | 88 | 6 (6) | 70 | partial | 2 | root_main_modules=6 workspace_members_followed=12 nested_projects_ignored=2 |
| hpack-hs-temporal-sdk | strict | 445 | 8 (8) | 73 | partial | - | FALLBACK |
| hpack-lootbox | root-only | 63 | 7 (7) | 62 | partial | - | root_main_modules=7 workspace_members_followed=0 nested_projects_ignored=0 |
| hpack-lootbox | strict | 63 | 7 (7) | 62 | partial | - | FALLBACK |
| hpack-lootbox-no-generated-cabal | root-only | 63 | 7 (7) | 62 | partial | - | root_main_modules=7 workspace_members_followed=0 nested_projects_ignored=0 |
| hpack-lootbox-no-generated-cabal | strict | 63 | 7 (7) | 62 | partial | - | FALLBACK |
| hpack-ogmios | root-only | 1149 | 13 (13) | 103 | partial | - | FALLBACK |
| hpack-ogmios | strict | 1149 | 13 (13) | 103 | partial | - | FALLBACK |
| hpack-ogmios-with-root-package-json | root-only | 11 | 0 (0) | 0 | partial | 13 | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=13 |
| hpack-ogmios-with-root-package-json | strict | 11 | 0 (0) | 0 | partial | 13 | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=13 |
| npm-express | root-only | 369 | 0 (0) | 0 | complete | - | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=0 |
| npm-express | strict | 369 | 0 (0) | 0 | complete | - | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=0 |
| rust-ripgrep | root-only | 68 | 10 (0) | 0 | complete | - | root_main_modules=10 workspace_members_followed=0 nested_projects_ignored=0 |
| rust-ripgrep | strict | 68 | 10 (0) | 0 | complete | - | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=0 |

**The prediction was met.** Under `root-only`, 7 of 8 Haskell targets stop falling back. ogmios, with no root package and no root declaration, still falls back. With a root `package.json` added, ogmios drops all 13 of its nested Haskell main modules: on `main` that read `complete`, and now it reads `partial`, with C244 count 13.

**`root-only` now matches `all`** on haskell-language-server (167), haskell-security-advisories (62), hpack-hs-opentelemetry (155) and both lootbox targets (63). Each repository's root declaration names every package it has.

**What the first corpus run found, and what changed because of it.** The run before T032 and T033 lost root-level components that no main module owns:

| target | lost | why | fix |
|---|---|---|---|
| haskell-security-advisories | 3 `flake.lock` inputs (`["."]`) | a declaration-only scope seeded no root directory | T032 |
| hpack-hs-opentelemetry | 20 devenv/flake inputs, the Stack snapshot, and a generic `unknown@…` | both reasons | T032, T033 |
| haskell-language-server | `pkg:generic/ghc-9.6.5`, `pkg:generic/stackage-lts-23.28` | no provenance and no edges | T033 |
| hpack-lootbox (both) | `pkg:generic/snapshot.yaml` | no provenance and no edges | T033 |

None of these were recorded anywhere, which is the failure this spec exists to remove. Before spec 1652 the fall-back hid them on Haskell repositories.

**The remaining drops are named by C244, and each follows from the repository's own declaration:**
- **go-opentelemetry, count 25:** 28 nested modules, 3 of which the root reaches. The log said 28 on `main`.
- **haskell-aeson under `strict`, count 3:** `aeson-benchmarks`, `aeson-examples`, `attoparsec-aeson`. The root package reaches the other two of its five nested packages. Under `root-only`, the six `packages:` lines of `cabal.project` make all six roots.
- **hpack-hs-temporal-sdk, count 2:**
  - `pkg:hackage/tix-to-markdown`, under `tools/`, two levels deep, which `packages: */*.cabal` does not name;
  - `pkg:cargo/temporal_bridge`, the nested Rust project in `core/rust`, which no Haskell package has an edge to.

  357 components leave with them: 354 `pkg:cargo` (the Rust project's crates) and 3 `pkg:hackage` (the tool and its own dependencies).
