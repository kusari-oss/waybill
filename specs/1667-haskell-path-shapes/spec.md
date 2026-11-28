# Feature Specification: Haskell path values have the right shape

**Feature Branch**: `1667-haskell-path-shapes`
**Created**: 2026-11-27
**Status**: Draft
**Input**: #1463 and #1691. C200 `waybill:hpack-read-outcome` records manifests as absolute host paths. `--split` records a Haskell subproject's `source_dir` as the manifest file instead of its directory.

## Background (measured 2026-11-27, `main` at 6084b343)

**C200 in the committed corpus goldens:**

- 9 targets carry C200, in all three formats, so 27 files: the 7 `hpack-*` targets, `nix-closure-moat` and `nix-closure-slacklinker`.
- `grep -c /home/runner` finds exactly those 27 files, one line each, and every such line is the C200 value. No other annotation in any golden holds a host path.
- An example value (`nix-closure-moat`): `manifests-read: ["/home/runner/.cache/waybill/corpus/53983fe1021353b3/<pin>/repo/package.yaml"]`.
- #1463 counted 6 files at `main` 1303ba82. More targets have gained C200 since then, so the exposure is 27 files, not 6.

**Where the paths come from.** `HpackReadOutcome::to_document_value` (`haskell/hpack.rs`) writes each path with `display()`, unchanged from how it was read. The readers join the paths to the canonical scan root.

**A consumer depends on the absolute form.** Spec 1652's `amend_scope_dropped` (`cli/scan_cmd.rs`, around line 4650) joins each dropped main module's manifest to the canonical root and matches it against C200's paths, to fill `manifests-dropped-by-scope`. The test `us3_symlinked_root_c200_matches` pins this for a scan through a symlink.

**`--split`'s `source_dir`.** `generate/split.rs::source_dir_for` returns the parent only when `is_manifest_basename` recognises the file name. The list has no `package.yaml` and no `*.cabal`. #1691 measured, at `main` 317fbae, that every Haskell subproject's `source_dir` is the manifest file (`package.yaml`, `local-packages/a/package.yaml`) under both `--split=workspace` and `--split=directory`.

**Two rules for one question.** Spec 1652 added `project_discovery::root_dir`, which takes the parent whenever `scan_root.join(source_dir)` is a file. It left `split.rs` unchanged because that spec held `--split`'s output fixed (its FR-007). The question "what is this subproject's directory" now has two answers in two places.

## Clarifications

### Session 2026-11-27

- Q: How is a manifest outside the scan root written? → A: Relative to the scan root with `..` segments (for example `../shared/defaults.yaml`), never absolute.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An SBOM does not describe the scanning host (Priority: P1)

An operator publishes an SBOM of a Haskell project. The document names the project's manifests by their place in the project, not by the scanning user's home directory and cache layout.

**Why this priority**: It leaks host details into a published artifact, in 27 corpus documents. It also means a golden regenerated anywhere but CI differs on that one line.

**Independent Test**: Scan a `waybill-fixture-*` hpack project under a temporary directory. No C200 path contains the temporary directory's prefix.

**Acceptance Scenarios**:

1. **Given** an hpack project scanned from any directory, **When** the SBOM is emitted, **Then** every path in C200 is relative to the scan root, in CycloneDX, SPDX 2.3 and SPDX 3.
2. **Given** the corpus, **When** regenerated on the branch, **Then** no golden contains `/home/runner`.
3. **Given** a `--project-discovery` scan that drops a Haskell main module, including one through a symlinked root, **When** emitted, **Then** `manifests-dropped-by-scope` lists the same manifests as on `main`, now relative.

---

### User Story 2 - A Haskell subproject's directory is a directory (Priority: P1)

An operator splits a multi-package Haskell repository. The split manifest names each subproject's directory, and `--split=directory` groups packages that share one.

**Why this priority**: `source_dir` is a file for every Haskell subproject, so `--split=directory` can't group by directory for Haskell.

**Independent Test**: A fixture with a root `package.yaml`, nested hpack packages under `local-packages/{a,c,d}`, and a directory holding a `package.yaml` beside a hand-written `.cabal`.

**Acceptance Scenarios**:

