# Implementation Plan: the Go modules and Rust crates inside a Nix-built binary

**Branch**: `1378-closure-binary-contents` | **Date**: 2026-11-10 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/1378-closure-binary-contents/spec.md`
**Issue**: #1396

> **Superseded on rebase (2026-11-10).** While this was being built, main
> landed milestone 1377 (#1405, issues #1398 and #1394): the package a closure
> scan evaluated is always emitted and is the document's subject, identified
> as `graph.evaluated` — the closure's sole unconsumed derivation. That is the
> root-as-component work this feature had taken on as "the minimum of #1398".
> On rebase the feature's own mechanism (a `root` value for
> `waybill:closure-role`, `DerivationRole::Root`, `graph::sole_root`) was
> **removed** in favour of main's. Text below that describes it is history.
> Main also took row C212, so this feature's rows are **C213, C214, C215**.

## Summary

Under `--nix-closure`, read each closure member's outputs that are already
present in the store with the existing Go build-info and cargo-auditable
readers, emit the recovered modules as top-level components the member depends
on, and record at document and component scope what was and was not read. Make
the closure root a component so its contents have something to attach to.

Technical approach, each step traceable to research.md:

1. A new closure role `root` makes the closure root emit (R1).
2. A new single-purpose entry point in `scan_fs/binary/` runs **only** the two
   embedded-build-information readers on a path, reusing their existing
   functions; it is not a full scan of the output (R2, measured: a full scan of
   `rustc` returns 709 lockfile crates).
3. A new `closure/contents.rs` pass maps each emitted member to its outputs,
   checks presence under the store directory, calls (2), and returns recovered
   components, member→module edges, per-member read states and the
   document-scope record (R3).
4. The pass runs in `scan_cmd.rs` after the m1366 identity pass and before the
   closure edges are joined, because the edge join reads each member's final
   PURL and this pass may promote one (R4).
5. Identity promotion reuses `DepsDevSource::confirm`, with the Go version
   given its `v` prefix (R5, measured).
6. Three catalog rows: C213 document-scope read record, C214 per-member read
   state, C215 per-member binary identity (R6).
7. One operator flag, `--nix-closure-require-root-contents` (R7).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `go_binary::read_binary`, `binary::scan::scan_binary`, `binary::entry::cargo_auditable_packages_to_entries`, `binary::discover::discover_binaries`, `DepsDevSource::confirm` + `EnrichmentKey`, `serde`/`serde_json`, `tracing`, `clap`. **Zero new Cargo dependencies.**
**Storage**: N/A — in-process per scan.
**Testing**: `cargo test --workspace --no-fail-fast` via `./scripts/pre-pr.sh`. Integration tests use the existing stub-`nix` pattern (`tests/closure_edges.rs:70-91`) plus a temporary store directory selected through `NIX_STORE_DIR`. Binary fixtures are generated at test time from bytes the existing reader unit tests already synthesize; no real package coordinates (fixture policy).
**Target Platform**: Linux, macOS, Windows hosts (the pass is inert where no store exists). Mach-O and ELF outputs measured.
**Project Type**: CLI (`waybill-cli`), single crate touched plus docs.
**Performance Goals**: Per-output read cost no worse than the standalone binary scan of that output. Measured baseline: 104 outputs in 29 s including process start and full-scan overhead; the in-process pass does strictly less per output (two readers, no walk registry, no emission).
**Constraints**: No subprocess beyond the existing `nix derivation show`. No realisation (FR-002). No network beyond the existing deps.dev confirmation call, which is skipped when deps.dev is disabled.
**Scale/Scope**: Closures of 800–2,500 derivations; 0–100% of outputs present. Recovered modules per member measured at 1–321.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
| --- | --- |
| I. Pure Rust, statically linked | Pass. No new dependencies, no FFI. |
| II / III. eBPF-only, fail closed | Not applicable — scan mode. III defers to X for scan mode. |
| IV. Type-driven correctness | Pass. Read state, binary-identity status and ecosystem are enums; PURLs stay `Purl`; no `.unwrap()` in production code. |
| V. Specification compliance, native first | Pass with one recorded carve-out. Member→module uses the native dependency relationship in all three formats. A confirmed identity is the component's native primary identifier. The unread record uses CycloneDX `compositions` natively (the root-completeness claim is withheld) plus C213 for the counts no format expresses. **Carve-out**: an *unconfirmed* candidate identity (C215) is carried as a `waybill:*` annotation in all three formats — see Complexity Tracking. |
| VI. Three-crate architecture | Pass. `waybill-cli` only. |
| VII. Test isolation | Pass. No privileges; store directory is a temp dir. |
| VIII. Completeness | Advances it: 0 → 163 and 0 → 42 on the measured targets. |
| IX. Accuracy | Pass. Identity promotion requires registry confirmation; an unconfirmed join is never primary (FR-012b). The binary's own module is never emitted as a second component. |
| X. Transparency | Pass. C213/C214 make unread members explicit; the root-completeness claim is withheld when any member is unread. |
| XI / XII. Enrichment | Pass. Reuses the existing deps.dev client and its on/off switch. |

**Post-design re-check**: unchanged. The one deviation is the C215 carve-out below.

## Project Structure

### Documentation (this feature)

```text
specs/1378-closure-binary-contents/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── annotations.md      # C213, C214, C215 wire shapes
│   ├── cli.md              # --nix-closure-require-root-contents
│   └── documentation.md    # docs that must change
├── checklists/requirements.md
├── measurements/           # FINDINGS.md + probes
└── tasks.md                # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/
├── scan_fs/
│   ├── binary/
│   │   ├── mod.rs                      # + pub(crate) fn embedded_contents(path)
│   │   └── embedded.rs                 # NEW — the two-reader entry point
│   └── package_db/
│       ├── go_binary.rs                # + pub(crate) dependency-entries helper
│       └── nix/closure/
│           ├── classify.rs             # + DerivationRole::Root
│           ├── graph.rs                # root_of → pub(crate)
│           ├── emit.rs                 # Root emits
│           ├── contents.rs             # NEW — the pass, read states, C213 record
│           ├── summary.rs              # role tally handles Root
│           └── mod.rs                  # mod contents
├── resolve/closure_role_scope.rs       # "root" arm (explicitly unclassified)
├── cli/scan_cmd.rs                     # flag, wiring, strict check, C213 plumbing
├── generate/
│   ├── mod.rs                          # ScanArtifacts.nix_closure_contents
│   ├── cyclonedx/{builder,metadata,mod,compositions}.rs
│   └── spdx/{annotations,v3_annotations,document,mod,packages,relationships}.rs
│   └── openvex/mod.rs                  # struct-literal default
└── parity/extractors/{cdx,spdx2,spdx3,mod}.rs   # C213–C215

