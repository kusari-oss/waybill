# Contract: emitted content

**Feature**: `1035-nix-closure-sbom` (#1034, #1040)

## Components

Closure members with role `ArtifactInput`, `BuildTooling` or `Both` become
components **in addition to** the manifest-derived set (spec FR-003a). Each
carries `waybill:closure-role` (C182).

Members with role `Unreferenced` do not — except that patches among them become
pedigree entries (below). Fetched sources, setup hooks and bootstrap toolchain
are out of scope for v1: they are the largest bucket and nothing measured yet
argues they belong in a document.

## Patches — native, and a first for waybill

CycloneDX, verified against `bom-1.6.schema.json`:

```json
"pedigree": {
  "patches": [
    { "type": "backport",
      "resolves": [ { "type": "security", "id": "CVE-2019-13232" } ] }
  ]
}
```

`patch.type` enum is `['unofficial','monkey','backport','cherry-pick']`;
`issue.type` enum is `['defect','enhancement','security']`. waybill uses
`pedigree` nowhere today, so this is new emission machinery, not a new field on
an existing path.

A patch whose name carries no CVE is still recorded — `type: "backport"` with
no `resolves` entry. Silence about it would make partial coverage look like
absence.

**SPDX 2.3 and SPDX 3 have no equivalent** and take the annotation bridge. This
is the one place the three formats genuinely diverge in capability rather than
in spelling.

## VEX

A backport produces **two** statements (spec FR-011), never one:

| | subject | status | grade |
|---|---|---|---|
| 1 | the component **version** | `affected` | `filename-derived` |
| 2 | **this build** | `not_affected` | `filename-derived` |

Neither may be emitted without its grade (FR-012a). An ungraded `not_affected`
from a filename is the claim FR-009 exists to prevent — a consumer could
suppress a real finding on it.

This extends waybill's existing OpenVEX emitter, which today emits only
`under_investigation` because that was "the status waybill can honestly emit".
The grade is what makes a stronger status honest.

## Document scope

`waybill:nix-closure` (C184): attribute evaluated, derivations seen, counts
emitted by role, patch count, and **patches carrying no CVE** (FR-010, FR-018).

That last figure is the one a consumer needs to judge the rest. On the measured
projects it is 40 and 46 — most patches name no CVE, so the CVE-derived VEX
covers a minority of the backports present. Omitting it would let absence of a
VEX statement read as absence of a backport.

## Catalogue and parity

Rows **C182**, **C183**, **C184** land with their extractors in the same
change. `every_catalog_row_has_an_extractor` checks both directions — an
extractor without a row fails it too, as milestone 1034 discovered.

Catalogue edits are exact-string, never anchored regex: a DOTALL pattern
anchored on one row has silently edited the next one before.

## Byte-identity

With `--nix-closure` absent, no closure query runs, none of C182–C184 is
emitted, no `pedigree` appears, and every committed corpus golden is unchanged.
