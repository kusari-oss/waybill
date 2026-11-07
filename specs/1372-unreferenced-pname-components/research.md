# Phase 0 research: the closure's build inputs that `unreferenced` hides

**Feature**: `1372-unreferenced-pname-components` | **Issue**: #1366 | **Date**: 2026-11-06

Every finding is cited to a file and line read out of the tree at `1d694ae2`,
or to a measurement run against the pinned closure. Predictions are labelled as
predictions and name the task that will measure them.

---

## R1 — What `classify` actually reads, and what it does not

`classify(closure)` (`closure/classify.rs:182-213`) is one pass over the
closure:

```rust
for drv in closure.derivations.values() {
    for (fields, sink) in [(ARTIFACT_FIELDS, &mut artifact), (TOOLING_FIELDS, &mut tooling)] {
        for field in fields {
            for path in drv.paths_in(field) {
                if let Some(key) = index.get(store_basename(path)) { sink.insert(key); }
            }
        }
    }
}
// role = match (artifact.contains(k), tooling.contains(k)) { … (false,false) => Unreferenced }
```

Two string-keyed field lists — `ARTIFACT_FIELDS` (3 names) and
`TOOLING_FIELDS` (4) — resolved through `output_index()`
(`derivation.rs:456`), which maps every output's store basename to its
derivation key.

**`inputs.drvs` is not consulted anywhere in this file.** The role is a
statement about *declared attributes* and has never been a statement about the
build graph.

---

## R2 — The build graph is already parsed and already in hand

`RawInputs.drvs` is a `BTreeMap<String, Value>` on every `RawDerivation`
(`derivation.rs:58-66`), populated by the normal parse. Milestone 1280 added
it and uses it for edges; `input_drvs()` is the accessor at
`derivation.rs:333`.

So this feature **consults a signal the reader already holds**. It adds no
parse, no I/O, no new data source. `classify` receives the whole
`&RawClosure`, so the referrer set can be built in the same function from the
same input.

**Why this matters for the risk profile**: the failure mode is not "we might
read the graph wrong" — milestone 1280's edges already depend on reading it
right, and the corpus goldens pin that. The failure mode is entirely about
*what we conclude* from it.

---

## R3 — The change is three lines of logic, and 641 relabellings

**Decision**: build a referrer set from `inputs.drvs` in `classify`, and use
it **only** for the `(false, false)` arm.

```text
(true,  true ) => Both            ← unchanged
(true,  false) => ArtifactInput   ← unchanged
(false, true ) => BuildTooling    ← unchanged
(false, false) => if graph_names(k) { BuildTooling } else { Unreferenced }
```

This is FR-003a exactly: a declared attribute always wins, and the graph
answers only where nothing else does. SC-004a asserts the "always wins" half.

**Measured blast radius** on the pinned closure:

| | count |
| --- | --- |
| derivations currently `unreferenced` | 742 |
| …named by some `inputs.drvs` | **741** |
| …**already emitting**, so only relabelled | **641** |
| — of those, milestone-1282 members (no declared name) | 603 |
| — of those, admitted by the `patches` clause | 38 |
| …**newly emitting** | **100** |
| still `unreferenced` afterwards | **1** — `moat 0.1`, the root |

The headline is 100 new components. The actual churn is **641 relabellings**,
six times larger, and every one of them lands in a golden that already ships.

---

## R4 — Emission does not become role-dependent (FR-002, Edge Cases)

`emits_as_component` (`closure/emit.rs:91`) is:

```rust
role_alone_emits(role)
    || drv.paths_in("patches").next().is_some()
    || drv.declared_pname().is_none()          // milestone 1282
```

The 603 members emit through the **third** clause, which does not mention the
role. Changing their role from `Unreferenced` to `BuildTooling` moves them
from "emitted by the pname clause" to "emitted by the pname clause **and** the
role clause" — a disjunction, so emission is unchanged.

**The 100 newly emitting are newly covered by `role_alone_emits`.** That is
the entire mechanism by which this feature adds components; no change to
`emits_as_component` itself is required.

**Verification obligation, not an assumption**: a task asserts the member
count is identical before and after, so that a change making emission
role-dependent would be caught rather than reasoned about.

---

## R5 — The lifecycle-scope consequence is real and is the largest side effect

Milestone 1277 derives lifecycle scope from the C182 role
(`resolve/closure_role_scope.rs`). 741 components moving from `unreferenced`
to `build-tooling` therefore also move in scope.

That is the spec's intended reading — a tool reached through a package's
`postInstall` is build-time — and it is a visible change for any consumer
filtering on scope.

**The limit the spec records, repeated here because the implementer will meet
it**: `allowedRequisites` is nixpkgs' assertion about the **runtime** closure,
so a derivation named only there is arguably a runtime requisite rather than
tooling. `inputs.drvs` cannot separate the two. `build-tooling` is the claim
most of the naming attributes support, not a claim about each one.

