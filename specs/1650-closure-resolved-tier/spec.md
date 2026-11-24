# Feature Specification: Closure-resolved Haskell components are not labelled `design`

**Feature Branch**: `1650-closure-resolved-tier`
**Created**: 2026-11-24
**Status**: Draft
**Input**: #1681. Under `--nix-closure`, components the closure resolved keep the manifest reader's `waybill:sbom-tier=design`. Refs #966, #1282.

## Background (measured)

The committed goldens for the five `nix-closure-*` corpus targets, as produced by `main` `18691d8` on CI Linux, all contain `--nix-closure` documents.

**The closures are pinned, and almost everything in them is resolved.** Each records the nixpkgs revision it evaluated (slacklinker: `5b2c2d84…`). `design` is 21–24 components per document, against 1,250–2,946 components in all:

| Target | Components | `design` | Reachable from the root | Reachable if `design` nodes are dropped |
|---|---|---|---|---|
| nix-closure-moat | 1,628 | 21 | 1,135 | 0 |
| nix-closure-openai | 1,374 | 22 | 1,362 | 0 |
| nix-closure-slack-web | 1,250 | 24 | 1,235 | 0 |
| nix-closure-slacklinker | 2,946 | 24 | 2,441 | 0 |
| nix-closure-shellcheck | 2,085 | 0 | 2,085 | 2,085 |

The last column models a consumer rule that drops `design` nodes and everything reachable only through them. It is a model, not any consumer's code, as in #1681.

**The few `design` components are the graph's hubs.** With `--root-name`, the root's direct dependencies are the repository's own packages: the evaluated package in each target, plus moat's `readme`. All are `design`, so everything hangs off a `design` node. shellcheck has no project manifest, and is unaffected.

**The `design` components fall into two groups, plus repository packages the closure does not evaluate (moat's `readme`, which correctly stays `design`):**

| Target | Project's own package | Closure GHC's boot libraries, `design` | Same compiler's boot libraries, already `build` | Bootstrap compiler's boot libraries, `build` | Other `design` |
|---|---|---|---|---|---|
| moat | 1 (+1, `readme`) | 19 (ghc-9.10.3) | 6 | 11 (ghc-binary-9.8.4) | — |
| openai | 1 | 21 (ghc-9.6.6) | 5 | 14 (ghc-binary-9.2.4) | — |
| slack-web | 1 | 23 (ghc-9.10.3) | 4 | 13 | — |
| slacklinker | 1 | 23 (ghc-9.10.3) | 5 | 11 | — |
| shellcheck | — | 0 | 27 (ghc-9.10.3) | 12 | — |

1. **The project's own package.** It carries `waybill:scope-classification-source=exempt-evaluated-package` (C203): the closure evaluated it, so it is a member of the build graph. No separate `build`-tier node exists for it. Its `waybill:closure-contents` (C214), which records only whether its outputs were examined, is `read` in three targets and `absent` in moat. Every closure member is `build` whatever its C214 value; moat has 1,054 members `absent` and `build` (research R3).
2. **Boot libraries of the closure's GHC,** versioned by milestone 966's boot table (`waybill:boot-library-compiler=ghc-9.10.3`).

**The tier of a boot library is inconsistent today.** A boot library of the same compiler, in the same document, is `build` or `design` with no rule a consumer could state. shellcheck's 27 are all `build`, while 19–23 per project target are `design`. Some of the `design` ones carry `package.yaml` evidence (`text`, `time`); others carry none (moat's `transformers` and `unix`). Which path assigns each tier is for the plan to establish.

**Out of scope: scans without `--nix-closure`.** There, `design` is correct by definition, since nothing has resolved the versions. hpack-ogmios has 103 of 103 Hackage components `design`. haskell-language-server and haskell-security-advisories get 315 and 215 `source` components from their lockfiles. Whether a main module should ever be `design` in a manifest-only scan is #1681's stated follow-up, not this spec.

**The tier definitions** (`docs/reference/sbom-types.md`, the CISA SBOM Types):
- **Design:** "intended dependency set, pre-implementation".
- **Source:** "present in the source tree".
- **Build:** "observed during the build process (build-tool output, build-graph trace)".

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The project's own package says the closure built it (Priority: P1)

A consumer reads a `--nix-closure` SBOM of a Haskell project. The package the closure evaluated and built is reported as built, not as a pre-implementation intention, so the graph below it is not discarded by a rule that drops design-only components.

**Why this priority**: in four of five closure targets, this one component decides whether a design-dropping consumer keeps the whole document or none of it.

**Independent Test**: scan a synthetic flake whose evaluated package is a Haskell project with `--nix-closure --root-name`. Its main module's tier is `build`, it records why, and the components below it remain reachable without passing through a `design` node.

