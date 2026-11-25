# Feature Specification: Closure documents contain every boot library a boot library depends on

**Feature Branch**: `1655-reachable-boot-libraries`
**Created**: 2026-11-25
**Status**: Draft
**Input**: #1696, narrowed. Under `--nix-closure`, emit as components the GHC boot libraries reachable from the boot libraries a document already contains, so every boot-library dependency edge has a target and the completeness reason `boot-library-edges-incomplete` clears. Refs #1696, #1688.

## Background (measured)

Spec 1653 draws dependency edges between the GHC boot libraries a closure document contains, from the committed per-release table `ghc_boot_dependencies.tsv`. It never adds a component. When a boot library depends on one the document lacks, that name goes into the library's C247 `absent` list, no edge is drawn, and C105 reports `boot-library-edges-incomplete`. Every closure document reads `partial` for this reason.

**What following the dependencies adds.** For each compiler a document names (C218), take the boot libraries present, follow their dependencies through the table to a fixed point, and count the libraries reached that are not present. Probe: `measurements/reach.py`; output: `measurements/reach-before.txt`. The table below counts (compiler, library) pairs; the plan's measurement corrects it to components (see **Corrected at plan time** below).

| target | runtime compiler | added | bootstrap compiler | added | total | C105 count today |
|---|---|---|---|---|---|---|
| moat | ghc-9.10.3 | 9 | ghc-binary-9.8.4 | 10 | 19 | 14 |
| openai | ghc-9.6.6 | 5 | ghc-binary-9.2.4 | 5 | 10 | 8 |
| shellcheck | ghc-9.10.3 | 8 | ghc-binary-9.8.4 | 10 | 18 | 21 |
| slack-web | ghc-9.10.3 | 8 | ghc-binary-9.8.4 | 12 | 20 | 17 |
| slacklinker | ghc-9.10.3 | 7 | ghc-binary-9.8.4 | 10 | 17 | 16 |

- Every library reached is in the table for its release, so the set closes: each added library's own dependencies are present or added.
- The most common additions are `rts` (every compiler), `ghc-boot-th`, `ghc-prim`, `ghc-internal` and `ghc-bignum`.
- Emitting each compiler's whole set instead would add 35 to 42 libraries per compiler.
- On shellcheck, `boot-library-edges-incomplete` is C105's only reason, so that document would become `complete`. The other four keep their other reasons: orphans on four of them, and `transitive-edges-unresolvable` on moat and slacklinker.

