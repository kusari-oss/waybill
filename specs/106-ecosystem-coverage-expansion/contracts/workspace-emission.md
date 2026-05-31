# Contract: Workspace emission model (FR-015, US1 + US2)

Shared emission policy for uv and Bun workspace projects, derived from Clarification Q1.

## Trigger

Either:
- Root `pyproject.toml` declares `[tool.uv.workspace]` with a `members = [...]` array (uv).
- Root `package.json` declares a top-level `workspaces` field with an array of glob patterns or paths (Bun).

## Emission shape

For a workspace with `<N>` members, mikebom emits:

1. **1 synthetic workspace-root component**
2. **`<N>` workspace-member components**
3. **`<N>` `dependsOn` edges** from workspace-root → each member
4. **Variable intra-workspace edges** between members, ONLY where source manifests declare them
5. **Transitive components** for external (PyPI / npm) deps, with edges from the appropriate member

### Workspace-root component

| Field | Value |
|---|---|
| PURL | `pkg:generic/<name>` where `<name>` derives from root manifest's `name` field (or `"workspace-root"` placeholder when absent / empty) |
| Properties | `mikebom:component-role: "workspace-root"` |
| Properties | `mikebom:source-files: "<path-to-root-manifest>"` |
| Hashes | empty (synthetic component, no on-disk file beyond the root manifest) |

### Workspace-member component

| Field | Value |
|---|---|
| PURL | `pkg:pypi/<name>@<version>` (uv) or `pkg:npm/<name>@<version>` (Bun, scoped names URL-encoded) |
| Properties | `mikebom:component-role: "main-module"` |
| Properties | `mikebom:source-files: "<path-to-member-manifest>"` (member's `pyproject.toml` or `package.json`) |

### Edges

- Workspace-root → each member: `dependsOn` (CDX), `DEPENDS_ON` (SPDX 2.3), `dependsOn` (SPDX 3).
- Member-A → Member-B (intra-workspace dep): emitted ONLY when Member-A's manifest declares it (uv `source = { workspace = true }` in `[[package.dependencies]]`, Bun `"workspace:*"` source-spec in member's `package.json` `dependencies`). Independent members produce NO edges between them.
- Member → external transitive: standard `dependsOn` edge to a PyPI/npm component.

## Worked examples

### Example 1: uv workspace (Cargo-style monorepo)

```
my-monorepo/
├── pyproject.toml              # [tool.uv.workspace] members = ["apps/web", "libs/shared"]
├── uv.lock                     # contains all resolved packages
├── apps/web/
│   └── pyproject.toml          # name = "web", dependencies = ["shared", "httpx"]
└── libs/shared/
    └── pyproject.toml          # name = "shared", dependencies = ["pydantic"]
```

Expected SBOM output (component list + edges):

```
Components:
  pkg:generic/my-monorepo                         (workspace-root)
  pkg:pypi/web@0.1.0                              (main-module, workspace member)
  pkg:pypi/shared@0.1.0                           (main-module, workspace member)
  pkg:pypi/httpx@0.27.2                           (external transitive)
  pkg:pypi/pydantic@2.9.2                         (external transitive)

Edges:
  pkg:generic/my-monorepo → pkg:pypi/web@0.1.0
  pkg:generic/my-monorepo → pkg:pypi/shared@0.1.0
  pkg:pypi/web@0.1.0      → pkg:pypi/shared@0.1.0    (intra-workspace dep)
  pkg:pypi/web@0.1.0      → pkg:pypi/httpx@0.27.2
  pkg:pypi/shared@0.1.0   → pkg:pypi/pydantic@2.9.2

NO edge between web and shared except the one direction declared above.
```

### Example 2: Bun workspace (same shape, JS)

```
my-app/
├── package.json                # "name": "my-app", "workspaces": ["packages/*"]
├── bun.lock
├── packages/web/
│   └── package.json            # "name": "@my/web", "dependencies": { "@my/shared": "workspace:*" }
└── packages/shared/
    └── package.json            # "name": "@my/shared"
```

Expected SBOM output mirrors Example 1 but with `pkg:npm/...` PURLs (scoped names use `%40` per the PURL spec).

### Example 3: Independent workspace members (no inter-dep)

```
my-tools/
├── pyproject.toml              # workspace = ["foo", "bar"]
├── foo/pyproject.toml          # no workspace deps
└── bar/pyproject.toml          # no workspace deps
```

Expected output:

```
Components: workspace-root + foo + bar + their respective externals
Edges:
  workspace-root → foo
  workspace-root → bar
  (no edge foo ↔ bar)
```

This matches the user's stated requirement: "if it's clearly independent components like foo and bar that there's no dependency between those two".

## Implementation notes

- The workspace-root component is constructed by the per-ecosystem reader (uv_lock.rs or bun_lock.rs) when it detects the workspace marker. Generic helpers may be extracted to `scan_fs/package_db/workspace.rs` if duplication emerges between uv and Bun implementations.
- Synthetic workspace-root components have no on-disk SHA-256 hash — the `hashes` field is empty.
- The C40 catalog row's `mikebom:component-role` annotation gains a new enum value `"workspace-root"`. Per research R3, the row's enum is open (not closed), so this is a doc-only update: edit the row's narrative in `docs/reference/sbom-format-mapping.md:86-87` to mention the new value alongside existing `"build-tool"`, `"language-runtime"`, `"main-module"`. No parity extractor changes needed.

## Parity verification

The existing C40 row's extractor (one of `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!`) passes any string value through. `"workspace-root"` flows through identically to `"main-module"` — no byte-identity break, no parity test changes needed. The new milestone-106 fixtures with workspace shapes will produce goldens that the existing parity round-trip suite validates as SymmetricEqual.
