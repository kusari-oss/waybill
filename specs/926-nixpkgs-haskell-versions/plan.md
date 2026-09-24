# Implementation Plan: Resolve Haskell dependency versions through the pinned nixpkgs

**Branch**: `926-nixpkgs-haskell-versions` | **Date**: 2026-09-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/926-nixpkgs-haskell-versions/spec.md`

## Summary

A Nix-built Haskell project emits every dependency versionless at design tier,
because `.cabal` declares ranges and no `cabal.project.freeze` exists. The
versions are determined by the nixpkgs revision `flake.lock` already pins, and
#946 already reads that lockfile without consulting what it points at.

The approach: retrieve the pinned revision's generated Haskell package set over
HTTPS from the source the lock entry names, cache it by immutable revision,
and attach exact versions plus source hashes to the components the Haskell
reader already emits. Dependencies the compiler supplies stay versionless with
a reason, never an invented version.

Phase 0 settled the two questions that shaped the design:

- The nixpkgs `sha256` **is** a flat SHA-256 of the source tarball — verified
  byte-for-byte against Hackage — so it belongs in the **native** checksum
  field, not an annotation. This is the opposite of m925's `narHash` (C165),
  and assuming the C165 outcome would have produced a needless catalog row and
  a weaker document.
- Retrieval costs **~1.0 s for 16.6 MB, parsed in 0.01 s**, once per revision.
  That is what makes default-on defensible; it is a different cost shape from
  #930's 2,291 sequential requests.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `reqwest` (workspace, `rustls-tls`) for retrieval, `serde`/`serde_json` for annotation values, `sha2` + `data-encoding` for hex encoding, `tracing`, `anyhow`/`thiserror`, `clap` for the opt-out flag. The Nix-base32 decoder is ~20 lines of stdlib arithmetic (custom alphabet, reversed bit order — no crate provides it). **Zero new Cargo dependencies.**
**Storage**: Per-revision cache at `~/.cache/waybill/nixpkgs/<rev>/`, mirroring the m090 / m108 / m195 pinned-SHA layout. Immutable revision → no TTL, no invalidation.
**Testing**: `cargo +stable test --workspace`; hermetic fixtures with the retrieval boundary injected, matching the existing reader suites. No network in tests.
**Target Platform**: All host platforms waybill supports; no host-tool dependency (no `nix` binary).
**Project Type**: CLI / library — single Rust workspace.
**Performance Goals**: One retrieval per revision, ~1.0 s observed warm (R1), then cached. A repository that does not qualify performs zero retrieval (SC-009).
**Constraints**: FR-019 bound of 30 s default, operator-overridable — a budget at 30× the measured warm fetch, explicitly not presented as a measurement (R7). Must never prompt for credentials or block the scan (FR-018).
**Scale/Scope**: 16,634,427-byte artifact; 19,437 derivation blocks → 19,058 unique names; 8 compiler configurations at the pinned revision, 35–47 nulled names each (R1, R4).

## Constitution Check

*GATE: passed before Phase 0; re-checked after Phase 1 design. No violations.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | ✅ Retrieve-and-parse, not `nix eval` (R1). No host tool, no new crates, no C. |
| **III. Fail Closed** | ✅ FR-014a takes the **union** of boot libraries across candidate compilers — a package nulled in any candidate is treated as boot. Measured cost: 5 packages when the flake names its compilers, 18 under fallback (R4). Unreachable sources degrade rather than guess (C7). |
| **IV. Type-Driven Correctness** | ✅ `ResolutionOutcome` is a two-variant enum with a closed reason set; there is no representable partial state (data-model C2). A hash that does not decode to 32 bytes yields no hash rather than a malformed one. |
| **V. Specification Compliance** | ✅ **Audit performed, and it changed the design.** The source hash has a native carrier in all three formats and is emitted there (C3) — verified, not assumed (R2). Only data with no native carrier becomes a `waybill:` row: resolution provenance, unresolved reason, candidate disclosure. |
| **VIII. Completeness** | ✅ Moves dependencies from versionless design tier to source tier with hashes. The transitive closure is deliberately out of scope and filed as #962, gated on measuring its component multiplier. |
| **IX. Accuracy** | ✅ FR-005 / SC-003: never synthesise a version, hash or versioned identifier. Boot libraries — the same set `cabal v2-freeze` declines to pin (#938) — stay versionless. FR-014c forbids resolving against the default package set, which would emit versions the build does not use. |
| **X. Transparency** | ✅ Every unresolved dependency carries a reason (FR-006, SC-002); every resolved one carries provenance and revision (FR-007); ambiguous compiler candidates are disclosed (FR-014b); degradation is recorded at document scope (C7). |
| **XI. Enrichment** | ✅ Enrichment that must not delay to failure: FR-019 bounds retrieval, FR-008 degrades with an annotation. |
| **XII. External Data Source Enrichment** | ✅ **Constraint 1 is the binding one** — "External sources MUST NOT introduce new components." FR-001a forbids walking the derivation graph to discover dependencies the project does not declare, so resolution only enriches components the Haskell reader already emitted (C9). Constraint 2 (provenance) → FR-007. Constraint 3 (degrade) → FR-008. |

**Complexity Tracking**: not required — no violations to justify.

## Project Structure

### Documentation (this feature)

```text
specs/926-nixpkgs-haskell-versions/
├── spec.md                                   # /speckit.specify + /speckit.clarify
├── plan.md                                   # this file
├── research.md                               # Phase 0 — R1–R8
├── data-model.md                             # Phase 1 — entities + emission mapping
├── quickstart.md                             # Phase 1 — operator + implementer guide
├── contracts/
│   └── resolution-contract.md                # Phase 1 — C1–C9 observable surface
├── checklists/requirements.md                # spec quality checklist
├── measurements/
│   ├── probe_nixpkgs_haskell.py              # committed probe (has a known R3 bug)
│   ├── probe-output-ghc96.txt
│   └── README.md                             # M1–M3
└── tasks.md                                  # /speckit.tasks — NOT created here
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── nix/                                      # existing, from m925
│   ├── mod.rs
│   ├── lockfile.rs                           # REUSED: PinnedNixpkgs comes from here (R6)
│   ├── identity.rs
│   └── haskell_packages/                     # NEW — this feature
│       ├── mod.rs                            # orchestration + gating (C1)
│       ├── fetch.rs                          # retrieval from the lock-named source, bounded (FR-016/019)
│       ├── cache.rs                          # per-revision cache (R5)
│       ├── package_set.rs                    # parse name -> (version, sha256) (R1)
│       ├── nix_base32.rs                     # base32 -> 32 bytes -> hex (R2)
│       └── boot_libraries.rs                 # attrset-depth-aware nulled-set parse (R3)
└── haskell.rs                                # MODIFIED: consume ResolutionOutcome

