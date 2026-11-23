# Controls (T001, T002, T008)

## T001: `main`'s binary

- **Commit**: `33c445f660b4f06a2d8d24007ad7004317697694`. `git diff 33c445f6 -- waybill-cli waybill-common Cargo.toml Cargo.lock` was empty when it was built.
- **Build**: debug, saved in the session scratchpad as `waybill-main-33c445f6` (`waybill 0.17.0`).

## T002: no ELF `DT_NEEDED` builder (as built)

T002 first checked how waybill reads a binary's imports, as it required. `scan_fs/binary/scan.rs:143-160` collects them from `object`'s `imports()`. On ELF, `object` 0.36.7 (`read/elf/file.rs:387-410`) names an import's library **only through symbol versioning** (`.gnu.version_r`), not from `DT_NEEDED`. Spec 1536 recorded the same.

An ELF with only `DT_NEEDED` entries would therefore produce no `dynamic-linkage` component, and a test built on it would check nothing. K1 uses the existing `macho_dylib(loads, …)` helper instead: `object` reports a Mach-O import through `LC_LOAD_DYLIB`, and it gave a `dynamic-linkage` component (below). The spec's phrase "a `DT_NEEDED` entry" is the evidence kind's name in waybill's docs; the mechanism is the import list.

## T008: what `main` emits for each case (probe)

From a temporary probe test run on the branch before any code change, which is `main`'s code:

| Case | Component | Evidence kind | Technique on `main` |
|---|---|---|---|
| K1 | `pkg:generic/%2Fusr%2Flib%2Flibwaybill-fixture-dep.1.dylib` | `dynamic-linkage` | `manifest-analysis` |
| K2 | `pkg:generic/waybill-fixture-fmt` | `embedded-source-path` | `manifest-analysis` |
| K3 | `pkg:cargo/waybill-fixture-rs@0.1.0`, `pkg:cargo/waybill-fixture-dep@1.2.3` | `cargo-auditable` | `manifest-analysis` |
| K4 | `pkg:rpm/fedora/waybill-fixture-noted@2.0.1` | `elf-note-package` | `manifest-analysis` |
| K5 | `pkg:golang/stdlib@v1.22.1`, `pkg:golang/example.com/waybill-fixture-godep@v1.4.0` | none | `manifest-analysis` |
| K7 | `pkg:generic/zlib@1.3.1` | `embedded-version-string` | `binary-analysis` |

**K6: no record merge is reachable.**
- **Versions:** `dynamic-linkage` and `embedded-source-path` PURLs carry no version, and a pkg-config or CMake record's always does. The equal-PURL deduplicator therefore never merges a record with either kind.
- **K6a**, a bundled-tree binary plus `waybill-fixture-fmt.pc`, produced no pkg-config record at all outside an install prefix.
- **K6b**, two binaries importing one library, produced one `dynamic-linkage` component. The linkage aggregator joined the two imports **before** the deduplicator, so no `kind_merge` fires there.
- **So:** K6's merge rule is a deduplicator unit test on constructed components (T014). K6b stays as an integration check that the joined component lists one method.

## T008: K1–K7 on the log-only commit

`cargo test -p waybill --test binary_kind_technique`, plus the red cases with `--ignored`:

| Case | Result |
|---|---|
| K1 `k1_dynamic_linkage_is_binary_analysis` | red, as planned: `left: ["manifest-analysis"]`, `right: ["binary-analysis"]` |
| K2 `k2_embedded_source_path_is_binary_analysis` | red, as planned: the same |
| K6 `k6_deduped_dynamic_linkage_lists_one_method` | red, as planned: the same |
| K3, K4, K5, K7 | pass |
| `log_only_census_reports_what_will_change` | pass. K1 and K2 each have one `kind_technique_change` (`old: package-database`, `new: binary-analysis`); K6b has one and no `kind_merge` |
| K5's census | pass. Three `go-buildinfo` events at emission, including the binary's `(devel)` main module, which the document then leaves out |

**Byte identity against `main`:** `control_byte_identical_to_main` with `WAYBILL_BINARY_KIND_MAIN_BIN` set to T001's binary and `WAYBILL_BINARY_KIND_IDENTICAL=K1,K2,K3,K4,K5,K6b,K7` passes. Every case's three documents equal `main`'s with version-4 UUIDs masked, so the log-only commit changes no output.

The red cases are marked `#[ignore = "spec 1648: red until T012"]`.
