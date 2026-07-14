# Contract: Emission Shape (m194)

**Date**: 2026-07-14
**Scope**: Byte-level wire-shape assertions the integration tests enforce.

## Fixture A — Go source repo (US1)

**Setup**: minimal `go.mod` (module `example.com/foo`) + `main.go` importing `github.com/spf13/cobra`. `go.sum` present. Scanned with mikebom, native root detection (no `--root-name`).

**Expected CDX shape** (post-m194):

```json
{
  "components": [
    {
      "name": "foo",
      "purl": "pkg:golang/example.com/foo@v0.0.0-<git-describe>",
      "properties": [
        {"name": "mikebom:component-role", "value": "main-module"}
      ]
    },
    {
      "name": "stdlib",
      "purl": "pkg:golang/stdlib@v1.22.0"
    },
    {"name": "cobra", "purl": "pkg:golang/github.com/spf13/cobra@v1.8.0"}
  ],
  "dependencies": [
    {
      "ref": "pkg:golang/example.com/foo@v0.0.0-<git-describe>",
      "dependsOn": [
        "pkg:golang/github.com/spf13/cobra@v1.8.0",
        "pkg:golang/stdlib@v1.22.0"
      ]
    },
    ...
  ],
  "metadata": {
    "properties": [
      {"name": "mikebom:graph-completeness", "value": "complete"}
    ]
  }
}
```

**Key assertions**:
- `metadata.properties[?(@.name=="mikebom:graph-completeness")].value == "complete"` (was `partial: orphaned-components-detected: 1` pre-m194)
- `dependencies[?(@.ref=="pkg:golang/example.com/foo@...")].dependsOn` includes `"pkg:golang/stdlib@v1.22.0"` (the new synthetic edge)
- Grep for the reason annotation: NONE (`mikebom:graph-completeness-reason` should be absent)

## Fixture B — Nested nameless npm workspace (US2)

**Setup**: Two-level project.
- `<root>/package.json` — `{"name": "@my/pkg", "version": "1.0.0", "dependencies": {"axios": "1.5.0"}}`
- `<root>/package-lock.json` — resolves axios
- `<root>/nested/package.json` — `{"dependencies": {"chalk": "5.0.0"}}` (NAMELESS)
- `<root>/nested/package-lock.json` — resolves chalk + its transitives

**Expected CDX shape** (post-m194):

```json
{
  "components": [
    {"name": "@my/pkg", "purl": "pkg:npm/%40my/pkg@1.0.0",
     "properties": [{"name": "mikebom:component-role", "value": "main-module"}]},
    {"name": "nested", "purl": "pkg:npm/nested",
     "properties": [{"name": "mikebom:component-role", "value": "main-module"}]},
    {"name": "axios", "purl": "pkg:npm/axios@1.5.0"},
    {"name": "chalk", "purl": "pkg:npm/chalk@5.0.0"}
  ],
  "dependencies": [
    {"ref": "pkg:npm/%40my/pkg@1.0.0", "dependsOn": ["pkg:npm/axios@1.5.0"]},
    {"ref": "pkg:npm/nested", "dependsOn": ["pkg:npm/chalk@5.0.0"]},
    ...
  ],
  "metadata": {
    "properties": [
      {"name": "mikebom:graph-completeness", "value": "complete"}
    ]
  }
}
```

**Key assertions**:
- A `pkg:npm/nested` component exists (synthesized nested mainmod).
- Its bom-ref/purl is versionless (per m191 spec-clean shape).
- Its properties include `mikebom:component-role: main-module`.
- `dependencies[?(@.ref=="pkg:npm/nested")].dependsOn` includes `"pkg:npm/chalk@5.0.0"`.
- Graph completeness reports `complete`.

## Fixture C — `--root-name` override with both stdlibs + nested workspaces (US1 + US2 + m192/m193 interaction)

**Setup**: mixed Go + npm project (Fixture A + Fixture B shape combined) scanned with `--root-name X --root-version Y`.

**Expected shape**:
- Both Go mainmod AND both npm mainmods (top-level + nested) get DROPPED by `apply_main_module_drop_or_demote`.
- m192/m193 pre-rewrite re-anchors ALL dropped mainmods' outgoing edges onto `target_ref = X@Y`.
- Post-rewrite `X@Y` has direct edges to: cobra (Go), stdlib (Go), axios (top-level npm), chalk (nested npm).
- BFS from `X@Y` reaches every component.
- `mikebom:graph-completeness == "complete"`.

## Byte-identity gate

For any golden that does NOT have:
- A `pkg:golang/stdlib@v*` component (drift from US1)
- A nested nameless `package.json` under a discovered project root (drift from US2)

The emitted SBOM MUST be byte-identical post-m194. Verified by workspace regression suite.

## Cross-format consistency

All shape changes (synthetic Go stdlib edge, synthesized nested npm mainmod + its edges) propagate identically to:
- CDX 1.6 `components[]` + `dependencies[]`
- SPDX 2.3 `packages[]` + `relationships[]` (DEPENDS_ON entries)
- SPDX 3 `software_Package` graph elements + `Relationship` graph elements with `relationshipType: dependsOn`

Because both fixes operate at the pre-emission `Vec<PackageDbEntry>` + `Vec<Relationship>` level, all three format emitters see the same data.
