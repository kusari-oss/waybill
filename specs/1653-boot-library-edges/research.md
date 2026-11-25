# Research: GHC boot libraries carry their dependency edges, or the document says they don't

Code references are at `main` `91d84d1`. Every figure is from `measurements/baseline.md`, which says how it was produced.

## R1. Where each release's package database comes from (FR-001, Clarification 1)

- **Decision:** GHC's own x86_64 Linux release bindists, `downloads.haskell.org/~ghc/<v>/ghc-<v>-x86_64-deb10-linux.tar.xz`. They are read once, at development time, by a committed probe, `scripts/ghc-boot-dependencies.py`, and the result is committed as `ghc_boot_dependencies.tsv`, embedded in the binary. Scans never read a package database.
- **Rationale:**
  - **Every release in the table has one.** All 36 releases have a bindist, 200–295 MiB each, and the probe read every one.
  - **It agrees with what nixpkgs builds.** The bindist's dependencies equal nixpkgs' installed package database exactly: 0 differences in 5 of 5, across 35 to 42 libraries and 155 to 199 edges (`nixpkgs-agreement.txt`). The five include openai's own GHC 9.6.6, read from its pinned nixpkgs revision, so with 9.10.3 every compiler output present in CI is checked (FR-001). Those four cover both bindist layouts, a source build (`ghc967`, `ghc9103`) and both bootstrap binaries (`ghc902Binary`, `ghc984Binary`). nixpkgs' `ghc-binary-*` compilers are these bindists repackaged.
  - **It agrees with milestone 966:** every release's library set matches the version table, with 0 version disagreements across the 36 releases.
- **Two layouts:**
  - **9.4 and later** ship `lib/package.conf.d/*.conf`, and the probe reads `depends:`, keeping unit-id names (`ghc-prim-0.12.0-3d5d` → `ghc-prim`).
  - **9.0 and 9.2** are make-built and ship `libraries/<lib>/dist-install/package-data.mk`, from which the probe reads `_DEP_NAMES`. The `ghc` library's is `compiler/stage2/package-data.mk`. `rts` is read from `rts/package.conf.in`, whose `depends:` is empty.
- **Alternatives:**
  - **The store output at scan time:** rejected by Clarification 1. It was present for 3 of the 10 compiler instances in the goldens, and for no bootstrap compiler.
  - **nixpkgs' binary cache as the source:** current nixpkgs has 4 of the 36 releases (`ghc90` has been removed), so the rest would mean archaeology across historical revisions. It is kept as the check: `nix store cat` reads one `.conf` in ≈2.6 s through range reads, not the 225 MB NAR.
  - **Hackage `.cabal` files:** these give `build-depends` with conditionals and version ranges, which describe intent rather than what a release resolved. Rejected.
  - **Stack's global hints:** these list versions only.

## R2. What the table yields on the goldens (FR-001, SC-001)

**Every boot library in every golden is in the table:** 25 to 28 for each project compiler, and 11 to 14 for each bootstrap compiler. The edges that can be drawn to boot libraries already in the document are:

| target | project compiler edges | bootstrap compiler edges |
|---|---|---|
| moat | 121 (24 of 25 libraries) | 27 (10 of 11) |
| shellcheck | 125 (26 of 27) | 32 (11 of 12) |
| slack-web | 125 (26 of 27) | 44 (12 of 13) |
| openai | 119 (25 of 26) | 32 (13 of 14) |
| slacklinker | 134 (27 of 28) | 27 (10 of 11) |

**Each compiler has exactly one boot library left with no edge,** and the reason is the same everywhere:
- the project compiler's `ghc-prim`, whose only dependency is `rts`, which is never a component;
- the bootstrap compiler's `base`, whose dependencies `ghc-bignum`, `ghc-prim` and `rts` are all absent from the document.

## R3. Dependencies the document does not contain (FR-002, US1 scenario 2)

**Measured:** most of each bootstrap compiler's boot libraries, and 5 to 11 of each project compiler's, depend on at least one library that is not a component. These are mainly `rts`, `ghc-internal`, `ghc-boot-th`, `ghci` and `hpc`, and for the bootstrap compilers `ghc-prim` and `array`.

- **Decision:**
  - No component is synthesised (FR-002).
  - The edge is omitted.
  - The boot library is recorded as **incomplete**: its dependencies are known, and some are absent from the document. It counts toward the completeness reason (R5).
  - A boot library the table does not cover is recorded as **unknown**.
- **Consequence, stated:** every one of the five goldens will carry the reason, shellcheck included. That is the true answer: shellcheck's `ghc-prim` depends on an `rts` the document omits.
- **Not done here:** emitting the compiler's whole boot-library set as components, so that every edge has a target. That would be a scope change: a new component population, with its own tiers and scopes. It is left as a follow-up issue.

