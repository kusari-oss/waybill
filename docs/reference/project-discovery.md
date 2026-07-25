# `--project-discovery=<mode>` — cap main-module discovery scope

Milestone 220 introduces a CLI flag that caps how waybill discovers
main-modules within a scan root. This page documents the three modes,
their per-ecosystem behavior, interactions with `--split[=<mode>]`,
and how consumers detect that a given SBOM was scoped.

## The three modes

| Mode        | Behavior                                                                                                                                                      | Use when                                                                                                                                                              |
|-------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `all` (default) | Every reader discovers main-modules wherever it finds qualifying manifests. Byte-identical to pre-m220 output on every existing fixture (SC-005 gate).      | Whole-repo scans; you want everything.                                                                                                                                 |
| `root-only` | Only root-level main-modules + their ecosystem-native workspace-declared members. Independent nested projects are dropped entirely.                          | Polyglot monorepo scans where you want to scope to one project without pulling in nested unrelated stuff.                                                              |
| `strict`    | Only the root-level manifest(s) themselves — no workspace-member walking, even for ecosystem-native `[workspace]` / `workspaces` / `go.work` / `<modules>`. | Niche audit / compliance snapshots keyed on a single manifest file.                                                                                                    |

**Flag syntax**: `--project-discovery=<mode>` (requires `=`; space-separated
form rejected). Env-var equivalent: `WAYBILL_PROJECT_DISCOVERY=<mode>`
(bridged from the CLI arg when set non-default).

**Invalid values** exit non-zero with a stderr message listing the
three accepted values.

## Interaction matrix vs `--split[=<mode>]`

Both flags govern the same pipeline stage but compose orthogonally.
`--project-discovery` filters main-modules FIRST; `--split[=<mode>]`
groups the remaining set into sub-SBOMs.

| `--project-discovery` | `--split` (default `workspace`) | Result                                                                            |
|-----------------------|---------------------------------|-----------------------------------------------------------------------------------|
| `all` (default)       | (omitted)                       | m215 default: single SBOM with all discovered main-modules.                       |
| `all`                 | `workspace`                     | m215/m219: one sub-SBOM per main-module.                                          |
| `all`                 | `directory`                     | m219: one sub-SBOM per canonical source-dir group.                                |
| `root-only`           | (omitted)                       | Single SBOM covering only root-level main-modules + workspace members.            |
| `root-only`           | `workspace`                     | One sub-SBOM per root-level main-module (typically 1; 2 on polyglot-root fixtures). |
| `root-only`           | `directory`                     | One sub-SBOM for the root directory group (typically 1).                          |
| `strict`              | any                             | Same as `root-only` but with workspace-declared members excluded.                 |

## Per-ecosystem workspace-member detection

Workspace-member detection is reused verbatim from each ecosystem's
existing reader. m220 does NOT extend or invent detection logic. When a
reader stamps `waybill:workspace-member = <root-purl>` on a component,
that component is preserved under `root-only`. When it doesn't, the
component is treated as an independent nested project and dropped.

| Ecosystem                          | Ecosystem-native workspace signal                       | Reader stamps `waybill:workspace-member`? | Value shape                                    |
|------------------------------------|---------------------------------------------------------|-------------------------------------------|------------------------------------------------|
| **Cargo**                          | `[workspace] members = [...]` in root `Cargo.toml`      | ✅ Yes (m127)                              | PURL of the workspace root                     |
| **npm / pnpm / yarn**              | `"workspaces": [...]` in root `package.json`            | ✅ Yes (m147 / m180)                       | PURL of the workspace root                     |
| **Go workspaces**                  | `use (...)` in root `go.work`                           | ✅ Yes (m161)                              | PURL derived from `go.work` root               |
| **Maven multi-module**             | `<modules>...</modules>` in root `pom.xml`              | ✅ Yes (m085)                              | PURL of the parent POM                         |
| **pyproject** (poetry/hatch/setuptools) | Varies per tool                                    | ⚠️ Reader-dependent                        | Whatever the reader decides                    |
| **Gemfile**                        | No workspace concept in Ruby                            | ❌ N/A                                     | Every Gemfile is an independent project        |
| **Composer / dart / etc.**         | Varies                                                  | ⚠️ Reader-dependent                        | Whatever the reader decides                    |

Ecosystems marked ❌ have no workspace concept — `root-only` and
`strict` produce identical output on scans containing only those
ecosystems.

## Worked examples

### Example 1: Cargo workspace

```
my-workspace/
├── Cargo.toml               # [workspace] members = ["crates/*"]
├── Cargo.lock
├── crates/
│   ├── api/Cargo.toml       # workspace member
│   └── worker/Cargo.toml    # workspace member
└── bench/
    └── Gemfile              # NOT a workspace member — independent Ruby project
```

