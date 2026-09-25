# Implementation Plan: Transitive runtime closure for Nix-built Haskell projects

**Branch**: `985-nix-haskell-runtime-closure` | **Date**: 2026-09-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/985-nix-haskell-runtime-closure/spec.md`

## Summary

Milestone 926 resolves a Nix-built Haskell project's **declared** dependencies
against the nixpkgs revision its `flake.lock` pins. The artifact it substitutes
for — a `cabal.project.freeze` — carries the transitive closure, so the
substitution is currently partial. This milestone walks the runtime dependency
relations already present in the package-set file milestone 926 downloads and
caches, emitting the closure as components with the same version, hash and
provenance treatment declared dependencies get, connected to the graph by real
edges.

Measured multipliers: **1.5× / 3.8× / 2.4×** on three real projects,
cross-validated against an independent Nix evaluation (exact agreement at 167
components on one). No additional network retrieval — the data is in a file
already on disk.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `regex` (already parses this file),
`serde`/`serde_json` (annotation values), `tracing`, `anyhow`. **Zero new Cargo
dependencies.** No subprocess calls, no network beyond what milestone 926
already performs.
**Storage**: N/A — all state is in-process per scan. The per-revision cache
milestone 926 established (`~/.cache/waybill/nixpkgs/<rev>/`) is reused
unchanged; this feature reads the same file it already stores.
**Testing**: `cargo +stable test --workspace` plus the existing
`nix_haskell_resolution_m926.rs` suite (24 tests today), the per-PR document
integrity suite (`document_integrity.rs`), and the public-corpus layer 0/1/2
gates. Measurement probes live in `measurements/`.
**Target Platform**: All hosts waybill supports (Linux, macOS, Windows). Pure
parsing; no platform-specific behaviour.
**Project Type**: CLI — single Rust workspace, three crates.
**Performance Goals**: SC-008 — scanning with the closure enabled takes no more
than 1.5× the wall clock of the same scan with it disabled, package set local.
**Constraints**: FR-003 — no retrieval beyond what declared-dependency
resolution already requires; a project that resolves offline must close offline.
FR-013 — byte-identical output across two scans of one revision.
**Scale/Scope**: Largest measured closure 394 components from 162 declared,
against a package set of ~18,650 attributes. Bounded above by the package set,
which is finite and fixed by the pinned revision.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Note |
|---|---|---|
| I. Pure Rust, Statically Linked | **PASS** | No new dependencies; no C; no FFI. |
| II. eBPF-Only Observation | **N/A** | Filesystem-scan path; no observation surface. |
| III. Fail Closed | **PASS** | Unresolvable names degrade to versionless-with-reason (FR-005/FR-005a); candidate compilers that disagree resolve to nothing; an offline cache miss degrades the whole pass (milestone 975 posture, reused). |
| IV. Type-Driven Correctness | **PASS** | No `.unwrap()` in production; the closure's provenance marker is a typed enum, not a bare string (see data-model). |
| V. Specification Compliance | **GATE — see research R3** | Whether "declared vs transitive" has a **native** CycloneDX / SPDX carrier must be audited before inventing a `waybill:` annotation. Principle V requires native-first. |
| VI. Three-Crate Architecture | **PASS** | All changes in `waybill-cli`; the shared types it touches already live in `waybill-common`. |
| VII. Test Isolation | **PASS** | Closure tests reuse the m926 hermetic seeded-cache pattern; no network in tests. |
| VIII. Completeness | **PASS — this is the motivating principle** | The feature exists to close a false-negative surface: packages present in the built artifact and absent from the document. |
| IX. Accuracy | **PASS** | Versions come from the pinned revision, never inferred; an unresolvable name is emitted versionless rather than guessed; a package is not traversed into when it is compiler-supplied. |
| X. Transparency | **PASS** | FR-006/FR-006a require explicit per-component provenance; FR-014 requires a document-scope summary. |
| XI. Enrichment | **PASS** | Closure data comes from the same artifact already used for version resolution. |
| **XII. External Data Source Enrichment** | **FLAGGED — pre-existing divergence, see below** | XII.1 forbids external sources introducing new components. |
| **Strict Boundary 1** | **FLAGGED — pre-existing divergence, see below** | Forbids manifest-based dependency discovery outright. |

### The flagged boundary, stated plainly

Strict Boundary 1 reads:

> **No lockfile-based dependency discovery.** Lockfiles and manifests MUST NOT
> be used as a source of dependency discovery. […] Lockfiles MAY be read for
> enrichment purposes only […] but MUST NOT introduce components not observed
> in the trace.

and Principle XII.1 repeats it: *"External sources MUST NOT introduce new
components."*

Read literally, this forbids the feature. It also forbids **`waybill sbom scan`
in its entirety** — every package-DB reader since milestone 002 discovers
components from manifests with no eBPF trace involved. The constitution
contains no scan-mode carve-out; searched for one and there is none. Version
3.0.0, last amended 2026-09-10.

So this is a **pre-existing divergence between the constitution and the shipped
product**, roughly 130 milestones wide, which this feature inherits rather than
creates. Governance is explicit that violations require "either a code fix or a
constitution amendment — never silent deviation", so it is recorded here rather
than passed over.

**This plan does not resolve it.** Blocking a single Haskell feature on a
boundary that the entire scan mode already crosses would be arbitrary, and
amending the constitution is not this milestone's business. The recommendation
is a separate constitution-amendment PR introducing a scan-mode scoping for
Principle XII and Boundary 1, so that trace-mode's trust model stays intact
while scan-mode's actual behaviour is governed by rules that describe it.
Tracked in Complexity Tracking below.

**What this feature does honour**, independent of that: every component it adds
is attributable to a pinned, content-addressed revision; carries explicit
provenance (FR-006); is connected to the graph (FR-008); and is suppressible
with one flag that returns byte-identical prior output (FR-016/FR-017).

### Post-design re-check (after Phase 1)

| Gate | Before Phase 0 | After Phase 1 |
|---|---|---|
| V. Specification Compliance | **GATE** — native carrier for declared-vs-transitive unaudited | **PASS** — audited in research R3 across all three formats; none has a native slot (CycloneDX has no direct/transitive field and waybill's emitter uses none; the SPDX relationship enum carries kind, not depth). Annotation justified and recorded, which is what Principle V asks. |
| III. Fail Closed | PASS | **PASS** — strengthened by design: E3/E5 make the unresolvable path emit a versionless component rather than drop an edge, so failure is visible in the document rather than as a hole. |
| IX. Accuracy | PASS | **PASS** — R5 keeps boot libraries untraversed, so no relation is attributed that the build does not have. |
| X. Transparency | PASS | **PASS** — E4 exists specifically because milestone 926 computed this record and dropped it (the defect #973 fixed); the data model states the summary MUST reach the document, not a log line. |
| XII / Strict Boundary 1 | FLAGGED | **STILL FLAGGED** — unchanged by design work. Recorded in Complexity Tracking; recommended as a separate constitution amendment. No design choice here makes it better or worse. |

No new violations were introduced by Phase 1. One gate cleared, one
pre-existing divergence carried forward unresolved and visible.

## Project Structure

### Documentation (this feature)

```text
specs/985-nix-haskell-runtime-closure/
├── plan.md              # This file
├── spec.md              # Feature specification (with Clarifications)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── closure-contract.md
├── checklists/
│   └── requirements.md  # Spec quality checklist
├── measurements/        # Probes + findings behind every number in the spec
│   ├── README.md
│   ├── closure_probe.py
│   ├── nix_closure_oracle.sh
│   └── nix_version_oracle.sh
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/
├── mod.rs                  # enrich(), classify(), EnrichmentSummary — EXTENDED
├── package_set.rs          # attribute-keyed parser — EXTENDED with dep relations
├── boot_libraries.rs       # nulled + alias parsing — UNCHANGED
├── closure.rs              # NEW: the walk, its result, cycle handling
├── cache.rs                # per-revision cache — UNCHANGED
└── fetch.rs                # retrieval boundary — UNCHANGED

