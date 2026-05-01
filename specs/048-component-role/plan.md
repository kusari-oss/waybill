# Implementation Plan: `mikebom:component-role` annotation

**Branch**: `048-component-role` | **Date**: 2026-04-30 | **Spec**: [spec.md](./spec.md)
**Input**: [spec.md](./spec.md)

## Summary

Add a path-heuristic-driven `mikebom:component-role` annotation
that classifies each resolved component as `build-tool`,
`language-runtime`, or absent. Annotation rides through the
existing `ResolvedComponent.extra_annotations` bag and the
established CDX-property + SPDX-2.3-annotation + SPDX-3-annotation
emission triple — exactly the shape the C-row catalog
documents for every other per-component annotation
(C14 `mikebom:detected-go`, C24–C39 binary-identity rows, etc.).

The classifier is a small standalone module with a curated
path-prefix table and a tiny single-segment-glob matcher. It
runs once per scan in a post-dedup pass over the resolved
components — late enough that all per-ecosystem `occurrences[]`
populations have completed, early enough that every serializer
sees the annotation through the existing `extra_annotations`
flow.

## Technical Context

**Language/Version**: Rust stable.
**Primary Dependencies**: existing only — `std::path`,
`serde_json`. No new crates.
**Storage**: N/A.
**Testing**: unit tests on the classifier (path table coverage,
glob matching, three-state semantics); inline test on the
post-dedup pass (synthetic `ResolvedComponent` with mocked
occurrences); existing `holistic_parity` regression
gate continues to enforce SymmetricEqual on the new C-row.
**Target Platform**: cross-platform.
**Project Type**: code-modifying milestone (resolve pipeline +
parity wiring + docs).
**Performance Goals**: O(components × N_paths × N_heuristics) at
classify time. N_heuristics is small (<10); N_paths per
component is typically 1–20 (occurrences). Negligible compared
to the existing per-component work.
**Constraints**: zero diff on existing 27 byte-identity goldens
(per audit, none of the existing synthetic fixtures contain
paths matching the heuristic — the milestone is purely
additive). `holistic_parity` 11/11 must remain green with the
new C-row participating in the SymmetricEqual matrix.
**Scale/Scope**: ~150 LOC of Rust + 1 new C-row + 3 parity
extractor invocations + 1 EXTRACTORS row.

No NEEDS CLARIFICATION markers — Phase 0 research below resolves
all four plan-level details flagged in /speckit.clarify.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1
design.*

| Principle | Engaged? | Status |
|---|---|---|
| I. Pure Rust, Zero C | Yes (Rust-only edits) | ✅ |
| II. eBPF-Only Observation | No — no discovery code change; this is enrichment over already-discovered components | ✅ vacuous |
| III–IV. (existing principles) | No code change to scan / generate hot path | ✅ vacuous |
| V. Specification Compliance (SPDX 2.3 / 3.x labeling) | Yes — adds a `mikebom:` annotation in the established envelope shape per the existing parity-extractors framework | ✅ |
| VI. Three-crate architecture | No `mikebom-common` / `mikebom-ebpf` change beyond reading `ResolvedComponent.occurrences[]` | ✅ untouched |
| Pre-PR Verification | All commits' `./scripts/pre-pr.sh` clean | ✅ enforced by SC-006 |

No gate violations.

## Phase 0: Research (resolved inline)

`/speckit.clarify` flagged four plan-level lookups; recon
resolved all four:

### R1. Hook point: post-dedup pass in `resolve/`

**Decision**: insert a post-dedup classifier pass after
`mikebom-cli/src/resolve/deduplicator.rs::deduplicate()`
returns. The classifier iterates each `ResolvedComponent`,
inspects `component.occurrences[].location`, applies the
heuristic, and inserts
`("mikebom:component-role", json!("build-tool"|"language-runtime"))`
into `component.extra_annotations` when a heuristic hits.

