# Contract: emitted content

## VEX — CVE-bearing declarations

One statement per (component, CVE):

```json
{
  "vulnerability": { "name": "CVE-…" },
  "products": [
    { "@id": "<the build>",
      "subcomponents": [ { "@id": "<the declared component>" } ] }
  ],
  "status": "affected",
  "impact_statement": "… nixpkgs-declared …"
}
```

`products[0]` is the **build**, with the component as a subcomponent
(FR-006a). This is the shape milestone 1035 gives its `not_affected`, which is
what makes the FR-012 swap comparable — a consumer seeing an `affected` where
a `not_affected` would have been is comparing like with like.

The grade rides in `impact_statement`, as milestone 1035's do, because OpenVEX
has no grade slot and a bare status hides what it rests on.

## Reconciliation

When a declaration and a patch name one CVE on one component:

| | emitted |
|---|---|
| declaration `affected` | yes |
| patch-derived `not_affected` | **withheld** |
| the patch itself in `pedigree.patches[]` | **yes, unchanged** |

The evidence survives; only the suppression is withheld. A document-scope
count keeps silence distinguishable from suppression (FR-013a).

## SBOM — prose declarations

Per-component annotation carrying the text verbatim. These are composition
facts: "Vendors Electron 2.0" says there is a component inside this one that
the SBOM does not list. No identifier is invented, which is why they cannot be
VEX.

## SBOM — document scope

- the acceptance record (FR-015), coarse, bounded by FR-016
- checked / unchecked member counts (FR-001d)
- declarations naming no CVE (FR-011)
- withheld reconciliations (FR-013a)

## Catalogue and parity

Rows land with their extractors in the same change — the gate fails in both
directions, as milestone 1034 found. Catalogue edits are exact-string, never
anchored regex: a DOTALL pattern anchored on one row has silently edited the
next one before.

Every new row needs the Principle V audit in research R6, which found no
native carrier for the prose annotation, the acceptance record, or the
coverage counts, and a native one (OpenVEX) for everything else.

## Byte-identity

Without `--nix-closure`: no evaluation runs, no annotation is emitted, no VEX
statement is added, and every committed corpus golden is unchanged.
