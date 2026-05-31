# Implementation Plan: Ecosystem Coverage Expansion (Phase 1)

**Branch**: `106-ecosystem-coverage-expansion` | **Date**: 2026-05-31 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/106-ecosystem-coverage-expansion/spec.md`

## Summary

Four new filesystem-scan readers in `mikebom-cli/src/scan_fs/package_db/` close
the highest-impact non-C/C++ coverage gaps surfaced via GitHub issues #275-#278:

| User Story | Issue | New parser file(s) | PURL ecosystem |
|---|---|---|---|
| US1 — uv (Python) | [#276](https://github.com/kusari-sandbox/mikebom/issues/276) | `pip/uv_lock.rs` | `pkg:pypi/...` |
| US2 — Bun (JS) | [#278](https://github.com/kusari-sandbox/mikebom/issues/278) | `npm/bun_lock.rs` + `npm/jsonc.rs` | `pkg:npm/...` |
| US3 — Gradle (JVM) | [#277](https://github.com/kusari-sandbox/mikebom/issues/277) | `gradle/lockfile.rs` (new dir) | `pkg:maven/...` |
| US4 — NuGet (.NET) | [#275](https://github.com/kusari-sandbox/mikebom/issues/275) | `nuget/csproj.rs`, `nuget/packages_lock.rs`, `nuget/directory_packages_props.rs` (new dir) | `pkg:nuget/...` |

Each new reader:

- Produces PURLs whose ecosystems are **already wired** through the resolver +
  deps.dev enrichment + parity catalog. **Zero new `mikebom:*` annotations**.
- Slots into the existing `scan_fs/package_db/mod.rs::read_all` dispatcher with
  one additional `<reader>::read(...)` call per story.
- Participates in the milestone-105 dedup pipeline (`scan_fs/dedup.rs`)
  unchanged — same `PackageDbEntry` output, same dedup precedence.
- Reuses the milestone-052 `lifecycle_scope` → CDX `scope: "excluded"` mapping
  for build-only deps (Gradle buildscript, NuGet `PrivateAssets="All"`).
- Adds workspace-root + workspace-member emission per Clarification Q1 — the
  existing `mikebom:component-role` annotation gains a new open-enum value
  (`workspace-root`), documented in `docs/reference/sbom-format-mapping.md`'s
  C40 row.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–105; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `toml = "0.8"` (uv.lock TOML parsing — already used by `cargo.rs`), `quick-xml = "0.31"` (NuGet `.csproj`/`.vbproj`/`.fsproj`/`Directory.Packages.props` XML parsing — already used by `maven.rs`), `serde_json` (bun.lock JSONC + `packages.lock.json` — pervasive in the workspace), `std::str::Lines` (Gradle line-format parsing), `tracing`, `anyhow`, `thiserror`, `clap`. **No new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; mirrors every milestone since 002.

**Testing**: `./scripts/pre-pr.sh` (= `cargo +stable clippy --workspace --all-targets -- -D warnings` then `cargo +stable test --workspace`). Plus per-reader unit tests in each new module, per-format goldens (CDX 1.6 / SPDX 2.3 / SPDX 3.0.1) under `mikebom-cli/tests/fixtures/golden/<ecosystem>/`, and integration tests against the new in-repo fixtures at `mikebom-cli/tests/fixtures/golden_inputs/<ecosystem>/`.

**Target Platform**: Linux (x86_64, aarch64), macOS, Windows. All readers are pure-Rust filesystem operations; no platform-specific code paths.

**Project Type**: CLI tool — single workspace, three-crate architecture per Constitution Principle VI (`mikebom-cli`, `mikebom-common`, `mikebom-ebpf`). Only `mikebom-cli` is touched.

**Performance Goals**: ≤5% wall-clock delta vs. milestone-105-merge baseline on the existing golden-fixture corpus (SC-008).

**Constraints**:

- Offline mode only (FR-012). No network calls during scan.
- Polyglot-safe per milestone-105 FR-014: a parse failure in any new reader MUST NOT abort the scan or interfere with other ecosystem readers.
- Byte-identity parity across CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 for any new component data — verified via the existing `parity::extractors` round-trip suite.
- Zero new `mikebom:*` annotations. The existing `mikebom:component-role` annotation (C40 catalog row) gains a new open-enum value `"workspace-root"` — that's an open-enum extension, NOT a new annotation, so no Principle V audit required.

**Scale/Scope**: 4 new readers, estimated ~1500–2500 LOC across `mikebom-cli/src/scan_fs/package_db/` + tests. ~6–9 PRs anticipated (one per user-story phase, possibly split where US4-NuGet's three-file scope warrants two PRs). One catalog-doc update (`sbom-format-mapping.md` C40 row enum extension) + one operator-doc update (`docs/ecosystems.md` coverage matrix).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

This milestone touches `mikebom sbom scan`'s static-manifest discovery codepath in `scan_fs/package_db/` — the same code path as milestone 105. The check below evaluates each principle against the milestone's scope.

| Principle | Status | Notes |
|---|---|---|
| **I. Pure Rust, Zero C** | ✅ PASS | All readers Rust-only. No new C, no build-script C, no FFI. |
| **II. eBPF-Only Observation** | ✅ N/A | Principle II governs `mikebom trace`'s discovery path; this milestone extends `mikebom sbom scan`'s `scan_fs/` codepath (same posture as milestones 002–105). |
| **III. Fail Closed** | ✅ N/A → Mapped | scan_fs path uses warn-and-continue per FR-010 (matches milestone-105 SC-008). |
| **IV. Type-Driven Correctness** | ✅ PASS | Existing `Purl` / `License` / `Hash` / `LifecycleScope` types reused. Production code uses `anyhow` + `thiserror`; no `.unwrap()` outside test code. |
| **V. Specification Compliance** | ✅ PASS | **Zero new `mikebom:*` annotations.** The existing C40 row `mikebom:component-role` gets a new open-enum value `"workspace-root"` (doc-only update per research R3). FR-016 from milestone 105 (credential redaction) is not needed here — no URL-emission paths in this milestone's readers. |
| **VI. Three-Crate Architecture** | ✅ PASS | All changes confined to `mikebom-cli/`. No new crates. |
| **VII. Test Isolation** | ✅ PASS | All new readers are pure logic; no eBPF, no kernel privileges, no root. |
| **VIII. Completeness** | ✅ PASS (advances) | Four new readers reduce false negatives directly. SC-001 through SC-004 quantify the per-ecosystem coverage gains. |
| **IX. Accuracy** | ✅ PASS (preserved) | Existing dedup pipeline (FR-015 from milestone 105) handles cross-reader collisions. Workspace-root + main-module annotations let downstream consumers distinguish self vs third-party. The `unresolved` version sentinel + `tracing::warn!` for CPM lookup misses (FR-007) avoids false-positive version pinning. |
| **X. Transparency** | ✅ PASS | All gap conditions (`unresolved` versions, JSONC parse failures, workspace import chains not chased) emit `tracing::warn!` events naming the offending file. |
| **XI. Enrichment** | ✅ PASS (extends) | New ecosystems flow through existing deps.dev enrichment (PyPI / npm / Maven / NuGet are all already supported there). |
| **XII. External Data Source Enrichment** | ✅ N/A | No external data sources consulted at scan time (offline-only per FR-012). |
| **Boundary #1**: No lockfile-based dependency discovery | ✅ N/A | Same interpretation as Principle II — applies to `mikebom trace`'s eBPF discovery path, not `scan_fs`. |
| **Boundary #2**: No MITM proxy | ✅ N/A | No network observation. |
| **Boundary #3**: No C code | ✅ PASS | All new code is Rust; no new transitive C deps (existing crates only). |
| **Boundary #4**: No `.unwrap()` in production | ✅ PASS | Existing `mikebom-cli` crate-root deny of `clippy::unwrap_used` enforces this; test code uses the `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention. |
| **Pre-PR Verification** | ⚠️ ENFORCED | Every PR in this milestone runs `./scripts/pre-pr.sh` clean. |

