# Phase 0 research: every unreachable component says why it has no edge

**Feature**: `1218-explain-every-orphan` | **Date**: 2026-10-19
**Spec**: [spec.md](./spec.md) | **Measurements**:
[measurements/i7-populations.md](./measurements/i7-populations.md)

Every claim below was read out of the tree at `main` `1311224e`, not
recalled. Line numbers are cited so the next reader can re-verify, and so a
stale one is visibly stale.

---

## R1 — The classifier is not scope-gated

**Decision**: no scope plumbing is needed; the existing single call site is
the whole integration point.

**Rationale**: `classify_orphans_pre_emit` is called exactly once, from
`waybill-cli/src/cli/scan_cmd.rs:4940`, inside a bare block with no
surrounding `match scope` or `if`. Its doc comment at the call site states
the design: it mutates "the shared `components` Vec every emitter (CDX /
SPDX 2.3 / SPDX 3) reads", so one classification serves all three formats.

**Why this was checked first**: feature 1217's FR-008a exists because
`report.rs::build_scoped` gates `unfalsifiable` on
`match scope { Corpus => …, SingleTarget => Vec::new() }` three lines above
where a sibling pass goes, and the lane that would notice a tidy-up is
network-gated. That asymmetry has **no analogue here**. Confirmed by
reading the call site rather than assuming the shape repeats.

**Alternatives considered**: none — this is an observation, not a choice.

---

## R2 — What the classifier can see, and in what form

**Decision**: the three predicates read `c.sbom_tier` (struct field),
`c.evidence.source_file_paths` (struct field, **raw**), and
`c.extra_annotations["waybill:workspace-member"]` /
`["waybill:source-mechanism"]` (annotation map). No new plumbing.

**Rationale**: measured per input.

| Predicate input | Where it lives | Verified at |
|---|---|---|
| `sbom_tier` | `ResolvedComponent` struct field `Option<String>` | `waybill-common/src/resolution.rs:103` |
| source paths | `ResolutionEvidence.source_file_paths: Vec<String>` | `waybill-common/src/resolution.rs:335` |
| `waybill:workspace-member` | `extra_annotations`, JSON-array-**encoded string** | written `scan_fs/mod.rs:1663` |
| `waybill:source-mechanism` | `extra_annotations`, plain string | written e.g. `bazel.rs:322` |
| `waybill:orphan-reason` | `extra_annotations`, plain string | read `orphan_reason.rs` step 2b |

**Ordering is safe.** `tag_components_with_workspace_member` is called at
`scan_cmd.rs:4308`; the classifier at `scan_cmd.rs:4940`. 4308 < 4940, and
4308 is an unconditional block, so the annotation is present on every run
the classifier sees — not merely usually.

**The trap, and it is a real one**: `waybill:source-files` is **not** an
annotation. It is derived at emit time in
`generate/cyclonedx/builder.rs:1500-1509` by passing
`component.evidence.source_file_paths` through
`scan_fs::sbom_path::emitted_source_files(paths, roots)`, which relativizes
each path against the scan roots (`sbom_path.rs:122-124`). So the value this
feature's measurements observed — `["examples/celery/requirements.txt"]` —
is the **emitted** form, and the classifier sees the **raw** form, which may
be absolute or URI-shaped (`path+file://<abs>`, per the comment at
`scan_cmd.rs:4298-4300`).

This codebase has already been bitten by exactly this: the m226 Pants Go
reader's note records "Go reader emits scan-root-relative source_paths,
index stores absolute".

**Decision**: predicates MUST NOT string-match raw paths against the
emitted spellings used in the spec's tables. Either route raw paths through
`emitted_source_files` first, or match on **file name** via
`Path::file_name`, which is invariant under relativization. T-tasks name
which, and a unit test asserts both an absolute and a relative input
classify identically.

**Alternatives considered**: adding `waybill:source-files` to
`extra_annotations` at reader time so the classifier and the emitter read one
value. Rejected — it would duplicate a derived field into the annotation
bag for every component in every scan, and the parity extractor for C18
would then have two sources that could disagree.

---

## R3 — Extending C45 in place, and what that costs

**Decision**: add variants to the existing `OrphanReasonCode` enum, extend
`OrphanReasonCounts`, delete both ecosystem guards, and update C45's row.
No new catalog row, no new extractor.