- `--project-discovery=all`: SBOM contains workspace root + both crates + `rack` gem transitives.
- `--project-discovery=root-only`: SBOM contains workspace root + both crates + their cargo deps.
  **NO `pkg:gem/*` components** — `bench/Gemfile` is dropped.
- `--project-discovery=strict`: SBOM contains workspace root's OWN deps only.
  **NO crate members. NO `pkg:gem/*`.**

### Example 2: Polyglot monorepo

```
monorepo/
├── Cargo.toml               # root cargo project
├── src/lib.rs
└── services/
    ├── api/                 # nested npm project (not workspace-member)
    │   ├── package.json
    │   └── package-lock.json
    └── worker/              # nested go project (not workspace-member)
        ├── go.mod
        └── go.sum
```

- `--project-discovery=all`: 3 main-modules + all 3 ecosystems' transitives.
- `--project-discovery=root-only`: cargo main-module + cargo deps ONLY.
  **NO `pkg:npm/*`. NO `pkg:golang/*`.**
- `--project-discovery=strict`: same as `root-only` for this fixture
  (cargo project has no workspace members).

### Example 3: Ruby-only project (m216)

```
gem-app/
├── Gemfile
├── Gemfile.lock
└── main.rb
```

Ruby has no workspace concept. `--project-discovery=root-only` and
`--project-discovery=strict` produce identical output — both cover
the root `Gemfile` and its deps.

## C140 doc-scope annotation `waybill:project-discovery-mode`

When the scan runs under a non-default mode, waybill emits a document-
scope annotation containing the mode name:

**CycloneDX**:

```json
{
  "metadata": {
    "properties": [
      { "name": "waybill:project-discovery-mode", "value": "root-only" }
    ]
  }
}
```

**SPDX 2.3** (document-level `Annotation` on `SPDXRef-DOCUMENT`, wrapped
in the `MikebomAnnotationCommentV1` envelope):

```json
{
  "annotations": [
    {
      "annotator": "Tool: waybill-<version>",
      "annotationDate": "<ISO-8601>",
      "annotationType": "OTHER",
      "comment": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:project-discovery-mode\",\"value\":\"root-only\"}"
    }
  ]
}
```

**SPDX 3.0.1** (Annotation element on the SpdxDocument root IRI; same
envelope).

**Silence-on-default**: absent under `--project-discovery=all` (default).
The absence + presence discipline means byte-identity is preserved on
every existing test fixture — no goldens regenerate.

**Consumer detection** (jq):

```sh
jq '.metadata.properties[]? | select(.name == "waybill:project-discovery-mode") | .value' scan.cdx.json
# → "root-only"     (or "strict", or nothing if scan used default `all`)
```

## FR-012 INFO log

When mode is non-default, scan-driver exit emits an INFO log line:

```
INFO waybill::scan_fs: scan: project-discovery mode complete mode=root-only root_main_modules=1 workspace_members_followed=2 nested_projects_ignored=3
```

- `root_main_modules`: count of root-level main-modules discovered + retained.
- `workspace_members_followed`: count of workspace-member components pulled in
  via annotation follow-up (belt-and-suspenders + FR-005 fixpoint recursion).
- `nested_projects_ignored`: count of main-modules that WOULD have been in the
  SBOM under `all` mode but were dropped. This is the operator-visible signal
  of "how much did the scope cap actually change" — non-zero means the mode
  had a real effect.

## FR-008 fallback: zero root-level manifests

If `--project-discovery=root-only` (or `strict`) runs on a scan root
that contains ZERO root-level manifests (every project is nested under
`services/*/{package.json, go.mod, ...}`), waybill emits a WARN log
naming the mode and falls back to full-scope emission — the SBOM is
still produced with the same components it would have under `all`. The
C140 annotation is NOT emitted on the fallback branch, so consumers can
distinguish "scope was applied cleanly" from "scope was requested but
had no target to apply to."

## Extensibility contract

Adding a new mode variant (e.g., `explicit=<paths>` or `depth=<N>`)
touches only these five files:

1. `waybill-cli/src/generate/project_discovery/mod.rs` — add the enum variant.
2. Its `is_root_in_scope` / `follows_workspace_members` match arms.
3. This docs page's mode table.
4. `waybill-cli/tests/project_discovery_scope.rs` — a new integration test.
5. (Optional) `docs/reference/sbom-format-mapping.md` — if a new C-row is
   introduced for a mode-specific annotation.

Zero touches to CLI flag parsing (clap re-derives), the filter pipeline,
the C140 doc-scope annotation shape, or the FR-012 INFO log — the enum's
method surface abstracts the mode.
