# Implementation Plan: Ruby built-in gem edges surfaced as SBOM components

**Branch**: `162-ruby-built-in-gems` | **Date**: 2026-07-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/162-ruby-built-in-gems/spec.md`

## Summary

Milestone-157 Round-2 audit against `kusari-sandbox/test-rails` surfaced that mikebom silently drops 1 of 250 `Gemfile.lock` edges (0.40%) — specifically edges to Ruby **built-in gems** (`bundler`, `bigdecimal`, `csv`, `logger`, `openssl`, etc.) that ship with the Ruby toolchain and are NOT tracked in `Gemfile.lock`'s GEM/specs section. The graph resolver's dangling-target-drop pass at emission time removes the edge without any operator-visible signal — Constitution Principle VIII (Completeness) violation.

Concrete example: `bundler-audit (0.9.3)` declares `bundler (>= 1.2.0)` as a dep, but `bundler` is not in GEM/specs (it's Ruby-toolchain-provided). Currently mikebom emits `bundler-audit@0.9.3.dependsOn = ["pkg:gem/thor@1.4.0"]` — the `bundler` edge is missing entirely.

**Technical approach** (per Q1 + Q2 clarifications):

1. **Static allowlist of Ruby built-in gems** — union across Ruby 3.2, 3.3, 3.4 stable-release `Gem::default_gems` outputs. In-source `const &[&str]` array at `mikebom-cli/src/scan_fs/package_db/gem.rs`. Union strategy (per Q2) keeps older-Ruby projects covered.
2. **Two-pass emission** — after the existing per-spec emission loop in `gem::read`, walk every entry's `depends` list; for each dep-name that is (a) in the allowlist, (b) NOT already emitted as a real component (FR-004 real-gem-precedence), emit a synthetic `PackageDbEntry` with versionless PURL + 2 new annotations.
3. **Versionless PURL construction** — `pkg:gem/<name>` (no `@version` segment) per Q1 + FR-003. `Purl::new("pkg:gem/bundler")` is spec-compliant.
4. **Per-component annotations**:
   - `mikebom:synthetic-built-in = "ruby"` (closed 1-value vocab in scope)
   - `mikebom:built-in-requirement = "<req>"` — the version constraint from `Gemfile.lock` (e.g., `>= 1.2.0`). Multi-source case: JSON array of constraints (matches milestone-159 multi-alias precedent).
5. **Parser preservation** — extend `GemSpec.depends` shape (currently `Vec<String>` of bare names) to preserve the version-constraint per dep. Alternative: sidecar `HashMap<(source_name, dep_name), constraint>` populated at parse time.

Q1 + Q2 clarifications (spec §Clarifications) lock: synthetic component with versionless PURL + annotations, union-of-Ruby-3.2/3.3/3.4 allowlist strategy.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–161; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `mikebom_common::types::purl::{Purl, encode_purl_segment}` (already used by `gem.rs::build_gem_purl`), `serde`/`serde_json` (annotation values), `tracing` (FR-011 log), `anyhow`/`thiserror` (error propagation). **Zero new Cargo dependencies.** The allowlist is a `const &[&str]` array literal.

**Storage**: N/A — all state in-process per scan; matches every milestone since 002. The allowlist lives in the compiled binary; the synthetic components live in the `Vec<PackageDbEntry>` returned by `gem::read`.

**Testing**: `cargo +stable test --workspace --no-fail-fast` per Constitution Development Workflow. New tests live in three tiers per milestone-055/091/158/160/161 precedent:
- Unit tests in `mikebom-cli/src/scan_fs/package_db/gem.rs` (module-inline `#[cfg(test)]`) covering allowlist membership + synthetic component construction + FR-004 real-gem-precedence dedup.
- Integration test at `mikebom-cli/tests/ruby_built_in_gems.rs` (per SC-010) with a synthetic Gemfile.lock referencing `bundler-audit → bundler` exercising the release binary end-to-end.
- SC-001 audit-fixture regression — the existing milestone-090 `gem` fixture will change if its Gemfile.lock references any allowlist gems (verified at Phase 5).

