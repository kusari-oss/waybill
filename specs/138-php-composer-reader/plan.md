# Implementation Plan: PHP/Composer ecosystem reader

**Branch**: `138-php-composer-reader` | **Date**: 2026-06-23 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/138-php-composer-reader/spec.md`

## Summary

Twelfth language-ecosystem reader added to mikebom (joins cargo, npm, pip, gem, maven, golang, nuget, swift, kotlin, conan, dart). Parses Composer 2.x's three input artifacts — `composer.lock` (source-tier, lockfile-pinned), `composer.json` (design-tier fallback), and `vendor/composer/installed.json` (deployed-tier, multi-layer container scans) — and emits one main-module component per `composer.json` (FR-012) plus one component per lockfile/installed.json entry. Source-discriminator handling per FR-003: Packagist gets bare `pkg:composer/<vendor>/<package>@<version>` (or `?repository_url=` for self-hosted), VCS gets `?vcs_url=git+...`, path falls back to `pkg:generic/<vendor>-<package>` placeholder, and composer-plugin/metapackage carries the standard PURL with a `composer-plugin`/`composer-metapackage` source-type annotation. Lockfile-vs-disk drift (orphan installed.json entries) emits with `mikebom:lockfile-orphan = true` annotation per Q1 clarification. `serde_json` is already pervasive workspace dep; zero new Cargo dependencies.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–137; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `serde`/`serde_json` (Composer artifacts are JSON; pervasive workspace dep), `mikebom_common::types::hash::ContentHash` (FR-013 SHA-1 emission; already uses `sha2` + `data-encoding` transitively), `tracing` (warn-and-skip per FR-008), `anyhow`/`thiserror` (error propagation), `mikebom_common::types::purl::Purl` (PURL construction + validation; the `composer` type is purl-spec-blessed). **No new Cargo dependencies.**

**Storage**: N/A — all state is in-process for the duration of a single scan. Mirrors every language-reader since milestone 002.

**Testing**: `cargo +stable test --workspace`. Synthetic-fixture pattern via `tempfile::tempdir()` constructing minimal `composer.json` + `composer.lock` + `vendor/composer/installed.json` trees. Four new integration test files at `mikebom-cli/tests/composer_*.rs` mirroring the milestone-137 dart_*.rs family. SC-004 byte-identity preservation guarded by the existing 11-ecosystem golden suite (no Composer project present → those goldens stay unchanged).

**Target Platform**: Cross-platform reader. Same cargo/dart/maven precedent applies — mikebom's host portability is independent of the scanned target's OS. The reader is pure-Rust JSON parsing.

**Project Type**: CLI tool — extends the `mikebom sbom scan` pipeline via the `read_all` dispatcher.

**Performance Goals**: ≤2 ms overhead per lockfile entry on the read path; ≤500 ms for a heavy Laravel/Symfony app (~200 deps in a typical CMS-or-framework Composer project). The no-Composer-detected fast path (walker doesn't find any `composer.json` / `composer.lock` / `vendor/composer/installed.json`) MUST add ≤5 µs per non-Composer scan.

**Constraints**:
- Byte-identical SBOM goldens when no Composer project present (SC-004).
- Zero new Cargo deps (matches the milestone-002 / 064 / 066 / 068 / 069 / 070 / 122 / 137 reader posture).
- Per-file JSON parse failures MUST warn-and-skip, not fail the scan (FR-008). When `composer.lock` malforms but sibling `composer.json` exists, fall back to design-tier per FR-005.
- The `composer` PURL type IS purl-spec-blessed — no informal-type follow-up needed (unlike `brew` in milestone 136).
- Workspace structure NOT represented in SBOM — one main-module per `composer.json`, no synthetic monorepo-root (per FR-010).
- Vendor + name segments MUST be lowercased per purl-spec `composer-definition.md` canonical form.

**Scale/Scope**: Typical Laravel app: 50–150 direct + transitive deps. Heavy Symfony app: ~200–300 (with framework subcomponents). Composer monorepo with 5 members × 80 deps each: ~400 unique components after dedup. Per-lockfile JSON parse: ~2–5 ms warm-cache.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Justification |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | All new code is user-space Rust; no FFI, no C. JSON parsing via `serde_json` (pure-Rust crate already pervasive in the workspace). |
| II. eBPF-Only Observation | N/A | This reader processes manifest/lockfile metadata for components ALREADY declared in source-tree files; no new dependency-discovery surface. Matches every language reader (cargo/npm/pip/gem/maven/golang/dart/etc.). |
| III. Fail Closed | ✓ | A source tree without any of `composer.lock` / `composer.json` / `vendor/composer/installed.json` is a clean no-op (FR-007), NOT a fail-closed condition. Per-file parse failures warn-and-skip (FR-008). |
| IV. Type-Driven Correctness | ✓ | Uses the existing `Purl` newtype + `ContentHash` newtype; no stringly-typed identifiers. `composer.lock`'s `license:` field is polymorphic (string OR array per composer-schema.json) — handled via `#[serde(untagged)]` enum. Production code MUST NOT call `.unwrap()` — error propagation via `Result`. |
| V. Specification Compliance | ✓ | **`composer` IS a purl-spec-defined type** ([composer-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/composer-definition.md)). Vendor + name lowercased per spec. Path-sourced placeholder uses `pkg:generic/<vendor>-<package>` + `mikebom:source-type = "composer-path"` annotation as the discriminator — annotation is a PARITY-BRIDGE per Principle V. The `repository_url=` qualifier is a parity-bridge per Phase 0 research correction (purl-spec doesn't bless composer-specific qualifier names, but allows generic qualifiers; symmetric with milestone-137's pub-definition usage). `mikebom:source-type` annotation reuses C1; no new C-row for identity. Documented in research §R1 + spec Clarifications. |
| VI. Three-Crate Architecture | ✓ | All new code lives in `mikebom-cli`. No new workspace crate. Reader is a peer of `cargo.rs` / `dart.rs` / `gem.rs` / `maven.rs` / `golang.rs` under `mikebom-cli/src/scan_fs/package_db/`. |
| VII. Test Isolation | ✓ | Synthetic tempfile fixtures only; no host-state dependency. Pure-Rust JSON parsing — runs on any host. |
| VIII. Completeness | ✓ | This feature IS a completeness improvement — eliminates the false-negative gap where every Composer-managed dep (the ~78%-of-the-web ecosystem) was invisible. The Q1 clarification — emitting orphan installed.json entries with `mikebom:lockfile-orphan = true` — explicitly chose completeness over silent omission. |
| IX. Accuracy | ✓ | PURL identity comes directly from on-disk lockfile / manifest / installed.json fields; no heuristic guesses. Vendor + name lowercased per purl-spec canonical form. Composer-plugin / metapackage types surface via the `mikebom:source-type` annotation rather than collapsing into runtime deps. |
| X. Transparency | ✓ | Per-file parse failures (FR-008) emit `tracing::warn!` with the affected file path. Source-type discriminator surfaces via the standard `mikebom:source-type` evidence property. Lockfile-vs-disk drift surfaces via `mikebom:lockfile-orphan = true` annotation. No silent drops. |
| XII. External Data Source Enrichment | ✓ | The lockfile + manifest + installed.json ARE the discovery sources — same posture as cargo/npm/pip/gem/maven/dart. No external enrichment in this feature (license + Packagist API explicitly out of scope per spec). |