**Rationale**: the role is cross-cutting (one heuristic for all
ecosystems) and depends on `occurrences[]` being fully
populated. Per-ecosystem hooks like
`mikebom:detected-go` (binary scanner) or
`mikebom:dev-dependency` (per-ecosystem parser) live earlier in
the pipeline because their data is ecosystem-local. Component-
role's data is the resolved component's full occurrence set,
which only exists post-dedup.

### R2. Path-heuristic matcher

**Decision**: tiny inline helper with single-segment-glob
support. Pattern `/usr/lib/jvm/*/lib/` matches any path whose
first 5 segments are `usr`, `lib`, `jvm`, `<anything>`, `lib`,
followed by zero-or-more additional path components.

**Rationale**: spec calls for `*` matching one path segment.
No regex engine needed; a hand-rolled segment-by-segment
comparison is ~20 LOC, dependency-free, and fully testable. The
pattern table itself is `&[(&str, ComponentRole)]` — about 10
entries.

**Alternatives considered**:
- `glob` crate or similar — over-engineered for a 10-entry
  curated table.
- `regex` — same critique; adds a heavy dep for trivial
  matching.

### R3. Catalog row C-number

**Decision**: **C40** for `mikebom:component-role`. Latest C-row
in the catalog is C39 (`mikebom:macho-codesign-team-id`).

### R4. Parity-extractor wiring

**Decision**: follow the established pattern verbatim. Three
macro invocations (one per format file) + one row in the
`EXTRACTORS` table at `mikebom-cli/src/parity/extractors/mod.rs`:

```rust
// cdx.rs
cdx_anno!(c40_cdx, "mikebom:component-role", component);
// spdx2.rs
spdx23_anno!(c40_spdx23, "mikebom:component-role", component);
// spdx3.rs
spdx3_anno!(c40_spdx3, "mikebom:component-role", component);
// mod.rs EXTRACTORS table
ParityExtractor {
    row_id: "C40",
    label: "mikebom:component-role",
    cdx: c40_cdx, spdx23: c40_spdx23, spdx3: c40_spdx3,
    directional: SymmetricEqual,
}
```

The `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros already
read `extra_annotations` keyed by the annotation name, so the
new row picks up emission automatically — the
`generate/cyclonedx/builder.rs:500` loop iterates
`extra_annotations` generically.

## Approach

Three commits, ordered so the classifier (and its emissions)
land before the parity wiring (which validates them).

### Commit 1 — `feat(048/us1+us2)` — classifier + post-dedup hook

Touched files:

- New module **`mikebom-cli/src/resolve/component_role.rs`** —
  ~80 LOC:
  - `pub enum ComponentRole { BuildTool, LanguageRuntime }` with
    a `pub fn as_str(&self) -> &'static str` returning
    `"build-tool"` / `"language-runtime"` (the values that
    appear in the annotation).
  - `const HEURISTIC_TABLE: &[(&str, ComponentRole)]` listing
    the curated path-prefix patterns from FR-002 + FR-005:
    - `("/usr/share/maven/lib/", BuildTool)`
    - `("/usr/share/gradle/lib/", BuildTool)`
    - `("/opt/sbt/", BuildTool)`
    - `("/usr/lib/jvm/*/lib/", LanguageRuntime)`
    - `("/usr/lib/node_modules/", LanguageRuntime)`
    - `("/usr/lib/python*/site-packages/", LanguageRuntime)`
    - `("/usr/lib/python*/dist-packages/", LanguageRuntime)`
  - `pub fn classify(occurrences: &[FileOccurrence]) -> Option<ComponentRole>`
    iterates the component's occurrences (in the natural order
    they're stored — already deterministic), returning the role
    of the first heuristic-matched path.
  - `fn matches_pattern(pattern: &str, path: &str) -> bool` —
    the single-segment-glob matcher: split pattern + path on
    `/`, walk segments, treat any pattern segment containing
    `*` as a glob (matches any one path segment, with optional
    prefix/suffix literals like `python*` or `jvm*`).
  - Inline `#[cfg(test)] mod tests` covering: each heuristic-
    table entry hits its expected paths; absence of match
    returns None; deeper paths under a matched prefix still
    classify; multiple-occurrences with mixed roles return the
    first hit.

