# Phase 1 data model: the closure's build inputs that `unreferenced` hides

**Feature**: `1372-unreferenced-pname-components` | **Issue**: #1366

No new persistent state. One new in-process set, one new per-component
disclosure, one document-scope record. The entity most affected —
`DerivationRole` — gains no variant; what changes is how one of its four
values is reached.

---

## 1. `GraphReferrers` — the new signal

A set of derivation keys that at least one other derivation's `inputs.drvs`
names.

| | |
| --- | --- |
| built from | `RawDerivation.inputs.drvs` (`derivation.rs:58-66`), already parsed |
| built where | inside `classify`, from the `&RawClosure` it already receives |
| lifetime | the duration of one `classify` call |
| cost | one pass over the closure; 10,342 edges on the pinned closure |

### Validation rules

- **V1 — self-reference does not count.** A derivation naming itself in its
  own `inputs.drvs` is not evidence that something else consumed it. Not
  observed on the pinned closure; excluded so it cannot become a silent
  self-admission.
- **V2 — the referrer need not be emitted.** A derivation consumed only by
  something that is itself not emitted is still consumed. The alternative —
  requiring an emitted referrer — would make the signal depend on its own
  output and is not what `inputs.drvs` records.
- **V3 — the key must resolve.** `inputs.drvs` keys are `.drv` store paths
  that are keys of `closure.derivations`. One that resolves to nothing is
  ignored rather than counted; milestone 1280 already measured dangling keys
  and has a fixture for them.

---

## 2. `DerivationRole` — unchanged shape, one arm rewritten

```text
                               BEFORE                      AFTER
(artifact, tooling)
  (true,  true )    →          Both                        Both
  (true,  false)    →          ArtifactInput               ArtifactInput
  (false, true )    →          BuildTooling                BuildTooling
  (false, false)    →          Unreferenced                graph names it ? BuildTooling
                                                                           : Unreferenced
```

**Only the fourth arm changes.** A derivation any declared dependency
attribute names keeps exactly the role it has today — FR-003a's "never
overrides better evidence", asserted by SC-004a.

### Measured transition on the pinned closure

| from | to | count | effect |
| --- | --- | --- | --- |
| `Unreferenced` | `BuildTooling` | **741** | 100 begin emitting, 641 relabelled |
| `Unreferenced` | `Unreferenced` | **1** | `moat 0.1`, the closure root |
| anything else | unchanged | 533 | no declared-attribute role is touched |

**The root is the control.** It is the one derivation nothing consumes, and
if it ever stops being `Unreferenced` the signal has started admitting things
on no evidence.

---

## 3. `AdmissionSignal` — the per-component disclosure (FR-005)

Closed enum. Records *why* a component carries the role it does.

| variant | meaning |
| --- | --- |
| `DeclaredAttribute` | a `buildInputs`-family attribute names it — today's behaviour |
| `BuildGraph` | **only** `inputs.drvs` names it — new |

**Emitted only for `BuildGraph`.** A component admitted the old way carries no
new annotation, which keeps FR-006's "no other annotation changes" true for
the components this feature does not touch, and makes the disclosure's
presence the signal.

> Deliberately **not** a fifth `DerivationRole` variant. The role answers
> "artifact or tooling"; this answers "on what evidence". Collapsing them
> would repeat the defect issue #1374 records, where one role value carries a
> claim its test does not support.

---

## 4. `GraphAdmissionRecord` — document scope (C210)

| field | type | notes |
| --- | --- | --- |
| `schema` | `&'static str` | `"waybill-closure-graph-admission/v1"` |
| `admitted` | `usize` | **derivations** that begin emitting because of this signal — expected **100** |
| `relabelled` | `usize` | **derivations** already emitting whose role was corrected — expected **641** |

> **The unit is derivations, not components, and both counts use it.** The two
> differ by a lot: 100 newly admitted derivations merge into roughly **45**
> distinct components, because build variants share a `(name, version)`. The
> classifier works in derivations and the role is a per-derivation fact, so a
> record about classification counts derivations. A reader comparing
> `admitted` against the document's component count will find they disagree —
> that is expected, and the catalog row says so.

**Invariant**: `admitted + relabelled` equals the number of derivations whose
role changed. Asserted, not commented.

**Emitted when**: a closure was recorded — i.e. wherever C204 and C208 are.
Absent otherwise, which keeps every non-closure golden untouched (research
R9), and makes its presence state that the classifier ran.

**Why both counts**: `admitted: 0, relabelled: 0` means *this closure declares
its dependencies properly and the signal found nothing to do*. Absence means
*no closure*. A single count could not express the first.

---

## Relationship to existing entities

```text
RawClosure.derivations[*].inputs.drvs ──► GraphReferrers
                                               │
ARTIFACT_FIELDS / TOOLING_FIELDS ──► (artifact, tooling)
                                               │
                                               ▼
                                     DerivationRole        ◄── only (false,false) consults the graph
                                      │          │
                   ┌──────────────────┘          └──────────────┐
                   ▼                                            ▼
        emits_as_component                              AdmissionSignal
   (role_alone_emits admits the 100;                    (C182 + the new
    the 603 members keep emitting                        per-component
    via the pname clause — R4)                           disclosure)
                   │                                            │
                   ▼                                            ▼
        graph::relationships                          GraphAdmissionRecord
     (more endpoints ⇒ edges move; R7)                      (C210)
                   │
                   ▼
        closure_role_scope (m1277)
   (741 components change lifecycle scope; R5)
```

**Not touched**: `ARTIFACT_FIELDS`, `TOOLING_FIELDS`, `consulted_fields()`
(R6 — growing it would make C205 name an attribute that does not exist),
`emits_as_component`, `output_index`, `member::classify`, and every component
whose role comes from a declared attribute.
