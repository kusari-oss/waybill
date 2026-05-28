# Contract: CPM.cmake reader extension (US1)

**Maps to**: FR-001, FR-002 | **Source-mechanism**: `cpm-cmake`

## Trigger

A `cpmaddpackage(...)` / `cpmfindpackage(...)` / `cpmdeclarepackage(...)` call appears in any file under the scan root whose path matches `**/CMakeLists.txt`, `**/Dependencies.cmake`, `**/cmake/*.cmake`, or `**/*.cmake`.

## Recognized argument keys

```
NAME <ident>                ← required
VERSION <semver-or-string>  ← optional
GIT_TAG <tag-or-sha>        ← optional
GITHUB_REPOSITORY <org>/<repo>  ← optional
GIT_REPOSITORY <url>        ← optional
GIT_BRANCH <branch>         ← ignored (FR-001 doesn't require)
URL <archive-url>           ← ignored (the FetchContent_Declare reader already covers URL fetches)
SYSTEM YES|NO               ← ignored
OPTIONS "..."               ← ignored
```

One-line and multi-line forms parse identically (whitespace + comment-tolerant).

## PURL derivation (FR-001)

| Inputs present | PURL produced |
|---|---|
| `GITHUB_REPOSITORY` + (`GIT_TAG` or `VERSION`) | `pkg:github/<org>/<repo>@<git-tag-or-version>` |
| `GIT_REPOSITORY` + (`GIT_TAG` or `VERSION`) | `pkg:git+https://<sanitized-url>@<git-tag-or-version>` |
| `GITHUB_REPOSITORY` only, no version | `pkg:github/<org>/<repo>@unknown` + `mikebom:resolver-step: "cpm-no-version"` |
| `NAME` + `VERSION` only (no repo) | `pkg:generic/<name>@<version>` |
| `NAME` only (no version, no repo) | `pkg:generic/<name>@unknown` + `mikebom:resolver-step: "cpm-no-version"` |

`GIT_REPOSITORY` URL is passed through `sanitize_userinfo` before PURL construction (FR-016).

## Annotations emitted

| Annotation | Value |
|---|---|
| `mikebom:source-mechanism` | `"cpm-cmake"` |
| `mikebom:source-files` | absolute path of the `.cmake` file containing the call |
| `mikebom:download-url` | the full `GIT_REPOSITORY` URL (sanitized) if present |
| `mikebom:resolver-step` | present only when version is `unknown` or set to `main`/`master`/rolling tag |

## Test cases (acceptance scenarios mapped)

| US1 Scenario | Fixture | Asserted PURL | Asserted annotation |
|---|---|---|---|
| 1 (GIT_TAG) | `golden_inputs/cpm_cmake/with_git_tag/Dependencies.cmake` | `pkg:github/fmtlib/fmt@12.1.0` | `cpm-cmake` |
| 2 (VERSION only) | `golden_inputs/cpm_cmake/version_only/Dependencies.cmake` | `pkg:github/gabime/spdlog@1.17.0` | `cpm-cmake` |
| 3 (rolling tag) | `golden_inputs/cpm_cmake/rolling/Dependencies.cmake` | `pkg:github/lefticus/tools@main` | `cpm-cmake` + `resolver-step: "cpm-rolling-tag"` |
| 4 (mixed CPM + FetchContent) | `golden_inputs/cpm_cmake/mixed/` | both PURLs present with their respective source-mechanism | both annotations |

## Boundaries (Assumptions)

- CPM custom function wrappers (project-specific macros that wrap `cpmaddpackage`) are out of scope.
- `cpm_default_*` configuration is ignored.
- `CPM_DOWNLOAD_LOCATION` overrides are ignored — we don't follow URL templates.
