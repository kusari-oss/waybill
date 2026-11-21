# Research: Record which readers found a merged component (C56)

Everything in this file was traced in code at `main` `4eec5a4` unless it says it was measured. The census (T001) measures the occurrences; nothing here predicts a count.

## R1. The CycloneDX carrier (FR-005): native-field audit

**Decision.** C56 becomes a component `properties[]` entry, `waybill:also-detected-via`, whose value is the same JSON array the SPDX annotation carries. Milestone 105's `waybill-source-mechanism` key on `evidence.identity[].methods[]` is removed from `build_evidence`, together with its `winning_source_mechanism` / `losing_source_mechanisms` parameters. No `methods[]` entry is added per reader.

**Audit**, against `waybill-cli/tests/fixtures/schemas/cyclonedx-1.6.json` (read with `python3 -I -B`, not from memory):
- `componentIdentityEvidence` has `field`, `confidence`, `concludedValue`, `methods` and `tools`.
- `methods[]` items are `additionalProperties: false`, with only `technique`, `confidence` and `value`.
  - `technique` is a closed enum of analysis kinds: `source-code-analysis`, `binary-analysis`, `manifest-analysis`, `ast-fingerprint`, `hash-comparison`, `instrumentation`, `dynamic-analysis`, `filename`, `attestation`, `other`. It cannot name waybill's readers; two readers that both read a manifest are both `manifest-analysis`.
  - `value` is "the value or contents of the evidence", the observed data. C207's audit (#1442) already rejected writing a classification there.
- `tools[]` holds bom-refs of objects defined in the BOM. waybill's readers are not BOM objects, and inventing tool components for them would add components to every document.
- `occurrences` records locations, not mechanisms.

Nothing native says "another reader's mechanism found this component". SPDX 2.3 and 3.0.1 also have no native carrier, as the C56 row already records. The property is a parity bridge, as permitted by Principle V's last paragraph.

**Why no `methods[]` entry per reader.** Every C55 mechanism maps to `manifest-analysis` or `filename`, so one entry per reader would repeat the same technique with nothing to tell them apart, and would change the evidence of every merged component for no information.

