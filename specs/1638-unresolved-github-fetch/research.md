# Research: Name a CMake git fetch with an unresolved reference after its repository

Traced in code at `main` `7f1a568a`. Each item says whether it was read in code, measured, or is a prediction a task will test.

## R1. Where the change goes: one branch of `fetch_entry`

**Read in code.** Every fetch route builds its component in `fetch_entry` (`waybill-cli/src/scan_fs/package_db/cmake.rs`):
- direct `FetchContent_Declare` and `ExternalProject_Add`;
- spec 1539 wrapper calls, once bound;
- spec 1540 `download_project()`.

Each reaches it through `cmake/fetches.rs::declared`. The unresolved branch returns `pkg:generic/<declared name>` before the GitHub check runs, and passes `download_url: None`.

**Decision.** Split the unresolved branch by what is unresolved. The literal-repository case is `GIT_REPOSITORY` present with no `${`, `GIT_TAG` present with a `${`, and no unresolved `URL`.
- **Literal GitHub repository** (FR-001): `pkg:github/<owner>/<repo>`, with no version.
  - Owner and repository come from the existing `github_re` and the spec 1530 FR-015 fold, the resolved branch's exact rule.
  - `download_url` is `Some(GIT_REPOSITORY)`, as on the resolved branch.
  - C151, the design tier and C55 are unchanged from today's unresolved branch.
- **Literal non-GitHub repository** (FR-004): unchanged `pkg:generic/<declared name>`, but `download_url` is `Some(GIT_REPOSITORY)`.
- **Everything else** (FR-005, FR-006): byte-for-byte today's branch, still with `download_url: None`. That covers an unresolved `URL`, an unresolved `GIT_REPOSITORY`, and a literal tag with some other unresolved keyword.

**The literal check comes before the regex.** `github_re` (`^https?://github\.com/([^/]+)/([^/\.\s]+)`) would match `https://github.com/${OWNER}/r.git` and take `${OWNER}` as the owner. An unresolved repository must therefore be excluded before the regex runs. Mutation M2 pins this.

**The component's `name` keeps the declared name** when that name is literal, as a resolved GitHub fetch's does. `declared()` already replaces an unexpanded name with `purl.name()`. Two consumers read `name`:
- `source_binding/cmake_observer.rs` looks up `_deps/<name>-build`;
- the reconciler's match key (R5).

**Alternatives rejected.**
- *Moving the GitHub check above the unresolved check for every case.* An unresolved `GIT_REPOSITORY` would reach the regex (above).
- *A new C151 reason string.* The existing one names `GIT_TAG` and stays true (spec Assumptions).

## R2. Wrapper recognition must not change (spec 1539 R2)

**Read in code; this is the trap in the obvious implementation.** `Definitions::collect` (`cmake/wrappers.rs`) decides whether a `function()`/`macro()` is a wrapper. It asks `declared(kind, body_args, "", vars).is_some()` of the body's built-in call. A body call with "an identity of its own" is spec 1538's function-body fetch, never a wrapper (spec 1539 R2; test `bodies_that_are_not_wrappers`).

pytorch's `aotriton_build_from_source(noimage project)` is recognised as a wrapper today only because its body has no identity: `ExternalProject_Add(${project} GIT_REPOSITORY https://github.com/ROCm/aotriton.git … GIT_TAG ${__AOTRITON_CI_COMMIT})`.
- On `main`, `fetch_entry` gives `pkg:generic/%24%7Bproject%7D`, which `declared()` filters out.
- Changing only `fetch_entry` makes `declared()` return `pkg:github/rocm/aotriton` for the template itself.

The function would then stop being a wrapper, with three consequences:
1. **The call sites are no longer read.** C234 is lost, which breaks FR-001 and US1 scenario 2.
2. **The template becomes a function-body fetch.** It would carry the function-body shape, not the call site's conditions.
3. **A wrapper that forwards a literal tag regresses.** `wf(x v1.2)` over `FetchContent_Declare(${name} GIT_REPOSITORY https://github.com/o/r.git GIT_TAG ${tag})` gives `pkg:github/o/r@v1.2` on `main`. It would become one versionless function-body component, which breaks FR-010.

**Decision.** The wrapper predicate keeps `main`'s answer exactly. A declaration that has a C151 reason and an unexpanded declared name (`Declared.name` empty) is the one case this spec newly emits. It does not count as an identity of its own for wrapper recognition.
- On `main` every such declaration was filtered out: its generic PURL embedded the encoded variable. So the predicate is unchanged for every input.
- Only the recognition closure in `read_fetches` applies the mask. `declared()`'s result at a call site, and the bind-agreement `identity` closure, use the new identity.

