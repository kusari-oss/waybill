# Data model

## C200 path string

| Case | Before | After |
|---|---|---|
| under the root | `/home/runner/.cache/waybill/corpus/<id>/<pin>/repo/server/package.yaml` | `server/package.yaml` |
| the root's own manifest | `/…/repo/package.yaml` | `package.yaml` |
| above the root (R3) | `/…/shared/defaults.yaml` | `../shared/defaults.yaml` |

The separator is `/` on every host. Each path is relative to the canonical scan root (R2).

## `split-manifest.json` `source_dir`

| Subproject | Before | After |
|---|---|---|
| root `package.yaml` | `package.yaml` | `` (empty) |
| `local-packages/a/package.yaml` | `local-packages/a/package.yaml` | `local-packages/a` |
| `local-packages/a/a.cabal` | `local-packages/a/a.cabal` | `local-packages/a` |
| `crates/api/Cargo.toml` | `crates/api` | `crates/api` (unchanged) |
