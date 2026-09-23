# Implementation Plan: Read Nix flake.lock inputs as pinned components

**Branch**: `925-nix-flake-reader` | **Date**: 2026-09-23 | **Spec**: [spec.md](spec.md)

## Summary

Add a reader for `flake.lock` so the inputs a Nix build is pinned to appear in
the emitted SBOM. Today waybill claims none of a Nix repository's files — 65 of
70 unclaimed on the reference repository — so the artefact that actually pins
the build is absent from the document.

The lockfile is JSON. No Nix evaluation, no subprocess, no network. Each locked
input becomes a source-tier component identified by its revision, connected to
the thing that declares it.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain (now pinned in
`rust-toolchain.toml`). No nightly. `waybill-ebpf` untouched.

**Dependencies**: Existing only — `serde`/`serde_json` (the lockfile is JSON),
`globset` (walker pattern registration, already a direct dep),
`waybill_common::types::purl::Purl` (identifier construction and validation),
`tracing` (FR-010/FR-012 diagnostics), `anyhow`/`thiserror` (error propagation).
**Zero new Cargo dependencies.** No subprocess calls. No network access.

**Storage**: N/A — all state in-process per scan, matching every reader since
m002.

**Testing**: `cargo +stable clippy --workspace --all-targets` and
`cargo +stable test --workspace`, per the mandatory pre-PR gate.

**Target Platform**: All hosts waybill supports; the reader is pure filesystem
reads and is platform-independent.

**Project Type**: Single Rust workspace (Option 1).

**Performance Goals**: Parsing one small JSON file per flake directory. No
measurable scan-time impact expected; SC-002's offline-equals-online check is
the operative constraint, not throughput.

**Constraints**: Output identical offline and online (SC-002); byte-identical
across runs (SC-006); a malformed lockfile must not perturb any other
ecosystem's output (SC-007).

**Scale/Scope**: One reader, two new annotations with their catalogue rows and
per-format extractors, and the fixture shapes enumerated in quickstart.md.

## Constitution Check

| Principle | Status | Note |
|---|---|---|
| I. Pure Rust, Statically Linked | **PASS** | No new dependencies at all; nothing vendoring C. |
| II. eBPF-Only Observation | **N/A** | User-space filesystem read; no observation of a running build. |
| III. Fail Closed | **PASS** | A malformed or unknown-version lockfile emits nothing and says so (FR-010, FR-012); it never guesses. The reader declines to identify an input lacking a revision rather than inventing one. |
| IV. Type-Driven Correctness | **PASS** | `InputEdge` is a discriminated type (`NodeRef` vs `Follows`) rather than a string that is sometimes an array — the R3 finding encoded in the type system so the `follows` case cannot be forgotten. |
| V. Specification Compliance | **PASS, with two documented departures** | Audit recorded in the spec's Functional Requirements. Identity, source location and dependency edges all use native constructs. Exactly two facts have no native representation — the NAR hash (A-1) and the pre-resolution reference (A-2) — and both are argued in `contracts/annotations.md`. |
| VI. Three-Crate Architecture | **PASS** | Reader lives in `waybill-cli`; only existing `waybill-common` types are consumed. |
| VII. Test Isolation | **PASS** | Fixtures are self-contained trees; no network, no shared mutable state, no reliance on a Nix installation. |
| VIII. Completeness | **PASS** | Every identifiable locked input is emitted (C-1), and inputs that cannot be identified are recorded rather than dropped silently. |
| IX. Accuracy | **PASS — and this is the load-bearing one** | FR-009 forbids presenting a NAR hash as a content checksum. The native field exists and filling it would be the natural, native-looking choice; it would also assert something false about the component. This is an explicit Principle IX over Principle V call, settled in clarification Q2. |
| X. Transparency | **PASS** | FR-006 preserves what was asked for alongside what it resolved to; FR-012 records why nothing was emitted when a flake is unlocked. |
| XI. Enrichment | **N/A** | No enrichment in this feature. |
| XII. External Data Source Enrichment | **N/A** | No external data source; resolving versions through the pinned nixpkgs is explicitly out of scope. |

**Gate result**: PASS. No unjustified violations. The two Principle V departures
are documented with their rationale, as the principle requires, rather than
taken silently.

## Project Structure

### Documentation (this feature)

```
specs/925-nix-flake-reader/
├── spec.md
├── plan.md               # this file
├── research.md           # Phase 0 — five lockfiles measured
├── data-model.md         # Phase 1
├── contracts/
│   ├── emission.md       # what a consumer may rely on
│   └── annotations.md    # the two non-native facts, argued
├── quickstart.md         # verification recipes + fixture shapes
└── checklists/
    └── requirements.md
```

### Source Code (repository root)

```
waybill-cli/src/scan_fs/package_db/
└── nix/                        # new reader module
    ├── mod.rs                  # registration, discovery, emission
    ├── lockfile.rs             # parse: version, nodes, root
    └── identity.rs             # FR-013a/b identifier construction

waybill-cli/src/parity/extractors/   # FR-009b: one extractor per format
docs/reference/sbom-format-mapping.md # FR-009b: catalogue rows

waybill-cli/tests/
├── nix_flake_lock_reader.rs        # C-1..C-3, C-6, FR-013
├── nix_flake_lock_failure_modes.rs # C-7, FR-010/FR-012, SC-007
└── fixtures/nix/                   # shapes from quickstart.md
```

**Structure Decision**: Option 1 (single project). The reader is a new module
under the existing `package_db/` tree, registered through the m664 walker
registry exactly as every reader since m664. No new crate, no new architectural
surface.

## Complexity Tracking

No constitutional violations requiring justification.

One deliberate deferral, recorded so it is not mistaken for an oversight: the
m128 `detect_host_typed_purl_inputs` helper solves the same identifier problem
but consumes `SRC_URI` **strings**, while `flake.lock` supplies structured
fields. Refactoring it to serve both is a worthwhile cleanup that would couple
this feature to the Yocto reader's tests for no functional gain. Revisit if a
third caller appears.

## Phase Ordering

Phases follow the spec's user-story priorities, each independently testable:

1. **P1 — inputs appear** (US1): parse, identify, emit. Delivers SC-001 and
   SC-004 alone.
2. **P2 — the graph connects** (US2): root and nested relationships. Delivers
   SC-003.
3. **P3 — original vs locked** (US3): the pre-resolution reference.

Cross-cutting and not deferrable to a later phase, because retrofitting either
is more expensive than building it in: deterministic ordering (SC-006, the #948
lesson) and per-directory scoping (FR-011, the #938 lesson).
