# Contract: MIKEBOM_* → WAYBILL_* environment variable migration mapping

**Feature**: 214-rename-to-waybill
**Kind**: User-facing migration contract (drop-in text-substitution reference)
**Consumers**: operators updating CI scripts, Makefiles, docker-compose files, or dev-env shell profiles that set project env vars.

## Migration rule

**Mechanical prefix swap**. For any environment variable whose name matches `^MIKEBOM_(.+)$`, the post-rename name is `WAYBILL_\1`. Every suffix is preserved verbatim (case, underscores, terminal characters). The mapping is 1:1 with zero exceptions.

## Enumeration (73 variables surveyed 2026-07-21)

| Pre-rename                                          | Post-rename                                          |
|---|---|
| `MIKEBOM_BIN`                                       | `WAYBILL_BIN`                                        |
| `MIKEBOM_CARGO_METADATA_TIMEOUT_SECS`               | `WAYBILL_CARGO_METADATA_TIMEOUT_SECS`                |
| `MIKEBOM_CLEARLY_DEFINED_CACHE_DIR`                 | `WAYBILL_CLEARLY_DEFINED_CACHE_DIR`                  |
| `MIKEBOM_CLEARLY_DEFINED_NO_CACHE`                  | `WAYBILL_CLEARLY_DEFINED_NO_CACHE`                   |
| `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE`               | `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE`                |
| `MIKEBOM_CORPUS_CACHE_DIR`                          | `WAYBILL_CORPUS_CACHE_DIR`                           |
| `MIKEBOM_CORPUS_SKIP_OCI`                           | `WAYBILL_CORPUS_SKIP_OCI`                            |
| `MIKEBOM_DEEP_HASH`                                 | `WAYBILL_DEEP_HASH`                                  |
| `MIKEBOM_EDGE_COUNT`                                | `WAYBILL_EDGE_COUNT`                                 |
| `MIKEBOM_EXCLUDE_PATH`                              | `WAYBILL_EXCLUDE_PATH`                               |
| `MIKEBOM_FINGERPRINTS_CACHE_DIR`                    | `WAYBILL_FINGERPRINTS_CACHE_DIR`                     |
| `MIKEBOM_FINGERPRINTS_CORPUS`                       | `WAYBILL_FINGERPRINTS_CORPUS`                        |
| `MIKEBOM_FINGERPRINTS_CORPUS_SHA`                   | `WAYBILL_FINGERPRINTS_CORPUS_SHA`                    |
| `MIKEBOM_FINGERPRINTS_NETWORK_TESTS`                | `WAYBILL_FINGERPRINTS_NETWORK_TESTS`                 |
| `MIKEBOM_FINGERPRINTS_NO_DEFAULT`                   | `WAYBILL_FINGERPRINTS_NO_DEFAULT`                    |
| `MIKEBOM_FINGERPRINTS_REV`                          | `WAYBILL_FINGERPRINTS_REV`                           |
| `MIKEBOM_FINGERPRINTS_SOURCES`                      | `WAYBILL_FINGERPRINTS_SOURCES`                       |
| `MIKEBOM_FIXED_TIMESTAMP`                           | `WAYBILL_FIXED_TIMESTAMP`                            |
| `MIKEBOM_FIXTURE_CACHE`                             | `WAYBILL_FIXTURE_CACHE`                              |
| `MIKEBOM_FIXTURES_DIR`                              | `WAYBILL_FIXTURES_DIR`                               |
| `MIKEBOM_GLOBAL_TIMEOUT_SLOW_TEST`                  | `WAYBILL_GLOBAL_TIMEOUT_SLOW_TEST`                   |
| `MIKEBOM_GO_MOD_WHY_BUDGET_MS`                      | `WAYBILL_GO_MOD_WHY_BUDGET_MS`                       |
| `MIKEBOM_GO_TOOLCHAIN_E`                            | `WAYBILL_GO_TOOLCHAIN_E`                             |
| `MIKEBOM_HELM_INTEGRATION`                          | `WAYBILL_HELM_INTEGRATION`                           |
| `MIKEBOM_HELM_RENDER`                               | `WAYBILL_HELM_RENDER`                                |
| `MIKEBOM_HELM_RENDER_TIMEOUT_SECS`                  | `WAYBILL_HELM_RENDER_TIMEOUT_SECS`                   |
| `MIKEBOM_INCLUDE_LEGACY_RPMDB`                      | `WAYBILL_INCLUDE_LEGACY_RPMDB`                       |
| `MIKEBOM_INCLUDE_VENDORED`                          | `WAYBILL_INCLUDE_VENDORED`                           |
| `MIKEBOM_LOG`                                       | `WAYBILL_LOG`                                        |
| `MIKEBOM_MAX_RPM_BYTES`                             | `WAYBILL_MAX_RPM_BYTES`                              |
| `MIKEBOM_NO_DEPRECATION_NOTICE`                     | `WAYBILL_NO_DEPRECATION_NOTICE`                      |
| `MIKEBOM_NO_DESIGN_TIER_ADVISORY`                   | `WAYBILL_NO_DESIGN_TIER_ADVISORY`                    |
| `MIKEBOM_NO_GO_MOD_WHY`                             | `WAYBILL_NO_GO_MOD_WHY`                              |
| `MIKEBOM_OCI_AUTH_PRIVATE_IMAGE_REF`                | `WAYBILL_OCI_AUTH_PRIVATE_IMAGE_REF`                 |
| `MIKEBOM_OCI_AUTH_TESTS`                            | `WAYBILL_OCI_AUTH_TESTS`                             |
| `MIKEBOM_OCI_CACHE`                                 | `WAYBILL_OCI_CACHE`                                  |
| `MIKEBOM_OCI_CACHE_DIR`                             | `WAYBILL_OCI_CACHE_DIR`                              |
| `MIKEBOM_OCI_CACHE_SIZE`                            | `WAYBILL_OCI_CACHE_SIZE`                             |
| `MIKEBOM_OCI_NETWORK_TESTS`                         | `WAYBILL_OCI_NETWORK_TESTS`                          |
| `MIKEBOM_OFFLINE`                                   | `WAYBILL_OFFLINE`                                    |
| `MIKEBOM_PERF_IMAGE`                                | `WAYBILL_PERF_IMAGE`                                 |
| `MIKEBOM_PKG_ALIAS`                                 | `WAYBILL_PKG_ALIAS`                                  |
| `MIKEBOM_PNPM_MULTIVER_AUDIT`                       | `WAYBILL_PNPM_MULTIVER_AUDIT`                        |
| `MIKEBOM_PODMAN_INTEGRATION`                        | `WAYBILL_PODMAN_INTEGRATION`                         |
| `MIKEBOM_PODMAN_ROOTFUL_INTEGRATION`                | `WAYBILL_PODMAN_ROOTFUL_INTEGRATION`                 |
| `MIKEBOM_PREPR_EBPF`                                | `WAYBILL_PREPR_EBPF`                                 |
| `MIKEBOM_REFERRER_MAX_BYTES`                        | `WAYBILL_REFERRER_MAX_BYTES`                         |
| `MIKEBOM_REGISTRY_GHCR_IO_PASSWORD`                 | `WAYBILL_REGISTRY_GHCR_IO_PASSWORD`                  |
| `MIKEBOM_REGISTRY_GHCR_IO_USERNAME`                 | `WAYBILL_REGISTRY_GHCR_IO_USERNAME`                  |
| `MIKEBOM_REGISTRY_MY_ECR_AMAZONAWS_COM_PASSWORD`    | `WAYBILL_REGISTRY_MY_ECR_AMAZONAWS_COM_PASSWORD`     |
| `MIKEBOM_REGISTRY_MY_ECR_AMAZONAWS_COM_USERNAME`    | `WAYBILL_REGISTRY_MY_ECR_AMAZONAWS_COM_USERNAME`     |
| `MIKEBOM_REGISTRY_PASSWORD`                         | `WAYBILL_REGISTRY_PASSWORD`                          |
| `MIKEBOM_REGISTRY_USERNAME`                         | `WAYBILL_REGISTRY_USERNAME`                          |
| `MIKEBOM_REQUIRE_SPDX3_VALIDATOR`                   | `WAYBILL_REQUIRE_SPDX3_VALIDATOR`                    |
| `MIKEBOM_REQUIRE_TRANSITIVE_PARITY`                 | `WAYBILL_REQUIRE_TRANSITIVE_PARITY`                  |
| `MIKEBOM_RPM_DISTRO`                                | `WAYBILL_RPM_DISTRO`                                 |
| `MIKEBOM_RUN_BYTE_IDENTITY_SUITE`                   | `WAYBILL_RUN_BYTE_IDENTITY_SUITE`                    |
| `MIKEBOM_RUN_PUBLIC_CORPUS`                         | `WAYBILL_RUN_PUBLIC_CORPUS`                          |
| `MIKEBOM_SBOMQS_BIN`                                | `WAYBILL_SBOMQS_BIN`                                 |
| `MIKEBOM_SKIP_DOCKER_INTEGRATION`                   | `WAYBILL_SKIP_DOCKER_INTEGRATION`                    |
| `MIKEBOM_UPDATE_CDX_GOLDENS`                        | `WAYBILL_UPDATE_CDX_GOLDENS`                         |
| `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS`              | `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS`               |
| `MIKEBOM_UPDATE_SPDX3_GOLDENS`                      | `WAYBILL_UPDATE_SPDX3_GOLDENS`                       |
| `MIKEBOM_UPDATE_SPDX_GOLDENS`                       | `WAYBILL_UPDATE_SPDX_GOLDENS`                        |
| `MIKEBOM_WALKER_DEBUG`                              | `WAYBILL_WALKER_DEBUG`                               |
| `MIKEBOM_WARM_GO_CACHE_CONCURRENCY`                 | `WAYBILL_WARM_GO_CACHE_CONCURRENCY`                  |
| `MIKEBOM_WARM_GO_CACHE_MODE`                        | `WAYBILL_WARM_GO_CACHE_MODE`                         |

