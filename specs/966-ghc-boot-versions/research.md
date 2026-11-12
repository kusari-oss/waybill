# Research: GHC boot-library versions from the closure's compiler

Code citations are from main@`d2041e9d`. External measurements are in `measurements/README.md` (posted on #966) and re-runnable from the scripts beside it.

## R1 — Where the compiler is known, and when

**Finding:** the closure is acquired before m926 runs.

- `scan_cmd.rs:4536` builds `nix_closure` (a `ClassifiedClosure`).
- `scan_cmd.rs:4604` calls `haskell_packages::enrich`, which classifies the design tier's declared dependencies and assigns their versions.
- `scan_cmd.rs:~4818` merges the closure members, and `:4887` runs the m1275 identity pass (`closure_identity::reclassify`), whose FR-012c emits the closure tier's versionless boot components.

**Decision:** derive the compiler(s) from the closure immediately after `nix_closure` is built, and pass them into `enrich` through `ResolveOptions`. m926 then classifies with the exact boot set (FR-006) and versions design-tier boot libraries in the same pass that assigns every other version, with no repair pass afterwards. The same value is passed to `reclassify` for FR-012c.

**Alternative rejected:** version the design-tier components in a pass after the closure merge. That would version a component m926 had already labelled `compiler-supplied` with C170, C171 and C151. Every one of those would then need withdrawing, and `os-string` would need re-resolving from the package set. m926 already does both correctly if it gets the right input.

## R2 — Identifying a compiler

**Finding:**
- `closure_identity::built_by_ghc` (`closure_identity.rs:130`) already reads `env.nativeBuildInputs` for a store path whose basename has `-ghc-` or `-ghc-binary-` followed by a digit. This is the signal FR-005 of m1275 measured at 60 of 60 agreement.
- `closure.graph.evaluated` (`emit.rs:~402`) names the evaluated derivation.
- Every Haskell closure golden holds exactly one `ghc` and one `ghc-binary` (the five targets: `ghc@9.10.3` or `ghc@9.6.6`, plus `ghc-binary@9.8.4` or `9.2.4`).

**Decision:**
- **A member's compiler** is the GHC store path in its derivation's `nativeBuildInputs`. The release is parsed from the basename: `<hash>-ghc-9.10.3` gives 9.10.3, and `<hash>-ghc-binary-9.8.4` gives 9.8.4. A member whose field names two GHCs has no single compiler and takes none (FR-009 never chooses).
- **The project compiler** (for design-tier declarations, FR-009's last bullet) is the evaluated derivation's compiler. If the evaluated derivation names none (a wrapper such as `symlinkJoin`), it is the closure's single non-bootstrap `ghc-X.Y.Z` when there is exactly one, and otherwise none.
- **No compiler identified** means no change (FR-011), recorded per R6.

The parser is shared with `is_ghc_store_path`, so the two can't disagree about what a GHC path is.

## R3 — The table: format, location, sources

**Format:** a committed TSV, `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv`, read with `include_str!` into a `OnceLock` map. Columns:

```
ghc	library	version	source	platform
```

- `source` is one of `release-notes`, `stack-global-hints` or `ghc-release-tag`.
- `platform` is `all` or `windows`.
- A header comment carries the observation date, the source URLs and the regeneration command.

A TSV rather than generated Rust because the probe's output *is* the data. A diff of it is a review of exactly what changed. And this repository's other committed observations (`measurements/*.txt`, the CPE dictionary probe output) are data files, not code.

**Sources and precedence (FR-002):**
1. GHC release notes, "Included libraries". These exist for 9.4.2–9.14.1. 9.0.x, 9.2.x and 9.4.1 return 404 or have no table.
2. Stack `global-hints.yaml`, only for libraries the notes omit, and for whole releases the notes don't cover.
3. Disagreements between 1 and 2 are settled from the `ghc-X.Y.Z-release` tag: the submodule's `.cabal` at the commit the tag pins. They're listed in the probe as explicit, cited overrides. Measured: one, 9.12.3 `process` 1.6.26.1.

**Coverage:** every `ghc-X.Y.Z` in Stack's table from 9.0.1 onward (36 releases and 1,407 rows in the 2026-11-12 snapshot). This covers every compiler and bootstrap compiler seen in the corpus (9.2.4, 9.6.6, 9.8.4, 9.10.3) and every alias at the pinned revisions. Older releases cost nothing to include, and a bootstrap `ghc-binary` is typically one or two series behind.

**Risk recorded, not hidden:** Stack-only entries have no second source in this probe. Stack's table has one known error, found only because the release notes disagreed. FR-004's package-database check is the independent verification for them (R5).

**Alternatives rejected:**
- *Stack only:* it has a known error.
- *Release notes only:* they omit `ghc-bignum`, `ghc-internal`, `os-string` (9.10) and others, and have nothing before 9.4.2.
- *GHC source tags as the primary source:* authoritative, but one fetch per library per release, and several in-tree libraries are `.cabal.in` templates whose version is substituted at configure time. They're used only to settle disagreements.

## R4 — Platform-specific libraries

**Finding:** the release notes and Stack list `Win32` for every release. The darwin package databases (ghc-9.6.6, ghc-9.10.3) lack it and hold `unix` and `terminfo`. `pkg:hackage/Win32` appears today in haskell-language-server's golden (static tier).

**Decision:** `platform = windows` for `Win32` and `all` for everything else. A `windows` row never versions a component on a non-Windows system.

*Corrected at implementation (966-impl):* the decision also said a `windows` row "is never in the boot set" there. Taking `Win32` out of the classification set hands it to ordinary m926 resolution, and the pinned package set does hold a `Win32` entry (measured: `hackage-packages.nix` at a799d3e3 and the five other revisions in the local cache), so a Linux scan would have versioned it from nixpkgs, a version the build never uses. The classification set is therefore every row the table lists for the release (`BootLibraryTable::names`), so `Win32` stays `compiler-supplied` as it is today; only versioning is platform-filtered (`version_of`). The closure's `system` (for example `x86_64-linux`) decides. Nix closures of GHC are never Windows in the corpus, so in practice `Win32` is always excluded. FR-004's check asserts that no non-Windows database holds a `windows` row and that every `all` row is present.

## R5 — Verifying the table (FR-004)

**Decision:** a test reads every GHC package database it can find, on `/nix/store` and under `$WAYBILL_GHC_PACKAGE_DB_PATHS`, at `lib/ghc-X.Y.Z/lib/package.conf.d/*.conf`, taking `name:` and `version:`. For each database it checks:
- every `.conf` agrees with the table;
- every `all` row for that release is present;
- no `windows` row is present on a non-Windows system.

It finds databases by listing the store, not by evaluating. If it finds none it prints that and, when `WAYBILL_REQUIRE_GHC_PACKAGE_DB=1` is set, **fails**. This is the `WAYBILL_REQUIRE_SPDX3_VALIDATOR` pattern. The public-corpus workflow sets it on Linux, where the IFD targets realise their GHCs. Measured locally: 9.6.6 (37 of 37) and 9.10.3 (42 of 42) pass today.

A mutation (one altered version) must make the check fail (SC-005).

## R6 — Carriers (FR-007, FR-008): Principle V audit

**Native constructs, audited against the vendored schemas:**
- **CDX 1.6** `component.evidence.identity[]` admits `field: "version"` with `methods[].technique` from a closed vocabulary (`source-code-analysis`, `binary-analysis`, `manifest-analysis`, `ast-fingerprint`, `hash-comparison`, `instrumentation`, `dynamic-analysis`, `filename`, `attestation`, `other`). None of them is "a published table keyed by a compiler release", and `other` says nothing. C181's audit reached the same conclusion for evaluated versions ("no format models which mechanism established this version").
- **SPDX 2.3 and 3.0.1** have no version-provenance field. `sourceInfo` describes acquisition and changes.
- **Existing rows:**
  - C170 says why a version is *absent*, and here one is present.
  - C171 lists *candidate* series, which is ambiguity, not provenance.
  - C175 says declared or transitive.
  - C181 is closed to `evaluated` / `file-parsed` for the m1034 tier.
  - Extending C181 with a third value would make it mean two mechanisms of two tiers.

**Decision: two new KEEP-NO-NATIVE rows, with parity extractors in all three formats.**

- **C218 `waybill:boot-library-compiler`** (per component): the compiler release whose shipped version this component carries, as the closure names it (`ghc-9.10.3`, `ghc-binary-9.8.4`). It is emitted on every boot-library component that took its version from the table, and on no other. Its presence is what replaces C170 `compiler-supplied`. The table's source and date are document-scope (C219), so they aren't repeated on every component.
- **C219 `waybill:ghc-boot-libraries`** (document scope): a JSON object encoded as a string, the C202 shape and rationale:

  ```
  {"table":"2026-11-12","system":"x86_64-linux","compilers":{"ghc-9.10.3":"in-table","ghc-binary-9.8.4":"in-table"}}
  ```

  - Each compiler's value is `in-table` or `absent-from-table` (FR-008).
  - `compilers: {}` means the closure ran and identified no GHC.
  - The row is emitted **iff the closure tier ran and either m926 ran (C174) or the closure names at least one GHC**. That is the applicability gate (C202's lesson). *(Corrected at implementation: the second branch was "the identity pass examined Haskell members (C201)", but C201 is emitted on every closure-appended member, Haskell or not; see the C219 contract.)*
    - The second branch was found by `/speckit-analyze`. shellcheck is a closure-only scan with no C174, yet it has 2,045 C201 rows, and all 26 of its versionless boot components come from the closure.
    - Gating on m926 alone would leave C218 there with no C219 beside it.
    - Absence means not applicable, so non-Haskell and non-closure documents are byte-identical (FR-011).

**C170 when the compiler is known but absent from the table:** unchanged (`compiler-supplied`). The distinction FR-008 asks for lives in C219, where the compiler is named. A new C170 value would duplicate it on every component.

## R7 — Components and edges with two compilers (FR-009, FR-010)

**Findings:**
- `emit_declared_edges` (`closure_identity.rs:920`) resolves a declared target **by name** through `purl_of`, ranking a Haskell build of the name (2) over any `pkg:hackage` component (1) over the rest (0).
- Today the design tier's versionless `pkg:hackage/base` holds `base` at rank 1, so every confirmed member's `base` edge, bootstrap-built ones included, lands on that one component.

**Decision:**
- **Design tier (in `enrich`):** each boot-library declaration takes the project compiler's version, and its PURL becomes `pkg:hackage/<name>@<version>` through m926's existing rename path. Renames are applied to relationships at `scan_cmd.rs:~4630` (#980), so no edge is orphaned (FR-010).
- **Closure tier (`emit_declared_edges`):** for a confirmed member whose compiler is known and in the table, a declared target that is a boot library *for that compiler* and has **no Haskell build** (rank 2) of its name in the document resolves to `pkg:hackage/<name>@<member compiler's version>`. If no component holds that PURL yet, a versioned compiler-supplied component is created. It's built as FR-012c's is today, plus the version and C218, and without C170.
  - When the member's compiler is the project compiler, this is the same PURL the design tier now holds, so there's one component.
  - When it's the bootstrap compiler at a different version, it's a second component, and only that compiler's members point at it.
- **Rank-2 targets keep today's behaviour.** A Haskell build of the name (`Cabal_3_14_2_0` on haskell-language-server) is a real closure member that something built from Hackage, and m1275/#1282 bind to it deliberately.
- **Member compiler unknown or absent from the table:** today's path, unchanged: by-name `purl_of`, falling back to FR-012c's versionless component.

**Scope of the bootstrap components:** they're reached only through bootstrap-built tools. m1277/m1423 classify them by their closure evidence like any member. This feature sets no scope (spec Edge Cases).

## R8 — The boot set's other consumers (FR-006)

- `summary.boot_libraries` (`mod.rs:1365`) feeds m1276's scope exemption.
- `boot` feeds FR-012c (`scan_cmd.rs:4873`).

**Decision:**
- With the project compiler known and in the table, `enrich` uses the table's `all`-platform names for it as `boot` and hands that set on through the same field. m1276 and the identity pass need no change in shape.
- `closure_identity` additionally receives the per-compiler table so R7 can test "boot for *this member's* compiler".
- With no compiler, or one absent from the table, `boot` is m926's union, as today.

**Measured consequence:** on openai (9.6.6), `os-string` leaves the boot set. m926 then resolves it as an ordinary declared dependency from the package set, which is SC-003.

## R9 — SC-002 check

`osv-scanner` 2.x is installed locally (`/opt/homebrew/bin/osv-scanner`), and `recall-truth.yml` pins 2.7.0. The check is `osv-scanner scan source --sbom <cdx>`, or the recall workflow's invocation, on each regenerated golden, asserting that HSEC-2023-0007 is reported against `pkg:hackage/base@<version>`. This is run once at close-out against the CI-regenerated goldens. The nightly can't run osv-scanner (recall-truth R7: no DB download in nightly), so the corpus expectation is the component-level proxy instead: `base` carries the table's version and C218.

## Implementation notes (966-impl)

Where the code met a decision above and the decision had to move. Each is also stated where it applies.

1. **R4, `Win32`.** The classification set keeps `windows` rows; only versioning is platform-filtered. Removing `Win32` from the set would have let m926 resolve it from the package set, which holds it.
2. **R6, the C219 gate.** "C201 present" became "the closure names at least one GHC": C201 is on every closure-appended member, so it did not distinguish a Haskell closure.
3. **R2, the shared parser.** `closure_compilers::ghc_name` recognises the `-ghc-` / `-ghc-binary-` marker for both callers. `closure_identity::is_ghc_store_path` still needs only a version to follow it (its m1275 behaviour, so C201 cannot drift); `GhcRelease` needs exactly `X.Y.Z`. Sharing the whole predicate would have changed which members m1275 confirms.
4. **R7 / data model, the shared label.** When the bootstrap compiler ships a library at the project compiler's version, the one component carries the project compiler's label whichever member reaches it first (contract C218, "a shared version is one component, and it carries the project compiler's label").
5. **T014, the resolved path.** A table version is applied by `apply_compiler_version`, not `apply_resolved`: the latter writes C176 (`nixpkgs-resolved-via`), a package-set hash and license, and the `source` tier, none of which is true of a compiler-shipped version, and the clarification keeps the tier `design`. The PURL rename still goes through `correct_version_in_place`, so `summary.renames` records it (#980).
6. **Fixtures of earlier milestones.** Every closure fixture of m1275/m1276/m1277/m1423/m1466 named `ghc-9.4.8`, which the table has, together with a synthetic boot library (`waybill-fixture-bootlib`, …) nulled in the configuration. With 9.4.8 known, the exact set replaces the union and the synthetic name is no longer a boot library, failing 15 tests that exercise the union rule. Those fixtures now name `ghc-99.0.1`, a release the table does not have, so the union rule they were written for still applies; their documents gain C219 `absent-from-table`.
7. **C170 as the only compiler-supplied signal.** `generate/orphan_reason.rs` explained an unreachable synthesised boot component by C170, and the corpus invariants I3, I7 and I9 identified compiler-supplied components by C170 alone. A versioned boot library carries C218 instead, so each now accepts either; otherwise the bootstrap compiler's versioned components would be unexplained orphans and I3/I9 would lose their population.
8. **Build variants built by different compilers (found by the first corpus regen, run 43252290895).** The first design kept a member only when its build variants agreed on one GHC. nixpkgs builds some packages twice under one identity, once with the bootstrap `ghc-binary` and once with the project's GHC. Those members got no compiler and fell back to the versionless component.
   - **What the regen showed:** on `nix-closure-shellcheck`, 14 libraries kept a versionless component beside the versioned ones, reached from 13 members (`base` from `alex`, `happy`, `happy-lib`, `hashable`, `splitmix`, …). On `nix-closure-slack-web`, `jailbreak-cabal` left `Cabal` and `Cabal-syntax` versionless. That breaks FR-010, and SC-001 fell short on two targets.
   - **The fix:** `by_member` holds every compiler that built a variant, and a member takes an edge to each one's version. This applies only when every one of them is in the table and ships the name; otherwise today's path decides the whole member, never half of it.
   - **The test:** `a_member_built_by_both_compilers_depends_on_both_versions`. It fails on the pre-fix code (`waybill-fixture-dep` reached only the 9.10.3 `base` and a versionless `stm`) and passes after it.
