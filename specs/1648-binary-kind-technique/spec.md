# Feature Specification: Dynamic linkage and embedded source paths are labelled binary analysis (#1668)

**Feature Branch**: `1648-binary-kind-technique`
**Created**: 2026-11-23
**Status**: Draft
**Input**: #1668, the follow-up spec 1646 (#1665) scoped out. Every evidence kind read from a binary, other than version banners and symbol fingerprints, still reports `manifest-analysis`. Refs #1665; tracked in #1543.

## Background

Spec 1646 gives a component the CycloneDX technique `binary-analysis` when its evidence kind is `embedded-version-string` or `symbol-fingerprint` (`waybill-cli/src/scan_fs/mod.rs::entry_technique`, keyed on `resolve/deduplicator.rs::BINARY_KINDS`). Every other scanned entry is converted with one technique, `PackageDatabase`. That is CycloneDX `manifest-analysis`, and `package-database` in the SPDX `evidence.identity` annotation (D1). The same list also gates spec 1646's merge absorption: a component that merges with a binary match lists both methods.

**What CycloneDX 1.6 means by each technique.** The XSD documents each value (`bom-1.6.xsd`, tag `1.6`, simpleType `evidenceTechnique`); the JSON and protobuf schemas only name them:
- `binary-analysis`: "Examines a compiled binary through reverse engineering, typically via disassembly or bytecode reversal."
- `manifest-analysis`: "Examines a package management system such as those used for building software or installing software."
- `source-code-analysis`: "Examines the source code without executing it."

**Measured, spec 1646's census** (`census-versions` run 45116219040 at `db322f49`; `specs/1646-merged-binary-evidence/measurements/census.md`). Counts are events / distinct PURLs:

| Evidence kind | TheRock SDK | Quality corpus |
|---|---|---|
| `embedded-source-path` (spec 1534: a package named by source paths compiled into the binary) | 645 / 15 | — |
| `dynamic-linkage` (a `DT_NEEDED` entry) | 49 / 49 | pants-backend-ai 8 / 8, cpp-mongo 3 / 3 |
| labelled "go-buildinfo" | — | go-kubernetes 38 / 38, go-cobra 1 / 1 |

`cargo-auditable` and `elf-note-package` produced no events there.

**Measured, public corpus goldens at `main` `33c445f6`.** Every `waybill:evidence-kind` value in the 21 CycloneDX goldens: `cargo-auditable` 540 (all in `nix-closure-slacklinker`), `cabal-pkg-descriptor` 316, `hpack-manifest` 95, `nix-flake-lock` 26, `stack-yaml-lock` 2. There is no `dynamic-linkage`, `embedded-source-path` or `elf-note-package` component.

**A correction to the census's Go label (read from the code at `33c445f6`).**
- The census's "go-buildinfo" count keyed on `detected_go`.
- `detected_go` is documented as set "on a file-level binary component when the Go BuildInfo extractor also matched", but `golang/legacy.rs:1076` (`build_main_module_entry`) also sets it on a Go **source-tree** main module.
- The dependency entries the Go binary reader emits (`package_db/go_binary.rs`) set neither `detected_go` nor an evidence kind.
- So the 38 go-kubernetes and 1 go-cobra events are probably source main modules, not build info (inference; FR-001 measures it). Today a component read from a Go binary's BuildInfo has no field that says so.

## Clarifications

### Session 2026-11-23

- Decided in the feature description (not asked): (a) `dynamic-linkage` → `binary-analysis`; (b) `embedded-source-path` → `binary-analysis`; (c) embedded manifests (Go BuildInfo, `cargo-auditable`, ELF package notes) stay `manifest-analysis`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A library inferred from a binary's own structure says so (Priority: P1)

A consumer reads a component's identity evidence to judge how much to trust it. A shared library named only because a binary's `DT_NEEDED` entry names it, or a package named by source paths compiled into a binary, today claims `manifest-analysis`, as if a package manager had listed it. No package manager did.

**Why this priority**: This is the defect. On the ROCm SDK it affects 64 distinct components (49 `dynamic-linkage`, 15 `embedded-source-path`).

**Independent Test**: Scan synthetic `waybill-fixture-*` binaries that produce one `dynamic-linkage` component and one `embedded-source-path` component. Each reports `binary-analysis` in all three formats.

**Acceptance Scenarios**:

