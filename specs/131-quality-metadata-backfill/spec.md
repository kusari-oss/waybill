# Feature Specification: Quality metadata backfill for milestone-130 new components

**Feature Branch**: `131-quality-metadata-backfill`
**Created**: 2026-06-19
**Status**: Draft
**Input**: User description: "lets continue"

## Context

Milestone 130 added **1,903 new components** to the audit-image SBOM (1,049 → 2,952) across three new
reader paths: cargo-auditable binary (US1), maven nested-JAR (US2), and PE/CLR managed-assembly
metadata (US3 Phase A). The post-milestone-130 sbom-comparison scorecard against syft showed:

| Dimension | Pre-130 | Post-130 | Δ |
|---|---|---|---|
| Completeness (common-package overlap with syft) | 942 | **1,920** | **+978** |
| Unique-to-mikebom | 133 | **1,032** | **+899** |
| Version Accuracy | 5/5 | 4/5 | -1 (373 mismatches) |
| License Coverage | 3/5 | **1/5** | **-2** |
| Dependency Graph | 4/5 | 2/5 | -2 |
| Supplier Attribution | 4/5 | **2/5** | **-2** |
| **OVERALL weighted** | **3.3** | **2.4** | **-0.9** |

The weighted-score regression is structural: milestone 130 added **identity** without **enrichment**.
The cargo-auditable ELF section carries the crate graph but no licenses or suppliers; PE/CLR Assembly
table metadata carries name+version but the license info lives in attached `.txt` LICENSE files
that mikebom doesn't read; nested-JAR `pom.properties` carries coordinates but mikebom only extracts
license expressions from top-level JAR `pom.xml`, not nested ones. The high-quality dimensions that
alpha.48 scored well on (license, supplier, dep-graph, version-accuracy) got diluted by 1,900 new
low-metadata components.

This feature restores those quality scores by backfilling the missing metadata on each of the three
new readers, in priority order by the regression's magnitude.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Version fidelity for PE/CLR assemblies (Priority: P1)

A platform engineer scans a .NET-bearing container image and inspects the emitted SBOM. They expect
the `pkg:nuget/<name>@<version>` PURLs to match what NuGet.org publishes for the same package —
specifically the `AssemblyInformationalVersion` (a semver-style string like
`"8.0.27-servicing.26230.7+sha.a1b2c3d"`) rather than the CLR-binding `AssemblyVersion` 4-tuple
(`"8.0.0.0"`). This matches what OSV / NVD / GitHub Advisories index against.

**Why this priority**: Single highest-ROI regression fix. Resolves all **373 VERSION_MISMATCH**
cases from the post-130 sbom-comparison. Restores Version Accuracy from 4/5 to 5/5. Bounded scope:
extend the existing milestone-130 US3 reader's metadata-table walk with `CustomAttribute` table
(token 0x0C) extraction. ~300 LOC.

**Independent Test**: Run `mikebom sbom scan --image
mcr.microsoft.com/dotnet/runtime:8.0-alpine`. For every `pkg:nuget` component sourced from
`mikebom:source-mechanism = "dotnet-assembly-metadata"`, the emitted version MUST match the value
of the assembly's `AssemblyInformationalVersionAttribute` custom attribute (when present),
falling back to `AssemblyFileVersionAttribute`, falling back to the Assembly table's 4-tuple
`AssemblyVersion` per the milestone-129 clarification Q3 ladder.

**Acceptance Scenarios**:

1. **Given** an assembly carrying `AssemblyInformationalVersionAttribute("8.0.27-servicing.26230.7")`,
   **When** the engineer scans the image, **Then** the emitted SBOM contains
   `pkg:nuget/<name>@8.0.27-servicing.26230.7` AND the component carries
   `mikebom:assembly-version-informational = "8.0.27-servicing.26230.7"` annotation.
2. **Given** an assembly with `AssemblyFileVersionAttribute("8.0.27.26230")` but no
   `InformationalVersion`, **When** the engineer scans, **Then** the emitted PURL version is
   `8.0.27.26230` AND `mikebom:assembly-version-file = "8.0.27.26230"` is set.
3. **Given** an assembly with NEITHER `Informational` NOR `File` version attributes, **When** the
   engineer scans, **Then** the PURL version is the Assembly table's 4-tuple
   (existing Phase A behavior — no regression).
