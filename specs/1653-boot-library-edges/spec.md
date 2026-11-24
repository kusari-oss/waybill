# Feature Specification: GHC boot libraries carry their dependency edges, or the document says they don't

**Feature Branch**: `1653-boot-library-edges`
**Created**: 2026-11-24
**Status**: Draft
**Input**: #1688. Under `--nix-closure`, GHC boot libraries are emitted as leaves, and graph completeness does not say so. Refs #1650, #1681, #966.

## Background (measured)

Measured on the committed corpus goldens at `main` `91d84d1`. A boot library is a component carrying C218 `waybill:boot-library-compiler`.

| Target | Boot libraries | With ≥1 `dependsOn` | `base` components / their edges | Compilers (C218) | Completeness |
|---|---|---|---|---|---|
| nix-closure-moat | 36 | 1 (`ghc`) | 2 / 0, 0 | `ghc-9.10.3`, `ghc-binary-9.8.4` | `partial` (orphans; `transitive-edges-unresolvable: hackage`) |
| nix-closure-shellcheck | 39 | 1 (`ghc`) | 2 / 0, 0 | `ghc-9.10.3`, `ghc-binary-9.8.4` | **`complete`** |
| nix-closure-slack-web | 40 | 1 (`ghc`) | 2 / 0, 0 | `ghc-9.10.3`, `ghc-binary-9.8.4` | `partial` (orphans only) |
| nix-closure-openai | 40 | 1 (`ghc`) | 2 / 0, 0 | `ghc-9.6.6`, `ghc-binary-9.2.4` | `partial` (orphans only) |
| nix-closure-slacklinker | 39 | 1 (`ghc`) | 2 / 0, 0 | `ghc-9.10.3`, `ghc-binary-9.8.4` | `partial` (orphans; `transitive-edges-unresolvable: cargo`) |

**What the table shows:**
- **Every target carries two compilers' boot libraries:** the project's GHC, and the bootstrap `ghc-binary` that built it. So `base` appears twice, and has no edges either time.
- **No completeness reason names the boot libraries.**
  - shellcheck reads `complete`.
  - moat's `transitive-edges-unresolvable: hackage` comes from `design` components, not from the boot libraries. Spec 1650 (#1681) made those `build`, which removed the same clause from openai, slack-web and slacklinker.
  - The gap predates spec 1650: shellcheck already had it.

**Where the edges could come from, from the code and the goldens:**
- **The compiler's own package database.**
  - Every GHC records each boot library's `depends:` in the `package.conf.d` of its store output.
  - The closure tier reads realised store outputs only when they are already present, and never builds or fetches (milestone 1378). C214 on the compiler member says whether its output was present in the run that produced each golden:

    | Target | Project compiler | Bootstrap compiler |
    |---|---|---|
    | nix-closure-moat | absent | absent |
    | nix-closure-shellcheck | absent | absent |
    | nix-closure-slack-web | **read** | absent |
    | nix-closure-openai | **read** | absent |
    | nix-closure-slacklinker | **read** | absent |

  - **Presence is a property of the machine,** not of the project: about one output in ten is present on a developer machine, and none on a fresh runner (milestone 1378).