**Consequence for FR-008.** The newly emitted unexpanded-name case reaches a component through three routes:
- a top-level call with an unresolved name;
- a non-wrapper function body (one that forwards nothing, or has two fetch calls);
- a wrapper call whose bound name is still unexpanded.

It never comes from a wrapper's template.

**Tests.** Two tests pin this, and mutation M4 removes the mask:
- U-6: a forwarded literal tag is unchanged from `main`;
- U-3: the aotriton-shaped wrapper keeps C234.

## R3. Repository identity: what FR-004 can and cannot give

**Read in code.** `repository_identity` (`cmake/fetches.rs`) gives:
- `github.com/<owner>/<repo>` for any `pkg:github` component, whatever its mechanism;
- the normalised `waybill:download-url` for a component whose C55 is `cmake-fetchcontent-git`;
- the `vcs_url` qualifier for anything else (a submodule);
- `None` otherwise.

So FR-001 gives an identity on both mechanisms, and FR-004 gives one to a `FetchContent_Declare`.

**Finding.** A non-GitHub `ExternalProject_Add` git fetch has C55 `cmake-externalproject`, and has no repository identity even when its tag resolves. The mechanism value does not say whether the download URL is a git repository or an archive. So `repository_identity` cannot use it without a way to tell the two apart.

**Decision.** This is a planning amendment, recorded in the spec's Clarifications. FR-004 records the URL on both mechanisms, which keeps the same form as a resolved fetch. Its repository identity follows the resolved fetch's: present for `FetchContent_Declare`, absent for `ExternalProject_Add`.

Giving resolved `ExternalProject_Add` git fetches an identity would change R9 merges in scans that have no unresolved fetch, which breaks FR-010. It is filed as a follow-up issue, not done here. U-16 pins the parity: a non-GitHub `ExternalProject_Add` fallback stays separate, as its resolved form does.

## R4. Merging: R9, R4 and FR-014 fall out of the existing rules

**Read in code.**
- **R9** (`merge_into_existing`) considers only *conditional* fetches: Optional, with a `cmake-*` C122. Two cases:
  - A conditional fetch with the same PURL as a required component merges into it (`required_by_purl`). A versionless `pkg:github/o/r` therefore merges into a required versionless `pkg:github/o/r`.
  - Otherwise it merges into the nearest ancestor-or-self holder of its repository identity. FR-007 needs no change to R9.
- **C233**: `superseded()` writes `lost.version`, which is `""` for an unresolved fetch. R5 and R9 both call it (`first_declaration_wins`, `merge_into_existing`). FR-013 is satisfied in code today and needs only the catalog row and a test.
- **R4** (`one_per_purl`): two declarations with one PURL become one component, its scope the strongest of theirs. Two unresolved fetches of one GitHub repository now share `pkg:github/o/r`, which covers the "two unresolved fetches" edge case.
- **FR-014**:
  - R9 never merges a required fetch.
  - `one_per_purl` merges only identical PURLs, and `pkg:github/o/r` is not `pkg:github/o/r@v1`.
  - The general deduplicator keys on the PURL, version included (`resolve/deduplicator.rs`), so it does not join them either.
  - That leaves the reconciler (R5).
- **C231**: an FR-008 declaration was `unresolved.identity` because `declared()` returned `None`. It now returns `Some`, so `read_fetches` counts it in `emitted` through `one_per_purl`. The equation holds by construction; U-5 asserts it.

## R5. The design–source reconciler could join what FR-014 keeps apart (prediction, tested first)

**Read in code.** `reconcile_design_source_tiers` (`resolve/reconciler.rs`, milestone 191) collapses a design-tier component into a source-tier sibling when their key matches. The key is (ecosystem, `name`, manifest directory); the manifest directory is the parent of the first source path when there is no source-manifest annotation. A resolved CMake fetch is source tier, and an unresolved one is design tier.
- **On `main`:** the unresolved component is `pkg:generic` and a resolved GitHub fetch is `pkg:github`. Their ecosystems differ, so they never matched.
- **Under FR-001:** both are `pkg:github`. Two declarations of one GitHub repository under the same declared name, in the same directory, one resolved and one not, would now match. The versionless one would be absorbed into the versioned one, which breaks FR-014 and the spirit of FR-002.
- **FR-008's case:** a component named after its repository could also meet a source-tier submodule of that name in the same directory.