**Rationale**: `orphan_reason.rs:71-91` defines `OrphanReasonCode` with five
variants and a frozen `as_str()`; `:96-121` defines `OrphanReasonCounts`
with one `usize` per code plus `tally()`. Adding a mechanism is: one enum
variant, one `as_str()` arm, one counts field, one `tally()` arm, one
predicate. The annotation key is a single `const ANNOTATION_KEY` at `:38`,
so the wire name does not change and C45's extractor keeps working — it
extracts a string property by name and does not enumerate values.

The two guards to remove are at `:161` (sibling-index build) and `:176`
(classification loop), both the identical
`if ecosystem != "npm" && ecosystem != "golang" { continue; }`.

**C45's row text must change in three ways**, not one:
1. Delete *"Emission scope is restricted to `pkg:golang/*` and `pkg:npm/*`
   ecosystems (FR-001); other ecosystems continue to carry no
   orphan-reason."* — this feature falsifies it (spec FR-001b).
2. Enumerate the new values in the open-enum list.
3. Extend the stated priority order, which the row already documents for
   m167 ("FR-005 priority order (most-specific to least-specific)…
   `flat-attached-fallback` sits above the priority order").

**Alternatives considered**: a new annotation name with its own C-row. This
was Q1 option C and was rejected by the user. Recording why it would have
cost more than it looks: `is_explained` in
`invariants/checks/orphan_explained.rs:31-37` tests five markers, one of
which is `waybill:orphan-reason`. A new name is a **sixth arm**, and the
spec's SC-014 now pins the arm count at five precisely so an implementation
that drifts back to this is caught.

---

## R4 — Mechanism B reuses `dead-lockfile-entry`, and the sibling index must widen with it

**Decision**: `dead-lockfile-entry` covers mechanism B. The sibling index
at `orphan_reason.rs:157-170` must be built over **all** ecosystems, not
just Go/npm.

**Rationale**: the existing classification is a pattern match on
`(ecosystem, has_reachable_sibling)`, and `dead-lockfile-entry` is the
`(npm, true)` arm — the multi-version case. Measured, **all ten** of
mechanism B have a reachable same-name sibling:

```
alabaster@0.7.16   SIBLING  alabaster@1.0.0
blinker@1.9.0      SIBLING  blinker@1.6.2
click@8.1.8        SIBLING  click@8.1.3
click@8.2.1        SIBLING  click@8.1.3
itsdangerous@2.2.0 SIBLING  itsdangerous@2.1.2
jinja2@3.1.6       SIBLING  jinja2@3.1.2
markupsafe@3.0.2   SIBLING  markupsafe@2.1.2
sphinx@7.4.7       SIBLING  sphinx@8.2.3
sphinx@8.1.3       SIBLING  sphinx@8.2.3
werkzeug@3.1.3     SIBLING  werkzeug@2.3.3
```

with reachable sibling: **10**, without: **0**. So B needs no no-sibling
companion code, and `dead-lockfile-entry`'s existing meaning fits without
strain.

**The non-obvious consequence**: deleting the guard at `:161` widens the
sibling index to every ecosystem, which is required for B — but it also
means a **maven** or **bazel** component can now find a same-name sibling.
`maven-guice` has exactly that: `guice@5.1.0` has a reachable sibling
`guice@7.0.0`. Under a naive widening those three would be classified
`dead-lockfile-entry`, which would **silently close #1235's cell** — the
failure mode spec SC-004 was written to catch.

**Decision**: the multi-version arm MUST be reached only for ecosystems
whose lockfile semantics it describes. The ordering rule in R5 handles
this, and SC-004 is the test.

`versionless_identity` at `:49-55` strips at the first `@` and is
ecosystem-neutral, so it needs no change. Its m866 doc comment explains why
the key is the versionless PURL rather than `Purl::name()` — worth not
re-breaking.

**Alternatives considered**: a new pypi-specific code. Rejected under Q1's
answer; the name `dead-lockfile-entry` says nothing about npm.

---

## R5 — The total order, and where it lives

**Decision**: extend the existing documented order; new codes sit **below**
all five m167 codes except `flat-attached-fallback`, which stays above
everything. Mechanism predicates are evaluated most-specific first:

1. `flat-attached-fallback` — preserved, never overwritten (m061 guard)
2. `stale-go-sum-entry` — Go, reachable sibling
3. `dead-lockfile-entry` — npm **or pypi-from-a-lockfile**, reachable sibling
4. `hoisted-unused` — npm, no sibling
5. `unresolved-indirect-require` — Go, no sibling
6. *(new)* toolchain pin — `design` tier, mechanism E
7. *(new)* foreign build system — mechanism C
8. *(new)* unreferenced lockfile node — mechanism D

**Rationale**: FR-004 requires every component carrying an m167 code today
to keep it. Placing new codes strictly below the m167 arms makes that
structural rather than a property to be tested for — a component that
matched an m167 arm before still matches it first. The order's home is
C45's row, which already states m167's.

E is placed above C and D because it is the only `design`-tier population
and the tier test is the most specific predicate available. C before D
because C's test (`source-mechanism` present, or a foreign root manifest)
is narrower than D's (a lockfile node nothing references).

**The ordering decision that protects #1235**: arm 3 must not fire for a
maven component. The predicate therefore tests the **source of the
version-bearing entry**, not merely "has a reachable sibling" — a lockfile
resolution fork, not a manifest-declared coordinate. `maven-guice`'s three
carry `waybill:source-type = workspace` and come from a `pom.xml`;
`python-flask`'s ten come from `uv.lock`. The distinguishing input is
available and measured.

**Alternatives considered**: placing new codes above the m167 ones so that
the "most specific" reading is semantic rather than historical. Rejected —
it puts FR-004 at risk for no gain, and makes the order's correctness
depend on a case analysis rather than on position.

---

## R6 — Mechanism E and why C151 is refused

**Decision**: a new C45 value. Not `waybill:unresolved-reason`.

**Rationale**: C151's row says it is emitted *"iff and only if the same
component carries `waybill:sbom-tier: design`"*. Measured, mechanism E's two
components are the only `design`-tier **offenders** — every other one of the
34 is `source`-tier.

**Correction, found by `/speckit.analyze`, kept visible rather than
quietly fixed.** An earlier draft inferred from that measurement that
"mechanism E is the only population where C151 is even available". The
inference is false and the error is instructive: *offender* means
unreachable **and unexplained**, so counting offenders by tier says nothing
about unreachable components that are already explained. Measured properly,
the corpus holds **six** unreachable `design`-tier components, all in
`haskell-language-server`:

```
pkg:generic/ghc-9.6.5@unspecified            mechanism E, unexplained
pkg:generic/stackage-lts-23.28@unspecified   mechanism E, unexplained
pkg:pypi/docutils           docs/requirements.txt, member ["docs"], has C151
pkg:pypi/myst-parser        docs/requirements.txt, member ["docs"], has C151
pkg:pypi/sphinx             docs/requirements.txt, member ["docs"], has C151
pkg:pypi/sphinx-rtd-theme   docs/requirements.txt, member ["docs"], has C151
```

So C151 is not "available" for E — it is **already in use** on four
`design`-tier neighbours, doing a different job, in the same document.

That strengthens the decision rather than weakening it, and it tightens the
predicate. A mechanism-E arm testing tier alone would fire on four
documentation dependencies and call them build tooling. Because those four
are already explained they are **not** offenders, so the probe's count would
not move and SC-001 through SC-004 would all still pass — an invisible false
assertion, which is the worst shape a defect can take here. FR-012 now
requires `design` tier **and** empty `source_file_paths` **and** absent
`workspace-member`; SC-018 checks those four by name.

It is still wrong. C151 asserts a resolution boundary was hit;
`pkg:generic/ghc-9.6.5@unspecified` is pinned exactly. C197's row draws
precisely this line — *"that row says resolution was attempted and did not
find a version, this row says there was never one to find, and a component
carrying both would assert its resolution failed and could not have been
attempted"* — and reusing C151 here would make it mean both. C151's values
are also documented *"display-only — downstream tools MUST render verbatim
and MUST NOT parse for programmatic branching"*, which is the wrong contract
for a fact a consumer acts on.

**The load-bearing corollary**: FR-015's extension of C151 to `bazel.rs` and
`nix/lockfile.rs` closes **no** I7 cell, because every offender from those
two readers is `source`-tier and C151 cannot attach. A plan that counted it
toward SC-001 would come up five components short. FR-015 is consistency
work for m236's inventory, and the spec now says so.

**Alternatives considered**: a new tier value (Q2 option C). Rejected — a
sixth `is_explained` arm against SC-014, and widening a taxonomy 305+
components depend on does not belong inside an annotation feature.

---

## R7 — C55 on the pip reader

**Decision**: the pip reader emits `waybill:source-mechanism` for
`requirements.txt`-sourced components. C55's row and extractor already
exist; nothing is added to the catalog.

**Rationale**: measured, mechanism C's three components identify their
build system unevenly — the bazel pair carries
`waybill:source-mechanism = bazel-http-archive`, and `codespell` carries
nothing, so a consumer must parse the string `"requirements.txt"` to learn
it is pip. `grep -c 'waybill:source-mechanism' waybill-cli/src/scan_fs/package_db/pip/*.rs`
returns 0 in every file.

The insert pattern is three lines, established across many readers —
`bazel.rs:322-325` is the model:

```rust
extra_annotations.insert(
    "waybill:source-mechanism".to_string(),
    serde_json::json!("bazel-http-archive"),
);
```

**Value choice**: deferred to the task, which must pick a value consistent
with C55's existing vocabulary across readers rather than invent a spelling.
The constraint is that it name the mechanism (how the component was found),
matching `bazel-http-archive`'s shape.

**Coordination risk, stated**: #1234 will also touch the pip reader, to add
the missing edges. The two are separable — an annotation on components
already emitted versus edges between them — but whichever lands second
should expect a conflict there, and #1234's golden churn will be larger.
FR-002a is deliberately the smaller change so this feature is not a
dependency of #1234.

---

## R8 — Observability: the log exists; only the new counters are missing

**Decision**: add three counters to the existing `tracing::info!`. Correct
the module header that misdescribes where it fires.

**Rationale, and a correction to this section's first draft.** An earlier
version of R8 claimed the counts were "computed and thrown away" and that
an operator could not distinguish a scan where nothing matched from one
where the classifier never ran. That was wrong. The log exists, inside the
callee, and fires unconditionally:

```rust
// orphan_reason.rs:317-327
// FR-008 — per-scan info-level log with per-code counters, one line
// per scan. Grep-friendly per the milestone-157-onwards convention.
// Zero counters indicate a healthy scan.
tracing::info!(
    orphan_reason_stale_go_sum_entry = counts.stale_go_sum_entry,
    ...
    "orphan-reason classification complete"
);
```

So zero-matched and not-run are already distinguishable — the line is
present either way and the counters read zero.

**How the error happened, since it is the second of its kind in this
feature.** I read `scan_cmd.rs:4939` (`let _m167_orphan_reason_counts = …`),
saw a discarded return value, and concluded the log was missing. The log is
emitted by the callee before it returns, so the discard proves nothing about
observability. R6 made the same shape of mistake — reading one site and
generalising about the whole data path — and the project's own standing note
("re-verify research empirical claims at implement time") exists for exactly
this.

**What is actually left**: the three new counters must be added to that
`tracing::info!`, or the new codes fire with no operator-visible signal. And
the module header at `orphan_reason.rs:18` says *"Per-scan `tracing::info!`
fires from the CALL SITE (not this module)"*, which contradicts line 320 and
is the sentence that misled me. It is corrected in the same change.

**Alternatives considered**: moving the log to the call site so the header
becomes true. Rejected — the callee is the right place (it owns the counts,
and every caller gets the log for free); the header is what is wrong.

## R9 — The two surviving declarations

**Decision**: edit `count_at_declaration` and `reason` in place for
`python-flask` (23 → 13) and `maven-guice` (5 → 3); delete the other three
entries whole.

**Rationale**: the file's entries are
`[[declaration]]` tables with `invariant`, `target`, `kind`,
`count_at_declaration` and a multi-line `reason` (read at
`invariant-declarations.toml:218-278`). `kind` stays
`"unreachable-and-unexplained"` for both survivors — the kind is unchanged,
only the count and the attribution.

The precedent for rewriting rather than deleting is commit `a3691e56`,
"docs(corpus): point the I8 declaration at the defect it actually has
(#1216)". A declaration that outlives its issue is the visible record that
the gap is open, which is the property the lane exists to preserve.

**Alternatives considered**: deleting all five per #1214's done-condition.
Rejected in the spec, with the deviation stated rather than silent.

---

## Open questions carried into tasks

None blocking. Two are deliberately task-level rather than spec-level:

- **The exact wire strings for the three new codes.** Named in the
  vocabulary contract and frozen there before any emission task runs, since
  consumers key on them.
- **The C55 value for the pip reader** (R7), to be chosen against C55's
  existing cross-reader vocabulary rather than invented.
