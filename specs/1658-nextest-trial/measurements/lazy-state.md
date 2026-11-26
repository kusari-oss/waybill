# Lazy-state check (T007, FR-007)

Test files holding `OnceLock`, `LazyLock` or `lazy_static` under `waybill-cli/tests`: 12 (`lazy-files.txt`). The spec's 16 came from a grep that also matched non-`.rs` files. Under nextest each test is its own process, so lazy setup runs once per test instead of once per binary.

- **cargo-test binary wall**: the binary's `finished in`, min / median / max over 3 runs.
- **nextest summed**: the median sum of per-test durations. It is CPU-side cost and shows repeated setup; it is not in the same units as the binary wall, because `cargo test` runs one binary's tests on threads.
- **nextest slowest test**: what the binary contributes to wall time under nextest, min / median / max.
- **regresses**: the slowest nextest test's median exceeds the cargo-test median by more than both runners' spreads.

## Files with their own lazy state

| binary | cargo-test binary wall (s) | nextest summed (s) | nextest slowest test (s) | regresses |
|---|---|---|---|---|
| `gemfile_main_module` | 0.13 / 0.14 / 0.17 | 1.72 | 0.33 / 0.35 / 0.40 | **yes** |
| `mapping_doc_bidirectional` | 0.57 / 0.58 / 0.62 | 1.69 | 1.09 / 1.10 / 1.13 | **yes** |
| `openvex_sidecar` | 0.05 / 0.05 / 0.05 | 0.54 | 0.06 / 0.06 / 0.07 | **yes** |
| `openvex_sidecar_path` | 1.33 / 1.46 / 1.49 | 2.88 | 1.41 / 1.42 / 1.43 | no |
| `sbom_type_signaling` | 1.17 / 1.18 / 1.18 | 3.84 | 2.02 / 2.29 / 2.50 | **yes** |
| `sbom_user_metadata` | 1.21 / 1.22 / 1.24 | 5.26 | 1.86 / 2.19 / 2.37 | **yes** |
| `spdx3_schema_validation` | 0.19 / 0.19 / 0.19 | 2.39 | 0.25 / 0.27 / 0.28 | **yes** |
| `spdx_schema_validation` | 0.05 / 0.05 / 0.05 | 0.57 | 0.06 / 0.07 / 0.07 | **yes** |
| `split_manifest_schema` | 0.06 / 0.06 / 0.06 | 0.10 | 0.10 / 0.10 / 0.11 | **yes** |

## Binaries including a shared lazy helper (`common/cdx_schema`, `sbom_edit_support`, `corpus_harness_195`)

