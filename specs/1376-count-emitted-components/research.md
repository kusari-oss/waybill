# Phase 0 research — C209 promises a sum a consumer cannot check

**Feature**: `1376-count-emitted-components` | **Issue**: #1385

Seven findings. The first overturned the issue this feature was filed from,
and the rest followed from measuring instead of reasoning.

---

## R1 — The counts are correct; the sentence is not

Measured on one scan of `slack-web`, all three formats:

| format | containers | total |
| --- | --- | --- |
| CycloneDX | 199 `components[]` + 1 `metadata.component` | **200** |
| SPDX 2.3 | 200 `packages[]` | **200** |
| SPDX 3 | 197 `Package` + 3 `File` | **200** |

Every format emits exactly 200 things and the row says 200.

**Decision**: change no count. **Rationale**: nothing is missing from any
document; the defect is in the description. **Alternative rejected**:
"fixing" the arithmetic, which is what #1385 originally proposed and what a
reader of its first version would have done.

## R2 — Two independent reasons `.components | length` is short

1. **CDX promotes a single main-module** to `metadata.component`
   (`root_selector::select_root`, documented at `metadata.rs:716`), which is
   correct CDX.
2. **SPDX 3 emits file-tier entries as `File` nodes**, so counting `Package`
   nodes is short by exactly `excluded_files`.

Neither is a defect. The row is one number carried into three documents that
arrange their contents differently.

## R3 — The CDX rule needs three clauses, each load-bearing

| root | real? | already in `components[]`? | adjustment |
| --- | --- | --- | --- |
| synthetic (`--root-name`, or no main module) | no | no | **+0** |
| one main-module, promoted | yes | no | **+1** |
| several main-modules, one elected | yes | yes | **+0** |

A consumer distinguishes real from synthetic without inside knowledge: a real
root carries `waybill:*` properties (`waybill:component-role`,
`waybill:sbom-tier`, …); a synthetic placeholder carries **none**.

**Decision**: state the expression per format, with all three CDX clauses.
**Rationale**: dropping any clause breaks one shape — omit the
already-present test and live `moat` over-counts by 1; omit the real test and
every `--root-name` scan over-counts by 1. **Verified** against `slack-web`
promoted, `slack-web --root-name` synthetic, live `moat` duplicated, and
**15 of the 16** corpus targets carrying the row.

The sixteenth, `pants-example-javascript`, disagrees by 1 because its golden
is **filtered** to `pkg:npm/*` by `corpus_harness_195/js_filter.rs` — a
structural exception already recorded in #1380's spec. The expression is
right; that artifact is not a whole document.

## R4 — Where the claim is written, and how many places

| location | text |
| --- | --- |
| `docs/reference/sbom-format-mapping.md`, C209 row | "they sum to `.components | length` exactly, with no filtering rule the reader has to know, which is the property a consumer can check with `jq`" |
| `scan_cmd.rs`, `total()` doc comment | "**Against the document as it stands**, not a subset the reader has to derive" |
| `scan_cmd.rs`, the `debug_assert` comment | "the five partition the component set" |

**Decision**: correct all three, and make the source comments say what the
assertion actually checks. **Rationale**: the `debug_assert` compares the
counts to the **slice**, which is the right check for "nothing was classified
twice" and says nothing about the document. Describing it as the latter is
how a true assertion came to back a false claim.

## R5 — The fixture already exists, four times over

The integration test needs one main module and no `--root-name`. Measured
across in-tree fixtures:

| fixture | elected root | case |
| --- | --- | --- |
| `pnpm_v9_scope` | `pkg:npm/waybill-fixture-pnpm-v9-scope@1.0.0` | **promoted** |
| `sbom_edit` | `pkg:npm/acme-shop@1.4.0` | **promoted** |
| `closure_role_scope` | `pkg:hackage/waybill-fixture-roleproj@0.1.0` | **promoted** |
| `nix_eval` | `pkg:hackage/waybill-fixture-ifd@0.1.0.0` | **promoted** |
| `cargo`, `pip`, `pants_go`, `scope_propagation` | `pkg:generic/filesystem-scan@0.0.0` | synthetic |

Two were checked to exhibit the exact failure:

```
pnpm_v9_scope       row=10  .components|length=9   naive check FAILS
closure_role_scope  row= 6  .components|length=5   naive check FAILS
```

**Decision**: use an existing fixture; add none. **Rationale**: the shape was
in the tree all along. The corpus could not reach it, but the unit fixtures
could have — **nothing ever looked**. That is a more accurate account of the
failure than "untestable", and it means the test is cheap.

## R6 — The residual gap this does not close

All four promoted-root fixtures are `npm` or `hackage`. `cargo` and
`pants_go` were measured to produce **synthetic** roots, so a promoted root
from a third ecosystem is covered by nothing.

**Decision**: test one `npm` and one `hackage` fixture, and record the gap
rather than closing it. **Rationale**: the rule under test is about the
*root's relationship to the array*, not about its ecosystem, and two
ecosystems is enough to show the rule is not npm-specific. **Alternative
rejected**: constructing a cargo fixture that promotes — that is a fixture
built to satisfy a test rather than to represent a real project shape.

## R7 — Why three milestones did not catch it

All 17 corpus targets pass `--root-name`, forcing the synthetic path where
the naive check happens to hold. The control is one flag on one project:

```
without --root-name   root=pkg:hackage/slack-web@2.4.0.0       row=200  components[]=199
with    --root-name   root=pkg:generic/slack-web-harness@0.0.0 row=200  components[]=200
```

**The configuration CI always runs is the one in which the claim is true.**
`feedback_match_harness_invocation` already records `--root-name` as
load-bearing for local-vs-CI differences; this is the same flag making CI the
configuration that cannot fail.

---

## A correction to the issue this came from

#1385 was filed as a counting defect and called this *"the third instance of
one shape"* — the counted slice and the emitted array being different sets.

That framing is right for milestone 1373 (file-tier outside the slice, a real
under-count) and 1374 (members inside it wrongly, a real over-count). It is
**wrong here**: the slice and the emitted set are the same size, and only the
container one component is written to differs.

Three symptoms that look alike are not three instances of one cause. The
issue has been corrected and retitled.

---

## Constitution alignment

| Principle | Check |
| --- | --- |
| **I. Pure Rust, statically linked** | No dependency change; one test and three comment edits. |
| **IV. Type-driven correctness** | No type change. The `debug_assert` is kept and its comment corrected to say what it checks. |
| **V. Specification compliance** | Nothing emitted changes, so no conformance surface moves. |
| **IX. Accuracy** | The feature is this principle applied to waybill's own documentation: a true number described falsely. |
| **X. Transparency** | A consumer gets a check that works rather than one that fails on correct documents. |
| **XII. External enrichment** | No external source involved. |
