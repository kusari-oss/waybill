# Feature Specification: Populate Remaining Public-Corpus Goldens

**Feature Branch**: `196-populate-corpus-goldens`
**Created**: 2026-07-14
**Status**: Draft
**Input**: User description: "Populate the remaining 5 target goldens"

## Overview

Milestone 195 shipped the public-corpus regression harness end-to-end with
Layer 1 assertions + Layer 2 byte-identity goldens for **one** target
(`go-cobra`). The other five manifest entries (`rust-ripgrep`,
`npm-express`, `python-flask`, `maven-guice`, `image-postgres16`) have
Layer 1 assertion functions and manifest entries in place, but no Layer 2
goldens on disk — so `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test
public_corpus` currently exercises only the go-cobra target end-to-end.
The nightly public-corpus workflow will therefore continue to run only
1 of 6 targets meaningfully until the goldens land.

This milestone closes that gap. It runs each of the 5 remaining targets
through the harness with `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1`,
commits the resulting goldens, and validates two things: (a) Layer 1
assertions written from spec knowledge in m195 actually match the real
mikebom output — where they don't, the assertions are corrected to
match observed reality per the m195 R8 seeding rule; (b) two consecutive
runs of each target produce byte-identical Layer 2 goldens after masking
(SC-006 from m195). The `image-postgres16` target additionally requires
resolving its pinned digest — m195 shipped a placeholder digest that
must be replaced with the real Docker Hub digest before the target
can run.

## Clarifications

### Session 2026-07-14

- Q: What platform should be used to generate the goldens? → A: Linux `ubuntu-latest` via CI dispatch. Maintainer dispatches `public-corpus.yml` against the `196-populate-corpus-goldens` branch with `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` env injected into the workflow, downloads the emitted-goldens artifact, commits it. Guarantees byte-identity against the nightly runner platform. Rationale: macOS-authored goldens would drift on the first nightly Linux run (embedded path separators, glibc version strings in binary-tier evidence, arch-specific reader behavior).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Nightly public-corpus job exercises all 6 targets (Priority: P1)

The Kusari-hosted nightly `public-corpus.yml` workflow runs against
`main` every night and reports pass/fail per corpus target. After this
milestone lands, every target — not just `go-cobra` — produces a real
verdict against pinned upstream artifacts. A mikebom regression that
breaks the rust reader (say) surfaces the next morning against
`rust-ripgrep`, not silently on a customer's machine three weeks
later.

**Why this priority**: This IS the whole feature. m195 built the
scaffolding; without goldens for the other five targets, the scaffolding
is dormant. Every subsequent benefit (cross-ecosystem regression catch,
class-of-bug tripwires beyond go-cobra) depends on this being done.

**Independent Test**: `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test
public_corpus` runs all 6 targets end-to-end from a fresh developer
machine (or the nightly runner). Every target passes.

**Acceptance Scenarios**:

1. **Given** the m196 branch merged into `main`,
   **When** the nightly `public-corpus.yml` workflow fires,
   **Then** every target — `go-cobra`, `rust-ripgrep`, `npm-express`,
   `python-flask`, `maven-guice`, `image-postgres16` — reports PASS.

2. **Given** a maintainer runs the full corpus locally with
   `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus`,
   **When** the run completes,
   **Then** all 6 corpus `#[test]` functions report `ok`.

3. **Given** a maintainer applies an intentional revert to the m194
   US1 stdlib-edge fix and re-runs the corpus,
   **When** the corpus fires,
   **Then** the `go-cobra` target still fails as before, AND — as long
   as any of the other 4 source targets also happen to hit the same
   revert class — those targets also produce actionable diagnostics.
   (Cross-ecosystem tripwire coverage from m195 US2 activates here.)

---

### User Story 2 - Postgres:16 pinned by real Docker Hub digest (Priority: P1)

The `image-postgres16` target's manifest entry currently carries a
placeholder digest (`sha256:1234...cdef`). Docker Hub will reject the
pull, so the target cannot run. Before goldens can be generated, the
manifest MUST hold the real digest resolved via `docker manifest
inspect docker.io/library/postgres:16`.

**Why this priority**: `image-postgres16` is the only polyglot-image
target in the corpus per m195 FR-002 (US2). Without a real digest the
corpus does not exercise the container-image code path at all —
defeating a big chunk of m195's ecosystem coverage goal.

**Independent Test**: `docker pull docker.io/library/postgres@<manifest-digest>`
succeeds against the pinned value; the harness's `ensure_hydrated`
step produces a hydrated cache directory without a `docker manifest
not found` error.

**Acceptance Scenarios**:

1. **Given** the m195-shipped placeholder digest in
   `manifest.rs::TARGETS[5].pinned`,
   **When** m196 lands,
   **Then** the digest is a real
   `sha256:<64-hex>` resolved via `docker manifest inspect
   docker.io/library/postgres:16` at m196-authoring-time.