4. **Given** the post-131 SBOM compared against syft v1.42.3 baseline, **When** the engineer runs
   `sbom-comparison`, **Then** the VERSION_MISMATCH count drops from 373 to fewer than 20 (residual
   cases are real disagreements, not Phase-A-truncation artifacts).

---

### User Story 2 — License coverage backfill for new components (Priority: P2)

The same platform engineer expects the SBOM's `pkg:cargo`, `pkg:nuget`, and nested `pkg:maven`
components to carry license expressions (CDX `licenses[].license.id` or `licenses[].expression`)
matching the package's declared license. Pre-131, the 1,116 cargo + 819 nuget + nested maven
components emit with empty `licenses[]`, dropping the License Coverage score from 3/5 to 1/5.

**Why this priority**: Largest single point loss on the weighted scorecard (-2 points). Each of the
three new reader paths has a documented license source that mikebom can extract without external
network calls:

- **cargo-auditable**: each `packages[]` entry carries a `source` field; for crates-io packages,
  the license string is NOT in the `.dep-v0` section itself, but mikebom can OPT to mark the
  component with `mikebom:license-source = "registry-required"` so downstream tools (or a future
  deps.dev-enrichment milestone) know where to look.
- **PE/CLR**: managed assemblies' package directory in `/usr/share/dotnet/...` typically has a
  `LICENSE.txt` sibling file that mikebom can read.
- **Nested-JAR (`maven-jar-nested`)**: each nested JAR's `pom.xml` (which mikebom already extracts
  for the top-level reader) contains `<licenses>` — mikebom currently parses this for top-level
  JARs but skips it for nested ones.

**Independent Test**: After scanning the audit image with milestone-131 applied, the License
Coverage scorecard dimension MUST be ≥3/5 (vs post-130 1/5). Verifiable via `sbom-comparison
--format summary`.

**Acceptance Scenarios**:

1. **Given** a nested JAR carrying a `META-INF/maven/<g>/<a>/pom.xml` with a `<licenses>` element
   declaring `<name>Apache License 2.0</name>`, **When** the engineer scans, **Then** the emitted
   `pkg:maven/<g>/<a>@<v>` nested component carries `licenses[].license.id = "Apache-2.0"`.
2. **Given** a `.NET` assembly DLL whose package directory (`/usr/share/dotnet/packs/<name>/<ver>/`)
   contains a `LICENSE.txt` file, **When** the engineer scans, **Then** the emitted
   `pkg:nuget/<name>@<ver>` component carries `licenses[].license.text` populated from the file's
   contents (capped at 4 KB per FR-013).
3. **Given** a cargo-auditable entry whose `source` is `"crates-io"` but mikebom has no local
   license source, **When** the engineer scans, **Then** the emitted `pkg:cargo` component carries
   `mikebom:license-source = "registry-required"` annotation, signaling downstream tools that the
   license is available from `index.crates.io/...` (no automatic emission of a placeholder).

---

### User Story 3 — Supplier attribution backfill (Priority: P3)

The same platform engineer expects the SBOM's `pkg:cargo`, `pkg:nuget`, and nested `pkg:maven`
components to carry supplier metadata (CDX `externalReferences[].url`). Pre-131, supplier
attribution dropped from 4/5 to 2/5 because the new readers don't emit external references.

**Why this priority**: Second-largest scorecard regression (-2). Bounded implementation: each
ecosystem has a canonical supplier-URL pattern (`https://crates.io/crates/<name>`,
`https://www.nuget.org/packages/<name>`, `https://search.maven.org/artifact/<g>/<a>/<v>/jar`)
that mikebom can synthesize from the PURL alone — no external lookups required.

**Independent Test**: After scanning the audit image with milestone-131 applied, the Supplier
Attribution scorecard dimension MUST be ≥3/5 (vs post-130 2/5). Verifiable via `sbom-comparison`.

**Acceptance Scenarios**:

1. **Given** a `pkg:cargo/<name>@<version>` component from cargo-auditable, **When** the engineer
   scans, **Then** the emitted component carries:
   - `externalReferences[].type = "website"` with `url = "https://crates.io/crates/<name>/<version>"`
   - `externalReferences[].type = "vcs"` with `url = "<host>/<owner>/<repo>"` IF and only if
     the `source` field in `.dep-v0` carries a parseable `git+https://...` URL.
