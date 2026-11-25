# Offline review (T010): the 38 candidates

The grep heuristic (`scan-without-offline.txt`) listed 38 files with a scan and no `--offline`. Each file's test binary was run twice by `network_probe.sh`: normally, and with outbound HTTP(S) sent to an unreachable proxy (localhost exempt). The disposition below comes from that measurement plus reading the file.

| file | normal | blocked | result when blocked | disposition |
|---|---|---|---|---|
| `binary_build_provenance` | 5.7s | 5.4s | ok. 1 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `binary_id_enrich` | 5.5s | 5.5s | ok. 3 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `binary_source_binding_cmake` | 0.2s | 6.1s | ok. 3 passed; 0 failed; 0 ignored | unchanged: no deps.dev cost; the blocked run was slower (0.2 s → 6.1 s), single sample, not investigated |
| `binary_source_binding_regression` | 0.0s | 0.0s | ok. 4 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `closure_binary_contents` | 1.5s | 1.3s | ok. 13 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `closure_component_cpes` | 1.2s | 1.1s | ok. 14 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `closure_dep_fields` | 1.3s | 1.1s | ok. 17 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `closure_ecosystem_identity` | 1.3s | 1.3s | ok. 9 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `closure_edges` | 0.8s | 0.7s | ok. 17 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `closure_members` | 1.4s | 1.2s | ok. 25 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `closure_role_scope` | 2.1s | 2.0s | ok. 15 passed; 0 failed; 4 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `closure_structured_attrs` | 1.3s | 1.1s | ok. 15 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `cpe_binary_id` | 5.4s | 5.5s | ok. 1 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `exclude_path_help_text` | 0.0s | 0.0s | ok. 1 passed; 0 failed; 0 ignored | unchanged: help text only, no scan |
| `file_tier_vcs_skip` | 0.2s | 0.2s | ok. 3 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `fingerprints_v1_regression` | 0.0s | 0.0s | ok. 2 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `fingerprints_v2_e2e` | 0.0s | 0.0s | ok. 3 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `go_proxy_fetch_bounds` | 1.0s | 1.0s | ok. 6 passed; 0 failed; 0 ignored | unchanged: enrichment-style test, already mocked (`wiremock`) |
| `nix_degradation_detail` | 0.5s | 0.5s | ok. 6 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `nixpkgs_declarations` | 37.6s | 47.7s | ok. 5 passed; 0 failed; 0 ignored | unchanged: 37.6 s either way, so its cost is not deps.dev (slower blocked, 47.7 s; not investigated here) |
| `no_binary_scan_us2_help` | 0.0s | 0.0s | ok. 1 passed; 0 failed; 0 ignored | unchanged: help text only, no scan |
| `oci_pull_backward_compat` | 0.1s | 0.1s | ok. 2 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `oci_pull_custom_ca` | 0.0s | 0.0s | ok. 3 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `oci_pull_flag_composition` | 0.1s | 0.1s | ok. 2 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `oci_pull_plain_http` | 0.1s | 0.1s | ok. 3 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `oci_pull_skip_verify` | 0.0s | 0.0s | ok. 3 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `oci_referrers_backward_compat` | 0.1s | 0.1s | ok. 4 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `oci_referrers_either_mode` | 0.1s | 0.1s | ok. 4 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `oci_referrers_strict_mode` | 0.0s | 0.0s | ok. 4 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `ruby_built_in_gems` | 0.1s | 0.1s | ok. 2 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `scan_binary` | 3.4s | 3.4s | ok. 25 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `scan_fingerprint_corpus_external` | 0.0s | 0.0s | ok. 1 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `scan_image_docker_daemon` | 3.7s | 3.3s | ok. 2 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `scan_rpm_file` | 0.2s | 0.2s | ok. 5 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `scan_split_basic` | 58.7s | 0.2s | ok. 9 passed; 0 failed; 0 ignored | **changed: `--offline` in `run_split_scan`**. It waited on live deps.dev (58.7 s → 0.2 s blocked); not an enrichment test |
| `scan_windows_smoke` | 0.0s | 0.0s | ok. 0 passed; 0 failed; 0 ignored | unchanged: 0 tests on this host (Windows only) |
| `scope_exemption_record` | 0.2s | 0.1s | ok. 3 passed; 0 failed; 0 ignored | unchanged: no network cost measured (normal ≈ blocked), and passes blocked |
| `split_manifest_schema` | 13.0s | 0.1s | ok. 1 passed; 0 failed; 0 ignored | **changed: `--offline`**. It waited on live deps.dev (13.0 s → 0.1 s blocked); not an enrichment test |

- **All 38 pass with the network blocked.** None depends on live deps.dev for its assertions. So changing the two files' input to `--offline` cannot change what they assert, and the after-runs below confirm they pass.
- **The closure test files** show no network cost (normal ≈ blocked), so they are left unchanged. `closure_binary_contents` passes `--no-deps-dev` (read). The others were not read flag by flag; the measurement is the evidence. None of them may gain `--offline`, which refuses the closure tier.
