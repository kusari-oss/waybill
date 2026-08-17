# Implementation Plan: Universalize `waybill:unresolved-reason` per-component annotation

**Branch**: `236-unresolved-reason` | **Date**: 2026-08-16 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/236-unresolved-reason/spec.md`

## Summary

Extend the NuGet-established `waybill:unresolved-reason` per-component annotation pattern to the other 17 design-tier-emitting readers. Every reader that today emits `waybill:sbom-tier: "design"` will additionally attach `waybill:unresolved-reason` at the same call-site via the existing `PackageDbEntry.extra_annotations` channel, with a reader-specific human-readable reason string per Q1 clarification (display-only; byte-stable within a build, best-effort stable across releases).

**Technical approach**: locate every existing `sbom_tier = "design"` call-site (18 sites — NuGet is the reference); at each, inject a companion `extra_annotations.insert("waybill:unresolved-reason", ...)` with the reader-specific reason string; register a single m071 parity extractor row (C-next-available) covering all readers uniformly; ship per-reader unit tests + a cross-reader integration test.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–235; no nightly required for this user-space-only feature).
**Primary Dependencies**: Existing only — `serde_json` (annotation value construction; pervasive), `tracing` (existing warn logs on the reader-side design-tier code paths), `anyhow`/`thiserror` (existing error propagation). Reuses the m071 parity extractor infrastructure (`parity/extractors/*.rs`) and the standard `PackageDbEntry.extra_annotations` channel. **Zero new Cargo dependencies.**
**Storage**: N/A — annotation is per-component in-process during a single scan; persisted only in the emitted SBOM.
**Testing**: `cargo test --workspace` — per-reader unit tests in each reader's `mod tests`; a cross-reader integration test in a new `waybill-cli/tests/unresolved_reason_universal.rs`.
**Target Platform**: same as m235 — Linux, macOS, Windows via CI matrix. No platform-specific code.
**Project Type**: waybill-cli internal (reader modifications). No CLI flag changes, no new subcommands.
**Performance Goals**: Negligible — one string insert per design-tier component; scan-wide impact undetectable.
**Constraints**:
- **Byte-identity on NuGet** — the existing NuGet reason string is preserved verbatim; a regression test asserts no drift (FR-006, SC-003).
- **No PII / paths / credentials in reason strings** — enforced by a substring-blacklist test over every reader's shipped reason strings (FR-010).
- **Cross-format parity** — annotation MUST appear in CDX + SPDX 2.3 + SPDX 3 emission byte-identically (SC-002).

**Scale/Scope**: 18 readers touched (NuGet regression-guard + 17 new emitters); ~30 lines per reader change on average; single m071 catalog row + extractor triple.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Applicable principles

- **I. Pure Rust, Zero C** — ✅ Rust-only; zero new deps. Constitution constraint fully met.
- **III. Fail Closed** — ✅ Emission is additive; if the reader itself fails elsewhere, the annotation just isn't emitted (matches NuGet precedent). Design-tier components already fail-open (versionless PURL rather than fabricating a version).
- **IV. Type-Driven Correctness** — 🟡 **Discussion**: NuGet today uses raw `String` for the annotation value (see `nuget/mod.rs:376-380`). Options: (a) keep raw strings; (b) introduce a `UnresolvedReason(String)` newtype in `waybill-common`. Decision in Phase 0 research §R1.
- **V. Specification Compliance (Standards-Native Audit)** — ✅ **KEEP-NO-NATIVE**: no CDX `evidence.identity`, SPDX 2.3 `Annotation.comment`, or SPDX 3 `LifecycleScope`-adjacent construct carries "human-readable reason a specific component couldn't be resolved". The `waybill:*` annotation is the right vehicle (matches C143/C145/C147/C148/C149/C150 precedent).
- **VI. Three-Crate Architecture** — ✅ All changes localized to `waybill-cli/src/scan_fs/package_db/<reader>.rs` + `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs` + `waybill-cli/tests/`. No `waybill-common` or `waybill-ebpf` touched (unless Principle IV opts for the newtype — R1 rejects).
- **VII. Test Isolation** — ✅ Per-reader unit tests use temp dirs (existing pattern in each reader's `mod tests`); cross-reader integration test uses `apply_fake_home_env` per m235 precedent.
- **VIII. Completeness** — ✅ This IS a completeness milestone (closes cross-reader gap flagged in issue #659).
- **IX. Accuracy** — ✅ Reinforces the accuracy-over-fabrication design: design-tier components already carry versionless PURLs; this annotation makes the reason accessible to human reviewers.
- **X. Transparency** — ✅ This IS a transparency milestone.

### Verdict

**Gate passes**. Principle IV discussion is a design-decision resolved in R1 (raw String, no newtype).

## Project Structure

### Documentation (this feature)

```text
specs/236-unresolved-reason/
├── plan.md              # This file
├── research.md          # Phase 0: R1-R4
├── data-model.md        # Phase 1: annotation entity + injection contract
├── contracts/           # Phase 1: annotation wire + per-reader strings
├── quickstart.md        # Phase 1: verify + add-new-reader recipe
└── tasks.md             # Phase 2: /speckit.tasks output
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   ├── scan_fs/package_db/
│   │   ├── cargo.rs                      # US1
│   │   ├── gem.rs                        # US1
│   │   ├── maven.rs                      # US1
│   │   ├── npm/mod.rs                    # US1
│   │   ├── npm/walk.rs                   # US1 (secondary design-tier path)
│   │   ├── pip/requirements_txt.rs       # US1
│   │   ├── nuget/mod.rs                  # (regression-guard only)
│   │   ├── kotlin_dsl/mod.rs             # US2
│   │   ├── kotlin_dsl/build_script.rs    # US2
│   │   ├── scala.rs                      # US2
│   │   ├── gradle/static_parser.rs       # US2
│   │   ├── helm.rs                       # US2
│   │   ├── yocto/recipe.rs               # US2
│   │   ├── cocoapods.rs                  # US3
│   │   ├── composer.rs                   # US3
│   │   ├── dart.rs                       # US3
│   │   ├── elixir.rs                     # US3
│   │   ├── erlang.rs                     # US3
│   │   ├── haskell.rs                    # US3
│   │   ├── pants_shell/component_emit.rs # US3
│   │   └── pants_go/mod.rs               # US3
│   └── parity/extractors/
│       ├── cdx.rs                        # add cN_cdx one-liner
│       ├── spdx2.rs                      # add cN_spdx23 one-liner
│       ├── spdx3.rs                      # add cN_spdx3 one-liner
│       └── mod.rs                        # register in EXTRACTORS array
└── tests/
    ├── unresolved_reason_universal.rs    # NEW: cross-reader integration
    └── fixtures/golden_inputs/
        └── unresolved_reason/             # NEW: per-reader minimal fixtures

docs/
└── reference/
    └── sbom-format-mapping.md            # add new C-row for waybill:unresolved-reason
```

**Structure Decision**: Single-crate change in `waybill-cli` (Option 1 from template). Each reader modification is self-contained; the parity extractor + catalog row is a shared cross-cutting concern that lands in one place.

## Phase 0: Outline & Research

### R1 — Type shape: raw `String` vs `UnresolvedReason` newtype

**Question**: Should the annotation value be a raw `String` (matching NuGet precedent) or a dedicated `waybill_common::UnresolvedReason` newtype (Principle IV domain-primitive)?

**Decision**: **Raw `String`**.
**Rationale**: Principle IV's newtype rule protects against mix-ups between domain primitives (e.g., raw `String` PURLs risking being passed where a raw `String` license expression is expected). Here the annotation value is a display-only human-readable string; there's no domain-primitive collision risk. A newtype adds friction (new type in `waybill-common`, `From`/`Display` impls, potential serde plumbing) without unlocking correctness. NuGet's raw-string precedent already exists in production per PR #656.
**Alternatives considered**:
- (a) `UnresolvedReason(String)` newtype in `waybill-common` — rejected as over-engineering per Principle IV's "primitive collision" test.
- (b) `UnresolvedReason` enum with per-reader variants — rejected because reason strings are open-ended (readers may add failure modes without a spec change per Q1 clarification's display-only contract).

### R2 — Per-reader reason string enumeration

**Question**: What's each reader's exact reason string?

**Decision**: Read each reader's existing design-tier emission call-site and name the resolution boundary in the reason. Draft table (final strings are contract per `contracts/per-reader-strings.md`):

| Reader | Reader file | Proposed reason string |
|---|---|---|
| cargo | `cargo.rs` | `"no matching entry in Cargo.lock"` |
| gem | `gem.rs` | `"no matching entry in Gemfile.lock"` |
| maven | `maven.rs` | `"no <version> in pom.xml; no dependency-reduced-pom.xml or effective-pom fallback"` |
| npm/mod | `npm/mod.rs` | `"no matching entry in package-lock.json / pnpm-lock.yaml / yarn.lock / bun.lock"` |
| npm/walk | `npm/walk.rs` | `"workspace member; no lockfile-resolved version"` |
| pip | `pip/requirements_txt.rs` | `"no version specifier in requirements.txt; no uv.lock / poetry.lock fallback"` |
| kotlin_dsl/mod | `kotlin_dsl/mod.rs` | `"Kotlin DSL declaration; --include-declared-deps enables emission; requires Gradle daemon for full resolution"` |
| kotlin_dsl/build_script | `kotlin_dsl/build_script.rs` | `"Kotlin DSL buildscript declaration; --include-declared-deps enables emission"` |
| scala | `scala.rs` | `"declared in build.sbt; no coursier-resolved lockfile"` |
| gradle_static | `gradle/static_parser.rs` | `"declared in build.gradle; US2 cache reader had no matching seed"` |
| helm | `helm.rs` | `"unrendered Chart.yaml dependency; --helm-render subprocess disabled or unavailable"` |
| yocto | `yocto/recipe.rs` | `"recipe .bb declaration; no PV/PR resolution"` |
| cocoapods | `cocoapods.rs` | `"no matching entry in Podfile.lock"` |
| composer | `composer.rs` | `"no matching entry in composer.lock"` |
| dart | `dart.rs` | `"no matching entry in pubspec.lock"` |
| elixir | `elixir.rs` | `"no matching entry in mix.lock"` |
| erlang | `erlang.rs` | `"no matching entry in rebar.lock"` |
| haskell | `haskell.rs` | `"declared in stack.yaml / .cabal; no stack.yaml.lock fallback"` |
| pants_shell | `pants_shell/component_emit.rs` | `"pants shell tool pin without version specifier"` |
| pants_go | `pants_go/mod.rs` | `"pants_go expected_version declared; no matching go corpus component"` |
| **nuget (regression)** | `nuget/mod.rs:376` | *preserved verbatim:* `"no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry"` |

**Rationale**: Each string names the specific resolution boundary the reader hit and the fallback mechanisms it tried. Every string is ASCII English (matches NuGet precedent), <200 chars, no PII/paths/credentials.
**Alternatives considered**: Terser strings ("no lockfile") rejected as insufficiently actionable for human reviewers per FR-002.

### R3 — Catalog row: new C-number vs extend existing NuGet row

**Question**: Does the m071 parity catalog already carry a C-row for `waybill:unresolved-reason`?

**Decision**: **Verify at task-time via grep**. Options:
- (a) A row exists (from PR #656) — this milestone extends the extractor's coverage from NuGet-only to all readers (no new row needed).
- (b) No row exists — this milestone lands the row + extractor triple (matches m235 C147/C148/C149/C150 pattern).

**Rationale**: The parity catalog row is a doc + extractor plumbing concern; the actual test signal is what matters. Either path lands the same wire behavior. Discovered at task-time via `grep -n "waybill:unresolved-reason" docs/reference/sbom-format-mapping.md`.

### R4 — Cross-reader integration test corpus

**Question**: What fixture corpus proves cross-reader universality (SC-001)?

**Decision**: 17 minimal per-reader fixtures under `tests/fixtures/golden_inputs/unresolved_reason/<reader>/` — each producing at least one design-tier component + carrying the reader's expected reason string. Cross-reader integration test scans a directory containing all 17 fixtures side-by-side and asserts every emitted design-tier component carries `waybill:unresolved-reason`.

**Rationale**: Reuses existing per-reader fixture patterns (matches m235 gradle_ladder + m226 pants_go patterns). Zero new test infrastructure. 17 fixtures × ~5 files each = ~85 files total (manageable). Every fixture uses synthetic package names per the `feedback_fixture_synthetic_package_names` memory.

**Alternatives considered**: Reuse existing fixtures from other milestones — rejected because they're not sized/scoped for this test (would drag in unrelated assertions). Minimal per-reader fixtures give tight test isolation.

## Phase 1: Design & Contracts

### 1. Data model

See `data-model.md` — one entity (`UnresolvedReason` string) + call-site injection contract.

### 2. Contracts

See `contracts/annotation-wire.md` — wire schema, insertion contract, presence conditional.
See `contracts/per-reader-strings.md` — the enumerated reason-string table from R2 as a locked contract.

### 3. Quickstart

See `quickstart.md` — verification recipe + add-new-reader checklist.

### 4. Agent context

Run `.specify/scripts/bash/update-agent-context.sh claude` after this plan is committed to add the milestone-236 line to `CLAUDE.md` Active Technologies (matches m235 precedent).

## Complexity Tracking

*None.* This milestone is a systematic cross-reader consistency pass with zero new dependencies, zero new PURL types, zero new subprocess calls, zero new network access. The pattern is directly established by NuGet.
