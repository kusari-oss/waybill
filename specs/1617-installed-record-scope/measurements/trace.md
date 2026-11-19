# Trace

## T001: premises at `main` (`857e542e`)

A throwaway `eprintln!` at the top of `record_scopes`'s loop printed each paired record's evidence and pairings. The binary was run on hand-built trees, then the source was restored with `cp` (verified with `cmp`) and the binary rebuilt.

- **(a) Holds: static archives are in a record's evidence before the rule runs.** On an `absl`-shaped tree (a version file, an exported-targets `*Exports.cmake` raising `_IMPORT_PREFIX` three times, and a `-release` file naming `${_IMPORT_PREFIX}/lib/libwaybill-fixture-e-core.a`), the record's evidence at `record_scopes` is `["…/WaybillFixtureEConfigVersion.cmake", "sdk/lib/libwaybill-fixture-e-core.a", "sdk/share/c/consumer-config.cmake"]`. Shared libraries arrive the same way, through `resolve`; I-1's and I-10's evidence controls assert them on the emitted documents.
- **(b) Holds: a test-directory search pairs as `(conditional: true, conditions: {})`.** That was `waybill_fixture_bare`, searched only from `sdk/share/x/tests/a/CMakeLists.txt`. Conditional with no conditions is why such a record is Optional with no C236 on `main`.
- **(c) Holds: a record with no library file searched under a condition is Optional with C236.** I-6 passes on `main`, and spec 1614's W-14b asserts the same.

## T005 / T014: I-11 corrected

The contract's first I-11 expected spec 1533 to Test-scope a shipped record under `share/<pkg>/tests/` by directory. It failed on `main`, with no scope. Spec 1533 scopes an installed-library **record** through a declaration (exported targets of a test package, a name ending `-tests`, `_tests`, `-test` or `_test`, per `installed_lib::is_test_package`), and scopes binaries and file-tier components by directory (`installed_test_scope.rs`). I-11 now guards the record shape spec 1533 does handle: a `waybill_fixture_p_tests` package with exported targets, which is Test with C62 `exported-targets-test-package`. It passes on `main`.

That is also `rocprofiler-sdk-tests`'s shape on the SDK. On `main`, spec 1614's search half made it Optional before spec 1533 ran, so spec 1533 skipped it.

## Red at `main`, before any source change

| Case | Fails on |
|---|---|
| I-1, I-2, I-4 | scope `excluded` (expected required) |
| I-5 | scope `optional` (expected `test`) |
| I-9 | scope, after the evidence control passed |
| I-10 | C122 `cmake-conditional` (expected `cmake-conditional-link`) |

Guards passing on `main`: I-3, I-6, I-7, I-8, I-11, I-rep, and I-ctl, whose golden was generated at this point.
