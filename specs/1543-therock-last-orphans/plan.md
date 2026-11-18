# Implementation Plan: TheRock's last two unexplained components

**Branch**: `1543-therock-last-orphans` | **Date**: 2026-11-18 | **Spec**: [spec.md](spec.md)

## Summary

The FR-001 trace (`measurements/trace.md`) settled the open questions.

**#1601.** A synthesized workspace root is stranded in 7 of 12 measured shapes, across all three readers that make one (uv, bun, Kotlin DSL). The cause: the graph-top fallback is the only thing that attaches one, and anything that gives the root an edge closes the fallback. Since spec 1541, the uv and bun cases carry a false `foreign-build-system`; the Kotlin case carries nothing. At the scan root, uv and bun even pick a member as the subject.

**#1602.** `CMakeLists_wsl.txt` is never read, because the reader takes only `CMakeLists.txt` and `*.cmake`. No condition crosses an `include()`, and an installed-library record is attached only through `find_package`.

**Fixes:**
- **R1:** a synthesized workspace root that nothing targets becomes a root anchor.
- **R2:** at the scan root, exactly one such root is the subject.
- **R3:** link declarations are read through `include()`, carrying the include site's conditions.
- **R4:** a link directory inside a record's prefix, plus a matching name or library file (a DVC pointer counts), links the record.
- **R5:** the edge comes from the linking file's owner. A record linked only conditionally is Optional, with a new C122 value and a new row C235 `waybill:cmake-link-conditions`.
- **R6:** no DVC annotation.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: Existing only:
- `root_edges` anchors, `root_selector`, `declared_attach::OwnerIndex`;
- `cmake/blocks.rs::BlockStack`, the `installed_lib::location::prefix_of` helper;
- `regex`, `serde_json`, `tracing`.

**Zero new Cargo dependencies** (FR-010).

**Storage**: N/A. In process per scan.

**Testing**:
- **Unit:**
  - the anchor collection;
  - the root-selector rung (one, zero and two workspace roots at the root);
  - `include()` resolution (variables, module names, cycles, depth);
  - link parsing (plain items, keywords, generator expressions, target pairing);
  - correlation (name match, file match, DVC, outside every prefix);
  - the scope merge.
- **Integration:** `waybill-cli/tests/scan_therock_last_orphans.rs`, cases W-1..W-8 and L-1..L-11 (`contracts/wire.md`), each asserted in all three formats with the corpus harness's own functions (`row_all_formats`, `probe_i7_on_document` logic). The determinism cases repeat 20 scans, as in spec 1542.
- **Mutations:** each rule removed or inverted (`measurements/mutations.md`):
  - the anchor;
  - the subject rung;
  - the include-site conditions;
  - the prefix test;
  - the name/file test;
  - the DVC match;
  - the owner lookup (forced to root);
  - the scope rule.
- **CI:**
  - the public corpus at the branch and `main`, with `repeat=2`;
  - the quality corpus against `main`;
  - TheRock via `measure-source` (interleaved, ≥5 pairs) and `measure-full` once.

**Target Platform**: The waybill CLI on Linux, macOS and Windows. No platform-gated code.

**Project Type**: The existing Rust workspace (`waybill-cli`).

**Performance Goals**: The median TheRock source scan is ≤ 1.1× `main`'s (SC-006). The new work reads a handful of extra included files, and does one linear pass over link declarations.

**Constraints**:
- byte-identical output for any document with no workspace root and no link-directory correlation;
- the public corpus gate stays green.

**Scale/Scope**: TheRock source tree: 2 target components. Public corpus: 0 workspace roots in 21 goldens (trace §1).

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies, no subprocess. |
| III. Fail closed | ✅ An unresolved `include()`, an unknown variable or a directory outside every prefix yields nothing; nothing is guessed (FR-006). |
| IV. Type-driven correctness | ✅ Link declarations are a typed record; scope uses the existing `LifecycleScope` merge. |
| V. Specification compliance | ✅ Scope is native: CycloneDX `scope`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`. The C235 audit is recorded (no native carrier for a build condition). The row and its extractor land together. |
| VII. Test isolation | ✅ Synthetic `waybill-fixture-*` trees in temp dirs. TheRock is never scanned locally. |
| VIII. Completeness | ✅ Two components gain the edges their builds declare. |
| IX. Accuracy | ✅ Removes a false `foreign-build-system`, and an edge that claimed nothing about its condition is not added: the edge is Optional with its condition. |
| X. Transparency | ✅ C122 names the derivation; C235 names the conditions. |

No violations. Re-checked after design: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1543-therock-last-orphans/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/wire.md
├── measurements/
│   ├── trace.md
│   ├── workspace-roots.sh   # the FR-001 probe
│   └── workspace-roots.out
└── tasks.md                 # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/
├── scan_fs/
│   ├── mod.rs                       # R1: workspace-root anchors; R5: link post-pass
│   ├── cmake_link_attach.rs         # new: R4 correlation + R5 owner/scope
│   └── package_db/cmake.rs          # R3/R4: collect link declarations
│       └── cmake/links.rs           # new: link + include() parsing
├── generate/root_selector.rs        # R2: subject rung
└── parity/extractors/…              # C235 extractor
docs/reference/sbom-format-mapping.md  # C235 row; C122 value
docs/ecosystems.md                     # CMake: links, includes
waybill-cli/tests/scan_therock_last_orphans.rs
```

**Structure Decision**:
- **Links** are a sibling of `fetches.rs` under `cmake/`, reusing `BlockStack`.
- **Correlation** is a post-pass sibling of `cmake_fetch_attach.rs`, reusing `OwnerIndex`, because both attach CMake declarations to owners.

## Complexity Tracking

None.
