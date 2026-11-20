# Research: A component for each CMake project the scanned tree builds

All code references are to `main` at `786091a8`. "Measured" means it was run. Anything else is labelled as reading of the code or as a prediction.

## R1. Where project declarations are read today

**Finding (code).** `cmake/fetches.rs::read_declarations` walks every listfile's commands once (spec 1614 R12). For `project`, it inserts the lowercased first argument into `Declarations.projects: BTreeSet<String>`, unless that argument contains `${` (`fetches.rs:250-254`). `cmake.rs:326` unions that set with config-template and installed-config stems into `FindCoverage.built_in_tree`. `cmake_search_pair::pair` (`:141`) then drops every search whose name is in the set, and counts it in C237 `in-tree`.

**Decision.** Replace the name set with a list of project declarations, built in the same loop: name, version argument, declaring file, and that file's `Vars`. `Vars` is the same-file literal `set()` map the loop already builds for fetch declarations (`fetches.rs:216`, `type Vars` at `:340`). The CMake reader emits one `PackageDbEntry` per distinct (lowercased name, resolved version). `built_in_tree` keeps the names, unchanged, for the config-template and installed-config cases (R6).

**Alternatives.**
- A second pass over the listfiles: rejected. Spec 1614 R12 tokenises each file once, measured at 6,000+ files on TheRock.
- Regex over raw text: rejected. It would disagree with the tokeniser on comments and multi-line calls.

## R2. Root selection (FR-007, US3, SC-008) — refines Clarification Q1

**Finding (code, `generate/root_selector.rs::select_root`).** The ladder:
1. override;
2. explicit image;
3. Nix evaluated package;
4. workspace root at the scan root;
5. **exactly one main module → that module, recording no heuristic**;
6. no main module → Maven coordinate or synthetic `<dir>@0.0.0`;
7. two or more → `RepoRoot` (one main module whose manifest is at the scan root), then `EcosystemPriority` (`golang, cargo, maven, npm, pip, gem, generic`), then `LongestCommonPrefix`, then the fallbacks.

`waybill:is-workspace-root` is set on any main module whose manifest directory is the scan root (`scan_fs/mod.rs::tag_main_modules_with_workspace_root`).

**Consequence (from the code, not run).** Take a repository with one Python main module, which takes rung 5, and CMake projects in subdirectories. If those projects joined the candidate set, the count would exceed one. Rung 7's `RepoRoot` would then pick the same Python module, but record the heuristic `repo-root`, so the document would change. If the Python module is not at the scan root and a CMake project is, the root itself would change. The clarification's "same directory" wording does not prevent either.

**Decision.** CMake project components are **root candidates only when no main module of any other ecosystem exists**. In code, `select_root` drops CMake-project main modules from `main_modules` whenever any other main module is present. This is "lowest precedence" applied to the whole candidate set. It makes every scan that has a non-CMake main module byte-identical in root selection (SC-008). A CMake-only repository with a top-level project takes rung 5 (one project) or `RepoRoot` (several, one at the root), so US3 holds.

**Recorded as a spec amendment** under Clarifications: a planning refinement of Q1's answer, not a reversal.

**Alternatives.**
- Same-directory precedence only: rejected, for the consequence above.
- Excluding CMake projects from root selection entirely: rejected. It contradicts Q1=A and US3.

## R3. Ownership precedence (FR-007)

**Finding (code, `scan_fs/declared_attach.rs::OwnerIndex::build`).** One owner per directory, by rank: a PyPI main module, then any other main module (lowest PURL), then a git submodule. `owner_of` walks up from a file's directory to the scan root.

**Decision.** Add `RANK_CMAKE_PROJECT` between other main modules and submodules. A CMake project owns its directory unless another ecosystem's main module is in the same directory. For example, pytorch's root has `pyproject.toml` beside `project(Torch)`, and the Python module keeps the root. It outranks a submodule in the same directory: the project is the thing built there, and the submodule is the checkout that contains it.

## R4. Edges and scope (FR-004, FR-006) — refines FR-004

**Finding (code, `cmake_search_pair.rs`).** `pair` turns each search paired with an installed record or a submodule into `Pairing`s. `attach` emits `owner DependsOn target`, or a root anchor when no project owns the file. `record_scopes` (spec 1543, 1614, 1617) decides the target's scope:
- a record that ships a library file goes to `conditions_only` (C236 only);
- a non-shipping record searched only conditionally becomes Optional;
- one searched only from test contexts becomes Test.