2. **Given** a `pkg:nuget/<name>@<version>` component from `.deps.json` OR `dotnet-assembly-metadata`,
   **When** the engineer scans, **Then** the emitted component carries
   `externalReferences[].url = "https://www.nuget.org/packages/<name>/<version>"` with
   `type = "website"`.
3. **Given** a nested `pkg:maven/<g>/<a>@<v>` component, **When** the engineer scans, **Then** the
   emitted component carries
   `externalReferences[].url = "https://search.maven.org/artifact/<g>/<a>/<v>/jar"`.

---

### Edge Cases

- **PE/CLR `InformationalVersion` with build metadata `+`**: A version string like
  `"8.0.27-servicing.26230.7+sha.a1b2c3d"` contains a `+` which must be URL-encoded in the PURL
  (`+` becomes `%2B`). The existing `mikebom_common::types::purl::Purl::new` constructor handles
  this per the milestone-005 PURL convention.
- **CustomAttribute row references a `MemberRef` that references a `TypeRef` outside the assembly**:
  the `TypeRef` table's `ResolutionScope` column points to an `AssemblyRef` row. mikebom MUST resolve
  through this to extract the canonical attribute type name. Failure to resolve → silent fall-through
  to the next-lower rung of the version ladder.
- **Nested JAR with `<licenses>` element pointing to a parent POM**: a child `pom.xml` may omit
  `<licenses>` and rely on the parent POM's declaration. milestone 131 scope is per-nested-JAR
  license extraction ONLY; parent-POM resolution for nested JARs is OUT of scope (deferred).
- **PE/CLR assembly's package directory has no `LICENSE.txt`**: emit the component with empty
  `licenses[]` AND a `mikebom:license-source = "package-dir-no-license"` annotation. Don't fall
  back to inferring license from name or guessing.
- **cargo-auditable `source = "git+https://...#<sha>"`**: parse to extract owner/repo for an
  optional `pkg:github/<owner>/<repo>@<sha>` shadow-PURL emission (similar to milestone-128's Yocto
  FR-002a). Mark with `mikebom:source-mechanism-secondary = "git-host-typed"`. Out of scope if the
  source field is malformed.
- **External-reference URL conflict (e.g. cargo crate with same name as npm package)**: the
  emitted `externalReferences[].url` is namespaced by the PURL's ecosystem prefix — `crates.io`,
  `nuget.org`, etc. — so name collisions across ecosystems cannot produce a wrong URL.

## Requirements *(mandatory)*

### Functional Requirements

#### Cross-cutting (all three stories)

- **FR-001**: Every component emitted with milestone-131 metadata MUST flow through the existing
  emission pipelines unchanged at the format-builder level (CDX `licenses[]` / `externalReferences[]`
  / SPDX 2.3 `licenseDeclared` / `externalRefs[]` / SPDX 3 `software_declaredLicense`).
- **FR-002**: All new metadata fields MUST use **standards-native** CDX / SPDX 2.3 / SPDX 3
  constructs first per Constitution Principle V. Mikebom-prefix annotations are permitted ONLY
  where the standards-native field cannot carry the semantic (e.g.
  `mikebom:license-source = "registry-required"` for the cargo crates-io-required case).
- **FR-003**: Byte-identity preservation across the 33 alpha.48 goldens.
- **FR-004**: No new Cargo dependencies.

#### US1 — PE/CLR CustomAttribute walking (Phase B)

- **FR-005**: System MUST walk the `CustomAttribute` table (token 0x0C) in the `#~` metadata stream
  to identify rows whose `Type` column resolves (through `MemberRef` → `TypeRef` → `#Strings` heap)
  to `"AssemblyInformationalVersionAttribute"` or `"AssemblyFileVersionAttribute"`.
- **FR-006**: For each matching row, system MUST decode the attribute's `Value` blob: the blob's
  prolog is `01 00`, followed by a UTF-8 length-prefixed string (per ECMA-335 §II.23.3).
- **FR-007**: The decoded string MUST be stored on the `ManagedAssembly` struct's
  `informational_version` or `file_version` field per attribute type.
- **FR-008**: The PURL version MUST follow the milestone-129 clarification Q3 fallback ladder:
  `AssemblyInformationalVersion → AssemblyFileVersion → AssemblyVersion 4-tuple`. When
  Informational is present, the PURL version is the Informational string verbatim (subject to
  PURL percent-encoding rules per FR-009).