1. **Given** an ELF binary whose `DT_NEEDED` names `libwaybill-fixture-dep.so.1`, **When** it is scanned, **Then** the `dynamic-linkage` component for that library reports technique `binary-analysis` in CycloneDX `evidence.identity[].methods[]`, and `binary-analysis` in the SPDX 2.3 and SPDX 3 D1 annotations.
2. **Given** a binary whose source paths name a package (spec 1534's shape), **When** it is scanned, **Then** that `embedded-source-path` component reports `binary-analysis` in all three formats.
3. **Given** either component, **When** it is scanned, **Then** its confidence, evidence kind and every other field are as on `main`.

---

### User Story 2 - A manifest the toolchain embedded keeps manifest analysis (Priority: P1)

A Go binary's BuildInfo, a Rust binary's `cargo-auditable` section and an ELF package note are manifests that the build system wrote into the binary. waybill reads them as manifests. Their technique stays `manifest-analysis`.

**Why this priority**: Relabelling them would be wrong under the definitions above. On the public corpus it would also change 540 golden components for no gain.

**Independent Test**: Scan fixtures producing a `cargo-auditable` component, an `elf-note-package` component and Go BuildInfo dependencies. Each is byte-identical to `main`.

**Acceptance Scenarios**:

1. **Given** a binary with a `cargo-auditable` section, an ELF package note or Go BuildInfo, **When** it is scanned, **Then** the components it yields report `manifest-analysis` (`package-database` in SPDX D1), exactly as on `main`.

---

### User Story 3 - A merge keeps the binary method visible (Priority: P2)

Spec 1646 made a component that merges with a banner or fingerprint match list both methods. A `dynamic-linkage` or `embedded-source-path` component that merges with a record (pkg-config, CMake, a package database) is the same situation.

**Why this priority**: It keeps spec 1646's rule uniform: a merged component lists every technique that identified it. Whether any such merge happens on real trees is counted first (FR-001).

**Independent Test**: A synthetic tree where a `dynamic-linkage` component and a record share a PURL. The merged component lists `manifest-analysis` then `binary-analysis`.

**Acceptance Scenarios**:

1. **Given** a record and a `dynamic-linkage` component with the same PURL, **When** they merge, **Then** CycloneDX lists both methods, and SPDX D1 adds `methods` with both, as spec 1646 does for a banner match.
2. **Given** a record that merges with a `cargo-auditable` or Go BuildInfo component, **When** they merge, **Then** nothing is absorbed: both sides are `manifest-analysis`, as on `main`.

---

### User Story 4 - Nothing else changes (Priority: P2)

Components of every other evidence kind produce the same documents as on `main`.

**Why this priority**: The conversion runs on every component of every scan. Every change must be traceable to one component and one kind.

**Independent Test**: Every public-corpus, quality-corpus and TheRock difference against a `main` control is traced, component by component.

**Acceptance Scenarios**:

1. **Given** a tree with no `dynamic-linkage` or `embedded-source-path` component, **When** it is scanned, **Then** the output is byte-identical to `main`'s.
2. **Given** the public corpus, the quality corpus and TheRock's SDK fixture, **When** each is scanned at this branch and at `main`, **Then** every difference is a technique or methods change on a component of a changed kind.

---

### Edge Cases

- **A `dynamic-linkage` component that also has a package-database record** (the library is owned by dpkg, rpm or apk). The record and the linkage component merge, so the merged component lists both methods (User Story 3). FR-001 counts how often this happens.
- **The file-level component of a binary** (`pkg:generic/<file>?file-sha256=…`). It is the binary itself, found by walking the tree, not identified from another binary's content. Its technique is unchanged.
- **A Go source-tree main module with `detected_go`.** It is not build info. Its technique is unchanged, and the census no longer counts it as build info (FR-005).
- **An evidence kind added later.** It keeps `manifest-analysis` until a spec decides it. The decision is one exhaustive list, so a new kind is a visible choice rather than a silent default (FR-006).
- **A component whose evidence kind is absent.** As on `main`: `manifest-analysis`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (census before design)**: Before output changes, a log-only build MUST count the facts the other requirements depend on. It runs on TheRock's SDK fixture (CI only, through kusari-sandbox/waybill-fixture-therock-sdk, dispatched `--ref main` with full SHAs and every `refs.txt` verified), the quality corpus and the public corpus. It counts:
  - components whose technique would change, by evidence kind, as distinct PURLs and as events;
  - merges whose methods would change (User Story 3), with both sides' evidence kind and mechanism;
  - Go components that really come from a binary's BuildInfo, identified by their origin in the Go binary reader, so that spec 1646's "go-buildinfo" figures are recounted (FR-005).

  The public corpus harness keeps no census log, so there the count is the golden differences at the wired commit, each traced. A positive control (a synthetic tree with one component of each kind: `dynamic-linkage`, `embedded-source-path`, `cargo-auditable`, `elf-note-package`, Go BuildInfo) shows each count firing, so a zero means absence. Where a count contradicts a requirement below, the requirement MUST be amended in this spec with the evidence before implementation.

- **FR-002 (binary analysis)**: A component whose evidence kind is `dynamic-linkage` or `embedded-source-path` MUST report technique `binary-analysis`: in CycloneDX `evidence.identity[].methods[].technique`, and as `binary-analysis` in the SPDX 2.3 and SPDX 3 D1 annotations. Its confidence is unchanged.

- **FR-003 (embedded manifests unchanged)**: Components from Go BuildInfo, `cargo-auditable` and ELF package notes (`elf-note-package`) MUST keep `manifest-analysis` (`package-database` in SPDX D1), byte-identical to `main`.

- **FR-004 (merges)**: The two kinds in FR-002 MUST count as binary for spec 1646's merge absorption. A component that merges with one of them lists both methods, in the order and form spec 1646 defines. A merge between two `manifest-analysis` sides absorbs nothing.

- **FR-005 (census correction)**: Spec 1646's `binary_derived_kind` census event MUST count as Go build info only components that come from a Go binary's BuildInfo, not components with `detected_go` from a source tree. The census records the corrected counts next to spec 1646's.

- **FR-006 (one decision per kind)**: Which evidence kinds are binary analysis MUST be decided in one place, covering every evidence kind a binary reader emits, so a new kind cannot become `binary-analysis` or stay `manifest-analysis` without a recorded decision. A test fails if a binary reader's evidence kind is missing from the decision.

- **FR-007 (no new field)**: No new annotation, catalog row or output field. The technique and methods are the existing ones (spec 1646).

- **FR-008 (byte identity)**: Output MUST be byte-identical to `main` for every component whose evidence kind is not `dynamic-linkage` or `embedded-source-path`, and that does not merge with one.

### Key Entities

- **Evidence kind**: the reader's label for how a component was identified (`waybill:evidence-kind`). This spec decides the technique for each kind a binary reader emits.
- **Technique**: the CycloneDX identity-evidence technique, and its SPDX D1 equivalent.
- **Merge**: spec 1646's combination of two components with equal PURLs, whose methods are absorbed when either side is binary analysis.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: FR-001's census is recorded in `measurements/`. It lists, per target, the components whose technique would change by kind, the merges whose methods would change, and the corrected Go build-info counts beside spec 1646's. The positive control shows each count firing.
- **SC-002**: Synthetic `waybill-fixture-*` fixtures: the `dynamic-linkage` and `embedded-source-path` components report `binary-analysis` in all three formats. The `cargo-auditable`, `elf-note-package` and Go BuildInfo components are byte-identical to `main`.
- **SC-003**: Each rule has a mutation record: dropping either kind from the binary list, adding an embedded-manifest kind to it, and removing either kind from the merge gate each fail at least one test.
- **SC-004**: `waybill sbom parity-check` agrees for every fixture, and on TheRock's SDK fixture the D1 row's change is the same in all three formats.
- **SC-005**: Every public-corpus, quality-corpus and TheRock difference against a `main` control is traced, component by component, to one kind. On TheRock the expected scale is about 64 components (49 `dynamic-linkage`, 15 `embedded-source-path`) plus any merges FR-001 counts. That figure is a prediction from spec 1646's census, confirmed by FR-001.
- **SC-006**: Scan time on TheRock's SDK fixture is within 1.05× `main`'s, median of at least 3 interleaved runs per arm.

## Assumptions

- The CycloneDX definitions are those of the 1.6 XSD, quoted above. The JSON schema the project validates against only enumerates the values.
- `dynamic-linkage` reads `DT_NEEDED` from the binary's own dynamic section. That is examining a compiled binary, without disassembly; the definition says "typically", not "only".
- `embedded-source-path` recovers source paths from a binary's bytes and attributes them to a package, which is binary analysis of the compiled artefact, not source-code analysis.
- Out of scope: the confidence of any kind; Go source-tree components; renaming `detected_go`; any change to which components are emitted.
- TheRock is scanned on CI only, never locally.
- Zero new Cargo dependencies.
