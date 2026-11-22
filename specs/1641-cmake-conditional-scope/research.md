# Research: A CMake condition that holds by default does not make a fetch optional

File references are to `waybill-cli/src/scan_fs/package_db/` at `main` `886792b7`. "Traced" means read in code. "Measured" means run.

## R1. Where today's rule lives (traced)

- **`cmake/blocks.rs::BlockStack`** tracks enclosing blocks. `if_conditions()` and `conditions()` return strings in C232's form.
  - `elseif` **replaces** the open `if`'s condition, and `else` prefixes the last one with `else: `. So the earlier branches of an `if`/`elseif` chain are lost, and FR-003's `elseif` and `else` rules cannot be evaluated from these strings.
- **`cmake/reach.rs::compute`** seeds each file with the conditions under which `include()`/`add_subdirectory()` reaches it (spec 1614 R1), in the same string form. The same loss applies.
- **`cmake/fallback.rs::classify`** picks the shape. `cmake-conditional` is its last rung: `(!if_conditions.is_empty()).then_some(Conditional)`.
- **`cmake/fetches.rs::mark`** makes any shape Optional, and writes C232 **only when the entry got a scope**. A conditional fetch brought into scope would lose C232, against FR-005.
- **`cmake/fetches.rs::merge_into_existing`** (spec 1538 R9) defines `conditional` as `Optional` with a `cmake-*` C122. A fetch brought into scope would stop being "conditional" and become a holder. That would bring back the sibling merge R9 was corrected for, against FR-006.
- **`cmake/fetches.rs` C231:** `conditional` counts components with `LifecycleScope::Optional`.
- **`cmake/searches.rs::scopes`:** a name is Optional when no search for it is unconditional and some search is conditional (not a test directory or `BUILD_TESTING`). C236 records the conditions.
- **Variables:** `cmake.rs::collect_set_vars` reads same-file literal `set()` only. Nothing reads `option()`, `cmake_dependent_option()` or `set(… CACHE …)`.

## R2. The evaluator: three values, one chain