1. **Given** that fixture, **When** scanned with `--split=workspace`, **Then** each Haskell subproject's `source_dir` is a directory: empty for the root, `local-packages/a` and so on for the rest.
2. **Given** two Haskell main modules in one directory, **When** scanned with `--split=directory`, **Then** they are grouped into one sub-SBOM.
3. **Given** a non-Haskell subproject (Cargo, npm and so on), **When** split, **Then** its `source_dir`, its sub-SBOM file name and its grouping are unchanged.

### Edge Cases

- **A manifest outside the scan root**: hpack's `include` can name a file above the root. It is written relative to the root with `..` segments (`../shared/defaults.yaml`), so no host prefix appears. Measured: no corpus target has one; every C200 path in all 9 targets sits under that target's scan root.
- **A symlinked scan root**: the paths are relative to the root the readers resolved, so they match spec 1652's dropped-by-scope comparison either way.
- **A `source_dir` that doesn't exist on disk**: kept as is, unless its file name is a listed manifest (`Cargo.toml` and so on), whose parent is taken as it is today.
- **A root-level main module**: `source_dir` is empty, as it already is for a root `Cargo.toml`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Before any code changes, measure on `main`:
  - every C200 value in every corpus golden, in all three formats;
  - `split-manifest.json` under `--split=workspace` and `--split=directory` for the US2 fixture.

  Then measure the branch the same way. Commit the probe and its output, and explain every difference.
- **FR-002**: C200's paths are relative to the scan root. This covers every path-bearing key: `manifests-seen`, `manifests-read`, `manifests-unread[].file`, `manifests-partial[].file`, `generated-cabal-suppressed`, `manifests-dropped-by-scope`, and the `manifest` and `file` of each `includes-resolved` and `includes-unresolved` entry (spec 1649). A path outside the root is written relative to it with `..` segments; no C200 path is ever absolute.
- **FR-003**: C200's value is otherwise unchanged: the same keys, the same order and parity-equal across CycloneDX, SPDX 2.3 and SPDX 3. If its shape changes, the catalog row and the extractor change in the same commit.
- **FR-004**: `manifests-dropped-by-scope` identifies the same manifests as on `main`, including through a symlinked root. Spec 1652's symlink test still passes.
- **FR-005**: One rule decides a subproject's directory, and both `--split` and `--project-discovery` call it: when the scan-root-relative path names a file, its parent; otherwise the path. One rule rather than adding Haskell names to `is_manifest_basename`, because a filesystem test covers every ecosystem's manifest without anyone keeping a list, and two rules for one question have already disagreed once (#1691).
- **FR-006**: For non-Haskell subprojects, `--split`'s `source_dir`, sub-SBOM file names and `--split=directory` grouping don't change. To be measured: every committed `--split` golden is unchanged.
- **FR-007**: Positive control on `waybill-fixture-*` projects:
  - a scan under a tempdir has no C200 path containing that prefix, including a manifest included from above the root;
  - a split fixture has each Haskell `source_dir` be a directory;
  - two main modules in one directory group under `--split=directory`.

  Recorded mutations kill the relativisation and the directory rule.
- **FR-008**: Corpus goldens change only where FR-001's measurement explains it. The expected change is the C200 line in the 27 files. No invariant declaration moves. Zero new Cargo dependencies.

### Key Entities

- **Scan-root-relative path**: a path as every other per-file annotation records it, with no host prefix.
- **Subproject directory**: the directory a subproject's manifest sits in, decided by one rule.

## Success Criteria *(mandatory)*

- **SC-001**: 0 corpus goldens contain a host path. Today it is 27.
- **SC-002**: 0 Haskell subprojects in the split fixture have a file as `source_dir`. Today it is every one.
- **SC-003**: Two Haskell main modules in one directory produce 1 sub-SBOM under `--split=directory`. Today they produce 2. Inference from #1691's cause: not yet measured, so FR-001 measures it.
- **SC-004**: 0 non-Haskell `--split` outputs change.
- **SC-005**: Each of the two behaviours fails under at least one recorded mutation, and every corpus difference between `main` and the branch is explained.

## Assumptions

- The readers' scan root and spec 1652's canonical root are the same root, so relativising against it keeps the dropped-by-scope comparison exact.
- The golden masking (`mask_nondeterministic`) needs no change once no host path is emitted.

## Out of Scope

- Any other annotation's path form (none holds a host path).
- `--split` file naming beyond `source_dir`.
- #1744, #1686.
