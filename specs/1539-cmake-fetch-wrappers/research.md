# Research: CMake fetches made through user-defined wrappers

The evidence:
- `measurements/census.md`: TheRock's wrapper census, run in CI (SC-001);
- `measurements/cmake-semantics/`: `probe.sh` and `results.txt`, how CMake 4.4.5 binds a call to a wrapper. These are synthetic projects with localhost tarballs, run locally;
- spec 1538's research, whose rules (R1 to R15) this feature reuses unchanged.

## R1. Which forwarding shapes count (FR-001)

**Decision.** A wrapper is a `function()` or `macro()` whose body makes exactly one call that is either a fetch built-in or another wrapper, and that call receives the caller's arguments by one or more of five shapes. The shape enum is closed:

| shape | example | on TheRock |
|---|---|---|
| `NameParameter` | `ExternalProject_Add("${target_name}" …)`, or `${ARGV0}` | `therock_subproject_fetch`, `llvm_ExternalProject_Add`, `add_custom_libcxx` |
| `Unparsed` | `${ARG_UNPARSED_ARGUMENTS}` after `cmake_parse_arguments(ARG …)` or `(PARSE_ARGV <n> ARG …)` | `therock_subproject_fetch`, the only identity carrier: 40 calls |
| `NamedKeyword` | `URL ${ARG_URL}`, `SOURCE_DIR ${ARG_SOURCE_DIR}` | all of LLVM's, and `therock_subproject_fetch`'s `SOURCE_DIR` / `PREFIX` |
| `Argn` | `${ARGN}` | none directly. #1518's repro. |
| `Argv` | `${ARGV}` | none |

**Rationale.**
- The census (SC-001) finds one shape pair carrying an identity on TheRock: the name parameter with `${ARG_UNPARSED_ARGUMENTS}`. FR-001 requires it.
- `NamedKeyword` costs nothing extra: binding already computes every `<P>_<KEYWORD>`. It is how LLVM's wrappers pass everything, so supporting it is what makes LLVM's 13 in-tree calls *countable* as unresolved rather than invisible (US2).
- `Argn` is #1518's own repro, and the commonest wrapper idiom outside TheRock. `Argv` is the same mechanism.
- A body that calls more than one fetch is not a wrapper. The census finds no such forwarding body, and binding one call to two fetches is a different feature.

**Alternatives considered.**
- *Only `${ARGN}`, as #1518 proposed.* It catches 0 of TheRock's 40 calls (census).
- *Any function whose body mentions a fetch.* This would treat 11 functions as wrappers that forward nothing (`rocm_libs_declare_shared_deps`, `_fetch_gtest`, …). Their literal declarations are already handled by spec 1538 as `cmake-function-body`.

## R2. A function whose body fetch has its own identity is not a wrapper

**Decision.** A definition is a wrapper only when its body call, read on its own exactly as spec 1538 reads it today, yields **no identity**: no PURL, or a PURL containing `${…}`. Otherwise the call is spec 1538's direct fetch in a function body, unchanged, and calls to the function are not read.

**Rationale.**
- Spec 1538's C-13b is exactly this case: `ExternalProject_Add(${dep} GIT_REPOSITORY https://github.com/<o>/<r>.git GIT_TAG v1.2)` inside a function. Its identity is literal, and it is emitted today (pytorch's dlfcn-win32 and xz in the quality corpus). Making it a template would make those components depend on resolving every call, and lose them where a call's name comes from a loop.
- It keeps FR-011 strong. Every declaration that produces a component on `main` produces the same component on the branch, because the only bodies that become templates are those that produced nothing.

**Spec effect.** FR-008's "the fetch call inside a recognised wrapper's body is a template" holds, and R2 says which bodies are recognised. A one-line note is added to FR-008.

## R3. How a call binds (FR-002)

**Decision.** Binding follows CMake's argument rules as measured:

| rule | probe | result |
|---|---|---|
| Named parameters take `ARGV0…`; `ARGN` is the rest; a macro and a function expose the same values | W7 | `name=n1 argv1=URL argn=[URL;u1]` for both |
| `cmake_parse_arguments(PARSE_ARGV 1 ARG …)`: a keyword the call passes but the wrapper does not parse goes to `ARG_UNPARSED_ARGUMENTS`, in order | W1b | `unparsed=[URL;<a>]`, and ExternalProject downloads `a` |
| A **multi-value** keyword takes every word up to the next keyword of the same parse, including an unparsed `URL` | W1 | `TOUCH t1 t2 URL <a> URL_HASH … EXTRA` puts everything into `ARG_TOUCH`; `unparsed` is empty and ExternalProject fails with "No download info" |
| A **one-value** keyword given twice keeps the last | W4 | `W_URL=[second]` |
| A default the body sets when the call did not | W8 | `wf(e8a)` downloads the default; `wf(e8b URL b)` downloads `b` |
| Quoted `"${ARG_UNPARSED_ARGUMENTS}"` and unquoted forwarding reach the fetch alike | W6 | `_EP_URL` is the URL either way |

