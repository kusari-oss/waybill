# Contract: reachability and orphan markers

Synthetic `waybill-fixture-*` trees, scanned in the three formats. "Reachable" means by the one `for_document` answer. Every case also checks:
- **I-A:** the document's completeness orphan count = the classifier's unreachable set size = the I7 population;
- **I-B:** the three formats agree.

| # | Tree | Expected |
|---|---|---|
| O-1 | `--root-name` operator root over a tree with an npm lockfile whose packages are unreachable from the root | each carries `hoisted-unused` or `dead-lockfile-entry` (the classifier sees the document's reachable set) |
| O-2 | the same tree without `--root-name` | the same markers as today |
| O-3 | a binary component (analysis tier) under `tests/data/` with no owner, unreachable | `test-data-binary` |
| O-4 | the same under `Inputs/` and under `test_assets/` | `test-data-binary` |
| O-5 | a binary under `prebuilt/` with no owner, unreachable | `unowned-binary` |
| O-6 | a binary with source files both under `tests/` and outside | `unowned-binary` (every file must be under a test-data directory) |
| O-7 | a component carrying an existing C45 code that would also match O-3 | keeps the existing code (FR-006) |
| O-8 | a library nested in a reachable binary (`parent_purl`) | reachable; no orphan marker; completeness does not count it |
| O-9 | a library nested in an unreachable binary | unreachable; explained by its own population's code |
| O-10 | a `uv.lock` whose root is `source = { editable = "." }` with no `version`, with `dependencies`, `optional-dependencies` and `dev-dependencies` | the root is emitted versionless with C151; edges to all three groups with runtime, optional and development scope; none of the lock's packages is unreachable |
| O-11 | a `uv.lock` root depending on three versions of one package under markers | an edge to each version |
| O-12 | a `Cargo.lock` with an older second version that nothing references, beside a reachable newer one | `dead-lockfile-entry` |

## Invariants

- **I-A:** one unreachable set (FR-002).
- **I-B:** the three formats agree (FR-009).
- **I-C:** every component carrying a C45 code on `main` keeps it (FR-006).
