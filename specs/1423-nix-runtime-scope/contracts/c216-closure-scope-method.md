# Contract: C216 `waybill:closure-scope-method`

**Scope**: document. **Encoding**: a JSON object serialized as a string, with keys sorted, the same as C204.
**Carriers**:
- CycloneDX: `metadata.properties[]`, `{"name": "waybill:closure-scope-method", "value": "<json>"}`
- SPDX 2.3: a document-scope `annotations[]` entry
- SPDX 3: a document-scope `Annotation`
**Parity**: `SymmetricEqual`, with an extractor in each format.
**Present when**: the `--nix-closure` tier ran (C184 present). Absent otherwise, including under `--offline`, where C190 already reports `offline-requested`.

## Shape

```json
{
  "method": "runtime-closure" | "reachability-walk" | "closure-role",
  "store": "local" | "<substituter URL>" | null,
  "verified": true | false | null,
  "unavailable-reason": null | "not-in-any-store" | "verification-failed" | "query-timed-out" | "no-evaluated-package" | "query-failed",
  "static-link-source": "declared-edges" | "derivation-fields" | "none" | null,
  "static-c-linking-detected": false,
  "unmatched-runtime-paths": <non-negative integer> | null,
  "walk-outcome": "used" | "reached-only-root" | null,
  "schema": 1
}
```

## Rules

- `method = runtime-closure`:
  - `store` and `verified` are non-null;
  - `verified` is `true`, because an unverified answer is never used;
  - `unavailable-reason` is null.
- `method = reachability-walk`:
  - `store` and `verified` are null;
  - `unavailable-reason` is non-null;
  - `walk-outcome` is `used`.
- `method = closure-role`:
  - the tier ran, but the scope pass could not use either evidence source;
  - `unavailable-reason` is non-null, and is the reason the runtime closure was unavailable;
  - `walk-outcome` is `reached-only-root` when a walk ran and was set aside, and null when no walk could run (`no-evaluated-package`: there is no root).
- A walk is set aside (`reached-only-root`) when it reaches no closure member other than the evaluated root **and** the root consumed a Haskell-builder derivation through `inputs.drvs`. That is shellcheck's `justStaticExecutables` wrapper, which names its static build only in `inputs.drvs` and an `installPhase` path. Used, such a walk would mark every library linked into the binary, and the fallback errs toward runtime, never toward dropping linked code (FR-002). For the same reason the role rule does not decide the Haskell build(s) the root consumed through `inputs.drvs` (other than one it names only in a build-machine field, such as shellcheck's `pandoc-cli`), nor the libraries linked into them: those are runtime, with C203 `statically-linked`. A root that reaches only itself and consumed no Haskell build keeps the walk: gh's walk reaches 1 of 788 derivations and is exact on its findings (55 true, 0 false), where the role rule leaves 185 false-runtime findings (`measurements/us2-walk-score/`, re-scored unchanged after this rule in `score-followup.json`).
- `walk-outcome` is null for `runtime-closure`: no walk ran.
- `static-link-source` is `none` when nothing seeds static linking: the runtime closure (under `reachability-walk`, the walk's reached set) contains no Haskell-builder member and, under `runtime-closure`, the evaluated root consumed no Haskell build through `inputs.drvs`. A wrapper root's consumed static build seeds it even when the runtime closure does not hold that build, as on x86_64-linux (`measurements/nix-behaviour/q10-*`). For `closure-role` it is null, except with `walk-outcome: reached-only-root`, where it is the source that found the libraries linked into the consumed Haskell build(s).
- `static-c-linking-detected` is always `false`. It exists so the document says, rather than implies, that static C linking is not detected (spec edge case).
- `unmatched-runtime-paths` counts runtime store paths that joined to no emitted component, including a path whose name matches a component but whose version does not (the join is by output path, never by name). It is null unless `method = runtime-closure`.

## Catalog row (sbom-format-mapping.md)

- **Classification**: KEEP-NO-NATIVE.
- **Justification**: CycloneDX 1.6, SPDX 2.3 and SPDX 3.0.1 have no document-scope construct for how component scope was decided (research R5).
