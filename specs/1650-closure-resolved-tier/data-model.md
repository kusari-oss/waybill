# Data Model: Closure-resolved Haskell components are not labelled `design`

No new types and no new fields. The change reads two existing annotations and writes one existing field.

## The rule (`resolve::closure_resolved_tier::retier`)

For each component whose `sbom_tier` is `design`:

| Evidence on the component | Meaning | New tier | Requirement |
|---|---|---|---|
| C203 `waybill:scope-classification-source` = `exempt-evaluated-package` | the closure evaluated this package; it is a member of the build graph | `build` | FR-002 |
| C218 `waybill:boot-library-compiler` present | its version came from a compiler the closure holds, inside which it ships | `build` | FR-003 |
| neither | — | unchanged | FR-004 |

A component already `source`, `build` or `analyzed` is never touched.

## Preconditions

- **It runs only when the closure ran.** Only the closure writes C203's `exempt-evaluated-package` and C218. In a manifest-only scan the rule matches nothing, so FR-007's byte identity holds by construction, and a test checks it.
- **It runs after `closure_role_scope::assign_role_scope`,** which writes C203.

## Outputs

`RetierSummary { evaluated_package: usize, compiler_supplied: usize }`, logged at INFO when either count is non-zero.

## What a consumer reads

- `waybill:sbom-tier` = `build` on both groups.
- **The reason** is the evidence already present, which the docs now state implies `build` under `--nix-closure`:
  - C203 for the project package;
  - C218 for a boot library.
- **Document-level aggregates follow:** CycloneDX `metadata.lifecycles` and SPDX 3 `software_sbomType` lose `design` where no `design` component remains (research R7).

## I11 (corpus invariant)

- **Applies to** a document holding at least one component with C214 `waybill:closure-contents` (a closure member).
- **(a) Fails if** any component is `design` while carrying C203 = `exempt-evaluated-package` or C218.
- **(b) Fails if** the root has direct dependencies and all of them are `design`.
- **The failure message** names each offending component, and the root's direct dependencies with their tiers.
