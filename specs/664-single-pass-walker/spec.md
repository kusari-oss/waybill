# Feature Specification: Single-Pass Walker with Reader-Registry Dispatch

**Feature Branch**: `664-single-pass-walker`
**Created**: 2026-08-21
**Status**: Draft
**Input**: User description: "Single-pass filesystem walker with reader-registry dispatch to eliminate N-times tree walks in package_db::read_all. npm keeps its own inner node_modules walk (option 1 for pragmatism)."

## Clarifications

### Session 2026-08-21

- Q: Same-directory sibling lookup — cache or re-read? → A: The shared walker builds an in-memory (directory → [filenames]) index during descent; the sibling-lookup helper reads from that index (zero extra syscalls per sibling query).
- Q: File-tier walker (m133) — bundle or defer? → A: Defer m133 to a follow-on milestone. This milestone scopes to package-db readers only. m133 keeps its own pass, unchanged; perf targets stay source-tree-focused.
- Q: Coexistence redundancy during the migration window — how does the shared walker behave? → A: Additive walks. The shared walker runs whenever at least one reader has registered interest, and legacy-walker readers continue to walk independently until they migrate. Walker costs are additive during migration; US1 pilot must migrate enough hot readers that the saved legacy walker cost exceeds the added shared walker cost by a margin sufficient to hit the US1 SC target.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Registry infrastructure + top-offender pilot migration (Priority: P1)

An operator scanning a Python-only project (like ansible) sees waybill finish materially faster than today because the readers that don't apply to their tree no longer independently walk it looking for their manifest files.

**Why this priority**: This user story delivers the first measurable perf win AND validates that the coexistence-based migration strategy is safe. Without this piece, the rest of the milestone is architectural speculation. It also establishes the registry API surface that every subsequent reader migration will use.

**Independent Test**: On an ansible checkout (5,793 files, ~500 dirs), running `waybill sbom scan --offline --file-inventory=off` completes at least 15% faster than the pre-milestone baseline (baseline 4.10s → target ≤ 3.5s in US1). The headline SC-001 (≤ 1.2s, ≥3.4× improvement) lands at US2 once every walker-using reader has migrated. Every existing golden SBOM test in the US1-migrated readers produces byte-identical output. Rationale for the ≤ 3.5s (rather than ≤ 3.0s) US1 target: the 2026-08-21 Phase-3-implementation audit found that the 5 clean-shape pilot readers (haskell, scala, erlang, rpm_file, ipk_file) collectively save ~790 ms of legacy walker cost on the ansible baseline; after paying the ~120 ms shared-walker tax during coexistence (FR-004), net improvement is ~670 ms → 4.10s → ≈3.43s.

**Acceptance Scenarios**:

1. **Given** an ansible checkout on the pre-milestone baseline, **When** an operator runs an offline scan, **Then** wall-clock time is ~4s and the scan visits the tree ~28 times (once per reader).
2. **Given** the same ansible checkout after US1 ships, **When** an operator runs the same offline scan, **Then** wall-clock time is ≤ 3.5s and the pilot readers dispatch via the shared walker (non-migrated readers continue to walk independently until US2 covers them). The additive-walk tax during coexistence (FR-004) is why the US1 target is a partial improvement rather than the headline SC-001 number.
3. **Given** the existing golden SBOM tests for every reader migrated in US1, **When** the test suite runs against the new registry-dispatched code path, **Then** every golden matches byte-for-byte.
4. **Given** a reader that has NOT yet been migrated in US1, **When** the scan runs, **Then** that reader continues to use its existing walker call site with no observable behavior change (coexistence property).

---

### User Story 2 - Remaining walker-using readers migrated (Priority: P2)

An operator scanning any project — including large polyglot trees like PyTorch and MongoDB — sees waybill approach the wall-clock cost of single-purpose SBOM tools without giving up any of waybill's higher-fidelity output.

**Why this priority**: This user story completes the migration and delivers the headline perf win. It's P2 rather than P1 because the P1 slice already validates the approach; P2 mechanically extends the win to the remaining readers.

**Independent Test**: On a PyTorch checkout (21,649 files) and a MongoDB checkout (55,186 files), running `waybill sbom scan --offline --file-inventory=off` completes within the P2 targets (see SC-002 and SC-003 below), and every existing golden SBOM test across every reader migrated in this milestone produces byte-identical output.

**Acceptance Scenarios**:

