# Research: Read dependency manifests in CI and docs dot-directories

Census runs, all in `kusari-sandbox/waybill-fixture-therock-sdk` with `census/dotdirs.py`:
- 44370134340 (dot-directory content);
- 44404589018 (adds the requirements-name list).

Both run on TheRock `b1cae390` with submodules, in CI only. The script was also run locally on waybill's own repository (the positive control) and on the 17 cached public-corpus trees.

## R1. How readers reach a directory: the shared walker's `descend_into` (contract C10)

**Decision:** add the listed directories to the `descend_into` of the pip, npm and CMake registrations. Build the globs case-insensitively, with the existing `globset_from_patterns_case_insensitive`. Do not change `should_skip_default_descent`.

**Rationale:**
- **What C10 already does.** The shared walker (`walk_registry/walker.rs`) checks every normally-skipped directory against each registration's `descend_into`. It enters a matching directory with dispatch restricted to the readers that asked (`compute_descend_into_scope`). The check runs in every directory, so `compiler/amd-llvm/.ci` is reached as readily as a root-level `.ci`.
- **Nested skips hold by construction (FR-002).** Inside a restricted subtree, a nested skipped directory (`.venv`, `node_modules`) is entered only if a registration *already in scope* asks for it. The intersection with the incoming scope means a nested `descend_into` cannot widen the scope.
- **Precedent.** `go_binary` and `cocoapods` use the same mechanism for `build`, `dist`, `out`, `coverage` and `venv`.
- **The file tier stays out by construction (FR-003).** It walks with its own walker and the unchanged helper. So does the binary tier (`binary/discover.rs`).

**Alternatives:**
- Change `should_skip_default_descent` itself. Rejected: about 15 walkers share it, the file tier among them, and FR-003 would then need a carve-out in each.
- A deny-list of caches. Rejected in the spec (Assumptions).

## R2. Which readers opt in

**Decision:** pip, npm and CMake.

**Rationale**, from what the census finds under the listed directories:

| Reader | Evidence |
|---|---|
| pip | 8 requirements files on TheRock |
| CMake | 4 `*.cmake` files under `compiler/amd-llvm/.github/workflows/test-suite/` on TheRock (LLVM test-suite cache scripts) |
| npm | the spec's `package.json` under `.github/actions/<name>/` case (JavaScript actions). The census found none on TheRock or the corpus, so this is a correctness case, not a measured one. |

**Not opted in:**
- **The other shared-walker readers.** No manifest type they read occurs under listed directories on TheRock or the corpus. Measured for .NET, JVM/Gradle, Cargo, Go, Ruby, conda and Conan/vcpkg names. Swift and Yocto names are not in the census's type list.
- **The readers with their own walkers** (nuget, gradle, kotlin_dsl, swift, yocto). Unchanged.

Adding a reader later is a one-line registration change.

**Docker and GitHub Actions:** no reader exists yet (#1537, #435), so the 5 Dockerfiles and 543 workflow and action files stay unread. The spec scopes them out.

## R3. The pip selector

**Finding:**
- **The selector.** `read_requirements_files` (`pip/requirements_txt.rs`) and `has_python_project_marker` (`pip/mod.rs`) accept only names starting with `requirements` and ending `.txt`.
- **The classifier.** `classify_requirements_scope` already labels `<scope>-requirements*.txt`, but no such file ever reaches it.
- **The census** (run 44404589018) found 4 requirements-shaped names on TheRock that the selector skips:
  - `compiler/amd-llvm/.ci/all_requirements.txt`;
  - `rocm-libraries/projects/composablekernel/dev-requirements.txt`;
  - `rocm-libraries/projects/miopen/dev-requirements.txt`;
  - `rocm-libraries/projects/hipblaslt/tensilelite/rocisa/tox-requirements.txt`.
  
  It found 0 in the public corpus, so no golden changes.

**Decision (spec FR-004a):** both the selector and the project-root marker also accept:
- `<scope>-requirements*.txt` for scope ∈ {`dev`, `test`, `docs`, `ci`};
- `*requirements*.txt` when the file is under a listed CI or docs directory.

**Rationale:**
- Every newly read file then gets a non-runtime label, from its name or from its directory.
- A blanket `*requirements*.txt` would read `tox-requirements.txt` as `Main` (runtime), because `tox` is not a scope word. That would label a test runner's tooling as shipped content. It stays unread, recorded as a known gap.
- The #1522 check (a file mostly not PEP 508 is not pip) applies to every newly read file.

## R4. Labelling pip files (FR-004)

**Decision:** extend `classify_requirements_scope`:
1. The immediate parent directory and the file name keep today's precedence.
2. If neither gives a scope, walk the file's ancestors. The nearest listed CI directory gives `ci`, and `.sphinx` gives `docs`.

The result flows through the existing path: `Optional` + C155 `waybill:python-req-file-scope`.

**Rationale:** C155 is a closed enum (`dev`/`test`/`docs`/`ci`) that already holds both values, so no catalog change is needed.

**How the eight files land:**
- `rocm-libraries/.github/requirements.txt`, `rocm-systems/.github/requirements.txt` and `.ci/metrics/requirements.txt`, which today would be `Main`, become `ci`.
- `.sphinx/requirements.txt` becomes `docs`.

## R5. Labelling other readers' components (FR-005, FR-006)

**Decision:** a pass after deduplication, beside spec 1533's test-scope pass. For each component with no lifecycle scope whose every evidence source file lies under a listed directory, it sets:
- `LifecycleScope::Development`;
- C62 `waybill:lifecycle-scope-derivation` = `ci-directory` (or `docs-directory` for `.sphinx`).

C62 is an open enum, so this adds two values and no new row. The catalog's value list is updated and the C62 parity extractor is unchanged.

**FR-006:**
- A component with any source file outside the listed directories keeps the scope it would have had. The deduplicator already unions source files (milestone 148).
- A pip component already carries a scope from R4, so the pass does not touch it.

**Rationale:**
- **Development, not Optional.** CI and docs tooling is development-time, and SPDX 3's `LifecycleScopeType` has `development`, where `Optional` would say the dependency is declared optional.
- **The pip mismatch is noted, not fixed.** pip's existing `Optional` for `ci` and `docs` files is a separate milestone's decision (670). Changing it is out of scope.

## R8. A runtime component keeps no CI label (found in the T001 trace)

**Finding** (`measurements/trace.md` §c): when a pip package is declared in both a scoped requirements file and an unscoped one, the deduplicator keeps the runtime scope (CycloneDX `required`), but C155 from the scoped file survives (`ci`).

**Decision:** the post-dedup pass (`ci_scope::apply`) removes C155 from any component whose final lifecycle scope is runtime.

**Rationale:** FR-006 requires the non-CI declaration's scope to be the component's scope. A label contradicting that scope defeats consumers who select by label.

**Effect on existing output:** this changes existing output for `requirements-<scope>.txt` beside `requirements.txt` with shared packages, which is traced in the public corpus (T022).

## R6. Performance

**Expectation:** TheRock's listed directories hold about 1,000 files (930 + 46 + 4) out of 335,551 walked. Only the opted-in readers' patterns are matched there. The remaining cost is reading 8 requirements files and 4 CMake files.

**Prediction**, to be verified by `measure-source` interleaved against `main`: within noise, under SC-005's 1.05×.

## R7. Walker-audit gate

No new walk function is added: `descend_into` is a registration field. `walk.audit-allowlist` is unchanged. The local reproduction of the audit grep runs before the PR (`feedback_walker_audit_local_check`).
