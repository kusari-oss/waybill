# Wire contract

This feature adds no catalog row, no flag and no field. It changes which components and edges appear, and the values of two existing rows.

## Components (all three formats)

Under `--nix-closure`, the document gains one component per added boot library (data model):

| | CycloneDX | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| identity | `purl` `pkg:hackage/<name>@<version>` | `externalRefs[]` purl, same | `software_packageUrl`, same |
| C218 | `properties[]` `waybill:boot-library-compiler` | annotation, `MikebomAnnotationCommentV1` | annotation element |
| C247 | `properties[]` `waybill:boot-library-dependencies` | annotation | annotation element |
| tier | `waybill:sbom-tier` = `build` | same, as annotation | same |
| CPE | none (spec 1373) | none | none |
| CycloneDX `scope` | absent, as on every boot library today | n/a | n/a |

No other property. The set of property keys equals that of a closure-synthesized peer: C218, C247 and `waybill:sbom-tier`.

## Edges

Each is a `dependsOn` from a boot library to the component holding the PURL its release ships. In SPDX 2.3 that is `DEPENDS_ON`, or `BUILD_DEPENDENCY_OF` where pass 3 types it, as today. In SPDX 3 it is a `dependsOn` relationship.

An edge may now join two components with different C218 labels: a bootstrap compiler's library depending on a version the project compiler also ships. It never joins a library of release R to a version R does not ship.

## C247 `waybill:boot-library-dependencies`

The value shapes are unchanged:
- `{"source":…}`;
- `{"absent":[…],"source":…}`;
- `{"reason":"release-not-in-table"|"library-not-in-table"}`.

What changes is that `absent` now lists only names milestone 966's table has no version for at that release. On every Linux closure measured, that is none.

The catalog row's prose "(`rts` and `ghc-internal` never are)" is replaced. The new text says `absent` names a dependency the document could not be given, because the version table lacks it. It also says spec 1655 adds every reachable boot library.

## C105 `waybill:graph-completeness-reason`

`boot-library-edges-incomplete` is no longer emitted when every C247 is `{"source":…}`. The code's text and grammar are unchanged.

## C104 `waybill:graph-completeness`

This becomes `complete` where `boot-library-edges-incomplete` was the only reason: shellcheck, among the goldens.

## C209 `waybill:cpe-coverage`

`addressable_without_cpe` rises by the number of added components, because Hackage is OSV-addressable (spec 1373 FR-003). The partition still sums to the component total.

## Unchanged

- Without `--nix-closure`, every document is byte-identical.
- No new `waybill:*` row and no catalog row count change. Parity directionality is unchanged.