**Gate result**: PASS — no unjustified violations, no audit items deferred to research. Cleanest constitution-check pass of any post-100 milestone (compare milestone 105 which had two audit items — R1 for the new annotation, R3 for the linkage-kind conflict).

## Project Structure

### Documentation (this feature)

```text
specs/106-ecosystem-coverage-expansion/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (consolidated R1-R6 findings)
├── data-model.md        # Phase 1 output (entities + workspace emission model)
├── quickstart.md        # Phase 1 output (operator-facing per-reader walkthrough)
├── contracts/           # Phase 1 output (per-reader contracts + workspace model)
│   ├── uv-lock.md
│   ├── bun-lock.md
│   ├── gradle-lockfile.md
│   ├── nuget-csproj.md
│   ├── nuget-packages-lock.md
│   ├── nuget-cpm.md
│   ├── jsonc-stripper.md
│   └── workspace-emission.md
├── checklists/
│   └── requirements.md  # Created during /speckit.specify (already exists)
└── tasks.md             # Phase 2 output (NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           ├── pip/
│           │   ├── uv_lock.rs                 # NEW — US1
│           │   └── mod.rs                     # MODIFIED — add uv_lock to pip dispatcher
│           │
│           ├── npm/
│           │   ├── bun_lock.rs                # NEW — US2
│           │   ├── jsonc.rs                   # NEW — US2 (shared with future tsconfig readers)
│           │   └── mod.rs                     # MODIFIED — add bun_lock to npm dispatcher
│           │
│           ├── gradle/                        # NEW directory — US3
│           │   ├── mod.rs                     # NEW
│           │   └── lockfile.rs                # NEW — both gradle.lockfile + buildscript-gradle.lockfile
│           │
│           ├── nuget/                         # NEW directory — US4
│           │   ├── mod.rs                     # NEW
│           │   ├── csproj.rs                  # NEW — .csproj/.vbproj/.fsproj XML parser
│           │   ├── packages_lock.rs           # NEW — packages.lock.json
│           │   ├── directory_packages_props.rs # NEW — Central Package Management
│           │   └── private_assets.rs          # NEW — PrivateAssets/IncludeAssets/ExcludeAssets resolver
│           │
│           └── mod.rs                         # MODIFIED — dispatch the 4 new readers in read_all
│
└── tests/
    ├── fixtures/
    │   ├── golden_inputs/                     # NEW per-ecosystem fixture trees (in-repo per research R6)
    │   │   ├── uv_lock/
    │   │   │   ├── basic/                     # US1 scenario 1
    │   │   │   ├── with_dependencies/         # US1 scenario 2
    │   │   │   ├── workspace/                 # US1 scenario 5
    │   │   │   └── source_only/               # US1 scenario 4
    │   │   ├── bun_lock/
    │   │   │   ├── basic/
    │   │   │   ├── scoped_packages/
    │   │   │   ├── workspace/
    │   │   │   └── overrides/
    │   │   ├── gradle_lockfile/
    │   │   │   ├── runtime_only/
    │   │   │   ├── buildscript_classpath/
    │   │   │   └── multi_config/
    │   │   └── nuget/
    │   │       ├── csproj_legacy/             # PackageReference with Version=
    │   │       ├── csproj_cpm/                # PackageReference without Version= + Directory.Packages.props
    │   │       ├── packages_lock_present/
    │   │       └── private_assets_all/
    │   │
    │   └── golden/                            # NEW per-ecosystem byte-identity goldens
    │       ├── cyclonedx/
    │       │   ├── uv-lock.cdx.json
    │       │   ├── bun-lock.cdx.json
    │       │   ├── gradle-lockfile.cdx.json
    │       │   └── nuget.cdx.json
    │       └── (parallel spdx-2.3/ and spdx-3/ trees)
    │
    └── (per-US contract + integration tests follow milestone-105 patterns)

docs/
└── reference/
    └── sbom-format-mapping.md                  # MODIFIED — C40 row enum extended with "workspace-root"

docs/ecosystems.md                              # MODIFIED — coverage matrix gains 4 new rows
```

