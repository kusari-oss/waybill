# Feature Specification: Split test fixtures into separate repo to remove security-scanner trigger surface

**Feature Branch**: `090-split-test-fixtures-repo`
**Created**: 2026-05-09
**Status**: Draft
**Input**: User description: "Before getting started here, I would like us to explore moving the various tests that include manifest files into their own repo. This is because some security tooling is starting to trigger on these fake projects and having it as a separate repo makes it easier to just ignore that repo and as part of the tests we should clone down this new repo and run the tests."

## Background

mikebom's test suite ships ~52 fixture directories — many of them deliberately-shaped "fake projects" containing real-looking source-language manifests (`Cargo.toml/.lock`, `package.json/package-lock.json`, `Gemfile.lock`, `pom.xml`, `go.mod/.sum`, `Pipfile.lock`, `requirements.txt`, `pyproject.toml`). They exist to drive mikebom's per-ecosystem readers + parity audits + golden-comparison regression tests.

These fixtures are now causing collateral damage with security tooling that runs against the mikebom repo itself: scanners flag the intentionally-vulnerable fixture lockfiles as if they were mikebom's own production deps. As of milestone 089, a trivy scan of the root repo flags 38+ advisories outside `Cargo.lock` — all rooted in fixture lockfiles like `tests/fixtures/transitive_parity/cargo/Cargo.lock` (clap-rs/clap @ v4.5.21), `tests/fixtures/polyglot-monorepo/frontend/package-lock.json`, etc. Operators (and security tooling that auto-scans Kusari repos) cannot trivially distinguish fixture-vuln noise from real-mikebom-vuln signal.

This milestone moves the manifest-bearing fixture set into a **separate Git repository** that the mikebom main repo no longer carries. Tests clone the fixture repo at test-setup time and resolve fixture paths against the cloned location. Security scans of the mikebom main repo see only its real production deps; security scans of the fixture repo can be allow-listed/excluded by org-level scanner configuration without touching mikebom's main repo posture.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operators see clean security scans of the mikebom main repo (Priority: P1)

A security-conscious operator (or an automated scanner integrated with the Kusari org) runs a vuln scanner against the mikebom main repo and sees only advisories from mikebom's actual production dependencies — not from intentionally-vulnerable test fixtures.

**Why this priority**: This is the entire reason for the milestone. The current state (38+ fixture-vuln advisories drowning out the production-deps signal) makes mikebom's repo-level scan output unactionable for operators trying to assess mikebom's actual security posture. Splitting the trigger surface is the user-visible payoff.

**Independent Test**: Run `trivy fs --scanners vuln --skip-dirs target` against a fresh clone of mikebom's main repo (no `--skip-dirs tests/fixtures` flag needed). Count advisories. Pre-090: ≥38. Post-090: only the residuals already documented in milestone 089's `known-acceptances.md` (currently 4 entries, all `rustls-webpki@0.102.x`).

**Acceptance Scenarios**:

1. **Given** a fresh clone of mikebom main repo post-090, **When** an operator runs `trivy fs --scanners vuln --skip-dirs target` (no fixture-skip flag), **Then** the result matches the production-deps-only result, with zero advisories from manifest-bearing fixtures.
2. **Given** the Kusari org's automated security scanner running its periodic sweep, **When** the scanner indexes the mikebom main repo, **Then** the scanner emits no fixture-vuln false-positives.

---

### User Story 2 - mikebom test suite runs end-to-end after a one-time fixture clone (Priority: P1)

A developer or CI agent running `cargo +stable test --workspace` (or `./scripts/pre-pr.sh`) for the first time after a fresh clone of mikebom's main repo sees the test suite automatically fetch the fixture repo into a known cache location and run successfully — without manual setup steps.

**Why this priority**: Tied with US1. A fixture-split that breaks `cargo test` is a non-starter; the P1 maintainer-loop ergonomics MUST survive intact. CI lanes also depend on this.

**Independent Test**: From a fresh clone of mikebom main repo with no prior fixture cache, run `./scripts/pre-pr.sh`. Confirm the script (or a build-time hook it triggers) clones the fixture repo to a known cache path AND every test suite reports `0 failed`.

