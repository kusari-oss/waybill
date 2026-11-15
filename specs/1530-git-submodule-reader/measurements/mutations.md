# Mutations (spec 1530)

Each mutation was applied to the source, the named tests were run, and the file was restored with `cp` from a pre-mutation copy (verified with `cmp`). Every mutation must fail at least one test, for the reason the test exists.

## US1 (task T024)

| # | Mutation | Failing test | Failure message |
|---|---|---|---|
| M1 | `read_gitlinks` runs `ls-files --stage -z`, with the index-reading guard emptied and `-c core.fsmonitor=false` removed from `base_args` | `sc010_repository_config_runs_nothing` | "a program the scanned repository configured ran during the scan" |
| M2 | The commit is read from the submodule checkout (`head_commit`), not the gitlink | `sc001_fresh_clone_pins_the_gitlink_commit` | version mismatch on the fresh clone, where the submodule is uninitialised |
| M3 | `sanitize_userinfo` skipped | `sc004_credentials_never_emitted` | "cdx carries the token" |
| M4 | The reader root anchors are not installed in `scan_cmd.rs` | `root_with_other_edges_still_depends_on_the_submodule` | "the root depends on the submodule; got [\"pkg:pypi/waybill-fixture-req@unresolved\"]" |

## Two first attempts that did not test what they claimed

- **M1, first form.** The mutated argument list was `["-c", "core.fsmonitor=", "ls-files", …]`. The empty `core.fsmonitor=` override itself disables fsmonitor, so the payload never ran. The tests failed only because `ls-tree`'s parser rejects `ls-files` output. Re-run as above without any override, the SC-010 marker assertion fires.
- **M4, first form.** No US1 fixture had a root with any other edge, so the emitters' "root depends on every parentless component" fallback attached the submodule and the anchor was redundant. That is #1009's trap. `root_with_other_edges_still_depends_on_the_submodule` was added, with a `pyproject.toml` main module and one dependency, plus a control asserting that edge exists. M4 now fails it.
