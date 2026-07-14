# Feature Specification: Public SBOM Regression Corpus

**Feature Branch**: `195-public-corpus-fixtures`
**Created**: 2026-07-14
**Status**: Draft
**Input**: User description: "pico fixture, but don't call it pico. Also don't pull anything internal to kusari. should only be public"

## Overview

Establish a **public-only** SBOM regression corpus inside `mikebom` itself so
that class-of-bug regressions like the milestone-194 pico orphan cascade
(#571 / #572) surface in mikebom's own CI before any downstream consumer runs
into them. Today the equivalent corpus lives in Kusari's private `pico`
repository, which means every mikebom release is validated against private
fixtures after the fact rather than protected by them up-front.

This feature adds a mikebom-owned corpus that scans a small set of
**publicly-available** upstream repositories and container images, generates
CDX + SPDX 2.3 + SPDX 3 SBOMs from each, and asserts a stable set of
invariants over the emitted output (graph-completeness value, orphan count
ranges, presence of specific canonical PURLs, dep-graph reachability from
the root). The corpus runs opt-in per invocation (heavy: it clones real
public repos and pulls real public images) but is fully reproducible from
any developer machine and from CI on demand.

## Clarifications

### Session 2026-07-14

- Q: What form do target invariants take? → A: Hybrid — small assertions-in-code catch class-of-bug regressions with clear diagnostics; full-SBOM golden catches unexpected drift. Assertions run first (fast fail); golden runs second (comprehensive).
- Q: When does the corpus run in CI? → A: Nightly on `main` (automatic scheduled run for regression catching within 24h) plus manual `workflow_dispatch` on any branch (so maintainers can validate a specific PR before merge when they touch a high-risk reader path). Never runs on default PR lanes.
- Q: Container-image target — MVP scope? → A: Include in MVP. MVP ships 5 source targets (Go, Rust, npm, Python, Java/Maven) PLUS at least one polyglot container-image target (e.g., `postgres:16` pinned by digest). CI runner must provide `docker` (or equivalent OCI-pull); cache footprint absorbs multi-GB per image; the 30-min SC-005 budget absorbs the ~5-10 min per-image scan slice.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Class-of-bug regression guardrails for the mikebom maintainer (Priority: P1)

A mikebom maintainer lands a change that unintentionally regresses graph-
completeness classification, drops emitted PURLs for a common ecosystem, or
breaks operator-override root handling. Before opening a PR they invoke the
public corpus locally (or the CI job runs it on demand); the invariant
assertions surface the regression against a real public codebase — not just
mikebom's own synthetic fixtures — with a diff pointing at which corpus
target broke and how.

**Why this priority**: This IS the whole feature. Every other benefit
(reproducibility, decoupling from Kusari-internal fixtures, cross-ecosystem
coverage) is downstream of the maintainer being able to run the corpus and
trust its verdicts. Without this loop closing, the pico regression class
recurs.

**Independent Test**: A maintainer applies an intentional regression to
mikebom source (e.g., revert m194 US1 stdlib-edge synthesis), runs the
corpus locally, and observes at least one target invariant failing with a
diff that names the change class ("golang mainmod no longer depends on
`pkg:golang/stdlib@v*`" or equivalent).

**Acceptance Scenarios**:

1. **Given** a clean checkout of mikebom at HEAD with all m194 fixes intact,
   **When** the maintainer runs the public corpus harness,
   **Then** every target reports its expected graph-completeness class
   (either `complete` or `partial` with a known reason code) and every
   pinned-invariant assertion passes.

2. **Given** an intentional revert of m194 US1 (Go stdlib edge synthesis)
   applied to the tree, **When** the corpus runs,
   **Then** at least one Go-source target fails with a diff pointing at the
   stdlib-orphan reappearance.

3. **Given** a corpus target's upstream release has published a new version
   since the pins were last refreshed, **When** the maintainer runs the
   corpus with the pinned SHA / image digest,
   **Then** the corpus scans the pinned artifact (not `HEAD` / `latest`) and
   invariants remain stable across the upstream churn.

---

### User Story 2 - Cross-ecosystem coverage across the ecosystems mikebom parses (Priority: P1)

The corpus targets EACH major ecosystem mikebom supports (Go, Rust, npm,
Python, Java/Maven, plus one polyglot container image) so that regressions
localized to one reader are caught by the target exercising that reader
regardless of whether other reader targets pass. A maintainer changing
`scan_fs/package_db/cargo.rs` sees the Rust target fail; a maintainer
touching `scan_fs/package_db/npm/mod.rs` sees the npm target fail; etc.

**Why this priority**: Without ecosystem breadth the corpus becomes a
targeted regression tripwire only for the ecosystems chosen — a maintainer
touching an unrepresented ecosystem gets no signal. mikebom already has 15+
readers; the corpus needs at least the five most-scanned ones represented.

**Independent Test**: The maintainer can enumerate corpus targets and
confirm at least one target per major ecosystem (Go / Rust / npm / Python /
Java-Maven / container-image). Removing any single target from the corpus
still leaves at least one target that would fail if the corresponding
reader broke.

**Acceptance Scenarios**:

1. **Given** the corpus manifest,
   **When** the maintainer lists included targets,
   **Then** each of {Go source, Rust source, npm source, Python source,
   Java/Maven source, polyglot container image} has at least one target.

2. **Given** an intentional break in the Rust reader (e.g., dropping
   emission of Cargo.toml `[workspace.package]` name),
   **When** the corpus runs,
   **Then** the Rust target's invariant fails; other ecosystem targets pass.

---

### User Story 3 - Public-only source of truth with no Kusari-internal pulls (Priority: P1)

The corpus MUST clone / pull ONLY from publicly-reachable sources — public
GitHub repositories, Docker Hub images, GHCR public images — with no
credentials, no VPN, no Kusari-internal registries, and no dependencies on
private artifacts. A first-time external contributor on a fresh laptop with
only `git`, `docker` (optional), and internet access can run the corpus
end-to-end without asking anyone for access.

**Why this priority**: This is why we're building this instead of just
pointing at Kusari's pico corpus. If the corpus needs any private access
it's no better than the status quo, and it can't be run by upstream
contributors, which is the whole point of moving it into mikebom's public
repo.

**Independent Test**: The maintainer runs the corpus with `HTTP_PROXY`
pointed at a proxy that logs every outbound request; the log shows only
`github.com`, `docker.io`, `registry-1.docker.io`, `ghcr.io`, and PyPI /
crates.io / npmjs.com (all public), and never `*.kusari.dev`,
`*.kusari.io`, or any private-registry hostname.

**Acceptance Scenarios**:

1. **Given** the corpus manifest,
   **When** a reviewer audits every target's source URL,
   **Then** every source resolves to a public GitHub org, Docker Hub image,
   GHCR public image, or a public package-registry URL.

2. **Given** a fresh developer machine with no Kusari credentials
   configured,
   **When** the maintainer runs the corpus,
   **Then** every target clones / pulls successfully and the corpus
   completes without prompting for credentials.

---

### User Story 4 - Reproducible pinning so corpus verdicts are stable over time (Priority: P2)

Every corpus target is pinned to a specific commit SHA (for source repos)
or a specific image digest (for container images). Upstream churn in the
target repos does NOT cause corpus flakes; pin refreshes are explicit,
reviewed commits into the mikebom repo. When an invariant changes as a
result of intentional mikebom behavior change, the maintainer updates the
pinned expectation in the same PR that changes the behavior.

**Why this priority**: Reproducibility is what makes the corpus a
regression tripwire instead of an intermittent-noise generator. Without
pinning, kubernetes/kubernetes's HEAD moving would break mikebom CI.

**Independent Test**: The maintainer runs the corpus, records the emitted
SBOMs, waits N days, re-runs the corpus, and byte-compares the SBOMs
(after masking known non-deterministic fields per the existing golden-
regression pattern). They MUST be identical.

**Acceptance Scenarios**:

1. **Given** the corpus manifest with pinned SHAs / digests,
   **When** the corpus runs today and again in 7 days,
   **Then** the emitted SBOMs are byte-identical (after masking known non-
   deterministic fields like scan timestamps and generator-tool version).

2. **Given** a corpus target repo whose upstream `main` has advanced,
   **When** the corpus runs against the pinned SHA,
   **Then** the pinned SHA is scanned (not `main`), and the corpus verdict
   reflects the pinned tree, not the current HEAD.

---

### User Story 5 - Opt-in execution so mikebom's default test suite stays fast (Priority: P2)

Running the public corpus involves cloning multi-hundred-MB source repos
and pulling container images that can be gigabytes. This MUST NOT run on
every `cargo test` invocation. The corpus runs when explicitly opted-in
via an environment variable, a dedicated cargo test filter, or a CI job
that is separate from the default lane.

**Why this priority**: The default developer inner loop must stay fast (a
minute or two). If the corpus becomes mandatory it becomes disliked, and
disliked infrastructure gets bypassed with `--no-verify`. Opt-in is the
delivery mechanism that lets us build this without burning goodwill.

**Independent Test**: The maintainer runs `./scripts/pre-pr.sh` (the
mandatory pre-PR gate) and observes it completes in the same time budget
it took pre-feature (no new multi-GB downloads). Separately, running the
opt-in corpus invocation completes with the corpus targets exercised.

**Acceptance Scenarios**:

1. **Given** a mikebom clean checkout,
   **When** the maintainer runs `./scripts/pre-pr.sh`,
   **Then** the pre-PR gate does NOT clone corpus repos or pull corpus
   images.

2. **Given** the corpus opt-in mechanism,
   **When** the maintainer invokes it explicitly,
   **Then** the corpus runs end-to-end and reports pass/fail per target.

---

### Edge Cases

- **Upstream repo deleted / renamed**: If a pinned corpus target's public
  repo is deleted, renamed, or made private after pinning, the corpus
  MUST fail loudly with an actionable error naming the missing target,
  rather than silently skip it.
- **Image digest unavailable**: Same as above for a corpus image pinned by
  digest whose registry no longer serves it (rare but happens for
  unmaintained images).
- **Network unavailable / offline development**: When the corpus is invoked
  without internet access, it MUST fail fast with a clear message rather
  than hang on TCP timeouts. If a local cache of previously-cloned repos
  exists, the corpus MAY reuse it (behavior similar to milestone 090's
  fixture cache pattern).
- **New ecosystem added to mikebom**: When mikebom gains a new ecosystem
  reader (e.g., a hypothetical Zig reader), the corpus does NOT
  automatically gain a target for it. Adding a corpus target for a new
  ecosystem is a follow-up task; the corpus does not block the reader's
  landing.
- **Corpus target starts emitting `partial` due to legitimate mikebom
  improvement**: When a mikebom improvement causes a corpus target's
  expected value to change from `partial` → `complete` (or from one reason
  code to a subset of reasons), the change is landed in the same PR as
  the mikebom improvement, with the pinned-invariant file updated.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST provide a corpus manifest identifying every
  corpus target by (a) source URL (git repo URL or container image
  reference), (b) pinned identifier (commit SHA for source repos; image
  digest for container images), (c) expected invariants (see FR-005).
- **FR-002**: The system MUST populate the corpus manifest with at least
  one target per major mikebom-supported ecosystem: Go source, Rust source,
  npm source, Python source, Java/Maven source, plus at least one polyglot
  container image target.
- **FR-003**: Every corpus target's source URL MUST resolve to a publicly-
  reachable location (public GitHub, Docker Hub, GHCR public, or a public
  package-registry URL). No Kusari-owned, Kusari-hosted, or otherwise
  private hostnames MAY appear anywhere in the manifest.
- **FR-004**: The system MUST NOT require authentication credentials
  (SSH keys tied to private repos, registry logins, private access tokens)
  to run the corpus. Anonymous public access MUST be sufficient.
- **FR-005**: The system MUST assert per-target invariants on the emitted
  SBOMs across all three formats (CycloneDX 1.6, SPDX 2.3, SPDX 3.0.1)
  using a **hybrid two-layer assertion model**:
  - **Layer 1 — Coarse assertions (fast-fail, diagnostic-first)**: per
    target, a small set of code-defined invariants asserted first —
    including at minimum (a) the `mikebom:graph-completeness` value,
    (b) the presence-or-absence of specific reason codes when the value
    is `partial`, (c) presence of expected canonical PURLs for the
    target's main-module or root component. Failures here surface with
    a class-of-bug-oriented diagnostic (per FR-009) naming the specific
    invariant that broke.
  - **Layer 2 — Full-SBOM golden (comprehensive drift catcher)**: per
    target, per format, one pinned SBOM golden compared byte-identically
    against the freshly-scanned output (after masking known non-
    deterministic fields), mirroring the existing `cdx_regression.rs` /
    `spdx_regression.rs` / `spdx3_regression.rs` pattern for synthetic
    fixtures. Failures here catch unexpected drift the coarse assertions
    didn't anticipate.
  - Layer 1 runs first; on failure the harness may skip Layer 2 for the
    same target (Layer 1 diagnostic is more actionable). On Layer 1
    pass, Layer 2 always runs so drift is surfaced explicitly.
- **FR-006**: Corpus execution MUST be gated behind an explicit opt-in
  mechanism (environment variable, dedicated test filter, or separate CI
  job) so that it does NOT run as part of the default `cargo test` /
  `./scripts/pre-pr.sh` flow.
- **FR-006a**: The CI cadence for the corpus MUST be **scheduled nightly
  against `main`** (automatic — surfaces regressions within 24 hours of
  merge) **plus manual `workflow_dispatch` against any branch** (so
  maintainers can validate a specific PR before merge when they touch a
  high-risk reader path). The corpus MUST NOT run as a required check on
  the default per-PR lane.
- **FR-007**: The corpus MUST scan the pinned artifact (SHA or digest),
  not upstream `HEAD` / `latest`, so that upstream churn does not flake
  the corpus.
- **FR-008**: When a corpus target's expected invariants intentionally
  change as a result of a mikebom behavior change, the invariant update
  MUST land in the same PR as the mikebom change. Diverging expectations
  MUST NOT be permitted on any long-lived branch.
- **FR-009**: On any corpus failure, the system MUST emit a diagnostic
  identifying (a) which target failed, (b) which invariant failed, (c)
  the observed value vs the expected value, and (d) a suggested next
  action (regenerate pins, investigate mikebom regression, or file
  upstream-corpus-target issue).
- **FR-010**: The corpus MUST provide a mechanism to regenerate the
  expected-invariant snapshots after an intentional mikebom behavior
  change (following the existing golden-regression precedent of
  `MIKEBOM_UPDATE_*_GOLDENS=1` env vars).
- **FR-011**: Corpus artifacts (cloned repos, pulled images) MUST be
  fetched into a well-defined cache directory that the maintainer can
  clear without breaking anything permanent (mirroring milestone 090's
  `~/.cache/mikebom/fixtures/<sha>/` pattern).
- **FR-012**: The corpus MUST clearly distinguish two failure classes:
  (a) mikebom-behavior regression (invariant mismatch on the scanned
  output — actionable within mikebom), and (b) corpus-infrastructure
  failure (repo unreachable, image gone, disk full — actionable outside
  mikebom).

### Key Entities

- **Corpus Target**: One entry in the manifest. Attributes: name
  (human-readable identifier), source-type (`git-source` or
  `container-image`), source-URL (git clone URL or image reference),
  pinned-identifier (commit SHA or image digest), ecosystem-tag
  (`go|rust|npm|python|java|polyglot-image`), expected-invariants
  (structured assertion set per FR-005).
- **Expected-Invariant Set**: Per-target, per-format assertions on the
  emitted SBOM. **Two layers** per FR-005 hybrid model: (a) coarse
  code-defined invariants — graph-completeness value, expected reason
  codes when partial, expected canonical PURLs; (b) a full-SBOM golden
  file per format for byte-identity drift detection.
- **Corpus Cache**: Local directory holding cloned corpus repos and
  pulled corpus images. Cleared / re-populated on demand; NOT checked
  into the mikebom repo.
- **Snapshot / Golden**: Reference SBOM(s) or reference invariant-set
  captured at pin time; the corpus compares fresh scans against these
  and reports diffs on drift.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The corpus catches an intentional revert of any m194 US1 /
  US2 / US3 / US4 fix within a single invocation, with a diagnostic that
  names the reintroduced orphan class or over-firing classifier reason
  code.
- **SC-002**: The corpus includes at least six targets — at least one per
  ecosystem in {Go, Rust, npm, Python, Java/Maven, polyglot container
  image} — with each ecosystem exercising the corresponding reader path.
- **SC-003**: Every target's source URL passes a "public-only" audit — a
  reviewer can hand-verify each URL resolves to a publicly-reachable
  location without credentials.
- **SC-004**: The default `./scripts/pre-pr.sh` invocation completes
  with a wall-clock **delta ≤ 5 seconds** vs the pre-feature baseline
  (no regression on the fast inner-loop pre-PR gate). Baseline captured
  once and stored under `specs/195-public-corpus-fixtures/scratch/`
  during implementation verification.
- **SC-005**: The opt-in corpus invocation completes end-to-end on a
  fresh developer machine (no cached corpus artifacts) in under 30
  minutes on a standard laptop with a typical broadband connection.
- **SC-006**: Two consecutive corpus runs (same mikebom checkout, same
  pinned corpus manifest) produce byte-identical SBOMs after masking
  known non-deterministic fields, confirming reproducibility.
- **SC-007**: When a corpus target's expected value changes due to an
  intentional mikebom improvement, the corresponding invariant update
  can be regenerated by a single documented command (matching the
  `MIKEBOM_UPDATE_*_GOLDENS=1` UX for existing golden regressions).

## Assumptions

- **Public-source availability**: The chosen public corpus targets remain
  publicly reachable indefinitely. If an upstream is deleted / privatized,
  the corpus is refreshed (a target-replacement PR).
- **`git` is available**: The corpus harness may shell out to `git` for
  source-repo clones. `git` is already a hard prerequisite for mikebom
  development per the existing milestone 090 pattern.
- **Container-image scans require an OCI-pull tool**: Per the Q3
  clarification, container-image targets are IN MVP. The corpus harness
  requires `docker` (or an equivalent OCI-pull mechanism like `skopeo`
  or `crane`) on the invoking machine. The corpus CI runner MUST
  provide one. If unavailable on a developer machine, the container-
  image target(s) skip with a clear diagnostic and the source-only
  targets still run (partial local invocation degrades gracefully; CI
  invocation MUST NOT degrade — missing tool is a CI setup failure).
- **Public network access during corpus invocation**: The corpus assumes
  outbound HTTPS access to public repositories, image registries, and
  package registries. Offline corpus invocation is out of scope for the
  initial delivery.
- **Corpus cache is per-user, not per-project**: The corpus cache lives
  in `~/.cache/mikebom/corpus/<pin>/` (mirroring milestone 090's fixture
  cache), NOT under the mikebom source tree, so it survives clean checkouts.
- **Corpus is validation infrastructure, not a mikebom feature surface**:
  Consumers of the mikebom binary do not see the corpus. It exists only
  to protect the mikebom project against regression classes; it does not
  change any CLI, library API, or SBOM wire shape.
- **Not a replacement for existing golden-regression suites**: The
  synthetic-fixture golden regressions in
  `mikebom-cli/tests/fixtures/golden/` continue to be authoritative for
  byte-identity SBOM shape. The public corpus is an additional layer that
  exercises real-world tree shapes / edge cases synthetic fixtures do
  not.
- **Failure-attribution language matches the existing bug-class
  taxonomy**: When the corpus reports a regression, its language uses
  the existing reason-code vocabulary (orphaned-components-detected,
  transitive-edges-unresolvable, etc.) so maintainers don't need a
  separate glossary.
