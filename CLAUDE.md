# mikebom Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-05-08

## Active Technologies
- Rust stable (user-space only; no eBPF touched in this milestone) (002-python-npm-ecosystem)
- N/A — pure filesystem reads. All state lives in memory for the lifetime of a scan. (002-python-npm-ecosystem)
- Rust stable (same workspace compiler as milestones 001–002). No new nightly-only features required for user-space readers. `mikebom-ebpf` is untouched. (003-multi-ecosystem-expansion)
- N/A — all state is in-process for the duration of a single scan, same as milestone 002. (003-multi-ecosystem-expansion)
- Rust stable, same workspace compiler as milestones 001–003. No new nightly-only features. `mikebom-ebpf` is untouched. (004-rpm-binary-sboms)
- N/A — all state is in-process for the duration of a single scan. Mirrors milestones 002 / 003. (004-rpm-binary-sboms)
- Rust stable (same workspace toolchain as milestones 001–004) + No new crates. Existing: `tar = 0.4`, `object = 0.36`, `rpm = 0.22`, `cyclonedx-bom`, `serde/serde_json`, `flate2`, `tempfile`, `tracing`. (005-purl-and-scope-alignment)
- N/A — in-memory per scan; no persistence. (005-purl-and-scope-alignment)
- N/A — attestations are single JSON files (signed or (006-sbomit-suite)
- Rust stable, same workspace toolchain as milestones 001–006. No nightly features. `mikebom-ebpf` untouched. + Existing only — `quick-xml = "0.31"` for POM parsing (already used in `maven.rs`), `walkdir`, `serde`/`serde_json`, `tracing`. No new crates. (007-polyglot-fp-cleanup)
- Rust stable, same workspace as milestones 001–007. No nightly features. `mikebom-ebpf` untouched. + Existing only — `quick-xml`, `zip`, `walkdir`, `serde`/`serde_json`, `tracing`. No new crates. (008-polyglot-final-cleanup)
- Rust stable, same workspace as milestones 001–008. No nightly features. `mikebom-ebpf` untouched. + Existing only — `zip` (archive read), `spdx` (via `SpdxExpression::try_canonical`), `tracing`. No new crates. (009-maven-shade-deps)
- Rust stable (same workspace toolchain as milestones 001–009). No nightly features. `mikebom-ebpf` is untouched — this milestone is user-space only. (010-spdx-output-support)
- N/A — all state is in-process for the duration of a single scan, mirroring milestones 002–009. (010-spdx-output-support)
- Rust stable (workspace toolchain inherited from milestones 001–010; no nightly required for user-space work) + existing only — `serde`/`serde_json` (JSON-LD encoding), `data-encoding` (BASE32 for deterministic SPDXIDs / IRIs), `sha2` (content-addressed IRIs, scan fingerprint), `chrono` (RFC 3339 timestamps), `spdx` (license-expression canonicalization, already used by SPDX 2.3 path), `tracing`, `anyhow`. Dev-dep: existing `jsonschema = "0.46"` (already validates SPDX 2.3) extended to SPDX 3.0.1. No new crates. (011-spdx-3-full-support)
- N/A — all state in-process per scan (mirrors milestones 002–010). (011-spdx-3-full-support)
- Rust stable (workspace toolchain inherited from milestones 001–011; no nightly required). + existing only — `spdx` (license-expression canonicalization), `data-encoding` (BASE32 for LicenseRef hash prefix), `sha2`, `serde`/`serde_json`, `tracing`, `anyhow`. Dev-dep: existing `jsonschema = "0.46"`. **No new crates.** (012-sbom-quality-fixes)
- N/A — in-process per scan. (012-sbom-quality-fixes)
- Rust stable (workspace toolchain inherited from milestones 001–012; no nightly). + existing only — `serde`/`serde_json` (format output parsing), `regex` (catalog-row parsing — already in the dependency closure), `tempfile`, `tracing`, `anyhow`. `clap` for the new `parity-check` subcommand (already used for `scan`). **No new crates.** (013-format-parity-enforcement)
- N/A — all state in-process per test invocation / per CLI invocation. (013-format-parity-enforcement)
- Rust stable (workspace toolchain inherited from milestones 001–015; no nightly required for this user-space-only work). + existing only — `cargo +stable clippy` (lint engine), `dtolnay/rust-toolchain@stable` (already used in CI), `Swatinem/rust-cache@v2` (already used). **No new crates.** (016-remaining-clippy-cleanup)
- N/A — purely source-tree edits. (016-remaining-clippy-cleanup)
- Rust stable (workspace toolchain inherited + existing only — `sha2` (per-file SHA-256, (038-minimal-image-deep-hash)
- N/A — all state in-process per scan; reuses milestone (038-minimal-image-deep-hash)
- N/A — this milestone touches Markdown only. + None new. (046-docs-refresh)
- Rust stable (workspace toolchain inherited). + existing only — `serde`, `serde_json`, (047-scope-self-description)
- Rust stable. + existing only — `std::path`, (048-component-role)
- Rust stable. + existing only — `std::collections`, (049-go-source-scope)
- Rust stable (workspace toolchain inherited from milestones 001–052; no nightly). + Existing only — `serde`/`serde_json`, `tracing`, `anyhow`, `tempfile`. **No new crates.** The version-resolution ladder shells out to `git describe`; `git` is already an implicit project assumption (workspace itself is a git repo, CI uses git). (053-go-main-module-edges)
- N/A — all state in-process per scan; no persistence. (053-go-main-module-edges)
- Rust stable (workspace toolchain inherited from milestones 001–053; no nightly required for this user-space-only work). + Existing only — `std::fs::canonicalize`, `std::collections::HashSet`, `PathBuf`. **No new crates.** Per spec assumption: not pulling `walkdir` or `ignore` crates — std-only is the design intent matching the existing minimal-dependency Cargo.toml posture. (054-fix-walker-symlink-hang)
- N/A — visited-set is per-walker-invocation in-memory state, cleared between scans. (054-fix-walker-symlink-hang)
- Rust stable (workspace toolchain inherited from milestones 001–054; no nightly required for this user-space-only work). + Existing only — `reqwest` (workspace, `default-features = false, features = ["json", "rustls-tls"]`) for proxy `.mod` fetches; `tokio` (workspace) for async semaphore + concurrent fetches; `std::process::Command` for `go mod graph` subprocess (same pattern as `git describe` at `golang.rs:733`); `serde_json`/`tracing`/`anyhow` already pervasive. **One new dev-only dep**: `wiremock = "0.6"` for FR-011/FR-012 hermetic HTTP fixture (alternative: hand-rolled `tokio::net::TcpListener` stub if dev-dep addition is contested in review). (055-go-transitive-edges)
- N/A — all state in-process per scan; no persistence (matches milestones 002–053 posture, restated in spec Q3 clarification). (055-go-transitive-edges)
- Rust stable (workspace toolchain inherited from milestones 001–063; no nightly required for this user-space-only work). + Existing only — `toml = "0.8"` (already used by `mikebom-cli/src/scan_fs/package_db/cargo.rs:305`), `serde`/`serde_json`, `tracing`, `anyhow`. **No new crates.** No subprocess calls (manifest-only resolution; the `git describe` ladder from milestone 053 is *not* needed because `[package].version` is always declared in cargo manifests). (064-cargo-main-module)
- N/A — all state in-process per scan; no persistence (matches every milestone since 002). (064-cargo-main-module)
- Rust stable (workspace toolchain inherited from milestones 001–065; no nightly required). + Existing only — `serde_json` (already used by `npm/walk.rs`), `tracing`, `anyhow`. **No new crates.** No subprocess calls. (066-npm-main-module)
- Rust stable (workspace toolchain inherited; no nightly). + Existing only — `toml = "0.8"` (already used by `mikebom-cli/src/scan_fs/package_db/cargo.rs:305` and indirectly elsewhere), `serde`/`serde_json`, `tracing`, `anyhow`. **No new crates.** No subprocess calls. (068-pip-main-module)
- Rust stable; no nightly. + Existing only — no new crates. Reuses `parse_gemspec_full` (regex-based pure-Rust gemspec parser at `gem.rs:947`), `build_gem_purl` (PURL helper), `parse_gemspec_groups` (dep-section extractor for FR-007 edge classification). (069-gem-main-module)
- Rust stable; no nightly. + Existing only — `quick-xml` (already used by `parse_pom_xml`), `serde`, `tracing`, `anyhow`. No new crates. (070-maven-main-module)
- Rust stable (workspace toolchain inherited from milestones 001–070; no nightly required for this user-space-only work). + Existing only — `serde_json` (JSON value walking + canonicalization), `quick-xml` n/a (SPDX 2.3 / SPDX 3 are JSON), `tracing`, `anyhow`. The existing `parity/extractors/` infrastructure (68 catalog rows, `Directionality` enum, `MikebomAnnotationCommentV1` envelope, `extract_mikebom_annotation_values` helper) IS the substrate. No new crates. (071-annotation-parity-spdx23)
- N/A — all parity comparison happens in-memory at test/CI time over emitted JSON documents. (071-annotation-parity-spdx23)
- Rust stable (workspace toolchain inherited from milestones 001–071; no nightly). + Existing only — `sha2` (workspace; SHA-256 over the canonical JSON envelope), `serde`/`serde_json` (canonicalization via the milestone-071 `canonicalize_for_compare` helper at `parity/extractors/common.rs`), `data-encoding` (already used for hex-encoding hashes), `tracing`, `anyhow`, `clap` (for the new flag + new subcommands). No new crates. (072-cross-tier-sbom-binding)
- N/A — binding metadata lives in emitted SBOMs only; no caches, no registries. The `verify-binding` / `trace-binding` commands read SBOMs from operator-supplied paths. (072-cross-tier-sbom-binding)
- Rust stable (workspace toolchain inherited from milestones 001–072; no nightly). + Existing only — `serde`/`serde_json` (envelope decode), `tracing` (info/warn logs), `anyhow` (CLI error propagation), `clap` (`ValueEnum` not needed — `Vec<String>` with manual validation is fine since the scheme syntax is regex-bounded; alternatively a custom `Identifier` newtype that implements `FromStr` and clap's `Args`-derive picks it up). `Command::new("git")` for the auto-detection shell-out (same pattern as milestone 053's `git describe`). No new `Cargo.toml` deps. (073-source-identifiers)
- N/A — identifier metadata lives in emitted SBOMs only. (073-source-identifiers)
- Rust stable (workspace toolchain inherited from milestones 001–073; no nightly required for this user-space-only work). + Existing only — `std::process::Command` for the new `git rev-parse HEAD` shell-out (same pattern as the existing `git remote get-url` calls at `auto_detect.rs:32`+ and as milestone 053's `git describe` ladder), `tracing` for info/warn logs, `anyhow` for error propagation. **No new `Cargo.toml` deps.** (074-build-tier-id-autodetect)
- N/A — identifiers live in emitted SBOMs only; no caches, no persistence. (074-build-tier-id-autodetect)
- Rust stable (workspace toolchain inherited from milestones 001–074; no nightly). + Existing only — the `url = "2.5.8"` crate is already in the dependency closure (transitive via `reqwest`); this milestone promotes it to a direct workspace dependency. No new transitive deps. Plus `tracing` (info-level logs), `anyhow` (error propagation), `clap` (the new boolean flag — `Args`-derive picks it up). **No additions to the dependency tree at the lockfile level.** (075-strip-id-credentials)
- N/A — sanitization is a pure-function transformation; no caches, no persistence. (075-strip-id-credentials)
- Rust stable (workspace toolchain inherited from milestones 001–075; no nightly). + Existing only — `serde`/`serde_json`, `tracing`, `anyhow`, `clap` (the two new flags via derive), `thiserror`. The build-tier subject extraction reuses the in-toto witness-v0.1 subject set the existing trace pipeline already collects in-process; no new subprocess calls, no new I/O, no new crates. **No additions to the dependency tree at the lockfile level.** (076-subject-component-ids)
- N/A — identifier emission is a pure metadata transform; no caches, no persistence. (076-subject-component-ids)
- Rust stable (workspace toolchain inherited from milestones 001–076; no nightly). + Existing only — `serde`/`serde_json`, `tracing`, `anyhow`, `clap` (the two new flags via derive). The `url` crate is already a direct workspace dep (promoted in milestone 075) and reused for URL-encoding the override values into the PURL `name` segment per RFC 3986. **No additions to the dependency tree at the lockfile level.** (077-root-component-override)
- N/A — purely a metadata transform; no caches, no persistence. (077-root-component-override)
- Rust stable (workspace toolchain inherited from milestones 001–077; no nightly). **Plus Python 3.10+** in CI/test for the JPEWdev `spdx3-validate` tool — Python is NEW for this milestone but only at the CI/test layer; the mikebom production binary stays pure Rust per Constitution Principle I. + Existing only on the Rust side — `serde`/`serde_json` (existing JSON-LD round-tripping), `tracing`, `anyhow`, plus a small `Command::new("spdx3-validate")` shell-out helper for the integration tests. Dev/CI side adds the **JPEWdev `spdx3-validate`** Python package, version-pinned per FR-008. **No additions to the Rust `Cargo.toml` deps.** (078-spdx3-conformance)
- N/A — purely a metadata transform on the SPDX 3 emission code path; no caches, no persistence. (078-spdx3-conformance)
- Rust stable (workspace toolchain inherited from milestones 001–078; no nightly). + Existing only — `serde`/`serde_json` (JSON-LD round-tripping), `regex` (already in the dependency closure for the `gitoid` detection per Q2's regex `^[0-9a-f]{40}$`), `tracing`, `anyhow`. No new Cargo deps. Python 3.10+ in CI/test layer (already added in milestone 078 for `spdx3-validate==0.0.5`); reused as-is. (079-spdx3-id-vocab)
- N/A — pure metadata transform on the SPDX 3 emission code path; no caches, no persistence. (079-spdx3-id-vocab)
- Rust stable (workspace toolchain inherited from milestones 001–079; no nightly). + Existing only — `serde`/`serde_json` (JSON-LD round-tripping; the existing CDX emission path uses `serde_json::Value` directly, not structured types from the `cyclonedx-bom` crate, so adding new fields is purely additive JSON construction), `tracing`, `anyhow`, `clap` (the five new flags via derive — `--creator` repeatable via `ArgAction::Append`; `--annotator`/`--annotation-comment` via two parallel `Vec<String>` fields with post-validation per research §3), `chrono` (annotation timestamp deterministic per scan-emission time, same source as `creationInfo.created`), `thiserror` (parser error enum). Reuses milestone 078's `spdx3-validate==0.0.5` as the SPDX 3 conformance gate. **No new Cargo dependencies.** (080-user-sbom-metadata)
- N/A — pure metadata transform on the SBOM emission code paths; no caches, no persistence. The `--metadata-file` JSON sidecar is a one-shot read; mikebom holds the parsed values in process for the duration of the scan. (080-user-sbom-metadata)
- Rust stable (workspace toolchain inherited from milestones 001–080; no nightly). + Existing only — `serde`/`serde_json` (JSON-LD round-tripping), `tracing`, `anyhow`, `clap` (the new `--sbom-type` flag via derive). Reuses milestone-047's `lifecycle_phases.rs::aggregate_phases` helper as the source of truth for tier aggregation. Reuses milestone-078's `spdx3-validate==0.0.5` conformance gate. **No new Cargo dependencies.** (081-sbom-type-clarity)
- N/A — pure metadata-emission transform. No caches, no persistence. (081-sbom-type-clarity)
- N/A — pure documentation work. No code paths touched. The Rust workspace continues to compile + test identically pre/post merge. + None new. The pre-PR gate uses the existing `cargo +stable clippy` + `cargo +stable test --workspace` pipeline; both remain stable across docs-only changes. (082-docs-refresh)
- Rust stable (workspace toolchain inherited from milestones 001–082; no nightly). + Existing only — `serde`/`serde_json` (parsing emitted SBOM JSON), `tracing`, `anyhow`. The audit harness shells out to `trivy` and `syft` as external CLI tools (similar to milestone 078's `spdx3-validate` shell-out pattern). Source-format direct readers use existing per-ecosystem parsers in `mikebom-cli/src/scan_fs/package_db/` for tiebreaker comparisons (we don't re-implement; we re-invoke). For OS package managers, native tools (`dpkg-query`, `rpm`, `apk`) are shelled out. **No new Cargo dependencies.** (083-transitive-correctness)
- N/A — purely test infrastructure. The 9 vendored fixtures (~500 KB total) live in `mikebom-cli/tests/fixtures/transitive_parity/<ecosystem>/`. No caches, no persistence beyond test fixtures. (083-transitive-correctness)
- Rust stable (workspace toolchain inherited from milestones 001–083; no nightly). + existing only — `serde`/`serde_json` (CDX JSON construction), `tracing`, `anyhow`, `thiserror`. **No new Cargo dependencies.** The `cyclonedx-bom` workspace dep is already in use; this milestone does not change which crates participate in CDX construction. (084-cdx-mainmod-collapse)
- N/A — purely emission-time identifier-string transformation. No caches, no persistence. (084-cdx-mainmod-collapse)

- Rust stable (user-space) + nightly (eBPF target via `aya-ebpf`) + aya, aya-ebpf, aya-build, tokio, clap, reqwest, serde/serde_json, cyclonedx-bom, packageurl, sha2, chrono, thiserror, anyhow, tracing (001-build-trace-pipeline)

## Feature flags

- **`ebpf-tracing`** (off by default; milestone 020): gates the user-space
  eBPF integration that powers `mikebom trace`. When off (the default
  everywhere — local dev, default CI lanes), aya/aya-log/libc are dropped
  from the dep graph and nightly + bpf-linker are not required. When on
  (Linux + `--features ebpf-tracing`), build the kernel-side artifact
  first via `cargo run -p xtask -- ebpf`, then test with
  `cargo +stable test --workspace --features ebpf-tracing`. Local pre-PR
  opt-in: `MIKEBOM_PREPR_EBPF=1 ./scripts/pre-pr.sh`. CI runs the
  feature-on path in the dedicated `lint-and-test-ebpf` job. See
  `specs/020-ebpf-feature-gate/contracts/feature-flag.md` for the full
  contract.

## Project Structure

```text
src/
tests/
```

## Commands

### Pre-PR verification (MANDATORY)

Before opening any PR, BOTH of these MUST pass locally — not one, not
a subset, BOTH:

1. `cargo +stable clippy --workspace --all-targets` — zero errors
2. `cargo +stable test --workspace` — every suite `ok. N passed; 0 failed`

`./scripts/pre-pr.sh` runs both in order and exits non-zero on the
first failure — preferred over invoking them by hand so the flag
set stays aligned with CI.

These are the exact commands CI runs (`.github/workflows/ci.yml`).
`cargo test -p mikebom` alone is insufficient: it does not run clippy,
and clippy's `--all-targets` enforces `clippy::unwrap_used` inside
`#[cfg(test)]` modules too (the `mikebom-cli` crate root deny'ies it
per Constitution Principle IV). Test code that uses `.unwrap()` must
be guarded with:

```rust
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
```

matching the existing convention throughout `mikebom-cli/src/trace/`.

If you open a PR without running these two commands clean, CI will
reject it. Do not cite a passing per-crate `cargo test` as evidence
of CI-readiness — they are not equivalent.

## Code Style

Rust stable (user-space) + nightly (eBPF target via `aya-ebpf`): Follow standard conventions

## Recent Changes
- 084-cdx-mainmod-collapse: Added Rust stable (workspace toolchain inherited from milestones 001–083; no nightly). + existing only — `serde`/`serde_json` (CDX JSON construction), `tracing`, `anyhow`, `thiserror`. **No new Cargo dependencies.** The `cyclonedx-bom` workspace dep is already in use; this milestone does not change which crates participate in CDX construction.
- 083-transitive-correctness: Added Rust stable (workspace toolchain inherited from milestones 001–082; no nightly). + Existing only — `serde`/`serde_json` (parsing emitted SBOM JSON), `tracing`, `anyhow`. The audit harness shells out to `trivy` and `syft` as external CLI tools (similar to milestone 078's `spdx3-validate` shell-out pattern). Source-format direct readers use existing per-ecosystem parsers in `mikebom-cli/src/scan_fs/package_db/` for tiebreaker comparisons (we don't re-implement; we re-invoke). For OS package managers, native tools (`dpkg-query`, `rpm`, `apk`) are shelled out. **No new Cargo dependencies.**
- 082-docs-refresh: Added N/A — pure documentation work. No code paths touched. The Rust workspace continues to compile + test identically pre/post merge. + None new. The pre-PR gate uses the existing `cargo +stable clippy` + `cargo +stable test --workspace` pipeline; both remain stable across docs-only changes.


<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
