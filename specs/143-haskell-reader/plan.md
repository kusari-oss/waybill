# Implementation Plan: Haskell ecosystem reader

**Branch**: `143-haskell-reader` | **Date**: 2026-06-25 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/143-haskell-reader/spec.md`

## Summary

Seventeenth language-ecosystem reader added to mikebom (joins cargo, npm, pip, gem, maven, golang, nuget, swift, kotlin, conan, dart, composer, cocoapods, elixir, erlang, scala). Parses `cabal.project.freeze` (cabal-install line-format pinned constraints), `stack.yaml.lock` (YAML lockfile with snapshot SHA + explicit `extra-deps`), `stack.yaml` (resolver identifier source for the Stack snapshot placeholder), `cabal.project` (multi-package project descriptor for multi-package discovery), and `*.cabal` (Cabal-DSL per-package descriptor — `name` / `version` / multi-stanza `build-depends:` extraction). Emits one main-module per `*.cabal` (FR-013 + Q2 union across all stanzas) plus one component per pinned dep, with `pkg:hackage/<name>@<version>` PURLs throughout. Stack snapshot resolvers (`lts-22.0` / `nightly-2024-01-15` / `ghc-9.6.4`) emit ONE placeholder per project with `pkg:generic/<resolver>@<sha-or-unspecified>` + `mikebom:stackage-resolver = "<resolver>"` annotation (full snapshot expansion deferred to v1.1 per FR-005 to preserve FR-012 no-network).

Three Q-clarifications drive the design:

- **Q1 (GHC-stdlib annotation)**: hardcoded ~20-name boot-library allowlist emits `mikebom:ghc-stdlib = "true"` on matching components. Mirrors milestone-141 OTP-stdlib pattern.
- **Q2 (multi-stanza union)**: main-module `depends` unions ALL stanzas' `build-depends:` (library + executable + test-suite + benchmark + build-tool-depends), with per-stanza `mikebom:lifecycle-scope` tagging (runtime vs development); most-binding wins on multi-stanza name collision.
- **Q3 (Hpack detect-and-warn)**: when `package.yaml` is found alongside a generated-by-Hpack `*.cabal` (identified by header regex), emit `tracing::warn!` recommending regeneration. Reader does NOT parse `package.yaml` directly — avoids second-source-of-truth complexity.

`mikebom:source-type` value-set: `hackage-freeze` (cabal-install freeze entries) / `hackage-stack-lock` (Stack lockfile extra-deps) / `hackage-snapshot` (Stackage placeholder) / `hackage-cabal-design` (design-tier from `*.cabal`) / `hackage-main-module` (per-package root). Distinguishes Haskell-derived components from sibling readers via the `hackage-` prefix per the milestone-122/137-142 convention.

**Zero new Cargo dependencies** — `regex` + `serde_yaml` + `serde_json` are workspace deps.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–142; no nightly required).

**Primary Dependencies**: Existing only — `regex` (workspace dep, used by gem/alpm/brew/yocto/cocoapods/elixir/erlang/scala for DSL extraction), `serde_yaml = "0.9"` (workspace dep, used by dart/cocoapods readers for YAML parsing — Stack lockfile is YAML), `serde_json` (workspace dep), `mikebom_common::types::purl::Purl` (PURL construction), `tracing` (warn-and-skip per FR-009 + Q3 Hpack-detect diagnostic per FR-015), `anyhow`/`thiserror`, `std::sync::OnceLock` (regex compile-once). **No new Cargo dependencies.**

**Storage**: N/A — all state is in-process for the duration of a single scan.

**Testing**: `cargo +stable test --workspace`. Synthetic-fixture pattern via `tempfile::tempdir()` constructing minimal `*.cabal` + `cabal.project.freeze` + `stack.yaml.lock` + `stack.yaml` + `cabal.project` trees. Four new integration test files at `mikebom-cli/tests/haskell_*.rs` mirroring the milestone-142 `scala_*.rs` family. SC-004 byte-identity preservation guarded by the existing 14-ecosystem golden suite.

**Target Platform**: Cross-platform reader. Pure-Rust regex + YAML extraction; no Haskell/GHC runtime required on the scan host.

**Project Type**: CLI tool — extends the `mikebom sbom scan` pipeline via the `read_all` dispatcher.

**Performance Goals**: ≤2 ms overhead per `cabal.project.freeze` constraint line. Typical Haskell project (~150 hex deps in freeze): ~8 ms. Heavy multi-package Haskell project (~400 deps across 5 sub-packages): ~15 ms. No-Haskell-detected fast path adds ≤5 µs per non-Haskell scan.

**Constraints**:

- Byte-identical SBOM goldens when no Haskell project present (SC-004).
- Zero new Cargo deps.
- Per-file parse failures warn-and-skip; malformed `cabal.project.freeze` OR `stack.yaml.lock` → fall back to design-tier from sibling `*.cabal` per FR-009.
- The `hackage` PURL type IS purl-spec-blessed; package names lowercased per spec.
- Stack snapshot resolvers emit ONE placeholder per project per FR-005 (preserves FR-012 no-network).
- Multi-stanza `*.cabal` union per Q2 with most-binding-scope precedence.
- Hpack `package.yaml` detection emits `tracing::warn!` per Q3 + FR-015 (reader does NOT parse `package.yaml`).
- GHC boot libraries emit `mikebom:ghc-stdlib = "true"` annotation per Q1 + FR-014.
- No `cabal` / `stack` / GHC invocation; regex + YAML parsing only.

**Scale/Scope**: Typical cabal-managed app: 80–150 freeze entries. Phoenix-equivalent Haskell web app (e.g., Servant + Persistent + Aeson): ~200. Heavy multi-package project (e.g., Cardano-style ~10 sub-packages): ~400–600 unique components after dedup. Per-freeze line-parse: ~2–5 ms warm-cache.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Justification |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | All new code is user-space Rust; no FFI, no C. Regex via workspace `regex`. YAML via workspace `serde_yaml` (pure Rust). No Haskell/GHC runtime evaluation. |
| II. eBPF-Only Observation | N/A | Source-tree language reader; pre-existing discovery surface per every prior language-reader milestone. |
| III. Fail Closed | ✓ | A source tree without any of `*.cabal` / `cabal.project*` / `stack.yaml*` is a clean no-op (FR-008). Per-file parse failures warn-and-skip (FR-009). Q3 Hpack-detect emits diagnostic but never aborts. |
| IV. Type-Driven Correctness | ✓ | Uses `Purl` newtype; no stringly-typed identifiers. Lockfile parsing into typed structs per data-model.md. Production code MUST NOT call `.unwrap()` — error propagation via `Result`. |
| V. Specification Compliance | ✓ | **`hackage` IS a purl-spec-defined type** ([hackage-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/hackage-definition.md)); used verbatim. Names lowercased per purl-spec canonical form. **Three NEW `mikebom:*` annotations**, each audited against standards-native carriers in research §R6: `mikebom:ghc-stdlib` (Q1 boot-library discriminator — no native CDX/SPDX field for "ecosystem stdlib member"), `mikebom:stackage-resolver` (FR-005 snapshot identifier — no native field for "curated bundle identifier"), `mikebom:hpack-source-detected` (FR-015 diagnostic — no native field for "operator-actionable build-tool-source warning"). All three documented in `docs/reference/sbom-format-mapping.md` Section I milestone-143 row. `mikebom:source-type` annotation reuses the milestone-122/137-142 prefixed convention with the `hackage-` prefix. `mikebom:lifecycle-scope` for test/benchmark/build-tool deps flows through the milestone-052 native-field path (CDX `scope` / SPDX 2.3 `*_DEPENDENCY_OF` / SPDX 3 `LifecycleScopeType`). |
| VI. Three-Crate Architecture | ✓ | All new code lives in `mikebom-cli`. No new workspace crate. |
| VII. Test Isolation | ✓ | Synthetic tempfile fixtures only; no host-state dependency. |
| VIII. Completeness | ✓ | Closes the Haskell gap entirely. Q1 chose informational annotation over emission-gating (boot libs still emit). Q2 chose union over single-stanza (recovers app-level dep visibility + dev-deps). Q3 chose detect-and-warn over silent ignore (operator-actionable stale-cabal signal). |
| IX. Accuracy | ✓ | PURL identity from lockfile fields directly; no heuristic guesses. Hackage names lowercased per spec. Snapshot resolver content preserved verbatim from `stack.yaml`'s `resolver:` field. Stackage-snapshot expansion deferred (FR-005) rather than synthesized inaccurately. |
| X. Transparency | ✓ | Per-file parse failures emit `tracing::warn!`. Source-type via `mikebom:source-type`. Hpack-source-detection diagnostic via FR-015. Design-tier mode emits `mikebom:sbom-tier = "design"` + `mikebom:requirement-range` evidence. Snapshot placeholder uses `pkg:generic/` (honest about loss of Hackage provenance for the bundle). |
| XII. External Data Source Enrichment | ✓ | The lockfiles + `*.cabal` files ARE the discovery sources. No external enrichment (license + Hackage API + Stackage snapshot expansion explicitly out of scope per spec Out-of-Scope). |

**Verdict: PASS.** No violations.

## Project Structure

### Documentation (this feature)

```text
specs/143-haskell-reader/
├── plan.md                        # THIS FILE
├── spec.md                        # with Q1+Q2+Q3 clarifications
├── research.md                    # Phase 0 — Haskell-specific decisions
├── data-model.md                  # Phase 1 — lockfile + *.cabal parsed shapes
├── quickstart.md                  # Phase 1 — operator scenarios
├── contracts/
│   └── haskell-component-purl.md  # PURL shape contract per FR-004 + FR-005 + FR-013
├── checklists/requirements.md     # 16/16 PASS (from /speckit-specify)
└── tasks.md                       # Phase 2 (created by /speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/src/
├── scan_fs/
│   ├── package_db/
│   │   ├── mod.rs                     # MODIFY: register haskell in read_all
│   │   ├── haskell.rs                 # NEW: cabal.project.freeze + stack.yaml.lock
│   │   │                              # + *.cabal + cabal.project + stack.yaml
│   │   │                              # parsing, main-module emission, source-type
│   │   │                              # discrimination, multi-package discovery,
│   │   │                              # design-tier fallback, Q1 GHC-stdlib allowlist,
│   │   │                              # Q2 multi-stanza union, Q3 Hpack detect-and-warn
│   │   ├── scala.rs                   # REFERENCE: milestone 142 — closest sibling
│   │   │                              # (multi-tier emission + main-module + design-tier
│   │   │                              # fallback + multi-project union discovery)
│   │   ├── erlang.rs                  # REFERENCE: milestone 141 — Q1 OTP-stdlib
│   │   │                              # allowlist template (Q1 here)
│   │   ├── elixir.rs                  # REFERENCE: milestone 140 — DSL regex extraction
│   │   │                              # template (mix.exs Elixir DSL → *.cabal Cabal DSL)
│   │   └── (no other scan_fs changes — haskell is purely additive)
│   └── walk.rs                        # UNCHANGED — safe_walk discovers
│                                       # *.cabal + cabal.project* + stack.yaml*
├── generate/cyclonedx/
│   ├── builder.rs                     # MODIFY: extend mikebom:evidence-kind
│   │                                  # allowlist to include "cabal-freeze",
│   │                                  # "stack-yaml-lock", "cabal-pkg-descriptor"
│   └── metadata.rs                    # (verify if main-module propagation
│                                      # needs new entries for mikebom:ghc-stdlib
│                                      # / mikebom:stackage-resolver — likely
│                                      # not since the main-module itself doesn't
│                                      # carry these annotations; verify during US2 test)
└── (no changes to other generate/, parity/, common/)

mikebom-cli/tests/
├── haskell_cabal_baseline.rs          # NEW: US1 — cabal.project.freeze baseline
├── haskell_stack_discrimination.rs    # NEW: US2 — Stack lockfile + snapshot placeholder
│                                       # + extra-deps + Q1 GHC-stdlib annotation
├── haskell_tier_fallbacks.rs          # NEW: US3 — design-tier + Q2 multi-stanza union
│                                       # + multi-package + dev-scope + Q3 Hpack-detect
└── haskell_edge_cases.rs              # NEW: malformed lockfile + flag constraints
                                       # + multiple *.cabal in one dir + main-module
                                       # fallback paths + boot-library allowlist match
```

**Structure Decision**: New file `haskell.rs` is a peer of cargo/dart/composer/cocoapods/gem/maven/golang/elixir/erlang/scala. Integration site is `read_all` dispatcher (placed alphabetically — after `gradle`/`golang` and before `kotlin_dsl`). Test files follow `<reader>_<scenario>.rs` convention. **No new workspace crate per Principle VI; no new Cargo deps.**

## Complexity Tracking

> No Constitution Check violations — no justifications required.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none)    | n/a        | n/a                                  |
