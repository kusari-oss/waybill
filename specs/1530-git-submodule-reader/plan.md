# Implementation Plan: Git submodules as components

**Branch**: `1530-git-submodule-reader` (base `main` at `fbaf3a6d`) | **Date**: 2026-11-15 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1530-git-submodule-reader/spec.md`

## Summary

Issue #1515 finishes milestone 105's User Story 6.

A new `git_submodule` reader runs only when the shared walker saw a `.gitmodules` file. For each repository top level holding one, it:
- parses `.gitmodules` itself, rejecting unsafe paths and URLs;
- reads the repository's gitlinks with one hardened `git ls-tree -r HEAD`, never an index-reading command (research R1 measured `ls-files` and `status` running a repository's `core.fsmonitor`);
- joins the two by path;
- emits one component per gitlink, pinned to its commit, with forge PURL types shared with the Yocto reader;
- recurses into initialised submodules.

Wiring:
- **Root edges.** Top-level submodules attach to the document root through #1009's root-anchor channel.
- **Nested edges.** A new exact-PURL `depends` resolution expresses them.
- **C57** gains CMake path references.
- **Coverage.** A new document-scope row, C222, records pinned and skipped counts by reason.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only: `serde`/`serde_json`, `regex`, `tracing`; the milestone-075 `identifiers::sanitize::sanitize_userinfo`. External: the host's `git`, already a runtime assumption (milestone 053). **Zero new Cargo dependencies.**
**Storage**: N/A. Everything is in process for one scan.
**Testing**: `cargo test`.
- **Unit tests:** `.gitmodules` parsing, URL to PURL, the join, and reason counting.
- **Integration tests:** they build real repositories in a tempdir with `git init` / `submodule add` over `file://`, including a fresh clone, a hostile `core.fsmonitor`, and `GIT_TEST_ASSUME_DIFFERENT_OWNER=1`.
- **Goldens:** the corpus goldens regenerated in CI.

**Target Platform**: Linux, macOS and Windows hosts, as today. Windows needs `git` on `PATH` just as the golang `describe` ladder does.
**Project Type**: CLI (single binary).
**Performance Goals**:
- Zero cost when no `.gitmodules` file exists, because the walker gates the reader.
- Otherwise one `ls-tree -r` per repository. Research R4 measured a 0.314 s median at 227,970 entries.
- Budget: 30 s per call.

**Constraints**:
- Offline.
- No program chosen by a scanned repository may run (FR-012, SC-010).
- `safe.directory` is never overridden (R2).
- No repository above the scan root is read (R14).

**Scale/Scope**: TheRock pins its submodules as gitlinks; issue #1515 measured 36 gitlinks against 62 declared paths on one large tree. No public-corpus target has a gitlink (R13).

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates. `git` is invoked as an external tool, as the golang and gem readers already do. |
| III. Fail Closed | Pass. Nothing is guessed: no gitlink, no component. Missing `git`, a refused repository, or missing repository data is recorded in C222, never filled in from `.gitmodules`. A hostile config cannot run code (R1). |
| IV. Type-Driven Correctness | Pass. The skip reason is a closed enum serialised in one place. C57 stays a closed two-value enum. |
| V. Specification Compliance | Pass with one recorded deviation (see Complexity Tracking). purl-spec's `types/` directory, checked 2026-11-15, defines `github`, `bitbucket` and `generic`, and those follow their definitions: `github` and `bitbucket` owner and repository are lowercased (FR-004), and `generic` carries `vcs_url`. `gitlab` and `codeberg` have no definition and are used for consistency with the existing Yocto and Nix readers (clarification 3). No `git+https` type is invented. C222 is added with extractors in all three formats, per the catalog rule. |
| VII. Test Isolation | Pass. Tests build local repositories in tempdirs over `file://`, with no network and no clone of a remote. TheRock is measured only in CI. |
| VIII. Completeness | Pass. This closes #1515's zero-component gap. A top-level submodule is anchored to the root even when the root has other edges, the #1009 lesson. |
| IX. Accuracy | Pass. Counting gitlinks, not `.gitmodules` lines, avoids 26 false components on the tree #1515 measured. Different PURLs are not merged by guesswork (FR-009). |
| X. Transparency | Pass. C222 makes skipped work visible in the document itself, and the probes are committed under `measurements/`. |

