# Research: closure documents contain every boot library a boot library depends on

Probes are in `measurements/`. All were run on 2026-11-25 against the committed corpus goldens on `main` at `a9a1393`.

## R1 — One version is one component: the edge lookup is wrong, not only the component set

**Measured** (`shared_purls.py` → `shared-purls-before.txt`). For each closure golden and compiler (C218), take the boot libraries reachable from that compiler's present set, at the versions its release ships, and ask whether a component already holds `pkg:hackage/<name>@<version>`:

| target | compiler | reachable, not labelled | held under the other label | no component |
|---|---|---|---|---|
| moat | ghc-9.10.3 | 9 | 0 | 9 |
| moat | ghc-binary-9.8.4 | 10 | 6 | 4 |
| openai | ghc-9.6.6 | 5 | 0 | 5 |
| openai | ghc-binary-9.2.4 | 5 | 1 | 4 |
| shellcheck | ghc-9.10.3 | 8 | 0 | 8 |
| shellcheck | ghc-binary-9.8.4 | 10 | 7 | 3 |
| slack-web | ghc-9.10.3 | 8 | 0 | 8 |
| slack-web | ghc-binary-9.8.4 | 12 | 7 | 5 |
| slacklinker | ghc-9.10.3 | 7 | 0 | 7 |
| slacklinker | ghc-binary-9.8.4 | 10 | 7 | 3 |

- **Every held one carries the project compiler's C218.** This was checked per PURL. `closure_identity::emit_declared_edges` labels a shared version with the project's compiler: "one version is one component, and it names the project's compiler when that compiler ships the same version".
- **The edge pass misses them.** `resolve::boot_library_edges::apply_with` keys present libraries by (C218, name). So `ghc-binary-9.8.4`'s `base@4.19.2.0` lists `array` as absent, although `pkg:hackage/array@0.5.8.0`, the version 9.8.4 ships, is a component labelled `ghc-9.10.3`.
- **Merging the two compilers' new PURLs** gives distinct components to add: moat 11, openai 8, shellcheck 10, slack-web 12, slacklinker 9. `rts@1.0.2` is shipped by 9.10.3, 9.8.4, 9.6.6 and 9.2.4, and is one component per document.

**Decision:** two changes, both in the spec 1653 pass:
- **(a)** Resolve each dependency to the component holding the PURL of the version the depending library's release ships, whatever its label. The (label, name) lookup stays as the first try, since it is the same answer for every library the table versioned.
- **(b)** Synthesize only the PURLs no component holds.

**Rationale:** (a) alone clears 1 to 7 bootstrap-compiler names per target with no new component. Without it, (b) would either add duplicate PURLs, which the closure deduplicates, or leave those names `absent`.

**Alternatives:**
- Relabel a shared component with every compiler: C218 is a single string (C218 contract), so this would be a wire change.
- One component per (compiler, name): two components with one PURL would break "one PURL, one component", which the deduplicator, parity and every consumer rely on.

## R2 — Where the synthesis runs

**Decision:** inside `boot_library_edges::apply`, before it draws edges. That pass runs once the closure tier has run, after the scope passes (`withdraw_compiler_supplied_scope`, `assign_role_scope`, `withdraw_scope_runtime_evidence_contradicts`) and `closure_resolved_tier::retier`. It runs before `type_artifact_absent_edges` and orphan classification.

**Rationale:**
- **One pass owns the whole job.** It already computes, per compiler, what is present and what is absent. Making it close that set is the smallest change that keeps the decision in one place.
- **No earlier pass would change an added library.**
  - The scope passes only withdraw or exempt a reader's scope on C218 components, and an added one has none (`lifecycle_scope: None`).
  - `retier` sets `sbom_tier` to `build` on C218 components, which the constructor already does.
  - `type_artifact_absent_edges` and orphan classification run after the pass, so they see the added components and edges.
- **It reuses the constructor.** The component comes from `closure_identity::compiler_supplied_component(name, Some((label, version)))`, made `pub(crate)`. That is the function every closure-synthesized boot library comes from today, so FR-002's shape holds by construction.

**Verification, not assumption:** T-check in tasks. On the stub fixture and every golden, the set of property keys on each added component must equal the key set of the boot libraries the closure already synthesized: those with C218 and no `waybill:source-files`. Measured today (moat, openai), those carry C218, C247 and `waybill:sbom-tier` only. If the sets differ, the synthesis moves earlier (`emit_declared_edges`), and this decision is reopened.

