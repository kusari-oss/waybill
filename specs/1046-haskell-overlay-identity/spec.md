# Feature Specification: One identity for a Nix-built Haskell dependency the package set does not carry

**Feature Branch**: `1046-haskell-overlay-identity`
**Created**: 2026-11-13
**Status**: Draft
**Input**: User description: "#1046: a Nix-built Haskell dependency appears twice when an overlay pins a version the nixpkgs package set doesn't have — a versionless design-tier pkg:hackage/<name> from the manifest requirement and a build-tier pkg:generic/<name>@<version> from the closure. Merge them when the built version satisfies the declared range, decide the merged identity (keep pkg:generic, or pkg:hackage with a vcs_url qualifier when built from VCS, optionally only when the commit is tied to the release), and carry the derivation's VCS source as a native external reference."

## Background (measured 2026-11-13, main `2aab3baf`)

Under `--nix-closure`, a Haskell project's document describes one dependency twice when the version the build used is not the version the pinned nixpkgs package set carries. The corpus holds two instances:

| target | name | design tier (manifest) | build tier (closure) |
|---|---|---|---|
| `nix-closure-slacklinker` | `slack-web` | `pkg:hackage/slack-web`, no version. Requires `>= 2.3.0.0`; the package set has 2.2.0.0 (`nixpkgs-version-violates-requirement`). | `pkg:generic/slack-web@2.3.0.0`, `closure-identity=version-disagrees`, statically linked into the executable |
| `nix-closure-slack-web` | `hspec-discover` | `pkg:hackage/hspec-discover`, no version. Requires `>=2.6.0 && <2.11`; the package set has 2.11.14. | `pkg:hackage/hspec-discover@2.11.14`, `closure-identity=confirmed` |

**The slacklinker case.** An overlay (`nix/deps/slack-web.nix`, a cabal2nix expression) builds slack-web 2.3.0.0 from a git checkout (`fetchgit`, revision `1ea1e48f`), not from the Hackage tarball.
- **The version is a real release.** Hackage publishes 2.3.0.0 (uploaded 2026-10-09).
- **The checkout is that release's code.** The pinned revision is one commit before the upstream `v2.3.0.0` tag, and the two differ only in `CHANGELOG.md`.
- **No advisory can reach it today.** The build-tier component is `pkg:generic`, so no Haskell advisory can match it. The design-tier component has no version, so no advisory can match that either.

**The hspec-discover case.** The build used a version outside the project's declared range. nixpkgs relaxes Haskell version bounds when it builds, so a build outside the declared range is ordinary.

**The source is also lost.** The git checkout the build used is a closure member of its own (`slack-web-1ea1e48`, role `source`), and the package's dependency edge reaches it (#1473). That member carries no reference to its URL or revision:
- On the slacklinker golden, 111 of 902 `source` members carry a download reference, and this one does not.
- Measured at nixpkgs `a799d3e3`: a `fetchgit` derivation records `url` and `rev` under `structuredAttrs`, not in `env`, and the closure reader looks for a fetch URL only in `env`.
- How many of the 791 members without a reference are structured-attribute fetches has not been measured; that needs the target's derivation graph.

## Clarifications

### Session 2026-11-13

- Q: What identity does a component built from a VCS checkout at a version Hackage publishes carry? → A: `pkg:hackage/<name>@<version>` with a `vcs_url` qualifier naming the repository and revision (option B). It applies whenever Hackage publishes that version. Tying the commit to the release is not required.
- Q: A design-tier requirement and a build-tier component of the same Haskell package whose version is outside the declared range (nixpkgs relaxed the bound): merge or keep both? → A: Merge into one component, and record that the build's version does not satisfy the declared range.
- Q: How is the scan-time Hackage lookup (FR-006a) gated? → A: It is on by default. It is refused under `--offline`, and it has its own opt-out flag, following the one-opt-out-per-network-source convention. Declining leaves `pkg:generic`, with the reason recorded.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - One component per dependency (Priority: P1)

A consumer reading a Nix-built Haskell project's document sees each dependency once. When the manifest declares a dependency and the closure built it, the document carries one component, holding both the build's version and the declared requirement as evidence.

**Why this priority**: two components for one dependency make every count wrong. Component totals, orphan counts and "is X a dependency" answers all double-count. The versionless twin also looks like an unresolved dependency when the build resolved it.

**Independent Test**: scan the slacklinker corpus target. `slack-web` appears once, with version 2.3.0.0 and the declared range `>= 2.3.0.0`.

**Acceptance Scenarios**:

