# Data Model: Pants resolves owned and named across both namespaces

All state is in-process per scan; nothing persists. Every entity below exists
today in some form. This feature extends them.

## Resolve

A named, locked dependency set in one language namespace.

| field | type | notes |
|---|---|---|
| namespace | `LanguageNamespace` (`Python` \| `Jvm`) | existing enum in `scan_fs/package_db/pants_resolve.rs`; wire form `python` / `jvm` |
| name | string | from `[<lang>.resolves]` key, Pants built-in default (`python-default` / `jvm-default`, R2), a tool table's name (R5), or the lockfile's filename stem (discovered only) |
| lockfile | path | one per resolve |
| declaration | `Declaration` (below) | how the repository establishes it |
| top_level_requirements | list of names | Python: pex lock `requirements`; JVM: `generated_with_requirements` reduced to `group:artifact` (R3) |

Identity is `(namespace, name)`, never `name` alone. Qualified string form:
`<namespace>:<name>` (C163's existing form).

## Declaration

How a resolve is known to the repository. It decides whether the resolve is
anchored and how strongly it is classified.

| variant | source | anchored | classification source |
|---|---|---|---|
| `Configured` | key in `[python.resolves]` / `[jvm.resolves]` | yes | Python: declared if a tool's `install_from_resolve` names it, else heuristic; JVM: heuristic |
| `PantsDefault` | `pants.toml` present, language `resolves` table absent, lockfile at the default path (R2) | yes | heuristic (name `*-default` classifies Runtime) |
| `ToolLockfile` (JVM) | any non-`jvm`/`python` table's `lockfile` resolving to the lockfile path (R5) | yes | declared → `Development` |
| `Discovered` | filename-stem convention only | **no** (m868 FR-003) | heuristic; counted unanchored |

Precedence for one lockfile path: `Configured` > `ToolLockfile` > `PantsDefault` >
`Discovered`. A configured name wins; a tool declaration on a configured path
only strengthens its classification (spec edge case).

## Owning (anchor) component

A `PackageDbEntry` that represents a declared resolve. It is not a package.

| field | value |
|---|---|
| purl | `pkg:generic/<name>?pants-namespace=<namespace>` (R1) |
| name | `<name>` |
| version | empty |
| depends | top_level_requirements, normalised per ecosystem (`pypi` names / maven `group:artifact`) |
| depends_ecosystem | `pypi` / `maven` |
| lifecycle_scope | from classification |
| annotations | `waybill:component-kind = lockfile-resolve`; `waybill:pants-resolve = [<name>]`; `waybill:pants-resolve-namespace = <namespace>`; `waybill:resolve-classification-source = declared \| heuristic-or-default` |

Validation: one owning component per `(namespace, name)`. Two anchors that share
a `name` across namespaces differ in their qualifier and never merge at dedup.

## Ownership statement (`waybill:resolve-ownership`, C161)

One per repository, document-scope.

| key | type | meaning |
|---|---|---|
| `declared` | sorted list of `<namespace>:<name>` | anchored resolves (Configured, PantsDefault, ToolLockfile) |
| `discovered` | sorted list of `<namespace>:<name>` | resolves found by convention only |
| `weak_classification` | count | anchored resolves classified by heuristic or default |
| `unanchored_lockfiles` | count | = `len(discovered)` |

Present if either namespace found at least one lockfile; absent otherwise.
In a per-resolve split document it is the whole repository's value (m912 FR-007).

## Package membership (existing, C143 / C164)

`waybill:pants-resolve` names now come from the same naming as the statement
(R2 changes `default` → `jvm-default` / `python-default` under `PantsDefault`).
`waybill:pants-resolve-namespace` is unchanged.
