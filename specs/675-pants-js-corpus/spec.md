# Feature Specification: Pants JavaScript/npm corpus regression gate

**Feature Branch**: `675-pants-js-corpus`
**Created**: 2026-09-02
**Status**: Draft
**Input**: User description: "Option B from issue #760 discussion — corpus-only regression gate for Pants-JS monorepos. No new Pants-side code. Validate that the current npm reader behavior on Pants-JS monorepos stays stable as a regression lock."

## Clarifications

### Session 2026-09-02

- Q: Should the layer 2 golden fixture capture the full emitted SBOM or only the JavaScript surface? → A: JS-only goldens — capture only `pkg:npm/*` components + their dependency edges + relevant document-scope annotations; non-JS surface is present in the emitted SBOM but excluded from the golden diff.
- Q: If fixture selection finds no suitable public Pants-JS monorepo, is a synthetic fallback in scope? → A: Yes — synthetic fallback is acceptable if the survey fails. Feature ships either way; a synthetic Pants-JS fixture in the `waybill-test-fixtures` sibling repo is the fallback substrate.
- Q: How many JavaScript package managers should the MVP cover? → A: npm only. `package-lock.json` is MVP scope. Pnpm and yarn coverage deferred to follow-up issues.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - waybill maintainer refactoring the npm reader (Priority: P1)

A waybill contributor is modifying the npm reader (or the shared walker, or the enrichment pipeline). They want an automated signal that their change does not silently break scanning of Pants-managed JavaScript monorepos — a common real-world deployment shape that the existing corpus does not cover.

**Why this priority**: Real-world Pants monorepo scans show thousands of `pkg:npm/*` components emit correctly today via the standard npm readers. This works, but nothing in the automated regression suite proves it will keep working. A single accidental change to the npm reader, the shared walker, or the Pants dispatch order could silently zero out those npm components on a real customer scan — with no in-repo signal until a human notices. This is a P1 gap.

**Independent Test**: Land the corpus target. Verify that the nightly CI job runs it and passes. Manually revert a small change to the npm reader (e.g., break package-lock.json parsing) on a scratch branch and confirm the corpus target fails with a diagnostic pointing at the npm reader. The test is fully independent — it does not depend on any other Pants-JS feature work.

**Acceptance Scenarios**:

1. **Given** the corpus target is added and pinned at a specific upstream SHA, **When** nightly CI runs with `WAYBILL_RUN_PUBLIC_CORPUS=1`, **Then** the target scans the Pants-JS monorepo and passes its layer 1 assertions.
2. **Given** a maintainer breaks the npm reader on a feature branch, **When** they push the branch and nightly CI runs, **Then** the corpus target fails and its diagnostic identifies the npm reader as the suspected regression site.

---

### User Story 2 - SBOM consumer scanning a Pants-JS monorepo (Priority: P2)

An operator scans their organization's Pants-managed JavaScript monorepo with waybill. They receive a CycloneDX SBOM listing every JavaScript dependency declared by the monorepo's lockfiles, exactly as they would from a non-Pants npm project. The Pants-provenance-on-npm gap (no `waybill:pants-target` on npm components) is not silently regressed by future changes.

**Why this priority**: This story codifies what P1 protects — a real user outcome, not a maintainer concern. It's P2 rather than P1 because the actual delivery happens today (the standard readers already work); this feature only prevents that outcome from being lost.

**Independent Test**: Compare the SBOM produced by scanning the pinned corpus target against the committed golden. Byte-identity confirms operators get the same output the corpus locks in.

**Acceptance Scenarios**:

1. **Given** an operator scans a Pants-JS monorepo at the exact SHA the corpus pins, **When** the scan completes, **Then** the emitted CycloneDX SBOM contains at least one `pkg:npm/*` component per top-level lockfile.
2. **Given** the same scan, **When** the operator inspects component properties, **Then** they see the standard npm reader output (name, version, purl, hashes) — the Pants-provenance annotations are absent by design and this absence is documented in the corpus target's source comment.

---

### User Story 3 - Future feature developer building the Pants-JS enricher (Priority: P3)

