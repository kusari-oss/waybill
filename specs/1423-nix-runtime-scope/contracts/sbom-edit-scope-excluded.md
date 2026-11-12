# Contract: `sbom edit` selector `scope=excluded`

`--drop`, `--keep` and every other selector-taking operation accept `scope=excluded`. It can be combined with the other scope values, e.g. `scope=excluded,development`.

| format | a component matches `scope=excluded` when |
|---|---|
| CycloneDX | `"scope": "excluded"` with no `waybill:lifecycle-scope` or `waybill:optional-derivation` property (those mean a lifecycle scope, which CycloneDX also writes as `excluded`) |
| SPDX 2.3 | it is the `spdxElementId` of a `BUILD_TOOL_OF` relationship |
| SPDX 3 | it is a `to` of a `LifecycleScopedRelationship` with `relationshipType: usesTool` and `scope: build` |

## Known limit (CycloneDX)

CycloneDX has one field for both "not in the artifact" and "a non-runtime lifecycle scope". A component that is both (for example a closure member that a lifecycle pass also scoped as development) is matched by `scope=excluded` in SPDX but not in CycloneDX. The literal rule (any `excluded`) was rejected: on moat it selected 1,320 in CycloneDX against 1,309 in SPDX (breaking SC-004), and it would select npm dev dependencies (breaking US3 scenario 2). Measured during US3; the user guide states the limit.

## Behaviour changes

- **CycloneDX `scope: excluded` with no lifecycle property:**
  - it no longer matches `scope=development`;
  - this affects closure members and Go `build_inclusion: NotNeeded` components;
  - the CHANGELOG states it.
- **Documents with real lifecycle scopes:**
  - every other `scope=` selection is unchanged (SC-006);
  - `scope=development` still matches components whose `waybill:lifecycle-scope` or native lifecycle relationship says development.

## Runtime-only recipe (FR-011)

```sh
waybill sbom edit closure.cdx.json --drop 'scope=excluded' -o runtime.cdx.json
```

The same command works on `.spdx.json` and `.spdx3.json`, and selects the same PURL set in all three (SC-004). It depends on #1428 for documents that carry VEX about dropped components.
