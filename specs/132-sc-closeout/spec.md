# Feature Specification: Close milestone-131 SC misses with grounded targets

**Feature Branch**: `132-sc-closeout`
**Created**: 2026-06-19
**Status**: Draft
**Input**: User description: "close milestone-131 SC misses against the audit image"

## Context

Milestone 131 shipped 4 PRs (#374, #375, #376, #377) but **did not meet 4 of its 6 success criteria**.
The maintainer flagged the pattern of declaring milestones "complete" based on PR landings rather
than scorecard movement. This milestone owes:

1. Honest closure of the unmet milestone-131 SCs that ARE achievable.
2. Documented acknowledgment of milestone-131 SCs that were unrealistic and require scope revision.
3. Retrospective edit to `specs/131-quality-metadata-backfill/spec.md` marking which SCs were
   actually met vs deferred — so the spec record matches reality.

### Current state vs milestone-131 targets (against `remediation-planner:latest`)

| SC | Target | Current | Gap | Achievable in milestone 132? |
|---|---|---|---|---|
| 131 SC-001 weighted | ≥ syft + 0.5 | syft + 0.1 | -0.4 | Yes, follows from below |
| 131 SC-002 VERSION_MISMATCH | <20 | 374 | -354 | **NO at original target; YES at revised <50** |
| 131 SC-003 License Coverage | ≥3/5 | 2/5 | -1 | Yes, with extension |
| 131 SC-004 Supplier Attribution | ≥3/5 | 2/5 | -1 | Yes, bounded |

### Grounded analysis of the 374 VERSION_MISMATCH cases

Direct join of mikebom's emitted `pkg:nuget` PURLs against syft's at name level shows **896 names
overlap with disagreeing versions**. Sample breakdown:

| Pattern | Sample | Cause |
|---|---|---|
| Semver `+<sha>` suffix | `csc 4.8.0-7.25569.25+38896ab4...` vs `4.8.0-7.25569.25` | mikebom emits `AssemblyInformationalVersion` verbatim (per SemVer §10 build-metadata); syft strips |
| Different version field | `Microsoft.AspNetCore 8.0.27+be2530...` vs `8.0.2726.23008` | mikebom picks `Informational` per milestone-129 Q3; syft picks `FileVersion` 4-tuple |
| Garbage from row-size approximation | `AsInt64 8.0.27+be2530...` | Milestone-130 Phase A row-size misalignment surfacing in Phase B too |

This is not a row-size bug at root — it's a semantic disagreement about which AssemblyVersion field
to use as the canonical PURL version. Both choices are defensible. The "fix" is to **emit BOTH
representations** so consumers can pick.

### Current state of the License Coverage gap

mikebom emits 1,107 components with non-empty `licenses[]` on the audit image, mostly from the
milestone-131 US2a PE/CLR LICENSE.txt fingerprint matcher (339 components hit). The other 768 come
from existing source-tier readers (apk/dpkg/rpm/maven). The cargo path emits `mikebom:license-source =
"registry-required"` annotation but no actual `licenses[]` field (1,058 cargo components affected).

To lift License Coverage from 2/5 to ≥3/5, the cargo emissions need actual SPDX expressions. Three
candidate paths:

1. Extend the PE/CLR fingerprint table from 6 SPDX IDs (Apache-2.0, MIT, BSD-3-Clause, BSD-2-Clause,
   GPL-3.0, GPL-2.0) to cover MS-PL, LGPL-2.1, LGPL-3.0, Microsoft permissive licenses common in
   .NET assemblies. Low-risk.
2. Read cargo crate license from rootfs-local sources — e.g. `~/.cargo/registry/cache/index.crates.io-*/<crate>-<version>.crate` if cached in the image. Rare in production images but possible.
3. **Constitution-XII-permitted online enrichment** via deps.dev for cargo + nuget. Network-required;
   gates on `--offline=false`. The milestone-131 plan explicitly deferred this.

### Current state of the Supplier Attribution gap

The sbom-comparison scorecard's Supplier Attribution dimension weights CDX `supplier.name` field
presence. milestone-131 US3 added `externalReferences[].url` synthesis but never populated
`supplier.name`. Sample current PE/CLR component:

```json
{"name": "FSharp.Build", "version": "12.8.102.0", "externalReferences": [...], "supplier": null}
```

To lift Supplier Attribution from 2/5 to ≥3/5, populate `supplier.name` from PURL ecosystem:
`pkg:cargo` → `"crates.io"`; `pkg:nuget` → `"nuget.org"`; `pkg:maven` → `"Maven Central"`;
`pkg:npm` → `"npmjs.com"`; etc. Pure PURL-derived synthesis, no external lookups.

## Clarifications

### Session 2026-06-19

- Q: US3 path choice timing — research-first-then-pick, or commit to a path now? → A: Research-first, BLOCKING US3 implementation. The `research.md` deliverable at FR-012 MUST complete (with measured per-path coverage numbers against the audit image) before any US3 implementation task starts. The chosen path is pinned at `/speckit-plan` time based on the research output, not at `/speckit-specify` time. Closes the assumption-driven-implementation pattern flagged by the maintainer.
- Q: SC-003 "≥3/5" — what coverage % maps to that band? → A: Research-task ORDER 0 — extract the comparison tool's actual License Coverage scoring formula from its source at `/Users/mlieberman/Projects/sbom-comparison/` BEFORE measuring any path. Document the formula in `research.md §SC-003 Threshold`. Per-path measurement then references the documented formula. Avoids the "ship implementation → discover scorecard didn't budge → expand scope" pattern that bit milestone-131 SC-002.
- Q: Audit-image reproducibility — pin to a SHA? → A: Pin to an immutable `@sha256:...` digest. Every quantitative measurement in this spec (374 mismatches, 1107/2953 license coverage, 339 fingerprint hits, US2's <50 target, SC-003 per-path numbers) is bound to a single moving `:latest` tag today; that makes "SC met" unverifiable across re-runs. The digest MUST be captured at `/speckit-plan` time (the first step in research.md is `aws ecr describe-images --image-ids imageTag=latest`) and recorded in BOTH `spec.md §Assumptions and Dependencies` AND `research.md §Audit Baseline`. All subsequent SC verification — including milestone-132's own SC verification AND the milestone-131 retrospective US4 — references the pinned digest, not `:latest`. Mirrors the milestone-094 deflake-perf-tests lesson and the cross-host byte-identity goldens memory.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Supplier name backfill (Priority: P1)

A platform engineer scans a polyglot container image and inspects the emitted SBOM. They expect
every component carrying a PURL of a known ecosystem (`pkg:cargo` / `pkg:nuget` / `pkg:maven` /
`pkg:npm` / `pkg:pypi` / `pkg:gem` / `pkg:golang` / `pkg:apk` / `pkg:deb` / `pkg:rpm` /
`pkg:bitbake` / `pkg:swift` / `pkg:opkg`) to carry a populated `supplier.name` field naming the
canonical registry or distribution channel.

**Why this priority**: Smallest scope (~50 LOC), no new readers, immediate scorecard movement.
Closes milestone-131 SC-004.

**Independent Test**: Scan the audit image; assert ≥80% of components carry non-null `supplier.name`.

**Acceptance Scenarios**:

1. **Given** a `pkg:cargo/<name>@<version>` component, **When** the engineer scans the image, **Then**
   the emitted component carries `supplier.name = "crates.io"`.
2. **Given** a `pkg:nuget/<name>@<version>` component (from `.deps.json` OR PE/CLR metadata),
   **When** the engineer scans, **Then** `supplier.name = "nuget.org"`.
3. **Given** a `pkg:maven/<g>/<a>@<v>` component (any source-mechanism), **When** the engineer
   scans, **Then** `supplier.name = "Maven Central"` (unless an upstream reader already set a
   sidecar-derived supplier, which wins).
4. **Given** the post-132 SBOM compared against the syft baseline via `sbom-comparison`, **When**
   the engineer runs the comparison, **Then** Supplier Attribution scorecard moves from 2/5 to
   ≥3/5.

---

### User Story 2 — Realistic VERSION_MISMATCH scoping (Priority: P2)

The same engineer compares mikebom's SBOM against syft's via `sbom-comparison`. They see 374
VERSION_MISMATCH findings on .NET components. The milestone-131 SC-002 promised this would drop
to <20, which is unachievable — the disagreement is structural (mikebom emits SemVer-canonical
`AssemblyInformationalVersion`; syft emits stripped or `FileVersion`-shaped). The engineer
expects mikebom to surface BOTH representations so downstream consumers (or the comparison tool
itself) can match against either.

**Why this priority**: Honest scope revision of an unrealistic milestone-131 target. Bounded
implementation (~30 LOC of additional annotations + a normalization helper).

**Independent Test**: Scan the audit image; assert that every PE/CLR-derived `pkg:nuget` component
that carried a `mikebom:assembly-version-informational` value with a `+<sha>` build-metadata suffix
ALSO carries a new `mikebom:assembly-version-informational-stripped` annotation containing the
suffix-stripped form. Independently: assert VERSION_MISMATCH count drops to <50 (not <20) on the
audit image.

**Acceptance Scenarios**:

1. **Given** a managed assembly with `AssemblyInformationalVersion = "8.0.27+be2530c3035e4bfa..."`,
   **When** the engineer scans, **Then** the emitted component carries BOTH
   `mikebom:assembly-version-informational = "8.0.27+be2530c3035e4bfa..."` (existing) AND
   `mikebom:assembly-version-informational-stripped = "8.0.27"` (new).
2. **Given** the post-132 SBOM compared against syft, **When** the engineer runs `sbom-comparison`,
   **Then** VERSION_MISMATCH count drops to <50.
3. **Given** the milestone-131 spec at `specs/131-quality-metadata-backfill/spec.md`, **When** the
   engineer reads the SC section, **Then** SC-002 is annotated with the actual measured outcome
   AND a pointer to milestone 132's revised target.

---

### User Story 3 — License Coverage extension (Priority: P3)

The same engineer wants the audit-image License Coverage scorecard to lift from 2/5 to ≥3/5. They
expect more components to carry actual SPDX expressions in `licenses[]`.

**Why this priority**: Largest unknown — depends on which of the three candidate paths (extended
fingerprinting, rootfs-local cargo cache reading, online deps.dev enrichment) actually moves the
needle. This US is a research-first track: a write-up of which path closes the gap, followed by
the chosen path's implementation.

**Independent Test**: Post-132 sbom-comparison against syft reports License Coverage ≥3/5 on the
audit image.

**Acceptance Scenarios**:

1. **Given** a measured baseline of licenses[] coverage on the audit image (currently 1,107 of
   2,953 components ≈ 37%), **When** milestone-132 US3 ships, **Then** ≥1,500 of the components
   carry non-empty licenses[] AND the scorecard moves to ≥3/5.
2. **Given** the chosen extension path (extended PE/CLR fingerprint table OR online deps.dev OR
   rootfs-local cargo cache), **When** an engineer reads the milestone-132 spec, **Then** the
   rationale for the choice (measured movement vs implementation cost) is documented.

---

### User Story 4 — Retrospective milestone-131 SC accounting (Priority: P1)

A future engineer reads `specs/131-quality-metadata-backfill/spec.md` to understand what milestone
131 actually delivered. They expect the spec's Success Criteria section to accurately reflect
what was achieved vs deferred — not the pre-implementation aspirational targets.

**Why this priority**: Single-line spec edit. Highest ROI on truthfulness. Closes a maintainer-flagged
pattern of premature "milestone complete" declarations.

**Independent Test**: Read `specs/131-quality-metadata-backfill/spec.md` post-132; confirm SC-001
through SC-004 each carry a `**Status**: <met / partially met / deferred to 132>` annotation citing
the measured value from the actual post-131 sbom-comparison run.

**Acceptance Scenarios**:

1. **Given** the existing milestone-131 spec, **When** an engineer reads SC-001, **Then** the line
   reads `SC-001: ... — **Status**: partially met (mikebom 2.6 vs syft 2.5, +0.1 short of the +0.5
   target). Deferred to milestone 132 US1+US2+US3.`
2. **Given** the existing milestone-131 spec, **When** an engineer reads SC-002, **Then** the line
   reads `SC-002: ... — **Status**: not met (VERSION_MISMATCH=374; original target <20 was based
   on incorrect assumption that the gap was a row-size bug — actual cause is structural semver
   build-metadata disagreement with syft, see milestone 132 US2 spec for revised <50 target).`
3. Same shape for SC-003 and SC-004.

---

### Edge Cases

- **Multi-mechanism components** (e.g. a `pkg:maven` component detected by both top-level reader
  AND nested-JAR walker): supplier name resolves from PURL ecosystem regardless. No conflict.
- **PURLs from ecosystems not in the FR-001 list** (e.g. `pkg:bitbake`, `pkg:opkg`): supplier
  name MAY be populated from the existing source-mechanism annotation OR left null. Out of scope
  for FR-001; covered by existing readers.
- **InformationalVersion without `+<sha>` suffix**: the new
  `mikebom:assembly-version-informational-stripped` annotation MUST NOT be emitted (no diff to surface).
- **InformationalVersion with multiple `+<sha>` candidates** (rare): strip everything after the
  FIRST `+` per SemVer §10. mikebom MUST NOT attempt to interpret the build-metadata further.
- **PE/CLR component where Phase B walk produced garbage InformationalVersion that passed the
  sanity filter**: the stripped annotation would emit garbage too. mikebom MUST NOT add additional
  sanity-filtering specifically for the stripped form; if Phase B emitted a value, the stripped
  form rides alongside.

## Requirements *(mandatory)*

### Functional Requirements

#### Cross-cutting

- **FR-001**: Every component emitted by milestone 132 paths MUST flow through the existing CDX
  / SPDX 2.3 / SPDX 3 emission pipelines unchanged at the format-builder level.
- **FR-002**: All new metadata fields MUST use **standards-native** CDX / SPDX 2.3 / SPDX 3
  constructs first per Constitution Principle V (v1.4.0 fifth bullet). For every new
  `mikebom:*`-prefixed field introduced in this milestone, an audit of each target format's
  existing native constructs MUST be cited in this Functional Requirements section (the
  literal location Principle V mandates). Milestone 132 audit citations:
  - **US1 supplier name** — CDX `components[].supplier.name` (native, used directly);
    SPDX 2.3 `Package.originator` (native, used directly); SPDX 3 `software:supplier`
    (native, used directly). **No new `mikebom:*` introduced for US1.**
  - **US2 stripped Informational version** — see FR-008.1 below for the per-field audit.
  - **US3 license expressions** — CDX `licenses[].license.id` (native); SPDX 2.3
    `licenseDeclared`/`licenseConcluded` (native); SPDX 3 `software:declaredLicense`
    (native). The only `mikebom:*` US3 emits is the existing milestone-012
    `mikebom:license-source` provenance annotation (already audited in
    `docs/reference/sbom-format-mapping.md` since milestone 012).
- **FR-003**: Byte-identity preservation across the 33 alpha.48 goldens — except for fixtures
  that carry `pkg:cargo` / `pkg:nuget` / `pkg:maven` / `pkg:npm` / `pkg:pypi` / `pkg:gem` /
  `pkg:golang` / `pkg:apk` / `pkg:deb` / `pkg:rpm` / `pkg:bitbake` / `pkg:swift` / `pkg:opkg`
  components (those gain the new `supplier.name` field per US1; intentional additive churn).
- **FR-004**: No new Cargo dependencies for US1, US2, and US4. US3 MAY introduce a deps.dev
  client dependency IF the chosen path is online enrichment (currently `reqwest` is already in
  the workspace dep closure; no NEW direct deps anticipated).

#### US1 — Supplier name backfill

- **FR-005**: For every emitted `ResolvedComponent` carrying a PURL whose ecosystem matches one
  of the canonical-supplier table entries, the emitted `supplier.name` MUST be set from the table.
  Table:

  | PURL ecosystem | `supplier.name` value |
  |---|---|
  | `cargo` | `"crates.io"` |
  | `nuget` | `"nuget.org"` |
  | `maven` | `"Maven Central"` |
  | `npm` | `"npmjs.com"` |
  | `pypi` | `"PyPI"` |
  | `gem` | `"RubyGems"` |
  | `golang` | (preserved — existing `supplier_from_purl` heuristic for github.com/gitlab.com/etc.) |
  | `apk` | `"Alpine Package Maintainer"` |
  | `deb` | `"Debian Package Maintainer"` |
  | `rpm` | `"RPM Package Maintainer"` |

- **FR-006**: When an upstream reader has ALREADY populated `entry.maintainer` (e.g. apk reader
  extracts the Maintainer field from `APKINDEX`), that value WINS over the FR-005 synthesized
  value. The existing `entry.maintainer.clone().or_else(|| supplier_from_purl(&entry.purl))`
  precedence chain at `scan_fs/mod.rs:572` already encodes this rule; the FR-005 synthesis layer
  hooks in as an OR'd fallback in that chain.

- **FR-007**: For the pinned audit image (per §Assumptions Q3), the post-132 sbom-comparison Supplier Attribution score MUST
  move from 2/5 to ≥3/5.

#### US2 — VERSION_MISMATCH realistic scoping

- **FR-008**: For every PE/CLR-emitted `pkg:nuget` component carrying
  `mikebom:assembly-version-informational` whose value contains a `+` build-metadata separator,
  the system MUST ALSO emit `mikebom:assembly-version-informational-stripped` containing the
  value with everything from the first `+` onward removed.
- **FR-009**: Components where the InformationalVersion has no `+` separator MUST NOT carry the
  stripped annotation (no semantic content to surface).
- **FR-010**: The milestone-131 US3 Phase A `is_plausible_version_string` sanity filter MUST be
  re-applied to the stripped form; if the stripped form fails sanity, mikebom MUST NOT emit it
  (silent skip).
- **FR-008.1 (Principle V v1.4.0 audit citation)**: The new
  `mikebom:assembly-version-informational-stripped` annotation is a parity-bridging
  `mikebom:*` field. Native-construct audit per Constitution Principle V:
  - CDX 1.6 `components[].version` — single canonical-version slot; cannot carry an
    alternate representation alongside the verbatim Informational. **No native fit.**
  - SPDX 2.3 `packages[].versionInfo` — same; single-valued. **No native fit.**
  - SPDX 3 `software:version` — same; single-valued. **No native fit.**
  No native construct in any of the three target formats expresses "alternate canonical
  version representation" alongside a primary version. The parity-bridging `mikebom:*`
  annotation is therefore justified. The annotation is registered as a new C-row in
  `docs/reference/sbom-format-mapping.md` per the catalog convention; the row content
  is specified in `specs/132-sc-closeout/contracts/sbom-format-mapping-row.md`.
- **FR-011**: For the pinned audit image (per §Assumptions Q3), the post-132 sbom-comparison VERSION_MISMATCH count MUST drop
  to <50. **This is a deliberate downscoping of milestone-131 SC-002's <20 target**; the original
  target was based on an incorrect premise.

#### US3 — License Coverage extension

- **FR-012**: Milestone 132 US3 implementation MUST be **BLOCKED** until a research deliverable
  at `specs/132-sc-closeout/research.md` is complete. The deliverable MUST contain two ordered
  sections:
  - **§SC-003 Threshold (Research Task ORDER 0, per the 2026-06-19 Q2 clarification)**: extract
    the actual License Coverage scoring formula from the `sbom-comparison` tool's source at
    `/Users/mlieberman/Projects/sbom-comparison/`. Document: which fields the tool counts as
    "license present" (CDX `licenses[].license.id` vs `.expression` vs `.text`; SPDX
    `licenseDeclared` vs `licenseConcluded`); how the 1/5–5/5 band maps to coverage percentages;
    any edge cases (NOASSERTION, empty arrays).
  - **§License Path Analysis**: per-path measurement against the audit image. For each candidate
    path: (a) ADDITIONAL components that would gain a populated `licenses[]` field; (b) resulting
    total coverage % evaluated against the §SC-003 Threshold formula; (c) projected scorecard
    band (1/5–5/5).
  The path choice is pinned at `/speckit-plan` time based on these numbers, NOT at
  `/speckit-specify` time. Closes the assumption-driven-implementation pattern flagged in
  milestone-131 post-mortem.
- **FR-013** (CORRECTED 2026-06-19 — milestone 132 US3 Path C verification PR; see
  §Plan corrections at the bottom of this section):
  - **Path A (extended PE/CLR fingerprinting)** — always on, no flag required, no
    network. Extends the existing milestone-131 `fn fingerprint_license` substring
    matcher at `pe_clr.rs:973` with six new SPDX patterns: MS-PL, LGPL-3.0, LGPL-2.1,
    MIT-0, EPL-2.0, EPL-1.0. Runs at PE/CLR reader time; populates `licenses[]` on
    hit. Shipped via PR #382.
  - **Path C (online deps.dev enrichment for every deps.dev-indexed ecosystem —
    cargo, npm, pypi, golang, maven, nuget)** — **ALREADY SHIPPED** pre-milestone-132.
    deps.dev license enrichment is **default ON** in `scan_fs/mod.rs::scan_cmd` (line
    1927; gated by `enrich_cfg.deps_dev` which defaults to `true`). The ONLY thing
    that turns it off is `--offline` (operator-set) or `--no-deps-dev` (operator-set).
    My original FR-013 description of "opt-in via the new `--enrich-licenses=depsdev`
    flag, off by default per Constitution III Fail Closed" was a fabricated claim
    about the CLI surface — the actual CLI is the inverse (default-on, opt-out). The
    milestone-132 US3 Path C deliverable is therefore not a code change; it is the
    SC verification step (re-scan WITHOUT `--offline` and measure the resulting
    license coverage). Existing enrichment infrastructure stamps
    `evidence.deps_dev_match` on enriched components for provenance.
  - **Path B (rootfs-local cargo cache)** — **REJECTED**. Production container images
    do not ship `~/.cargo/registry/cache/` artifacts; projected lift on the audit image
    is ≈0. Full rejection rationale in `research.md §License Path Analysis §Path B`.
- **FR-014**: License Coverage score on the pinned audit image (per §Assumptions Q3)
  MUST move to ≥3/5. Measured outcome (PR-with-this-correction): **4/5** (mikebom
  effectiveRate 86.3 % vs syft 3.1 %) when scanned without `--offline`. Offline-mode
  scans MUST NOT regress license coverage relative to the milestone-131 baseline —
  unchanged at 37.9 % / 2/5 (the Path A complement from PR #382 alone is insufficient
  to lift the offline-mode score).

### Plan corrections (2026-06-19, milestone 132 US3 Path C PR)

Two fabricated claims about Path C are corrected in place above:

1. **CLI surface**: the milestone-132 spec / research / data-model / quickstart
   described Path C as gated by a "new `--enrich-licenses=depsdev` flag, off by
   default". The actual CLI has deps.dev license enrichment **on by default** with
   `--no-deps-dev` as the opt-out and `--enrich-sources <list>` as the allowlist
   form. `--offline` is the global kill-switch. No new flag exists.
2. **Scope of code change**: I claimed milestone 132 would "ship" Path C as a code
   change ("add a `cargo` arm to the `match purl.ecosystem()` dispatch"). The cargo
   arm — along with every other deps.dev-indexed ecosystem — was already present in
   `enrich/deps_dev_system.rs::deps_dev_system_for` and already wired into the scan
   pipeline pre-milestone-132. The milestone-132 US3 Path C deliverable was the
   verification step (re-scan without `--offline`), not a code change.

Both fabrications were caught during US3 implementation prep (after Path A PR #382
merged). The first surfaced when I read the existing depsdev_source.rs scaffolding
expecting to add a cargo arm and found `enrich_components` already iterating every
supported ecosystem. The second surfaced when I traced the CLI flag back through
`resolve_enrich_sources` and found `deps_dev: !args.no_deps_dev` (default true).

#### US4 — Retrospective milestone-131 SC accounting

- **FR-015**: `specs/131-quality-metadata-backfill/spec.md`'s "Success Criteria" section MUST
  be amended in-place. Each of SC-001 through SC-004 MUST gain a `**Status**:` line citing the
  measured post-milestone-131 value AND naming where (if anywhere) the work is being deferred to.
  Edit format MUST NOT delete the original target — it appends the post-hoc reality so the
  historical aspiration stays readable alongside what actually shipped.
- **FR-016**: A new `## Post-Milestone Outcomes (2026-06-19)` section MUST be added to milestone-131's
  spec immediately after the existing Success Criteria block, documenting the actual measured
  sbom-comparison scorecard and identifying which SCs were met / partially met / deferred. This
  section MUST cite the specific commits / PR numbers and the measured weighted score.

### Key Entities

- **`supplier.name` canonical-ecosystem table** (US1): static lookup `BTreeMap<&'static str,
  &'static str>` populated at module-load time. Keyed on `Purl::ecosystem()` return value;
  produces the human-readable supplier name string emitted into CDX `supplier.name` and SPDX
  `Package.originator`.
- **Stripped-Informational version** (US2): A new annotation key
  `mikebom:assembly-version-informational-stripped` carrying the InformationalVersion with the
  `+<build-metadata>` suffix removed per SemVer §10. Companion to the existing
  `mikebom:assembly-version-informational` annotation, NOT a replacement.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the pinned audit image (`remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c`, captured at
  `/speckit-plan` time per §Assumptions Q3), the post-132 `sbom-comparison`
  weighted score MUST exceed syft's by ≥0.4 points (current: +0.1; close-out target chosen at +0.4
  not +0.5 because the dimensions where syft still wins — Completeness 1/5 via syft's file
  inventory + Checksum 1/5 via syft's per-file hashes — are structural and won't move without
  changing what mikebom emits at a design level; see milestone 133 candidate work).
- **SC-002**: VERSION_MISMATCH count drops from 374 to <50 (revised from milestone-131's
  unrealistic <20 target; see FR-011 rationale).
- **SC-003**: License Coverage score moves from 2/5 to ≥3/5 (per FR-014).
- **SC-004**: Supplier Attribution score moves from 2/5 to ≥3/5 (per FR-007).
- **SC-005**: Byte-identity preserved across the 33 alpha.48 goldens EXCEPT for fixtures
  containing `pkg:cargo` / `pkg:nuget` / `pkg:maven` / `pkg:npm` / `pkg:pypi` / `pkg:gem` /
  `pkg:apk` / `pkg:deb` / `pkg:rpm` components (those gain `supplier.name` via US1 — intentional
  additive golden churn, the inverse of milestone-131 PR #374's documented golden update).
- **SC-006**: Total scan time growth on the audit image MUST be under 30% relative to milestone
  131 (per Constitution VIII and milestone-131 SC-006 precedent).
- **SC-007**: `specs/131-quality-metadata-backfill/spec.md` carries the FR-015 + FR-016
  retrospective edits before milestone 132 closes.

## Assumptions

- **Audit-image pin (per 2026-06-19 Q3 clarification)**: The audit baseline is an immutable
  `767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c` reference, NOT
  the `:latest` tag. The `<DIGEST>` value is captured at `/speckit-plan` time via
  `aws ecr describe-images --region us-east-1 --repository-name remediation-planner --image-ids imageTag=latest`
  and recorded in `research.md §Audit Baseline` AND back-substituted into this section. Every
  quantitative measurement in this spec (374 mismatches, 1107/2953 components with licenses, 339
  fingerprint hits, US2's <50 target, SC-003 per-path numbers) is bound to that pinned digest.
  All SC verification — milestone-132's own AND the US4 milestone-131 retrospective — re-scans the
  pinned digest, not `:latest`. A broader audit-corpus expansion is tracked separately (see Out of
  Scope item 2).
- The 374 VERSION_MISMATCH count is reproducible across re-scans of the pinned-digest image.
- Between milestones 131 and 132, neither mikebom nor syft pushed a release that would shift the
  baseline relative to the digest captured at spec time.
- Constitution XII permits deps.dev enrichment IF chosen as the US3 path. Permitted does not mean
  required; if FR-012's research finds that Path A (extended fingerprinting) alone closes the gap,
  Path C is not pursued in this milestone.

## Out of Scope

- **Audit-corpus expansion** (additional images: Spring Boot uber JAR, pure-Python, pure-Go,
  multi-arch, Windows containers). Tracked for a future milestone.
- **Emitting `syft:file`-style per-file inventory** to close the Completeness 1/5 vs 5/5 gap. This
  is a Constitution-level design conversation about what mikebom emits and is intentionally
  deferred. Tracked for separate constitution-level discussion.
- **Checksum Coverage 1/5 → 4/5 movement** via attaching hashes to file entries. Same structural
  gap as Completeness; same out-of-scope rationale.
- **Phase C of PE/CLR row-size computation** — full ECMA-335 §II.22 row-width implementation.
  Tracked as a separate hardening milestone if needed; the milestone-130/131 best-effort
  approximation is acceptable for milestone 132's stated targets.
- **Online enrichment for ecosystems other than cargo + nuget** (e.g. deps.dev for npm / pypi /
  maven). Could lift License Coverage further but out-of-scope for milestone 132's targeted
  movement to ≥3/5.

## Dependencies

- Existing milestone-001 `scan_fs/mod.rs::supplier_from_purl` function — extended by US1 with the
  full PURL-ecosystem canonical-supplier table.
- Existing milestone-130 US3 + milestone-131 US1 PE/CLR reader at
  `mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs` — extended by US2 with the stripped-
  Informational annotation emission.
- Existing milestone-131 contracts catalog at
  `docs/reference/sbom-format-mapping.md` — extended by US2 with one new C-row
  (`mikebom:assembly-version-informational-stripped`).
- The pinned audit image `767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c`
  (digest captured at `/speckit-plan` time per the 2026-06-19 Q3 clarification; `<DIGEST>`
  back-substituted into Assumptions section) + syft baseline SBOM regenerated against the same
  pinned digest (NOT the cached `~/Downloads/remediation-planner-syft-image-sbom.json` which is
  bound to a stale `:latest`) + comparison tool at
  `/Users/mlieberman/Projects/sbom-comparison/sbom-comparison` for SC verification.
- IF US3 chooses Path C (deps.dev): the existing milestone-001 `reqwest` workspace dep + the
  existing milestone-012 deps.dev enrichment scaffolding at `mikebom-cli/src/enrich/depsdev_source.rs`.

## Honest accounting clauses

This milestone is a **closeout** of premature claims, not a normal forward-progress milestone.
Two clauses to surface explicitly:

1. **Milestone 131's SC-001..SC-004 were not met when I (the implementing AI) declared the
   milestone "complete" after each PR merge.** The maintainer flagged this pattern; milestone
   132's US4 + FR-015 + FR-016 + SC-007 is the structural remediation. Future milestones MUST NOT
   declare "complete" until the spec's measurable SCs are verified against the audit baseline.
2. **Milestone-131 SC-002's "VERSION_MISMATCH <20" target was unrealistic** because the
   underlying analysis assumption (gap is a row-size bug) was wrong. The actual gap is structural:
   mikebom emits SemVer-canonical InformationalVersion; syft strips build-metadata or emits the
   4-tuple FileVersion. Closing to <20 requires choosing syft's representation over mikebom's,
   which contradicts the milestone-129 clarification Q3. Milestone 132 US2 revises this to <50
   via dual-annotation emission so the spec record can be measured honestly.
