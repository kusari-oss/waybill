# Control: `main`'s binary and the red/green baseline

## T001: the control binary

`main` at `ea14906535e5c8261d1959e19fa20de1ee09d308`, built with `cargo build -p waybill` (debug profile, the same profile as every branch binary measured here). Before the build, `git diff ea149065 -- waybill-cli waybill-common Cargo.toml Cargo.lock` was empty (0 lines). The binary reports `waybill 0.17.0`.

## T008, T012, T015: the cases on `main`'s code

`waybill-cli/tests/hpack_include_merge.rs` was run on the branch before any reader change: **3 passed, 14 failed, 1 ignored** (the control).

| Case | On `main` | Why |
|---|---|---|
| M1, M2, M3 | red | the merge is never applied; M2's repeated `<<` makes the manifest unread |
| M4 | **green** | as-built: `main` ignores the merge entirely, so the explicit `[text]` survives in both orders. M4 is a guard against a wrong fold, not a red case (tasks.md said M1–M4 red) |
| M5 | green | guard: a merge of a sequence contributes nothing either way |
| I1–I6 | red | `unknown anchor`, or the include dropped |
| U1–U5 | red | no `includes-unresolved` in C200 |
| N1 | green | guard |

## After the change

17 passed, 0 failed, plus the control:

| Run | Cases | Result |
|---|---|---|
| `control_byte_identical_to_main` | N1, M4, M5 | **pass**: all three documents identical to `main`'s, version-4 UUIDs masked |
| positive control | M1 | **fails**, as it must: `M1: cdx differs` |
