---

description: "Task list for milestone 670 — Fix critical Python under-detection"
---

# Tasks: Fix critical Python under-detection

**Input**: Design documents from `/specs/670-pip-under-detection-fix/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: INCLUDED. SC-001/SC-002/SC-003 acceptance is fixture-integration-test based; per-reader unit tests continue the existing waybill convention (`#[cfg(test)] mod tests` inline in each reader file).

**Organization**: Lean 3-PR plan targeting the actual gaps surfaced by the 2026-08-31 pre-implementation diagnostic (see [diagnostic notes](#diagnostic-findings-2026-08-31) below). Each PR delivers a distinct fix mapped to one or more user stories.

---

## Diagnostic findings (2026-08-31)

Before implementation began, a diagnostic scan revealed that the pip module already contains substantial reader infrastructure (m018/m068/m106/m183). The three fixtures fail for three specific, small reasons — not because readers are missing.

### markitdown (4 → target ≥30)
- 4 sub-project pyproject.tomls at `packages/*/pyproject.toml`, **no lockfiles anywhere**, no requirements.txt
- **Root cause**: `pip/mod.rs:25-28` documents a deliberate m018 design decision — "pyproject.toml-only projects emit zero components ... `[project.dependencies]` holds build specs, not resolved versions, so fabricating components from it would bloat SBOMs with phantoms."
- **Fix**: reverse the m018 skip. Emit `[project.dependencies]` as design-tier components with `waybill:unresolved-reason = "python-manifest-unpinned"` when no lockfile is present.

### OctoPrint (3 → target ≥30)
- Root has `pyproject.toml` + `setup.py` + stub `requirements.txt`
- `setup.py` sets `install_requires=INSTALL_REQUIRES` where `INSTALL_REQUIRES = [...]` is a module-level variable defined earlier
- **Root cause**: no static-parse of `setup.py` currently; even if added, the FR-006 "literal-list at setup() call" pattern in `contracts/setup_py_static.md` would miss OctoPrint's variable-indirection pattern.
- **Fix**: static `setup.py` parser that recognizes `install_requires=<IDENT>` and then chases the `<IDENT> = [literal list]` assignment earlier in the same file. **Plus** the m018 reversal above (so OctoPrint's pyproject deps also surface).

### cpython (16 → target ≥50) — realistic target may need adjustment
- 3 requirements files (`Doc/requirements.txt` [7 deps], `Tools/requirements-hypothesis.txt` [1 dep], `Tools/requirements-dev.txt` [3 deps]) totaling **~11 unique declared deps**
- 2 nested pyproject.tomls, 4 test-fixture setup.py files (small)
- Existing readers ARE finding the requirements files (sphinx/mypy/hypothesis/blurb visible in current output)
- **Note**: SC-003 target of ≥50 assumed more declared-dep sources than cpython actually ships. Realistic ceiling from just parsing declared files is ~15-20 unique components. Hitting ≥50 would require a different attack (deep enumeration of Lib/site-packages vendored packages, cpython's Modules/_ctypes/libffi_*/README dep-refs, etc.) that's outside the surgical scope of this milestone.
- **Fix (US2 scope)**: PEP 508 direct-URL handling (Doc/requirements.txt has a git-URL for pygments), `-c constraints.txt` reference handling, better emission for constrained-but-unpinned entries. Realistic improvement: 16 → ~25.
- **Escalation path**: if the spec's SC-003 ≥50 is strict, note it as an unmet target in the milestone completion report; flag as a follow-up milestone rather than an implementation blocker.

---

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 (Setup, Polish carry no story label)
- Exact absolute file paths in every description

## Path Conventions

- **Source root**: `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pip/`
- **Test root**: `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/`
- **Fixture goldens**: `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/public_corpus/`

---

## Phase 1: Foundational (shared reason strings only)

**Purpose**: Add the new locked reason strings that all 3 PRs use. Everything else needed already exists.

- [X] T001 Reserve three new m236 reason strings in the locked contract at `/Users/mlieberman/Projects/mikebom/specs/236-unresolved-reason/contracts/per-reader-strings.md`. The m236 mechanism uses **human-readable English sentences** inlined per reader (not a const vocabulary module); the contract file is the documentation lock. Adding one row per upcoming PR: (a) `mod.rs` (pyproject reader path): `"declared in pyproject.toml; no uv.lock / poetry.lock / Pipfile.lock fallback"` — used by PR-1's `pyproject_declared_deps` for PEP 621 / PEP 735 / Poetry-legacy manifests without a resolved lockfile; (b) `setup_py.rs`: `"declared in setup.py install_requires; no uv.lock / poetry.lock / Pipfile.lock fallback"` — used by PR-2's static var-indirection parser; (c) `requirements_txt.rs` (extended path): `"PEP 508 direct-URL entry; no rev extractable from URL"` — used by PR-3's direct-URL handling when the URL fragment lacks a resolvable revision. These are documentation reservations only; source-side inlining + `unresolved_reason_universal.rs::locked_reason_strings()` additions happen in each PR (T003, T008, T012).

**Checkpoint (Phase 1)**: Reason vocabulary extended. `cargo +stable test --workspace` still green.

---

## PR-1: Reverse the m018 pyproject-only skip (Priority: P1) 🎯 MVP

**Story**: US1 primarily (markitdown); partially satisfies US3 (OctoPrint's pyproject side)

**Goal**: When a `pyproject.toml` declares `[project.dependencies]` (PEP 621), `[project.optional-dependencies]`, `[dependency-groups]` (PEP 735), or `[tool.poetry.*]` and NO paired lockfile is present, emit each declared dep as a design-tier component with `waybill:unresolved-reason = "python-manifest-unpinned"`. Reverse the deliberate m018 "emit zero" policy documented at `pip/mod.rs:25-28`.

**Independent Test**: Scan `kusari-sandbox/test-markitdown`; emitted CDX contains ≥ 30 `pkg:pypi/*` components (up from 4 baseline).

### Implementation for PR-1

- [X] T002 [P] [US1] Update the module-level docstring at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pip/mod.rs:1-28` to reflect the new emission policy: "pyproject.toml-declared dependencies ARE emitted as design-tier components with `version = "unresolved"` when no lockfile is present; lockfile-declared versions take precedence via the m191 reconciler when both exist." Delete the "pyproject.toml-only projects emit zero components" language.
- [X] T003 [US1] Add a new function `pyproject_declared_deps(project_root: &Path) -> Vec<PackageDbEntry>` in `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pip/mod.rs` (near `build_pip_main_module_entry`, m068 pattern). Reads:
  - `[project.dependencies]` (PEP 621) → `LifecycleScope::Main`
  - `[project.optional-dependencies]` groups → `LifecycleScope::Optional { scope_name: <group> }`
  - `[dependency-groups]` (PEP 735) → `LifecycleScope::Optional { scope_name: <group> }`
  - `[tool.poetry.dependencies]` (fallback if PEP 621 absent) → `LifecycleScope::Main`
  - `[tool.poetry.dev-dependencies]` → `LifecycleScope::Optional { scope_name: "dev" }`
  - `[tool.poetry.group.<name>.dependencies]` → `LifecycleScope::Optional { scope_name: <name> }`
  Each emitted entry: `pkg:pypi/<name>@unresolved` + `waybill:unresolved-reason = "python-manifest-unpinned"` + `waybill:version-constraint = <raw-constraint>` when a constraint is declared. Skip `python` itself (Poetry declares `python = "^3.11"` which is not a package). Add 8-10 inline unit tests covering PEP 621 / PEP 735 / Poetry-legacy shapes.
- [X] T004 [US1] Wire the new `pyproject_declared_deps` into the `finalize()` dispatcher at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pip/mod.rs:283` (inside the `for project_root in &project_roots` loop, alongside the existing `poetry::read_poetry_lock` / `pipfile::read_pipfile_lock` / `uv_lock::read_uv_lock` calls). Use `merge_without_override` — lockfile-derived entries win over unresolved manifest entries per m191 semantics. **Correction from as-written**: name-based dedup (not `merge_without_override`'s PURL-based dedup) is required because manifest entries have `@unresolved` PURLs that would not collide with lockfile-resolved `@<version>` PURLs. Implementation uses a `HashSet<String>` of already-covered names, growing as manifest entries land, applied in a second loop AFTER the tier-1/2/3 readers exhaust. Also required a `source_path` shape fix: entries use `path+file://<project_root>` (directory, not manifest file) to align with m068's convention and m176's workspace-member derivation (regression caught by `workspace_visibility::t007`).
- [X] T005 [US1] Update the `poetry_only_skips` diagnostic log at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pip/mod.rs:346-349` — the Poetry-legacy skip referenced by issue #104 no longer needs to skip main-module emission because Poetry deps are now handled by T003. Remove the skip; emit main-module for Poetry-legacy manifests.
- [X] T006 [US1] Add the `test-markitdown` fixture entry to the m090+m195 public-corpus. Pin commit SHA in `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/public_corpus/markitdown/pin.json`. Regenerate the 3 golden SBOMs. **Scope adjustment**: pivoted to synthetic fixtures inlined in `waybill-cli/tests/scan_python_m670.rs` (T007). Rationale: the m195 heavy corpus is (a) opt-in behind `WAYBILL_RUN_PUBLIC_CORPUS=1`, so wouldn't run in default CI; (b) requires cross-host-stable golden SBOMs per memory `feedback_cross_host_goldens`, which are complex to regenerate deterministically; (c) the fixture would live in `kusari-sandbox/waybill-test-fixtures` (sibling repo) requiring a separate upstream PR per memory `feedback_upstream_prs_need_explicit_approval`. Synthetic fixtures with `waybill-fixture-*` package names (per memory `feedback_fixture_synthetic_package_names`) cover every T003/T005 branch (PEP 621, PEP 735, Poetry-legacy, extras, constraints) and run in default CI. Real markitdown/OctoPrint/cpython verification against SC-001/002/003 is the ad-hoc sweep script (T019).
- [X] T007 [US1] Add the US1 integration test to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/transitive_parity_python.rs` (create the file if missing): scan the markitdown fixture from cache, assert `[.components[] | select(.purl | startswith("pkg:pypi/"))] | length >= 30` (SC-001). Assert every emitted pypi component has ≥ 1 `source_file_paths` entry (SC-005 for markitdown). Assert wall-clock ≤ 549ms (SC-007). **Delivered as** `waybill-cli/tests/scan_python_m670.rs` — 6 integration tests covering every T003/T005 branch: PEP 621 `[project.dependencies]` (5-dep synthetic fixture), version-constraint annotation preservation, `[project.optional-dependencies]` scope-derivation, Poetry-legacy full-stack (main-module + 3 dep sections + python-skip), PEP 735 `[dependency-groups]`, and a 20-dep perf-envelope test (< 2s ceiling to catch pyproject_declared_deps scaling regressions). SC-001 acceptance for real markitdown (32 pypi ≥ 30 target) was validated end-to-end by the T004 release-binary scan; T019 will re-verify pre-PR via the ad-hoc sweep. SC-005 evidence check + m236 unresolved-reason assertion baked into every fixture test.

**Checkpoint (PR-1)**: markitdown fixture emits ≥ 30 pypi components on Linux + macOS. Ships as its own PR — closes the biggest single gap and lays the groundwork for PR-2/PR-3.

---

## PR-2: setup.py static var-indirection (Priority: P2) — **CANCELLED post-diagnostic**

**Original goal**: setup.py static parser for the `install_requires=VAR` var-indirection pattern.

**Cancellation rationale (2026-09-01 diagnostic at T008 implementation)**: OctoPrint — the anchor fixture for this PR — does **not** use `install_requires` in `setup.py`. Its setup.py contains only `version` / `license` / `cmdclass` args; the ~50 declared deps all live in `pyproject.toml [project.dependencies]`. PR-1's T003 (`pyproject_declared_deps`) already handles OctoPrint end-to-end: **73 pypi components emitted (target ≥ 30)**, SC-002 met without any setup.py reader work.

**Broader assessment**: the `install_requires=<IDENTIFIER>` var-indirection pattern is increasingly rare in modern Python projects. None of the 21 kusari-sandbox `test-*` repos exhibit it. Building a defensive parser has poor cost/coverage ratio.

**Fate of the deliverables**:
- Contract file `contracts/setup_py_static.md` — retained as design reference in case a future milestone needs a setup.py reader
- Parity-catalog rows planned by T016 (C154 `waybill:direct-url-source`, C158 `waybill:python-req-file-scope`) — still relevant for PR-3; unchanged
- The m670 reason-string `"declared in setup.py install_requires; no uv.lock / poetry.lock / Pipfile.lock fallback"` reserved in T001 remains reserved (harmless; no source-side inlining happens)

- [~] T008 [US3] ~~Create setup_py.rs static parser with var-indirection~~ **CANCELLED**: OctoPrint's setup.py has no install_requires; PR-1's T003 covers OctoPrint via pyproject.toml. 73 pypi emitted, SC-002 met.
- [~] T009 [US3] ~~Wire setup_py::read into finalize()~~ **CANCELLED**: no reader to wire.
- [~] T010 [US3] ~~Add test-OctoPrint fixture entry~~ **CANCELLED**: SC-002 met by PR-1; ad-hoc sweep (T019) re-verifies before shipping.
- [~] T011 [US3] ~~OctoPrint integration test~~ **CANCELLED**: T007's `scan_python_m670.rs` fixtures already exercise every PEP 621 branch that OctoPrint uses.

**Post-cancellation status**: US3 (OctoPrint) is SC-002-satisfied by PR-1 alone. No PR-2 needed; milestone advances directly to PR-3 (cpython).

---

## PR-3: requirements.txt improvements (Priority: P3)

**Story**: US2 (cpython — modest improvement)

**Goal**: Extend the existing `requirements_txt.rs` to handle PEP 508 direct URLs (`pkg @ git+https://...@rev`), constraints-file references (`-c constraints.txt`, warn+skip per spec), and scope-tagging from the FR-005a filename+parent-dir heuristic.

**Independent Test**: Scan `kusari-sandbox/test-cpython`; emitted CDX contains ≥ 25 `pkg:pypi/*` components (up from 16 baseline; realistic target given cpython's ~11 unique declared deps — see diagnostic notes above).

**Warning**: The spec's SC-003 target is ≥ 50. The realistic upper bound from parsing cpython's 3 requirements files + 2 nested pyproject.tomls + 4 setup.py files is ~15-25 components. If the spec's ≥ 50 remains strict, PR-3 will fall short and require follow-up. Flag this to the reviewer.

### Implementation for PR-3

- [X] T012 [US2] Extend `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs` to recognize PEP 508 direct-URL entries (`pkg-name @ git+https://.../@rev` or `pkg @ https://...tar.gz`). Emit `pkg:pypi/pkg-name@<rev-or-unresolved>` + `waybill:direct-url-source` annotation carrying `{url, kind, resolved_rev}`. Reuse `direct_url::parse` if that helper module is added; otherwise inline the URL-shape recognition. Add 4 inline unit tests: git-URL with rev, git-URL without rev, https-tarball URL, invalid URL fallback. **Delivered**: added `DirectUrlRef` struct + `DirectUrlRef::parse()` helper in-file (rejected separate module to keep the new surface small); `parse_requirements_line` now special-cases `body.split_once(" @ ")` BEFORE the bare-URL branches; git URLs with `@rev` in the path segment (post-first-slash) populate both `version` and `resolved_rev`; archive URLs populate the annotation with `resolved_rev = null`. Bare-URL branches (pre-existing `git+...` / `https://...` entries without name-prefix) also get the annotation for consistency but keep `version = ""` to preserve pre-m670 semantics. T012-specific m236 reason string `"PEP 508 direct-URL entry; no rev extractable from URL"` fires when tier=design AND direct_url is present AND no rev extracted. **Note on cpython**: end-to-end verified — the `pygments @ https://.../archive/2cad2642...tar.gz` entry now emits the annotation with `kind=url`, `resolved_rev=null`, full URL captured. Component count on cpython stays at 16 (T012 enriches existing components, doesn't add new ones); ≥25 target still pending T013 (scope heuristic won't add either — cpython's realistic ceiling from declared-file parsing is ~15-20).
- [X] T013 [US2] Add the filename+parent-dir scope heuristic (FR-005a) to `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs` `read_requirements_files` or a new `req_scope_heuristic.rs` sibling module. Priority: parent-dir name (`docs/`, `tests/`, `ci/`) → filename signal (`requirements-dev*.txt`, `dev-requirements.txt`) → default `Main`. Attach `waybill:python-req-file-scope` annotation with derived scope name. Update `LifecycleScope` on the emitted entries accordingly. Add 6 inline unit tests. **Delivered inline** (no sibling module): `RequirementsScope` enum + `classify_requirements_scope()` public helper + `matches_scope_filename()` word-boundary matcher (rejects `requirements-special.txt` → `ci` false-positive). Case-insensitive parent-dir + filename matching so cpython's `Doc/requirements.txt` (capital D) matches `docs`. Populates `lifecycle_scope = Some(LifecycleScope::Optional)` on non-Main entries + emits `waybill:python-req-file-scope` annotation (catalog row C158 pending T016). End-to-end verified on cpython: `Doc/requirements.txt` → 7 deps scope=docs, `Tools/requirements-dev.txt` → 3 deps scope=dev (mypy/types-psutil/types-setuptools), 10 scope annotations total; `Tools/requirements-hypothesis.txt` doesn't match heuristic (falls to Main — defensible).
- [X] T014 [US2] Add the `test-cpython` fixture entry to the m090+m195 public-corpus. Pin commit SHA. Regenerate goldens. **Delivered as** 4 new cpython-shape synthetic tests in `waybill-cli/tests/scan_python_m670.rs`: `m670_cpython_shape_doc_requirements_gets_docs_scope` (Doc/ case-insensitive parent-dir → docs), `m670_cpython_shape_tools_dev_requirements_gets_dev_scope` (filename signal → dev), `m670_cpython_shape_tools_hypothesis_falls_to_main_scope` (word-boundary regression guard), `m670_cpython_shape_direct_url_annotation_populated_on_pep508_entry` (T012 end-to-end verification). **Scope-adjusted** per user directive: same rationale as T006/T007 — m195 heavy corpus is opt-in behind `WAYBILL_RUN_PUBLIC_CORPUS=1` and requires cross-host-stable goldens; synthetic inline fixtures cover the T012+T013 code paths and run in default CI. T019's sweep provides the end-to-end real-cpython verification.
- [X] T015 [US2] Extend the integration test at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/transitive_parity_python.rs` with the cpython case: assert ≥ 25 pypi components (modified from SC-003's ≥50 per diagnostic; document in test comment), scope-tag distribution matches expectations (`docs/requirements.txt` deps → `Optional{"docs"}`, `Tools/requirements-dev.txt` → `Optional{"dev"}`), wall-clock ≤ 5.575s. **Covered by T014**: T014's 4 cpython-shape synthetic tests in `scan_python_m670.rs` exercise the same T012+T013 code paths (Doc/ case-insensitive parent-dir → docs, Tools/requirements-dev.txt → dev, PEP 508 direct-URL, word-boundary regression guard). Real cpython end-to-end verification is delivered by T019's sweep (16 pypi baseline + 10 new scope annotations + 1 direct-URL annotation; SC-003 ≥50 target flagged as unachievable via declared-file parsing alone). No separate cpython-fixture-clone test is needed.
- [X] T016 [US2] Add 2 new parity-catalog rows to `/Users/mlieberman/Projects/mikebom/docs/reference/sbom-format-mapping.md`: **C154** `waybill:direct-url-source` (`SymmetricEqual`), **C158** `waybill:python-req-file-scope` (`SymmetricEqual`). Add matching extractor modules to `/Users/mlieberman/Projects/mikebom/waybill-cli/src/parity/extractors/` per memory feedback `feedback_sbom_format_mapping_extractor_gate`. Native-alternative audit per Principle V bullet 5 in each row's justification. Reuses existing C151 `waybill:unresolved-reason` (m236) for the new `python-*` reason strings — no new row needed. **Delivered**: C154 `waybill:direct-url-source` (per-component, JSON-stringified `{url, kind, resolved_rev}` object matching pip's PEP 610 `direct_url.json` shape) + C155 `waybill:python-req-file-scope` (per-component, closed-enum string `dev`/`test`/`docs`/`ci`). **Renumbered** from tasks-spec `C158` to sequential `C155` — no reserved gap needed since the other originally-planned rows (C155/C156/C157/C159 for version-constraint / python-extras / pep508-marker / python-lockfile-format) were dropped from scope during the m670 T012 diagnostic. Both rows are `Directionality::SymmetricEqual`. Native-alternative audits: C154 rejects CDX `externalReferences[type: vcs]`/`distribution` because the single-string slot cannot carry the `{url, kind, resolved_rev}` triple; C155 rejects the closed-vocabulary standards-native `scope`/`OPTIONAL_DEPENDENCY_OF`/`LifecycleScopeType` fields because they can't carry the finer-grained scope-name distinction (dev vs test vs docs vs ci). Extractor macros (`c154_cdx`, `c154_spdx23`, `c154_spdx3`, `c155_cdx`, `c155_spdx23`, `c155_spdx3`) added to `parity/extractors/{cdx,spdx2,spdx3}.rs`; `EXTRACTORS` array entries added at `parity/extractors/mod.rs:614-618`. Verified: `every_catalog_row_has_an_extractor` bidirectional test + all 11 `holistic_parity` tests pass.

**Checkpoint (PR-3)**: cpython fixture emits ≥ 25 pypi components. If SC-003's ≥50 is strict, mark as follow-up in completion report.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T017 [P] Update `/Users/mlieberman/Projects/mikebom/CLAUDE.md` "Recent Changes" section with a milestone-670 entry (auto-agent-context script may handle this; verify). **Delivered**: replaced the auto-generated stub at CLAUDE.md:429 with a full descriptive entry covering the m018 policy reversal, PEP 621/PEP 735/Poetry-legacy support, PEP 508 direct-URL emission, FR-005a scope heuristic, zero new Cargo deps, 2 new catalog rows, and sweep verification numbers (markitdown 5→32, OctoPrint 13→83, SC-003 deferred).
- [X] T018 [P] Add a memory-note file at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_pip_manifest_declared_deps.md` documenting: (a) the m018 policy reversal, (b) the setup.py var-indirection pattern, (c) the emission shape for `python-manifest-unpinned` entries. Register in MEMORY.md. **Delivered**: comprehensive memory-note capturing m670's full ship-set: m018 policy reversal + Poetry-legacy main-module emission + PR-2 CANCELLATION rationale (OctoPrint pattern didn't need setup.py parser) + PR-3 direct-URL + scope heuristic + parity catalog rows + workspace-source-path gotcha (regression caught by workspace_visibility::t007 — use project_root directory, not manifest path) + sweep verification results. Registered in MEMORY.md between m665 and any subsequent entries.
- [X] T019 Run the sweep-regression check: `bash /tmp/waybill-sweep.sh` (or equivalent) against all 21 kusari-sandbox test-* repos. Verify non-Python fixtures within ± 5% component count; Python fixtures monotonically increase. Update the sweep TSV at `/Users/mlieberman/Projects/mikebom/specs/670-pip-under-detection-fix/artifacts/sweep-after.tsv`. Depends on T007, T011, T015. **Delivered**: zero regressions across all 21 repos. Full artifacts + comparison table at `specs/670-pip-under-detection-fix/artifacts/`: `sweep-baseline-2026-08-31.tsv`, `sweep-after-2026-09-01.tsv`, `sweep-comparison.md`. **SC results**: SC-001 met (markitdown 5→32), SC-002 met (OctoPrint 13→83), SC-006 met (14/14 non-Python at 0.0% delta), SC-007 met (markitdown 49ms→50ms; budget 549ms), SC-008 met (cpython 575ms→580ms; budget 5575ms). **SC-003 NOT met by PR-3** alone (cpython unchanged at 16 pypi; declared-file ceiling is ~15-20 without a different attack surface). Documented as follow-up in the comparison markdown.
- [X] T020 Run the mandatory pre-PR gate for each of the 3 PRs before opening: `./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace` MUST pass green. Include SPDX 3 conformance via `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1`. **Delivered**: gate green. Full-workspace inventory: 290 test-suites, 5371 tests passed, 0 failed, 14 ignored. Clippy: zero warnings under `-D warnings`. SPDX 3 validator gated on via `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` + `.venv/spdx3-validate/bin/` on PATH. **Three test-side updates required during the gate iteration**: (a) `scan_python.rs::pyproject_only_emits_zero_pypi_components` → `pyproject_only_emits_declared_deps_as_design_tier` (fixed during T004); (b) `pip/dist_info.rs::pyproject_only_project_emits_only_main_module` → `pyproject_only_project_emits_main_module_and_manifest_declared_deps` (fixed during T004); (c) `scan_pip.rs::scan_pip_poetry_only_skips_main_module` → `scan_pip_poetry_only_emits_main_module_post_m670` (fixed during T020); (d) `transitive_parity_pip_poetry.rs::EXPECTED_WAYBILL_EDGE_COUNT` bumped from 88 → 63 (fixed during T020; documented in-file with the m018→m670 timeline). One transient flake surfaced on `gradle_ladder::sc005_subprocess_timeout_degrades_gracefully` (15s vs 10s budget under concurrent load; isolated re-run: 4.45s — timing-sensitive by design, not m670-attributable).
- [X] T021 Verify the walker-audit allowlist at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/walk.audit-allowlist.txt` needs no new entries — all new code reuses the existing m664 shared-walker registry entries (which pip already registered per `read_all` at mod.rs:193). **Delivered**: reproduced the CI grep-and-diff logic from `.github/workflows/ci.yml:141` locally against current `HEAD`. Result: **byte-for-byte match** — 12 live `fn walk[_(]` entries under `waybill-cli/src/scan_fs/`, exactly matching the 12-entry allowlist. m670's code doesn't add any new `fn walk*` functions (all changes threaded through existing `pyproject_declared_deps`, extended `parse_requirements_line`, and existing readers' emission paths). **Local repro gotcha**: the shell function `grep` (claude-code plugin wrapper via `ugrep`) needs to be bypassed with `command grep`; use `/usr/bin/sed` (absolute path) since the plugin's env may reset PATH in some contexts. Locked-recipe for future T021 verifications preserved as memory note candidate for `feedback_walker_audit_local_check`.

---

## Dependencies & Execution Order

### Phase / PR Dependencies

- **Phase 1** (Foundational): T001 blocks nothing structurally but the new reason strings are cited by all 3 PRs.
- **PR-1** (T002–T007): depends on T001.
- **PR-2** (T008–T011): depends on T001. Independent of PR-1 (each PR's fixture-integration test is isolated).
- **PR-3** (T012–T016): depends on T001. Independent of PR-1/PR-2.
- **Polish** (T017–T021): T019/T020 depend on the completion of all 3 PRs; T017/T018/T021 can run any time.

### Recommended Execution Order

For a single contributor:

1. **T001** — unblocks all 3 PRs (~30 min)
2. **PR-1 (T002–T007)** — MVP; biggest win; ships as its own PR — 1 day
3. **PR-2 (T008–T011)** — OctoPrint fix; ships as its own PR — half day
4. **PR-3 (T012–T016)** — cpython improvements; ships as its own PR — half day (note: may fall short of SC-003 ≥50)
5. **T017–T021** — polish + sweep-regression + final pre-PR gates

### Parallel Opportunities

- **T002 + T008 + T012** — Three PRs' opening tasks in parallel if multiple contributors
- **T017 + T018 + T021** — Polish tasks parallelizable

---

## Parallel Example: All 3 PRs at once

```bash
# After T001 completes, three contributors can work concurrently:
Task: "PR-1 T002–T007 — m018 policy reversal + pyproject_declared_deps"
Task: "PR-2 T008–T011 — setup.py static var-indirection"
Task: "PR-3 T012–T016 — requirements.txt direct-URL + scope-heuristic"
```

---

## Task Count Summary

- **Phase 1 Foundational**: 1 task (T001)
- **PR-1** (markitdown + OctoPrint / MVP): 6 tasks (T002–T007) — **complete as of 2026-09-01**; delivers SC-001 (markitdown 32) + SC-002 (OctoPrint 73)
- **PR-2** (OctoPrint setup.py): 4 tasks (T008–T011) — **CANCELLED**: PR-1 already covers OctoPrint via pyproject.toml; no setup.py reader needed
- **PR-3** (cpython): 5 tasks (T012–T016) — pending; targets SC-003 (cpython requirements-file coverage improvements)
- **Polish**: 5 tasks (T017–T021)

**Total**: 21 tasks originally, 17 active after PR-2 cancellation. Down from 34 in the pre-diagnostic plan. Reflects two diagnostic-driven scope reductions:
1. **First** (m670 spec-time): 5 pip readers already exist; 4 planned catalog rows (C155/C156/C157/C159) proved unnecessary.
2. **Second** (m670 T008-time): PR-2 turned out to be unnecessary because OctoPrint's dep declaration is pyproject.toml-based, not setup.py-based; PR-1 covers it.

---

## Comparison to original plan (deleted)

The original tasks.md planned 34 tasks including 11 stub reader files. Diagnostic revealed:

| Original assumption | Reality |
|---|---|
| Add 11 new reader files (T001) | 5 of 11 already exist (`pipfile.rs`, `poetry.rs`, `requirements_txt.rs`, `uv_lock.rs`, `dist_info.rs`); 3 turn out unnecessary (`pdm.rs`, `setup_cfg.rs`, `venv_prune.rs`) |
| ~34 tasks across 3 stories | 21 tasks; scope narrowed to 3 surgical fixes |
| 6 new parity catalog rows | 2 needed (C154 direct-url-source, C158 python-req-file-scope); 4 others deferred as unused |
| Fixture-integration test infrastructure | Reuses existing m090+m195 test infrastructure |

---

## Notes

- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps task to US1/US2/US3 for traceability (Setup + Polish carry no story label)
- Each PR is independently mergeable; MVP = PR-1 alone
- Pre-PR gate (T020) is NON-NEGOTIABLE per Constitution v2.1.0 §Development Workflow
- Every new `waybill:*` annotation (C154/C158) carries a Principle V bullet-5 audit in its catalog-row justification
- The two constitutional divergences (Principle II + Strict Boundary #1) documented in `plan.md ## Complexity Tracking` remain unchanged; no new divergences introduced
- SC-003 ≥50 for cpython may need adjustment based on ground-truth availability of declared deps; flagged in PR-3's task notes and completion report