**Alternative:** synthesize in `closure_identity::emit_declared_edges`, the reader-side stage. Every pass would then run over the added libraries. But C247 and the absent computation would be split across two modules, and the edge pass would need the closure's compilers twice.

## R3 — Labels and versions for a synthesized library

**Decision:**
- **Version:** milestone 966's `boot_table::version_of(release, name, system)`, with `system` from `ClosureCompilers::system`.
- **Label:** the project compiler (`ClosureCompilers::project_in_table`) when its release ships the same version. Otherwise the compiler that reached the library. When two non-project compilers reach one PURL, the lexically first label wins, for determinism. Today's goldens have one bootstrap compiler, so this cannot happen there.
- **Input:** `apply` gains `Option<&ClosureCompilers>`; `scan_cmd` passes `closure_compilers.as_ref()`, which is in scope.

**Rationale:**
- This is the closure's own rule (`emit_declared_edges`), so a library reached both ways has one label.
- The two tables agree on every non-empty version (`identity-probe.md`). Milestone 966's table is the one with no blanks: it has `rts` on 9.0 and 9.2. `boot_dependencies.rs` already checks the agreement for every shared row (`libraries_and_versions_equal_milestone_966s`). FR-003's test exists, and this feature keeps it.
- **`system`:** for non-Windows systems the 966 table's rows are all `all` (1,371 rows; the other 36 are `windows`). Nix closures are never Windows, so passing the real system matters only for correctness of intent. It is still passed rather than hard-coded.

## R4 — A shared component's own dependencies

**Measured:** in `ghc_boot_dependencies.tsv`, 5 of 436 (library, version) pairs have different dependencies in different releases:
- `directory@1.3.8.5` and `unix@2.8.6.0` gain `os-string` in later releases;
- `template-haskell@2.23.0.0` gains `ghc-internal`;
- `integer-gmp@1.1` has 3 variants;
- `ghc-compact@0.1.0.0` gains `ghc-prim`.

**Decision:** a component's own dependencies come from its C218 release, as today. A shared component held under the project label gets the project release's dependencies.

**Rationale:**
- In the goldens the shared pair is (9.10.3, 9.8.4). For `directory` and `unix`, 9.10.3's set is the superset, so the project release's set loses no edge there.
- Drawing the union over every release that ships the PURL would be a separate behaviour change to 1653's pass, and nothing measured needs it.

This is recorded so that `after.md` can confirm no shared component's edge set shrinks against today's golden.

## R5 — Identity of `rts` and `ghc-platform`

These are decided in the spec (FR-004, Clarifications), from `identity-probe.md`:
- `pkg:hackage/<name>@<version>`;
- OSV returns 0 advisories for either shape, and the positive controls return advisories;
- no CPE, per spec 1373.

There is nothing further to research. The GHC-ecosystem gap is #1707.

## R6 — Tests that encode the old behaviour

`tests/boot_library_edges.rs` asserts three things this feature changes:
- **`every_boot_library_carries_c245`** asserts 9.10.3's `base` lists `ghc-internal` as absent. After the change it lists nothing; the test asserts `ghc-internal` is a component and `base` reaches it.
- **`edges_stay_within_a_compiler`** asserts no edge crosses C218 labels. After (a), the bootstrap `base@4.19.2.0` → `array@0.5.8.0` edge crosses labels correctly. Its real invariant is unchanged and becomes the assertion: an edge from a library of release R goes to the version R ships, so the bootstrap `base` never reaches `ghc-prim@0.12.0`.
- **`incomplete_boot_library_edges_are_reported`** asserts the reason is present. On the stub it no longer can be, which was measured (`measurements/stub-before.md`):
  - both compilers are `in-table`, and every C218 library is in the dependency table;
  - C218 is only ever stamped from milestone 966's table, which covers the same 36 releases as 1653's (`the_releases_are_milestone_966s`), so `release-not-in-table` cannot arise from a real closure.

  The test is inverted: the stub's C105 lacks the reason, and still carries `transitive-edges-unresolvable: hackage`, so the document stays `partial`. The reason's positive control moves to unit level, in `boot_library_edges` tests over `apply_with` with a constructed table:
  - a dependency whose release row has no version in milestone 966's table, so it cannot be synthesized and stays `absent`;
  - a library missing from the table, giving `library-not-in-table`.

  The existing `graph_completeness` unit tests (`boot_library_edges_incomplete_counts_absent_and_unknown`) already keep the classifier honest (`feedback_zero_results_can_mean_broken`).