waybill-cli/src/cli/scan_cmd.rs        # flag wiring + relationship emission — EXTENDED
waybill-cli/src/generate/              # annotation emission in 3 formats — EXTENDED
waybill-cli/src/parity/extractors/     # catalog rows for new annotations — EXTENDED

waybill-cli/tests/
├── nix_haskell_resolution_m926.rs     # EXTENDED — closure cases join the suite
└── document_integrity.rs              # UNCHANGED — already enforces I2 per-PR
```

**Structure Decision**: The closure is a new module beside the existing resolver
rather than more surface on `mod.rs`, which is already ~1,000 lines carrying
retrieval, classification, degradation and summary. The walk has its own
concerns — cycle termination, provenance attribution, edge synthesis — and its
own tests. `package_set.rs` gains dependency-relation extraction because it
already owns the attribute-keyed parse of that file and nothing else should
parse it twice.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Principle XII.1 / Strict Boundary 1 — external source introduces components | The feature's entire purpose is emitting packages the project depends on but does not declare. Inherited from `sbom scan` mode, which has done manifest-based discovery since milestone 002 and which the constitution never scoped. | There is no simpler alternative that delivers the feature: not adding components is not adding the closure. The real fix is a constitution amendment scoping XII and Boundary 1 to trace mode, which is out of this milestone's scope and recommended as separate work. Recorded rather than silently deviated from, per Governance. |
| A new module (`closure.rs`) rather than extending `mod.rs` | `mod.rs` already carries retrieval, gating, classification, degradation and summary at ~1,000 lines. | Extending it was considered and rejected: the walk's failure modes (cycles, unresolvable names, provenance precedence) have no overlap with retrieval's, and milestone 980 showed how a change buried in a large module can miss an invariant that lives elsewhere. |