1. **Given** a design-tier component with no version and a declared range, and a build-tier component of the same Haskell package whose version satisfies that range, **When** the project is scanned, **Then** the document has one component for that package, with the build's version and the declared range.
2. **Given** a design-tier component and a build-tier component of the same name that is **not** a Haskell package (a C library that shares the name), **When** scanned, **Then** they are not merged.
3. **Given** the build-tier component already carries a Hackage identity (`closure-identity=confirmed`), **When** the build's version satisfies the declared range, **Then** the same merge applies.

---

### User Story 2 - A dependency built from VCS at a released version is reachable by advisories (Priority: P1)

A security engineer scanning the document gets Haskell advisories for a dependency the build took from a git checkout of a released version, the same as for the Hackage release. The document still says the source was a VCS checkout, not the release tarball.

**Why this priority**: this is the failure #1046 exists to fix. On slacklinker, slack-web 2.3.0.0 is the code Hackage ships as 2.3.0.0, and no advisory for it could ever match.

**Independent Test**: scan slacklinker. The merged slack-web component's identity is one a Hackage-aware matcher keys on, and it records the git URL and revision it was built from.

**Acceptance Scenarios**:

1. **Given** an overlay builds `<name>` at a version Hackage publishes, from a git checkout, **When** scanned, **Then** the merged component's identity is `pkg:hackage/<name>@<version>?vcs_url=git+<repository>@<revision>`.
2. **Given** an overlay builds `<name>` from the Hackage release tarball, at a version the package set does not carry, **When** scanned, **Then** the identity is `pkg:hackage/<name>@<version>` with no `vcs_url` qualifier.
3. **Given** an overlay builds `<name>` at a version Hackage does **not** publish (an unreleased fork), **When** scanned, **Then** the component keeps `pkg:generic`, and the document records why.
4. **Given** the scan is `--offline`, so whether Hackage publishes the version cannot be checked, **When** scanned, **Then** the component keeps `pkg:generic`, and the document records that the check was not made.

---

### User Story 3 - The VCS source is in the document (Priority: P2)

A reviewer can see which repository and revision a git-fetched dependency was built from, in each format's native slot, without opening the Nix expressions.

**Why this priority**: it is the provenance that makes option B honest, and on its own it closes the gap that every git-fetched source member currently loses its URL and revision. It is useful even if the identity decision lands elsewhere.

**Independent Test**: scan a closure that contains a `fetchgit` source. The source member, and the package built from it, each carry the repository URL and the revision as a native VCS reference.

**Acceptance Scenarios**:

1. **Given** a source member whose fetch records `url` and `rev` as structured attributes, **When** scanned, **Then** the member carries a VCS reference with both.
2. **Given** a source member that records its URL in the environment (the existing `fetchurl` shape), **When** scanned, **Then** its existing download reference is unchanged.
3. **Given** a package component whose dependency edge reaches a VCS source member, **When** scanned, **Then** the package component carries the same VCS reference.

---

### User Story 4 - A build outside the declared range (Priority: P3)

A consumer can tell that the build used a version outside the range the project declared, which nixpkgs routinely allows. That is the hspec-discover case: `>=2.6.0 && <2.11` declared, 2.11.14 built.

**Why this priority**: it is the second instance on the corpus, and it is a different case from #1046's. It needs its own decision so the merge rule in US1 is not silently extended to it.

**Independent Test**: scan the slack-web corpus target and inspect `hspec-discover`.

**Acceptance Scenarios**:

1. **Given** a design-tier component with a declared range, and a build-tier component of the same Haskell package whose version is outside that range, **When** scanned, **Then** the document has one component, with the build's version, the declared range, and a record that the version does not satisfy the range.

### Edge Cases