2. **Given** the real digest pinned,
   **When** `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test
   public_corpus corpus_image_postgres16` runs,
   **Then** the docker pull succeeds and the SBOM scan produces
   parseable CDX + SPDX 2.3 + SPDX 3 output.

---

### User Story 3 - Layer 1 assertions match empirical mikebom output (Priority: P1)

The 5 non-cobra Layer 1 assertion functions in m195 were written from
spec knowledge alone — no actual scan was performed at m195-authoring
time to validate what mikebom really emits. Per m195 research §R8
("seed invariants from actual observed output, not aspirational
behavior"), Layer 1 assertions that don't match reality MUST be
adjusted to match observed output rather than force the goldens to
match aspirational behavior. This milestone reconciles any drift by
running each target, observing the actual emitted shape, and updating
the Layer 1 assertion where it disagrees.

**Why this priority**: If Layer 1 assertions are wrong, the corpus
signal is wrong — the assertion fires on legitimate mikebom output.
That's the same class of bug the corpus is supposed to prevent, so
we can't ship the corpus with self-inflicted assertion errors.

**Independent Test**: For each of the 5 non-cobra targets, running
the target's `#[test]` with Layer 1 alone (no golden diff) exits
clean, and the exit-clean output is the same shape a maintainer
would see from a manual `mikebom sbom scan` against the same pinned
artifact.

**Acceptance Scenarios**:

1. **Given** each of the 5 non-cobra target's Layer 1 assertion
   function,
   **When** the target's scan is run against the pinned upstream
   artifact,
   **Then** every assertion in the function either (a) passes against
   the real output, OR (b) has been adjusted so that it passes and
   the adjustment is documented alongside the assertion.

2. **Given** a maintainer inspects the emitted SBOMs (in
   `~/.cache/mikebom/corpus/*/emitted/`) after a corpus run,
   **When** they check each of the 5 targets' emitted CDX,
   **Then** the observed graph-completeness value, PURL prefixes,
   and edge shapes match what the Layer 1 assertion function
   expects.

---

### User Story 4 - Byte-identity holds across two consecutive runs for every target (Priority: P2)

m195 verified byte-identity for `go-cobra` alone via the
`byte_identity_across_two_runs` test. For confidence that the
corpus regression signal is deterministic across every target
(SC-006), this milestone verifies each of the 5 new goldens passes
the same byte-identity check.

**Why this priority**: Byte-identity is the second-layer signal
that catches non-determinism in mikebom's emitters. Without it, a
future non-determinism regression (e.g., a HashMap iteration order
change that reorders `dependsOn[]`) could silently break every
downstream consumer without the corpus noticing.

**Independent Test**: For each of the 5 new goldens, running the
target's corpus test twice consecutively (with the goldens already
in place) produces `test ... ok` both times — no Layer 2 drift.

**Acceptance Scenarios**:

1. **Given** the 5 new goldens committed to `main`,
   **When** the corpus runs twice in succession (same mikebom
   checkout, same pinned manifest),
   **Then** every target passes both runs with no drift diagnostic.

2. **Given** a hypothetical mikebom change that introduces
   iteration-order non-determinism into `dependsOn[]`,
   **When** the corpus runs post-change,
   **Then** at least one target's Layer 2 fires a drift diagnostic
   naming the affected format.

---

### Edge Cases

- **Docker Hub rate-limits the digest resolution**: the maintainer's IP
  is temporarily blocked by Docker Hub's anonymous-pull throttle. Fix:
  authenticate the pull (`docker login`) — anonymous access is optional
  per Docker Hub policy but the resolved digest, once pinned, is
  publicly-accessible thereafter and satisfies m195 FR-004.
- **Upstream tag advances during pin resolution**: unlikely (m196
  pins to specific commit SHAs already resolved in m195) but if a
  refresh happens mid-milestone, the m195 pins are re-verified against
  their tags before goldens are generated.
- **An observed graph-completeness value is `partial` for a target
  that m195 assertions expected `complete`**: per US3 acceptance
  scenario 1(b), the Layer 1 assertion is updated to accept `partial`
  with the observed reason code, provided the observed reason is
  legitimate (m177 tier-fidelity signal, orphan-class documented in
  the m194/m192 stack, etc.) — NOT masked-over silently.
- **A target's scan takes > 5 minutes cold-cache** (violating the
  per-target budget-share of SC-005's 30-min total): investigation
  required — flag as a follow-up milestone rather than silently
  accept slower goldens.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST commit Layer 2 golden files for each of
  the 5 remaining corpus targets (`rust-ripgrep`, `npm-express`,
  `python-flask`, `maven-guice`, `image-postgres16`) across all three
  emitted formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1) — 15 golden files
  total.
- **FR-002**: The `image-postgres16` target's `PinnedRef::Digest.algo_hex`
  field MUST be updated from the m195 placeholder value to a real
  `sha256:<64-hex>` resolved via `docker manifest inspect
  docker.io/library/postgres:16`.
- **FR-003**: Every Layer 1 assertion function that disagrees with
  observed mikebom output MUST be updated to match the observed shape.
  The update rationale MUST be recorded either in the function's
  doc-comment or in a scratch note under
  `specs/196-populate-corpus-goldens/scratch/`.
