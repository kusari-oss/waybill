# Research: Closure-resolved Haskell components are not labelled `design`

All counts are from the committed corpus goldens at `main` `18691d8` (CI Linux). Code references are at the same commit.

## R1. How each tier is assigned today (FR-001, from the code; T001 confirms with counts)

| Path | Sets | Where |
|---|---|---|
| The Haskell reader emits a declared dependency or the project's own package | `design` (no lock), or `source` (a freeze file or Stack lock) | `haskell.rs` (`sbom_tier: Some("design")`, lines 2133, 2316–2337, 2575) |
| Milestone 926 resolves a declared dependency from the pinned nixpkgs package set | `source` | `nix/haskell_packages/mod.rs::apply_resolved` (line 1761) |
| Milestone 966 versions a boot library from the closure's compiler | **leaves the tier as the reader set it** | `apply_compiler_version`. Its doc comment: "the tier stays what the reader said, because it records where the dependency was declared (clarified 2026-11-12)" |
| The closure synthesises a boot library as an edge target | `build`, **but only if no component already holds that PURL** (`held_purls.insert`) | `closure_identity.rs::compiler_supplied_component` (line 1339) and its call site (1206–1222) |
| A declared dependency merges into its closure twin | `build` | `twin_merge.rs` (line 265) |
| Every closure member | `build` | `nix/closure/emit.rs` (line 661) |
| The closure classifies the evaluated package | C203 `exempt-evaluated-package`; **the tier is not touched** | `resolve::closure_role_scope::assign_role_scope` |

**Why one compiler's boot libraries have two tiers.** A boot library the project's reader declared is already held, as the `design` component milestone 966 versions, so the closure links to it instead of synthesising its own `build` copy. A boot library nobody declared is synthesised as `build`. The tier is decided by which path emitted the PURL first:
- shellcheck: no project manifest, so all 27 are `build`;
- project targets: 4–6 `build` and 19–23 `design`.

