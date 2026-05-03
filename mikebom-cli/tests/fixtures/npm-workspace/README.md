# npm-workspace fixture (milestone 066)

Two-member npm 7+ workspace exercising:

- Workspace root with `private: true` + `workspaces: ["packages/*"]`
  + no `version` → main-module emission MUST be skipped per FR-002.
- Each member emits its own `pkg:npm/<name>@0.5.0` main-module.
- `b → a` workspace path-dep edge per FR-011.
- `documentDescribes` MUST list both `a` and `b` SPDXIDs sorted
  alphabetically (US3 AS#2).

Used by integration tests in `tests/scan_npm.rs`.