A future contributor decides to implement the full Pants-JS enricher (option A from issue #760 — decorating `pkg:npm/*` components with `waybill:pants-target` annotations). They need a concrete before/after baseline that proves their new code produces additive (not disruptive) changes to the SBOM.

**Why this priority**: Enables the follow-up option A work but is not required for its own value. P3 because the benefit accrues only when someone picks up option A.

**Independent Test**: When option A ships, its regeneration of this corpus target's goldens must show only additive Pants-provenance annotations — no dropped components, no changed PURLs, no reordered edges. The diff is the test.

**Acceptance Scenarios**:

1. **Given** the pants_js enricher (option A) is implemented, **When** its author regenerates this corpus target's goldens, **Then** the diff shows only new `waybill:pants-target` annotations on existing `pkg:npm/*` components (no other changes).

---

### Edge Cases

- **Upstream removes Pants-JS support in a later release**: `scripts/corpus/refresh-pins.sh` surfaces the drift when a maintainer next attempts a pin bump; the fix is a deliberate SHA-pin freeze at the last supported commit or a fixture swap.
- **Chosen upstream repo pivots away from Pants**: same as above — pin freeze until a suitable alternative is identified.
- **The Pants-JS monorepo also has Python or Go surface**: both layer 1 assertions AND layer 2 golden capture are scoped to `pkg:npm/*` per the Session 2026-09-02 clarification, so mixed-ecosystem drift in the fixture does not affect either check.
- **The upstream repo is force-pushed and the pinned SHA disappears**: the kusari-sandbox fork mirror insulates the corpus from this failure mode (same protection PR #757 gave the other pants corpus targets).
- **Pants-JS lockfile format changes across upstream Pants versions**: the pinned SHA freezes the observed format; deliberate refresh reveals the format shift.
- **Zero npm components emit from the pinned scan** (the "sanity floor" edge case): the layer 1 assertion fails with a diagnostic; this catches selection errors during initial corpus authoring.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The regression corpus MUST include at least one Pants-managed JavaScript monorepo target using `package-lock.json` (npm) as its lockfile format. Pnpm-lock.yaml and yarn.lock coverage is explicitly out of scope for this feature per the Session 2026-09-02 clarification; each is a separate follow-up.
- **FR-002**: The target's underlying source SHOULD be a publicly-reachable upstream repository mirrored into `kusari-sandbox/*` (matching PR #757's pants-example-{python,django,golang} pattern). If a fixture-selection survey during planning finds no suitable public Pants-JS monorepo of appropriate scale, a synthetic Pants-JS fixture in the `waybill-test-fixtures` sibling repo is an acceptable substitute per the Session 2026-09-02 clarification; the feature ships either way.
- **FR-003**: When FR-002 resolves to a public-monorepo target, the target MUST be SHA-pinned and refresh flows through the existing `scripts/corpus/refresh-pins.sh` diff-and-review path. When FR-002 resolves to a synthetic fallback, the fixture MUST be pinned to a specific commit in the `waybill-test-fixtures` repo via the existing m090 sibling-repo cache mechanism.
- **FR-004**: The target's layer 1 assertion MUST verify that at least one `pkg:npm/*` component emits from the pinned scan (derived from `package-lock.json`).
- **FR-005**: The target's layer 1 assertion MUST verify the presence of at least one component known to be a JavaScript top-level dependency of the pinned monorepo (equivalent to the m673 US2 pattern of asserting `pkg:pypi/django@*` for the Django fixture). The specific dependency name is planning-phase output — determined once the fixture is chosen.
- **FR-006**: The corpus MUST document — in a machine-readable form (source comment) — that Pants-side provenance annotations (`waybill:pants-target`, `waybill:pants-resolve`) are EXPECTED absent on npm components under current waybill behavior. This absence is not a bug; option A from issue #760 is the tracked follow-up if this behavior needs to change.
- **FR-007**: The target MUST NOT run in the default `cargo test` lane. It MUST only execute when the existing `WAYBILL_RUN_PUBLIC_CORPUS=1` env gate is set (matching the existing m195 pattern).
- **FR-008**: Golden SBOMs (CycloneDX, SPDX 2.3, SPDX 3) MUST be generated and committed alongside the target definition, mirroring the m195 layer 2 pattern, BUT scoped to the JavaScript surface only: goldens include `pkg:npm/*` components + their dependency edges + document-scope annotations relevant to the JavaScript readers; non-JS components (`pkg:pypi/*`, `pkg:golang/*`, etc.) are present in the emitted SBOM at scan time but stripped before golden comparison. This isolates the regression signal from unrelated ecosystem drift in the pinned fixture.
- **FR-009**: Adding the target MUST NOT modify any production waybill code path. Changes are limited to test infrastructure (`waybill-cli/tests/corpus_harness_195/`, `waybill-cli/tests/public_corpus.rs`, `waybill-cli/tests/fixtures/public_corpus/*/`) plus the fork itself.

### Key Entities

- **Corpus target definition**: One `CorpusTarget` entry in `manifest.rs` naming the pinned source, SHA, ecosystem tag, and layer 1 assertion function. Analogous to the existing `pants-example-python` etc. entries.
- **Layer 1 assertion function**: A pure function taking the emitted SBOM trio and returning either `Ok(())` or a diagnostic naming the invariant that regressed. Analogous to `pants_example_python_layer1` etc.
- **Golden SBOMs**: Three JSON files (CDX, SPDX 2.3, SPDX 3) per target under `waybill-cli/tests/fixtures/public_corpus/<target-name>/`. Compared byte-for-byte against re-emitted SBOMs, with the existing structural masking already applied, and additionally filtered to include only `pkg:npm/*` components + their dependency edges + JS-relevant document-scope annotations before comparison (per the JS-only scope decided in Session 2026-09-02).
- **kusari-sandbox fork**: A public GitHub fork of the chosen upstream Pants-JS monorepo, refreshed by human decision, not upstream cadence.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Adding the corpus target requires zero new Cargo dependencies at the workspace level.
- **SC-002**: Adding the corpus target requires zero changes to production waybill code (source-tree diff limited to test infrastructure and fixtures).
- **SC-003**: The target's layer 1 assertion produces a diagnostic naming the suspected reader / classifier module within 5 seconds of failure detection.
- **SC-004**: Golden SBOM fixture size (all three formats combined, JS-only scoped) stays under 2 MB per target. Empirical measurement at implement-time landed at ~1.5 MB for the pinned `pantsbuild/example-javascript` fixture (302 npm components): CDX ~544 KB + SPDX 2.3 ~976 KB + SPDX 3 ~4 KB. The SPDX 2.3 emission is annotation-verbose (7 `annotations[]` blocks per package × 302 = ~793 KB of the SPDX 2.3 total) — this is the format's per-component overhead, not a filter defect. The 2 MB ceiling stays well below full-SBOM sizing (which for this fixture would be ~30 MB across all three formats) and retains full layer 2 regression coverage across every emitted field.
- **SC-005**: The nightly CI runtime attributable to this target stays under 60 seconds end-to-end (clone + scan + assertion + golden compare).
- **SC-006**: A synthetic regression seeded into the npm reader that reduces emitted `pkg:npm/*` count by ≥ 10% fails the layer 1 assertion with a diagnostic pointing at the npm reader.
- **SC-007**: The `public_only_audit` and `public_hostname_allowlist` manifest audits already in place pass for the new target without further relaxation beyond what PR #757 already applied.

## Assumptions

- A publicly-reachable Pants-JS monorepo of appropriate scale (small enough to clone quickly, large enough to exercise `package-lock.json` with a meaningful component count) MAY exist — for example, an upstream `pantsbuild/example-*` node.js variant if it exists, or a suitable community-maintained OSS Pants-JS project. A fixture-selection survey is the first step of planning. If the survey turns up no suitable public candidate, the Session 2026-09-02 clarification permits a synthetic fallback in `waybill-test-fixtures`.
- Empirical evidence from real-world Pants-JS monorepo scans shows the standard npm readers (m066, m147, m180) emit correct `pkg:npm/*` components without Pants-side awareness. This is the baseline the corpus locks in.
- The chosen fixture will emit `pkg:npm/*` components in the tens-to-hundreds range (enough to be a meaningful regression signal, not so many that JS-only-scoped goldens exceed SC-004's 500 KB target).
- The m195 corpus harness's existing structural masking (hash normalization, timestamp masking, workspace-path rewrite) is sufficient for JavaScript SBOMs; no format-specific masking extension is required.
- Pants-side provenance annotations on npm components (option A from issue #760) are EXPLICITLY out of scope for this feature. If option A ships later, the corpus target's goldens will be regenerated to include the new annotations at that time.
- The existing nightly CI lane runs corpus targets. No new CI lane is created by this feature.