**Decision.**
- **Pairing.** A search whose name is an in-tree project's pairs with that project's component (`Pairing { installed: false, project: true }`), and `attach` emits the edge.
- **Scope**, following spec 1617's rule that what the tree provides is not lowered by when a consumer asks for it:
  - a project's scope comes from its directory (FR-015);
  - conditional searches add only C236 (the `conditions_only` path);
  - a project with no directory-derived scope, found only by test-context searches, is Test, with that derivation (spec 1617 R4). For example, gtest under `third_party/` searched only from `tests/`.
- **Enclosing edge (FR-006).** A project that no search targets gets an edge from the owner of its directory's *parent* (`OwnerIndex::owner_of` starting one level up, so it never finds itself), or a root anchor.
- **Projects at a submodule root.** A project whose directory is a submodule's root is enclosed by that submodule: the submodule owns the same directory at a lower rank, and the walk looks past the project itself.

**Recorded as a spec amendment:** FR-004's "C122 … that a search of the same shape carries" becomes "C236 and, for test-only searches, Test with its C62 derivation". Searches never give a project C122.

## R5. Identity collisions (FR-005)

**Decision (code reading).**
- **Order.** Spec 1614's order is extended with in-tree projects last, so `pair` checks fetch, then installed record, then submodule, then project.
- **Collisions.**
  - **Fetch:** a project whose lowercased name equals a fetch's declared name is not emitted; its `CMakeLists.txt` joins the fetch's evidence.
  - **Installed record or submodule:** the same rule applies, matching names exactly as `pair` does (the record's name, or the submodule directory's basename).
- **Duplicate names.** Two declarations of one name with the same resolved version form one component, listing both files. Different versions form two components. A version-less declaration beside a versioned one of the same name merges into the versioned component; there is one fact, and one carries more of it.
- **`--include-vendored`.** The opt-in `parse_vendored` (`cmake.rs`, `cmake-vendored`, `pkg:generic/<dir>@<version.txt>`) can name the same directory. When a directory it reports contains a project declaration, that entry is dropped and its file joins the project component's evidence.

## R6. Config-template names (C237)

**Finding (code).** The 25 names #1616 counted with a `<name>Config.cmake[.in]` but no `project(<name>)` come from `config_template_stem` and `provided_config_stem`. No component can stand for them: there is no version and no single project directory.

**Decision.** They stay as today: no component, and listed in C237 `in-tree`. Searches paired with a project component count in a new `paired.project` key, serialised only when non-zero so that a scan with no project components keeps C237's bytes (FR-012). The existing equation still holds: `read = unresolved + tools + paired.fetch + paired.installed + paired.submodule + paired.project + in-tree-declarations + emitted`.

**Recorded as a spec amendment:** FR-010 is narrowed so that `in-tree` keeps the names that have no project component.

## R7. Versions (FR-003)

**Finding (code and catalog).**
- C197 `waybill:version-absence` is a closed enum, extensible without a new row (`by-construction` only), meaning "no version exists to be found".
- C151 `waybill:unresolved-reason` is emitted **iff** `waybill:sbom-tier = design`.
- Main modules are `source` tier. Measured locally on a cargo + CMake fixture: the main module is `source`, an unpaired `find_package` component is `design`, a FetchContent component is `source`.

**Decision.**
- **No `VERSION` argument:** C197 gains the value `not-declared`. The project declares no version, so none exists to be found.
- **`VERSION <literal>`:** the version.
- **`VERSION ${X}`:** resolved through the file's `Vars` when it holds a literal `set(X …)`. Otherwise the component is versionless and carries a new row, C238 `waybill:declared-version-expression`, holding the unresolved expression verbatim (for example `${ROCM_VERSION}`).
  - C151 cannot carry this case without breaking its "iff design" invariant.
  - C197 cannot, because a version does exist.
  - The row is named generically so another reader's unresolved manifest expression can reuse it.
- **Principle V audit:** no format has a field for "the version is this unresolved expression".

## R8. Test, example and benchmark directories (FR-015)

**Finding (code).** `cmake/fetches.rs::TEST_DIRS = ["test", "tests", "unittests"]` is what CMake fetches and searches already use for C62 `source-test-directory`. No example or benchmark list exists.

