# Implementation Plan: Closure documents contain every boot library a boot library depends on

**Branch**: `1655-reachable-boot-libraries` | **Date**: 2026-11-25 | **Spec**: [spec.md](spec.md)
**Input**: `specs/1655-reachable-boot-libraries/spec.md`. Refs #1696, #1688.

## Summary

Spec 1653's pass, `resolve::boot_library_edges::apply`, changes in two ways, both measured (research R1):

1. **Lookup by PURL.** A boot library's dependency resolves to the component that holds `pkg:hackage/<dep>@<version its release ships>`, whatever that component's C218 label. One version is one component, labelled with the project compiler. Today the pass looks up (label, name), so on 1 to 7 bootstrap-compiler names per golden it reports `absent` a component the document already has.
2. **Close the set.** Before drawing edges, the pass follows each compiler's present boot libraries through the dependency table to a fixed point. It synthesizes every reached PURL that no component holds, with the closure's existing constructor (`compiler_supplied_component`), at milestone 966's version, labelled as the closure labels a shared version (R2, R3).

**Expected effect:**

| | added components | names that already had a component | `boot-library-edges-incomplete` after |
|---|---|---|---|
| moat | 11 | 6 | gone |
| openai | 8 | 1 | gone |
| shellcheck | 10 | 7 | gone, and the document becomes `complete` |
| slack-web | 12 | 7 | gone |
| slacklinker | 9 | 7 | gone |
| stub (with ghc-prim) | 18 | 0 | gone; stays `partial` (hackage) |
| stub (plain) | 19 | 0 | gone; stays `partial` (hackage) |

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde_json` (C247), `tracing`; milestone 966's `boot_table`, spec 1653's `boot_dependencies`, the closure's `ClosureCompilers`. **Zero new Cargo dependencies.**
**Storage**: N/A. Both tables are already embedded; no scan-time network, store or package-database read (FR-010).
**Testing**: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.
- Unit tests in `boot_library_edges` cover closure, labels, PURL lookup and the reason's positive control.
- Integration tests in `tests/boot_library_edges.rs` run on the stub fixture in three formats with `assert_parity`.
- A mutation for each new behaviour.
- One corpus dispatch with `regen_goldens=true`, with every diff traced in `measurements/after.md`.

**Target Platform**: every host; the stub fixture is Unix-only (`#![cfg(unix)]`), as today.
**Project Type**: CLI (single Rust workspace).
**Performance Goals**: no new scan cost. The fixed point is at most 42 libraries per compiler over an in-memory table.
**Constraints**: without `--nix-closure`, output is byte-identical. The pass is already gated on the closure tier having run.
**Scale/Scope**: about 8 to 19 components per closure document.

## Constitution Check

| Principle | Check | Status |
|---|---|---|
| I. Pure Rust, statically linked | No new dependency. | Pass |
| III. Fail closed | A library whose version milestone 966's table lacks is not synthesized versionless. It stays `absent` and the reason reports it, rather than being guessed. | Pass |
| IV. Type-driven | PURLs through `Purl::new`; the compiler label is the closure's `GhcRelease`, not a raw string, at the new parameter. | Pass |
| V. Specification compliance | `pkg:hackage/<name>@<version>` is a valid purl for every added name. No new catalog row (C218 and C247 are reused). Principle V audit: unchanged from spec 1653 R5; no native construct is displaced. | Pass |
| VIII. Completeness | This is the feature: edges and components the compiler release records, which were missing. | Pass |
| IX. Accuracy | Added components are the release's own recorded dependencies, at its recorded versions, reached only from libraries the document already has (FR-001, not the whole set). `rts` and `ghc-platform` OSV matches were measured as 0 (spec FR-004). | Pass |
| Measure first (CLAUDE.md) | Every count in the spec and this plan traces to `measurements/`; the spec's SC-002 was corrected from a measurement. | Pass |

No violations. **Post-design re-check:** unchanged. The design adds no row, no flag and no dependency.

## Project Structure

### Documentation (this feature)

```text
specs/1655-reachable-boot-libraries/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/
    ├── reach.py, reach-before.txt            # (compiler, library) pairs
    ├── shared_purls.py, shared-purls-before.txt  # the PURL correction (R1)
    ├── identity-probe.md                     # Hackage, OSV, the two tables
    ├── stub-before.md                        # stub fixture before-state
    └── after.md                              # written at implement time
```

### Source Code (repository root)

```text
waybill-cli/src/resolve/boot_library_edges.rs            # closure + PURL lookup (the change)
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/
    closure_identity.rs                                  # compiler_supplied_component -> pub(crate)
waybill-cli/src/cli/scan_cmd.rs                          # pass closure_compilers.as_ref() to apply
waybill-cli/tests/boot_library_edges.rs                  # R6: three tests updated, new ones added
docs/reference/sbom-format-mapping.md                    # C247 row text: `absent` no longer "rts and ghc-internal never are"
CHANGELOG.md                                             # [Unreleased]
waybill-cli/tests/fixtures/public_corpus/nix-closure-*/  # regenerated by dispatch only
```

## Design

**`apply(components, relationships, compilers: Option<&ClosureCompilers>)`.** With `None`, behaviour is today's: lookup by label, then by PURL, and no synthesis. `scan_cmd` always passes `Some` when the closure ran.

1. **Index.** Build (label, name) → PURL as today, plus a set of every component PURL in the document.
2. **Close** (only with `Some(cc)`).
   - **Walk.** For each label in the index whose release is in both tables, walk the dependency table from its present names. A name is satisfied when its PURL at that release's 966 version, `pkg:hackage/<n>@<v>`, is in the PURL set, or the (label, name) index has it. Otherwise, when 966 has a version for it, it is queued for synthesis as (PURL, candidate label). The walk continues through its dependencies at the same release.
   - **Label rule** (R3). The project label (`cc.project_in_table()`) is used if that release ships the same version. Otherwise the reaching label is used, and the lexically first label wins on a tie.
   - **Synthesis.** `compiler_supplied_component(name, Some((&release, version)))` builds each queued PURL once. It is pushed, added to the PURL set and to the (label, name) index under its label.
   - **Order.** The walk is breadth-first over a `BTreeMap`/`BTreeSet`, so component order and label choice are deterministic.
3. **Edges and C247**, as today, over all components including the added ones. The present-lookup is (label, name), then the PURL at the depending release's 966 version. Only a name with neither is `absent`.
4. **Summary.** `BootEdgeSummary` gains `added` (components synthesized). The existing INFO line prints it.

**Why the 966 version for the PURL lookup** rather than the 1653 row's version: the two agree wherever 1653's is non-empty (unit test, both directions). 966's is the only one with `rts` on 9.0 and 9.2.

## Phases

- **Phase 0, research**: complete (`research.md` R1 to R6). No open questions.
- **Phase 1, design**: `data-model.md`, `contracts/wire.md`, `quickstart.md`.
- **Phase 2, tasks**: `/speckit-tasks`.

## Complexity Tracking

None.