**Corrected at plan time** (`measurements/shared_purls.py`, `shared-purls-before.txt`; research R1). One version of a library is one component, whichever compilers ship it: the closure labels a shared PURL with the project's compiler in C218. Spec 1653's edge pass looks dependencies up by (C218 compiler, name), so a bootstrap compiler's library never finds a dependency held under the project compiler's label at the identical PURL, and lists it `absent`. Of the pairs above:
- **Already components, held under the other label** (an edge-lookup defect, not a missing component): moat 6, openai 1, shellcheck 7, slack-web 7, slacklinker 7, all on the bootstrap compiler (`array`, `directory`, `ghc-bignum`, `mtl`, `pretty`, `stm`, `time`).
- **Not components, as distinct PURLs** (both compilers' needs merged, so `rts@1.0.2` counts once): moat 11, openai 8, shellcheck 10, slack-web 12, slacklinker 9.

**No present boot library is an orphan.** None of the 36 to 40 boot libraries in each golden has a `waybill:orphan-reason`. An added library is reached only through edges from present ones, so it cannot be an orphan either (`measurements/identity-probe.md`).

**The two tables agree on versions** (`measurements/identity-probe.md`):
- milestone 966's `ghc_boot_libraries.tsv` (1,407 rows) and spec 1653's `ghc_boot_dependencies.tsv` (1,371 rows) share 1,371 (release, library) keys.
- 9 shared rows differ, all `rts` on 9.0.1 and 9.0.2 to 9.2.8. The 1653 table leaves those blank, because the `package-data.mk` layout those releases use records no `rts` version; milestone 966 has `1.0` or `1.0.2`.
- No row carries two different non-empty versions. The 36 rows only in milestone 966 are all `Win32`.
- openai's bootstrap compiler is 9.2.4, so its added `rts` is one of the 9 differing rows.

**Two added names are not on Hackage** (`measurements/identity-probe.md`):
- `rts` and `ghc-platform` return 404 from `hackage.haskell.org/package/<name>`; the other ten added names return 200.
- OSV returns 0 advisories for all of these, whether queried as `pkg:hackage/<name>` or `pkg:generic/<name>`.
- The controls do return advisories: `pkg:hackage/base@4.17.0.0` returns HSEC-2023-0007, and `pkg:hackage/process@1.6.17.0` returns HSEC-2024-0003.
- OSV also has a separate `GHC` ecosystem. Its three advisories name `ghc` with no purl, and no Hackage purl reaches them, not even `pkg:hackage/ghc`. Defects in the runtime system are filed against the compiler there, not against `rts`.

## Clarifications

### Session 2026-11-25

- Q: Which version does an added library carry when the two tables differ? → A: Milestone 966's. The two never carry two different non-empty versions. Where they differ, 1653's is blank (`rts` on 9.0 and 9.2), and an added component must not be versionless when a committed table knows the version.
- Q: What identity do `rts` and `ghc-platform` get? → A: `pkg:hackage/<name>@<version>`, the same as every other boot library. See FR-004 for the reasoning.
- Q: (plan, measured) Is a reachable library that is not labelled with its compiler missing? → A: No, when another compiler's label holds the identical PURL. One version is one component. FR-001 and FR-005 key on the PURL, and SC-002's counts were corrected from (compiler, library) pairs to distinct PURLs (research R1).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - every boot-library edge has a target (Priority: P1)

A consumer of a `--nix-closure` document walks the dependency graph from `base`, `bytestring` or another boot library and reaches `ghc-prim`, `ghc-bignum`, `ghc-internal` and `rts`. Today the walk stops at a C247 `absent` list naming libraries that aren't in the document.

**Why this priority**: this is the whole feature. It is the reason every closure document reads `partial`.

**Independent Test**: scan the stub fixture `tests/fixtures/nix_closure_boot_versions` with `--nix-closure`. No boot library's C247 lists `absent` names, every dependency the table records has an edge in all three formats, and C105 does not contain `boot-library-edges-incomplete`.

**Acceptance Scenarios**:

1. **Given** a closure document whose `base` (ghc-9.10.3) depends on `ghc-internal` and `ghc-prim`, neither present today, **When** it is scanned, **Then** both are components with C218 `ghc-9.10.3`, and `base` has an edge to each.
2. **Given** `ghc-internal` added this way, **When** its own dependencies (`ghc-bignum`, `ghc-prim`, `rts`) are not otherwise present, **Then** they are added too (`rts` is reached only this way and through `ghc-prim`, never directly from 9.10.3's `base`), and its C247 lists no `absent` names.
3. **Given** a library that no present boot library depends on, even transitively, **When** it is scanned, **Then** it is not added. The added set is the reachable set, not the compiler's whole set.

### User Story 2 - added libraries carry a truthful identity (Priority: P2)

A vulnerability scanner or a reader matches added libraries the same way as the boot libraries already present: by a Hackage purl at the version that compiler shipped.

**Why this priority**: a component with a wrong version or a misleading identity is worse than an absent one.

**Independent Test**: on openai's golden, the added `rts` for `ghc-binary-9.2.4` reads `pkg:hackage/rts@1.0.2` (milestone 966's version), and every added component's version equals milestone 966's row for its (release, library).

**Acceptance Scenarios**:

1. **Given** the 1653 table has no `rts` version for 9.2.4, **When** `rts` is added for that compiler, **Then** its version is milestone 966's `1.0.2`.
2. **Given** an added `rts` or `ghc-platform`, **When** it is emitted, **Then** its purl is `pkg:hackage/<name>@<version>`, it carries C218 like its peers, and no CPE is synthesized for it, as for every Hackage component (spec 1373).

### Edge Cases

- **A compiler not in the table.** When C219 records a compiler as not in the table, or C247 reports `release-not-in-table`, nothing is added for it, and its existing C247 and C105 reasons are unchanged.
- **A library listed but not in the table.** When a present library's C247 reports `library-not-in-table`, it has no dependencies to follow, nothing is added for it, and its reason is unchanged.
- **Same name under two compilers.** One version is one component: `rts@1.0.2`, reached from ghc-9.10.3 and from ghc-binary-9.8.4, is one component whose C218 names the project's compiler, as the closure already labels a shared version. Two different versions (`ghc-prim@0.12.0` and `ghc-prim@0.11.0`) are two components, each with its own C218.
- **A dependency held under another compiler's label.** A bootstrap compiler's `containers@0.6.8` depends on `array`; 9.8.4 ships `array@0.5.8.0`, which is already a component labelled ghc-9.10.3 (moat, measured). The edge goes to that component, and `array` is not `absent`.
- **A reached library already present under the same compiler**, whether declared by the project, synthesized by the closure, or added through another path, is not added again. Edges go to the existing component.
- **No closure.** Without `--nix-closure`, output is byte-identical.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Under `--nix-closure`, for each compiler named by C218 whose release is in the table, the document MUST contain every boot library reachable from that compiler's present boot libraries through the table's dependencies. Identity is the PURL `pkg:hackage/<name>@<that release's version>`: a reachable library whose PURL some component already holds, under any C218 label, is not added. It MUST NOT add any other boot library.
- **FR-002**: An added library MUST have the same shape as the closure-synthesized boot libraries already present: name, version, C218 naming the compiler, C247, and the same `sbom-tier` (`build`). It MUST get no property that its present peers lack.
- **FR-003**: An added library's version MUST come from milestone 966's table for its (release, library). Where the 1653 table carries a non-empty version, the two MUST agree, and a test MUST check that across all shared rows.
- **FR-004**: An added library's purl MUST be `pkg:hackage/<name>@<version>`, including `rts` and `ghc-platform`. Reasons, all measured:
  - **Consistency.** These names are what GHC's installed package database calls them, which is the namespace every boot library is already emitted under.
  - **No change in scanner matches.** OSV returns 0 advisories for both names as `pkg:hackage` and as `pkg:generic`.
  - **Feed coverage.** `pkg:generic` would move them to C209's `unaddressable`, under spec 1373's FR-003.
  - **The real gap is elsewhere.** The defects OSV publishes for GHC's runtime are keyed on the compiler (`GHC`/`ghc`), which neither identity reaches.
- **FR-005**: Spec 1653's pass MUST resolve a boot library's dependency to the component holding `pkg:hackage/<dependency>@<version the depending library's release ships>`, whatever that component's C218 label, and MUST draw the edges to the added libraries. It MUST draw them in all three formats, with the same lifecycle typing as every other boot-library edge.
- **FR-006**: After this change, no boot library's C247 MAY list `absent` names unless its compiler's release or the library is missing from the table. Where none is missing, C105 MUST NOT report `boot-library-edges-incomplete`.
- **FR-007**: No added library MAY be an orphan. Each target's C105 orphan count MUST be unchanged.
- **FR-008**: Parity-check MUST report no failing row (spec 1654's `verdict`) that does not already fail on the same golden today.
- **FR-009**: Corpus goldens MUST be regenerated only by `public-corpus.yml` dispatch. Every changed golden's diff MUST be traced to FR-001 through FR-006.
- **FR-010**: Zero new Cargo dependencies. No scan-time network access, store read or package-database read is added; both tables are already embedded.

### Key Entities

- **Present boot library**: a component with C218, there before this feature runs. It was declared by the project or synthesized by the closure.
- **Reachable boot library**: a (compiler, library) pair the table's dependencies reach from a present boot library of the same compiler, identified by the PURL of the version that compiler's release ships.
- **Added boot library**: a reachable boot library that was not present, and that this feature emits.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the stub fixture and on all five closure goldens, 0 boot libraries list C247 `absent` names, and `boot-library-edges-incomplete` is absent from C105.
- **SC-002**: Added components per target are 11, 8, 10, 12 and 9 (moat, openai, shellcheck, slack-web, slacklinker), the distinct PURLs of `measurements/shared-purls-before.txt`; edges to components already held under another label are 6, 1, 7, 7 and 7 distinct dependency names. Any difference is traced in `measurements/after.md`.
- **SC-003**: The shellcheck golden's C104 reads `complete`. The other four goldens' C105 lose only the `boot-library-edges-incomplete` clause.
- **SC-004**: The orphan count in C105 is unchanged on every closure golden.
- **SC-005**: Parity-check's failing rows on each corpus golden are a subset of today's.
- **SC-006**: Every added component's version equals milestone 966's row for its (release, library), and the test of FR-003 covers all shared table rows.

## Assumptions

- The 1653 table is the authority on dependencies and the 966 table on versions; neither is regenerated by this feature.
- Closure-synthesized boot libraries are already identified by their shape (C218, C247 and `sbom-tier` only, no project-declaration properties), so an added library needs no new marker to say why it is present.
- Added libraries are build inputs: they ship inside the compiler, as their present peers do. They take the same tier and lifecycle treatment as those peers, whatever it is; this feature does not change it.

## Out of Scope (file separately)

- Emitting each compiler's whole boot-library set (#1696's original direction).
- The CycloneDX `scope` of boot libraries. None carries one today, including the bootstrap compiler's.
- The orphan and `transitive-edges-unresolvable` reasons the other four targets keep.
- The compiler's identity in OSV's `GHC` ecosystem: `ghc` advisories that no Hackage purl reaches (#1707, `measurements/identity-probe.md`).