**Measurement task**: record how many components change lifecycle scope, as a
distinct number from the 741 role changes. They will differ — scope assignment
has its own exemptions (m1276's compiler-supplied rule) — and a reader who
assumes they are the same number will mis-read the diff.

---

## R6 — C205 is unaffected, and saying so is load-bearing

`waybill:closure-unclassified-refs` (C205) records **nixpkgs dependency
attribute names the reader did not consult**, assembled from
`consulted_fields()` (`classify.rs:110`), which is derived from the same two
constants classification reads.

The attributes implicated in this feature — `postInstall`, `installPhase`,
`postPatch`, `buildPhase`, `initialPath`, `allowedRequisites` — are **not
dependency attributes**, so they were never candidates for C205 and their
absence from it was never a defect.

**Consequence**: `consulted_fields()` must **not** grow. Adding `inputs.drvs`
to it would make C205 report a dependency attribute that does not exist, and
C205's value is precisely that it is trustworthy when silent. The new signal
is a third input to classification, not a fourth field in the two lists.

---

## R7 — Edges: more components, and the join already handles them

`graph::relationships` (`closure/graph.rs:240`) joins the projected graph onto
components by `(name, version) → purl`, reading the live component slice.
`graph::project` takes `emit::emitted_drv_keys`, so a derivation that starts
emitting also starts being a possible edge endpoint.

**Prediction, not a measurement**: edge count rises. Milestone 1280's
projection walks *through* non-emitted derivations to the nearest emitting
ones, so admitting 100 derivations that previously sat in the middle of such
walks should convert some pass-through paths into direct edges — which can
*reduce* the count of long edges while adding short ones. The net direction is
not predictable from the component count alone.

A task measures it. **Issue #1367's edge undershoot is adjacent and out of
scope**; this feature must record its own effect so that #1367 is not later
diagnosed against a number this change moved.

---

## R8 — The document-scope record (FR-008)

**Decision**: a new row, **C210**, document scope, counting how many
components the build-graph signal admitted and how many it relabelled.

Next free id confirmed: the catalog's highest row is **C209** (milestone 1367).

**Shape**, on the C190 / C192 / C204 / C208 / C209 template: a JSON object
with `schema`, `admitted` (newly emitted because of this signal) and
`relabelled` (already emitting, role corrected).

**Why two counts again**: the same argument C209 settled. One number cannot
distinguish "this closure declares its dependencies properly, so the signal
found nothing to do" from "the signal did not run". With `admitted: 0,
relabelled: 0` meaning the former and absence meaning the latter, both are
readable.

**Native-field audit (Constitution Principle V)**: no CDX 1.6 / SPDX 2.3 /
SPDX 3.0.1 construct records how a classifier reached a conclusion or how many
classifications it revised. The per-component carrier for *which signal*
admitted a component (FR-005) is the existing **C182** role plus a new
per-component disclosure; the role itself is already catalogued.

---

## R9 — Where the goldens churn, and why this is bigger than milestone 1367

| golden set | expected effect |
| --- | --- |
| local `cdx/spdx/spdx3_regression` and the three others | **unchanged** — none scans a nix closure |
| corpus `nix-closure-moat` | **large** — 641 role changes, 100 new components, plus edges |
| the other 16 corpus targets | **unchanged** — no closure |

This is the inverse of milestone 1367, whose unconditional C209 touched *every*
golden but by four lines each. Here one target changes by a lot and the rest
not at all.

**C210 is emitted only where a closure was recorded** — like C204 and C208,
unlike C209 — which is what keeps the non-closure goldens still.

> Milestone 1367's plan did not predict its golden blast radius and the gate
> found it. This row exists in the plan's risk table for that reason.

---

## Constitution alignment

| Principle | Finding |
| --- | --- |
| **I. Pure Rust, statically linked** | Zero new Cargo dependencies. No new I/O, parse or data source — `inputs.drvs` is already parsed. |
| **IV. Type-driven correctness** | `DerivationRole` stays a four-variant enum; no new variant (see Complexity Tracking in the plan). No `.unwrap()` in production. |
| **V. Specification compliance** | Native-field audit for C210 performed (R8). The per-component role is the existing C182. No PURL changes, so no new conformance surface. |
| **VIII. Completeness** | The feature **is** this principle: 100 components present in the build and absent from the document. |
| **IX. Accuracy** | Cuts both ways and is the reason FR-003a is bounded. Admitting the 100 is accuracy; calling all 741 `build-tooling` is one claim the evidence mostly supports, and R5 records where it does not. |
| **X. Transparency** | FR-005's per-component disclosure and FR-008's C210. A consumer can tell a graph-admitted component from a declared one. |
| **XII. External data source enrichment** | No external source. `inputs.drvs` is in the closure the tier already fetched. |
| **Operating Modes** | Scan mode; generation context unchanged. |

**No violations.**
