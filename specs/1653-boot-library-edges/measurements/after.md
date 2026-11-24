# Spec 1653: after the change

## T014: nothing changes without `--nix-closure` (FR-007, SC-005)

**What was scanned:** milestone 966's flake-pinned Haskell project (`tests/fixtures/nix_closure_boot_versions/project`).
- **Binaries:** `main` at `91d84d1` (built in a worktree) and this branch.
- **How:** through the same stub `nix`, using `NixStub`'s binary override from a temporary, uncommitted test, in three formats.
- **Masking:** UUIDs and timestamps, with sorted keys; then `diff -r`.

| case | differing lines |
|---|---|
| without `--nix-closure` | **0** |
| with `--nix-closure` (positive control) | 116 |

**The 116 lines are only:**
- the new `dependsOn` entries (`process` and `stm` → `base`);
- four C245 properties;
- C105, gaining `boot-library-edges-incomplete: 4 hackage boot librar(ies), 4 with dependencies not in the document, 0 without dependency data` after the existing `transitive-edges-unresolvable: hackage`.

**Not demonstrated:** the non-closure document carries no C218 at all. So it does not exercise the `matches!(nix_closure, Some(Ok(_)))` gate in `scan_cmd`, which stops the pass if milestone 926 ever stamps C218 without the closure. No input available here produces that, and the gate is kept as defence.
