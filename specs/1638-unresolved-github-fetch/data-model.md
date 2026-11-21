# Data Model: Name a CMake git fetch with an unresolved reference after its repository

No new types or fields. One existing value, the component `fetch_entry` returns, gains new shapes; one existing predicate is masked.

## Unresolved shape (classification inside `fetch_entry`)

The keywords are read after same-file single-literal `set()` expansion. "Unresolved" means the value still contains `${`.

| Shape | `GIT_REPOSITORY` | `GIT_TAG` | `URL` | Today | After |
|---|---|---|---|---|---|
| GitHub, unresolved ref | literal, matches `github_re` | unresolved | absent or literal | `pkg:generic/<name>`, no URL | `pkg:github/<owner>/<repo>`, URL recorded (FR-001) |
| Other repository, unresolved ref | literal, not GitHub | unresolved | absent or literal | `pkg:generic/<name>`, no URL | `pkg:generic/<name>`, URL recorded (FR-004) |
| Unresolved repository | unresolved | any | any | `pkg:generic/<name>` | unchanged (FR-006) |
| URL | absent | — | unresolved | `pkg:generic/<name>` | unchanged (FR-005) |
| Any other unresolved combination | — | — | — | `pkg:generic/<name>` | unchanged (FR-010) |

What every shape keeps:
- **C151:** the reason `CMAKE_UNRESOLVED_VARIABLE_REASON`, unchanged.
- **Tier:** `design`.
- **C55:** `cmake-externalproject` for `ExternalProject_Add`, `cmake-fetchcontent-git` for a git `FetchContent_Declare`, and `cmake-fetchcontent-url` for a URL one.
- **Version:** `""` (no `@`).
- **`waybill:download-url`:** the literal `GIT_REPOSITORY`, verbatim (no credential stripping, as on the resolved branch). Recorded only where the table says "URL recorded".

## Component fields after the change

| Field | GitHub shape | Other-repository shape |
|---|---|---|
| `purl` | `pkg:github/<owner>/<repo>` (lowercased per spec 1530 FR-015) | `pkg:generic/<declared name>` |
| `name` | the declared name; `purl.name()` when the declared name is unexpanded (existing `declared()` rule) | the declared name |
| `version` | `""` | `""` |
| repository identity (`repository_identity`) | `github.com/<owner>/<repo>` | normalised URL for `cmake-fetchcontent-git`; none for `cmake-externalproject` (R3) |

## Wrapper-recognition predicate (`read_fetches` → `Definitions::collect`)

`has_identity(file, kind, args)` was `declared(...).is_some()`. It becomes `declared(...).is_some_and(|d| !(d.name.is_empty() && d.entry carries C151))`.
- `d.name.is_empty()` means the declared name was unexpanded.
- On `main` no such declaration reached `Some`, so the predicate's value is unchanged for every input (R2).

## Rules applied unchanged (R4)

- **R4 `one_per_purl`:** one component per PURL.
- **R5 `first_declaration_wins`:** by declared name; the FR-008 case has no name and never matches.
- **R9 `merge_into_existing`:** a conditional fetch only; first by PURL, then by repository identity in the nearest ancestor-or-self directory.
- **C233:** `{"file", "version"}`, with `"version": ""` for an unresolved declaration on both paths (FR-013).
- **C231:** an FR-008 declaration moves from `unresolved.identity` to `emitted`.
