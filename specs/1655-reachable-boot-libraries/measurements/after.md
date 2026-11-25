# After: spec 1655

## T001: before-state on `main`

`shared_purls.py` over `main`'s goldens (a9a1393) is byte-identical to
`shared-purls-before.txt` (`cmp`).

## Stub fixture (T005, T006, T011)

`tests/boot_library_edges.rs`, 10 tests, all pass:

- `added_purls_match_the_measurement`: the added sets equal, as sets,
  `stub-before.md`'s measured PURLs: 18 with ghc-prim declared, 19 plain.
  The implementation walks each component at its own label's release, where
  the probe walked per compiler; on the stub the two agree exactly.
- `every_boot_library_carries_c245`: every C218 component's C247 has `source`
  and no `absent`.
- `an_edge_reaches_the_version_its_release_ships`: 9.8.4 `base@4.19.2.0` →
  `ghc-prim@0.11.0` and `rts@1.0.2`, never `ghc-prim@0.12.0`; `rts@1.0.2` is
  one component, labelled `ghc-9.10.3`.
- `boot_library_edges_are_complete`: C105 has no
  `boot-library-edges-incomplete` and keeps
  `transitive-edges-unresolvable: hackage` (still `partial`).
- `no_added_boot_library_is_an_orphan`, `added_libraries_have_their_peers_shape`
  (key set {C218, C247, `waybill:sbom-tier`}, `pkg:hackage/`, no CPE, no
  scope, tier `build`).

Every scan goes through `common::parity::assert_parity`.

## Mutations (T009)

Each applied to `resolve/boot_library_edges.rs` by an exact-string
replacement, reverted with `cp` from a backup (verified with `cmp`).

| mutation | unit tests failing | integration tests failing |
|---|---|---|
| M1: no PURL lookup | `a_dependency_held_under_another_label_resolves_by_purl`, `a_shared_version_is_one_component`, `a_held_purl_is_never_duplicated` | `every_boot_library_carries_c245`, `an_edge_reaches_the_version_its_release_ships`, `boot_library_edges_are_complete` |
| M2: no synthesis | `the_reachable_set_is_closed`, `a_shared_version_is_one_component`, `a_dependency_held_under_another_label_resolves_by_purl`, `a_version_the_table_lacks_stays_absent`, `the_version_is_milestone_966s` | `added_purls_match_the_measurement`, `every_boot_library_carries_c245`, `an_edge_reaches_the_version_its_release_ships`, `boot_library_edges_are_complete`, `added_libraries_have_their_peers_shape` |
| M3: over-reach (walk from `Cabal` too) | `an_unreached_library_is_not_added`, `the_reachable_set_is_closed`, `a_held_purl_is_never_duplicated` | `added_purls_match_the_measurement` |
| M4: no project-label rule | `a_shared_version_is_one_component` | none |

M4 is caught at unit level only. On the stub the walk reaches every shared
version from the project compiler first (labels sort `ghc-9…` before
`ghc-binary-…`), so the rule never decides there. The unit test's second case
(only the bootstrap compiler present) is the one that makes M4 fail. It was
added before the mutation run, because test (c) as first written could not
distinguish the rule from walk order (reasoned, not run).

## Order of work

T007 and T008 were written before T004's unit tests, against tasks.md's
order. The new tests cannot run against the pre-change file (its signature
differs), so the mutations above are the evidence each test can fail.

## Corpus goldens (T014)

Pending the dispatch.