- **FR-004**: Every committed golden MUST be byte-identical when
  regenerated a second time against the same mikebom checkout + same
  pinned manifest, **on the Linux `ubuntu-latest` platform matching
  the nightly runner** (per Q1 clarification). Cross-platform byte-
  identity is explicitly out of scope for this milestone (matches
  m195 `feedback_cross_host_goldens` scoping) — a maintainer running
  the corpus locally on macOS MAY see Layer 2 drift they cannot
  reproduce in CI; that is expected until the corpus grows cross-
  platform golden variants (future milestone).
- **FR-004a**: Goldens MUST be generated via a CI dispatch of
  `public-corpus.yml` against the `196-populate-corpus-goldens`
  branch with `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` injected. The
  emitted-goldens artifact is downloaded and committed. No local-
  laptop golden generation is permitted for this milestone (per Q1).
- **FR-005**: The MVP `go-cobra` golden files committed in m195 MUST
  remain byte-identical after m196 — this milestone is additive-only
  and MUST NOT trigger accidental regens on the already-shipped
  target.
- **FR-006**: `./scripts/pre-pr.sh` MUST continue to pass green after
  m196 lands — the corpus opt-in gate protection from m195 FR-006 /
  SC-004 continues to hold.

### Key Entities

- **Layer 2 Golden File**: Per-target, per-format pinned SBOM against
  which fresh scans are byte-identity-compared. Lives in-repo at
  `mikebom-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`.
  Committed as part of this milestone.
- **Corpus Target Manifest Entry**: The typed `CorpusTarget` in
  `mikebom-cli/tests/corpus_harness_195/manifest.rs`. This milestone
  updates the `image-postgres16` entry's `pinned` field but leaves
  the other 5 entries' `pinned` fields unchanged (they were pinned
  correctly in m195).
- **Layer 1 Assertion Function**: The per-target Rust function in
  `mikebom-cli/tests/corpus_harness_195/layer1_assertions.rs`. This
  milestone MAY adjust the 5 non-cobra functions to match observed
  mikebom output but does NOT restructure them.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A fresh `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test
  public_corpus` run (with git + docker available) reports 6 of 6
  corpus targets `ok`. Post-milestone, no target skips or errors.
- **SC-002**: 15 new golden files land in
  `mikebom-cli/tests/fixtures/public_corpus/` (3 formats × 5 non-cobra
  targets).
- **SC-003**: Two consecutive full-corpus runs (same checkout, same
  manifest, same platform) produce byte-identical goldens for every
  target — no drift firings.
- **SC-004**: The nightly `public-corpus.yml` workflow's first
  execution against `main` post-merge reports success across all 6
  targets.
- **SC-005**: The `go-cobra` goldens committed in m195 remain
  byte-identical after m196 lands (no accidental regen).
- **SC-006**: `./scripts/pre-pr.sh` continues to complete with a
  wall-clock delta ≤ 5 seconds vs the pre-m196 baseline (matches
  m195 SC-004 threshold).

## Assumptions

- **git + docker are available on the maintainer's machine**: FR-002
  requires `docker manifest inspect` for the postgres:16 digest
  resolution — this is the ONLY step that runs locally on the
  maintainer's machine. Per Q1 / FR-004a, the golden generation
  itself runs in GitHub Actions on `ubuntu-latest`, so the
  maintainer does NOT need to be able to run the full corpus
  locally.
- **Docker Hub anonymous-pull rate-limit is not currently exhausted**:
  if it is, the maintainer can `docker login` to lift the limit.
  Post-pinning, subsequent public consumers of the digest do not need
  authentication (matches m195 FR-004).
- **Layer 2 masking rules from m195 are sufficient**: the masking
  helpers in `mikebom-cli/tests/corpus_harness_195/layer2_golden.rs`
  cover every non-deterministic field in mikebom's emitters. If a new
  non-determinism surfaces in a target that wasn't hit by go-cobra
  (e.g., a SPDX 3 IRI shape unique to image scans), the mask is
  extended in the same PR alongside the goldens.
- **Layer 1 assertion updates are corrective, not weakening**: this
  milestone MAY adjust assertions to match observed output, but MUST
  NOT weaken assertions past the point where they still trip on a
  class-of-bug regression. Any assertion weakening must be flagged
  and reviewed.
- **Postgres:16 currently satisfies m195 R8 seeding rule**: the
  `image-postgres16` Layer 1 seed values were derived from m194
  session observations; they should hold at m196 authoring time
  unless something has drifted upstream in Docker Hub's postgres:16
  build.
- **The nightly workflow does NOT need m196-branch validation
  pre-merge**: m195's CI workflow was designed to be triggerable via
  `workflow_dispatch` on any branch — a maintainer MAY manually
  dispatch against `196-populate-corpus-goldens` before merge to
  validate CI-runner-environment reproducibility, but it's not a
  merge blocker.