W1 matters for correctness, not only completeness. A reader that extracted `URL` from the call text by keyword search would emit a component CMake never downloads. The binder therefore models multi-value consumption exactly, and W-11 in the contract pins it.

**Defaults.** A default is recognised in two forms, each setting a parsed variable (`<P>_<KW>`) to a literal:
- `if(NOT DEFINED <P>_<KW>)` around a `set()`;
- `if(NOT <P>_<KW>)` around a `set()`.

Any other assignment to a parsed variable makes it unknown, and it decides nothing unless the identity depends on it. On TheRock, the three defaults are all the first form, and none affects identity.

## R4. A keyword the body and the call both give (spec edge case)

**Decision.** The bound argument list is passed, in body order, to the existing `fetch_entry`, which takes the **first** `URL`. That matches CMake.

**Evidence.**
- W2: a body literal `URL a`, followed by the caller's `URL b` through `${ARGN}`, makes ExternalProject's `URL` the list `a;b`. `a` is downloaded, and `b` is never requested. The probe's HTTP log has no request for `b` in W2, W2b or W3.
- W2b: `URL a b` in one keyword is the same list.
- W3: FetchContent behaves the same.

So a repeated `URL` is a mirror list, not an override, and its first entry is what is fetched. `GIT_REPOSITORY` and `GIT_TAG` are one-value in ExternalProject. The existing first-match read is kept for them too, and no TheRock call repeats them (census).

## R5. A wrapper defined more than once (FR-002, clarified)

**Decision.** Bind against every definition, as the clarification chose. Emit only when every definition gives the same PURL and the same declared hash. Otherwise the call is unresolved, and also counted in `wrappers.ambiguous-calls`.

**Evidence.**
- W5: two definitions, with a call after each: each call runs the definition evaluated last before it.
- W5b: a redefinition in an `add_subdirectory()`d directory is still in effect in the parent afterwards, because function definitions are global.

So the definition in effect depends on evaluation order across `include()` and `add_subdirectory()`, which static reading cannot know. No nearest-directory rule is sound.

**On TheRock:** no wrapper is defined more than once (census), so the rule does not fire there. It is a guard for other trees, pinned by W-10, W-10b and W-10c.

## R6. Naming (FR-005, clarified)

**Decision.** Applied after `fetch_entry`, to wrapped `URL` fetches only. Take the URL's last path segment, without query, and strip one archive extension: `.tar.gz`, `.tgz`, `.tar.bz2`, `.tbz2`, `.tar.xz`, `.txz`, `.tar.zst`, `.zip`, `.tar` or `.7z`. If what remains matches `^(.+?)[-_]v?<version>$`, where `<version>` is exactly the version `parse_version_from_url` returned, the name is the captured group. The PURL becomes `pkg:generic/<name>@<version>`.

**On TheRock** (census): 36 of 39 tarballs are named from the archive. Three keep their bound name at `unknown`, because their filename carries a commit or SQLite's `MMNNPP00` number, not a dotted version: OpenBLAS, libbacktrace and sqlite. Recovering those is #1539's.

**Alternatives considered.**
- *The archive name whenever the filename has a `-`.* This would turn `libbacktrace-<sha>.zip` into `libbacktrace@unknown`, a name the URL suggests but whose version the reader cannot state. Tying the name to a parsed version keeps name and version from one rule.
- *Lowercasing the name.* `pkg:generic` does not require it, and `Catch2` and `SuiteSparse` are the upstream spellings.

## R7. Where binding runs, and what a call site is

**Decision.** Binding runs inside spec 1538's `read_fetches`, extending its two passes:
1. **Pre-pass.** It already tokenises every walked CMake file. It now also builds `Definitions`, by tracking `function()`/`macro()` blocks and their bodies. Each body is checked for a single fetch or wrapper call and its shapes (R1), resolved to a fixed point so that wrappers of wrappers are found (FR-004).
2. **Main pass.** A command whose lowercased name is in `Definitions` is a call site, unless it sits inside the body of a wrapper, where it is part of that wrapper. A call site is bound (R3, R5), and a `Bound` result becomes a `Declaration` exactly as a direct fetch does. From there, spec 1538's marking, first-declaration-wins, one-per-PURL, R9 merge and ownership run unchanged (FR-006).

The built-in call inside a wrapper body is skipped as a declaration (FR-008), and counted in `wrappers.definitions`.

