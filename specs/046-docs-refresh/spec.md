# Feature Specification: Refresh README and user-facing docs to reflect post-alpha.6 reality

**Feature Branch**: `046-docs-refresh`
**Created**: 2026-04-29
**Status**: Draft
**Input**: User description: "can we analyze the readme and docs? they seem out of date a bit"

## Background

A pre-spec audit (parallel Explore + grep + git log against the
just-shipped v0.1.0-alpha.6) inventoried 10 concrete drift items
across `README.md` and `docs/`. They cluster into three buckets:

- **3 HIGH-severity factual errors** that actively mislead users
  reading the primary docs.
- **4 MEDIUM-severity discoverability or stale-framing gaps** —
  capabilities exist and work, but docs don't mention them or
  imply they're deferred when they're shipped.
- **3 LOW-severity cosmetic items** (stale "new in milestone X"
  framing, internal-jargon leakage in user-facing comments,
  design-notes deferred-items list lagging the active milestone
  cursor).

The full inventory (file:line for every item) is the substrate this
spec works from; the audit was the recon phase, this milestone is
the cleanup phase.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Correct the factual errors that actively mislead (Priority: P1) 🎯 MVP

A user lands on the project's `README.md` or
`docs/user-guide/cli-reference.md` looking for current behavior.
Today, they encounter three statements that are flatly wrong as of
v0.1.0-alpha.6:

1. **README's status line** says `Status: 0.1.0-alpha.3, pre-1.0.`
   when the released version is `0.1.0-alpha.6` (two pre-releases
   ahead).
2. **The CLI reference's `sbom scan` flags table** does not
   document `--image-src docker,remote` — the headline flag of
   v0.1.0-alpha.6 (the very feature the user just upgraded for).
3. **The CLI reference's `--image` flag description** says refs
   are "pulled from the registry, layers decompressed…" — i.e.
   describes registry-first behavior. As of milestone 044, the
   default is **docker-daemon-first**: an OCI ref is resolved
   against the local docker daemon's cache before any registry
   call. The doc contradicts the actual default.

Each of these is independently fixable without coordination; bundled
under one priority because they share an "actively wrong" character
(vs. the P2/P3 buckets which are merely incomplete).

**Why this priority**: factual incorrectness in the most-trafficked
user-facing surfaces. Anyone evaluating mikebom or copying recipes
against alpha.6 will be misled. Cheap to fix; expensive to leave.

**Independent Test**: After the milestone ships, the following
assertions all pass:

- `grep -nE 'Status: 0\.1\.0-alpha\.[0-5]([^0-9]|$)' README.md`
  returns zero matches.
- `grep -n -- '--image-src' docs/user-guide/cli-reference.md`
  returns at least one match in the `sbom scan` flag-table region.
- `grep -nE 'Refs are pulled from the registry|pulled from the registry, layers decompressed' docs/user-guide/cli-reference.md`
  returns zero matches.

**Acceptance Scenarios**:

1. **Given** a user reading README.md to learn the current
   release's version, **When** they read the project status
   header, **Then** the status line names `v0.1.0-alpha.6` (or
   the current cargo workspace version, sourced from a single
   place so future bumps don't drift).
2. **Given** a user looking up CLI flags in
   `docs/user-guide/cli-reference.md`, **When** they read the
   `sbom scan` flag table, **Then** `--image-src
   <docker|remote>[,<...>]` appears with description and at
   least one example illustrating both default and forced-remote
   behavior.
3. **Given** a user reading the `--image` flag's behavior section,
   **When** they look for what happens with an OCI ref, **Then**
   the prose describes "local docker daemon first, registry
   fallback" as the default — with a one-line pointer to
   `--image-src` for users who want to override.

---

### User Story 2 — Surface shipped capabilities that the docs still treat as "coming soon" or omit entirely (Priority: P2)

The MEDIUM-severity drift items aren't factual errors; they're
discoverability gaps. Users have to read the CHANGELOG to discover
that:

1. **The `--image` flag accepts an OCI reference directly** (no
   `docker save` round-trip needed). README's recipes section
   shows only the tarball path (`docker save … | mikebom sbom
   scan --image …tar`); it doesn't mention `mikebom sbom scan
   --image alpine:3.19` works since milestone 034.
2. **`--image-src docker,remote` is the new default**, with both
   `--image-src docker` (local-only) and `--image-src remote`
   (force registry) as documented overrides. README's recipes
   should illustrate at least the docker-first default and the
   force-remote override.
3. **The `--include-legacy-rpmdb` flag works** end-to-end against
   pre-RHEL-8 BDB rpm databases. The CLI reference still says it
   "threads through today as a no-op until that code lands"
   (milestone 004 US4) — that code shipped long ago. Stale framing
   undermines confidence in a mature feature.
4. **OCI layer caching has user-tunable knobs** (`--no-oci-cache`,
   `--oci-cache-size`). The detailed prose exists in the CLI
   reference's "OCI layer caching" section, but the flag-table
   rows for those flags don't link to it; users skimming the
   table miss the depth.

**Why this priority**: capabilities work as advertised in
CHANGELOG; users just have a harder time discovering them than
they should. Improvement, not blocker. Bundled together because
they all live in the same two files (README + cli-reference) and
benefit from a single editorial pass.

**Independent Test**: After the milestone ships:

- README's "Stable recipes" section contains at least one example
  using a bare OCI ref (e.g. `mikebom sbom scan --image
  alpine:3.19`) AND at least one example using `--image-src
  remote`.
- The CLI reference's `--include-legacy-rpmdb` description
  contains no language matching `(no-op|threads through|until
  that code lands|deferred)`.
- The CLI reference's `--no-oci-cache` and `--oci-cache-size`
  flag rows each contain a markdown link to the "OCI layer
  caching" section.

**Acceptance Scenarios**:

1. **Given** a new user reading README's recipes, **When** they
   want to scan a registry image, **Then** they see a one-line
   recipe `mikebom sbom scan --image alpine:3.19` with prose
   explaining mikebom checks the local docker daemon first and
   falls back to a registry pull, and a follow-up recipe
   illustrating `--image-src remote` for users who need a
   guaranteed-fresh fetch.
2. **Given** a user with a CentOS 7 / Amazon Linux 2 image
   reading the CLI reference for `--include-legacy-rpmdb`,
   **When** they look for whether the flag works today, **Then**
   the description simply says it enables the BDB rpmdb reader
   for pre-RHEL-8 images — no "deferred" / "no-op until later"
   framing.
3. **Given** a user evaluating OCI cache trade-offs from the
   CLI reference flag table, **When** they hover or click on
   `--no-oci-cache` or `--oci-cache-size`, **Then** they reach
   the "OCI layer caching" section that explains semantics and
   defaults.

---

### User Story 3 — Cosmetic cleanup of stale framing and internal-jargon leakage (Priority: P3)

Three LOW-severity items are pure polish:

1. **README's intro** says "(new in milestone 013)" about a
   capability that shipped 30+ milestones ago.
2. **CLI reference's `--image` example block** has a comment
   `# 'oci-registry' is on by default as of milestone 033.` —
   internal-milestone-numbering jargon leaking into user-facing
   docs.
3. **`docs/design-notes.md`** still lists glibc / musl / V8
   version-string detection as deferred to "milestone 026.x" and
   PE Authenticode as "deferred from 028." Even when the items
   are still genuinely deferred, the milestone-numbered framing
   is dated and confusing for non-contributors.

**Why this priority**: doesn't mislead, doesn't block; just makes
docs feel maintained rather than archaeological. Quick wins; the
edits are local and obvious.

**Independent Test**: After the milestone ships:

- `grep -rnE '(new in milestone|milestone 0[0-3][0-9])' README.md docs/user-guide/ docs/reference/`
  returns zero matches in user-facing files. (CHANGELOG is
  exempt — milestone numbers are appropriate there.)

**Acceptance Scenarios**:

1. **Given** a user reading README's intro paragraph, **When** they
   look at the SBOM-analysis description, **Then** the sentence
   no longer claims a feature is "new in milestone 013" — either
   the phrasing is dropped or the framing shifts to a
   user-facing version reference (e.g. "since v0.1.0-alpha.X").
2. **Given** a user reading any user-facing doc (README,
   `docs/user-guide/*`, `docs/reference/*`), **When** they encounter
   a comment or note, **Then** internal milestone numbers
   (`milestone 033`, etc.) do not appear; capabilities reference
   user-facing version tags or simply describe current behavior.

---

### Edge Cases

- **Single-source-of-truth for the version pin**: README's status
  line should ideally derive from the cargo workspace version
  somehow (either explicitly noting "see `Cargo.toml`" or
  deferring to a short note that points to the latest release
  tag), so future alpha bumps don't have to remember to update
  prose. The simpler alternative is "update the prose version
  string in this milestone, accept that future bumps must also
  update it, and handle drift via a per-release checklist." Spec
  lands the simpler approach unless the reviewer prefers
  automation.
- **CHANGELOG is the source of truth, not the docs**: this
  milestone reconciles README + `docs/user-guide/*` to current
  behavior. CHANGELOG is exempt — milestone numbers and dated
  language are appropriate there. The drift inventory's grep
  patterns will exclude `CHANGELOG.md`.
- **Goldens / test fixtures embed version strings** (e.g.
  `mikebom-0.1.0-alpha.6` in the SBOM tool field). Those are
  byte-identity-pinned and regenerated on each release bump
  — out of scope for this milestone.

## Requirements *(mandatory)*

### Functional Requirements

#### US1 — HIGH-severity factual error fixes

- **FR-001**: README.md MUST display the current release version
  on the status line. The simplest acceptable form is a literal
  string `v0.1.0-alpha.6` (or whichever tag is current at merge
  time). A more durable form is acceptable if it doesn't add a
  build-time dependency.
- **FR-002**: `docs/user-guide/cli-reference.md` MUST document the
  `--image-src` flag in the `sbom scan` flag table, including:
  (a) the value grammar (`docker`, `remote`, comma list),
  (b) the default (`docker,remote`), (c) at least one example
  illustrating each behavior, and (d) a brief explanation of when
  to override (CI-only, force-fresh-fetch, etc.).
- **FR-003**: `docs/user-guide/cli-reference.md`'s description of
  the `--image <ref>` flag MUST describe the actual default
  behavior — local docker daemon first, registry fallback — not
  registry-first.
- **FR-004**: A grep over README.md for
  `Status: 0\.1\.0-alpha\.[0-5]` returns zero matches post-merge.
- **FR-005**: A grep over `docs/user-guide/cli-reference.md` for
  `Refs are pulled from the registry|pulled from the registry, layers decompressed`
  returns zero matches post-merge.

#### US2 — MEDIUM-severity discoverability + stale-framing fixes

- **FR-006**: README.md's recipes section MUST include at least
  one example invocation using a bare OCI ref (e.g. `mikebom
  sbom scan --image alpine:3.19`) AND at least one example
  using `--image-src remote` to illustrate the force-registry
  override.
- **FR-007**: `docs/user-guide/cli-reference.md`'s
  `--include-legacy-rpmdb` description MUST describe what the
  flag does (enable BDB-format rpmdb reading for pre-RHEL-8
  images), without language suggesting the implementation is
  pending or stubbed.
- **FR-008**: `docs/user-guide/cli-reference.md`'s flag-table
  rows for `--no-oci-cache` and `--oci-cache-size` MUST
  cross-link to the "OCI layer caching" section in the same
  document.

#### US3 — LOW-severity cosmetic / framing cleanup

- **FR-009**: README.md's intro paragraph MUST NOT claim any
  capability is "new in milestone <N>" where N predates the
  current release by more than two minor pre-releases. Either
  drop the framing, or restate as a user-facing version
  reference.
- **FR-010**: User-facing docs (`README.md`, `docs/user-guide/*`,
  `docs/reference/*`) MUST NOT reference internal milestone
  numbers in flag examples or note blocks. CHANGELOG is exempt.
- **FR-011**: `docs/design-notes.md`'s deferred-items lists MAY
  retain milestone-number references if the deferred items are
  still genuinely backlogged (it's a contributor-facing
  document); however, items the audit flagged as already-shipped
  (none today, but check at merge time) MUST be removed or
  reframed as completed.

#### Cross-cutting

- **FR-012**: No changes to code, tests, fixtures, or goldens.
  This is a docs-only milestone. Pre-PR gate must remain green
  with zero diff in `mikebom-cli/src/`, `mikebom-cli/tests/`,
  `mikebom-common/src/`, or `mikebom-cli/tests/fixtures/`.

### Key Entities

- **Drift item**: a (file, location, current-text,
  ground-truth-text, severity) tuple from the audit. The audit
  identified 10; this milestone closes them all.
- **User-facing doc**: `README.md` and any file under `docs/` that
  is referenced from README or surfaced in user-onboarding flows.
  Contributor docs (`docs/contributing/`, `docs/design-notes.md`)
  are partially in scope per FR-011 but with looser standards.

## Success Criteria *(mandatory)*

### Measurable Outcomes

#### US1

- **SC-001**: The 3 HIGH-severity drift items from the audit all
  fixed; the corresponding greps (FR-004, FR-005, plus a
  positive-match `grep '\-\-image\-src'
  docs/user-guide/cli-reference.md`) all pass.

#### US2

- **SC-002**: The 4 MEDIUM-severity drift items all fixed; the
  README + CLI-reference greps in FR-006, FR-007, FR-008 all
  pass.

#### US3

- **SC-003**: A repo-wide grep
  (`grep -rnE '(new in milestone|milestone 0[0-3][0-9])' README.md docs/user-guide/ docs/reference/`)
  returns zero matches.

#### Cross-cutting

- **SC-004**: `git diff main..HEAD -- mikebom-cli/src/ mikebom-common/src/ mikebom-cli/tests/ mikebom-cli/tests/fixtures/`
  is empty (docs-only milestone).
- **SC-005**: `./scripts/pre-pr.sh` clean (clippy + workspace
  tests). All 3 CI lanes green on the milestone PR.
- **SC-006**: A second pass of the audit (re-running the
  Explore-style drift inventory after the PR merges) finds zero
  remaining drift items in the categories this milestone
  scoped.

## Assumptions

- README.md and `docs/user-guide/cli-reference.md` are the two
  primary user-facing surfaces. `docs/design-notes.md` is
  contributor-facing — touched only for the LOW-severity items
  per FR-011.
- The CLI flag surface as documented in the audit is the
  current canonical surface (cross-checked against
  `mikebom-cli/src/cli/scan_cmd.rs:40-179` during audit).
- `CHANGELOG.md` is the authoritative record of what shipped
  in each release; it does not get rewritten by this milestone.
- No CHANGELOG entry needed — docs-only milestones don't
  user-visibly change behavior. The PR description and commit
  messages are sufficient documentation of what changed.

## Out of scope

- Documentation site / docusaurus / mdbook generation (none
  exists today; the docs are flat Markdown).
- Auto-generating CLI reference from `clap`'s help output. Worth
  considering for a future milestone since manual sync is the
  source of this drift, but adding tooling is bigger than this
  cleanup needs.
- Translating docs to other languages.
- Reorganizing `docs/` structure (file moves, table-of-contents
  changes). This is a content-correctness milestone, not an
  IA refactor.
- Backfilling docs for ANY ecosystem / format / feature beyond
  the audit findings. The audit covered the surfaces a typical
  user touches; if reviewers spot additional drift, file as
  follow-on.
- Updating goldens / fixtures / SBOM test outputs. Those are
  release-bump deliverables (handled in the alpha.6 release PR),
  not a docs concern.
