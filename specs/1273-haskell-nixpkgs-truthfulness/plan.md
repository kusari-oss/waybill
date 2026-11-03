# Implementation Plan: Haskell/nixpkgs resolution correctness

**Branch**: `1273-haskell-nixpkgs-truthfulness` | **Date**: 2026-11-02
**Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/1273-haskell-nixpkgs-truthfulness/spec.md`

## Summary

Four open issues against the Haskell reader and the nixpkgs resolution pass,
all one defect class: **the pass does not tell the truth about what it did.**
An hpack-only project emits nothing and claims completeness (#1278); the
optional-mark closure is seeded from the components it is asked to confirm
(#1281); resolved components carry no license though the file the pass
already reads declares one for every package (#1280); and two components are
version-resolved while still carrying an unresolved-reason (#1279).

**Technical approach.** Phase 0 measured the one external behaviour the
design turns on, and the measurement decided the architecture. hpack does
not evaluate `when:` conditions — it lowers them to Cabal `if` blocks, even
for a `flag(x)` whose default is declared in the same file. So a `when:`
block *is* a Cabal conditional, and waybill already evaluates those against
flag defaults. The hpack reader therefore **lowers `package.yaml` into the
existing `CabalManifest` representation** and every downstream pass — scope
assignment, flag-default evaluation, the optional mark, the C151 reason —
runs unchanged. A second conditional evaluator would let one project get two
answers depending on which file declared its dependencies, which is the
defect class this milestone exists to remove.

The other three are narrower. The withdrawal gets a second closure seeded
from the declared set with the marked names removed, which is the only
formulation correct at every depth. The license is one more field parsed out
of a file the pass already opens, through a 15-entry mapping table. The
stale reason is stripped where the version is assigned, not in a sweep.

**Zero new Cargo dependencies, no subprocess, no network, no CLI flag.**

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched — user-space only.

**Primary Dependencies**: existing only.
- `serde_yaml` — already a direct dependency, already parsing `stack.yaml.lock`
  in `haskell.rs`; parses `package.yaml`.
- `regex` — already scanning `hackage-packages.nix` (`version_re`, `sha256_re`);
  one more pattern for the license field.
- `spdx` via `waybill_common::types::license::SpdxExpression::try_canonical` —
  already used by the `.cabal` license path (milestone 954).
- `tracing`, `anyhow`, `serde_json` — pervasive.

**Zero new Cargo dependencies** (FR-021, and no `hpack` binary: FR-022).

**Storage**: N/A for new state — all in-process per scan, as with every reader
since milestone 002. The milestone-926 per-revision nixpkgs cache is read, and
its **on-disk format is unchanged**: this parses one more field out of a file
it already downloads.

**Testing**: `cargo clippy --workspace --all-targets` and
`cargo test --workspace --no-fail-fast`, via
`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.
No `+toolchain` argument (#1189). Mutation-tested per SC-006.

**Target Platform**: Linux, macOS, Windows — scan mode, filesystem reads only.

**Project Type**: CLI / library (the three-crate workspace, Principle VI).

**Performance Goals**: no regression. The one added traversal is a second BFS
over an already-parsed in-memory package set; the existing walk reports its
size as `relations_walked`. No new I/O, no new fetch.

**Constraints**: `--offline` must stay honoured (FR-001c, SC-010). No behaviour
may vary with host `PATH` (FR-022). Every other ecosystem's dev-axis and
optional-axis output byte-identical (FR-015, SC-007).

**Scale/Scope**: 4 user stories, 27 functional requirements, 10 success
criteria, 27 document/CLI contracts. Measured blast radius: 3 of 4 Haskell/Nix
corpus goldens change, 1 is a control.

**NEEDS CLARIFICATION**: none. The spec entered Phase 0 with zero markers after
`/speckit.clarify`, and Phase 0 discharged both of its measurement obligations
(FR-001a's hpack semantics; SC-003's inherited figure).

## Constitution Check

*Constitution v4.0.1. Gate evaluated before Phase 0 and re-evaluated after
Phase 1 design; both results below.*

| principle | verdict | note |
|---|---|---|
| **Operating Modes** | PASS | Scan mode throughout. `waybill:generation-context` unchanged (`filesystem-scan`). No crossover: nothing here touches the trace path. |
| **I. Pure Rust, Statically Linked** | PASS | No new dependency of any kind. No C, no FFI, no dynamic linkage. FR-021/FR-022 forbid even an *optional* `hpack` subprocess, which is stricter than the principle requires. |
| **II. eBPF-Only Observation** | N/A | Trace-mode principle; this is scan mode. |
| **III. Fail Closed** | PASS | The scan-mode analogue — *report, never pad* — is the whole milestone. An unread manifest reports (C8) instead of yielding a confident empty answer. |
| **IV. Type-Driven Correctness** | PASS | `UnreadableReason`, `UnmodelledConstruct`, `DeclaredLicense`, `ManifestOrigin` are enums, not strings; `DeclaredLicense` makes FR-011's "uncanonicalised" a type state rather than a convention. Serialized only at the emission boundary. |
| **V. Specification Compliance** | PASS — **audited** | See the standards-native audit below. No new `waybill:*` key, no new catalog row. |
| **VI. Three-Crate Architecture** | PASS | Changes confined to `waybill-cli`. No crate added, split or renamed. |
| **VII. Test Isolation** | PASS — **with a named trap** | No new env-var mutation, so no `EnvGuard` participation needed. Fixtures are per-test temp dirs. The probes under `measurements/` read the shared nixpkgs cache **read-only**; **no test may.** `/speckit.analyze` caught the first task breakdown extending that probe-only licence into the test suite, which would have passed locally and failed on CI. The pattern to reuse is `seed_cache()` at `waybill-cli/tests/nix_haskell_resolution_m926.rs:116`: a synthetic `hackage-packages.nix` of `waybill-fixture-*` names written into a `tempfile::tempdir()`, with `WAYBILL_NIXPKGS_CACHE` pointed at it — and at `__no_cache__` for the negative case, because "an unset cache would fall back to the real `$HOME`". That file also records why none of its tests is `#[ignore]`d: "an ignored test is one nobody runs, and this project has already been bitten by a suite that was green because it was not looking (#949)". |
| **VIII. Completeness** | PASS | Directly served: #1278 is a false-negative surface (every declared dependency absent) *plus* a completeness claim over it. FR-005 restores the Principle X signal the empty set currently suppresses. |
| **IX. Accuracy** | PASS — **and the sharper half** | US2 is a false-*positive* fix: 10 components presented as runtime dependencies that the project's own Nix build under default flags does not contain. FR-012/FR-011a also forbid inventing a license, including the string `unknown`. |
| **X. Transparency** | PASS | FR-018 requires every signal in the *document*, not only the log — which is this principle stated as the milestone's premise. FR-019 keeps the carriers spec-native/existing. |
| **XI. Enrichment** | PASS | US3 is enrichment of exactly the named kind ("license data … package-embedded license files"), from a source already fetched. Degrades to an absent field with no delay, per the principle's closing clause. |
| **XII. External Data Source Enrichment** | PASS | Scan mode, so resolving what the evidence declares is permitted. Constraint 2 (provenance) is met: the nixpkgs document-scope records already name the revision, and the license inherits that provenance. Constraint 1 is respected — no component is introduced by this change. Constraint 3: an uncached revision under `--offline` degrades with a reason, unchanged. |
| **Strict Boundary 1** (no lockfile discovery in trace mode) | N/A | Scan mode. |
| **Pre-PR Verification** | PASS | Both commands, no `+toolchain`, via `gate.sh` with `--expect`. |

### Principle V — standards-native audit (required, and cited per the principle)

The principle requires every spec to audit each target format for an existing
native construct before introducing a `waybill:*` key, and requires reviewers
to reject specs that do not cite the result. FR-019 carries this; here is the
audit.

| signal | native construct? | carrier chosen |
|---|---|---|
| a manifest went unread; completeness not assertable | **No.** CycloneDX has no document-level completeness field; SPDX 2.3's `filesAnalyzed` is file-scoped and SPDX 3's `completeness` applies to relationships, not to "a reader did not run" | existing `waybill:graph-completeness` (milestone 158), via a **new `ReasonCode` enum variant**. The variant renders through the existing annotation value, so this adds a reason code and **not** a catalog row |
| why a component has no version | **No** native per-component "unresolved because" field in any of the three | existing `waybill:unresolved-reason`, catalog row **C151** (milestone 236) |
| the withdrawal account | **No.** CDX `scope`/SPDX `LifecycleScopeType` carry the *scope*, not the derivation of a change to it | existing nixpkgs document-scope records (**C173**/**C175**), extended additively |
| a component's license | **Yes** — CDX `component.licenses[]`, SPDX 2.3 `PackageLicenseDeclared`, SPDX 3 `simplelicensing_LicenseExpression` | the **native** field in all three. This is the principle's primary case and the reason US3 adds no annotation for the license itself |
| that a license is uncanonicalised | **Partly.** The native fields take an expression; none has a "this did not canonicalise" flag. Milestone 153's `LicenseRef-` handling is the nearest existing mechanism | native field for the value; the uncanonicalised fact rides the existing `waybill:unresolved-reason` vocabulary rather than a new key |

**Result: no new `waybill:*` property and no new catalog row.** Every signal
lands in a native field or in an annotation that already has extractors in
all three formats. Should a row nonetheless prove necessary, its extractor
ships in the same change — `parity::extractors::tests::every_catalog_row_has_an_extractor`
and `holistic_parity` enforce it, and a documentation row ahead of emission
code is a known way to break them.

### Post-Phase-1 re-evaluation

Re-checked after `data-model.md` and `contracts/README.md`. **No new
violations, and one gate tightened rather than loosened**: the Phase 0
license measurement (Probe B) showed the cached set's `"unknown"` value
would have been published as a license under FR-011 as written. That is a
Principle IX (Accuracy) breach the design would have shipped, so FR-011a
was added to the spec to forbid it. The gate found something.

No deviations to record. This milestone adds no dependency, no flag, no
subprocess, no network call, no catalog row and no crate.

## Project Structure

### Documentation (this feature)

```text
specs/1273-haskell-nixpkgs-truthfulness/
├── plan.md                     # this file
├── spec.md                     # 27 FRs, 10 SCs, 0 markers
├── research.md                 # Phase 0 decisions R1–R8
├── data-model.md               # Phase 1
├── quickstart.md               # Phase 1
├── contracts/
│   └── README.md               # C1–C27, document + CLI contracts
├── checklists/
│   └── requirements.md         # 16/16
└── measurements/
    ├── findings.md             # Probes A–E
    ├── reproduce-1273.sh       # all four issues
    ├── hpack-semantics.sh      # Probe A — FR-001a's obligation
    ├── license-shapes.sh       # Probe B — US3's declaration shapes
    ├── sc003-public-library.sh # Probe D — SC-003, re-measured
    └── fanout.py               # Probe D's reachability analysis
```

### Source (repository root)

```text
waybill-cli/src/
├── scan_fs/package_db/
│   ├── haskell.rs                      # US1: lowering, suppression, FR-004a
│   │                                   #   (3302 lines; the hpack parser lands
│   │                                   #    in a sibling module, not in here)
│   ├── haskell/
│   │   └── hpack.rs                    # NEW — parse + lower package.yaml
│   └── nix/haskell_packages/
│       ├── mod.rs                      # US2 withdrawal; US4 reason stripping
│       ├── package_set.rs              # US3 license field + classifier
│       └── closure.rs                  # US2: the re-seeded walk
└── generate/graph_completeness/
    ├── mod.rs                          # FR-005 — the trivially_complete site
    └── reason_codes.rs                 # FR-005 — one new variant

waybill-cli/tests/                      # new integration suites, per story
waybill-cli/tests/fixtures/haskell_hpack/   # NEW fixtures (synthetic names)
```

**Structure decision**: the hpack parser is a new sibling module rather than
an addition to `haskell.rs`, which is already 3302 lines. `haskell.rs` keeps
the discovery, the lowering call site, the suppression decision and the
warning; `hpack.rs` owns the wire types and the lowering function. The
split follows the existing `nix/haskell_packages/` precedent, where parsing,
caching, the closure and boot libraries are separate files under one module.

## Phase 0 — research (COMPLETE)

Output: [`research.md`](./research.md), [`measurements/findings.md`](./measurements/findings.md).

Both measurement obligations the spec states rather than assumes are
discharged, and both changed the spec.

| probe | subject | outcome |
|---|---|---|
| **A** | hpack 0.38.3's rendering rules, 12 cases | **Decided the architecture.** `when:` is not evaluated → lower into `CabalManifest`. Also: inheritance reaches tests/benchmarks; a repeated name is last-wins with **no intersection**; hpack rewrites ranges and waybill must not |
| **B** | the cached package set's license declarations | 19,437 declarations, all single-line, partitioning exactly into 3 families; the attribute vocabulary is **15 names**; `"unknown"` is 360 → **produced FR-011a** |
| **C** | the four issues at 0.14.0 | the spec's Context table |
| **D** | SC-003's inherited figure | **8 → 10**, and a different composition. The figure cannot be read as a classification disagreement at all |
| **E** | which corpus goldens churn | **4** Haskell/Nix targets, not 0; `nix-closure-moat` is a live #1278 witness; `haskell-aeson` is a control |

Three corrections Phase 0 made to claims drafted from inference:

1. **SC-003's 8 is 10.** The one figure in the spec taken from an issue
   rather than re-run. Corrected in `spec.md` against Probe D.
2. **The corpus does have Haskell targets.** An earlier `research.md` §R7
   asserted it did not, by analogy with milestone 1272's npm work. Four
   targets with committed goldens.
3. **`package.yaml` in the goldens is a reason string, not a manifest.** The
   55/53/19 occurrences are the value of `REASON_CABAL_NO_FREEZE`. The
   pinned trees had to be queried directly.

Also recorded: the hpack probe's own first version lied. It filtered the
rendered `.cabal` through awk blind to the deeper indent of a Cabal `if`
block, so the three `when:` cases printed identically when their whole
subject is that they differ. Cases that must differ reading the same is the
signature of a broken harness.

## Phase 1 — design (COMPLETE)

Output: [`data-model.md`](./data-model.md),
[`contracts/README.md`](./contracts/README.md),
[`quickstart.md`](./quickstart.md).

### Design decisions, each traceable to a measurement

| decision | measurement that forced it |
|---|---|
| lower `package.yaml` into `CabalManifest`; add no second evaluator | A5 — hpack lowers `when:` to a Cabal `if` and does not evaluate `flag()` even against a default in the same file |
| apply the top-level list to `tests:`/`benchmarks:` and let `stanza_lifecycle_scope` assign `Development` | A1 — inheritance reaches all four stanza kinds |
| last-wins on a repeated name; **never** intersect | A3 — `text >= 1.0` + `text < 3` renders `text <3` |
| record the range as declared | A4 — hpack rewrites `>= 4 && < 5` to `==4.*`; that is cosmetic |
| read `source-dirs` for evidence only | A6 — measured incapable of changing the dependency set |
| announce `defaults:`; never fetch | A7 — hpack itself refuses the whole manifest offline, so no subprocess tier could do better |
| re-seed the closure rather than test parents locally | D2 — 8 non-boot components fan out below the marked pair, each with an unmarked parent, so a parent-local test withdraws nothing and still publishes all 8 |
| 15-entry attribute table; line-oriented license parse | B1, and B's assertion of **zero** multi-line/list forms |
| `"unknown"` → absent, not uncanonicalised | B3 — 360 of 994, and it is not a license |
| strip C151 where the version is assigned | R5 — the reason is stale, not wrong; a sweep would have to decide for every other reader's C151 (FR-015 forbids) |

### Where the four stories meet, and what it means for sequencing

They are not independent, and the plan says so rather than discovering it.

- **US1 → US2.** An hpack-only project with a default-off `when: flag(...)`
  block gets an optional mark (A5), and that mark then goes through US2's
  closure. A fixture exercising both exists by construction.
- **US2 → US4.** The 2 components that are both resolved and
  unresolved-reasoned are the same 2 US2 is about. The spec's own assumption
  requires US4's remaining population be measured **after** US2 lands.
- **US3 ⟂ the rest.** The only genuinely independent story: it reads one
  more field out of a file the pass already opens.

So: US1 and US2 can proceed in parallel; US4's *measurement* task depends on
US2's implementation; US3 is parallel to everything.

### Risks the design carries deliberately

| risk | why accepted, and the guard |
|---|---|
| **Three corpus goldens churn.** The refresh cannot be driven locally | Measured in Probe E rather than discovered in a red nightly. Its own task, through `workflow_dispatch`. `haskell-aeson` is the control (C27): if it changes, the blast radius is wrong |
| **`haskell-language-server`'s only `package.yaml` is someone else's test data** | The walker does not exclude test data, and excluding it would be a scope change this milestone has no mandate for. It exercises FR-002's suppression, which is a real path; named in advance |
| **The optional seed is the shape that bit milestone 1272** | Folding an extra section into a runtime seed silently reassigned Optional → Development there. R7 and C25 both name it; the Haskell dev-scope tests must run unchanged |
| **A diagnostic that no input can trigger** | FR-017 and C26. This is the defect class, so reintroducing it while fixing it is the worst outcome. Each diagnostic gets a fixture whose sole purpose is to fire it, mutation-tested (SC-006) — a passing test may exercise nothing |

### Agent context

`.specify/scripts/bash/update-agent-context.sh claude` — run below; adds this
milestone's technology line to `CLAUDE.md` between the managed markers,
preserving manual additions.

## What `/speckit.tasks` has to produce

Not tasks (that is the next command's job) — the constraints the task
breakdown must satisfy:

1. **A unit test settling the `!include` question first.** `data-model.md`
   labels as *inference* that `serde_yaml` rejects an unresolved `!include`
   tag rather than yielding a value. The two cases need different code;
   guessing puts the wrong one in.
2. **Phase 0's probes re-run in CI or on demand, not re-derived.** SC-008
   requires every committed probe reproduce its recorded finding.
3. **US4's measurement task sequenced after US2's implementation**, per the
   spec's assumption.
4. **A mutation step per requirement**, not per story. SC-006 is explicit
   that two of the four defects are cases where the existing tests passed
   throughout, so "the tests are green" is not evidence here.
5. **A corpus-refresh task** through `workflow_dispatch`, asserting
   `haskell-aeson` unchanged.
6. **A task for FR-004a's warning text**, separate from FR-002's
   suppression. Measurement shows they are different code paths, and the
   warning is the one an operator reads.

## Stop

Planning complete. `/speckit.tasks` next.