- **Milestone 966's committed table** (`ghc_boot_libraries.tsv`).
  - It has 1,407 rows across 36 GHC releases, 9.0.1 to 9.14.1, each giving a boot library's version.
  - It records **no dependencies**. Its sources (GHC release notes, Stack's global hints, release tags) list versions.
  - Using it would mean adding a `depends` column, from a source still to be chosen and measured.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A boot library's dependencies are in the graph (Priority: P1)

A consumer walks a closure document's dependency graph, for reachability or vulnerability triage. Walking from `base` reaches `ghc-prim`, `ghc-bignum` and whatever else that compiler's `base` depends on, as the compiler itself records it.

**Why this priority**: today the walk stops at `base`. Every boot library is a leaf, so anything reached only through one is invisible to a graph consumer, though it is in the document.

**Independent Test**: a synthetic closure whose compiler package database declares `base` → `ghc-prim`. The document has that edge, and says where it came from.

**Acceptance Scenarios**:

1. **Given** a closure whose compiler's package database is available, **When** scanned, **Then** each boot library of that compiler carries the edges the database records, to the boot-library components of the same compiler that the document already contains.
2. **Given** a recorded dependency on a library the document does not contain, **When** scanned, **Then** no component is synthesised for it. The edge is omitted, and that boot library counts as having unknown dependencies (User Story 2).
3. **Given** two compilers in one closure (project and bootstrap), **When** scanned, **Then** edges never cross compilers: `base` of `ghc-9.10.3` depends on `ghc-prim` of `ghc-9.10.3`.
4. **Given** any added edge, **When** the document is read, **Then** it says where that boot library's dependencies came from, identically in all three formats.

---

### User Story 2 - A boot library whose dependencies are unknown is not presented as complete (Priority: P1)

When the dependencies of one or more boot libraries cannot be established, for example because the compiler's output was not present and no other source covers that release, the document's graph-completeness verdict is not `complete`. The reason says how many boot libraries, and in which ecosystem.

**Why this priority**: shellcheck reads `complete` over 38 boot libraries whose dependencies were never established. That is a confident wrong answer about exactly the components a reachability consumer trusts most.

**Acceptance Scenarios**:

1. **Given** a closure where some boot library's dependencies are unknown, **When** scanned, **Then** `waybill:graph-completeness` is not `complete`, and its reason names the count of such boot libraries and their ecosystem.
2. **Given** CycloneDX output for that scan, **When** `compositions[]` is read, **Then** no record claims those components' ecosystem complete. The completeness pass and `compositions[]` agree, through an explicit case, not a wildcard.
3. **Given** a closure where every boot library's dependencies are established, including those that truly have none, **When** scanned, **Then** no such reason is emitted.
4. **Given** a boot library that truly depends on nothing in the source (`rts`, for example), **When** scanned, **Then** it is a known leaf, not an unknown one.

---

### User Story 3 - Nothing else changes (Priority: P2)

**Acceptance Scenarios**:

1. **Given** any scan without `--nix-closure`, **When** compared with `main`, **Then** it is byte-identical.
2. **Given** a closure scan, **When** compared with `main`, **Then** the only differences are the added boot-library edges, their provenance, and the completeness reason and `compositions[]` where it applies. Component sets, versions, tiers and every other edge are unchanged.
3. **Given** the five closure targets, **When** invariant I11 and the other corpus invariants are evaluated, **Then** they pass as before.

### Edge Cases

- **The bootstrap compiler's output is absent on every target.** Its boot libraries' dependencies are unknown unless another source covers that release (FR-001).
- **The two sources disagree for a release,** for example a table row against a package database. The plan defines precedence, and the disagreement is recorded rather than resolved silently.
- **A package database names a dependency by unit id** (`ghc-prim-0.12.0`). The edge goes to the component of that name and version from the same compiler. When the name matches and the version does not, the plan decides whether that is an edge or an unknown, and records it.
- **A boot library appears once per compiler,** and edges are keyed by the compiler (C218), so the two `base` components never share edges.
- **A dependency outside the boot-library set** (an external library the compiler links). It is not a component, so no edge is drawn (US1 scenario 2).
- **A compiler release outside the table and with no present output.** All its boot libraries are unknown, and the reason counts them.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (measure first)**: Before the design is fixed, the work MUST record, per closure target (moat, shellcheck, slack-web, openai, slacklinker) and per compiler (project, bootstrap):
  - how many boot-library edges each candidate source yields: the compiler's package database when its output is present, and a committed per-release table if one is built;
  - whether the sources agree where both apply;
  - which source the closure tier already has on disk in CI.

  Any number in the plan that describes a source's coverage is traceable to that record.
- **FR-002 (edges)**: Under `--nix-closure`, each boot library MUST carry dependency edges to the boot-library components of the **same compiler** that its established dependencies name, and that the document already contains. No component may be synthesised for an edge.
- **FR-003 (provenance)**: Each boot library with established dependencies MUST say where they came from, identically in CycloneDX, SPDX 2.3 and SPDX 3. Use a standards-native construct first (Principle V); otherwise an existing row, extended compatibly; only failing both, a new row with its parity extractor.
- **FR-004 (known leaves)**: A boot library whose established dependency list is empty is a known leaf, and does not count as unknown.
- **FR-005 (completeness)**: When at least one boot library in the document has unknown dependencies, `waybill:graph-completeness` MUST NOT be `complete`. Its reason MUST be a closed-vocabulary code naming the count and the ecosystem. CycloneDX `compositions[]` MUST agree, through an explicit case for the code.
- **FR-006 (no new network, no new eval)**: Establishing edges MUST NOT fetch, build or evaluate anything the closure tier does not already. Reading an output already present in the store is allowed (milestone 1378's rule), and so is committed data embedded in the binary.
- **FR-007 (unchanged otherwise)**: Scans without `--nix-closure` MUST be byte-identical. On closure scans, nothing other than FR-002, FR-003 and FR-005's outputs may change.
- **FR-008 (no new dependencies)**: Zero new Cargo dependencies.

### Key Entities

- **Boot library**: a component carrying C218, keyed by its compiler (`ghc-9.10.3`, `ghc-binary-9.8.4`).
- **Dependency source**: where a boot library's dependencies come from, either the compiler's package database or a committed per-release record.
- **Known leaf**: a boot library whose source records no dependencies.
- **Unknown boot library**: one no source covers.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (the counts)**: For each of the five closure targets, the number of boot libraries with at least one edge, and of edges, is recorded before and after, per compiler. FR-001's per-source counts are recorded beside them.
- **SC-002 (`base` is right)**: In every closure golden where `base`'s dependencies are established, its edges are exactly those the compiler's own package database records for it, restricted to components present in the document.
- **SC-003 (no false `complete`)**: No closure golden reads `complete` while a boot library it lists has unknown dependencies. Shellcheck's verdict is re-derived and explained.
- **SC-004 (positive control and mutations)**: A synthetic closure whose compiler package database declares `base` → `ghc-prim` emits that edge in all three formats with no parity gap. Removing the edge source fails a test. Removing the completeness reason, or its `compositions[]` case, fails a test.
- **SC-005 (nothing else)**: Every non-closure corpus golden is byte-identical. Every closure golden diff, regenerated only by the CI corpus dispatch with `regen_goldens=true`, is traced to FR-002, FR-003 or FR-005. I11 and the existing invariants pass.

## Assumptions

- **The five closure goldens are the measurement surface.** Their documents were produced on an x86_64-linux runner, and C214 on each compiler member says whether its output was present there.
- **A committed per-release table, if the plan chooses one,** follows milestone 966's discipline:
  - observed from named public sources;
  - dated;
  - regenerable by a committed probe;
  - embedded, with no scan-time network.

  Its source is measured in the plan, not assumed.
- **The `ghc` library's existing edge** (its one `dependsOn` today) comes from elsewhere, and is kept unless the plan finds it contradicts the source.
- **Edges among boot libraries are build-relevant.** Their scope follows the boot library's own scope as milestones 1276 and 1277 classify it. Scope is unchanged here.