**Acceptance Scenarios**:

1. **Given** a closure scan whose evaluated package carries closure evidence, **When** it is scanned, **Then** that package's tier is `build` in all three formats, and the reason is recorded.
2. **Given** the five corpus closure targets, **When** they are scanned, **Then** the components reachable from the root are the same whether or not `design` nodes are dropped, except components with no closure evidence (moat's `readme` itself).
3. **Given** a project package the closure did not evaluate (no closure evidence), **When** it is scanned, **Then** its tier is unchanged.

---

### User Story 2 - A boot library of the closure's compiler has one tier (Priority: P1)

A consumer asks what tier a GHC boot library is in a closure SBOM. The answer is `build` whenever the closure's compiler supplies it, whether or not the project's manifest also declares it, matching what shellcheck already shows for all 27 of its boot libraries.

**Why this priority**: 19–23 components per project target, and today the answer depends on which reader also saw the library.

**Independent Test**: a synthetic closure whose GHC supplies `base`, where the project's manifest declares `base` and another boot library it does not declare. Both are `build`, each with its reason.

**Acceptance Scenarios**:

1. **Given** a boot library versioned from the closure's GHC, declared by the project's manifest, **When** scanned, **Then** its tier is `build`.
2. **Given** a boot library of the closure's GHC that the manifest does not declare, **When** scanned, **Then** its tier is `build`, as today.
3. **Given** a boot library whose version did not come from the closure's compiler (no closure, or a compiler the closure does not hold), **When** scanned, **Then** its tier is unchanged.

---

### User Story 3 - A regression fails CI (Priority: P2)

A maintainer changes the Haskell or closure code. If a closure-resolved component goes back to `design`, or the root's direct dependencies all become `design` again, the corpus run fails and says which component.

**Why this priority**: the defect was found by a consumer, not by any check. The guard is what keeps the fix.

**Independent Test**: the new invariant fails on `main`'s goldens (a positive control) and passes on the branch's.

**Acceptance Scenarios**:

1. **Given** `main`'s five closure goldens, **When** the invariant is evaluated, **Then** it fails on moat, openai, slack-web and slacklinker, naming the components, and passes on shellcheck.
2. **Given** the branch's goldens, **When** evaluated, **Then** it passes on all five, with no declaration.
3. **Given** a manifest-only Haskell target, **When** evaluated, **Then** the invariant does not apply.

---

### User Story 4 - Nothing else changes (Priority: P2)

A scan without `--nix-closure`, or of a non-Haskell ecosystem, is byte-identical. In a closure scan, only the tier of the two groups and the record of why changes.

**Acceptance Scenarios**:

1. **Given** every manifest-only Haskell corpus target, **When** scanned, **Then** its goldens are byte-identical to `main`'s.
2. **Given** the corpus closure goldens, **When** regenerated, **Then** every difference is a tier change or its recorded reason on a component in the two groups, or a document-level value that follows from them (such as SPDX 3's aggregated SBOM types). Each is traced.

### Edge Cases

- **A project package whose outputs were not examined** (C214 `absent`, as in moat) is still a closure member, so FR-002 applies. C214 does not decide the tier for any closure member (research R3).
- **A repository package the closure does not evaluate** (moat's `readme`: a second `.cabal`, demoted from main module, with no C203) stays `design`. Nothing resolved it beyond its own manifest (FR-004).
- **Two compilers in one closure.** The bootstrap compiler's boot libraries are already `build` (11–14 per target), and stay so. A `design` component versioned from any compiler the closure holds is supplied by a closure member, so FR-003 needs no distinction between compilers (research R4).
- **A boot library that is both declared and versioned from a different compiler** (the manifest pins it outside the boot table) is not versioned from the closure, and stays as it is.
- **Genuinely unresolved Haskell components** (no version from any source) stay `design`. Since #1687 there are none on the corpus; slacklinker's `hunit` was the last.
- **`--tier`.** The filter runs twice, once before the closure passes and once after them (`scan_cmd.rs`, #1360), and the re-tier sits between the two. So under `--tier design-only` a closure scan no longer emits the re-tiered components, because they are no longer `design`. That is intended, and a fixture states it.
- **The document's aggregated type.** CycloneDX `metadata.lifecycles` and SPDX 3 `software_sbomType` aggregate per-component tiers. When `design` disappears from a document, it disappears from the aggregate too. That is a consequence, and is traced, not avoided.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (measure the assignment first)**: Before the change is implemented, the work MUST establish, for every `design` component in the five closure goldens, which code path assigned its tier. That includes why the same compiler's boot libraries are `build` in some cases and `design` in others. The result is recorded with counts per path.
- **FR-002 (the project's own package)**: Under `--nix-closure`, a project package that the closure evaluated (C203 = `exempt-evaluated-package`) MUST take tier `build`, whatever its C214 value. The Build definition is "observed during the build process (build-graph trace)", and the closure's derivation graph is that build graph. `source` was considered and rejected: the package is in the source tree, but the stronger claim, that the closure evaluated it into the build graph, is evidenced, and `build` is the tier that claim belongs to. The 143–242 Hackage closure members per target that are `source` take their version from the nixpkgs package set; this package does not (research R3).
- **FR-003 (boot libraries of the closure's compiler)**: A boot library whose version comes from the compiler the closure holds MUST take tier `build`, regardless of which reader first declared it. It ships inside that compiler, a `build`-tier closure member, and this matches the tier the same libraries already get when no manifest declares them. This supersedes milestone 966's clarification of 2026-11-12, "it stays `design`", as #1681 asks (research R2).
- **FR-004 (unchanged otherwise)**: A component without the evidence FR-002 or FR-003 requires keeps its tier. Specifically:
  - genuinely unresolved components stay `design`;
  - nothing changes in scans without `--nix-closure`;
  - the bootstrap compiler's boot libraries are untouched.
- **FR-005 (say why)**: Each re-tiered component MUST record why, naming the evidence: the closure built it, or the closure's compiler supplies it. The record must be the same in CycloneDX, SPDX 2.3 and SPDX 3. A standards-native field is used if one expresses it (Principle V). Otherwise an existing row that fits is used, and only failing both, a new catalog row with its parity extractor in the same change.
- **FR-006 (the regression guard)**: A new corpus invariant MUST assert, for every `--nix-closure` document:
  - (a) no component carrying the closure evidence of FR-002, or a version from the closure's compiler per FR-003, is `design`;
  - (b) when the root has direct dependencies, not all of them are `design`.

  It applies to every closure target with no declaration, is catalogued in `docs/reference/sbom-invariants.md` with its boundary against existing invariants, and does not apply to manifest-only documents.
- **FR-007 (byte identity elsewhere)**: Documents from scans without `--nix-closure` MUST be byte-identical to `main`'s.
- **FR-008 (no new dependencies)**: Zero new Cargo dependencies.

### Key Entities

- **Closure evidence on a package**: `waybill:scope-classification-source=exempt-evaluated-package` (C203), meaning the closure evaluated the package.
- **A boot library versioned from the closure's compiler**: one carrying `waybill:boot-library-compiler` (C218), which is written only for a compiler the closure holds.
- **The re-tier reason**: what FR-005 records, for each component that changes tier. The existing C203 and C218 carry it (research R5).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (fixtures)**: Synthetic `waybill-fixture-*` closure fixtures cover:
  - each acceptance scenario of User Stories 1 and 2;
  - the edge cases: a project package whose outputs were not examined (C214 `absent`), a repository package the closure does not evaluate, the bootstrap compiler's boot libraries, a manifest pin outside the boot table, and a genuinely unresolved component;
  - `--tier design-only` on a closure scan;
  - a manifest-only control.
- **SC-002 (the corpus result)**: On the five closure targets, the only `design` components left are those with no closure evidence: expected moat's `readme`, and none elsewhere (research R7). Components reachable from the root without `design` nodes are expected to be 1,134 (moat, `readme` alone dropped), 1,362, 1,235, 2,441 and 2,085, against 0 in the four project targets today. A deviation is traced.
- **SC-003 (the guard has teeth)**: The new invariant fails on `main`'s goldens for moat, openai, slack-web and slacklinker (positive control), and passes on all five closure goldens produced by the branch.
- **SC-004 (nothing else)**: Every manifest-only Haskell golden, and every non-Haskell golden, is byte-identical. Every closure golden difference is traced to FR-002, FR-003 or FR-005, or to a document-level aggregate that follows from them.
- **SC-005 (parity)**: `waybill sbom parity-check` reports no gap on any fixture or closure target.
- **SC-006 (mutations)**: Each rule is mutated, and each mutation is killed:
  - FR-002 and FR-003 each disabled;
  - FR-004's unresolved exemption removed;
  - FR-006's two clauses each disabled.

## Assumptions

- The corpus goldens are a fair measurement surface: CI Linux, the harness's flags, pinned revisions. Local scans are used where a fixture needs one; the moat golden is Linux-only.
- "The compiler the closure holds" is identifiable from the closure itself: the `pkg:generic/ghc@<version>` member, matched against `waybill:boot-library-compiler`. FR-001 confirms this on all five targets before it is relied on.
- `build` is the CISA Build type. Changing a component's tier does not change its PURL, version, scope or edges.
- #1685 (a `package.yaml` outside `cabal.project` is emitted) and #1686 (case-folded edges) are unrelated and stay separate.