- **Decision:** a new module, `cmake/conditions.rs`. It parses one condition string with CMake's `if()` grammar and evaluates it to `Holds`, `DoesNotHold` or `Unknown`.
  - **Precedence:** parentheses, unary tests, binary tests, `NOT`, `AND`, `OR`.
  - **Constants:** `ON`/`OFF`/`TRUE`/`FALSE`/`YES`/`NO`/`Y`/`N`/`1`/`0`/`IGNORE`/`NOTFOUND`/`*-NOTFOUND`.
  - **Platform:** a platform variable (FR-003's list) alone, or compared with a literal by `STREQUAL`/`MATCHES`/`EQUAL`/`VERSION_*`, gives a fourth internal value, `Platform`.
    - `NOT Platform` is `Platform`;
    - `Platform AND x` is `x`;
    - `Platform OR x` is `Platform`;
    - at the top of a chain, `Platform` holds.
  - **Super-build guard:** `NOT CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR`, either operand order, and the `PROJECT_SOURCE_DIR` spelling, hold. The positive form, and `PROJECT_IS_TOP_LEVEL`, are `Unknown`: they describe the standalone build.
  - **Variables:** a bare name or `${NAME}` takes its default (R4), else `Unknown`.
  - **Everything else is `Unknown`:** `DEFINED`, `EXISTS`, `TARGET`, `COMMAND`, `POLICY`, `TEST`, `IN_LIST`, generator expressions, comparisons of non-platform variables.
  - Three-valued logic: `NOT Unknown = Unknown`, `False AND Unknown = False`, `True OR Unknown = True`.
- **The chain** is the conjunction of every enclosing branch: the reach seed's, then the file's own, then a wrapper body's.
- **Rationale:** FR-003 asks whether the fetch runs by default on some supported platform. That is a property of the whole chain, not of any one atom. Classifying atoms alone could not express `if(WIN32 AND X)`.
- **Alternatives considered:**
  - **Classify atoms and take the "worst".** This is wrong for `OR`.
  - **Run CMake.** Ruled out: no subprocess, and the configure step needs the toolchain.

## R3. Branch history, without changing C232

- **Decision:** `BlockStack`'s `If` gains the list of earlier branch conditions, beside the current string. `if_conditions()` and `conditions()` keep returning today's strings, so C232 and C236 are byte-identical (FR-009). A new `branches()` returns, per enclosing `if`, the earlier conditions and the current one.
  - **The evaluator's reading of a branch:**
    - an `elseif` branch holds when every earlier condition does not and its own does;
    - an `else` holds when every earlier condition does not;
    - the `else` of a chain whose conditions are all `Platform` is `Platform`.
  - `reach.rs` seeds carry the same structure, so a file reached from inside an `else()` is evaluated correctly.
- **Alternative rejected: change the C232 strings to carry history.** That changes every conditional component's C232 on every target, for no consumer benefit.

## R4. Where an option's default comes from (FR-002, planning amendment)

- **Decision:** a pre-pass collects, per file, the boolean defaults it declares:
  - `option(NAME "doc" ON|OFF)`, where a missing value means OFF, as CMake does;
  - `cmake_dependent_option(NAME "doc" DEFAULT "DEPENDS" FORCE)`: `DEFAULT` when every `DEPENDS` condition holds by R2, `FORCE` when one does not, else `Unknown`;
  - `set(NAME VALUE CACHE BOOL …)`;
  - a plain literal `set(NAME VALUE)`, reusing `collect_set_vars`.
- **"In reach" of a fetch in file F** is the file set CMake has processed in F's scope chain:
  - F itself;
  - every file that `include()`s F, transitively (same scope);
  - the `CMakeLists.txt` of each directory that `add_subdirectory()`s F's directory, transitively, with the files they include.

  `reach.rs::compute` already walks these edges and gains a `scope_files(path)` accessor.
- **Planning amendment to FR-002:** the position of a definition within a file is not tracked ("before the fetch" is dropped).
  - `option()` defines a cache variable, which is visible for the rest of the configure run.
  - Parents define options before `add_subdirectory()` in the common case.
  - A definition after the fetch, in the same scope, conflicting with an earlier one, already makes the value `Unknown`.

  The census (R6) counts any option defined only after its use, and the spec records this amendment.
- **Conflicts:** several definitions in reach with different defaults give `Unknown`. Equal ones agree.

## R5. Wiring: scope, C232, R9, C231, searches

- **Fetches:** in `fetches.rs`, `classify` still returns `Conditional`. The evaluator then decides:
  - a chain that does not hold leaves `Optional` + C122 `cmake-conditional`;
  - `Holds` or `Unknown` leaves no scope and no C122 (FR-001, FR-004).

  The six specific shapes and Test are untouched, because only the `Conditional` result is re-evaluated.
- **C232:** `mark` writes C232 whenever the declaration has conditions and is not unconditional, in scope or not (FR-005).
- **R9:** `conditional` becomes "a fetch carrying C232 that is not Test": Optional with a `cmake-*` C122, or in scope with C232. Today's set is unchanged, plus the fetches FR-001 brings into scope (FR-006).
- **Merged declarations** (`one_per_purl`): today's rule stands. A component is Optional only if every declaration is. A kept-in-scope declaration ranks with the required ones.
- **C231:** `conditional` keeps counting Optional components (FR-006).
- **Searches** (`searches.rs`): each `PackageSearch` gains the evaluation of its chain. In `scopes`, a search whose chain holds or is unknown counts as `runtime`, and only a search whose chain does not hold counts as `optional` (FR-007). Test-directory and `BUILD_TESTING` searches are unchanged, and so is a search in a `function()` body: spec 1614 (W-17) gave it spec 1538's sixth shape, which this spec leaves Optional. C236 still records every condition.
- **Merged declarations, survivor (added at T021):** `one_per_purl` ranks an unconditional declaration first, then a kept one, then Optional. Ranking a kept declaration with the unconditional ones let it win a tie on file order and move the survivor's C55 (TheRock: Catch2; case M-2).

## R6. The census, before the rule is wired (SC-001)

- **Decision:** a log-only commit lands the evaluator, the branch history and the default collection, and emits one `tracing::debug!(target: "waybill::cmake_conditions", …)` per catch-all conditional fetch and search. The fields are `kind` (fetch or search), `subject` (PURL or search name), `versioned`, `category`, `eval` and `conditions`.
  - **`category`:**
    - `platform`, `superbuild`, `option-on`, `option-off` and `literal`, when the chain is one such atom;
    - `compound` when it combines several;
    - `unknown` otherwise.
  - Scope is unchanged at this commit.
- **The fixture repository** gains `census/conditions.py` (stdlib, `python3 -I -B`) and `.github/workflows/census-conditions.yml`, modelled on spec 1639's `census-merges`. The matrix is TheRock (submodules) plus the quality corpus's CMake targets. It tallies by kind × category × eval, with the versioned count and the would-change count under FR-004.
- **The public corpus** runs at the log-only commit with `regen_goldens` (0 differing expected).
- **Positive control:** a fixture with one fetch per category must produce one event each, before the census is trusted.
- **Result** (`measurements/census.md`, run 44950692611 at 5ae18211):
  - **TheRock:** 57 of 69 catch-all conditional fetches change scope, 41 of them versioned. 44 change through unknown; the most common are TheRock's bundled system libraries, under `THEROCK_BUNDLE_SYSDEPS`/`THEROCK_ENABLE_*` with no visible default.
  - **Quality corpus:** cpp-mongo 3, python-pytorch 1, nlohmann-json 0.
  - **Public corpus:** 0 by measurement.

  The counts support FR-004 and FR-007 as clarified.

## R7. Fixtures and tests

- **Unit, in `conditions.rs`:**
  - every atom kind;
  - `NOT`/`AND`/`OR`/parentheses precedence;
  - the `Platform` rules;
  - the guard in both operand orders;
  - defaults: missing value, `cmake_dependent_option`, `CACHE BOOL`, conflict;
  - `else`/`elseif` from branch history.
- **Integration**, `tests/cmake_conditional_scope.rs`:
  - one case per row of the spec's table and per User Story 2 scenario;
  - an option in an enclosing directory;
  - an option with no default;
  - an unknown expression (`if(EXISTS …)`);
  - an R9 merge of a kept-in-scope fetch into a submodule;
  - the six fallback shapes and `BUILD_TESTING`, unchanged;
  - a conditional package search: under a platform check (in scope) and under an OFF option (Optional);
  - C-ctl: a golden from `main`'s binary, for a tree with no catch-all conditional.

  Every case runs parity (C122, C232, C236, scope), schema validation and the unique-`bom-ref` check, and repeats 10 times.
- **Control:** the integration cases land at the log-only commit, where the changed cases still give Optional, and those failures are recorded.

## R8. Cost

The evaluator parses each enclosing condition once per declaration or search. The default collection is one pass over already-parsed commands. SC-005's 1.05× ceiling is measured on TheRock's `measure-source`, median of 3.
