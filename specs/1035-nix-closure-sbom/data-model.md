# Phase 1 Data Model: Nix derivation closure as SBOM content

**Feature**: `1035-nix-closure-sbom` (#1034, #1040) · **Date**: 2026-09-29

## Entities

### `ClosureMember`

One derivation from `nix derivation show -r`.

| Field | Meaning |
|---|---|
| `drv_path` | the `.drv` key; the closure's own identity |
| `pname` / `version` | from `env`, where the derivation carries them |
| `role` | see `DerivationRole` |
| `outputs` | output basenames, used to resolve references between members |

**Parsing note that is easy to get wrong**: output paths are stored *without*
the `/nix/store/` prefix, while the `env` fields that reference them carry it.
Comparing the two directly matches nothing — and matches nothing *silently*,
producing a classification where every member is unreferenced. That is exactly
what the first draft of the classifier did.

### `DerivationRole`

How nix itself references a member. Not inferred.

| Variant | Source | moat / slack-web |
|---|---|---|
| `ArtifactInput` | `buildInputs`, `propagatedBuildInputs`, `depsHostHost` | 264 / 390 |
| `BuildTooling` | `nativeBuildInputs`, `depsBuild*`, `nativeCheckInputs` | 134 / 146 |
| `Both` | referenced through each | 52 / 55 |
| `Unreferenced` | neither — sources, patches, hooks, bootstrap | 825 / 944 |

Both `ArtifactInput` and `BuildTooling` become components, distinguished by a
scope marker (spec FR-002, clarified 2026-09-29). `Unreferenced` members are
not components; patches among them become `PatchRecord`s.

### `PatchRecord`

A modification applied to a component. Not a component itself (FR-004) — it
describes a change to one.

| Field | Meaning |
|---|---|
| `name` | the patch derivation's name, e.g. `CVE-2019-13232-1.patch` |
| `applies_to` | the component patched — see task T-R1 |
| `resolves` | CVE identifiers extracted from `name`, possibly empty |
| `grade` | `EvidenceGrade` for each identifier |

### `EvidenceGrade`

How the CVE association was established. Required on every association
(FR-009) and on every VEX statement (FR-012a).

| Variant | Meaning |
|---|---|
| `FilenameDerived` | parsed from the patch derivation's name. The only grade v1 produces. |

A single-variant enum is deliberate. It is not a `bool` and not an `Option`,
because the point is that a *future* stronger provenance — a patch header, an
upstream mapping — must be distinguishable from this one, and a consumer that
reads the grade today keeps working when it is. An `Option<CveId>` would
record the association and lose how it was obtained.

### `ClosureVexStatement`

A backport produces two, never one (FR-011).

| Field | `affected` statement | `not_affected` statement |
|---|---|---|
| subject | the component **version** | **this build** |
| basis | nixpkgs applied a CVE-named patch, so it considered the version vulnerable | the patch is present in this build |
| grade | `FilenameDerived` | `FilenameDerived` |

They carry different evidential weight. That nixpkgs patched something is
strong evidence the version was thought vulnerable; that the patch fully
resolves the issue rests on a filename. Emitting only the second would let a
consumer suppress a real finding on the weaker half.

## Emitted metadata — Principle V audit

**`pedigree.patches[]` is native, and this is the first waybill feature to use
`pedigree` at all.** Verified against `bom-1.6.schema.json`:

```
patch.type enum : ['unofficial', 'monkey', 'backport', 'cherry-pick']
patch fields    : ['type', 'diff', 'resolves']
issue.type enum : ['defect', 'enhancement', 'security']
issue fields    : ['type', 'id', 'name', 'description', 'source', 'references']
component.pedigree.patches[] -> #/definitions/patch
```

So a backport resolving `CVE-2019-13232` is expressible exactly:
`type: "backport"`, `resolves: [{ type: "security", id: "CVE-2019-13232" }]`.
No `waybill:` property is needed for the fact itself.

Rows still required, continuing the catalogue (highest current row: **C181**):

| Row | Field | Scope | Native carrier considered | Verdict |
|---|---|---|---|---|
| **C182** | `waybill:closure-role` | component | CDX `component.scope` is `required`/`optional`/`excluded` — about inclusion, not about whether a thing built the artifact or went into it. SPDX 2.3 `Package.primaryPackagePurpose` has no build-tool value that means this. | **KEEP-NO-NATIVE** |
| **C183** | `waybill:patch-evidence-grade` | component | No format models how confidently a CVE was associated with a patch. CDX `issue.source` names *where* an issue is tracked, not the strength of the link. | **KEEP-NO-NATIVE** |
| **C184** | `waybill:nix-closure` | document | doc-scope record: attribute evaluated, derivations seen, emitted by role, patches, patches without a CVE (FR-010, FR-018). | **KEEP-NO-NATIVE**, same shape as C178 |
| — | patch data in SPDX 2.3 / SPDX 3 | package | Neither has a `pedigree` equivalent. | **Bridge required** — the only asymmetry here |

Every row needs its extractor in the same change, or
`every_catalog_row_has_an_extractor` fails — and it checks both directions.

## State transitions

```
  flag off ─────────────────────────► manifest-derived set only (unchanged)
      │
  flag on
      ▼
  ┌─ nix usable, IFD refusal verified? ── no ──► degrade (m1034 reasons)
  │      yes
  ▼
  ┌─ packages.<system>.default resolves? ─ no ──► degrade(NoEvaluableAttribute),
  │      yes                                       naming available attributes
  ▼
  ┌─ closure query within budget? ─────── no ──► degrade(BudgetExceeded)
  │      yes
  ▼
  classify by role ──► emit components (marked) ─┐
                  └──► patches ──► pedigree ─────┼──► document
                              └──► two VEX stmts ┘
                                   (both graded)
```

Every degradation leaves the manifest-derived set intact and emits a reason —
the closure supplements, so losing it costs only the supplement.
