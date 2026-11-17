# Contract: what the document says about fetches made through wrappers

Each case is a synthetic `waybill-fixture-*` tree, scanned with `--offline` and checked in CycloneDX, SPDX 2.3 and SPDX 3. Wrappers are defined in `cmake/wrap.cmake` unless stated. "Wrapped(chain)" means the component carries C234 = `[chain]`, C18 names the call site's file, and the component is the one a direct call with the bound arguments would give, apart from FR-005's name. "Optional(x)" is as in spec 1538: `LifecycleScope::Optional`, C122 = `x`, conditions in C232.

| # | Tree | Expected |
|---|---|---|
| W-1 | `function(wf_argn name)` → `ExternalProject_Add(${name} ${ARGN})`; one call `wf_argn(waybill-fixture-a URL https://example.invalid/waybill-fixture-a-2.1.tar.gz URL_HASH SHA256=<64 hex>)`; and a direct `ExternalProject_Add` of another URL | two components. The wrapped one is Wrapped(`["wf_argn"]`), `pkg:generic/waybill-fixture-a@2.1` with the hash. The direct one has no C234. (#1518's repro, US1-1) |
| W-2 | TheRock's shape: `function(wf_parse target_name)`, `cmake_parse_arguments(PARSE_ARGV 1 ARG "CMAKE_PROJECT" "SOURCE_DIR;PREFIX;EXCLUDE_FROM_ALL" "TOUCH")`, a default for `ARG_PREFIX`, `ExternalProject_Add("${target_name}" … SOURCE_DIR "${ARG_SOURCE_DIR}" … ${ARG_UNPARSED_ARGUMENTS})`. The call is `wf_parse(waybill-fixture-zstd-sources SOURCE_DIR "${_s}" URL "https://mirror.invalid/waybill-fixture-zstd-1.5.7.tar.gz" URL_HASH "SHA256=<64 hex>" TOUCH "${_t}")` | one component, `pkg:generic/waybill-fixture-zstd@1.5.7` (FR-005), with the SHA-256, mechanism `cmake-externalproject`, Wrapped(`["wf_parse"]`), required. (US1-2) |
| W-3 | `macro(wf_named)`, `cmake_parse_arguments(W "" "NAME;URL;URL_HASH" "" ${ARGN})`, `FetchContent_Declare(${W_NAME} URL ${W_URL} URL_HASH ${W_URL_HASH})`; called with `NAME waybill-fixture-n URL …/waybill-fixture-n-3.4.zip …` | `pkg:generic/waybill-fixture-n@3.4`, Wrapped(`["wf_named"]`), mechanism `cmake-fetchcontent-url`. (US1-3) |
| W-4 | W-1's wrapper called five times with five URLs | five components. C231 `read` = 6 (five calls and the direct call), `wrappers.calls` = 5, `wrappers.definitions` = 1. (US1-4) |
| W-5 | W-1's wrapper defined, never called | no component. C231 is absent, because no declaration was read (I-4). `wrappers.definitions` is counted only when C231 is emitted. (US1-5, edge case "defined but never called") |
| W-6 | `wf_parse` called with `SOURCE_DIR` only, as LLVM's runtimes are | no component. `unresolved.identity` = 1. (US2-1) |
| W-7 | `foreach(d a b)` → `wf_argn(${d} URL https://example.invalid/${d}-1.0.tar.gz)` | no component. `unresolved.identity` = 1: one call site, read once, and never guessed. (US2-2) |
| W-8 | `function(wf_outer n)` → `wf_argn(${n} ${ARGN})`; called once with a URL | Wrapped(`["wf_outer","wf_argn"]`). `wrappers.definitions` = 2. (US3-2, FR-004) |
| W-9 | `wf_a` calls `wf_b`, which calls `wf_a`; called once with a URL | terminates. No component. `unresolved.identity` = 1. |
| W-10 | `wf_dup` defined identically in `a/` and `b/`; called with a URL | one component. |
| W-10b | `wf_dup` defined in `a/` forwarding `${ARGN}`, and in `b/` with a different literal `URL`; called with a URL | no component. `unresolved.identity` = 1, `wrappers.ambiguous-calls` = 1. (FR-002) |
| W-10c | `wf_dup` defined in `a/` as a wrapper, and in `b/` as a function with no fetch | no component, `wrappers.ambiguous-calls` = 1 |
| W-11 | `wf_parse` called with `TOUCH t URL <u>`: the multi-value keyword before `URL` | no component, `unresolved.identity` = 1. CMake swallows `URL` into `TOUCH` (probe W1). |
| W-12 | the body passes its own literal `URL a` before `${ARGN}`; the call passes `URL b` | the component is `a`'s. CMake downloads the first `URL` (probes W2, W3). |
| W-13 | the body sets `ARG_URL` to a literal when `NOT DEFINED ARG_URL`; one call passes no URL, one passes `URL b` | two components: the default's and `b`'s (probe W8) |
| W-14 | a call under `if(NOT WaybillFixtureX_FOUND)`, to a wrapper whose body fetch sits under `if(WIN32)` | Optional(`cmake-find-package-fallback`). C232 = `["NOT WaybillFixtureX_FOUND","WIN32"]`, the call site's first. (FR-006) |
| W-15 | a call in `tests/CMakeLists.txt` | test scope, C62 `source-test-directory` (FR-006) |
| W-16 | a FetchContent wrapper called in `sub/` with bound name `waybill-fixture-p` at 2.0; the root declares `waybill-fixture-p` directly at 1.0 | one component, 1.0. C233 lists `sub/CMakeLists.txt` at 2.0. `merged.first-declaration-wins` = 1. The survivor carries no C234: its own declaration is direct. |
| W-17 | a call to `wf_argn` inside `function(add_deps)`, which forwards nothing and is not a wrapper | Optional(`cmake-function-body`), C232 = `["function: add_deps"]`, Wrapped(`["wf_argn"]`) |
| W-18 | calls whose URL ends in `v2.1.tar.gz`, `download?id=7` and `waybill-fixture-q.tar.gz` | named from the bound name, as a direct call is: `pkg:generic/<bound>@2.1`, `@unknown` and `@unknown` (FR-005 fallback) |
| W-18b | a wrapped `GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-g.git GIT_TAG v1` | `pkg:github/waybill-org/waybill-fixture-g@v1`, named as a direct call is (FR-005 applies to URL fetches only) |
| W-19 | a function whose body is spec 1538's C-13b: `ExternalProject_Add(${dep} GIT_REPOSITORY https://github.com/<o>/<r>.git GIT_TAG v1.2)` | unchanged from spec 1538: emitted from the body, Optional(`cmake-function-body`), no C234. The body has an identity of its own, so it is not a wrapper (R2). |
| W-20 | `wf_parse` is defined in a file excluded by `--exclude-path` | the call is not recognised and counted nowhere, as today (edge case "defined outside the scan") |
| W-21 | the wrapper names its fetch `${ARGV0}` and reads `${ARGV1}` as the URL | bound by position like a named parameter |
| W-22 | the same tree as W-2 but with the call spelt `WF_PARSE(…)` | identical to W-2: command names are case-insensitive |

Spec 1538's contract cases C-1 to C-15 still hold, with one change. **C-13**'s parameterised declaration inside `function(wf_fetch)` is now a wrapper template. When nothing calls it: `wrappers.definitions` = 1, `unresolved.identity` = 0, and `read` no longer counts it. C-13's literal declaration is unchanged.

## Invariants (every case, and every corpus document)

- **I-1 to I-4:** spec 1538's, unchanged. The root's direct targets agree in the three formats; C231 V1 holds; C122 and C232 agree; no fetch means no C231 to C234.
- **I-5:** a component carries C234 iff at least one wrapper call is among the declarations it stands for (SC-008).
- **I-6:** C231 `wrappers.ambiguous-calls` ≤ `unresolved.identity`, and `wrappers.calls` ≤ `read`.
- **I-7:** a tree with no wrapper definitions produces a document byte-identical to `main`'s (FR-011).