**Acceptance Scenarios**:

1. **Given** a fresh clone of mikebom main repo with no fixture cache, **When** the developer runs `cargo +stable test --workspace`, **Then** the test setup automatically fetches the fixture repo to a cache location, fixture-path resolution succeeds, and all tests pass.
2. **Given** a CI lane running on a fresh-each-time runner with no persistent cache, **When** the lane runs `./scripts/pre-pr.sh`, **Then** the lane succeeds end-to-end, and the additional wall-time overhead from the fixture fetch is ≤30 seconds (not user-visible noise).
3. **Given** an existing fixture-cache from a previous test run, **When** the developer runs `cargo test` again, **Then** the test suite reuses the cache without re-cloning (no network roundtrip on the warm path).

---

### User Story 3 - Maintainers can pin a specific fixture-repo revision (Priority: P2)

A maintainer reading the mikebom main repo at any historical commit can determine exactly which fixture-repo revision that commit was tested against, and reproduce the test results deterministically.

**Why this priority**: Without revision pinning, a fixture-repo bump silently changes test inputs across the mikebom commit history — a regression test that passed at commit A might fail at commit A reproduced today simply because the fixture repo moved. Lower than P1 because most tests don't need fixture stability across years (one snapshot per release cycle is enough), but it's a maintainability invariant for milestone-083-style audit work.

**Independent Test**: Check out an arbitrary historical commit of mikebom main repo (e.g., the alpha.27 release tag). Run the test suite. Confirm the test runner fetches the fixture-repo revision pinned for that commit, NOT the latest fixture-repo `main` branch. Test results match the alpha.27-era results.

**Acceptance Scenarios**:

1. **Given** mikebom commit A pinned at fixture-repo revision X, **When** a maintainer at commit A runs the test suite today, **Then** the test runner fetches fixture-repo revision X (not the latest main).
2. **Given** the fixture repo bumps to revision Y (e.g., to add a new fixture for a future ecosystem audit), **When** mikebom main repo is updated to pin revision Y, **Then** the pin update is a single-file change with a clear diff.

---

### User Story 4 - Offline development survives a one-time clone (Priority: P2)

A developer working offline (no network) after a one-time clone of both repos can run `cargo test` repeatedly without network access. CI's existing `--offline` postures (per milestone 020 + others) continue to work for the test execution phase; only the initial fixture fetch requires network.

**Why this priority**: mikebom currently supports fully-offline `cargo test` (per Constitution + per the `--offline` flag in CLI usage). Splitting fixtures must not regress this — a one-time online clone is acceptable, but ongoing tests must not require network. Lower than US1/US2 because it's a maintainer ergonomics concern, not a correctness concern.

**Independent Test**: After a one-time online fixture-cache populate, disconnect network. Run `cargo test --workspace`. All tests pass.

**Acceptance Scenarios**:

1. **Given** a one-time-populated fixture cache and no network, **When** the developer runs `cargo test`, **Then** the test runner reuses the cache and all tests pass with zero network attempts.

---

### Edge Cases

