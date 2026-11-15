# Wire contract: Git submodules as components

## Components

One per gitlink. Example CycloneDX component; SPDX 2.3 and SPDX 3 carry the same values as packages and annotations, per C55 and C57's existing rows:

```json
{
  "type": "library",
  "name": "waybill-fixture-dep",
  "version": "9ada6e3b6f74fa7770f46bf8ab6dc4e2a9d59564",
  "purl": "pkg:github/waybill-org/waybill-fixture-dep@9ada6e3b6f74fa7770f46bf8ab6dc4e2a9d59564",
  "properties": [
    { "name": "waybill:build-reference", "value": "declared-and-used" },
    { "name": "waybill:sbom-tier", "value": "source" },
    { "name": "waybill:source-mechanism", "value": "git-submodule" }
  ]
}
```

| URL | PURL |
| --- | --- |
| `https://github.com/O/R(.git)`, `git@github.com:O/R.git`, `ssh://git@github.com/O/R` | `pkg:github/o/r@<commit>`, owner and repository lowercased per the `github` type definition |
| the same three forms for `bitbucket.org` | `pkg:bitbucket/o/r@<commit>`, lowercased likewise |
| the same three forms for `gitlab.com`, `codeberg.org` | `pkg:gitlab/O/R@…`, `pkg:codeberg/O/R@…`, declared case kept (no purl-spec definition) |
| any other host, or a forge path that is not exactly `<owner>/<repo>` | `pkg:generic/<last-segment>@<commit>?vcs_url=git+<sanitized-url>` |
| a relative URL with no `origin` remote | `pkg:generic/<last-segment>@<commit>?vcs_url=git+<relative-url>` |
| no declaration | `pkg:generic/<path-basename>@<commit>` |

`<commit>` is the gitlink's full object name. The PURL never carries credentials (FR-005).

## Edges

- **Root.** The document root depends on every top-level submodule component, including one FR-009 merged into another reader's component, which is recognised by C57 since C56 is not yet emitted (#1558). Top-level means no other submodule component depends on it. This is unconditional, through the root-anchor channel (research R5), so it holds even when the root already has other edges.
- **Nested.** A nested submodule is depended on by its enclosing submodule's component, never by the root directly.

## C57 `waybill:build-reference` (amended definition)

`declared-and-used` when either holds; otherwise `declared-only`:
1. A `find_package` target name case-insensitively equals the submodule path's last segment (milestone 105, FR-008a).
2. A CMake `add_subdirectory(<dir>)`, or the `SOURCE_DIR <dir>` of `ExternalProject_Add` or `FetchContent_Declare`, resolves to the submodule's path or a path inside it. The spec's FR-008 uses the same wording.

Both collectors read the CMake files `discover_cmake_files` returns (research R10). The catalog row text is updated to say so.

## C222 `waybill:git-submodule-coverage` (new)

| | CycloneDX 1.6 | SPDX 2.3 | SPDX 3 |
| --- | --- | --- | --- |
| carrier | `metadata.properties[]` entry | document annotation (`MikebomAnnotationCommentV1` envelope) | document annotation (same envelope) |
| present | only when a `.gitmodules` file was seen under the scan root | same | same |

Value: a JSON object serialised with sorted keys, every reason always present:

```json
{"pinned":3,"skipped":{"git-refused":0,"git-unavailable":0,"no-gitlink":1,"no-repository-data":0,"no-url":0,"unsafe-path-or-url":0}}
```

- **Counting.** `pinned` counts emitted submodule components. Each `skipped` counter counts submodule entries.
- **The one overlap.** A gitlink with no declaration is both emitted (`pinned`) and counted under `no-url`. Every other reason counts entries that produced no component.
- **Parity.** The extractor in each format returns the parsed object, so the parity check compares structure, not bytes.

## Diagnostics (logs)

Warnings at WARN, with the field `reader = "git-submodule"`:
- every skip, naming the `.gitmodules` file;
- every URL whose credentials were stripped (redacted form only);
- `git-refused`, with the `safe.directory` remedy;
- a declared URL that disagrees between two `.gitmodules` files listing the same path.

One INFO summary per scan, with the C222 counts.
