# Implementation Plan: CocoaPods ecosystem reader

**Branch**: `139-cocoapods-reader` | **Date**: 2026-06-23 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/139-cocoapods-reader/spec.md`

## Summary

Thirteenth language-ecosystem reader added to mikebom (joins cargo, npm, pip, gem, maven, golang, nuget, swift, kotlin, conan, dart, composer). Parses CocoaPods 1.0+ `Podfile.lock` (YAML, source-tier), `Podfile` (Ruby DSL regex-extracted, design-tier fallback), and `Pods/Manifest.lock` (deployed-tier when no sibling Podfile.lock per Q3). Emits one main-module component per project root (FR-012, with the Podfile→target-name OR dir-basename fallback cascade per Q1) plus one component per `PODS:` entry. Subspecs encode via the PURL `#subpath` mechanism per Phase 0 research correction (NOT a `?subspec=` qualifier — that was the initial spec guess; purl-spec authority overrules). Git-source pods get the resolved 40-char SHA from `CHECKOUT OPTIONS:` per Q2 + Phase 0 confirmation. Subspec entries look up SHA-1 by root pod name per Phase 0 correction (`SPEC CHECKSUMS:` is root-keyed). `serde_yaml = "0.9"` is already a workspace dep; zero new Cargo dependencies.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–138; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `serde`/`serde_yaml = "0.9"` (Podfile.lock + Manifest.lock are YAML; workspace dep per dart.rs + npm/yarn_lock.rs precedent), `serde_json` (annotation construction), `mikebom_common::types::hash::ContentHash` + `HashAlgorithm::Sha1` (FR-008 SHA-1 emission per milestone-138 precedent), `regex` (Podfile line-by-line `pod` / `target` extraction — workspace dep, already used by alpm + brew + yocto), `tracing` (warn-and-skip per FR-007), `anyhow`/`thiserror` (error propagation), `mikebom_common::types::purl::Purl` (PURL construction + validation; the `cocoapods` type is purl-spec-blessed). **No new Cargo dependencies.**

**Storage**: N/A — all state is in-process for the duration of a single scan. Mirrors every language-reader since milestone 002.

**Testing**: `cargo +stable test --workspace`. Synthetic-fixture pattern via `tempfile::tempdir()` constructing minimal `Podfile` + `Podfile.lock` (+ optional `Pods/Manifest.lock`) trees. Four new integration test files at `mikebom-cli/tests/cocoapods_*.rs` mirroring the milestone-138 composer_*.rs family. SC-004 byte-identity preservation guarded by the existing 11-ecosystem golden suite.

**Target Platform**: Cross-platform reader. Same dart/composer precedent applies — mikebom's host portability is independent of the scanned target's OS. The reader is pure-Rust YAML + regex parsing.

**Project Type**: CLI tool — extends the `mikebom sbom scan` pipeline via the `read_all` dispatcher.

