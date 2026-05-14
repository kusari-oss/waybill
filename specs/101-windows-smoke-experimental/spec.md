# Feature Specification: Windows smoke test + experimental docs callout

**Feature Branch**: `101-windows-smoke-experimental`
**Created**: 2026-05-13
**Status**: Draft
**Input**: User description: "Add Windows-host smoke test + mark Windows binary experimental in docs."

## Clarifications

### Session 2026-05-13

- Q: Smoke test ecosystem scope — cargo only, cargo + one polyglot fixture, or all 6 cross-platform readers separately? → A: Option B — cargo + one polyglot fixture (the existing `polyglot-monorepo` fixture, which covers pypi + npm). This gives multi-reader confidence (3 ecosystems exercised: cargo, pypi, npm) at ~15 sec runtime; balances coverage vs CI-time cost.
- Q: Smoke test per-scan timeout / hang detection — 30s / 60s / 5min / none? → A: Option A — 60-second hard timeout per scan invocation. On timeout, kill the subprocess and fail the test with a "scan timed out — likely hang regression" message. 12× safety margin over the typical <5s runtime; catches the milestone-054 symlink-loop regression class without false positives on Windows runner variability.
- Q: Failure-diagnostic policy — minimal panic, inline diagnostics + `actual.json`, full SBOM dump, or CI artifact upload? → A: Option B — inline diagnostics + `actual.json` tempfile. On failure, print first N component PURLs, the asserted-but-missing prefix, the path-shaped fields containing backslashes (if any), AND write the full emitted JSON to a per-test tempdir as `actual.json` for local inspection. Matches the existing `cdx_regression.rs` `.actual.json`-next-to-golden pattern; no upload-artifact CI step needed.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operator trusts Windows binary won't silently break (Priority: P1)

A Windows operator downloads `mikebom-v<version>-x86_64-pc-windows-msvc.zip`, extracts `mikebom.exe`, and points it at a real Rust project. They need confidence the binary will at least run and emit a structurally valid CDX SBOM — even if some advanced features (Linux-only readers, OCI cache, full per-ecosystem fidelity) don't work yet. Before milestone 100, the only signal Windows didn't regress was the clippy step on the Windows CI lane; clippy proves the binary *compiles* but not that it *runs*.

This story adds a minimal end-to-end smoke test that exercises the Windows binary against a small bundled cargo fixture and asserts the headline behaviors users actually depend on: the process exits 0, the emitted file is parseable JSON conforming to CycloneDX 1.6, and the path strings inside the SBOM are forward-slash normalized (Contract 3 from milestone 100). Any regression that would make `mikebom.exe sbom scan` unusable on Windows trips this test before merge.

**Why this priority**: This is the cheapest gate that catches the regression class users will actually encounter. Without it, a future PR could break Windows-binary runtime behavior (e.g., subtly miscompiled code path, registry-dependent initialization, malformed JSON serialization) without any CI signal — because the broader Windows test step is marked `continue-on-error: true` per milestone 100's descope. Inserting one targeted end-to-end smoke test restores defense against the most catastrophic regression mode without requiring full POSIX-test-suite parity.

**Independent Test**: On a `windows-latest` runner, the new smoke test invokes `mikebom.exe sbom scan --path <bundled-cargo-fixture> --output out.cdx.json`, asserts exit code 0, parses `out.cdx.json` as JSON, validates `bomFormat == "CycloneDX"` and `specVersion == "1.6"`, confirms at least one component has a `purl` starting with `pkg:cargo/`, and checks that every path-shaped value (`evidence.occurrences[].location`, `mikebom:source-files` property values) contains zero backslash characters. The test runs as part of `cargo test --workspace` on the Windows lane and as a dedicated CI gating step on the same lane (not `continue-on-error`).

**Acceptance Scenarios**:

1. **Given** a working Windows-built `mikebom.exe` and a bundled cargo fixture directory, **When** the smoke test runs `mikebom.exe sbom scan --path <fixture> --output out.cdx.json`, **Then** the process exits 0, `out.cdx.json` is parseable as a CycloneDX 1.6 document, and the components array contains ≥1 entry with a `pkg:cargo/...` PURL.
2. **Given** the same setup, **When** the test inspects every path-shaped string field in the emitted JSON, **Then** none contain a literal backslash character — confirming the milestone-100 path normalization is in effect at runtime.
3. **Given** a future PR that introduces a runtime regression (e.g., panic on startup, malformed JSON, missing forward-slash normalization), **When** the Windows CI lane runs, **Then** the smoke test fails and blocks the merge, while the broader `cargo test --workspace` step's per-test backlog (issue #210) continues to run non-blocking for visibility.

---

### User Story 2 - User understands Windows support is experimental (Priority: P1)