waybill-cli/src/generate/
├── cyclonedx/                                # native hashes[] + new annotations
└── spdx/                                     # native checksums[] + new annotations

waybill-cli/src/parity/extractors/            # extractors for each new catalog row
docs/reference/sbom-format-mapping.md         # new catalog rows

waybill-cli/tests/
├── nix_haskell_resolution_m926.rs            # NEW — the quickstart scenario table
└── fixtures/nix_haskell/                     # NEW — hermetic fixtures, no network
```

**Structure Decision**: extend the existing m925 `nix/` reader module rather
than create a sibling. The lockfile reader, `OriginalPinState` and the input
identity logic already live there, and R6 depends on reusing them rather than
re-reading `flake.lock`. Haskell package-set concerns are isolated in a
`haskell_packages/` submodule so the Nix reader's existing surface is
unchanged.

## Phase Summary

**Phase 0 — complete.** `research.md`, R1–R8. Two findings reshaped the design
(native hash; ~1 s retrieval) and one is a correctness trap the implementation
must avoid (R3 attrset-depth parsing). The committed probe reproduces every
figure and carries the R3 bug as a known defect to fix.

**Phase 1 — complete.** `data-model.md` (entities, closed reason set, emission
mapping), `contracts/resolution-contract.md` (C1–C9), `quickstart.md`
(operator usage, reproduction, scenario table, implementer gotchas).

**Phase 2 — `/speckit.tasks`, not this command.** Expected shape: fix the probe's
R3 bug first (it is the executable statement of the boot-library rule), then
`nix_base32` with the R2 vector as its test, then package-set and
boot-library parsing, then cache and bounded retrieval, then emission with
native hashes, then the catalog rows and extractors, then the hermetic fixture
suite covering the quickstart scenario table.
