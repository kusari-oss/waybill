# Implementation Plan: C/C++ Ecosystem Expansion (Phase 2)

**Branch**: `105-cpp-ecosystem-expansion` | **Date**: 2026-05-28 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/105-cpp-ecosystem-expansion/spec.md`

## Summary

Six new filesystem-scan readers in `mikebom-cli/src/scan_fs/package_db/` close
the highest-impact C/C++ coverage gaps identified by end-to-end testing of
gRPC, Zephyr, and esp-idf corpora: **CPM.cmake** (modern cmake mainstream),
**conanfile.py** (modern Conan 2.x), **west.yml** (Zephyr), **idf_component.yml**
(esp-idf), **vcpkg classic mode**, and **`.gitmodules` + `find_package` correlation**
(gRPC / LLVM patterns). Yocto / OpenSTLinux is explicitly split into a
follow-on milestone per the clarification session.

Each new reader:

- Produces components with real PURLs and real versions (per US acceptance scenarios)
- Emits the existing C55 `mikebom:source-mechanism` annotation with a new closed-enum value
- Reuses milestone-075's URL-sanitization helper unconditionally on every URL-derived emission (FR-016)
- Participates in a new cross-reader deduplication pipeline that selects winners by deterministic precedence and records losing-reader signal via a new `mikebom:also-detected-via` annotation (FR-015)
- Achieves byte-identity parity across CycloneDX 1.6 / SPDX 2.3 / SPDX 3.0.1
  via two parity-catalog changes: C55 enum extension (5 new values) +
  new row C56 for `mikebom:also-detected-via`

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–104; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `serde`/`serde_json` (manifest parsing for `idf_component.yml`, JSON-LD construction for the parity layer), `serde_yaml = "0.9"` (already a direct dep at `mikebom-cli/Cargo.toml:99` per research R2; used by the new `west.yml` and `idf_component.yml` readers), `regex = "1"` (already a direct dep; CPM.cmake call-site extraction + find_package correlation), `quick-xml` (already used by maven; no role here), `tracing`, `anyhow`, `thiserror`, `clap`. **No new Cargo dependencies.** No subprocesses, no network.

**Storage**: N/A — all state in-process per scan; mirrors every milestone since 002.

**Testing**: `./scripts/pre-pr.sh` (= `cargo +stable clippy --workspace --all-targets -- -D warnings` then `cargo +stable test --workspace`). Plus integration tests against new fixtures in `mikebom-cli/tests/fixtures/golden_inputs/<reader>/`, byte-identity golden suites under `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/<reader>.{cdx,spdx,spdx3}.json`, and a dedicated dedup-determinism test fixture (SC-010) that pins two readers to the same canonical PURL and asserts walk-order invariance.

**Target Platform**: Linux (x86_64, aarch64), macOS, Windows (the milestone-100 platform matrix). All readers are pure-Rust filesystem operations; no platform-specific code paths.

**Project Type**: CLI tool — single workspace, three-crate architecture per Principle VI (`mikebom-cli`, `mikebom-common`, `mikebom-ebpf`). Only `mikebom-cli` is touched.

**Performance Goals**: ≤5% wall-clock delta vs. alpha.41 baseline on the existing golden-fixture corpus (SC-009). For Zephyr v4.4.0 (a large real-world corpus, 577 MB tree), full scan completes in under 60 seconds on the existing CI hardware.

**Constraints**:

- Offline mode only (FR-012). No `deps.dev`, no Conan Center, no Espressif Component Registry. Version resolution is entirely local-file driven.
- No new subprocess shellouts (no `git submodule status` or similar — read `.gitmodules` and the submodule's `.git/HEAD`/packed-refs directly).
- Byte-identity parity across CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 for every new annotation (Constitution Principle X + the existing parity-catalog invariants).
- Constitution Principle V audit completed for every new `mikebom:*` annotation (only one new: `mikebom:also-detected-via`; the audit is the first research item).
- Constitution Principle II / Strict Boundary #1: this milestone extends the **`mikebom sbom scan` filesystem-discovery codepath** (`scan_fs/package_db/`), which is operationally distinct from the `mikebom trace` eBPF-discovery codepath that Principle II governs. The codebase has operated this way for 90+ milestones (002 onward).

**Scale/Scope**: 6 new readers, estimated ~3000–5000 LOC across `mikebom-cli/src/scan_fs/package_db/`, the parity catalog, and tests. ~12–18 PRs anticipated (one per user story phase plus the cross-reader dedup pipeline as a separate phase). Two parity-catalog changes: C55 enum extension (5 new closed-enum values), new C56 row (`mikebom:also-detected-via`).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

This milestone touches `mikebom sbom scan`'s static-manifest discovery codepath in `scan_fs/package_db/`. The check below evaluates each principle against the milestone's scope.

| Principle | Status | Notes |
|---|---|---|
| **I. Pure Rust, Zero C** | ✅ PASS | All readers Rust-only. `serde_yaml = "0.9"` (candidate dep) is pure Rust. No C, no build-script C, no FFI. |
| **II. eBPF-Only Observation** | ✅ N/A | Principle II governs `mikebom trace`'s discovery path. This milestone extends `mikebom sbom scan`'s `scan_fs/` codepath, which is operationally distinct (filesystem-static, not trace-driven). Established practice since milestone 002. The two modes co-exist in the workspace today; this milestone changes no trace-path code. |
| **III. Fail Closed** | ✅ N/A → Mapped | The scan_fs analog of "fail closed" is "warn and continue on per-file parse failures, never silently drop a discoverable manifest". FR-013 codifies this. The polyglot-robustness SC-008 strengthens it (no scan-abort on adjacent-ecosystem errors). |
| **IV. Type-Driven Correctness** | ✅ PASS | No new domain primitives needed; existing `Purl` / `License` / `Hash` newtypes reused. Production code uses `anyhow` + `thiserror`; no `.unwrap()` outside test code (with the `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention). |
| **V. Specification Compliance** | ✅ PASS (post-research) | **Two** new `mikebom:*` annotations introduced — `mikebom:also-detected-via` (C56) and `mikebom:build-reference` (C57). Audits resolved during Phase 0: **R1** for C56 chose a hybrid emission strategy (CDX native `evidence.identity[].methods[]` with `mikebom-source-mechanism` sub-field; SPDX 2.3 / 3.0.1 use the parity-bridging annotation). **R3** introduced C57 as a NEW annotation rather than the originally-spec'd reuse of `mikebom:linkage-kind`, which would have collided with the existing CDX builder debug-assert (`dynamic`/`static`/`mixed` enum) and conflated source-tier with binary-tier semantics. Both audits documented in `docs/reference/sbom-format-mapping.md` per the constitution requirement. The six new C55 enum values are extensions of an already-audited annotation (C55 audit completed in PR #272 / alpha.41); no re-audit needed. |
| **VI. Three-Crate Architecture** | ✅ PASS | All changes confined to `mikebom-cli/`. No new crates, no workspace restructuring. |
| **VII. Test Isolation** | ✅ PASS | All new readers are pure logic; no eBPF, no kernel privileges, no root. Tests run cleanly under unprivileged `cargo +stable test --workspace`. |
| **VIII. Completeness** | ✅ PASS (advances) | Six new readers reduce false negatives directly. SC-001 through SC-005 quantify the completeness gains per ecosystem. |
| **IX. Accuracy** | ✅ PASS (preserved) | New `mikebom:linkage-kind` annotation on submodules (FR-008a) flags un-referenced submodules as `declared-only` so vuln scanners can filter false-positives. FR-009 handles uninitialized submodules transparently. The dedup precedence (FR-015) ensures one component per canonical PURL — no phantom duplicates. |
| **X. Transparency** | ✅ PASS (extends) | New `mikebom:also-detected-via` (FR-015), `mikebom:linkage-kind: "declared-only"` (FR-008a), `mikebom:resolver-step` (FR-009 for uninitialized submodules), and `tracing::warn!` events on credential redaction (FR-016) all use spec-native or already-audited custom mechanisms. |
| **XI. Enrichment** | ✅ PASS (extends) | `also-detected-via` enriches without changing primary component identity. |
| **XII. External Data Source Enrichment** | ✅ N/A | No external data sources consulted at scan time (offline-only per FR-012). |
| **Boundary #1**: No lockfile-based dependency discovery | ✅ N/A | Same interpretation as Principle II — applies to `mikebom trace`'s eBPF discovery path, not scan_fs. |
| **Boundary #2**: No MITM proxy | ✅ N/A | No network observation in this milestone. |
| **Boundary #3**: No C code | ✅ PASS | Verified for `serde_yaml = "0.9"` (pure Rust) during Phase 0 research item R2. |
| **Boundary #4**: No `.unwrap()` in production | ✅ PASS | Existing `mikebom-cli` crate-root deny of `clippy::unwrap_used` enforced by pre-PR clippy. |
| **Pre-PR Verification** | ⚠️ ENFORCED | Every PR in this milestone runs `./scripts/pre-pr.sh` clean: `cargo +stable clippy --workspace --all-targets -- -D warnings` then `cargo +stable test --workspace`. Identical to CI. |