A potential mikebom user lands on the README or the installation guide, sees Windows listed as supported, and downloads the binary expecting the same fidelity as Linux/macOS. Today's docs (post-milestone-100) say "Windows: supported (milestone 100)" — accurate but misleading: dpkg/rpm/apk readers don't apply on Windows, four production-code Windows-portability bugs are catalogued in #210, and the full `cargo test --workspace` doesn't pass.

This story adds a prominent "🧪 Experimental" callout to both the README's Windows install section and `docs/user-guide/installation.md`'s platform notes. The callout explicitly states (a) Windows builds are available, (b) they are not feature-equivalent to Linux/macOS yet, (c) known gaps are tracked in #210, and (d) operators should not rely on Windows builds for production SBOM workflows until #210 closes.

**Why this priority**: One-time documentation fix that directly addresses the user's stated concern ("make clear there is a windows binary but it's experimental and should not be relied on"). It costs nothing at runtime and prevents the most common user-frustration pattern: discovering the limitations only after committing to a Windows-based workflow. Ships together with Story 1 because both are about restoring honest signal — Story 1 to CI, Story 2 to docs.

**Independent Test**: Inspect README.md and `docs/user-guide/installation.md` rendered in GitHub's Markdown view; confirm the "🧪 Experimental" callout appears within the first paragraph of each Windows-relevant section, links to issue #210, and explicitly states "not feature-equivalent" and "not for production use yet."

**Acceptance Scenarios**:

1. **Given** a user reading README.md's "Windows install" section, **When** they scroll to the Windows row, **Then** they see a callout prefixed with "🧪 Experimental" (or equivalently emphasized prose) in the first paragraph, the callout names the known-gap categories (Linux-only OS readers, HOME env-var derivation, OCI cache, path-resolver matcher), and links to issue #210.
2. **Given** a user reading `docs/user-guide/installation.md`, **When** they reach the platform-support discussion, **Then** they encounter the same experimental warning consistent with README.md (same gap categories, same #210 link).
3. **Given** the existing platform-support table in README.md, **When** the user inspects the Windows row, **Then** the cell text reads `🧪 experimental (milestone 100)` or equivalent (not the current `✅ supported (milestone 100)`).

---

### User Story 3 - Smoke test gates on regressions (Priority: P2)

The Windows lane's Tests step currently runs with `continue-on-error: true` per milestone 100's descope. That means any runtime regression in the smoke-test workflow lands silently — only clippy gates the merge. To restore a runtime gate for the headline behavior, split the Windows Tests step into two: (a) the new smoke test as a dedicated step **without** `continue-on-error` (blocks merge on regression), and (b) the broader `cargo test --workspace` step as today, still `continue-on-error: true` (per-test backlog stays visible but non-blocking).

**Why this priority**: P2 because Stories 1 + 2 are independently shippable — even with the smoke test running inside the existing non-blocking step, it surfaces in CI logs as documented failures and a maintainer would catch a regression on PR review. Splitting the steps is the "make-the-gate-strict" enhancement that takes Windows from "compiles + emits SBOMs at least once at PR creation" to "compiles + emits SBOMs *every PR*". Worth doing in the same PR because the CI workflow file is already being touched, but a future PR could add this if Story 1 + 2 ship alone.

**Independent Test**: Inspect `.github/workflows/ci.yml`; confirm the Windows lane has two test steps. The smoke-test step runs the new smoke test with a filter (e.g., `cargo +stable test --test scan_windows_smoke`) without `continue-on-error`. The broader `cargo +stable test --workspace` step retains `continue-on-error: true`. A follow-up PR that deliberately breaks `mikebom.exe sbom scan` verifies the Windows lane blocks the merge.

**Acceptance Scenarios**:

1. **Given** the Windows CI lane with the new two-step layout, **When** a PR introduces a smoke-test-breaking regression, **Then** the smoke-test step fails with `continue-on-error: false` and blocks the merge.
2. **Given** the same layout, **When** a PR triggers a non-smoke per-test failure (e.g., one of the #210-tracked backlog tests), **Then** the broader test step reports failure but `continue-on-error: true` lets the merge proceed.
3. **Given** the smoke test is the *only* test in the smoke-test step, **When** the broader test step crashes during compilation, **Then** the smoke step still runs independently (proves the build worked and core scan functionality is intact).

---

### Edge Cases

- **Smoke test cargo fixture missing**: the test loads the fixture from `mikebom-cli/tests/fixtures/cargo/lockfile-v3/` (already vendored via milestone 090's fixture cache + already used by `cdx_regression_cargo`). If the cache is empty or missing, the test fails fast with a clear error referencing the fixture-cache build.rs.
- **Build artifact missing on Windows runner**: the smoke test invokes the locally-built `mikebom` binary (via `env!("CARGO_BIN_EXE_mikebom")` — cargo's standard integration-test pattern). If the binary failed to build, the upstream clippy/test compilation fails first.
- **CDX schema drift**: if a future mikebom version changes the JSON shape (e.g., `bomFormat` field renamed), the smoke test must adapt or it'll false-positive. Mitigation: the assertions are minimal (`bomFormat == "CycloneDX"`, `specVersion == "1.6"`, components is a non-empty array with ≥1 cargo PURL) — these are stable parts of the CDX 1.6 spec and unchanged across the last 8+ milestones.
- **Backslash in legitimate non-path content**: CPE 2.3 strings contain literal `\/` escape sequences (already seen during milestone 100). The smoke test's backslash check must be scoped to path-shaped fields only (`mikebom:source-files`, `evidence.occurrences[].location`, `mikebom:source-path`), not blanket-scanning the JSON document.
- **Docs callout placement on README**: the existing README has a "Why" section, an "Install" section, a "Supported ecosystems" section, etc. The Windows callout must appear in the first place a Windows-curious reader looks (the platform-support table + the dedicated Windows install/usage subsection).
- **#210 reference becomes stale**: if issue #210 closes (Windows portability fixes ship), the experimental callout becomes overcautious. Mitigation: #210's resolution PR includes "Remove the experimental callout from README + installation.md" in its task list.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST include a Windows-host integration smoke test that, on `windows-latest`, builds `mikebom`, invokes `mikebom sbom scan` against TWO vendored fixtures — (1) the cargo `lockfile-v3` fixture and (2) the `polyglot-monorepo` fixture (pypi + npm) — and asserts exit code 0 for each invocation.
- **FR-002**: For BOTH scan invocations, the smoke test MUST validate that the emitted SBOM file parses as JSON, has `bomFormat == "CycloneDX"`, has `specVersion == "1.6"`, and has a non-empty `components[]` array. The cargo-fixture run MUST contain ≥1 component whose `purl` starts with `pkg:cargo/`. The polyglot-fixture run MUST contain ≥1 `pkg:pypi/` AND ≥1 `pkg:npm/` component — providing multi-reader regression coverage for the three most common cross-platform ecosystems.
- **FR-003**: The smoke test MUST assert that every path-shaped field in the emitted SBOM (specifically `mikebom:source-files` property values, `evidence.occurrences[].location` if present, `mikebom:source-path` property values if present) contains zero literal backslash (`\`) characters — confirming milestone-100 forward-slash normalization is in effect at runtime.
- **FR-004**: The smoke test MUST be gated `#[cfg(windows)]` so it doesn't run on Linux/macOS hosts (where the equivalent forward-slash behavior is already covered by existing goldens regression tests).
- **FR-005**: System MUST update README.md's Windows section to include a prominent "🧪 Experimental" callout in the first paragraph that (a) explicitly says Windows builds are not feature-equivalent to Linux/macOS, (b) lists the known gap categories (Linux-only OS package readers, HOME env var fallback, OCI cache atomic-rename, path-resolver matcher, Python stdlib collapse), (c) links to issue #210, and (d) advises against production reliance.
- **FR-006**: System MUST update `docs/user-guide/installation.md` with an equivalent experimental warning consistent with README's wording and gap list. The two docs MUST reference the same issue #210 and the same gap categories.
- **FR-007**: System MUST update the existing platform-support table in README.md so the Windows row reads `🧪 experimental (milestone 100, #210)` (replacing the current `✅ supported (milestone 100)` cell). The cell text MUST link `#210` via GitHub's auto-link or an explicit `[#210](https://github.com/kusari-sandbox/mikebom/issues/210)` markdown link.
- **FR-008**: `.github/workflows/ci.yml`'s `lint-and-test-windows` job MUST split the test step into (a) a smoke-test-only step **without** `continue-on-error: true` (gates the merge), and (b) the existing broader `cargo test --workspace` step retained with `continue-on-error: true` (per-test #210 backlog stays visible but non-blocking).
- **FR-009**: System MUST NOT introduce new Cargo dependencies. The smoke test uses `std::process::Command` for binary invocation and `serde_json::Value` for parsing — both already in the dependency closure. The 60-second timeout uses `std::time::Instant` + a polling wait loop (cross-platform), or a dedicated thread that calls `Child::kill()` after the deadline — std-only, no `wait-timeout` or `timeout` crate.
- **FR-010**: System MUST NOT modify the production-code Windows path-handling logic. This PR is test-and-docs-only; production fixes remain in issue #210.
- **FR-011**: The smoke test MUST enforce a hard 60-second per-scan timeout. If `mikebom sbom scan` does not exit within 60 seconds, the test MUST kill the subprocess via `Child::kill()` and fail with a clear diagnostic message ("scan timed out — likely hang regression"). This guards against hang-regressions (e.g., the milestone-054 symlink-loop class) without waiting for the GitHub Actions job-level timeout.
- **FR-012**: On any assertion failure, the smoke test MUST print inline diagnostics to stderr including (a) the first **10** emitted component PURLs (fixed cap), (b) the asserted-but-missing PURL prefix or path-field issue, (c) the path-shaped fields containing backslashes (if FR-003 fails — print up to **5** offending field/value pairs), AND (d) write the full emitted SBOM JSON to a per-test tempdir as `actual.cdx.json` with the absolute path printed so a maintainer can inspect locally. Matches the existing `cdx_regression.rs` `.actual.json`-next-to-golden diagnostic pattern.

### Key Entities *(include if feature involves data)*

- **Smoke-test fixtures**: TWO existing fixtures reused from milestone-090's fixture cache; no new fixtures added.
  - **Cargo fixture**: `<MIKEBOM_FIXTURES_DIR>/cargo/lockfile-v3/` (already used by `cdx_regression_cargo`).
  - **Polyglot fixture**: `<MIKEBOM_FIXTURES_DIR>/polyglot-monorepo/` (already used by `scan_polyglot_monorepo.rs`; covers pypi + npm).
- **Smoke-test artifact**: `out.cdx.json` (an emitted CycloneDX 1.6 document), written to a per-test tempdir and asserted against. Not persisted.
- **Experimental callout**: prose + linked issue reference, added to README.md and `docs/user-guide/installation.md`. Two locations; one canonical wording referenced from both.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A maintainer can verify Windows binary runtime behavior in <5 seconds of CI log inspection by reading the smoke-test step's pass/fail result on any PR. (Today: requires reading the entire `cargo test --workspace` non-blocking output and discerning whether failures are #210-backlog or new regressions.)
- **SC-002**: A regression that breaks `mikebom.exe sbom scan --path <cargo-project>` on Windows fails the Windows CI lane (blocks the merge) ≥99% of the time — i.e., the smoke test detects the regression class users care about most.
- **SC-003**: A first-time Windows user reading README.md encounters the "experimental" notice within the first paragraph of any Windows-relevant section, with no scrolling required after they land on the Windows-install heading.
- **SC-004**: The two documentation locations (README.md + `docs/user-guide/installation.md`) state the same experimental warning, link to the same issue, and list the same gap categories — verifiable by string-equivalence of the callout's content portion.
- **SC-005**: Build-correctness gating remains intact: `cargo +stable clippy --workspace --all-targets -- -D warnings` continues to gate the Windows lane (unchanged from milestone 100). The smoke test adds a complementary runtime gate without weakening any existing gate.
- **SC-006**: Zero net change to Linux/macOS CI behavior: the existing Linux + macOS lanes' behavior is unchanged (the smoke test is `#[cfg(windows)]` so it's a no-op on those hosts; the docs callouts don't affect any code path).
- **SC-007**: The smoke test runs in <30 seconds on the Windows CI lane (two `mikebom sbom scan` invocations — cargo fixture + polyglot fixture — plus JSON parsing and assertions; well under the 9-minute existing Windows build time).
- **SC-008**: Diff scope is bounded: ≤4 modified files (1 NEW test file, README.md, installation.md, ci.yml) + 0 production-code changes + 0 new Cargo dependencies.

## Assumptions

- The Windows CI lane's `Lint + test (windows-latest)` job successfully produces a working `mikebom.exe` (verified by milestone 100's clippy gate); the smoke test uses cargo's `CARGO_BIN_EXE_mikebom` integration-test pattern, no separate `gh release` download step.
- The vendored cargo fixture at `mikebom-cli/tests/fixtures/cargo/lockfile-v3/` (or equivalent post-milestone-090 fixture-cache path) is available on the Windows runner. Milestone 090's `actions/cache` for the fixture repo applies to Windows too (verified by the `cdx_regression_cargo` test, which IS passing on Windows post-milestone-100).
- Issue #210 will continue to exist as the canonical follow-up tracker; the docs reference it by number. If #210 is renumbered or the repo migrates, both docs callouts need the same one-line update.
- The "Experimental" label is a temporary state — when #210 closes, a follow-up PR drops the callouts. This is non-blocking; the docs accurately reflect today's state regardless of when that follow-up lands.
- The smoke test's assertions (`bomFormat`, `specVersion`, `pkg:cargo/` PURL prefix, no-backslash-in-path-fields) are stable parts of the CDX 1.6 spec + the milestone-100 path-normalization contract; they should not need adjustment for the foreseeable future.
- The CI workflow split (FR-008) is a YAML-only change. Splitting the test step doesn't require new GitHub Actions extensions or matrix changes.
- The README's platform-support table is in markdown (verified via existing milestone-100 edits). The cell update is a simple text replacement.