1. **Given** a PyTorch checkout, **When** an operator runs an offline scan post-US2, **Then** wall-clock time is ≤ 1.5s (pre-milestone baseline was 4.30s).
2. **Given** a MongoDB checkout, **When** an operator runs an offline scan post-US2, **Then** wall-clock time is ≤ 3.0s (pre-milestone baseline was 15.68s).
3. **Given** every existing golden SBOM test in the workspace, **When** the full test suite runs post-US2, **Then** every golden matches byte-for-byte.
4. **Given** an operator has custom scripts that invoke `waybill sbom scan` with any combination of currently-supported flags, **When** those scripts run post-US2, **Then** every emitted SBOM is byte-identical to the pre-milestone output (the operator observes only the speed change, not any behavioral difference).

---

### User Story 3 - Perf regression guard + legacy call-site removal (Priority: P3)

A future contributor accidentally adding a per-reader independent walker sees CI fail with a clear error message pointing at the shared registry as the expected dispatch mechanism.

**Why this priority**: This user story locks in the perf win against regression. It's P3 because it delivers no immediate perf improvement of its own — it's insurance against future drift. Ship it after US2 has stabilized so we know exactly what the guard should assert.

**Independent Test**: A synthetic test PR that adds a new reader calling `safe_walk` outside the shared registry MUST fail CI with a diagnostic pointing at the new registry API. Additionally, a synthetic 10,000-file test tree measures p95 per-file dispatch overhead ≤ 100 µs.

**Acceptance Scenarios**:

1. **Given** every reader migrated in US1 and US2, **When** US3 ships, **Then** every one of those readers' code paths no longer contains a `safe_walk` call site (the legacy paths are physically deleted).
2. **Given** a synthetic microbenchmark building a 10,000-file tree with mixed manifest and non-manifest files, **When** the benchmark measures per-file dispatch overhead across the shared walker, **Then** the p95 sample is ≤ 100 µs.
3. **Given** a hypothetical contributor PR that adds a new reader calling `safe_walk` directly, **When** CI runs, **Then** CI fails with a message referencing the shared-registry migration policy.

---

### Edge Cases