No violations, so Complexity Tracking is empty. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1530-git-submodule-reader/
├── spec.md                  # amended during planning: R2, R13, R14 (marked inline)
├── plan.md
├── research.md              # R1–R15
├── data-model.md
├── quickstart.md
├── contracts/wire.md        # PURL forms, edges, C57 amendment, C222
├── checklists/requirements.md
└── measurements/
    ├── probe/git-safety.sh  # R1–R3
    ├── git-safety.txt
    ├── ls-tree-timing.txt   # R4
    ├── corpus-gitmodules.tsv # R13
    └── corpus-gitlinks.tsv   # R13
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── git_submodule/                 # NEW reader
│   ├── mod.rs                     # registration (walker marker: **/.gitmodules), finalize, recursion, C222 counts
│   ├── gitmodules.rs              # .gitmodules parser + unsafe-value rejection (R7)
│   ├── gitlinks.rs                # hardened `git ls-tree` invocation + parse (R1, R2, R15)
│   └── identity.rs                # URL → PURL, relative-URL resolution (R8, R9)
├── forge.rs                       # NEW shared host → PURL-type table (R9); yocto/recipe.rs calls it
├── yocto/recipe.rs                # uses forge.rs; output byte-identical
├── cmake.rs                       # wire collect_find_package_targets; add path-reference collector (R10); lowercase pkg:github/pkg:bitbucket owner+repo (FR-015, R16)
└── mod.rs                         # register + finalize the reader
waybill-cli/src/scan_fs/mod.rs           # exact-PURL `depends` resolution (R6)
waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs   # run_bounded generalised over the program (R15)
waybill-cli/src/supplement/mod.rs        # root anchors = supplement anchors ∪ top-level submodules (R5)
waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs  # C222 extractors
waybill-cli/src/generate/…               # C222 document annotation, in each emitter's doc-scope path
docs/reference/sbom-format-mapping.md    # C57 amended; C222 added
waybill-cli/tests/git_submodule_*.rs     # integration tests (tempdir repositories)
waybill-cli/tests/dedup_precedence_determinism.rs  # un-ignore; assert FR-009 (two components) + the merging fixture
waybill-cli/tests/fixtures/public_corpus/haskell-language-server/  # regenerated in CI (R13)
CHANGELOG.md
```

**Structure Decision**: A single project. One new reader module, one small shared forge table, and three narrow changes to shared plumbing (exact-PURL `depends`, root anchors, the bounded runner). Each is required by a research item, and each is byte-identical when no submodule is present.

## Approach

1. **Measurements re-verified at implementation** (memory rule: re-verify research claims). Re-run the R6 grep (no reader puts `pkg:` into `depends`) and `probe/git-safety.sh`.
2. **Shared plumbing, each with a no-op control.**
   - Generalise `run_bounded` (R15).
   - Add the exact-PURL `depends` lookup (R6).
   - Make the anchor source a union (R5).
   - Lift the forge table out of the Yocto reader (R9).
   - Control: the full gate passes with zero golden churn before any reader code exists.
3. **The reader, written test-first.**
   - Parser tests from the spec's acceptance scenarios, run red first.
   - Integration fixtures built in tempdirs: one submodule (checked out and as a fresh clone), more declarations than gitlinks, nested submodules, a gitlink with no declaration, credentials, a hostile `core.fsmonitor` (SC-010), `GIT_TEST_ASSUME_DIFFERENT_OWNER=1` (`git-refused`), no `.git` (`no-repository-data`), and `git` missing from `PATH` (`git-unavailable`).
4. **C57** (R10). Wire both collectors, with the three-submodule fixture of SC-008.
5. **C222.** The catalog row, the emitters and the extractors. The parity tests and `every_catalog_row_has_an_extractor` must pass.
6. **Dedup** (R12). Un-ignore and rewrite the `dedup_collision` test (two components), add the merging fixture, and run 10 scans of each for byte-identity.
7. **Mutations.** Each must fail a test:
   - use `ls-files` instead of `ls-tree` (the SC-010 test must catch it);
   - emit per declaration instead of per gitlink;
   - read the commit from the checkout instead of the gitlink;
   - drop the root anchor;
   - drop the credential stripping.

   Each is reverted with `cp`.
8. **Goldens.**
   - Locally, nothing should change; the gate is the check.
   - In CI, dispatch the corpus regeneration, then review a normalised diff. Only the three `haskell-language-server` files should move, each gaining C222 (R13).
   - TheRock (SC-003): capture `git ls-tree -r b1cae390… | grep -c '^160000'` and the reader's component count in a branch-only CI step, never locally.
9. **Docs.** The C57 row, the C222 row, and the CHANGELOG, not adjacent to another unreleased entry.
10. **Gate.** `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.

## Cataloger-interface notes (#1545)

- **What core keeps.** This reader covers only git's own mechanism. TheRock's subproject topology and its fetch functions stay a cataloger concern (#1539).
- **What a `CatalogOutput` needs, observed here:**
  - root-anchored components, without knowing the root;
  - edges by exact PURL across PURL types;
  - a document-scope coverage record with per-reason counts.

  The plumbing added for R5 and R6 is therefore exactly what an external cataloger would need too.

## Complexity Tracking

| Deviation | Why needed | Simpler alternative rejected because |
|---|---|---|
| `pkg:gitlab` and `pkg:codeberg` are emitted although purl-spec publishes no definition for them (checked 2026-11-15). | The Yocto (`yocto/recipe.rs`) and Nix (`nix/identity.rs`) readers already emit them. One repository should have one identity across readers (clarification 3). | `pkg:generic/…?vcs_url=` for those hosts would split one repository's identity between this reader and the two existing ones. If the deviation should be removed, it should be removed for all three readers together, in a separate change. |