**Notes**:
- The 73-line raw grep survey included a few partial captures (`MIKEBOM_REGISTRY_`, `MIKEBOM_UPDATE_`, `MIKEBOM_UPDATE_PUBLIC_`, `MIKEBOM_REQUIRE_SPDX`) that are prefixes of longer full names — table lists only the canonical full names in use.
- `MIKEBOM_NONEXISTENT` + `MIKEBOM_NONEXISTENT_PASSPHRASE_ENV` + `MIKEBOM_PRE` are test-fixture-only strings (assert-not-defined checks); they still rename mechanically → `WAYBILL_NONEXISTENT` etc.

## Consumer migration recipe (drop-in shell)

```bash
# For any CI script, Makefile, docker-compose file, or shell profile:
sed -i.bak 's/\bMIKEBOM_/WAYBILL_/g' <files>

# Verify:
grep -E '\bMIKEBOM_' <files>   # should return empty
```

## Contract stability

- Discriminants (env-var names) are stable within the post-rename world. A follow-up spec that adds a new env var uses `WAYBILL_<name>` — never re-introduces `MIKEBOM_*`.
- The 73-entry mapping is captured here as the canonical audit artifact. Any newly-discovered `MIKEBOM_*` var in the codebase during rename implementation extends this table + gets renamed in the same commit.