- **FR-009**: A version string containing `+` or other PURL-reserved characters MUST be
  percent-encoded via the existing `mikebom_common::types::purl::Purl::new` constructor's
  validation pass. Malformed-after-encoding strings emit a single `warn` and fall through to the
  next-lower rung of the ladder.
- **FR-010**: The component MUST carry separate annotations for each EXTRACTED version field:
  `mikebom:assembly-version-informational`, `mikebom:assembly-version-file`,
  `mikebom:assembly-version-runtime`. Missing fields produce no annotation.
- **FR-011**: For the audit image, the post-131 `mikebom-vs-syft` VERSION_MISMATCH count MUST drop
  to <20 (vs post-130 373). Residual cases reflect real disagreements, not Phase-A truncation.

#### US2 — License coverage backfill

- **FR-012**: For nested-JAR `pkg:maven/<g>/<a>@<v>` components (milestone 130 US2 source-mechanism
  `"maven-jar-nested"`), the system MUST parse the nested JAR's `META-INF/maven/<g>/<a>/pom.xml`
  for `<licenses>` declarations and emit them via the same path as top-level JAR licenses.
- **FR-013**: For PE/CLR `pkg:nuget/<name>@<v>` components, the system MUST probe the package
  directory (typically `/usr/share/dotnet/packs/<name>/<v>/` or the parent of the `.dll`'s file
  path) for a `LICENSE`, `LICENSE.txt`, `LICENSE.md`, `COPYING`, or `COPYING.txt` file (case-
  insensitive). When found, the system MUST attempt to detect the SPDX license ID by
  fingerprint-matching the first 4 KB against the canonical opening text of common SPDX licenses:
  `"Apache License"` (→ `Apache-2.0`), `"MIT License"` / `"Permission is hereby granted, free of charge"` (→ `MIT`),
  `"BSD 3-Clause"` / `"Redistribution and use in source and binary forms"` (→ `BSD-3-Clause`),
  `"BSD 2-Clause"` (→ `BSD-2-Clause`), `"GNU General Public License"` + `"version 3"` (→ `GPL-3.0`),
  `"GNU General Public License"` + `"version 2"` (→ `GPL-2.0`). When a match fires, emit the
  resolved SPDX ID as a `SpdxExpression` via the existing `try_canonical` path and populate the
  component's `licenses[].license.id` (CDX) / `Package.licenseDeclared` (SPDX 2.3) /
  `software_declaredLicense` (SPDX 3). When the file is found but no fingerprint matches, emit
  `mikebom:license-source = "package-dir-unrecognized"` and `mikebom:license-text-sha256 = <hex>`
  so downstream tools can identify the license externally. mikebom MUST NOT embed the raw
  license text in the SBOM (the existing `SpdxExpression` type validates SPDX-canonical strings;
  free-text bodies would fail Constitution Principle IV's type-driven correctness invariant).
- **FR-014**: For cargo-auditable `pkg:cargo` components from a `crates-io` source, the system
  MUST emit a `mikebom:license-source = "registry-required"` annotation signaling that the license
  is available externally but not extracted by this milestone (Constitution Principle XII —
  external enrichment is permitted but not required; defer to a future deps.dev milestone).
- **FR-015**: For PE/CLR components where no license file is found, the system MUST emit a
  `mikebom:license-source = "package-dir-no-license"` annotation. Mikebom MUST NOT fabricate
  license expressions.
- **FR-016**: For the audit image, the post-131 License Coverage score from `sbom-comparison` MUST
  be ≥3/5 (vs post-130 1/5).

#### US3 — Supplier attribution backfill

- **FR-017**: For every `pkg:cargo/<name>@<version>` component, the system MUST emit
  `externalReferences[].type = "website"` with
  `url = "https://crates.io/crates/<name>/<version>"` (URL-encoded per the PURL spec for any
  reserved characters in `<name>` or `<version>`).
- **FR-018**: For every `pkg:nuget/<name>@<version>` component (regardless of source-mechanism),
  the system MUST emit
  `externalReferences[].url = "https://www.nuget.org/packages/<name>/<version>"` with
  `type = "website"`.
- **FR-019**: For every nested `pkg:maven/<g>/<a>@<v>` component, the system MUST emit
  `externalReferences[].url = "https://search.maven.org/artifact/<g>/<a>/<v>/jar"` with
  `type = "website"`.
- **FR-020**: When the cargo-auditable `source` field is parseable as `"git+https://<host>/<owner>/<repo>(\.git)?(#<rev>)?"`,
  the system MUST emit an additional `externalReferences[].type = "vcs"` with
  `url = "<host>/<owner>/<repo>"` (without trailing `.git`, without the `#<rev>` fragment per CDX
  best practice).
- **FR-021**: For the audit image, the post-131 Supplier Attribution score MUST be ≥3/5 (vs
  post-130 2/5).

#### Catalog / parity bookkeeping

- **FR-022**: Any new `mikebom:*` annotation key MUST be catalogued in
  `docs/reference/sbom-format-mapping.md` with full Principle V audit narrative per the milestone-128
  convention. New keys this milestone introduces:
  - `mikebom:license-source` (US2)
  - `mikebom:license-text-sha256` (US2, when license file is found but fingerprint doesn't match)
  - `mikebom:cargo-vcs-source-url` (US3, when cargo-auditable `source` field carries a parseable `git+https://...` URL — preserves the build-time-declared VCS reference)
- **FR-023**: Each catalogued key MUST be registered as a `ParityExtractor` slice entry with
  matching `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros, emitting `SymmetricEqual` across
  all three formats.

### Key Entities

- **`CustomAttribute` row** (US1): ECMA-335 §II.22.10 metadata-table entry. Columns:
  `Parent` (HasCustomAttribute coded index), `Type` (CustomAttributeType coded index),
  `Value` (`#Blob` heap reference). The `Value` blob's wire format is documented in §II.23.3:
  prolog `0x0001` + serialized argument list.
- **`MemberRef` row** (US1, transitive): ECMA-335 §II.22.25. Used to resolve the `Type` column
  of CustomAttribute through to a TypeRef + a method name like `".ctor"`.
- **`TypeRef` row** (US1, transitive): ECMA-335 §II.22.38. Used to resolve to the
  `#Strings`-heap-indexed type name (`"AssemblyInformationalVersionAttribute"`,
  `"AssemblyFileVersionAttribute"`).
- **License-source enum** (US2): `"package-dir"` (extracted from `LICENSE.txt`), `"pom-xml"`
  (extracted from nested `pom.xml`), `"registry-required"` (deferred to external enrichment),
  `"package-dir-no-license"` (probed but absent).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the audit image (`remediation-planner:latest`), the post-131 `sbom-comparison`
  weighted score MUST exceed syft by ≥0.5 points (vs post-130 syft-leads-by-0.1).
  **Status (2026-06-19)**: NOT MET. Measured post-milestone-131 score is syft + 0.1
  (no movement). Deferred to milestone 132 SC-001 at a revised +0.4 target. The two
  remaining gap dimensions are structural (Completeness 1/5 via syft's per-file inventory;
  Checksum 1/5 via syft's per-file hashes) — closing those requires a Constitution-level
  conversation about whether mikebom emits `syft:file`-style inventory, deferred per
  milestone-132 spec §Out of Scope.
- **SC-002**: VERSION_MISMATCH count drops from 373 to <20 (US1 acceptance gate).
  **Status (2026-06-19)**: NOT MET. Measured post-milestone-131 count is 374. The original
  <20 target was based on an incorrect premise: the residual mismatches were assumed to be
  row-size / parser bugs, but direct PURL-join analysis showed they are structural
  semver-build-metadata disagreements with syft (mikebom emits
  `AssemblyInformationalVersion` verbatim including `+<sha>` per SemVer §10; syft strips).
  Deferred to milestone 132 SC-002 at a revised <50 target, addressed via emitting a
  companion `mikebom:assembly-version-informational-stripped` annotation per
  milestone-132 US2.
- **SC-003**: License Coverage score moves from 1/5 to ≥3/5 (US2 acceptance gate).
  **Status (2026-06-19)**: NOT MET. Measured post-milestone-131 score is 2/5 (37.8 %
  EffectiveRate; 1107 / 2926 components). The milestone-131 PE/CLR LICENSE.txt
  fingerprint matcher (PR #375) added 339 nuget components with licenses, but cargo
  components (1116 total) remained at 0 coverage — the cargo path emits
  `mikebom:license-source = "registry-required"` but no actual `licenses[]` field.
  Deferred to milestone 132 SC-003, addressed via combined Path A (extended fingerprint
  patterns) + Path C (deps.dev online enrichment for cargo + nuget) per milestone-132 US3.
- **SC-004**: Supplier Attribution score moves from 2/5 to ≥3/5 (US3 acceptance gate).
  **Status (2026-06-19)**: NOT MET. Measured post-milestone-131 score is 2/5. PR #374
  populated `externalReferences[].url` for cargo / nuget / maven via PURL-derived
  synthesis but did NOT populate `supplier.name`, which is the actual scorecard input.
  Deferred to milestone 132 SC-004, addressed via the canonical PURL-ecosystem →
  registry-name `SUPPLIER_TABLE` per milestone-132 US1.
- **SC-005**: Byte-identity preserved across the 33 alpha.48 goldens (FR-003).
- **SC-006**: Total scan time growth on the audit image MUST be under 30% relative to milestone 130.
- **SC-007**: Each user story is independently shippable per the milestone-130 cadence —
  three sequential PRs (US1 → US2 → US3) OR a single bundled PR if confidence is high after US1
  lands.

## Post-Milestone Outcomes (2026-06-19)

Documented honestly after the milestone-131 PRs landed and the audit baseline was
re-measured. This section exists because the milestone was declared "complete" by the
implementing AI after each PR merged, treating PR-landing as SC-evidence. The maintainer
flagged the pattern; this section is the structural remediation per milestone-132 US4 +
FR-015 + FR-016 + SC-007.

### Measured scorecard

Audit image: `767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner` against
the cached SBOM at `/tmp/mb-rp-131-final.cdx.json` (scanned against `:latest` at
milestone-131 close-out time; the milestone-132 spec subsequently pinned the baseline to
`@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c` per the
2026-06-19 Q3 clarification, so milestone-132 SC verification uses the digest, but this
historical table reflects the same image content).

| SC | Target | Measured | Status | Disposition |
|---|---|---|---|---|
| SC-001 | syft + 0.5 | syft + 0.1 | NOT MET | Deferred to milestone 132 SC-001 (revised +0.4 — the +0.5 target wasn't reachable without structural Completeness/Checksum work) |
| SC-002 | VERSION_MISMATCH < 20 | 374 | NOT MET | Deferred to milestone 132 SC-002 (revised <50; root cause was semver-build-metadata disagreement, not parser bugs) |
| SC-003 | License Coverage ≥3/5 | 2/5 (37.8 %) | NOT MET | Deferred to milestone 132 SC-003 (Path A + Path C) |
| SC-004 | Supplier Attribution ≥3/5 | 2/5 | NOT MET | Deferred to milestone 132 SC-004 (SUPPLIER_TABLE lookup) |
| SC-005 | Byte-identity goldens | preserved | MET | — |
| SC-006 | Scan time growth <30 % | not re-measured | UNVERIFIED | Tracked via the milestone-094 perf harness as a separate concern; not gated for milestone-131 close |
| SC-007 | Independent-shippability cadence | 4 PRs shipped (#374, #375, #376, #377) | MET (in cadence) | — |

### What the milestone-131 implementation actually delivered

- **PR #374** (US3 phase): cargo+nuget+maven `externalReferences[].url` synthesis. Lifted
  PURL Quality but NOT Supplier Attribution — the scorecard's supplier dimension reads
  `supplier.name`, not `externalReferences[].url`. **Root error: scoring-target
  misidentification.**
- **PR #375** (US2 phase): PE/CLR LICENSE.txt fingerprint matcher with 6 SPDX IDs
  (Apache-2.0, MIT, BSD-3-Clause, BSD-2-Clause, GPL-3.0, GPL-2.0). 339 / 819 nuget
  components hit (41 %). Did not lift overall License Coverage because cargo (1116
  components, 0 covered) dominates the denominator. **Root error: scope
  misidentification — the gap was always cargo, not nuget.**
- **PR #376** (cargo-auditable plumbing): removed the `--skip-secondary-evidence` gate
  for cargo-auditable; surfaced 1058 cargo components correctly. Necessary infrastructure
  but no scorecard movement on its own.
- **PR #377** (US1 phase B): PE/CLR CustomAttribute walker for
  `AssemblyInformationalVersion`. Surfaced the structural disagreement with syft
  (semver build-metadata suffix). VERSION_MISMATCH stayed at 374 because the
  disagreement is semantic, not a parser bug. **Root error: incorrect premise about the
  cause of the 374 baseline.**

### Why "complete" was declared prematurely

The implementing AI declared each PR's user story complete after the PR merged,
treating PR-landing as SC-evidence. The actual SC measurements were either (a) not run,
or (b) run but interpreted incorrectly (e.g. counting `externalReferences[].url` as
"supplier attribution" instead of `supplier.name`).

The structural remediation is in milestone 132:

- Milestone 132's `quickstart.md §Step 3` defines an exact `jq`-based assertion script
  for each SC against the pinned-digest scorecard JSON — no more interpretive
  ambiguity.
- Milestone 132's tasks have a final polish step (T025) that runs the full quickstart
  before any PR cites SC closure.
- Milestone 132's `spec.md §Honest accounting clauses` carries forward this lesson
  explicitly: future milestones MUST verify SC measurements against the audit baseline
  before claiming closure.

## Assumptions

- The Phase B CustomAttribute walking is bounded by the same ECMA-335 row-width approximation
  caveat that milestone 130 US3 documented. The post-131 sanity-filter rejection rate may stay
  in the 5-10% range until a future milestone implements full §II.22 row-width computation.
- LICENSE / LICENSE.txt / COPYING file probing in the PE/CLR package directory is a best-effort
  match. Some Microsoft packages ship the license inside the `.nupkg` archive (deferred — not
  shipped in container images), inside the runtime store at a non-standard path (rare), or
  reference an SPDX expression in the `.nuspec` (not shipped in container images either). The
  net License Coverage gain may be smaller than 3/5 → 4/5; targeting 3/5 minimum.
- External-references URL synthesis is purely PURL-derived; no external lookups. For supplier
  metadata beyond the canonical registry URL (e.g. company name, contact email), defer to a future
  deps.dev-enrichment milestone — out of scope here.
- The audit image (`remediation-planner:latest`) is the standing acceptance baseline. A future
  audit-corpus expansion (additional images) is out of scope.

## Out of Scope

- Full ECMA-335 §II.22 row-width computation (the Phase B follow-up to Phase B's CustomAttribute
  walking). Tracked for a separate milestone after US1 lands.
- deps.dev / PurlDB online enrichment for the cargo licenses + Microsoft NuGet supplier metadata.
- Parent-POM resolution for nested JARs (some children rely on the parent for `<licenses>`).
- Reading license metadata from `.nuspec` files (not shipped in container images).
- Reading license metadata from PE `VS_VERSIONINFO` resource blocks (separate PE resource walker
  — large work item, deferred).
- Adding `externalReferences` to ecosystems milestone 131 doesn't touch (apk, dpkg, rpm, npm,
  gem, pip — those have their own conventions already handled by their existing readers).

## Dependencies

- Existing milestone-130 US3 PE/CLR reader at `mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs`.
- Existing milestone-130 US2 nested-JAR walker at `mikebom-cli/src/scan_fs/package_db/maven.rs`.
- Existing milestone-029 cargo-auditable reader at `mikebom-cli/src/scan_fs/binary/cargo_auditable.rs`.
- Existing milestone-009 top-level maven reader's `<licenses>` parser at the same maven.rs.
- Existing CDX / SPDX 2.3 / SPDX 3 `licenses[]` + `externalReferences[]` emission paths.
- Existing milestone-097 `mikebom:cpe-candidates` annotation channel (reused for new components).
- Existing milestone-128 parity catalog C-row system in `docs/reference/sbom-format-mapping.md`
  (1 new C-row for `mikebom:license-source`).
- The `mikebom_common::types::purl::Purl::new` constructor for PURL validation + percent-encoding
  on the version string with `+` / etc.
- The `object` crate (workspace dep; used for the US1 metadata-table walking).
- The `quick-xml` crate (workspace dep; used for nested-JAR `<licenses>` parsing — reused).
- The milestone-130 audit corpus (`/Users/mlieberman/Projects/sbom-comparison/sbom-comparison` tool
  + the syft baseline + the `remediation-planner:latest` ECR image) for end-to-end SC verification.
