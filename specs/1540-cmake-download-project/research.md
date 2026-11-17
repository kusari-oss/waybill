# Research: CMake fetches made through DownloadProject's `download_project()`

The evidence:
- `measurements/census.md`: TheRock's `download_project` census, run in CI (SC-001);
- `measurements/cmake-semantics/`: `probe.sh`, `results.txt` and `cmake-version.txt`. These show what a `download_project()` call fetches, measured with CMake 4.4.5 on a synthetic module with DownloadProject's mechanism, with localhost tarballs and no network;
- spec 1539's research (R1–R15), whose binder this feature reuses.

## R1. Recognition: by name and shape (FR-001, clarification)

**Decision.** A `download_project` definition is DownloadProject when:
- its body calls `cmake_parse_arguments` with prefix `DL_ARGS`, whose resolved one-value keywords include `PROJ`;
- and it `configure_file()`s a template named `DownloadProject.CMakeLists.cmake.in`.

Such a definition gets a synthesized body call, `ExternalProject_Add("${DL_ARGS_PROJ}" ${DL_ARGS_UNPARSED_ARGUMENTS})`, in place of the `configure_file()`, and from there is an ordinary spec 1539 wrapper.

**Evidence.** All 9 definitions on TheRock have the shape (census). The template is not read, and the synthesized call is what probe D1 shows CMake running:
- the template's `ExternalProject_Add(${DL_ARGS_PROJ}-download ${DL_ARGS_UNPARSED_ARGUMENTS} …)`, after `configure_file()` without `@ONLY`, becomes `ExternalProject_Add(p1-download URL;<url> …)`;
- the unquoted list splits;
- the URL is fetched.

**Rationale.** Synthesizing the call reuses spec 1539's binder, accounting and evidence unchanged. Shape is checked, not only the name, so a project's own `download_project` is never bound through DownloadProject's semantics.

**Alternatives considered.** Reading the configured template (spec option A). It needs `.cmake.in` files read (the walker hands the CMake reader `CMakeLists.txt` and `*.cmake` only), the template path resolved from `${_DownloadProjectDir}`, and `configure_file()` substitution modelled. TheRock needs none of it. Rejected at clarification.

## R2. Keywords come from each definition (FR-002, clarification)

**Decision.** The keyword-list arguments of `cmake_parse_arguments` are expanded in the body's scope before parsing, as CMake expands them before the command runs.
- Each list is expanded as one string and split on `;`, so an empty list keeps its position.
- A list still containing `${…}` after expansion makes the call `Unbound::UnknownKeywords`, which is unresolved.

**Evidence.**
- **Three keyword lists on TheRock** (census): the upstream list (4 copies), with `STATUS_MSG` (3), and without `BUILD_PROJECT` (2). All are held in variables.
- **Probe D5:** a copy whose list has `EXTRA_KW` consumes it (D5a). A copy without it forwards `EXTRA_KW x` to `ExternalProject_Add`, whose generated configure then fails, and **nothing is fetched** (D5b). With a fixed list, a rocthrust call passing `STATUS_MSG` would be bound as forwarding a keyword CMake actually consumes.

**Effect on spec 1539.** Today spec 1539's binder reads keyword lists literally, so a wrapper with variable lists parsed nothing. Expanding first corrects that. Every wrapper in spec 1539's TheRock census has literal lists, so its documents are unchanged there; the corpora check the rest (SC-006).

## R3. Binding rules carried over

| rule | probe | result |
|---|---|---|
| DownloadProject's keywords are consumed, not forwarded | D4 | `SOURCE_DIR custom` lands the download in `custom`; `unparsed` holds only `URL;<url>` |
| a one-value keyword's last value wins | D3 | `PROJ first … PROJ second` → `second` |
| `BUILD_PROJECT` does not change what is fetched | D2a, D2b | both fetch `a` |
| a repeated `URL` is a mirror list, the first fetched | D6 | `a` |

These agree with spec 1539's W1–W8, which the synthesized call inherits.

## R4. Naming (FR-004, clarification)

**Decision.** The synthesized call is named `${DL_ARGS_PROJ}`, so a component is named `<PROJ>`, not DownloadProject's internal `<PROJ>-download` target. Spec 1539's FR-005 archive naming applies to a URL fetch. On TheRock every call names a GitHub repository, so the name always comes from the repository (census).

## R5. What it emits on TheRock (prediction)

*Inference. `measurements/census.md` has the full prediction, which a task checks against a CI run.*
- All 8 calls bind.
- Two PURLs are new: `rocRAND@develop` and `rocPRIM@master`.
- **The other three identities may merge into existing components:**
  - `rocm-libraries@rocm-7.2.4` is a conditional fetch, inside the `rocm-libraries` submodule's own tree, of that same repository, so spec 1538's R9 merges it into the submodule;
  - `googletest@release-1.12.0` already exists;
  - `googletest@release-1.11.0` may have an enclosing supplier.

## R6. Branch-name tags (FR-009)

**Decision.** Not marked in this feature, and filed as **#1598**.

**Evidence.**
- 5 of the 8 calls name a branch-like tag: `develop` ×3, `master` ×2. "Branch-like" is a census heuristic.
- `main` already emits `pkg:github/rocm/rocprim@develop` from a direct fetch.

So the gap is not specific to `download_project`; it applies to every CMake git fetch.

**Rationale.** FR-009 allowed marking only through an existing row. The closest, C166 and C167, state the right fact ("the author named a moving reference"), but are defined for Nix flake inputs. Whether a `GIT_TAG` is a branch also cannot be known statically. Both decisions belong to one change across all git fetches, which #1598 proposes.

## R7. Accounting and evidence

- **C231:** each shaped definition counts in `wrappers.definitions` (9 on TheRock), and each call in `wrappers.calls` (8). An unresolved call (no `PROJ`, no identity, unresolved keywords) counts in `unresolved.identity`. V1 is unchanged.
- **C234:** the chain ends in `download_project` as defined, for example `[["download_project"]]`.

No new row.

## R8. Cost

*Inference.* The shape check and the synthesized call are per definition (9 on TheRock), and binding is per call (8). SC-005 is measured by the interleaved `measure-source` workflow against `main`.
