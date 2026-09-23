# Phase 1 Data Model: Nix flake.lock reader

Derived from the spec's Key Entities and the Phase 0 measurements. Field names
below describe the *lockfile*, not the emitted document.

## Entity 1 — FlakeLockDocument

The parsed lockfile.

| field | type | source | validation |
|---|---|---|---|
| `version` | integer | `.version` | MUST equal 7; any other value is unrecognised (FR-010) |
| `nodes` | map of key → FlakeNode | `.nodes` | MUST be present and an object |
| `root_key` | string | `.root` | MUST name a key present in `nodes` |
| `path` | filesystem path | walker | the directory this lockfile governs (FR-011) |

**State**: one of `Parsed`, `UnrecognisedVersion`, `Malformed`. The latter two
both take the warn-and-continue path (FR-010) and emit no components — they are
distinguished only in the diagnostic, because "I do not know this version" and
"this is not valid JSON" are different things to a reader of the log.

## Entity 2 — FlakeNode

One entry in `nodes`. The root node and input nodes share a shape; the root is
distinguished only by being named in `.root`.

| field | type | source | notes |
|---|---|---|---|
| `locked` | LockedRef or absent | `.locked` | absent on the root node; an input without it is malformed |
| `original` | OriginalRef or absent | `.original` | absent on the root node |
| `inputs` | map of name → InputEdge | `.inputs` | absent when the node declares none |

## Entity 3 — LockedRef

The resolved pin. This is what becomes a component.

| field | type | measured values | notes |
|---|---|---|---|
| `type` | string | `github`, `tarball` | drives identifier construction (FR-013a/b) |
| `owner` | string or absent | present on `github` | absent on `tarball` |
| `repo` | string or absent | present on `github` | absent on `tarball` |
| `url` | string or absent | present on `tarball` | the upstream location (FR-005) |
| `rev` | string | present on **both** measured types | the pinned revision |
| `nar_hash` | string | `sha256-<base64>` | SRI over a NAR serialization (FR-009) |
| `last_modified` | integer or absent | epoch seconds | upstream metadata, not identity |

**Validation**: a LockedRef without `rev` cannot be identified by revision. Per
FR-002 identity is the locked revision, so such an input is not emitted, and the
omission is recorded rather than passed over silently.

## Entity 4 — OriginalRef

The reference as written, before resolution. Explanatory only (FR-006).

| field | type | notes |
|---|---|---|
| `type` | string | usually equal to the locked type |
| `ref` | string or absent | a moving branch or tag, e.g. `nixos-unstable` |
| `rev` | string or absent | present when the author pinned exactly |

**Emitted only when it differs from the locked reference.** An `original` that
already names the locked revision adds nothing and would be noise (FR-006, and
the second acceptance scenario of User Story 3).

## Entity 5 — InputEdge

A value in a node's `inputs` map. **This is the discriminated type that R3
found** and the reason the reader cannot treat `inputs` as a simple string map.

| variant | JSON shape | meaning | handling |
|---|---|---|---|
| `NodeRef(key)` | string | names another node | becomes a relationship to that node's component |
| `Follows(path)` | array of string | an alias resolving through the input graph | resolved to its target; MUST NOT mint a component (FR-004) |

## Entity 6 — EmittedInput

The reader's output per emitted input. Maps onto the existing component type; no
new emission channel is introduced.

| field | derived from | requirement |
|---|---|---|
| identifier | `type` + `owner`/`repo` or `url`, + `rev` | FR-013a/b |
| version | `locked.rev` | FR-002 |
| source location | `owner`/`repo` or `url` | FR-005, native field |
| nar-hash annotation | `locked.nar_hash` | FR-009a, annotation only |
| original-ref annotation | `original` when it differs | FR-006 |
| tier | source | lockfile states what the build resolves to |
| relationships | owning node's `inputs` | FR-007, FR-008 |

## Relationships

```
FlakeLockDocument 1 ──* FlakeNode
FlakeNode         0..1 ── LockedRef        (absent on root)
FlakeNode         0..1 ── OriginalRef      (absent on root)
FlakeNode         1 ──* InputEdge
InputEdge         ──> FlakeNode            (NodeRef directly; Follows transitively)
FlakeNode(locked) ──> EmittedInput         (1:1, except path/indirect types — FR-003)
```

## Ordering

Emission order MUST be deterministic and independent of map iteration order
(SC-006). `nodes` is a JSON object, so iteration order is not guaranteed to be
stable across runs or parsers — the same class of defect as #948, where SPDX 2.3
inherited an unstable order and two scans of one tree differed in bytes. Sort by
a total order over the emitted identifier before emission.
