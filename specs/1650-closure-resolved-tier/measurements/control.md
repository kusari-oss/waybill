# Controls

## T002: `main`'s binary

`main` at `18691d8b5861a15a6634949e23031ee45445d29c`, built with `cargo build -p waybill` (debug profile). Before the build, `git diff 18691d8 -- waybill-cli waybill-common Cargo.toml Cargo.lock` was empty (0 lines). The binary reports `waybill 0.17.0`.

## T004, T007: the fixture cases before the change

`waybill-cli/tests/closure_resolved_tier.rs` on the branch before `retier` existed: **3 passed, 5 failed, 1 ignored**.

| Case | Result | Why |
|---|---|---|
| P2, R1, T1, B1 | red | the evaluated package and the declared boot libraries were `design` |
| P3 | red | on its `waybill-fixture-app` assertion; the unevaluated `waybill-fixture-extra` stayed `design`, as required |
| B2, B3, B4 | green | guards: `stm` already `build`, `Win32` `design`, the bootstrap `base` `build` |

After the change: **8 passed, 0 failed**, plus 4 unit tests (`s1650_*`).

## T012: N1 byte identity

`n1_control_byte_identical_to_main_without_the_closure`, run with `WAYBILL_TIER_MAIN_BIN` set to the T002 binary:
- **without `--nix-closure`:** pass, so all three documents are identical, version-4 UUIDs masked;
- **positive control** (`WAYBILL_TIER_EXPECT_DIFF=1`, with `--nix-closure --root-name`): pass, so the documents differ, as they must.

## T010: I11 on `main`'s committed goldens (positive control)

With I11 registered and `waybill-cli/tests/fixtures/public_corpus/` still holding `main`'s goldens, `golden_matrix_is_evaluable_and_gated` fails:

```
4 undeclared or stale cell(s) in the golden matrix:
  I11 / nix-closure-moat: UndeclaredFailure — closure-resolved-but-design x20 (e.g. pkg:hackage/array@0.5.8.0); root-direct-dependencies-all-design x2 (e.g. pkg:hackage/moat@0.1, pkg:hackage/readme@0.1.0.0)
  I11 / nix-closure-openai: UndeclaredFailure — closure-resolved-but-design x22 (e.g. pkg:hackage/array@0.5.6.0); root-direct-dependencies-all-design x1 (e.g. pkg:hackage/openai@2.5.4)
  I11 / nix-closure-slack-web: UndeclaredFailure — closure-resolved-but-design x24 (e.g. pkg:hackage/array@0.5.8.0); root-direct-dependencies-all-design x1 (e.g. pkg:hackage/slack-web@2.4.0.0)
  I11 / nix-closure-slacklinker: UndeclaredFailure — closure-resolved-but-design x24 (e.g. pkg:hackage/array@0.5.8.0); root-direct-dependencies-all-design x1 (e.g. pkg:hackage/slacklinker@0.0.0)
```

The applicability line reads `I11: 5/20`. shellcheck is applicable and passes.

**The counts equal the census** (`census.md`): moat's 20 are 19 C218 components plus `moat` (C203), and the other three are 21 + 1, 23 + 1 and 23 + 1. Clause (b) fires on all four.

**The local gate stays red from here until T014 installs the regenerated goldens.** These cells must not be declared.