**Prediction.** U-13b collapses to one component when no guard is present: two required `FetchContent_Declare`s of one GitHub URL, one with `GIT_TAG v1` and one with `GIT_TAG ${UNSET}`, both named `waybill-fixture-dep`, both in the root `CMakeLists.txt`.

**Decision rule, fixed before the run.** T-R5 runs U-13b against the US1 change with no guard and records the outcome in `measurements/reconciler-probe.md`.
- **If it collapses:** the reconciler skips any design-tier component whose C55 is a CMake fetch mechanism and whose ecosystem is `github`. On `main` no CMake design-tier fetch was `pkg:github`, so the guard changes nothing outside this spec (FR-010), and the `pkg:generic` behaviour of FR-005 and FR-006 is untouched. Mutation M5 removes it.
- **If it does not collapse:** no guard is added, and U-13b stays as the regression test.

**Alternative rejected.** Excluding every CMake unresolved fetch from reconciliation. That would change `pkg:generic` behaviour for URL and unresolved-repository fetches, which breaks FR-005 and FR-006.

## R6. Consumers of a fetch component's PURL

**Read in code**, every reader of `cmake-fetchcontent-*` / `cmake-externalproject` (`grep -rln`, the whole output: 9 files):
- **`cmake_search_pair`** (spec 1614) pairs a package search with a fetch by declared name (`fetch_purls`, keyed by the lowercased declared name). The pairing is unchanged, and the paired PURL is the new one. The FR-008 case has no name, which is unchanged, because it was dropped before.
- **`cmake_fetch_attach`** (spec 1538 R6) attaches by declaring file, independent of the PURL.
- **`cmake_link_attach`** reads `cmake-package-version` and `pkg-config` records only.
- **`source_binding/cmake_observer`** binds `_deps/<name>-build` by `name`, which is unchanged (R1). The bound PURL becomes the GitHub one.
- **`scan_fs/dedup.rs`** maps mechanism names; **`parity/extractors/cdx.rs`** handles C55 values. Neither depends on the ecosystem.

No consumer needs a change.

## R7. Census method (SC-001)

**Measured in CI only**, in `kusari-sandbox/waybill-fixture-therock-sdk`. A new `census/unresolved_fetches.py` runs from the existing `census-fetches` workflow; the TheRock trees there are sparse, with submodules at their recorded commits. A step runs it on the three quality-corpus targets at their pins, cloned the way `census-projects` clones them:
- `cmake-nlohmann-json` `65ee6845`;
- `python-pytorch` `2b3ec348`;
- `cpp-mongo` `41a57524`.

It counts the fetches whose `URL`, `GIT_REPOSITORY` or `GIT_TAG` still holds `${` after same-file single-literal `set()` expansion, split four ways:
- a literal GitHub repository;
- a literal other repository;
- an unresolved repository;
- a URL.

The FR-008 subset is the one with an unexpanded name.

**The census is a measuring script, not a model of waybill.** It reports a wrapper's template body (a `function()`/`macro()` body call whose name is a parameter, which spec 1539 does not count as a declaration) on a separate line, never in the four-way split.

It is reconciled against waybill's own output on `main` in the same run, declaration by declaration, not total against total. `main`'s output shows components only after R4, R5 and R9 have merged them, so no pre-merge count exists to compare with. For each declaration the census counts, the result is one of three:
- **found**: its file is in the C18 source files of a `main` component whose C151 is the CMake reason;
- **accounted**: it has an unexpanded name, or its file is in C233 of some component, consistent with C231's `unresolved.identity` and `merged.*` counts;
- **unexplained**.

Every unexplained declaration is listed and explained before the counts are used. The census is evidence for the success criteria, not an input to the design: FR-001 to FR-014 fix the behaviour whatever the counts are. It runs before any source change (T001).

## R8. Byte-identity (FR-010) and measurement

- **Control.** U-ctl's golden is produced by `main`'s binary before any source change, on a fixture holding every unchanged shape:
  - resolved GitHub and generic fetches;
  - a URL fetch with an unresolved version;
  - an unresolved repository;
  - the forwarded-literal-tag wrapper.
- **Public corpus.** `regen_goldens` through `public-corpus.yml`; every golden that changes is traced to an FR. Prediction: none change unless the census finds a literal-repository unresolved fetch in a corpus target.
- **Quality corpus.** Traced component by component against a `main` control (SC-002).
- **TheRock.** `measure-source` against `main`, three runs each, for SC-003 and SC-006 (median of three).
