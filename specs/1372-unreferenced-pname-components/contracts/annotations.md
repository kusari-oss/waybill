# Contract: emitted interfaces

**Feature**: `1372-unreferenced-pname-components` | **Issue**: #1366

Three changes to what a document says: components that did not exist, a role
value that changes on components that did, and one new document-scope record.
No new CLI flags.

---

## 1. Components that did not exist before

~45 distinct `(name, version)` pairs begin appearing, including:

```
gzip 1.14        gawk 5.4.0       gnused 4.9
less 692         patch 2.8        coreutils 9.11
build 1.4.4      macOS-SDK 26.4   libSystem B
cctools-binutils-darwin 1010.6    llvm-binutils 21.1.8
bootstrap-stage{0..4}-stdenv-darwin 26.05pre-git
```

Each is an ordinary closure component: a `pkg:generic` PURL unless another
pass identifies it better, the licence and patches the closure records, and
the C182 role `build-tooling`.

**They are not special-cased anywhere.** A component admitted this way is
subject to milestone 1366's ecosystem identity pass and milestone 1367's CPE
pass exactly as any other, so some may arrive already carrying a `pkg:pypi`
identifier or a CPE. That is intended and is why those passes run after the
closure merge.

---

## 2. `C182 waybill:closure-role` — same row, new population

**741 components change from `unreferenced` to `build-tooling`.** 100 of them
are new; **641 already ship in goldens today**.

The row's documented meaning does not change — it still answers "artifact
input, build tooling, both, or nothing references this". What changes is that
`unreferenced` becomes **true**: before this, 741 components carried it while
the build graph named them.

**Compatibility, stated plainly**: a consumer filtering `waybill:closure-role
== "unreferenced"` to find components nothing depends on currently gets 742
and will afterwards get 1. That is the correction, not a regression — but it
is a large behavioural change for anyone keying on that value, and it is the
reason FR-006 was narrowed explicitly rather than quietly.

---

## 3. `C211 waybill:closure-admission` — per-component (FR-005)

Emitted **only** on a component the build graph admitted and no declared
attribute names.

| format | carrier |
| --- | --- |
| CycloneDX | `properties[]` entry `{"name":"waybill:closure-admission","value":"build-graph"}` |
| SPDX 2.3 | per-Package `annotations[].comment`, `MikebomAnnotationCommentV1` envelope |
| SPDX 3 | per-Package `Annotation.statement`, same envelope |

Closed value set: `build-graph` today. A component admitted by a declared
attribute carries **nothing**, so presence is the signal and no existing
component gains an annotation it did not have.

`Directionality::SymmetricEqual`, `order_sensitive: false`. Rides the generic
`extra_annotations` bag, which all three emitters already carry — so, as in
milestone 1367, **no emitter code is needed**.

---

## 4. `C210 waybill:closure-graph-admission` — document scope (FR-008)

```json
{
  "schema": "waybill-closure-graph-admission/v1",
  "admitted": 100,
  "relabelled": 641
}
```

- Emitted wherever C204 and C208 are — i.e. a closure was recorded — and
  absent otherwise. This is what keeps all 16 non-closure corpus targets and
  every local golden untouched.
- `admitted: 0, relabelled: 0` means the closure declares its dependencies
  properly and the signal found nothing to do. **Absence** means no closure
  ran. One count could not say both.
- Carriers: CDX `metadata.properties[]`; SPDX 2.3 and SPDX 3 document-scope
  annotation. `SymmetricEqual`.

> The figures above are the measured expectation from the pinned closure, not
> a contract. SC-001 requires the implementation's own numbers be recorded.

Both rows need matching entries in `parity/extractors/mod.rs::EXTRACTORS` or
`every_catalog_row_has_an_extractor` and `holistic_parity` fail — and that
gate lives in the **lib** target.

---

## 5. Dependency edges

Edges are not directly specified by this feature, but they move: a derivation
that begins emitting becomes a possible edge endpoint, and milestone 1280's
projection walks *through* non-emitting derivations to the nearest emitting
one. Admitting 100 derivations that previously sat mid-walk converts
pass-through paths into direct edges.

**The net direction is not predictable from the component count** — short
edges are added while long ones are replaced. A task measures it, and records
the number so that issue #1367's edge undershoot is not later diagnosed
against a figure this feature moved.

---

## 6. CLI

**No new flags.** The behaviour is reached by `--nix-closure` and is not
optional: a classifier that labels 741 components `unreferenced` while the
build graph names them is wrong, and a flag to keep it wrong is not a feature.

---

## 7. Compatibility summary

| | effect |
| --- | --- |
| component count | **rises** by ~45 distinct identities |
| existing components' identity, version, licence | **unchanged** (FR-006) |
| existing components' C182 role | **641 change** — the correction |
| lifecycle scope | moves with the role for those 741 (research R5) |
| edges | move; direction measured, not predicted |
| non-closure scans | **byte-identical** (SC-006) |
| local goldens | **unchanged** — none scans a closure |
| corpus goldens | `nix-closure-moat` changes substantially; the other 16 do not |