- **Several build-tier variants of the name at different versions.** Merge only when exactly one satisfies the declared range. Otherwise merge none, and record why.
- **A design-tier component with no declared range.** Any version satisfies it, so US1 applies.
- **A design-tier component that already has a version.** It is not a twin. That is today's reconciliation, and it is unchanged.
- **A VCS revision that is not a full commit hash** (a tag or branch name in the expression). It is recorded as given, and never used to tie the checkout to a release.
- **A git checkout fetched as a forge archive** (a tarball URL that embeds the revision) rather than with `fetchgit`. It keeps its download reference, and is not treated as a VCS checkout.
- **Offline scans, the opt-out, and an unreachable Hackage.** The publication check is not made: the component stays `pkg:generic`, the merge still happens, and the document records which of the three applied.
- **The same name in another ecosystem** (a Python or C namesake). The existing rule holds: a declared dependency never binds to a namesake that is not a Haskell package (#1466).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A design-tier Haskell component that has no version MUST be merged with a build-tier component of the same Haskell package, leaving one component in all three formats. This holds whether or not the build's version satisfies the declared range (FR-011).
- **FR-002**: The merged component MUST keep the build's version and identity, and the design tier's declared range and source-file evidence.
- **FR-003**: The merge MUST NOT apply when the build-tier component is not a Haskell package: when its identity is not Hackage and the closure's identity pass did not find the name in the Hackage package set.
- **FR-004**: When several build-tier components of the name exist, the merge MUST take the one that satisfies the declared range, if exactly one does. If none or several do, it MUST leave the components unmerged and record the reason. A single build-tier component is merged whether or not it satisfies the range.
- **FR-005**: Every dependency edge that named either twin MUST name the merged component afterwards, so no endpoint dangles.
- **FR-006**: A component built from a VCS checkout at a version Hackage publishes MUST carry `pkg:hackage/<name>@<version>` with a `vcs_url` qualifier naming the repository and revision. A component built from the Hackage tarball at a version the package set does not carry MUST carry `pkg:hackage/<name>@<version>` without the qualifier.
- **FR-006a**: Whether Hackage publishes a version MUST come from Hackage itself, not from the pinned package set, which records one version per package. The lookup is on by default. It MUST NOT run under `--offline` or when its dedicated opt-out flag is given. When the answer cannot be determined (`--offline`, the opt-out, or Hackage unreachable), the component MUST keep `pkg:generic`, and the document MUST record which of the three applied.
- **FR-007**: A component built from a VCS checkout at a version Hackage does not publish MUST NOT carry a Hackage release identity.
- **FR-008**: A source member whose fetch records a repository URL and a revision, in the environment or in structured attributes, MUST carry them as a native VCS reference in all three formats.
- **FR-009**: A package component whose dependency edge reaches such a source member MUST carry the same VCS reference.
- **FR-010**: Existing download references (`fetchurl`-shaped sources) MUST be unchanged.
- **FR-011**: When the merged build's version does not satisfy the declared range, the merged component MUST record that, together with the range and the version, so that a consumer can tell a relaxed bound from a satisfied one.
- **FR-012**: Scans without `--nix-closure` MUST be byte-identical before and after.

### Key Entities

- **Design-tier twin**: a Haskell component from a manifest requirement, with no version, a declared range, and the reason it is versionless.
- **Build-tier twin**: a closure member for the same package, with the version the build used and an identity outcome from the closure's identity pass.
- **VCS source**: a fixed-output fetch recording a repository URL and a revision, which a package's dependency edge reaches.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the slacklinker corpus target, `slack-web` is described by exactly one component (two today). It carries version 2.3.0.0 and the range `>= 2.3.0.0`.
- **SC-002**: Across the five closure corpus targets, the number of name-twins (a versionless design-tier Haskell component plus a build-tier component of the same name) falls from 2 to 0. The merged `hspec-discover` on the slack-web target records that 2.11.14 does not satisfy `>=2.6.0 && <2.11`.
- **SC-002a**: On the slacklinker target, the merged `slack-web` identity is `pkg:hackage/slack-web@2.3.0.0` with a `vcs_url` qualifier naming the revision `1ea1e48f…`. A Hackage-ecosystem advisory query keyed on name and version reaches it.
- **SC-003**: On the slacklinker target, the merged `slack-web` component and its source member each carry the repository and the revision `1ea1e48f…`. The count of source members carrying any reference rises above 111, and the PR records the new count.
- **SC-004**: Edges between named components are unchanged on every corpus target, apart from edges into merged twins, and no edge endpoint dangles in any format.
- **SC-005**: Every corpus target scanned without `--nix-closure` is byte-identical.

## Assumptions

- **Hackage package names are unique**, so a closure member whose name the identity pass found in the Hackage package set is that Hackage package.
- **nixpkgs routinely builds Haskell packages outside their declared bounds**, so a build outside the declared range is not evidence of a different package.
- **The structured-attribute shape of a git fetch was measured at nixpkgs `a799d3e3` only.** Other pinned revisions may use the environment form, and FR-008 accepts both.
- **slacklinker's pinned nixpkgs revision has not been checked** for the same shape. SC-003 is the measurement that settles it on that target.
- **No release-tie check.** The upstream comparison (the pinned revision against the `v2.3.0.0` tag) was made with the forge's API, for this spec only. The scan does not compare commits to tags, because option B was chosen over C.
- **A fork can be misidentified, and the document shows it.** A fork that reuses a released version number is matched as that release. The `vcs_url` qualifier and the VCS reference make that visible to a reader.