waybill-cli/tests/
├── closure_binary_contents.rs          # NEW — integration
└── fixtures/nix_closure_contents/      # NEW — closure JSON fixtures

docs/reference/sbom-format-mapping.md   # C182 value `root`; rows C213–C215
docs/ecosystems.md                      # closure tier section
```

**Structure Decision**: single crate, following the closure tier's existing
module layout. The binary-side entry point lives in `scan_fs/binary/` so the
recognition logic has one owner; the closure-side pass lives beside the other
closure passes.

## Phases

- **Phase 0 — research.md**: seams, measurements, decisions R1–R9.
- **Phase 1 — design**: data-model.md, contracts/, quickstart.md.
- **Phase 2 — tasks**: generated by `/speckit-tasks`.

## Risks

| Risk | Mitigation |
| --- | --- |
| Corpus golden `nix-closure-moat` changes (new role value, C214 on every member, C213) and is `#[ignore]`d locally. | Regeneration needs the `public-corpus.yml` dispatch; called out as a task that cannot be completed locally. |
| Golden content now depends on which outputs the CI runner has. | The corpus target runs with nothing built, so every member reads `absent`; if the runner image ever ships store paths that collide, C214 will show it. Recorded, not solved. |
| A hand-built component from a reader entry can drift from the standalone path. | A test compares the two for one binary field by field (FR-005). |
| `root_of` picks the first of several unconsumed derivations. | Unchanged behaviour; `nix derivation show -r X` yields one by construction. A closure with more than one is reported in C213 as `root: ambiguous` and no root is promoted. |

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| C215 carries an unconfirmed candidate identity as a `waybill:*` annotation although CycloneDX 1.6 has `evidence.identity[].concludedValue`. | SPDX 2.3 and SPDX 3 have no way to mark an identifier as unconfirmed: a second `purl` external reference is indistinguishable from a confirmed one, which FR-012b forbids. One annotation gives the three formats identical information. | Using CycloneDX's native slot alone leaves SPDX without the candidate (parity gap); using SPDX's extra identifier asserts an unverified join. Emitting the native CycloneDX slot *in addition* is #1245's deliverable and is left to it, so the two do not define the slot twice. |