- **Fixture-repo unavailable at test time** (e.g., GitHub outage during CI): tests that require the fixture must fail loudly with an actionable error message ("clone fixture repo from <url> to <path> manually, or check network"), not silently skip. Aligns with Constitution Principle III (Fail Closed).
- **Stale fixture cache** (e.g., the cache pinned at revision X exists locally but mikebom main repo has bumped to revision Y): the test runner detects the mismatch and re-fetches, OR fails loudly with a "clear cache and rerun" message. Silent staleness is forbidden (would re-introduce the milestone-083-style baseline-drift risk).
- **Fixture repo split across two locations**: the current state already has fixtures at both `mikebom-cli/tests/fixtures/` and `tests/fixtures/`. The migration MUST consolidate to a single canonical location in the new repo (no per-crate scattering). Goldens (the test EXPECTED-OUTPUT directory at `mikebom-cli/tests/fixtures/golden/`) MUST stay in mikebom main repo — they encode mikebom's regression contract, not test inputs.
- **Schema files** at `mikebom-cli/tests/fixtures/schemas/` (CycloneDX/SPDX validation schemas) MUST stay in mikebom main repo — they are not "fake projects"; they are real upstream schema documents pinned for offline validation.
- **Binary fixtures** (`tests/fixtures/binaries/{elf,macho,pe}/`, `tests/fixtures/go/binaries/`, `tests/fixtures/polyglot-rpm-binary/`) MAY stay in main repo or move with the manifest fixtures — plan-level decision based on whether they trigger any scanners. They typically don't have source-language manifests and are smaller.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A separate fixture repo MUST be created (under the same Git host org as mikebom main repo) containing every directory that ships a source-language manifest or lockfile (`Cargo.toml/.lock`, `package.json/package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `Gemfile/Gemfile.lock`, `pom.xml`, `go.mod/.sum`, `Pipfile/Pipfile.lock`, `requirements*.txt`, `pyproject.toml`).
- **FR-002**: Every directory listed in FR-001 MUST be removed from the mikebom main repo on the post-090 commit. The mikebom main repo's `git status` after the move MUST show ONLY the moved directories as deletions, plus the test-setup wiring as additions/modifications.
- **FR-003**: The mikebom test runner (whatever wires fixture-path resolution today) MUST be updated to resolve fixture paths against a configurable cache location (default: `~/.cache/mikebom/fixtures/<pinned-rev>/` or similar) populated by a clone of the fixture repo at the pinned revision.
- **FR-004**: A revision pin (commit SHA, Git tag, or equivalent) MUST live in the mikebom main repo (e.g., a `.fixture-rev` file, a `Cargo.toml` metadata entry, or a build script constant) so that any historical mikebom commit can be tested against the matching fixture-repo state.
- **FR-005**: First-run fixture fetch from a fresh mikebom clone MUST complete in ≤30 seconds on standard developer hardware over a residential broadband connection. Wall-time overhead measured against the existing pre-PR-gate baseline (~9 minutes).
- **FR-006**: Subsequent test runs (cache warm) MUST add zero network requests. Verified by running tests with network disabled after a one-time online setup.
- **FR-007**: Fixture-fetch failure (network unavailable, repo unreachable, revision not found) MUST emit an actionable error message naming the expected URL + cache path + the workaround command — not a silent test skip. Aligns with Constitution Principle III.
- **FR-008**: Goldens at `mikebom-cli/tests/fixtures/golden/` MUST stay in the mikebom main repo (they encode the regression contract). Schemas at `mikebom-cli/tests/fixtures/schemas/` MUST stay (they are not fake projects). Other non-manifest assets (binary fixtures, OS-package synthetic fixtures, the `reference/` directory) MAY stay in the main repo OR move — plan-level decision.
- **FR-009**: Existing test code that references `tests/fixtures/<path>` or `mikebom-cli/tests/fixtures/<path>` MUST be updated to use the new fixture-cache resolution mechanism. Zero test deletions; zero test skips that weren't already skipped pre-090.
- **FR-010**: CI lanes (Linux, macOS, eBPF-feature) MUST work without manual setup beyond the existing checkout step. The fixture fetch happens as part of the standard test-run flow, transparently.

### Key Entities