**Decision.**
- **Test directories:** use `TEST_DIRS` unchanged, so a project and a search in the same directory agree.
- **Example, benchmark and documentation directories:** `example`, `examples`, `sample`, `samples`, `benchmark`, `benchmarks`, `bench`, and, added by the T001 census, `doc`, `docs`, `documentation`. They take a new C62 value, `source-example-directory` (`measurements/census.md` (c)).
- **Census before implementation.** T001 counts, on TheRock and the three quality-corpus targets, how many project directories each name catches. A name that catches a first-party library is removed, and one the census shows missing is added, before implementation.

## R9. Projects the root build does not reach (FR-016)

**Finding (code, `cmake/reach.rs::compute`).** Reach follows `add_subdirectory` and `include`. A file nothing reaches is **Unconditional** (`fixed_point`), so the existing `ReachMap` cannot say "not reached from the root build".

**Decision.**
- **Reachability.** `ReachMap` gains a set of files reached from `<scan root>/CMakeLists.txt`: a breadth-first search over the `into` edges `compute` already builds.
- **New row C239 `waybill:cmake-build-reach`**, value `not-reached`, on a project whose declaring listfile is outside that set. It is emitted only when a scan-root `CMakeLists.txt` exists; without one, "reached from the root build" is undefined, and nothing is annotated.

**Prediction.** On TheRock most subprojects are built by a super-build through custom functions, not `add_subdirectory`, so they will carry C239. That is true of what the reader can see. The row's description says reach covers only `add_subdirectory` and `include`.

## R10. Vendored classification (FR-008, SC-006)

**Finding (code).** `waybill:vendored = true` is already emitted by the opt-in `parse_vendored`, and has **no catalog row**: a pre-existing gap.

**Decision on the wire shape.**
- `waybill:vendored = true` is reused, so `--include-vendored` output does not change. It is catalogued as C240, which closes the gap.
- A new row, C241 `waybill:vendored-signals`, holds a sorted JSON array of the signals that held. It appears on any project where at least one candidate signal holds.
- C240 is set when the **chosen** rule holds.

**Decision on choosing the rule. The criterion is fixed now, before any count is read:**
- **Census and labelling (T001):** for each candidate signal, count the projects on TheRock and the three quality-corpus targets that it marks. A random sample of at least 30 projects is hand-labelled vendored or first-party from the project's own files (licence holder, upstream URL, README). The sample and labels are committed in `measurements/vendored-sample.tsv` before any precision is computed.
- **Candidate signals:**
  - `path-segment`: a directory named `third_party`, `third-party`, `thirdparty`, `3rdparty`, `external`, `extern`, `vendor` or `deps` on the path below the scan root;
  - `submodule-boundary`: the project is at or below a git submodule's root;
  - `licence-file`: a licence file in the project directory;
  - `not-reached` (C239).
- **The rule:** the single signal, or two-signal conjunction, with the highest precision on the labelled sample, provided that precision is ≥ 0.90. Ties go to higher recall.
- **If nothing reaches 0.90:** C241 is still emitted, and US2's verdict (C240) is not implemented. The measurement goes to the maintainer before any further decision.

## R11. Override demotion and `--split` (FR-007)

**Finding (code).**
- `apply_main_module_drop_or_demote` (m860, #1626) demotes every main module under an override, and `anchor_retained_mainmod_edges` adds `root → module` for each one.
- `generate/split.rs` enumerates main modules as boundaries.

**Decision.** No change. CMake projects inherit both behaviours (Q1=A).

**Prediction (to be measured in T020).** Under the corpus harness's `--root-name`, the root gains a direct edge to every CMake project, as it already does for Maven's 16 modules on maven-guice. Each project also keeps its enclosing-project edge.

## R12. Blast radius

**Measured (local, `main` `786091a8`).** None of the 16 cached public-corpus repositories has a `CMakeLists.txt` with a `project(` call:
- cobra, flask, opentelemetry-go, moat, the five pants examples, haskell-security-advisories, haskell-language-server, aeson, guice, express, pants_backend_clojure, ripgrep.

The remaining public-corpus targets are a container image and Nix closures. FR-012 therefore predicts byte-identical goldens, confirmed by T021.

**Quality corpus.** `cmake-nlohmann-json`, `python-pytorch` and `cpp-mongo` are the affected targets (CI, T021).

**TheRock.** CI only (T020).

## R13. Performance (SC-007)

**Prediction.** Project declarations are read in the existing single pass. The added work:
- one breadth-first search over the reach graph;
- one ownership lookup per project;
- one pairing per searched name.

None of these adds file I/O. To be measured with measure-source (T020).