- **`mikebom-cli/src/resolve/mod.rs`** — register the new
  module: `pub mod component_role;`.

- **`mikebom-cli/src/resolve/deduplicator.rs`** (or wherever
  `deduplicate()` returns the final component list) — after
  dedup, run the classifier pass:
  ```rust
  for component in &mut components {
      if let Some(role) = component_role::classify(&component.occurrences) {
          component.extra_annotations.insert(
              "mikebom:component-role".to_string(),
              serde_json::Value::String(role.as_str().to_string()),
          );
      }
  }
  ```

- Inline test in `deduplicator.rs::tests` (or alongside the
  classifier) that constructs a synthetic `ResolvedComponent`
  with `occurrences[0].location = "/usr/share/maven/lib/foo.jar"`,
  runs the post-dedup pass, asserts the annotation appears in
  `extra_annotations`.

Verification:
- `cargo +stable test -p mikebom -- resolve::component_role` —
  all unit tests pass.
- `./scripts/pre-pr.sh` clean.
- `git diff main..HEAD -- mikebom-cli/tests/fixtures/golden/` —
  empty (no existing fixture has heuristic-matched paths).

### Commit 2 — `feat(048/us3)` — catalog row + parity extractors

Touched files:

- **`docs/reference/sbom-format-mapping.md`** — add C40 row
  after C39, classified `Present` × 3 formats × `SymmetricEqual`,
  description naming the open-enum semantics + three-state
  rule (`build-tool`, `language-runtime`, absent — absence ≠
  application).
- **`mikebom-cli/src/parity/extractors/cdx.rs`** — one line:
  `cdx_anno!(c40_cdx, "mikebom:component-role", component);`
- **`mikebom-cli/src/parity/extractors/spdx2.rs`** — one line:
  `spdx23_anno!(c40_spdx23, "mikebom:component-role", component);`
- **`mikebom-cli/src/parity/extractors/spdx3.rs`** — one line:
  `spdx3_anno!(c40_spdx3, "mikebom:component-role", component);`
- **`mikebom-cli/src/parity/extractors/mod.rs`** — one row in
  the `EXTRACTORS` table for C40 plus the three function
  imports.

Verification:
- `cargo +stable test -p mikebom --test holistic_parity` —
  11/11 ok with the new C40 participating.
- The existing `every_catalog_row_has_an_extractor` test (or
  equivalent) passes with C40 registered for all three
  formats.
- `./scripts/pre-pr.sh` clean.

### Commit 3 — `chore(048)` — CHANGELOG + spec scaffolding

- `CHANGELOG.md` `[Unreleased]` entry under "Added": one line
  documenting the new `mikebom:component-role` annotation, what
  it tags, and the three-state semantics.
- `specs/048-component-role/` — spec.md + plan.md + tasks.md +
  checklists/requirements.md.
- `CLAUDE.md` if `update-agent-context.sh` produced changes.

## Touched files

| File | Commit | LOC |
|---|---|---|
| `mikebom-cli/src/resolve/component_role.rs` (new) | 1 | ~80 + ~50 tests |
| `mikebom-cli/src/resolve/mod.rs` | 1 | +1 (`pub mod component_role;`) |
| `mikebom-cli/src/resolve/deduplicator.rs` | 1 | +5 (post-dedup pass) + ~20 tests |
| `docs/reference/sbom-format-mapping.md` | 2 | +1 row |
| `mikebom-cli/src/parity/extractors/cdx.rs` | 2 | +1 |
| `mikebom-cli/src/parity/extractors/spdx2.rs` | 2 | +1 |
| `mikebom-cli/src/parity/extractors/spdx3.rs` | 2 | +1 |
| `mikebom-cli/src/parity/extractors/mod.rs` | 2 | +1 row + 3 imports |
| `CHANGELOG.md` | 3 | +1 entry |
| `specs/048-component-role/` | 3 | scaffolding |