**Rationale.** All of spec 1538's semantics key off a `Declaration`, so producing the same type at the call site is what makes "applies unchanged" true by construction, not by re-implementation.

## R8. Conditions and scope (FR-006)

**Decision.**
- **Conditions:** the call site's enclosing `if()` conditions, then the `if()` conditions enclosing the fetch inside each wrapper body along the chain, outermost first. They go to `fallback::classify` and C232.
- **Function body:** `cmake-function-body` (and C232's `function: <name>`) comes from the **call site's** enclosing function, never from the wrapper's own body. The wrapper's body is why the fetch exists, not a condition on it.
- **Shapes read from arguments** (`FIND_PACKAGE_ARGS`) are read from the bound arguments.
- **Test scope** comes from the call site's directory. `TRY_FIND_PACKAGE_MODE` comes from the call site's directory (spec 1538 R3).

## R9. Accounting (FR-008)

**Decision.** C231 gains one object, `wrappers`, outside V1: `{"ambiguous-calls":N,"calls":N,"definitions":N}`.
- **`definitions`**: wrapper definitions read. Their body calls are not declarations.
- **`calls`**: top-level wrapper call sites, each of them one declaration in `read`.
- **`ambiguous-calls`**: calls whose definitions disagree (R5). Each is also in `unresolved.identity`.

The object is present only when at least one wrapper definition was read, so trees without wrappers keep `main`'s C231 byte for byte. V1 and V2 are unchanged.

A wrapper template that nothing calls no longer counts in `read`. Spec 1538's C-13 parameterised case moves from `unresolved.identity` to `wrappers.definitions`. On TheRock, 3 of `main`'s 44 unresolved declarations are such templates (census).

## R10. The evidence row (FR-009): C234 `waybill:cmake-fetch-wrapper`

**Decision.** A per-component sorted JSON array of chains, each chain a JSON array of wrapper names as defined, outermost first. It is carried through one-per-PURL as a union, and dropped for a declaration that is merged into another component by first-declaration-wins or R9, which C233 already records.

**Principle V audit.**
- CycloneDX `evidence.identity[].methods[]` has a technique enum (`source-code-analysis`, `manifest-analysis`, …), with free text only in `value`. It names *how* a component was identified, not *through which build-system command*. Using it for one reader would also leave SPDX without the information.
- SPDX 2.3 and 3 have no field for the build-system command a dependency was declared through.

So it is a `waybill:*` row, with parity extractors in the three formats, the C232 pattern.

**Alternatives considered.** Putting the chain in C232's conditions as `wrapper: <name>`. Rejected because C232 is present only on non-runtime fetches, and a wrapped fetch is usually required.

## R11. Operator flag (FR-010)

**Decision.** No flag.
- Every TheRock call site with an identity is bound by the static shapes: 40 of 40.
- The census finds 8 `download_project` calls with an identity that no wrapper rule can bind. DownloadProject's body has no fetch call: it writes a generated project with `ExternalProject_Add` through `configure_file()`. A flag naming extra wrapper commands would not bind it either, because there is no body to bind through. What would work is treating `download_project` as an `ExternalProject_Add` alias with `PROJ` as its name. That is a separate decision about one widely vendored module, recorded here with its count, and proposed as a follow-up issue, not part of this feature.

## R12. Cost (SC-005)

**Decision.** Binding adds one map lookup per command and, per call site, work linear in its arguments. Definitions are built in the pre-pass that already tokenises every file.

**Expectation.** *Inference, not a measurement.* There is no measurable change: 53 call sites against 6,172 files. SC-005 is measured by the interleaved `measure-source` workflow, at the branch against `main`. Its baseline is `main`'s 109.8 s full-tree median (run 44034336542), re-measured in the same run.

## R13. Corpus prediction (SC-006)

*Inference, to be checked in CI.*
- **Public corpus goldens:** a target changes only if a walked CMake file defines a wrapper. A target whose tree defines a wrapper template that main counted as `unresolved.identity` changes its C231: `read` and `unresolved.identity` drop, and `wrappers` appears. Components change only where a call binds. The CI compare run decides, and every change is traced to a definition or a call.
- **Quality corpus:** pytorch's C-13b components are unaffected (R2). Every movement is traced PURL by PURL against `main`.

## R14. Recursion and depth (FR-004)

**Decision.**
- **Pre-pass:** the wrapper fixed point stops when an iteration adds no name. That is bounded by the number of definitions.
- **Binding:** follows the chain with the names already on it. A repeat is `Recursion`. Depth is capped at 8, the same bound `expand_known_vars` uses. Both are unresolved, never a loop.
- **On TheRock:** the deepest chain is 2 (`runtime_register_target` → `llvm_ExternalProject_Add`).
