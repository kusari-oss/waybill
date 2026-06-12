# Implementation Plan: Go Build-Inclusion Clarity

**Branch**: `112-go-build-inclusion` | **Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/112-go-build-inclusion/spec.md`

## Summary

Source-tier Go scans can emit components whose production-build
participation is unknowable from the SBOM (20 of 87 on the kusari-cli
anchor). Two layers fix this: **Part B** — a typed
`BuildInclusion::Unknown` status rendered as the consumer-visible
`mikebom:build-inclusion: unknown` annotation on go.sum-fallback /
flat-attached components no higher-fidelity signal confirms (always on,
toolchain-free); **Part C** — when a `go` toolchain is on PATH, a
chunked `go mod why -m -vendor` shell-out (cyclonedx-gomod's
`FilterModules` mechanism; 60s total budget; spawn+`recv_timeout`
pattern reused from `go_mod_graph.rs`) classifies each module:
not-needed → kept in output with native CDX `scope: "excluded"` +
derivation annotation, test-only → existing `LifecycleScope::Test`
path, prod-needed → untouched. Every failure mode degrades to Part B
with warn logs; scans never fail; non-Go output stays byte-identical.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–111; no nightly required for this user-space-only feature)
**Primary Dependencies**: Existing only — `std::process::Command` + `std::sync::mpsc` (subprocess with timeout, pattern at `golang/go_mod_graph.rs:81–158`), `clap` (new boolean flag via derive), `serde`/`serde_json` (annotation values), `tracing` (FR-013 logs), `anyhow`/`thiserror` (error classes). **No new Cargo dependencies.** Child-process tool: the host's `go` binary (optional; absence degrades).
**Storage**: N/A — all classification state is in-process per scan; persisted only as annotations in the emitted SBOM.
**Testing**: `cargo +stable test --workspace` + `cargo +stable clippy --workspace --all-targets` (pre-PR gate). Hermetic Part C tests via stub `go` executables on a prepended PATH (`#[cfg(unix)]`); goldens run with `MIKEBOM_NO_GO_MOD_WHY=1` for host-independence; one env-gated real-toolchain e2e test.
**Target Platform**: Linux + macOS first-class; Windows lane covered by the flag-off/no-toolchain degrade path (stub-script tests are unix-gated).
**Project Type**: CLI (existing three-crate workspace; touches `mikebom-common` + `mikebom-cli`)
**Performance Goals**: Part C adds ≤60s worst-case to a Go source scan (hard budget); warm-cache typical <5s; Part B pass is O(components), negligible.
**Constraints**: Scan must NEVER fail due to analysis (FR-007); `--offline` must not cause network (`GOPROXY=off GOFLAGS=-mod=mod GOTOOLCHAIN=local`); byte-identity outside the documented annotation additions (FR-008).
**Scale/Scope**: Module sets of O(10²) per scan (kusari-cli: 87); chunks of 20 modules per subprocess invocation.

## Constitution Check

*GATE: evaluated pre-Phase-0 and re-evaluated post-Phase-1 — PASS (no violations).*

| Principle | Verdict | Notes |
|---|---|---|
| I. Pure Rust, Zero C | PASS | No new crates; child `go` process is an external tool invocation (same class as existing `go mod graph`, `git`, `trivy` shell-outs), not linked code. |
| II / III (eBPF discovery / fail closed) | PASS (N/A) | `scan_fs` static-scan path, established since milestone 002; this feature introduces NO discovery — classification only. |
| IV. Type-Driven Correctness | PASS | New `BuildInclusion` enum in `mikebom-common`; verdicts as `GoModWhyVerdict` enum; no raw strings across boundaries; no `.unwrap()` in production (test mods carry the standard `cfg_attr` allow). |
| V. Specification Compliance | PASS | Native-construct audit performed and cited (contracts/annotations.md): not-needed uses native CDX `scope: excluded`; `unknown` has no native construct in any format; SPDX 2.3/3 annotation rows are documented parity bridges naming the missing native excluded-scope field. CDX 1.6/SPDX schemas unaffected (properties/annotations are schema-valid). |
| VI. Three-Crate Architecture | PASS | Only `mikebom-common` + `mikebom-cli` touched. |
| VII. Test Isolation | PASS | All new tests unprivileged; subprocess tests hermetic via stubs; real-toolchain test env-gated. |
| VIII. Completeness | PASS | FR-011: no component ever dropped by the new passes; not-needed stays in output. |
| IX. Accuracy | PASS | The feature's purpose — phantom-dependency signal-to-noise. Conservative precedence (BuildInfo wins) prevents false exclusions. |
| X. Transparency | PASS | Unknown/not-needed/derivation are exactly Principle X structured limitation metadata, using native fields where they exist. |
| XII. External Data Source Enrichment | PASS | `go mod why` enriches already-discovered components (scope/annotations only); introduces no components; provenance annotated; unavailability degrades gracefully (constraint 3). |
| Strict Boundary 1 (no lockfile discovery) | PASS | Classification only; component set unchanged. |