| binary | cargo-test binary wall (s) | nextest summed (s) | nextest slowest test (s) | regresses |
|---|---|---|---|---|
| `also_detected_via` | 3.61 / 3.69 / 3.74 | 43.61 | 5.19 / 5.34 / 5.53 | **yes** |
| `cisa_2026_signing` | 0.43 / 0.44 / 0.86 | 4.03 | 0.45 / 0.45 / 0.46 | no |
| `closure_ecosystem_identity` | 1.31 / 1.34 / 1.34 | 7.62 | 1.34 / 1.35 / 1.48 | no |
| `closure_role_scope` | 2.18 / 2.19 / 2.20 | 11.34 | 2.12 / 2.22 / 2.42 | no |
| `closure_runtime_scope` | 3.17 / 3.18 / 3.25 | 33.83 | 2.07 / 2.12 / 2.14 | no |
| `closure_secondary_cpe` | 0.60 / 0.61 / 0.67 | 3.63 | 0.62 / 0.62 / 0.64 | no |
| `cmake_build_tree` | 1.02 / 1.05 / 1.51 | 8.76 | 2.02 / 2.20 / 2.25 | **yes** |
| `cmake_conditional_scope` | 5.50 / 5.94 / 7.03 | 128.05 | 4.30 / 4.50 / 4.50 | no |
| `cmake_declared_datasets` | 1.08 / 1.14 / 1.69 | 3.13 | 1.86 / 1.88 / 1.91 | **yes** |
| `cmake_project_components` | 2.36 / 2.62 / 3.15 | 20.40 | 5.30 / 5.43 / 5.57 | **yes** |
| `cmake_unresolved_github_fetch` | 1.65 / 1.68 / 1.97 | 15.80 | 2.72 / 2.79 / 2.80 | **yes** |
| `corpus_harness_self_tests` | 5.42 / 5.70 / 6.00 | 44.45 | 5.36 / 5.40 / 5.58 | no |
| `declared_url_credentials` | 5.34 / 6.43 / 7.84 | 114.08 | 4.33 / 4.39 / 4.84 | no |
| `installed_record_scope` | 1.73 / 1.81 / 2.00 | 8.25 | 3.78 / 3.80 / 3.94 | **yes** |
| `merged_binary_evidence` | 0.19 / 0.27 / 0.54 | 1.98 | 0.37 / 0.37 / 0.40 | no |
| `orphan_explanations` | 0.80 / 0.84 / 1.02 | 7.50 | 1.38 / 1.98 / 2.02 | **yes** |
| `pants_coursier_jvm_reader` | 0.12 / 0.12 / 0.15 | 1.78 | 0.19 / 0.26 / 0.28 | **yes** |
| `propagation_seeds` | 4.14 / 4.15 / 4.31 | 16.88 | 12.86 / 14.97 / 15.00 | **yes** |
| `public_corpus` | 0.01 / 0.01 / 0.01 | 0.10 | 0.02 / 0.02 / 0.02 | **yes** |
| `python_owned_binaries` | 1.54 / 1.60 / 1.71 | 15.56 | 1.89 / 2.09 / 2.11 | **yes** |
| `requirements_owner` | 5.35 / 5.78 / 5.83 | 81.15 | 7.61 / 8.30 / 8.65 | **yes** |
| `sbom_edit_chain` | 0.68 / 0.73 / 0.80 | 2.08 | 0.89 / 0.98 / 1.32 | no |
| `sbom_edit_filter` | 37.38 / 38.67 / 41.67 | 58.65 | 48.90 / 49.87 / 49.89 | **yes** |
| `sbom_edit_parity` | 0.14 / 0.14 / 0.14 | 0.33 | 0.12 / 0.14 / 0.16 | no |
| `sbom_edit_redact` | 1.38 / 1.38 / 1.42 | 3.03 | 2.35 / 2.49 / 3.21 | **yes** |
| `sbom_edit_retier` | 0.30 / 0.30 / 0.30 | 2.59 | 0.31 / 0.31 / 0.43 | no |
| `sbom_type_signaling` | 1.17 / 1.18 / 1.18 | 3.84 | 2.02 / 2.29 / 2.50 | **yes** |
| `sbom_user_metadata` | 1.21 / 1.22 / 1.24 | 5.26 | 1.86 / 2.19 / 2.37 | **yes** |
| `scan_cmake_tree_finds` | 2.20 / 2.23 / 2.57 | 18.49 | 8.28 / 9.57 / 9.59 | **yes** |
| `scan_determinism` | 2.69 / 2.79 / 3.82 | 45.27 | 6.12 / 7.03 / 7.07 | **yes** |
| `scan_dot_directory_manifests` | 1.46 / 1.47 / 1.75 | 10.25 | 5.67 / 6.39 / 6.46 | **yes** |
| `scan_nested_req_scope` | 1.85 / 1.86 / 2.17 | 16.24 | 8.95 / 10.24 / 10.51 | **yes** |
| `scan_therock_last_orphans` | 7.30 / 7.54 / 10.05 | 148.83 | 9.41 / 10.00 / 10.59 | no |
| `version_string_boundary` | 0.54 / 0.54 / 0.73 | 8.89 | 1.00 / 1.11 / 1.15 | **yes** |

**Binaries that regress: 29.**

## Control, and what this means

Every binary's tests run slower per test under nextest, because nextest runs the whole suite at once and the machine is loaded, so the "regresses" column above is mostly load, not lazy state. The control compares the same ratio, slowest nextest test over cargo-test binary wall, for binaries at or above 0.5 s:

| group | binaries | median ratio | p25 / p75 |
|---|---|---|---|
| lazy-state binaries | 30 | 1.66 | 1.02 / 2.10 |
| all other binaries (control) | 68 | 1.20 | 1.02 / 1.62 |

So lazy-state binaries pay somewhat more than the control, consistent with repeated setup, but none of them sets the suite's wall time: the slowest test overall is `sbom_edit_filter`'s at about 50 s, and the suite still finishes in 123–126 s against 334–354 s (`runs.tsv`). No test is changed for this (FR-007 asks for a list, not a fix). If a later slice wants the CPU back, `also_detected_via`, `requirements_owner`, `scan_determinism`, `declared_url_credentials`, `cmake_conditional_scope` and `scan_therock_last_orphans` have the largest nextest summed times.

Produced by `lazy_state.py` from the inputs `.direct` (files with their own lazy state) and `.users` (binaries that include a shared lazy helper).
