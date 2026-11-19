# Wire contract

Every case is asserted in CycloneDX, SPDX 2.3 and SPDX 3 through the parity extractors for C42 and C62. "Required" means no non-runtime scope and no C62. "transitive" means the propagated scope with C62 `transitive`. Packages are `waybill-fixture-*`. The fixture trees use a pip `uv.lock` with a PEP 735 dev group to make declared dev seeds, unless a case says otherwise.

| Case | Tree | Expected |
|---|---|---|
| P-1 | projects `a` (seeded) and `b`; `b` declares `shipped` at runtime; `a`'s dev group holds `devtool`, which depends on `shipped`; `a` itself declares `a-runtime` at runtime, which `devtool` also reaches | `shipped` required (FR-001); `a-runtime` required (the base seeds still protect, FR-005) |
| P-2 | P-1, and `shipped` depends on `shipped-dep` | `shipped-dep` required: `b`'s runtime closure |
| P-3 | P-1, and `devtool` also depends on `devonly`, which no project declares at runtime | `devonly` transitive (dev), as on `main` |
| P-4 | a third project `c` that is itself non-runtime (declared in another project's dev group), depending on `cdep`, which `devtool` also reaches | `cdep` transitive (FR-002) |
| P-5 | a project whose only manifest is under `.github/actions/x/`, depending on `actdep`, which `devtool` also reaches | `actdep` not protected by that project (FR-002, R2) |
| P-6 | `b` reaches `shipped` only through an inferred cross-ecosystem edge (`--experimental-cross-ecosystem-edges`) | `shipped` not protected by it (FR-003) |
| P-9 | a non-top npm project `n` declares `waybill-fixture-shared` at runtime; a pip dev-group member reaches the same npm package over a declared edge | `waybill-fixture-shared` required (cross-ecosystem edge case). If no reader emits a declared pip → npm edge, record that in `trace.md`, and assert the case as a unit test in `SP` instead. |
| P-10 | P-1 scanned with `--root-name waybill-fixture-root` | same as P-1: main modules are seeds under an operator override (edge case; research R1) |
| P-7 | a tree with one main module per ecosystem | byte-identical to a golden of `main`'s output (FR-008) |
| P-8 | P-1 | the propagation summary log reports `immune_by_added_seeds=2`: `shipped` and `shipped-dep`, both protected by `b` alone (FR-006; corrected at T015 from 1, an arithmetic slip) |
| P-rep | P-1 to P-5 in one tree, scanned 20 times | identical |