Some `design` boot libraries carry no `package.yaml` evidence (moat's `transformers` and `unix`). They come from another reader-emitted manifest in the same repository, which T001 confirms per component.

## R2. The tier of a boot library versioned from the closure's compiler (FR-003)

- **Decision:** `build`.
- **Rationale:**
  - **One library, one answer.** The same library from the same compiler is already `build` when no manifest declares it (shellcheck's 27; 4–6 per project target). Today's tier depends on emission order, which no consumer can reason about.
  - **What supplies it is in the build graph.** It ships inside the closure's `ghc` member (`pkg:generic/ghc@9.10.3`), which is `build`. C218 already names that compiler.
- **This supersedes a recorded clarification.** Milestone 966 decided on 2026-11-12 that such a component "stays `design`" because "the tier says where the dependency was declared". #1681 (filed 2026-11-23) asks instead that these "take the tier of what supplies them". Under `--nix-closure`, the 966 rule left one library with two tiers, so it does not hold up. Where a dependency was declared remains recorded, by its evidence (`package.yaml` occurrences, C-evidence-kind). The tier states the strongest resolution.
- **Alternatives:**
  - **`source`,** matching milestone 926's package-set resolutions. Rejected: those take their version from the nixpkgs package set (a source artifact). A boot library takes its version from the compiler release inside the build graph.
  - **Keeping `design`** (milestone 966). Rejected for the reason above.

## R3. The tier of the project's own package (FR-002)

- **Decision:** `build`, when the package carries C203 `exempt-evaluated-package`: the closure evaluated it, so it is a member of the build graph.
- **Not the evidence: C214 `closure-contents`.** C214 records only whether a member's outputs were examined, and every closure member is `build` whatever its value. moat has 1,054 members `absent` and `build`, and moat's own evaluated package is `absent`. Keying on `read`, as the spec's first draft did, would leave moat on `design`.
- **The precedent that points elsewhere, recorded:** 143–242 Hackage closure members per project target are `source`, milestone 926's package-set resolutions. The project package differs: its version comes from its own manifest, not the package set, and it is the derivation the closure builds. `build`'s definition, "observed during the build process (build-graph trace)", is what the closure evidences.
- **Alternatives:** `source` also removes the cliff. It was rejected because it understates the evidence: "in the source tree" is true of every project file, whereas this package was evaluated into the build graph.

## R4. Where the change is made

- **Decision:** one pass, `resolve::closure_resolved_tier::retier`, run in `scan_cmd.rs` straight after `closure_role_scope::assign_role_scope` (whose C203 it reads) and before emission.
  - For each component whose tier is `design`, it re-tiers to `build` if it carries C203 = `exempt-evaluated-package` (FR-002), or C218 (FR-003, a version from a closure compiler).
  - It returns counts for one INFO line.
- **Rationale:**
  - Both rules sit in one place, each mutable alone (SC-006).
  - It runs only when the closure ran, because only the closure writes C203 and C218, so manifest-only scans are untouched by construction (FR-007).
  - `apply_compiler_version`'s doc comment is updated to point here.
- **Alternatives:**
  - **Set the tier inside `apply_compiler_version`.** That covers FR-003 only, and moves the tier decision into a version helper.
  - **Set it inside `assign_role_scope`.** That mixes scope with tier.
  - **Stop the `held_purls` ordering.** That would emit two components for one PURL, the #1216 defect class.
- **Two compilers.** C218 is written for any compiler the closure holds, the bootstrap `ghc-binary` included. A `design` component versioned from the bootstrap compiler is equally supplied by a closure member, so the rule needs no compiler distinction. The spec's "bootstrap untouched" edge case means only that the existing `build` ones are unchanged, which holds trivially. T001 counts whether any `design` component is versioned from a bootstrap compiler; the corpus shows none.

## R5. Recording why (FR-005, Principle V)

- **Decision:** no new row. The evidence is already on the component, in all three formats with parity:
  - C203 `exempt-evaluated-package` for the project package;
  - C218 `waybill:boot-library-compiler` for a boot library.

  The rule "under `--nix-closure`, these imply `build`" is documented on the C203 and C218 rows and in `docs/reference/sbom-types.md`.
- **The standards-native audit:**
  - **CycloneDX 1.6** has no per-component lifecycle or tier field. `metadata.lifecycles` is document-level, and `evidence.identity` concerns identity, not phase.
  - **SPDX 2.3** has no per-package equivalent.
  - **SPDX 3's** `software_sbomType` is on the `Sbom` element, not the package.

  The per-component tier is itself `waybill:sbom-tier`, and its reason is the two existing rows.
- **Alternative:** a new `waybill:sbom-tier-reason` row. Rejected, because it would restate C203 and C218.

## R6. The regression guard (FR-006)

- **Decision:** invariant **I11**, in `waybill-cli/tests/corpus_harness_195/invariants/checks/closure_resolved_tier.rs`, registered with the others and catalogued in `docs/reference/sbom-invariants.md`.
  - **It applies to** a document holding at least one closure member (a component carrying C214); manifest-only documents are out.
  - **(a)** No component is `design` while carrying C203 `exempt-evaluated-package` or C218.
  - **(b)** If the root has direct dependencies, at least one is not `design`.
- **Boundary:** I9 and I10 assert scope (`optional`, `excluded`), and I11 asserts tier. No precondition is shared beyond C218 being read by I9 for a different purpose.
- **Enforcement:** every invariant × target cell must pass unless declared in `invariant-declarations.toml` (`invariants/mod.rs`, `Verdict::UndeclaredFailure`). The must-pass file named in older specs is not in the tree. I11 gets no declaration.
- **Positive control:** evaluated on `main`'s goldens (T003), I11 must fail on moat, openai, slack-web and slacklinker, naming the components, and pass on shellcheck.

## R7. Expected outcome (a prediction from R1–R5, to be measured in T016)

| Target | `design` now | `design` after | Root's direct dependencies | Reachable, all | Reachable without `design`: now → after |
|---|---|---|---|---|---|
| moat | 21 | 1 (`readme`: a repository package the closure does not evaluate; FR-004) | `moat` → `build`, `readme` stays `design` | 1,135 | 0 → 1,134 |
| openai | 22 | 0 | `build` | 1,362 | 0 → 1,362 |
| slack-web | 24 | 0 | `build` | 1,235 | 0 → 1,235 |
| slacklinker | 24 | 0 | `build` | 2,441 | 0 → 2,441 |
| shellcheck | 0 | 0 | unchanged | 2,085 | 2,085 |

The 1,134 for moat follows from the measured reachability once only `readme`, which has no closure evidence, is dropped.

**Document-level consequences, to be traced rather than avoided:**
- **The aggregated document type.** CycloneDX `metadata.lifecycles` and SPDX 3 `software_sbomType` lose `design` where no `design` component remains: openai, slack-web and slacklinker, but not moat.
- **`--tier`.** Milestone 232's filter (`closure_tier_filter.rs`) selects differently for closure scans.

The spec's first draft, SC-002 ("expected 0") and the `closure-contents` edge case, is amended to match R3 and this table.

## R8. Fixtures

- **Decision:** synthetic closures through the stub `nix` that `nix_config_closure.rs` established, with a package-set cache seeded as `nix_haskell_resolution_m926.rs` and `closure_role_scope.rs` do. No test reads the host's cache (Principle VII). Names are `waybill-fixture-*`.
- **Cases:** contracts/wire.md.