**Target Platform**: Linux + macOS + Windows dev hosts (per milestones 100/101). No platform-specific behavior — pure in-process emission.

**Project Type**: CLI (Rust workspace with 3 crates per Constitution Principle VI).

**Performance Goals**: Preserve existing gem-reader posture — O(gems × avg-deps-per-gem) allowlist lookup on each dep-name at emission time. For test-rails (~150 gems × ~5 deps avg = ~750 lookups against a ~40-entry allowlist), sub-millisecond total overhead. No performance concerns.

**Constraints**: **No new Cargo dependencies** (FR spec assumption). **No `.unwrap()` in production** per Constitution Principle IV. **Standards-native precedence** per Principle V — FR-009 documents the audit result (no CDX/SPDX-native synthetic-component field as of 2026-07-04).

**Scale/Scope**: `test-rails` has ~150 gems and 1 known built-in dep-name (`bundler`). Real-world gem projects vary widely — some (Ruby monorepos) may have hundreds of gems with 5-10 built-in refs; the emitted-synthetic-component count is bounded by the allowlist size (~40). Golden regeneration impact: 10 non-`gem` milestone-090 fixtures × 3 formats = 30 goldens byte-identical (SC-003); the `gem` fixture goldens MAY change if its Gemfile.lock references any allowlist gems (verified separately during Phase 5).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle-by-principle assessment

**I. Pure Rust, Zero C** — ✅ PASS. All work is user-space Rust in `mikebom-cli`. No FFI. No C dependencies added. `mikebom-ebpf` untouched.

**II. eBPF-Only Observation** — ✅ N/A. Milestone 162 does not touch discovery. Gemfile.lock parsing continues via the existing static reader (permitted by Constitution Strict Boundary §1 — enrichment path, not primary discovery).

**III. Fail Closed** — ✅ PASS. Allowlist misses (Ruby version introduces a new built-in gem not yet in the union list) continue to silently drop the edge — the same behavior as pre-162 for non-built-in gems. Adding new gems to the allowlist requires an explicit milestone bump per FR-006; no silent behavior change.

**IV. Type-Driven Correctness** — ✅ PASS. New `SyntheticGemKind` enum with single variant `RubyBuiltIn` (extensible for future language runtimes). All new annotation values are string-typed matching the milestone-159 C106/C107 shape. No `.unwrap()` in production paths.

**V. Specification Compliance** — ⚠️ GATE. Two audit checks required:
- **Native-first check**: FR-009 explicitly documents the audit — no CDX 1.6 or SPDX 3.0.1 native field for "synthetic/inferred component" as of 2026-07-04. The `mikebom:synthetic-built-in` prefix is compliant per Principle V's "parity-bridging" clause.
- **Existing-mikebom-annotation check**: no existing per-component annotation carries "component was synthesized from a dropped edge because target is toolchain-provided" semantic. C104/C108/C110/C112 are all doc-scope or ecosystem-specific and orthogonal to this concept.

**VI. Three-Crate Architecture** — ✅ PASS. All changes are in `mikebom-cli`; no new crates.

**VII. Test Isolation** — ✅ PASS. New unit tests are pure logic (allowlist membership + dedup rules). Integration test uses a synthetic Gemfile.lock in a tempdir — no eBPF privilege.

**VIII. Completeness** — ✅ CENTRAL. Milestone 162 directly addresses the Completeness gap discovered in the milestone-157 audit (0.4% edge coverage loss on test-rails). Every previously-silently-dropped built-in edge now surfaces as either a real edge or a documented allowlist miss.

**IX. Accuracy** — ✅ CENTRAL. The versionless PURL (per Q1) avoids false-positive CVE matches — a critical accuracy signal for downstream vulnerability scanners. Emitting `pkg:gem/bundler@v0.0.0-guessed` would be a Principle-IX violation; versionless PURL is spec-compliant + accuracy-preserving.

**X. Transparency** — ✅ CENTRAL. The `mikebom:synthetic-built-in = "ruby"` annotation is the explicit consumer signal that this component was synthesized (not observed from GEM/specs). Consumers doing PURL-based lookups match on gem-name; consumers doing evidence-based reasoning distinguish real from synthetic.

