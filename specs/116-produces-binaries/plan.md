# Implementation Plan: Automatic binary-name binding via source-tier `produces-binaries` annotation

**Branch**: `116-produces-binaries` | **Date**: 2026-06-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/116-produces-binaries/spec.md`

## Summary

Two coordinated changes across mikebom's source-tier and cross-tier paths:

**Source-tier** — every per-ecosystem main-module extractor gains a small parsing step that reads the ecosystem's binary-name declaration (Cargo `[[bin]]` + `src/main.rs` + `src/bin/*.rs`; npm `bin`; pip `[project.scripts]`/`[project.gui-scripts]`; gem `executables`; maven shade-/jar-plugin `<finalName>`; Go `package main` directory walk) and stamps a new `mikebom:produces-binaries` property (JSON array of canonical extensionless lowercase strings, sorted+deduped, union-merged with operator pre-seeded values per FR-012) on the main-module component's `extra_annotations` map. The same `extra_annotations` channel that already carries `mikebom:component-role` (milestone 047/049 C40 pattern at `mikebom-cli/src/scan_fs/package_db/cargo.rs:363-368`) is reused — no new emission plumbing.

**Cross-tier** — `SourceSbomContext` (`mikebom-cli/src/binding/verify.rs:460-474`) gains a `binary_name_to_purl: HashMap<String, Purl>` index built during `load()` by scanning every component's `mikebom:produces-binaries` property. When `binding_for_purl()` at line 520 misses on exact PURL match AND the incoming PURL has the shape `pkg:generic/<name>` (with the FR-002 case-insensitive + `.exe`/`.jar`-suffix tolerance), the binder consults the index to find a source-side match. The existing milestone-111 `SourceDocumentBinding` envelope (`mikebom-cli/src/binding/mod.rs:187-217`) gains one new field — `alias_source: Option<AliasSource>` with variants `OperatorSupplied | AutomaticFromProducesBinaries` — recording the alias provenance. Operator-supplied aliases continue to take precedence (FR-004 + spec clarification Q3); the automatic-alias path is suppressed when an operator alias would produce the same image-side PURL match.

**Technical approach**: greenfield additions only — no behavior change for SBOMs that lack the declaration (FR-014 / SC-005 backwards-compat is total). Per-ecosystem extractors are independent of each other; US1 (Cargo) is the MVP slice, US2 (npm + pip + gem + maven) is the polyglot rollout, US3 (Go) is the harder filesystem-walk case. Constitution Principle V audit concluded NEGATIVE — no native CDX 1.6 or SPDX 2.3/3.x field expresses "this package produces these executable names" — so `mikebom:produces-binaries` is the canonical home (documented in `docs/reference/sbom-format-mapping.md` per Principle V).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–115; no nightly required for this user-space-only feature).
**Primary Dependencies**: Existing only — `toml = "0.8"` (already used by cargo + pip parsers), `quick-xml = "0.31"` (already used by maven), `serde`/`serde_json`, `tracing`, `anyhow`, `thiserror`, `clap`. Reuses milestone-114's `scan_fs::walk::safe_walk` for Cargo `src/bin/*.rs` enumeration (FR-005's third source) and Go `package main` directory walk (FR-010). Reuses milestone-111's `Purl` newtype + binding envelope plumbing. **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM via the existing `extra_annotations` channel + the extended `SourceDocumentBinding` envelope. No caches, no databases (matches every milestone since 002).
**Testing**: `cargo +stable test --workspace` (existing harness). New fixtures: per-ecosystem mini-projects under `mikebom-cli/tests/fixtures/produces_binaries/<ecosystem>/` (cargo / npm / pip / gem / maven; golang in US3 slice) — vendored locally, NOT pushed to the milestone-090 fixture repo (these are small synthetic projects, not external real-world corpora). Integration tests in `mikebom-cli/tests/produces_binaries_<ecosystem>.rs` cover source-tier emission shape + cross-tier auto-alias resolution + the operator-precedence rule per FR-004.
**Target Platform**: Linux x86_64 + macOS aarch64 + Windows x86_64 (same matrix as every milestone since 001). Per-ecosystem extraction is platform-independent (manifest parsing); the FR-002 `.exe` suffix tolerance is exercised by Linux runners reading Windows-shaped image-tier names.
**Project Type**: Single-project Rust CLI (`mikebom-cli/`).
**Performance Goals**: Per-ecosystem extraction is O(manifest-size + bin-dir-entry-count) — negligible relative to existing per-scan cost (typically <1 ms per main-module component). The `binary_name_to_purl` index build in `SourceSbomContext::load()` is O(source-tier-component-count); for a typical source SBOM with ~hundreds of components, well under 10 ms. No performance regression budget needed; we'll observe via the existing perf-bench gates.
**Constraints**: Backwards-compat is TOTAL per SC-005 / FR-014 — source SBOMs lacking the declaration MUST bind identically to the milestone-072 pre-feature behavior. The `SourceDocumentBinding` envelope extension MUST follow milestone-111's `#[serde(default, skip_serializing_if = "Option::is_none")]` paired-presence pattern so old SBOMs deserialize cleanly. Per spec clarification Q3, the declaration appears ONLY on main-module components.
**Scale/Scope**: 6 per-ecosystem extractors (cargo, npm, pip, gem, maven, golang); 1 binder extension (`SourceSbomContext` + `binding_for_purl`); 1 envelope-shape extension (`SourceDocumentBinding`); 1 docs update (`docs/reference/sbom-format-mapping.md` per Constitution Principle V). Diff size estimate: ~600 LoC production + ~1200 LoC tests + ~150 LoC docs.

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | std-only feature; no new C deps. |
| II. eBPF-Only Observation | N/A | `sbom scan` path; trace path is untouched. |
| III. Fail Closed | ✓ | No fallback to non-trace data; the produces-binaries declaration is enrichment over the existing main-module path, not a discovery substitute. Operator can't bypass the binder's exact-PURL match via produces-binaries — auto-alias only applies when exact match has already failed. |
| IV. Type-Driven Correctness | ✓ | New `AliasSource` enum (not `String`); reuse milestone-111's `Purl` newtype; binary-name strings carry no domain semantics so `Vec<String>` is fine (binary names are user-supplied identifiers, not parseable types). Zero new `.unwrap()` in production code. |
| V. Specification Compliance | ✓ | Constitution Principle V bullet 5 audit COMPLETED — no native CDX 1.6 or SPDX 2.3/3.x field expresses "this package produces these executable names." Closest CDX neighbor is `externalReferences[type=executable]` but that field carries URLs (homepage/distribution/VCS), not output-binary names — semantically wrong shape. SPDX `Package.annotation[]` is the documented extensibility point for novel semantics (same path C1–C45 already follow). Therefore `mikebom:produces-binaries` is justified as a parity-bridging annotation per Principle V bullet 5. The audit result + this justification MUST be added to `docs/reference/sbom-format-mapping.md` as part of this PR's docs slice. |
| VI. Three-Crate Architecture | ✓ | Lives entirely in `mikebom-cli/`; no new crates. |
| VII. Test Isolation | ✓ | Pure-logic + filesystem-fixture tests; no eBPF, no privileged operations. New fixtures vendored locally, NOT in the milestone-090 fixture repo. |
| VIII. Completeness | ✓ | Library-only crates correctly emit no declaration (FR-005 / FR-008); absence is correct, not a gap. Backwards-compat (FR-014 / SC-005) preserves the existing binding behavior. |
| IX. Accuracy | ✓ | Per FR-013, name-collision across multiple source components produces `weak` strength + `multiple-source-candidates-for-binary-name` reason — the binder never silently picks one. |
| X. Transparency | ✓ | FR-003's `alias_source` field gives auditors the binding provenance; operators and downstream tools can distinguish `--pkg-alias`-supplied from automatic-from-produces-binaries via a machine-readable field. |
| XI. Enrichment | ✓ | Auto-alias IS enrichment of the existing milestone-072 binding result. Does not introduce new components (Strict Boundary 1). |
| XII. External Data Source Enrichment | N/A | All extraction is from local manifests; no registry queries, no API calls. |
| Strict Boundary 1 (no lockfile-based discovery) | ✓ | Produces-binaries is read from MANIFEST (Cargo.toml, package.json, pyproject.toml, gemspec, POM) and from FILESYSTEM LAYOUT (Cargo `src/bin/*.rs`, Go `package main` dirs) — neither is a lockfile. The auto-alias path NEVER introduces a new component into the image-tier SBOM; it only changes how an existing image-tier component matches against the source-tier set. |
| Strict Boundary 2 (no MITM) | N/A | |
| Strict Boundary 3 (no C code) | ✓ | |
| Strict Boundary 4 (no `.unwrap()` in production) | ✓ | All new error paths use `?` + `Option::map_or` over manifest-parse failures (matches existing pattern). |

**Result**: Constitution Check PASSES. No violations. Principle V bullet 5 audit cited in research.md § Decision 1 + replicated into `docs/reference/sbom-format-mapping.md` per the principle's documentation requirement.

## Project Structure

### Documentation (this feature)

```text
specs/116-produces-binaries/
├── plan.md              # This file
├── research.md          # Phase 0 — 8 implementation decisions (field name, shape, binder index, envelope extension, Cargo src/bin walk, PR-split strategy, suffix-tolerance scope, env-var-parity)
├── data-model.md        # Phase 1 — ProducesBinariesDeclaration, AliasSource enum, binary_name_to_purl index, invariants + serde shape
├── quickstart.md        # Phase 1 — "operator removes --pkg-alias from CI" / "contributor adds a new ecosystem"
├── contracts/
│   ├── property.md      # The `mikebom:produces-binaries` source-tier property shape contract
│   └── binder.md        # The cross-tier auto-alias derivation + alias_source provenance contract
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── binding/
│   │   ├── mod.rs                                    # EXTENDED — SourceDocumentBinding gains alias_source field
│   │   ├── verify.rs                                 # EXTENDED — SourceSbomContext gains binary_name_to_purl index; binding_for_purl gains auto-alias resolution
│   │   ├── alias.rs                                  # UNCHANGED — milestone-111 parser stays as-is
│   │   └── annotation.rs                             # UNCHANGED — envelope serialization handles new optional field via existing serde plumbing
│   ├── scan_fs/
│   │   └── package_db/
│   │       ├── cargo.rs                              # EXTENDED — build_cargo_main_module_entry() parses [[bin]] + src/main.rs + src/bin/*.rs (US1)
│   │       ├── npm/walk.rs                           # EXTENDED — build_npm_main_module_entry() parses package.json's bin field (US2)
│   │       ├── pip/mod.rs                            # EXTENDED — build_pip_main_module_entry() parses [project.scripts] + [project.gui-scripts] (US2)
│   │       ├── gem.rs                                # EXTENDED — gemspec extractor parses executables array (US2)
│   │       ├── maven.rs                              # EXTENDED — build_maven_main_module_entry() parses shade-/jar-plugin <finalName> (US2)
│   │       └── golang/legacy.rs                      # EXTENDED — build_main_module_entry() walks for package main directories (US3)
│   └── cli/
│       └── scan_cmd.rs                               # UNCHANGED — attach_bindings_to_components reuses the same path; the new auto-alias logic lives behind binding_for_purl
├── tests/
│   ├── produces_binaries_cargo.rs                    # NEW — US1 end-to-end test (source-tier emission + cross-tier auto-alias resolution)
│   ├── produces_binaries_npm.rs                      # NEW — US2 npm slice
│   ├── produces_binaries_pip.rs                      # NEW — US2 pip slice
│   ├── produces_binaries_gem.rs                      # NEW — US2 gem slice
│   ├── produces_binaries_maven.rs                    # NEW — US2 maven slice
│   ├── produces_binaries_golang.rs                   # NEW — US3 golang slice
│   ├── produces_binaries_backcompat.rs               # NEW — SC-005 verification: SBOM without declaration still binds via exact PURL match
│   └── fixtures/
│       └── produces_binaries/
│           ├── cargo/                                # NEW — minimal Rust project + expected source-tier SBOM + image-tier scan fixture
│           ├── npm/                                  # NEW (US2)
│           ├── pip/                                  # NEW (US2)
│           ├── gem/                                  # NEW (US2)
│           ├── maven/                                # NEW (US2)
│           └── golang/                               # NEW (US3)
docs/
└── reference/
    └── sbom-format-mapping.md                        # EXTENDED — adds row for mikebom:produces-binaries with Principle V audit citation
mikebom-common/                                        # UNCHANGED
mikebom-ebpf/                                          # UNCHANGED
```

**Structure Decision**: Single-project layout (every milestone since 001). The binding extension lives in `mikebom-cli/src/binding/` alongside milestone-072 + milestone-111 code; each per-ecosystem extension lives in its existing per-ecosystem `package_db/<ecosystem>` module so the diff stays touching one place per US slice. Tests are per-US-slice integration tests under `mikebom-cli/tests/`. Fixtures vendored under `mikebom-cli/tests/fixtures/produces_binaries/<ecosystem>/` to keep them in-tree (small enough; not external real-world corpora that would benefit from milestone-090's fixture-repo cache).

## PR-split strategy

Per research.md § Decision 6, this feature ships as **three sequential PRs** matching the US priorities:

- **PR-A (US1, MVP)**: Foundation + Cargo. Ships the `SourceDocumentBinding.alias_source` envelope extension, the `SourceSbomContext.binary_name_to_purl` index, the auto-alias resolution in `binding_for_purl()`, AND the Cargo per-ecosystem extractor. Backwards-compat test (`produces_binaries_backcompat.rs`) lands here. After PR-A merges, the issue body's textbook Rust workflow is unblocked end-to-end.
- **PR-B (US2)**: npm + pip + gem + maven extractors. Each ecosystem is a separate per-ecosystem fixture + test file; the binder is unchanged from PR-A. Internally sequenceable as four byte-identity-preserving slices if the diff gets too large to review in one pass.
- **PR-C (US3)**: Go extractor. Adds the `package main` directory walk (using `safe_walk` per milestone 114) inside the milestone-053 main-module extractor.

This matches the milestone-110 multi-PR / milestone-102 PR-A+PR-B / milestone-114 single-PR-with-internal-checkpoints precedents. The first PR is sufficient by itself to close the issue's textbook case; later PRs extend coverage without changing the contract.

## Complexity Tracking

No constitution violations. No complexity to justify. The Principle V audit (FR-011) is documented in research.md § Decision 1 and again in `docs/reference/sbom-format-mapping.md` per the principle's documentation requirement.