**Gate result**: PASS, conditional on resolution of audit item R1 (Principle V) during Phase 0 research. No unjustified violations.

## Project Structure

### Documentation (this feature)

```text
specs/105-cpp-ecosystem-expansion/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (resolved audit items + dep decisions)
├── data-model.md        # Phase 1 output (entities + dedup precedence model)
├── quickstart.md        # Phase 1 output (operator-facing how-to)
├── contracts/           # Phase 1 output (per-reader manifest contracts + dedup contract)
│   ├── cpm-cmake.md
│   ├── conanfile-py.md
│   ├── west-yml.md
│   ├── idf-component-yml.md
│   ├── vcpkg-classic.md
│   ├── git-submodule.md
│   ├── dedup-precedence.md
│   └── credential-redaction.md
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
│           ├── cmake.rs                      # EXISTING — extended with CPM.cmake call-site detection (US1)
│           ├── conan.rs                      # EXISTING — extended with conanfile.py path (US2)
│           ├── west.rs                       # NEW — Zephyr west.yml reader (US3)
│           ├── idf_component.rs              # NEW — Espressif Component Manager idf_component.yml reader (US4)
│           ├── vcpkg.rs                      # EXISTING — extended with classic-mode `installed/.../info/*.list` (US5)
│           ├── git_submodule.rs              # NEW — `.gitmodules` reader with HEAD-revision lookup + find_package correlation (US6)
│           └── mod.rs                        # EXISTING — adds the two new readers to the dispatch table
│
├── src/
│   └── scan_fs/
│       └── dedup.rs                          # NEW — cross-reader dedup precedence pipeline (FR-015)
│
├── src/
│   └── identifiers/
│       └── sanitize.rs                       # EXISTING (milestone 075) — reused by new readers via FR-016
│
├── src/
│   └── parity/
│       └── extractors/
│           ├── cdx.rs                        # EXISTING — extended with c56_cdx for `mikebom:also-detected-via`
│           ├── spdx2.rs                      # EXISTING — extended with c56_spdx23
│           ├── spdx3.rs                      # EXISTING — extended with c56_spdx3
│           └── mod.rs                        # EXISTING — adds new C56 ParityExtractor row
│
└── tests/
    ├── fixtures/
    │   ├── golden_inputs/                    # NEW — synthetic project trees per reader
    │   │   ├── cpm_cmake/
    │   │   ├── conanfile_py/
    │   │   ├── west/
    │   │   ├── idf_component/
    │   │   ├── vcpkg_classic/
    │   │   ├── git_submodule/
    │   │   └── dedup_collision/              # NEW — two-reader collision fixture (SC-010)
    │   └── golden/                           # EXISTING — adds new per-reader CDX/SPDX 2.3/SPDX 3 byte-identity goldens
    │       ├── cyclonedx/
    │       │   ├── cpm-cmake.cdx.json
    │       │   ├── conanfile-py.cdx.json
    │       │   ├── west.cdx.json
    │       │   ├── idf-component.cdx.json
    │       │   ├── vcpkg-classic.cdx.json
    │       │   ├── git-submodule.cdx.json
    │       │   └── dedup-collision.cdx.json
    │       └── (parallel spdx-2.3/ and spdx-3/ trees)
    │
    ├── source_mechanism_annotation_*.rs       # NEW — per-reader contract tests, mirroring PR #272 pattern
    ├── transitive_parity_cpp_phase2.rs        # NEW — Zephyr / esp-idf / gRPC integration tests
    └── dedup_precedence_determinism.rs        # NEW — SC-010 walk-order invariance test

docs/
└── reference/
    └── sbom-format-mapping.md                 # EXISTING — C55 row enum extended (5 new values), new C56 row appended
```

**Structure Decision**: Single-project layout (the mikebom workspace). All changes confined to the existing `mikebom-cli/` crate. The reader-dispatch architecture in `scan_fs/package_db/mod.rs` is the integration point — each new reader is a peer module added to the existing dispatch table. The cross-reader dedup pipeline (`scan_fs/dedup.rs`) is a NEW module that sits between reader output and the SBOM emission stage; it consumes the deduplicated input the existing emitter expects. The parity-catalog extension follows the exact pattern established by PR #272 (milestone 102): three extractor functions per new row, one row entry in `mod.rs`. The 13 new golden-input/golden-output fixture sets follow the milestone-090 split-test-fixtures convention.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Audit item R1 (Principle V) — `mikebom:also-detected-via` annotation | Cross-reader corroboration signal needs a stable, byte-identical home across all three SBOM formats; the spec mandates it (FR-015) for downstream consumers. | (Considered) Using CDX 1.6 `evidence.identity[].methods[]` as the sole emission with no SPDX-side parity. Rejected because SPDX 2.3 has no native equivalent — the parity catalog would have to skip this signal in the SPDX 2.3 output, violating byte-identity parity invariants. Phase 0 research item R1 evaluates whether a hybrid is feasible (native CDX + parity-bridging `mikebom:*` for SPDX). |
| ~~One candidate new direct dep (`serde_yaml = "0.9"`)~~ — **Resolved during research R2** | west.yml and idf_component.yml are YAML; mikebom needs a YAML parser. | **Resolved: `serde_yaml = "0.9"` is already a direct dep at `mikebom-cli/Cargo.toml:99` (confirmed by research R2).** No Cargo.toml change required; the milestone uses the existing workspace dep. Kept in this table as a historical record of the planning-time consideration. |
