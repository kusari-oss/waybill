# Contract: annotations

Three new catalog rows and one new value on an existing row. All `Directionality::SymmetricEqual`.

> **Superseded on rebase (2026-11-10).** While this was being built, main
> landed milestone 1377 (#1405, issues #1398 and #1394): the package a closure
> scan evaluated is always emitted and is the document's subject, identified
> as `graph.evaluated` — the closure's sole unconsumed derivation. That is the
> root-as-component work this feature had taken on as "the minimum of #1398".
> On rebase the feature's own mechanism (a `root` value for
> `waybill:closure-role`, `DerivationRole::Root`, `graph::sole_root`) was
> **removed** in favour of main's. Text below that describes it is history.
> Main also took row C212, so this feature's rows are **C213, C214, C215**.

## C182 `waybill:closure-role` — new value `root`

The closure root: the package the scan evaluated. Never marked absent from the
artifact by the role-scope pass.

## C213 `waybill:nix-closure-contents` — document scope

Present on every document where the closure tier produced members, including
when nothing was read. Absent when `--nix-closure` was not given or the tier
degraded (C190 explains that case).

| format | carrier |
| --- | --- |
| CycloneDX | `metadata.properties[]` |
| SPDX 2.3 | document `annotations[].comment` envelope |
| SPDX 3 | `Annotation.statement` on the document |

Value: the JSON object in `data-model.md` § ContentsRecord, keys sorted.

KEEP-NO-NATIVE: no format expresses per-member read counts. CycloneDX
`compositions` carries the consequence natively (the root-completeness claim
is withheld when `absent + failed + partial > 0`).

## C214 `waybill:closure-contents` — component scope

On every closure member (every component carrying C182). One of `read`,
`partial`, `absent`, `failed`, `no-outputs`.

KEEP-NO-NATIVE: none of the three formats has a per-component "contents
examined" marker.

## C215 `waybill:closure-binary-identity` — component scope

On a closure member only when at least one binary in its outputs names its own
module or crate. JSON object, keys sorted:

```json
{ "candidate": "pkg:golang/example.test/app@v1.2.3", "status": "confirmed" }
{ "candidate": "pkg:cargo/app@1.2.3", "status": "unchecked", "reason": "disabled" }
{ "candidate": "pkg:cargo/app@1.2.3", "status": "declined", "binary-version": "1.2.2" }
{ "status": "ambiguous" }
```

Invariant: `status == "confirmed"` ⇔ the component's primary identifier equals
`candidate`.

Native audit: CycloneDX 1.6 `evidence.identity[].concludedValue` can carry a
candidate; SPDX 2.3 and 3 cannot mark an identifier unconfirmed. Carried as an
annotation in all three for parity; the native CycloneDX slot is #1245's.

## Relationships — no new row

Member → recovered module is the format's ordinary dependency relationship:
CycloneDX `dependencies[].dependsOn`, SPDX 2.3 `DEPENDS_ON`, SPDX 3
`dependsOn`. Recovered modules are top-level components.
