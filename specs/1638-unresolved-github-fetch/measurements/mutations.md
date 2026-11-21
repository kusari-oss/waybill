# T016: mutations

**Harness.** `mutate-run.sh` + `mutate.py`, run at `c7b1bfe` from the repository root. For each mutation:
- apply one exact-string replacement, whose anchor must occur exactly once;
- run `cargo test -p waybill --test cmake_unresolved_github_fetch`, plus the bin-target unit tests `s1638_unresolved_shapes`, `wrapper_recognition_is_unchanged…` and `a_versionless_github_cmake_fetch…`;
- restore the file with `cp` from `git show HEAD:<file>`, and check the restore with `cmp`.

The binary was rebuilt after the last restore. A kill counts only with zero `^error[E…]` lines, so a compile failure is never mistaken for a caught mutation. After the run, `git status --short waybill-cli/src` was empty.

| Mutation | Compile errors | Restored | Killed by |
|---|---|---|---|
| M1 the unresolved GitHub branch emits `pkg:generic` | 0 | yes | `s1638_unresolved_shapes`, `wrapper_recognition_is_unchanged…`, U-1, U-2, U-3, U-4, U-5, U-6, U-12, U-14, U-15, U-xhost |
| M2 drop the repository-has-no-variable check | 0 | yes | `s1638_unresolved_shapes`, U-9, U-ctl |
| M3 no `download_url` on the literal-repository shapes | 0 | yes | `s1638_unresolved_shapes`, U-1, U-2, U-3, U-4, U-5, U-6, U-7, U-8, U-15, U-xhost |
| M4 remove the wrapper-recognition mask (research R2) | 0 | yes | `wrapper_recognition_is_unchanged…`, U-3, U-6, U-ctl, U-xhost |
| M5 remove the reconciler guard (research R5) | 0 | yes | `a_versionless_github_cmake_fetch_is_never_absorbed`, U-13b |
| M6 an unresolved URL fetch gets `download_url` | 0 | yes | `s1638_unresolved_shapes`, U-10, U-ctl |

**Every mutation is killed.** Two kills show the case is a guard, not just a failing-first test:
- **M4 kills U-ctl and U-6:** without the mask, a wrapper forwarding a literal tag changes from `main`'s output, the regression research R2 predicted.
- **M5 kills U-13b:** the same result as the T011 probe.
