# Data model: CMake fetches made through DownloadProject's `download_project()`

Everything is in process for one scan. The change is in spec 1539's `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs`. No new type reaches the wire.

## DownloadProject definition

A spec 1539 `Definition` whose lowercased name is `download_project` and whose body has DownloadProject's shape (FR-001):
- a `cmake_parse_arguments` with prefix `DL_ARGS`, whose **resolved** one-value keyword list contains `PROJ`;
- a `configure_file()` whose first argument's last path segment is `DownloadProject.CMakeLists.cmake.in`.

**How the alias is made.** Such a definition gets a synthesized body call in place of its `configure_file()`:

```text
ExternalProject_Add("${DL_ARGS_PROJ}" ${DL_ARGS_UNPARSED_ARGUMENTS})
```

The body call has no enclosing conditions. The template's `if(${DL_ARGS_BUILD_PROJECT})` is not modelled, because both of its branches fetch the same thing (probe D2).

From there it is an ordinary spec 1539 wrapper:
- its shapes are `ParsedVariable` and `Unparsed`;
- the own-identity check (R2) finds no identity in the synthesized call;
- binding, C234, C231 `wrappers` and every spec 1538 rule apply unchanged.

The synthesized name is `${DL_ARGS_PROJ}`, so the component is named `<PROJ>` (FR-004, clarification), not DownloadProject's internal `<PROJ>-download` target (probe D1). Spec 1539's FR-005 archive naming still applies to a URL fetch.

**A definition named `download_project` without the shape** is read exactly as spec 1539 reads it: a wrapper if its own body forwards, otherwise not. If one definition has the shape and another does not, spec 1539's FR-002 makes the call ambiguous.

## Keyword lists (FR-002, clarification)

`cmake_parse_arguments(<P> <options> <one> <multi> …)` and its `PARSE_ARGV` form take three keyword-list arguments. Today spec 1539's binder reads them literally. With this feature, each list argument is first **expanded in the body's scope**, as CMake expands it before the command runs:
- a list held in a variable set by an unconditional literal `set()` in the body resolves to that list: `set(oneValueArgs PROJ PREFIX …)` then `"${oneValueArgs}"`;
- a list that still contains `${…}` after expansion (set conditionally, appended, or never set in the body) is **unresolved**.

```rust
enum Unbound { NotAWrapper, TooFewArguments, Recursion, DepthExceeded, Ambiguous,
               UnknownKeywords }   // new: a keyword list could not be resolved
```

An unresolved keyword list makes the call `Unbound::UnknownKeywords`, counted under C231 `unresolved.identity`. It is never bound with a guessed list.

Each list is expanded as **one string** and then split on `;`, never spliced as separate arguments. An empty list (`set(multiValueArgs "")`) is therefore an empty list in its position, not a missing argument that would shift the next one.

**Effect on spec 1539's wrappers.** A wrapper whose keyword lists are literal (every wrapper on TheRock, per spec 1539's census) is unchanged. A wrapper whose lists are variables changes:
- **before**, the lists were read as literal keywords named `${…}`, so nothing was parsed and every argument went to `UNPARSED_ARGUMENTS`;
- **now**, it binds with its real keywords if they resolve, and is unresolved otherwise.

That is a correction under FR-008's intent: direct calls, and spec 1539's wrappers as measured, stay byte-identical. The corpora check it (SC-006).

## Evidence and accounting

No new fields:
- **C234:** the chain ends in `download_project` as defined, after any spec 1539 wrapper around it.
- **C231:** each shaped `download_project` definition counts in `wrappers.definitions`, and each call in `wrappers.calls`. A call with no `PROJ`, no identity, or unresolved keywords counts in `unresolved.identity`.