**Structure Decision**: Single-project layout (the mikebom workspace). All changes confined to the existing `mikebom-cli/` crate. Two new module directories (`gradle/` and `nuget/`) hold ecosystem-specific parsers; the other two stories extend existing module directories (`pip/` and `npm/`). The reader-dispatch architecture in `scan_fs/package_db/mod.rs::read_all` is the single integration point — each new reader adds one call into that function. No changes to the dedup pipeline (`scan_fs/dedup.rs`), parity catalog, or builder code — all four ecosystems use PURL types and the LifecycleScope enum that were established in earlier milestones.

## Complexity Tracking

| Concern | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| New JSONC stripper from scratch (US2 + future reuse) | `bun.lock` is JSONC — `serde_json` rejects comments. Research R1 confirmed no existing in-tree JSONC handler; the spec's original "tsconfig.json handling" reference was a hallucination, since corrected. | Adding `serde_jsonc` / `json5` as a Cargo dep was considered but rejected per the workspace's no-new-deps posture. Hand-rolled stripper is ~20 LOC + ~10 unit tests; cheaper than dep churn. Pattern mirrors existing `gem::strip_ruby_comment` and `golang::legacy::strip_line_comment` — proven shapes in-tree. |
| Workspace-root synthetic component (US1 + US2) | Clarification Q1 settled the workspace emission policy: workspace members as first-class components + synthetic root above them. This is the model the user worked through with the mikebom-cli + mikebom-common Cargo-workspace example. | Skipping workspace members entirely (the spec's original draft stance) was rejected during clarification because it loses intra-workspace dependency edges that genuinely matter for SBOM accuracy. The synthetic-root approach is one new concept but reuses the existing `mikebom:component-role` annotation (C40 row open-enum extended with `workspace-root`) — no new annotations. |
| Two new ecosystem directories (`gradle/`, `nuget/`) | First-class ecosystems with multiple file shapes warrant their own modules (matches the existing `pip/`, `npm/`, `golang/` pattern). NuGet specifically has 3 distinct file types (.csproj XML, packages.lock.json JSON, Directory.Packages.props XML) plus the PrivateAssets resolver, justifying internal sub-modules. | Putting Gradle into the existing `maven/` directory was considered but rejected — gradle.lockfile syntax is distinct enough from `pom.xml` that file-grouping by ecosystem (JVM) over file-grouping by manager (Maven vs Gradle) loses clarity. Same logic for NuGet vs an existing dotnet-related module (there isn't one yet). |
