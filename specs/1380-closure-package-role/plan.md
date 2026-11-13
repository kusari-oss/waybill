# Implementation Plan: A closure member is a `package` only when it is one

**Branch**: `1380-closure-package-role` | **Date**: 2026-11-13 | **Spec**: [spec.md](spec.md) | **Issue**: #1374

## Summary

C206 assigns `package` to any nameless closure member whose name holds a version-shaped token. On the goldens, 50 of moat's 73 `package` members are version checks, test programs or source trees.

The fix adds two evidence rungs and two name-rule rungs to `member::classify`, plus a fifth role, `check`, which is a graph endpoint.

- **Evidence rungs** (research R2, R3):
  - **Patches:** a member whose output another derivation lists in `patches` is `source`.
  - **Source-only trees:** a member whose `phases` are exactly `unpackPhase patchPhase installPhase` (nixpkgs `srcOnly`) is `source`.
- **Name rules**, admitted because no derivation evidence separates these groups (R1, R4):
  - `-get-version-<v>` and `-simple-program-<v>` are `check`;
  - `-source`, `-src-<v>` and `-vendor` are `source`.
- **Every move is between graph-endpoint roles**, so the dependency graph is unchanged (FR-006).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: Existing only. `serde_json` for the C184 counts, and the closure parser's `RawDerivation::paths_in`. **Zero new Cargo dependencies.**
**Storage**: N/A, in-process per scan.
**Testing**: `cargo test` unit tests in `member.rs`. A fixture closure test under `waybill-cli/tests/`, with synthetic `waybill-fixture-*` names, and corpus golden regeneration through `public-corpus.yml`.
**Target Platform**: Every host the closure tier runs on. The rules read derivation JSON only.
**Project Type**: CLI (`waybill sbom scan --nix-closure`).
**Performance Goals**: The patches set is built once per closure, in time linear in the declared `patches` paths. The classifier is unchanged otherwise.
**Constraints**:
- Edges are byte-identical (FR-006).
- Non-closure goldens are byte-identical (FR-009).
- C206 stays `SymmetricEqual` in parity.
**Scale/Scope**: The 5 closure goldens. Per research, 54 distinct check derivations, 9 named source trees and 5 patch names across 3 probed closures.

## Constitution Check

| Principle | Status | Note |
|---|---|---|
| III Fail Closed | Pass | A member no rung claims stays `fragment`, as today. A name rule only moves a member between endpoint roles, and never invents an identity. |
| IV Type-Driven Correctness | Pass | `MemberRole` gains a variant. `ALL` is an array, so its length makes a forgotten count a compile error. |
| V Specification Compliance | Pass | C206 is a `waybill:` annotation with no native equivalent in any of the three formats (catalog row unchanged in kind). |
| VII Test Isolation | Pass | Fixtures are synthetic. The research probe evaluates nixpkgs only, and no test evaluates anything. |
| IX Accuracy | Pass | This is the point of the change. Every rule is checked against the census and the probe, with no false positives found (R4). |
| X Transparency | Pass | The C206 catalog row states each rule and whether it is evidence or name shape (FR-008). C184 counts `check`. |
| XII External Data | N/A | No network at scan time. |

**Post-design re-check**: Pass. The design adds no state and no dependency. The one consumer-visible extension, a fifth enum value, was settled in clarification Q1.

## Project Structure

### Documentation (this feature)

```text
specs/1380-closure-package-role/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/member-role.md
├── checklists/requirements.md
└── measurements/
    ├── package-role-census.sh, package-role-census-2026-11-13.txt
    └── member-evidence-probe.sh, member-evidence-probe-2026-11-13.txt
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/closure/
├── member.rs        # MemberRole::Check, PatchOutputs, classify(drv, &PatchOutputs), rungs
├── emit.rs          # build PatchOutputs once; pass to the two classify call sites
├── graph.rs         # pass PatchOutputs to the edge-endpoint test
└── summary.rs       # C184 member-roles gains "check" via MemberRole::ALL (no code change expected)
waybill-cli/src/cli/debug_cmd.rs   # closure-dump passes PatchOutputs
waybill-cli/tests/closure_member_role.rs   # new fixture test (US1–US3)
waybill-cli/tests/fixtures/closure_member_role/closure.json
docs/reference/sbom-format-mapping.md     # C206 row: five values, rules; C184 note
docs/reference/nix-evaluation.md          # role section
waybill-cli/tests/fixtures/public_corpus/nix-closure-*/   # regenerated
```

**Structure Decision**: Single crate. The work stays inside the closure module where the classifier lives. `PatchOutputs` is a closure-wide input, so it is built once per closure where `output_index` is built, and passed to `classify`. A per-derivation lookup would repeat the scan.

## Complexity Tracking

None.
