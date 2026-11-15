# Data model: Git submodules as components

All state is in process for one scan. Nothing persists.

## SubmoduleDeclaration

One `[submodule "<name>"]` section of a `.gitmodules` file (research R7).

| field | type | rule |
| --- | --- | --- |
| `name` | string | Section name. Used only in diagnostics. |
| `path` | relative path | Required. Rejected (`unsafe-path-or-url`) if absolute, if it contains `..`, NUL or a newline, or if it escapes the declaring repository. |
| `url` | string, optional | Rejected if it begins with `-`. Credentials are stripped (FR-005) before any use beyond parsing. |
| `declared_in` | path | The `.gitmodules` file, for diagnostics and C222 attribution. |

A path declared by several `.gitmodules` files (a monorepo repeating its projects' entries) collapses to one declaration, and the first URL in path order wins. A disagreement between URLs is logged.

## Gitlink

One tree entry of mode `160000` (research R1).

| field | type | rule |
| --- | --- | --- |
| `repo` | canonical path | The repository whose tree holds the entry. |
| `path` | relative path | Relative to `repo`. |
| `commit` | 40- or 64-hex string | The full object name, as `ls-tree` prints it. |
| `initialised` | bool | `<repo>/<path>` is itself a repository top level. If so, it is recursed into (FR-006). |

## SubmoduleComponent

One per gitlink (FR-002). It becomes a `PackageDbEntry`.

| field | source |
| --- | --- |
| PURL | FR-004, research R9: `pkg:<forge>/<owner>/<repo>@<commit>`, or `pkg:generic/<name>@<commit>?vcs_url=git+<url>`, or `pkg:generic/<path-basename>@<commit>` |
| version | `commit` |
| `sbom_tier` | `source` |
| `waybill:source-mechanism` (C55) | `git-submodule` |
| `waybill:build-reference` (C57) | `declared-and-used` / `declared-only` (FR-008, research R10) |
| `depends` | The canonical PURLs of the nested submodules this one pins (research R6) |
| `source_path` | The declaring `.gitmodules` file |

A top-level submodule, one no other submodule depends on, is a root anchor (research R5).

## SubmoduleCoverage (C222)

One per document, present only if a `.gitmodules` file was seen (FR-011, research R11).

| field | type |
| --- | --- |
| `pinned` | count of SubmoduleComponents emitted |
| `skipped.no-gitlink` | declarations with no gitlink |
| `skipped.no-url` | gitlinks with no declaration |
| `skipped.no-repository-data` | `.gitmodules` files whose directory is not a repository top level |
| `skipped.git-unavailable` | repositories that could not be read because `git` is absent or timed out |
| `skipped.git-refused` | repositories git refused (`safe.directory`, research R2) |
| `skipped.unsafe-path-or-url` | declarations rejected by research R7 |

A gitlink with no declaration is still emitted (spec edge case) and is counted under both `pinned` and `no-url`. That is the only overlap between the counts, and the contract states it.

## Transitions

```
.gitmodules seen by walker ─┬─ dir is not a repo top level ─► count no-repository-data (each entry)
                            └─ dir is a repo top level ─► git ls-tree ─┬─ git absent / timeout ─► git-unavailable (each entry)
                                                                       ├─ dubious ownership ─────► git-refused (each entry)
                                                                       └─ gitlinks ─► join with declarations by path
                                                                                       ├─ declaration, no gitlink ─► no-gitlink
                                                                                       ├─ gitlink, no declaration ─► emit + no-url
                                                                                       └─ both ─► emit; if initialised, recurse
```
