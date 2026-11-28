# Contracts

No wire shape changes and no catalog rows change.

| Row | Before (#1744 fixture) | After |
|---|---|---|
| Components named `waybill-fixture-frozen` | 2: one versionless, one `@4.0.0` | 1: `@4.0.0` |
| Dependency edge from the member to `@4.0.0` | absent | present, in all three formats |
| C45 `waybill:orphan-reason` on `@4.0.0` | `edgeless-lock-pin` | absent |
| `waybill:source-files` of `@4.0.0` | `cabal.project.freeze` | `cabal.project.freeze` (unchanged) |
| Negative-control pin with no user | `edgeless-lock-pin` | `edgeless-lock-pin` |
