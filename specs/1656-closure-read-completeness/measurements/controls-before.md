# Controls before (T001), 2026-11-25, branch at 3b9da57f

Captured through the project's own harnesses: throwaway copies of
`tests/closure_binary_contents.rs` and `tests/closure_structured_attrs.rs`
whose extra test wrote each scan's CycloneDX, deleted after the run. The stub
values are spec 1655's dump of `tests/boot_library_edges.rs`'s plain project.
That dump predates spec 1655's change: on `main` today the stub's C105 is
`transitive-edges-unresolvable: hackage` only, as
`boot_library_edges_are_complete` asserts. Its contents values are unchanged.

| scan | `nix-closure-read` | contents (members / read / partial / absent / failed / no-outputs) | C104 | C105 |
|---|---|---|---|---|
| closure_binary_contents, populated_store() | none | 3 / 2 / 0 / 1 / 0 / 0 | partial | `transitive-edges-unresolvable: cargo, golang, hackage` |
| closure_binary_contents, empty store | none | 3 / 0 / 0 / 3 / 0 / 0 | partial | `transitive-edges-unresolvable: hackage` |
| closure_structured_attrs, CLOSURE_JSON | `{"declarations-unconsulted":1,"derivations-affected":1,"fields-unconsulted":["defaultNativeBuildInputs"],"format-version":4}` | 6 / 0 / 0 / 6 / 0 / 0 | partial | `orphaned-components-detected: 6 component(s) not reachable from root; transitive-edges-unresolvable: hackage` |
| stub (nix_closure_boot_versions, plain project) | none | 4 / 0 / 0 / 4 / 0 / 0 | partial | `transitive-edges-unresolvable: hackage; boot-library-edges-incomplete: 4 hackage boot librar(ies), 4 with dependencies not in the document, 0 without dependency data` |

## Findings

- **No existing scan has neither gap.** `populated_store()` leaves
  `waybill-fixture-lib` absent (1 of 3 members). T006 adds an output for it,
  in that test only, to build the neither-gap control.
- **The declaration-gap fixture also has unread contents** (6 of 6 absent).
  So FR-003's "declarations alone" combination is covered at unit level only
  (T004); the integration test sees both codes.
- **The stub** has unread contents (4 of 4) and no read row: the
  contents-only control.

## Orphan reasons on the declaration-gap scan (T010's before-state)

- `pkg:generic/waybill-fixture-flatdep@1.0`: `reachable-only-through-unemitted-package`
- `pkg:generic/waybill-fixture-flattool@1.0`: `reachable-only-through-unemitted-package`
- `pkg:generic/waybill-fixture-typeddep@2.0`: `reachable-only-through-unemitted-package`
- `pkg:generic/waybill-fixture-typedidentity@7.0`: `reachable-only-through-unemitted-package`
- `pkg:generic/waybill-fixture-typedtool@2.0`: `reachable-only-through-unemitted-package`

5 of 11 components carry `waybill:orphan-reason`.
