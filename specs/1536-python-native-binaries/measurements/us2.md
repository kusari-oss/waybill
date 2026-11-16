# US2: OS-owned binaries unchanged (T012, T013)

The T012 tests are in `waybill-cli/tests/python_owned_binaries.rs`, prefixed `us2_`. All five passed on their first run, so T013's fix-it-in-the-code branch was not needed. Each test pairs the case with a control that shows the suppressed signal does fire without the claim, so a pass is not an empty result:

| Test | Case | Control |
|---|---|---|
| `us2_os_claim_wins_over_record` | dpkg and `RECORD` list one ELF: no binary, no zlib, no C230 | dpkg lists another file: zlib nested, C230 `{"analysed":1,"identified":1}` |
| `us2_ipk_claim_wins_over_record` | an `.ipk` installs the path: no zlib, no C230 | the `.ipk` installs another path: zlib reported |
| `us2_cpython_lib_dynload_still_collapses` | a `lib-dynload` module beside an owned package still folds into `pkg:generic/cpython@3.12` | — |
| `us2_owned_go_binary_gets_no_second_diagnostic_component` | an owned Go binary with unsupported build info is one component, nested | left out of `RECORD`: two components (file-level plus the `go_binary` diagnostic) |
| `us2_unowned_binary_linking_a_pip_library_reports_linkage` | a binary the package does not own linking a pip-installed `usr/lib` library gets a linkage component (research R1, accepted) | — |
