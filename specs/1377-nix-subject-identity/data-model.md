# Data model: milestone 1377

All state is in-process for one scan, as for every closure milestone.

## Identifier row (from the meta evaluation)

One row per `(name, root)` that resolves to a derivation, as milestone 1281
already returns, with two fields added (research R1):

| field | type | source |
| --- | --- | --- |
| `attr`, `root`, `drv`, `out`, `licenses` | as today | unchanged |
| `version` | `Option<String>` | `p.version or null` |
| `cpe` | `Option<String>` | `p.meta.identifiers.cpe or null`, inside its own `tryEval` |

## DeclaredCpe

A CPE 2.3 formatted string that parsed (Principle IV). It is closure-local and
is not a change to the shared `ResolvedComponent.cpes: Vec<String>`.

- 13 colon-separated fields, beginning `cpe:2.3:`, part ∈ {`a`, `o`, `h`}, vendor and product non-empty and not `*`.
- A value that fails is **rejected**: not emitted, and counted.

## Join of a member to a row

1. A row whose `drv` equals the member's `.drv` basename → basis `drv`.
2. Otherwise a row whose `attr` equals the member's `pname` and whose `version` equals the member's version → basis `name-version`.
3. Otherwise unreached.

A member reached both ways takes `drv`. Several `name-version` rows with
*different* CPEs leave the member without a declared CPE (ambiguous,
counted with `rejected`), as 1281 withholds an ambiguous license.

## CpeProvenance (per CPE-bearing closure component)

| field | values | when |
| --- | --- | --- |
| `source` | `nixpkgs-declared` \| `curated` | always, on closure components with a CPE |
| `basis` | `drv` \| `name-version` | `nixpkgs-declared` only |
| `overrode` | the curated CPE string, or absent | `nixpkgs-declared` only, when the curated table would have produced a different CPE |

Precedence for a closure component's single CPE: a valid declared CPE, then a curated CPE, then none.

## IdentifierCoverage (extends the C184 summary)

| field | meaning |
| --- | --- |
| `wanted` | emitted closure members, the same population as `licenses.wanted` |
| `received` | members joined to a row by either basis |
| `by-drv`, `by-name-version` | split of `received` |
| `declared` | members carrying a valid declared CPE |
| `rejected` | declared values that failed to parse, plus ambiguous name-version joins |

Invariants: `by-drv + by-name-version = received ≤ wanted`; `declared + rejected ≤ received`.
The unreached count FR-009 asks for is `wanted − received`.

## Evaluated-package tag

An internal annotation, never emitted (`is_internal_emission_key`), set by the
closure on exactly one component: the one whose derivation is
`ClassifiedClosure.graph.root`. Set on none when `root` is `None`.

## Root selection

- `ScanTargetKind::NixClosure`: set when a closure resolved.
- `RootSelectionHeuristic::NixEvaluatedPackage`: `"nix-evaluated-package"`, 0.99.
- Subject: `ResolvedRootSubject::MainModule(idx)` of the tagged component.
- Ladder position: below the operator override, above every main-module rung.
