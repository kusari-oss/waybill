# Research: Haskell pin annotations say what is true

Every finding below was read from code on `main` at 46506106, or from the committed corpus goldens. The goldens are produced in CI by `public-corpus.yml`, with network and a populated nixpkgs cache.

## R1 — Where the measurement comes from

- **Decision.** The "on `main`" half of FR-001 is the committed goldens: CycloneDX, cross-checked against SPDX 2.3 and SPDX 3. The branch half is the regenerated goldens from a `public-corpus.yml` dispatch with `regen_goldens=true`. `measurements/measure.sh` extracts the same table from both sets. The #1741 fixture and the hs-opentelemetry case are measured locally with a branch build and a `main` build.
- **Rationale.** C172 needs nixpkgs enrichment. A local `--offline` scan with an empty `WAYBILL_NIXPKGS_CACHE` degrades to `offline` and emits no C172 at all. That would be a zero result that means "broken", not "clean". The harness sets `WAYBILL_NIXPKGS_CACHE` from its cache (`corpus_harness_195/harness.rs:236`).
- **Alternatives.** Populating a nixpkgs cache locally: rejected. It reproduces the CI environment by hand, and that copy can drift.

## R2 — Where the version source can be read

- **Decision.** The disagreement loop (`nix/haskell_packages/mod.rs:1079`) reads `c.source_type`. `hackage-stack-lock` is the Stack lock file and `hackage-freeze` is the freeze file. `*main-module` is skipped (R3). Any other source type is reported by its value, not mapped to a guessed label.
- **Rationale.** Since spec 1665 both lock kinds carry their own source type, and their source path is the lock file. The note can therefore say exactly what produced the version: `"the emitted version came from <kind> <file> and takes precedence"`. Here `<file>` is the component's source path(s), sorted, and is rootfs-relative as in C18.
- **Alternatives.** A closed enum of kinds without the file name: it would not tell apart two lock files that pin different versions in a multi-lock repository.

## R3 — Main modules and workspace members

- **Decision.** Skip any component whose `source_type` ends in `main-module`, using the existing `is_main_module` (`mod.rs:837`). Every Haskell workspace member is emitted with that source type.
- **Rationale.** The 10 corpus components with the lockfile note are all `hackage-main-module`, and #1682 measured 23 more on hs-opentelemetry. Their versions come from the project's own manifest, and nixpkgs carries the project's last release. That is the working tree compared with a release, not two sources disagreeing about a build (spec FR-003).
- **Alternatives.** A distinct "ahead of release" note: rejected. It describes something the consumer already knows from the version and the release history, and it keeps the annotation on 33 components for no decision a consumer could make.

## R4 — Recomputing after the twin merge

Measured: the two corpus components whose C172 has `"emitted": null` and whose component now has a version.

| Component | Built version | C172 `nixpkgs` | C221 |
|---|---|---|---|
| `hspec-discover` (slack-web) | 2.11.14 | 2.11.14 | present: range `>=2.6.0 && <2.11` |
| `slack-web` (slacklinker, `vcs_url`) | 2.3.0.0 | 2.2.0.0 | absent |

- **Decision.** In `twin_merge::fold`, after the survivor has its version, look at the C172 the design twin carried:
  - If the survivor's version equals C172's `nixpkgs`, remove C172. The closure built nixpkgs' own version, so nothing disagrees. The range violation stays recorded by C221 `waybill:requirement-unsatisfied`, which exists for exactly that fact.
  - If it differs, rewrite C172. `emitted` becomes the built version, `requirement-ranges` is dropped, and the note says the closure built that version. The flake's overlay pinned another release, which is what the #1457 note predicted.
- **Rationale.** Rewriting hspec-discover's C172 would produce `emitted == nixpkgs`, which by definition is not a disagreement. Keeping it would also duplicate C221. Slacklinker's slack-web is a real disagreement between the closure and the package set.
- **Spec impact.** FR-005 is refined from "rewrite" to "recompute: remove when equal, rewrite when different". The twin-merge test that asserts "C172 stays true" (`twin_merge.rs:389`) is updated to match: its fixture builds 2.1, and its C172 names `nixpkgs 1.0.0.0`, so C172 is rewritten and still present.

## R5 — The disagreement count (C174)

- **Decision.** C174 `waybill:nixpkgs-haskell-resolution`'s `disagreements` counts the components that carry C172 when the document is written. The merge returns how many it removed, and the call site (`cli/scan_cmd.rs:5173`) subtracts that from `nixpkgs_haskell_summary`. The main-module skip simply doesn't increment the count.
- **Measured.** haskell-language-server has `disagreements: 11`, which is 5 main modules plus 6 in the `"emitted": null` form. On the branch it should be 6.
- **Alternative.** Recount from the components at emission: rejected. It would move C174's computation away from the pass that owns it.

## R6 — The freeze orphan arm (#1741)

- **Decision.** At `generate/orphan_reason.rs:648`, match `Some("hackage-stack-lock" | "hackage-freeze")` and return `EdgelessLockPin`. Update the arm's comment and the C45 row's code list to name both kinds.
- **Rationale.** Neither file kind records edges: a freeze file is a list of `constraints`. In both cases the pin is project-local and no declared package reaches it. One reason means one fact for a consumer.
- **Alternatives.** A new `edgeless-freeze-pin` code: rejected. It splits one fact into two names a consumer must union.

## R7 — I7 sees a freeze file

- **Decision.** I7 is a corpus invariant, and no corpus target has a freeze file. Rather than add a target, the integration test runs the same unreachable-and-unexplained check as I7 on the #1741 fixture, in all three formats.
- **Rationale.** Adding a public corpus target costs a pin, a cache entry, goldens and a nightly lane, all for one file shape. A fixture is enough to prove the shape and is hermetic.
- **Alternatives.** Look for a public repository with a committed `cabal.project.freeze` and add it as a target: deferred. If one is found later it can be filed as an issue.

## R8 — C172's shape

- **Decision.** The keys are unchanged: `emitted`, `nixpkgs`, `revision`, `note`, and `requirement-ranges` only in the unresolved form. Only the note's text changes. The C172 row is updated to say the note names the source. Extractors aren't touched: the value is compared as one canonical object, so parity holds as long as all three emitters receive the same value. They do, because it is set once on the component.