**Verdict: PASS.** No violations, no justifications required.

## Project Structure

### Documentation (this feature)

```text
specs/138-php-composer-reader/
├── plan.md              # This file
├── spec.md              # Feature spec (already written; corrected post-Phase-0)
├── research.md          # Phase 0 output — purl-spec composer audit + composer.lock/installed.json schemas + reader pattern + error posture
├── data-model.md        # Phase 1 output — ComposerJson + ComposerLock + LockfilePackage + InstalledJson + PackageDbEntry mapping
├── quickstart.md        # Phase 1 output — operator-facing walkthrough (7 scenarios)
├── contracts/           # Phase 1 output — wire-format contract
│   └── composer-component-purl.md
├── checklists/
│   └── requirements.md  # Spec-quality checklist (already written; 16/16 PASS)
└── tasks.md             # Phase 2 output (via /speckit.tasks)
```

### Source Code (repository root)

```text
mikebom-cli/src/
├── scan_fs/
│   ├── package_db/
│   │   ├── mod.rs                     # MODIFY: register composer in read_all dispatcher
│   │   ├── composer.rs                # NEW: composer.json + composer.lock + installed.json
│   │   │                              # parsing, main-module emission, source-type
│   │   │                              # discrimination, design+deployed-tier fallbacks,
│   │   │                              # orphan-installed.json detection
│   │   ├── dart.rs                    # REFERENCE: milestone 137 main-module + source-type
│   │   │                              # prefixed-value precedent
│   │   ├── cargo.rs                   # REFERENCE: milestone 064 main-module + workspace pattern
│   │   └── maven.rs                   # REFERENCE: language-reader source-tree walk pattern
│   └── (no other scan_fs changes — composer is purely additive)
├── generate/cyclonedx/builder.rs       # MODIFY: extend mikebom:evidence-kind enum to
│                                       # include "composer-lock", "composer-json",
│                                       # "composer-installed-json"
└── (no changes to other generate/, parity/, common/)

mikebom-cli/tests/
├── composer_laravel_baseline.rs       # NEW: US1 — composer.json + composer.lock fixture
├── composer_source_discriminators.rs  # NEW: US2 — packagist + vcs + path + plugin fixture
├── composer_tier_fallbacks.rs         # NEW: US3 — design-tier (no lockfile) + deployed-tier
│                                       # (installed.json only) + lockfile+installed drift
└── composer_edge_cases.rs             # NEW: malformed JSON + monorepo + missing name +
                                       # missing version + license polymorphism + Composer-1
                                       # installed.json warn-and-skip + multi-layer dedup
```

**Structure Decision**: Extends the existing `mikebom-cli/src/scan_fs/package_db/` reader family with a new language-ecosystem reader. New file `composer.rs` is a peer of cargo/dart/npm/pip/gem/maven/golang/nuget/swift/kotlin/conan. Integration site is the existing `read_all` dispatcher; no file-claim tracker integration (language readers don't claim binary paths). Test files follow the existing `<reader>_<scenario>.rs` integration-test naming convention. **No new workspace crate per Principle VI; no new Cargo deps; no new annotation in the parity catalog for identity** (source-type discriminator reuses existing C1 row; the new `mikebom:lockfile-orphan` annotation is a deferred parity-catalog refresh).

## Complexity Tracking

> No Constitution Check violations — no justifications required.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none)    | n/a        | n/a                                  |
