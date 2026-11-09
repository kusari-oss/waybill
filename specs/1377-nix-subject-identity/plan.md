# Implementation Plan: the subject of a Nix scan is the package it built, identified by the CPE nixpkgs declares

**Branch**: `1377-nix-subject-identity` | **Date**: 2026-11-09 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1377-nix-subject-identity/spec.md`

## Summary

When `--nix-closure` resolves a closure, the derivation the evaluated
attribute produced (`ClassifiedClosure.graph.root`) becomes the document's
subject. This is a new root-selection rung below the operator override and
above every main-module heuristic, returning the root's real component
(`MainModule(idx)`) rather than a synthetic placeholder.

Closure members, the subject included, carry the CPE nixpkgs declares in
`meta.identifiers.cpe`. That value is read in the meta evaluation milestone 1281
already runs and joined to members by `.drv` basename, then by pname + version.
It takes precedence over waybill's curated table, which stays the fallback.
Each CPE's source is recorded in each format's native identifier-provenance
field. The join basis and any overridden curated value go in one new annotation
(C212). The closure summary (C184) counts members whose metadata was not reached.

Every number this rests on was measured (research.md, `measurements/`).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde`/`serde_json`, `tracing`, `anyhow`. The host's `nix`, already required by `--nix-closure`. **Zero new Cargo dependencies.**
**Storage**: N/A. In-process per scan.
**Testing**: `cargo test --workspace --no-fail-fast` and clippy via `./scripts/gate.sh … ./scripts/pre-pr.sh`. Closure tests use the existing stub-`nix` fixture pattern (`tests/closure_*.rs`), never a real evaluation (Principle VII). Scanner-outcome criteria are judged by the committed probes against real Nix.
**Target Platform**: unchanged (Linux, macOS, Windows). The closure tier runs where `nix` exists.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: no measurable change. Adding `identifiers` to the meta evaluation measured 0.80 s → 0.82 s median (N = 3, 358 names), and it adds no evaluation call.
**Constraints**: no new network access or evaluation beyond `--nix-closure` (FR-010). Output byte-identical for every scan that does not resolve a closure (FR-011).
**Scale/Scope**: openssh's buildtime closure emits ~457 components; 17 carry a nixpkgs-declared CPE. The largest measured closure (httpie) emits 2,143.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | How |
| --- | --- | --- |
| I. Pure Rust, statically linked | PASS | No new dependencies. |
| III. Fail Closed (scan mode → X) | PASS | "Evidence that could not be read is reported, never filled in by guesswork." No placeholder CPE (FR-012); unreached members are counted (FR-009). |
| IV. Type-driven correctness | PASS for new code; one boundary into a pre-existing violation, accepted 2026-11-09 | Declared CPEs are a validated `DeclaredCpe` across every closure-tier function boundary, converted to `String` exactly once, at insertion into the pre-existing `ResolvedComponent.cpes: Vec<String>`. That field predates this milestone and violates IV for every ecosystem (108 references, 49 files). It is tracked as its own issue (Setup task), not widened here (analysis C1, option a). No `.unwrap()` in production code. |
| V. Specification compliance | PASS | Per-format native-construct audit done (research R5, R6), with results cited in FR-008 and FR-009. Native carriers: CDX `evidence.identity`, SPDX 2.3 `externalRefs[].comment`, SPDX 3 `ExternalIdentifier.comment`. The one new `waybill:*` field (C212) carries only what no format expresses, with a justification clause in `sbom-format-mapping.md`. Declared CPEs are emitted only if they parse as CPE 2.3. |
| VII. Test isolation | PASS | Stub-`nix` fixtures; no privileges; no live evaluation in `cargo test`. |
| VIII. Completeness | PASS | Adds identifiers (gnugrep gains one); removes none (SC-003, SC-006). |
| IX. Accuracy | PASS | Replaces the curated bash encoding grype matches to nothing (measured) with the declared one. A declared CPE is used as declared, never "corrected". |
| X. Transparency | PASS | Per-CPE source in native fields; coverage gap in C184. |
| XII. External data source enrichment | PASS | nixpkgs metadata comes from the evaluation `--nix-closure` already performs at the lockfile's pinned revision. It is annotated with its source, and if it is unavailable the result is the curated table (degrade, not fail). |

