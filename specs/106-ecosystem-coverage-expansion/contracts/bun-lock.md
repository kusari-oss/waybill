# Contract: bun.lock reader (US2, FR-003, FR-004, FR-015)

**New modules**: 
- `mikebom-cli/src/scan_fs/package_db/npm/bun_lock.rs` (the reader)
- `mikebom-cli/src/scan_fs/package_db/npm/jsonc.rs` (JSONC comment stripper — see `contracts/jsonc-stripper.md`)

## Trigger

A file named `bun.lock` at the scan root. (The legacy binary `bun.lockb` format is explicitly OUT OF SCOPE per spec.)

## Parsing

1. Read file as `String`.
2. Pass through `npm::jsonc::strip_comments()` to remove `//` line comments + `/* */` block comments while preserving string-literal contents.
3. `serde_json::from_str::<serde_json::Value>` for untyped JSON access (the schema is flexible — we walk the JSON manually rather than typing every shape).

## PURL derivation per package

The bun.lock `packages` map keys have the shape `"<name>@<source-spec>"` where `<source-spec>` can be:
- A semver string (e.g. `lodash@4.17.21`) → registry package → `pkg:npm/<name>@<version>`
- A workspace marker (`@my/web@workspace:packages/web`) → workspace member → `pkg:npm/<name>@<version>` + `mikebom:component-role: "main-module"`
- A git URL (`my-pkg@git+https://...`) → `pkg:git+https://<url>@<rev>`
- A tarball URL (`my-pkg@https://...tgz`) → `pkg:npm/<name>@<version>` with `mikebom:download-url` annotation

Scoped names (`@my/web`) URL-encode the `@` to `%40` per PURL spec: `pkg:npm/%40my/web@...`.

## Workspace handling

When the root `package.json` declares `workspaces: ["..."]`:

1. Emit a synthetic workspace-root component:
   - `pkg:generic/<root-name>` (where `root-name` = root `package.json`'s `name` field, or `"workspace-root"` placeholder)
   - `mikebom:component-role: "workspace-root"`
2. Walk `bun.lock`'s `workspaces` map (excluding key `""` = root); each non-root entry is a workspace member.
3. Add `dependsOn` edges from workspace-root → each member.
4. Resolve `"workspace:*"` source entries in member `dependencies` to intra-workspace edges.

## Annotations emitted (per component)

| Annotation | Value |
|---|---|
| `mikebom:source-files` | absolute path of `bun.lock` (root) or member's `package.json` (members) |
| `mikebom:component-role` | `"workspace-root"` / `"main-module"` / absent (as above) |
| `mikebom:download-url` | tarball URL for tarball-source packages |

## Edge cases handled

- **JSONC comments inside string values**: the stripper respects string-literal boundaries — comments inside `"..."` strings are preserved as part of the string content.
- **`overrides` map**: Bun's `bun.lock` can declare overrides; the overridden version wins. The un-overridden version is NOT also emitted as a separate component.
- **Top-of-file marker comment** (`// bun: lockfileVersion: 1`): strips cleanly via the JSONC handler.

## Test fixtures

- `tests/fixtures/golden_inputs/bun_lock/basic/` — simple registry packages
- `tests/fixtures/golden_inputs/bun_lock/scoped_packages/` — `@scope/name` PURL encoding
- `tests/fixtures/golden_inputs/bun_lock/workspace/` — workspace-root + 2 members
- `tests/fixtures/golden_inputs/bun_lock/overrides/` — override version wins