## Project Structure

### Documentation (this feature)

```text
specs/112-go-build-inclusion/
├── plan.md              # This file
├── research.md          # Phase 0 — R1..R8 decisions
├── data-model.md        # Phase 1 — BuildInclusion, verdicts, transitions
├── quickstart.md        # Phase 1 — validation walkthrough + stub pattern
├── contracts/
│   ├── annotations.md            # annotation keys, format mapping, parity rows
│   ├── cli-flags.md              # --no-go-mod-why + env var + behavior matrix
│   └── go-toolchain-invocation.md# subprocess command/env/budget/parsing
└── tasks.md             # Phase 2 (/speckit.tasks — not created here)
```

### Source Code (repository root)

```text
mikebom-common/src/
└── resolution.rs                      # + BuildInclusion enum; + build_inclusion field on
                                       #   PackageDbEntry-backing component types

mikebom-cli/src/
├── main.rs                            # + --no-go-mod-why flag + MIKEBOM_NO_GO_MOD_WHY bridge
├── scan_fs/package_db/
│   ├── mod.rs                         # read_all(): + apply_go_mod_why_classification,
│   │                                  #   + apply_go_build_inclusion_unknown_markers
│   │                                  #   (after existing G3/G4 filters, lines ~546/554)
│   └── golang/
│       └── mod_why.rs                 # NEW — runner (chunked, 60s shared budget, offline env
│                                      #   pinning) + section parser + GoModWhyVerdict
├── generate/
│   ├── cyclonedx/builder.rs           # scope: "excluded" for NotNeeded (bypasses include-dev
│   │                                  #   gate at ~599); build-inclusion properties (~928)
│   └── spdx/
│       ├── annotations.rs             # SPDX 2.3 package annotations (existing bag path)
│       └── v3_annotations.rs          # SPDX 3 element annotations (existing bag path)
└── parity/catalog.rs                  # parses the two new mapping rows (no code change expected)

docs/reference/sbom-format-mapping.md  # + 2 rows (build-inclusion, build-inclusion-derivation)
                                       # + amend lifecycle-scope-derivation value enum

mikebom-cli/tests/
├── go_build_inclusion.rs              # NEW — Part B markers, stub-toolchain Part C verdicts,
│                                      #   degrade matrix (SC-003), budget exhaustion
├── scan_go.rs                         # drift: new annotations on fallback components
├── transitive_parity_go.rs            # cross-format presence of new annotations
└── fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/golang.*  # one-time regeneration
```

**Structure Decision**: Existing three-crate workspace; one new module
(`golang/mod_why.rs`) and one new integration-test file; everything else
is edits to established files. Matches every milestone since 002.

## Implementation Notes

- **Pass ordering** (research R4): `read_all()` runs existing
  `apply_go_linked_filter` → `apply_go_production_set_filter` → NEW
  classification pass → NEW unknown-marker pass (last, so it observes
  final state). Scan root + offline/disable flags are plumbed as
  parameters into `read_all` callers (the `MIKEBOM_OFFLINE` env bridge
  remains the interim source where signatures can't change cheaply).
- **Reliability preflight** (research R3 empirical addendum): `go mod
  why` silently reports false not-needed when module resolution fails
  (exit 0 — verified on go 1.26.2). A `go list all` preflight gates
  each main-module analysis; failure → skip reason
  `unresolvable-packages`, no verdicts accepted. The `-vendor` flag
  changes the not-needed phrasing to "does not need to vendor module" —
  the parser matches the `(main module does not need` prefix.
- **Precedence** (FR-010, data-model transitions): BuildInfo-confirmed
  entries (binary present, no `mikebom:not-linked`) are exempt from
  NotNeeded/Unknown; existing test tags are never downgraded;
  main-module entries exempt from all passes.
- **PR #332 dependency**: builds on the test-only-closure work
  (`mikebom:lifecycle-scope-derivation` key). If unmerged at
  implementation start, branch from it or rebase after merge.
- **Golden policy** (SC-004): suite-wide `MIKEBOM_NO_GO_MOD_WHY=1` via
  the shared test-env helper keeps goldens host-independent; Go goldens
  regenerated once for Part B markers; non-Go goldens byte-identical.

## Complexity Tracking

> No Constitution Check violations — table intentionally empty.