**Re-check after Phase 1:** unchanged. The design adds one annotation row (C212), justified, and extends two existing rows (C69 value, C184 field).

## Project Structure

### Documentation (this feature)

```text
specs/1377-nix-subject-identity/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
├── measurements/          # spike probes, bash/openssh encodings, identifier requirements, meta-eval cost
└── tasks.md               # /speckit.tasks
```

### Source Code (repository root)

```text
waybill-cli/src/
├── scan_fs/package_db/nix/
│   ├── eval/licenses.rs          # R1: version + guarded cpe in licenses_expr; row type gains both fields
│   └── closure/
│       ├── mod.rs                # R1: identifier join (drv, then name-version); attach to ClassifiedClosure
│       ├── identifiers.rs        # NEW: DeclaredCpe (validated), join, IdentifierCoverage
│       ├── emit.rs               # R2: always emit graph.root; set cpes from DeclaredCpe; evaluated-package tag
│       └── summary.rs            # R6: C184 gains `identifiers`
├── generate/
│   ├── root_selector.rs          # R3: ScanTargetKind::NixClosure, NixEvaluatedPackage rung, internal tag key
│   ├── cpe.rs                    # R4: reuse synthesize_cpes to compute the overridden curated value
│   ├── cyclonedx/
│   │   ├── metadata.rs           # R7: no placeholder CPE for NixEvaluatedPackage subjects
│   │   ├── evidence.rs           # R5: evidence.identity entry field=cpe
│   │   └── builder.rs            # R5: waybill:cpe-provenance property; verify :841/:1313 kind checks
│   └── spdx/
│       ├── packages.rs           # R5: CPE externalRef comment
│       ├── annotations.rs        # R5: C212 envelope
│       ├── v3_external_ids.rs    # R5: cpe23 ExternalIdentifier comment
│       └── v3_annotations.rs     # R5: C212 statement
├── parity/extractors/            # C212 row (cdx/spdx2/spdx3 + mod.rs); C69/C184 unchanged in shape
└── cli/scan_cmd.rs               # set ScanTargetKind::NixClosure; pass identifiers through

waybill-cli/tests/
├── closure_subject.rs            # NEW: US1 via stub nix
├── closure_declared_cpe.rs       # NEW: US2/US3 via stub nix
└── fixtures/…                    # stub-nix JSON: derivation show + meta rows with version/cpe

docs/reference/sbom-format-mapping.md   # C69 value, C184 field, C212 row + justification
docs/reference/nix-evaluation.md        # subject + declared-CPE behaviour for operators
waybill-cli/tests/fixtures/public_corpus/nix-closure-moat/   # regenerated goldens (SC-008)
```

**Structure Decision**: the existing three-crate workspace. All changes are in `waybill-cli`. `waybill-common`'s `ResolvedComponent` is unchanged; the closure-local `DeclaredCpe` lives in the new `closure/identifiers.rs`.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| One `DeclaredCpe` → `String` conversion, at the shared `ResolvedComponent.cpes` field | The field is `Vec<String>` today, for every ecosystem's CPEs. | A `Cpe` newtype on the shared field touches about 49 substantive sites and about 59 constructors across ecosystems unrelated to Nix. It is tracked as its own issue (analysis C1, option b rejected for this milestone). |
| Two signals for one fact: a native provenance carrier per format *and* the C212 annotation | Principle V requires the native carrier as the primary signal. The join basis and the overridden curated CPE have no native field. | Annotation-only violates Principle V. Native-only loses the basis and the overridden value, which US3 scenario 3 requires. C212 carries only what the native field cannot. |