**Alternatives rejected:** keep the methods[] key (schema-invalid, #1442); put the mechanism in `methods[].value` (wrong semantic, C207); add one tool component per reader (adds components).

## R2. The survivor's C55 (FR-003)

**Decision.** Unchanged. In the general deduplicator, members are sorted by `evidence.confidence` descending with a stable sort, so reader order breaks ties (`resolve/deduplicator.rs`). A loser's annotations are folded in, first value wins, so a survivor with no C55 takes the first loser's.

**For the record (SC-001):** T001's instrumentation also logs which member milestone 105's `precedence_rank` would have chosen for each deduplicator group, or `unranked` when a member's C55 is not one of its 16 variants. The census reports how many C55 values that rule would have changed. Nothing applies it.

## R3. Merge sites (FR-004)

Every function that removes a component after recording it on another was listed by name (`merge|absorb|fold|collapse|reconcile|one_per_purl|first_declaration|attribut`) and read.

**Sites that can join different C55 values.** Each calls the shared helper (R4).

| # | Site | What is absorbed into what | Mechanisms that can differ |
|---|---|---|---|
| S1 | `resolve/deduplicator.rs::deduplicate`, loser loop | any same-PURL member into the highest-confidence member | any pair, including binary-to-source attribution (milestone 109), which rewrites a binary component's PURL to the source PURL and then merges here |
| S2 | `resolve/deduplicator.rs::fold_declared_not_cached` | a declared-not-cached Maven entry into its on-disk twin | Maven mechanisms (`maven-jar-nested` vs none) |
| S3 | `resolve/reconciler.rs::reconcile_design_source_tiers` | design-tier into a source-tier match | e.g. `pip-requirements-txt` into a lockfile component |
| S4 | `cmake/fetches.rs::first_declaration_wins` (R5) | a `FetchContent_Declare` into an ancestor's declaration of the same name | `cmake-fetchcontent-git` vs `cmake-fetchcontent-url` |
| S5 | `cmake/fetches.rs::one_per_purl` (R4) | same-PURL fetches into one | `cmake-fetchcontent-*` vs `cmake-externalproject` |
| S6 | `cmake/fetches.rs::merge_into_existing` (spec 1538 R9) | a conditional fetch into its supplier | a fetch mechanism vs `git-submodule`, or another reader's |
| S7 | `cmake.rs::fold_vendored_into_projects` | `cmake-vendored` into the project owning that directory | `cmake-vendored` vs `cmake-project` |
| S8 | `cmake_search_pair::pair`, project claim | a `cmake-project` into the installed record or submodule of its name | `cmake-project` vs `cmake-package-version`, `pkg-config`, `git-submodule` |
| S9 | `cmake_search_pair::pair`, search pairing | a package search into the record, submodule or project it names | `cmake-find-package`, `cmake-pkg-check-modules` vs the target's |

A search classified `in-tree` (S9) is removed without a target. That is not a merge, so it records nothing.

**Sites not wired, and why.** Each joins members of one reader, or members no reader gives C55. The C55 setters were listed (`grep '"waybill:source-mechanism"'` plus the named constants), and none of these readers is among them:
- the Nix closure merges (`scan_cmd.rs::merge_closure_components`, `twin_merge`);
- the supplement merge (`supplement/merge.rs`; operator input, not a reader);
- Go (`merge_shared_module_entry`, `merge_workspace_replaces`), gem `merge_groups` and Haskell `merge_scope`, all within one reader;
- `pe_clr::absorb`, which aggregates assemblies within `dotnet-assembly-metadata`;
- the Python and JDK binary collapsers, which produce entries and do not merge components.

T001 re-checks this list against the setters at implementation time.

## R4. One helper, one normalisation

**Decision.** A small module replaces `scan_fs/dedup.rs` (FR-008). It works on the annotation map that `PackageDbEntry` and `ResolvedComponent` share (`BTreeMap<String, serde_json::Value>`) and has three functions. `observe(site, subject, survivor, absorbed)` only emits the debug event below; the census uses it alone (R6).
- `record(site, subject, survivor, absorbed)`, where `subject` is the survivor's PURL:
  - adds the absorbed member's C55, and any C56 the absorbed member already carries, to the survivor's C56 set;
  - normalises;
  - emits one `tracing::debug!` event (target `waybill::also_detected`) naming the site, the subject, the survivor's C55 and the absorbed C55. The census groups events by (site, subject) (R6).
- `normalise(survivor)`:
  - removes the survivor's own C55 from the set;
  - removes C56 entirely when the survivor has no C55, or when the set is empty;
  - otherwise writes the set as a sorted, deduplicated JSON array of strings.

**Invariant.** Only S1's fold can give a survivor a C55 it did not have. S1 therefore calls `record` after folding each loser's annotations, and the generic first-wins fold skips the C56 key (as it already special-cases the pants keys). Every other site leaves C55 alone.

**Transitivity.** A component merged at S6 and then at S1 carries one C56: the union of both, sorted (FR-004).

## R5. Planning amendment: C56 needs a survivor C55

**Problem.** S3, the reconciler, does not fold annotations. A source-tier survivor with no C55 that absorbs a design-tier component with C55 `pip-requirements-txt` would, under FR-001 as written, gain C56 `["pip-requirements-txt"]` with no C55 beside it. That breaks FR-012, which promises byte-identical output for a group with only one distinct mechanism, and C56 would mean "also" with nothing to be also to.

**Decision.** C56 is recorded only when the survivor carries a C55 after the merge. In S1 this changes nothing, because the fold gives a C55-less survivor the first loser's C55 and the rest go to C56. In S3, a C55-less survivor records no C56; the design manifest is already recorded there, in `waybill:source-manifests`. FR-001 and FR-002 are amended to say so (spec Clarifications).

**Alternative rejected:** emitting C56 without C55 (breaks FR-012 and gives the row no anchor).

## R6. The census (SC-001)

**Decision.** The census asks the binary, not a model of it. Phase 2 lands R4's helper and has every site call `observe`, the logging half of `record`, which emits the debug event and changes nothing (**log-only**). US1 then switches each site to `record`. Output is shown byte-identical to `main` (public-corpus run, `regen_goldens`: 0 differing). The census then runs that binary in CI:
- **Where:** a new workflow, `census-merges.yml`, in `kusari-sandbox/waybill-fixture-therock-sdk`, dispatched with `--ref main -f waybill_ref=<full sha>`.
- **Targets:** TheRock (CI only) and the three quality-corpus targets. The public corpus is covered by SC-004's golden diff instead, where every changed golden shows its C56 directly.
- **Logging:** `RUST_LOG=waybill::also_detected=debug`.
- **Output:** a stdlib Python script (`python3 -I -B`) tallies events by site and by (survivor C55, absorbed C55) pair. It also counts the S1 groups where `precedence_rank` would have chosen differently, and whether any fingerprint collision was written (the old key in the output).

The scanned SHA is read from `refs.txt`, as in spec 1638.

**Why not count from output.** Losers vanish in the merge, so `main`'s documents cannot show what merged.

## R7. The fingerprint collision (FR-007)

**Decision.** New row **C242**, `waybill:fingerprint-also-matched`: the other library names whose symbol fingerprints matched the same binary, as a sorted JSON array of strings. `scan_fs/binary/entry.rs` writes it instead of `waybill:also-detected-via`. The source field `SymbolFingerprintMatch::also_detected_via` is renamed to match. So is the never-populated `MatchResult::also_detected_via` in `fingerprints/matcher.rs`, so that no code calls it by C56's name. No alias is kept (spec Clarification Q3). The C58 row's co-emission sentence and `docs/reference/identifiers.md` §11.5 name C242.

**Test level.** No integration test builds a binary that matches two bundled records, and building one would need a C toolchain the suite does not assume. C242 is therefore tested at two levels:
- the entry mapping (a match with collisions writes C242 and no C56);
- the emitters (a component carrying C242 emitted in all three formats, with the parity extractors agreeing).

Mutation M5 proves the mapping test can fail.

## R8. The C56 extractors

**Decision.** All three formats use the existing annotation macros (`cdx_anno!`, `spdx23_anno!`, `spdx3_anno!`), as C233 does for its array. The current CDX extractor reads `methods[]`. The SPDX ones flatten the array into single strings, which the macro form would not match, so all three are replaced together and stay `SymmetricEqual`.