**XI. Enrichment** — ✅ N/A. Milestone 162 does not fetch new external data.

**XII. External Data Source Enrichment** — ✅ N/A. No new external source.

### Strict Boundary compliance

**§1 (No lockfile discovery)** — ✅ N/A. Gemfile.lock is used only for ENRICHMENT (adding dep-graph edges to already-observed gems). Milestone 162 does not add lockfile-based discovery — synthetic built-in components are inferred from EDGE TARGETS declared in Gemfile.lock, and only for known Ruby-toolchain-provided gems.

**§2 (No MITM proxy)** — ✅ PASS.

**§3 (No C code)** — ✅ PASS.

**§4 (No `.unwrap()` in production)** — ✅ PASS. New code follows the milestone-055/091/160/161 pattern with `anyhow::Result` + `?` propagation.

**§5 (No file-tier duplicates in default mode)** — ✅ N/A. File-tier emission not touched.

### Gate result

Constitution Check **PASSES** — no violations. All principles + boundaries compliant.

## Project Structure

### Documentation (this feature)

```text
specs/162-ruby-built-in-gems/
├── plan.md              # This file
├── research.md          # Phase 0 output (R1–R6 below)
├── data-model.md        # Phase 1 output (entities: allowlist, synthetic-component, requirement-string)
├── quickstart.md        # Phase 1 output (contributor path: build+test)
├── contracts/
│   └── annotations.md   # Phase 1 output (per-format wire shape for C113/C114)
├── checklists/
│   └── requirements.md  # Already exists from /speckit-specify
└── tasks.md             # /speckit-tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       └── gem.rs                    # EDIT: add BUILT_IN_ALLOWLIST const; extend read() with second-pass synthetic emission; extend parser to preserve requirement strings on GemSpec.depends
│   └── parity/
│       └── extractors/
│           ├── mod.rs                    # EDIT: register C113 + C114 rows
│           ├── cdx.rs                    # EDIT: cdx_anno! invocations
│           ├── spdx2.rs                  # EDIT: spdx23_anno! invocations
│           └── spdx3.rs                  # EDIT: spdx3_anno! invocations
└── tests/
    └── ruby_built_in_gems.rs             # NEW: SC-010 integration test (synthetic Gemfile.lock with bundler-audit → bundler)
```

**Structure Decision**: Milestone 162 is a targeted extension of the existing `gem.rs` reader plus 2 new parity-catalog row registrations. No new files in `src/`. One new integration test file. This is the smallest source-tree footprint of the milestone-160/161/162 audit-round-2 series.

## Complexity Tracking

*No Constitution violations. Section not applicable.*

## Phase completion status

- ✅ **Phase 0 (research)** — see `research.md` for R1–R6 resolutions.
- ✅ **Phase 1 (design & contracts)** — see `data-model.md`, `contracts/annotations.md`, `quickstart.md`.
- 🔲 **Phase 2 (task decomposition)** — deferred to `/speckit-tasks`.

## Post-design constitution re-check

Post-design re-check passes. R1 confirms the semantic distinction between C113/C114 and existing catalog rows (all doc-scope or ecosystem-specific — this milestone's C113/C114 are per-component + Ruby-specific but with a designed extension path). R4 (multi-source requirement collision) resolves the design ambiguity — JSON array of constraints matches milestone-159 C106/C107 multi-alias precedent.

## Notes

- The plan is genuinely simpler than milestones 160 + 161 — no empirical investigation loop needed. The fix shape is fully specified at plan time.
- No fixture-cache dependency for the P1 MVP — the SC-010 integration test uses a synthesized minimal Gemfile.lock; the SC-001 audit against `test-rails` is opportunistic (via a new gated test at `mikebom-cli/tests/gem_built_in_audit.rs`) but is not blocking for the impl PR.
- Preserve milestone-051's dev-scope classification behavior unchanged — synthetic built-in components inherit the lifecycle scope of the source's declaration (typically Runtime for prod-reachable built-ins).
- The `mikebom:synthetic-built-in` value vocab is CLOSED at `"ruby"` for this milestone; future extensions (other language runtimes with equivalent patterns) require a spec-milestone bump.