- **Two readers register the same filename pattern**: Both readers are dispatched — pattern matching is inclusive, not first-match-wins. Rationale: today, two readers observing the same file (e.g., `pyproject.toml` is read by pip AND by pip's main-module emitter AND by m183 optional-extras derivation) is the normal case; the registry preserves that.
- **Reader callback panics**: The panic is caught and logged (following the m209 resolver-chain `catch_unwind` precedent); the shared walker continues dispatching to the remaining readers. Rationale: one buggy reader must not corrupt scan output for all others.
- **Symlink loop encountered during descent**: The shared walker's visited-set semantics (m054 / m114) are preserved verbatim — a canonicalized path already visited is skipped. Rationale: current behavior; migrating must not regress it.
- **Directory cannot be read (permission denied, transient EIO)**: Silent skip, matching the current permissive posture from m114 (`safe_walk` FR-011). Rationale: air-gapped scanners and rootfs scans routinely hit unreadable directories; the walker must not abort.
- **Reader that today walks twice with different `max_depth`**: The registry allows a reader to register more than one glob-pattern-plus-callback pair with per-registration configuration. Rationale: preserves reader flexibility without forcing an artificial single-pattern model.
- **Reader that today walks a specific subtree only**: The registry allows a reader to opt out of the shared walk and use its own `safe_walk` call site (npm's `node_modules/**` inner walk is the reference case per FR-005). Rationale: pragmatism decision — some readers genuinely need bounded, content-driven descent.
- **File-tier component emission (m133)**: The m133 file-tier walker is OUT OF SCOPE for this milestone (see FR-013). It keeps its own independent walker pass, unchanged. Rationale: m133's callback shape (SHA-256 hashing + content-shape gate + package-tier occurrence-set dedupe) is materially different from manifest-file dispatch; bundling would double the migration's blast radius and complicate the FR-006 golden-identity guarantee. Follow-on milestone.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST provide a reader-registry mechanism through which every ecosystem reader that today walks the filesystem can declare its interest in specific filename patterns and receive per-file callbacks during a single shared walk of the scan tree.
- **FR-002**: The shared walker MUST support all existing `WalkConfig` semantics for the whole-scan traversal (max-depth, base-name skip predicate, m113 exclude-path set, m114 permissive-on-error posture, m054 symlink-loop protection).
- **FR-003**: The system MUST support two-phase reader logic (find a project marker → read siblings in the same directory) via a same-directory sibling-lookup helper accessible from the per-file callback. The helper MUST serve queries from an in-memory (directory → [filenames]) index that the shared walker populates during descent; the helper MUST NOT trigger a fresh `read_dir()` syscall per query. Rationale: pip, cargo, gem, cocoapods, elixir, erlang, scala, haskell, and several others follow this pattern today; if the sibling helper re-opened directories, the per-lookup syscall cost would erode the SC-001/SC-002/SC-003 wins.
- **FR-004**: Migration MUST proceed one reader at a time (coexistence property). Both the shared registry and the existing per-reader `safe_walk` call sites MUST function correctly during the migration window; a reader is either "using the registry" or "using its legacy safe_walk," never both, at any given commit. During the migration window, the shared walker runs whenever at least one reader has registered interest; non-migrated readers continue to walk independently via their existing `safe_walk` call sites. Walker costs are additive during migration (the shared walker adds a walker-floor tax of roughly the m664 baseline `--no-package-db` cost on top of the still-running legacy walkers); pilot-reader sizing for US1 MUST account for this so the net improvement exceeds the tax by a margin sufficient to hit US1's SC.
- **FR-005**: npm's inner `node_modules/**` deep walk MUST remain as an independent `safe_walk` call site (pragmatism decision from the pre-spec design discussion). Only the outer npm project-root discovery walk migrates to the shared registry. This is a permanent scope decision, not a deferred item.
- **FR-006**: Every existing golden SBOM test across the workspace MUST produce byte-identical output before and after each reader migrates to the shared registry. Divergence is a blocker on the migrating PR; the migration must be reworked to preserve byte-identity, not the golden updated.
- **FR-007**: Readers that today read from fixed system paths without walking (dpkg reading `/var/lib/dpkg/status`, apk reading `/lib/apk/db/installed`, alpm reading `/var/lib/pacman/local`, brew reading `$HOMEBREW_PREFIX/Cellar`, rpmdb-bdb reading `/var/lib/rpm`) are OUT OF SCOPE. They do not walk today, so they do not migrate.
- **FR-008**: A perf regression guard MUST exist that fails CI when a new reader is added that calls `safe_walk` outside the shared registry, provided that new reader is one of the reader kinds covered by this milestone's scope. Readers explicitly using their own walk per FR-005-style pragmatism MUST be documented in an allowlist to distinguish them from accidental drift.
- **FR-009**: The scan MUST emit an INFO-level diagnostic log line at completion summarizing (a) the number of shared-walker passes performed (baseline: one), (b) the total files visited by the shared walker, and (c) the per-reader dispatch count for each reader that received callbacks. Rationale: operator visibility into the perf-critical path and a signal for the FR-008 regression guard.
- **FR-010**: No new Cargo dependencies MUST be added by this milestone. All infrastructure MUST be built on `std::path`, `std::fs`, and existing workspace crates. Rationale: Constitution Principle I (Pure Rust, Minimal Dependencies).
- **FR-011**: The system MUST NOT change the semantics of ANY existing CLI flag or emitted SBOM field. Behavior changes are limited to (a) wall-clock time, and (b) the FR-009 diagnostic log. Any other observable change is a bug.
- **FR-012**: The shared walker MUST NOT introduce reader parallelism (rayon or tokio-based fan-out across readers) in this milestone. Reader-level parallelism is explicitly deferred as a follow-on so that the single-pass baseline can be measured cleanly. Rationale: mixing two optimizations at once makes attribution harder if either regresses.
- **FR-013**: The m133 file-tier discovery walker is OUT OF SCOPE for this milestone. It keeps its own independent walker pass, unchanged. Only package-db readers (per FR-001 and the walker call-site inventory in `waybill-cli/src/scan_fs/package_db/`) migrate. Rationale: m133's callback shape (SHA-256 hash + content-shape gate + package-tier occurrence-set dedupe) is materially different from manifest-file dispatch; bundling would double the milestone's blast radius and complicate the FR-006 golden-identity guarantee. Follow-on milestone.

### Key Entities

- **Reader**: An ecosystem-specific unit that today walks part or all of the scan tree looking for manifest files, then processes matches. Post-migration, a Reader declares its interest in filename patterns and receives per-file callbacks. Examples: pip, cargo, npm-outer, maven, haskell, ipk_file, pants_common, scala, erlang, rpm_file, gradle, yocto-recipe, cmake, gem, kotlin_dsl, swift, nuget, cocoapods, composer, dart, elixir, bazel, vcpkg, conan, go_binary, golang-legacy.
- **Reader-Registry**: The dispatch mechanism that maps observed files to interested Readers. Owned by the scan pipeline; populated at scan start.
- **Shared Walker**: The single-pass filesystem traversal that emits per-file dispatch events into the Reader-Registry. Runs exactly once per scan.
- **Directory-Sibling Lookup**: A helper that lets a Reader's per-file callback ask "what other files exist in this file's directory?" without triggering a fresh walk. Rationale: two-phase Readers (pip, cargo, gem, etc.) find a marker file, then read siblings.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On an ansible checkout (5,793 files, ~500 directories), `waybill sbom scan --offline --file-inventory=off` completes in ≤ 1.2 seconds wall-clock (baseline 4.10s, ≥ 3.4× improvement) on the reference dev environment (macOS APFS, warm caches, release build).
- **SC-002**: On a PyTorch checkout (21,649 files), `waybill sbom scan --offline --file-inventory=off` completes in ≤ 1.5 seconds wall-clock (baseline 4.30s, ≥ 2.8× improvement) on the reference dev environment.
- **SC-003**: On a MongoDB checkout (55,186 files), `waybill sbom scan --offline --file-inventory=off` completes in ≤ 3.0 seconds wall-clock (baseline 15.68s, ≥ 5× improvement) on the reference dev environment.
- **SC-004**: 100% of existing golden SBOM tests across the workspace produce byte-identical output post-milestone. Golden-identity is a blocker gate on every reader-migration PR.
- **SC-005**: On a synthetic 10,000-file tree with mixed manifest and non-manifest files, p95 per-file dispatch overhead through the shared registry is ≤ 100 µs. Enforced by a new regression-guard microbenchmark test.
- **SC-006**: Total pre-PR verification time (clippy + full workspace test) stays within 20% of the pre-milestone baseline. Rationale: the refactor is large; the test suite must not balloon.
- **SC-007**: Post-milestone, adding a new ecosystem reader that walks the filesystem takes fewer lines of code than adding one pre-milestone, measured against a representative sample (e.g., the m225 pants-shell reader would have needed roughly 200 fewer LOC of walker boilerplate).

## Assumptions

- The reference dev environment for SC-001 / SC-002 / SC-003 wall-clock targets is macOS APFS with warm caches on a release-mode waybill build. CI-linux perf targets are not asserted directly (Linux filesystem I/O is characteristically 2-3× faster than macOS APFS; SC-005's per-file dispatch overhead is the CI-appropriate assertion).
- The count "~28 walker-using readers" is drawn from the m664 diagnostic sample (git grep across `waybill-cli/src/scan_fs/package_db/`); the exact count may shift by ±2 as new readers land pre-merge. The migration must handle whichever readers exist at implementation time.
- The pilot readers for US1 were finalized during Phase-3 implementation audit (2026-08-21) to **5 clean-shape readers**: haskell (287 ms samples), scala (203 ms across four walker sites), erlang (151 ms across three walker sites), rpm_file (52 ms), and ipk_file (97 ms). Total legacy walker cost saved: ~790 ms. Two readers originally slated for US1 (pants_common and yocto/recipe) were deferred to US2 bundle migrations after the audit revealed structural coupling: pants_common is a shared helper for pants_go + pants_shell + pants_jvm (all US2), and yocto/recipe is coupled to layer_conf + bbappend (US2 T059). Migrating either alone leaves a majority of the ecosystem's walker cost as legacy — better to bundle. The rpm_file and ipk_file inclusions in US1 depend on the per-reader-state API extension (`ReaderRegistration.state`) that landed with Phase 2 hotfixes when the audit surfaced the need.
- The file-tier walker (m133) is treated as a separate concern from package-db readers. Its migration is deferred to a follow-on unless the plan phase surfaces a compelling reason to bundle it.
- The `discover_by_filenames` generic helper in `haskell.rs` (and similar helpers elsewhere) refactor to consume the shared registry rather than expanding independently. Rationale: this is where the biggest concentration of duplicate walking lives.
- Perf targets in SC-001 through SC-003 assume `--offline` mode. `--offline=false` runs incur network cost (deps.dev, deps.dev-graph) that is not attributable to walker cost and is unaffected by this milestone.
- No `--ecosystem` CLI flag is added by this milestone. Ecosystem-scope filtering is a separate concern that a follow-on could layer on top of the shared registry.