Total: ~150 LOC of Rust + ~50 LOC of tests + ~10 LOC across
catalog/parity wiring + scaffolding.

## Risks

- **R1: Heuristic false positives on rare path conventions.**
  Some applications ship under `/usr/share/maven/lib/` by
  convention (extremely uncommon). Acceptable tradeoff per
  spec — three-state semantics (annotation = "build-tool"
  doesn't claim application code is impossible there) lets
  consumers handle case-by-case via `severity: advisory` in
  conformance GTs. Mitigation: keep the heuristic table small
  and curated; add new entries only when real fixtures surface.
- **R2: Goldens regen surprises.** Plan claims zero golden diff.
  If a synthetic fixture turns out to have a heuristic-matched
  path (audit said no, but worth a final check), goldens regen
  cleanly with the new annotation. Mitigation: post-commit-1
  inspection of `git diff -- mikebom-cli/tests/fixtures/golden/`
  before committing. If unexpected diffs appear, the fix is to
  accept them (the new annotation is correct emission).
- **R3: `extra_annotations` insertion ordering.** The
  `BTreeMap<String, Value>` collector keeps annotations sorted
  by key, so insertion order doesn't matter. Goldens regen
  deterministically.
- **R4: Single-segment-glob matcher edge cases.** The
  hand-rolled matcher must handle: `*` alone (matches any one
  segment), `python*` (prefix glob — matches segments starting
  with `python`), `*-debian` (suffix glob — matches segments
  ending with `-debian`), and literal segments. Inline tests
  cover each shape; standard issue if any case is missed
  surfaces immediately on goldens or unit tests.

## Phasing

| Phase | Commits | Effort |
|---|---|---|
| Setup + recon | done (Phase 0 above) | 0 |
| Commit 1 (classifier + hook) | 1 | 1.5 hr |
| Commit 2 (catalog + parity) | 1 | 30 min |
| Commit 3 (CHANGELOG + scaffold) | 1 | 10 min |
| Verify + PR | 0 | 15 min |
| **Total** | **3 commits** | **~2.5 hr** |

## What this milestone does NOT do

- Does NOT consume the new annotation in the conformance suite
  (GT updates, `severity: advisory` declarations, etc.). That
  belongs in the sbom-conformance repo's follow-on work — it's
  the consumer side of this milestone's emission side.
- Does NOT add per-occurrence role tagging. Component-level
  only; multiple-path components get the role of the first
  heuristic-matched occurrence.
- Does NOT extend the heuristic table beyond Maven / Gradle /
  sbt / JDK / system Python / system Node. Future fixtures
  that surface other build-tool / runtime paths get
  mechanical follow-on PRs adding to the table.
- Does NOT introduce package-metadata-based classification
  (e.g., recognizing Maven's own JARs by `groupId`). Filesystem
  path is the heuristic; package-metadata is a separable axis
  if needed later.
- Does NOT change CLI flags, `--include-declared-deps`
  semantics, or `mikebom:sbom-tier` values. The
  `component-role` annotation is a new orthogonal axis from
  the document-level `metadata.lifecycles[]` and per-component
  `mikebom:sbom-tier` axes shipped in milestones 047 + earlier.

## Why no `data-model.md` / `contracts/` / `quickstart.md`

Same rationale milestones 021 / 022 / 023 / 042 / 046 / 047 used
(the project's tighter 4-file template):

- `data-model.md`: one new tiny enum (`ComponentRole {
  BuildTool, LanguageRuntime }`) with two static-string
  values. Spec.md FR-001 / FR-004 + this plan's R1 already
  document the shape.
- `contracts/`: the public-API change is one new C-row in the
  format-mapping catalog. Spec FR-006 + the catalog row itself
  fully specify the contract; no separate file needed.
- `quickstart.md`: spec's User Stories include
  acceptance-scenario verifications that read like quickstart
  steps (jq commands, cargo test invocations). Duplicating
  noise.

This is the seventh use of the tighter 4-file template — pattern
stable for genuinely contained, additive milestones.