## R4. Where edges and provenance attach (FR-002, FR-003)

- **Decision:** a new post-pass, `resolve::boot_library_edges::apply(&components, &mut relationships)`, in `waybill-cli/src/resolve/boot_library_edges.rs`. In `scan_cmd` it runs immediately after `closure_resolved_tier::retier` (around line 5596), on closure scans only.
- **What it does:**
  1. Groups the components carrying C218 by compiler value: `ghc-9.10.3`, or `ghc-binary-9.8.4`, whose release is `9.8.4`.
  2. Looks up each library in the table for that release.
  3. Adds a `DependsOn` relationship to the component of each dependency's name with the same C218 value, when one exists and the edge is not already present.
  4. Stamps C247 on each boot library.
- **The key is (compiler, name):** measured on all five goldens, no compiler has two boot libraries of the same name, and no PURL is shared by two compilers.
- **The relationship's provenance** (internal) is `source: "ghc-boot-dependencies"`, `data_type: "dependency"`.
- **Rationale:** it follows spec 1650's precedent, a pure post-pass over the components that carry C218, and touches no reader.

## R5. Provenance and the completeness reason (FR-003, FR-005; Principle V)

**The standards-native audit:**
- **CycloneDX 1.6 `dependencies[]`** has `ref`, `dependsOn` and `provides`, and no place on an edge for where it came from.
- **SPDX 2.3 and SPDX 3** relationships carry a `comment`, which could hold provenance per edge, but CycloneDX has nothing to match it, so it cannot meet the parity row.
- **Finding: KEEP-NO-NATIVE**, as for C218. A per-component row carries it in all three formats.

**Decision, provenance:** a new row, **C247 `waybill:boot-library-dependencies`**. It is per boot library, a canonical JSON string:

```json
{"absent":["rts"],"source":"ghc-boot-dependencies/2026-11-24"}
{"reason":"release-not-in-table"}
{"reason":"library-not-in-table"}
```

- `absent` is sorted, and omitted when empty.
- `source` names the table and its observation date.

**Decision, completeness:** a new closed-vocabulary code, `ReasonCode::BootLibraryEdgesIncomplete { incomplete, unknown }`. It renders as:

```
boot-library-edges-incomplete: <N> hackage boot librar(ies), <I> with dependencies not in the document, <U> without dependency data
```

- **How it is computed:** from the components' own C247 values inside `compute_graph_completeness`, following `classify_go_transitive_coverage_degraded`, which also reads the components' own evidence. No parameter is threaded through.
- **Which components count:** those whose C247 has a non-empty `absent` (I), or a `reason` (U).
- **CycloneDX `compositions[]`:** `degraded_ecosystems` gains an explicit arm that adds `hackage`.
- **The vocabulary grows to 12:** C105's row is updated, and the CHANGELOG records it.

**Alternatives:**
- An edge-level SPDX `comment` only: rejected for parity.
- Reusing C218's value: rejected, because C218 says which compiler supplied a version, a different fact.

## R6. What stays the same (FR-007)

- **Not touched:** readers, C218, versions, tiers, scope, the component set, and every other edge.
- **Without `--nix-closure`:** there is no C218, so the pass does nothing, and documents are byte-identical.
- **Corpus:** only the five closure goldens change, through edges, C247, the reason and `compositions[]`. They are regenerated by CI dispatch, and every diff is traced.
- **I11 and the other invariants:** I11 checks tiers, which do not move. The rest are re-run in CI.

## R7. Tests (SC-002 to SC-005)

- **The table, by unit tests:**
  - parsing;
  - release-set equality with milestone 966's table (SC-002b);
  - library-set and version equality per release;
  - `rts` is a known leaf;
  - 9.10.3's `base` is exactly {`ghc-internal`, `ghc-prim`};
  - 9.6.6's `base` is exactly {`ghc-bignum`, `ghc-prim`, `rts`}.
- **The pass, by unit tests:**
  - edges stay within one compiler;
  - no duplicate edges;
  - `absent` is listed;
  - `release-not-in-table` and `library-not-in-table`;
  - the empty table gives no edges.
- **The completeness code, by unit tests:** its rendering, which contains no `;`; that it fires; and that it does not fire when every boot library is complete.
- **Integration**, on the milestone 966 stub-nix closure fixtures (`tests/fixtures/nix_closure_boot_versions/`), whose compiler is 9.10.3:
  - `base` → `ghc-prim` appears, in three formats, with parity (SC-004's positive control);
  - C247 is present;
  - the reason fires.
- **Mutations:**
  - the table lookup returns nothing;
  - the compiler key is ignored, so edges cross compilers;
  - the code is not computed;
  - the `compositions` arm is removed;
  - C247 is not emitted in one format.
- **Machine independence (SC-002a):** the same stub scanned with a fake present store output gives identical edges.
