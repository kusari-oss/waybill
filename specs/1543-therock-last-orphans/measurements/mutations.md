# Mutations (T024)

`mutate.py` holds 12 mutations, one per rule; `mutate-run.sh` applies each, runs the spec's tests (`--bin waybill -- s1543 cmake::links bun_lock`, and `--test scan_therock_last_orphans` W-1..W-8 and L-1..L-12), restores the touched file with `cp`, and checks the tree is clean against the commit. Run on `75dd0922`, 2026-11-17. Every file was restored clean.

The runner's `compile_errors` column counts cargo's `error: test failed` lines; no mutation failed to compile.

| id | rule | tests that failed |
|---|---|---|
| M1 | R1: no workspace-root anchors | W-1, W-2, W-3, W-7 |
| M2 | R2: the subject rung never fires | W-4, W-5, W-6, `s1543_one_workspace_root_at_the_scan_root_is_the_subject` |
| M3 | R2: the rung reports C69 `repo-root-main-module` | W-4, W-5, the unit test (W-6 is not expected to: it emits no C69) |
| M4 | R3: an included file starts with no conditions | L-7, `links::an_include_carries_its_site_conditions_and_its_own_directories` |
| M5 | R4: the prefix test removed | L-5 |
| M6 | R4: the name/file test removed | L-6 |
| M7 | R6: a DVC pointer is not the library | L-4 |
| M8 | R5: the owner lookup forced to the root | **survived** on `75dd0922`; L-8 after strengthening (below) |
| M9 | R5: one unconditional link no longer keeps the scope | L-2, L-9 |
| M10 | R4: the outermost prefix wins | L-12 |
| M11 | R1: bun members back to bare names | W-2, the two `bun_lock` unit tests |
| M12 | R5: the Optional scope never applied | L-1, L-3, L-4, L-7, L-8, L-9, L-10, L-11, L-12 |

## M8

L-8 asserted the owner's ref "contains `x`" — copied from spec 1538's C-14. The document root's ref, `waybill-fixture-root@1`, contains an `x` too ("fi**x**ture"), so a root-owned edge passed. L-8 now asserts the owner is not the subject and is a component with `waybill:source-mechanism` = `git-submodule`. Re-run with M8 applied: L-8 fails with `the root does not own the link` (left: `waybill-fixture-root@1`); restored clean.

(C-14 in `cmake_nested_fetches.rs` makes the same check, but also asserts the root's own edges do not include the fetch, so it does not share the weakness.)