- **Fixture repo**: a new Git repository (named e.g., `mikebom-test-fixtures` under the same Git org as `mikebom`) containing the manifest-bearing fake projects. Public / private status is plan-level.
- **Fixture-cache directory**: a per-developer (and per-CI-runner) on-disk location where the fixture repo is cloned. Cache key includes the pinned revision so multiple revisions can co-exist (useful for git-bisecting through mikebom history).
- **Fixture-revision pin**: a single source-of-truth file/value in mikebom main repo recording the fixture-repo SHA or tag the current mikebom commit was tested against.
- **Fixture-path resolver**: the test-helper code (currently `mikebom-cli/tests/transitive_parity_common/mod.rs::fixture_path` and similar `workspace_root().join("tests/fixtures/...")` patterns) that returns absolute paths to fixture directories. Updated to resolve against the fixture-cache instead of `workspace_root()`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Post-090, a trivy fs scan against a fresh clone of mikebom main repo (no `--skip-dirs tests/fixtures` flags) flags zero advisories from manifest-bearing fake projects. The only flagged advisories are mikebom's real production-deps residuals (currently 4 entries documented in milestone 089's `known-acceptances.md`).
- **SC-002**: Maintainers running `./scripts/pre-pr.sh` post-090 see clean output: zero clippy warnings, every test suite `0 failed`, identical pass-rate to pre-090.
- **SC-003**: First-run fixture-fetch wall-time is ≤30 seconds on standard developer hardware. Tracked as a regression metric in the pre-PR script's logged timings.
- **SC-004**: Cache-warm test runs add zero network requests. Verified by running tests with network disabled after a one-time setup.
- **SC-005**: Mikebom main repo size shrinks by ≥10 MB (current `tests/fixtures/` is 16 MB + `mikebom-cli/tests/fixtures/` 1.8 MB = 17.8 MB total; goldens + schemas + binary fixtures retained ≈ 5–7 MB; net reduction ≈ 10–12 MB). Measured by `git clone --depth 1` size pre vs post.
- **SC-006**: Historical reproducibility holds: checking out mikebom main repo at the alpha.27 release tag and running `cargo test` against the matching pinned fixture revision yields identical pass-rate to running alpha.27 pre-090 (modulo the pre-090 vs post-090 fixture-resolution mechanism difference).

## Assumptions

- The fixture repo will live in the same Git host org as mikebom main repo (e.g., `kusari-sandbox/mikebom-test-fixtures`). Public visibility matches mikebom main repo. Org-level decision made outside this milestone.
- Fixture-fetch over HTTPS Git is acceptable; no SSH-only dependency.
- Mikebom developers are OK with a one-time online clone after `git clone mikebom`. The "fully offline first run" use case is not supported and not in scope.
- Goldens at `mikebom-cli/tests/fixtures/golden/` stay in main repo — they're the EXPECTED-OUTPUT regression contract, not test inputs. Same for schemas at `mikebom-cli/tests/fixtures/schemas/`.
- Binary fixtures (ELF/MachO/PE/Go binaries) MAY move or stay; they don't have source-language manifests so they don't trigger SBOM-aware scanners. Plan-level decision based on size / scanner-behavior tradeoffs.
- The split is a one-shot migration, not an incremental rollout — every manifest-bearing fixture moves in the same PR. Partial-state intermediate commits (some fixtures moved, others not) would create a confusing dual-resolution path that's worse than either current state or final state.

## Dependencies

- Org-level Git permission to create a new repo under the same org as mikebom main repo.
- mikebom alpha.27 (released) as the baseline. Post-090 may or may not warrant an alpha.28 release depending on test-setup-flow user-visibility; the dep-graph and SBOM emission are unchanged so the user-observable behavior is unaffected.
- Constitution Principle III (Fail Closed) — fixture-fetch failures must surface explicit errors, not silent test skips.

## Out of Scope

- **Replacing test fixtures with synthetic data** — keeping the real-world fixtures (clap-rs/clap, fastlane/fastlane, etc.) is essential for the milestone-083 transitive-parity audit's value (it specifically validates against real-world dep graphs). The fix is to MOVE them, not to remove them.
- **Adding new fixtures** — this milestone touches only the location of existing fixtures, not their content.
- **Reorganizing fixtures within the new repo** — the new repo's internal directory layout may mirror the current mikebom layout (`<ecosystem>/<name>/`). Plan-level decision; not blocking.
- **Allow-listing the new fixture repo in security scanners** — that's an org-level scanner-config change (e.g., adding the fixture repo to a scanner's exclude list), not a code change in either repo.
- **Migrating fixture vulnerabilities to a separate vuln-acceptance file** — fixtures are intentionally vulnerable; the new repo's `README.md` documents the design intent but doesn't need a per-vuln acceptance file (those are for production deps).
- **Refactoring the existing `transitive_parity_common::fixture_path` helper or any other test helper beyond the path-resolution-mechanism update** — minimal-scope helper update only.