**Performance Goals**: ≤2 ms overhead per PODS entry on the read path; ≤500 ms for a heavy Firebase-using iOS app (~150 pods after subspec expansion). The no-CocoaPods-detected fast path (walker doesn't find any `Podfile.lock` / `Podfile` / `Manifest.lock`) MUST add ≤5 µs per non-iOS scan.

**Constraints**:
- Byte-identical SBOM goldens when no CocoaPods project present (SC-004).
- Zero new Cargo deps.
- Per-file YAML parse failures MUST warn-and-skip; lockfile-malformed → fall back to design-tier from sibling `Podfile` per FR-007.
- The `cocoapods` PURL type IS purl-spec-blessed.
- Names are CASE-SENSITIVE per purl-spec — preserve lockfile case verbatim in both `name` field AND PURL identity (unlike Composer's lowercase requirement).
- Subspecs encode via PURL `#subpath` per purl-spec correction; SPEC CHECKSUMS lookup is ROOT-keyed.
- No `Pods/<pod>/` directory walking — out of spec scope.
- No Ruby DSL evaluation; regex-only Podfile parsing (matches gem reader posture).

**Scale/Scope**: Typical iOS app: 30–80 pods after subspec expansion. Heavy Firebase + GoogleSignIn + FBSDK app: ~150–250 pods. Per-Podfile.lock YAML parse: ~2–5 ms warm-cache.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Justification |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | All new code is user-space Rust; no FFI, no C. YAML parsing via `serde_yaml`; regex extraction via the workspace `regex` crate. |
| II. eBPF-Only Observation | N/A | Source-tree language reader; pre-existing discovery surface (every prior language reader operates the same way). |
| III. Fail Closed | ✓ | A source tree without any of `Podfile.lock` / `Podfile` / `Pods/Manifest.lock` is a clean no-op (FR-006), NOT a fail-closed condition. Per-file parse failures warn-and-skip (FR-007). |
| IV. Type-Driven Correctness | ✓ | Uses the existing `Purl` newtype + `ContentHash` newtype; no stringly-typed identifiers. `Podfile.lock`'s `PODS:` entries are heterogeneous (string OR map per Phase 0 research) — handled via post-parse `serde_yaml::Value` dispatch. Production code MUST NOT call `.unwrap()` — error propagation via `Result`. |
| V. Specification Compliance | ✓ | **`cocoapods` IS a purl-spec-defined type** ([cocoapods-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/cocoapods-definition.md)). Subspecs use the spec-mandated `#subpath` form (Phase 0 correction from the initial `?subspec=` guess). Path-sourced placeholder uses `pkg:generic/` + `mikebom:source-type = "cocoapods-path"` annotation as PARITY-BRIDGE per Principle V. **syft/trivy divergence note**: both syft and trivy fold subspec into the name (`pkg:cocoapods/Firebase/Database@1.0.0`) which is purl-spec-non-conformant. Per Principle V, mikebom emits the spec-conformant form; syft/trivy compatibility annotation (`mikebom:also-known-as`) deferred to v1.1. `mikebom:source-type` annotation reuses C1; no new C-row for identity. Documented in research §R1 + spec Phase 0 corrections. |
| VI. Three-Crate Architecture | ✓ | All new code lives in `mikebom-cli`. No new workspace crate. Reader is a peer of cargo / dart / composer / gem / maven / golang. |
| VII. Test Isolation | ✓ | Synthetic tempfile fixtures only; no host-state dependency. Pure-Rust parsing. |
| VIII. Completeness | ✓ | Closes the iOS-side gap symmetric to milestone 137's Dart/Flutter closure. The Q1 dir-basename fallback for no-Podfile cases + Q3 deployed-tier path both chose completeness over silent omission. |
| IX. Accuracy | ✓ | PURL identity comes directly from on-disk lockfile fields; no heuristic guesses. Names case-preserved per purl-spec. Subspec entries emit as distinct components (FR-003 + SC-009) rather than collapsing into parent pod. SHA-1 hashes from `SPEC CHECKSUMS:` flow into standards-native `hashes[]` array. Resolved git SHAs from `CHECKOUT OPTIONS:` flow into `mikebom:vcs-ref` annotation per Q2. |
| X. Transparency | ✓ | Per-file parse failures emit `tracing::warn!`. Source-type discriminator surfaces via the standard `mikebom:source-type` evidence property. Source-vs-deployed tier surfaces via the standard `mikebom:sbom-tier` property (per Q3 Manifest.lock-only handling). No silent drops. |
| XII. External Data Source Enrichment | ✓ | The lockfile + Podfile + Manifest.lock ARE the discovery sources — same posture as cargo/dart/composer. No external enrichment in this feature (license + spec-repo API explicitly out of scope per spec). |

**Verdict: PASS.** No violations, no justifications required.

## Project Structure

### Documentation (this feature)

```text
specs/139-cocoapods-reader/
├── plan.md
├── spec.md              # corrected post-Phase 0
├── research.md          # Phase 0 — 8 sections
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   └── cocoapods-component-purl.md
├── checklists/requirements.md  # 16/16 PASS
└── tasks.md             # Phase 2 via /speckit.tasks
```

### Source Code (repository root)

```text
mikebom-cli/src/
├── scan_fs/
│   ├── package_db/
│   │   ├── mod.rs                     # MODIFY: register cocoapods in read_all
│   │   ├── cocoapods.rs               # NEW: Podfile.lock + Podfile + Manifest.lock
│   │   │                              # parsing, main-module emission, source-type
│   │   │                              # discrimination, subspec subpath encoding,
│   │   │                              # design+deployed-tier fallbacks
│   │   ├── composer.rs                # REFERENCE: milestone 138 — multi-tier + SHA-1
│   │   ├── dart.rs                    # REFERENCE: milestone 137 — serde_yaml + prefixed
│   │   │                              # mikebom:source-type
│   │   └── gem.rs                     # REFERENCE: regex-extracted Ruby DSL parsing
│   └── (no other scan_fs changes — cocoapods is purely additive)
├── generate/cyclonedx/builder.rs       # MODIFY: extend mikebom:evidence-kind enum to
│                                       # include "cocoapods-podfile-lock",
│                                       # "cocoapods-podfile", "cocoapods-manifest-lock"
└── (no changes to other generate/, parity/, common/)

mikebom-cli/tests/
├── cocoapods_ios_app_baseline.rs       # NEW: US1
├── cocoapods_source_discriminators.rs  # NEW: US2
├── cocoapods_tier_fallbacks.rs         # NEW: US3 — design + deployed + Q1 dir-basename
└── cocoapods_edge_cases.rs             # NEW: malformed + multi-target + missing name +
                                       # subspec multi-level + CHECKOUT OPTIONS + SHA-1
```

**Structure Decision**: New file `cocoapods.rs` is a peer of cargo/dart/composer/gem/maven/golang. Integration site is the `read_all` dispatcher (placed alphabetically between `cmake` and `composer`); no file-claim tracker integration. Test files follow the existing `<reader>_<scenario>.rs` convention. **No new workspace crate per Principle VI; no new Cargo deps.**

## Complexity Tracking

> No Constitution Check violations — no justifications required.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none)    | n/a        | n/a                                  |
