# Split-mode grouping strategies (milestone 219)

Consumer + contributor guide to `waybill sbom scan --split[=<mode>]`.
Two modes today; extensible to more per FR-007.

## 1. Mode table

| Mode | Behavior | Use when |
|--|--|--|
| `workspace` (default) | One sub-SBOM per detected main-module (m215 semantics). | You want per-package artifacts; downstream consumers key on subproject identity. |
| `directory` | One sub-SBOM per canonicalized source directory. All main-modules whose dirs match merge into ONE SBOM. | Polyglot repos where npm + go + cargo coexist in one dir; consumers organizing by directory (Backstage, IDE plugins). |
| `resolve` (#902) | One sub-SBOM per Pants resolve. **Selects by membership, not by walking the graph.** | Pants monorepos, where a resolve is a real dependency-resolution boundary and a vulnerability in a linting resolve should be distinguishable from one in the production resolve. |

### `resolve` mode differs from the other two in kind

`workspace` and `directory` both enumerate **main-modules** and walk the graph
from each. A Pants resolve is not a main-module, and a resolve found by the
`3rdparty/python/*.lock` convention has no anchor component at all — so there
is nothing for a walk to start from. `resolve` mode therefore filters: a
component belongs to resolve R when its `waybill:pants-resolve` names R.

Consequences worth knowing before you consume the output:

- **A package pinned by several resolves appears in several documents**, each
  time carrying its **full** membership rather than one narrowed to the
  containing document. So a reader triaging one resolve's SBOM can see the
  same fix lands in another. The cost is that a document may name resolves
  whose packages it does not contain — correct, not a dangling reference.
- **Edges separate without being tagged.** An edge belongs to resolve R when
  both its endpoints name R. A component in two resolves depending on a
  package those resolves pin differently reaches BOTH versions in the unsplit
  document, and exactly one in each per-resolve document.
- **Every per-resolve document says which resolve it is**, in the doc-scope
  `waybill:document-resolve` annotation (catalogue row C163), as a
  namespace-qualified name:

  ```
  default.generic.cdx.json   waybill:document-resolve = ["python:default"]
  lint.generic.cdx.json      waybill:document-resolve = ["python:lint"]
  ```

  The name is qualified because `[python.resolves]` and `[jvm.resolves]` are
  separate namespaces in `pants.toml`, so one repository may declare `default`
  in both and a bare name could not tell them apart.

  The annotation is present on **every** per-resolve document, including those
  whose root component already names the resolve, so a consumer needs one code
  path rather than one per provenance. Where both are present they agree.

  It is **absent** — not empty — from an unsplit document and from
  `--split=workspace` / `--split=directory`, where the question has no answer,
  and from anything produced before waybill 0.10.0.

  This matters when a document is separated from its manifest. The manifest's
  `root_purl` maps every file to its resolve, including discovered ones, so
  while the two travel together either answers. A file renamed, attached to a
  ticket, or handed to a scanner has only its own bytes.

  A resolve discovered by filename convention still names the **repository** in
  its root component, and deliberately so: synthesising an owning component
  would assert an ownership the repository never declared
  ([#887](https://github.com/kusari-oss/waybill/issues/887)). Naming a resolve
  is information; inventing a component that owns its packages is a claim.
- **Two resolves sharing a name across namespaces are different resolves, and
  get different documents.** `[python.resolves]` and `[jvm.resolves]` are
  separate namespaces in `pants.toml`, so a repository may declare `default` in
  both. Grouping is on the namespace-qualified resolve, and each component
  carries `waybill:pants-resolve-namespace` (catalogue row C164) so a consumer
  partitioning membership itself can tell them apart too.

  Where a name collides, **both** documents are namespace-qualified — filename,
  manifest `subproject_id` and manifest `root_purl` alike:

  ```
  python-default.generic.cdx.json    the Python resolve
  jvm-default.generic.cdx.json       the JVM resolve
  lint.generic.cdx.json              no collision — unchanged
  ```

  Qualification applies **only** where a collision exists. A repository whose
  resolve names are unique keeps byte-identical filenames and manifest entries.
  Scripts that key on split filenames are affected only if the repository
  actually has a name collision — in which case they were reading a document
  containing two resolves' packages before.

  Fixed in [#919](https://github.com/kusari-oss/waybill/issues/919); before
  that the two merged into one document whose contents were the union of a
  Python resolve and a JVM resolve. The merge was also invisible in a
  repository whose *only* resolves were the colliding pair: the split counted
  one group, decided the repository was not partitionable, and emitted **no
  split at all**.
- **No resolves, or only one**, falls back to a single SBOM with a warning,
  the same as the other modes with fewer than two boundaries.

## 2. Worked example — `workspace` mode (bare / explicit)

```sh
waybill sbom scan --path ~/Projects/monorepo --split --output-dir ./sboms/
# OR
waybill sbom scan --path ~/Projects/monorepo --split=workspace --output-dir ./sboms/
```

For a fixture with `services/api/{Cargo.toml, package.json}` + `services/worker/{go.mod}`:

```sh
ls ./sboms/
# m219-api.cargo.cdx.json      ← per-main-module m215 shape
# m219-api.npm.cdx.json
# m219-worker.golang.cdx.json
# split-manifest.json
```

Manifest entry (single-member, no `members[]` field):

```json
{
  "subproject_id": "m219-api.cargo",
  "root_purl": "pkg:cargo/m219-api@0.1.0",
  "source_dir": "services/api",
  "component_count": 12,
  "shared_deps_count": 0,
  "files": {"cyclonedx-json": "m219-api.cargo.cdx.json"}
}
```

## 3. Worked example — `directory` mode

```sh
waybill sbom scan --path ~/Projects/monorepo --split=directory --output-dir ./sboms/
```

Same fixture, different grouping:

```sh
ls ./sboms/
# services-api.multi.cdx.json  ← merged: pkg:cargo/m219-api + pkg:npm/m219-api
# m219-worker.golang.cdx.json  ← single-member, m215 filename verbatim
# split-manifest.json
```

Manifest entry (multi-member, WITH `members[]` field):

```json
{
  "subproject_id": "services-api.multi",
  "root_purl": "pkg:generic/services-api@0.0.0-unknown",
  "source_dir": "services/api",
  "component_count": 20,
  "shared_deps_count": 0,
  "files": {"cyclonedx-json": "services-api.multi.cdx.json"},
  "members": [
    {"purl": "pkg:cargo/m219-api@0.1.0", "source_dir": "services/api"},
    {"purl": "pkg:npm/m219-api@0.1.0", "source_dir": "services/api"}
  ]
}
```

Filename convention for multi-member groups: `<dir-slug>.multi.<format-ext>`. `<dir-slug>` derives from the canonicalized `source_dir` with `/` → `-`; empty source_dir → `"root"` sentinel.

## 4. `split-manifest.json` schema evolution

The `members: [{purl, source_dir}]` field is **additive-optional** (per m219 Q1 clarification):
- OMITTED when a group covers exactly one main-module (m215 wire-shape byte-identity preserved).
- PRESENT (sorted lex by `purl`) when a group covers ≥2 members.
- Schema URL unchanged: `https://waybill.dev/schema/split-manifest/v1.json`.

**m215 consumers**: no code change needed. `.members` doesn't exist → they don't read it → single-member entries look identical to alpha.67.

**m219-aware consumers**: check `if entry.get("members").is_some()` to detect multi-member groups.

## 5. Extensibility contract (for contributors)

Adding a future grouping strategy (e.g., `--split=ecosystem`, `--split=owner`) requires touching only 4 surfaces:

1. **The enum variant list** in `waybill-cli/src/generate/split.rs`:
   ```rust
   pub enum SplitMode {
       Workspace, Directory,
       Ecosystem,  // NEW
   }
   ```
2. **The `group_key` match arm** in the same file:
   ```rust
   SplitMode::Ecosystem => root.ecosystem.clone(),
   ```
3. **This docs page's mode table** (§1 above).
4. **A new test scenario** in `waybill-cli/tests/split_modes.rs`.

**Zero changes** required to:
- CLI-flag definition (clap re-derives `ValueEnum` automatically).
- Split-manifest schema (already flexible via additive-optional `members[]`).
- `emit_split` orchestration (grouping is data-driven; the orchestrator iterates `Vec<GroupedProjection>` blind to the mode).
- Filename computation (single-member groups use m215 shape; multi-member groups use m219 `<dir-slug>.multi`; both branches key off `members.len()`, not the mode).

**SC-009 mechanical verification**: `sc009_extensibility_gate_hand_add_ecosystem_variant` in `generate/split.rs::tests` proves this contract at test time — the test defines a `TestOnlySplitMode` variant + match arm inline and demonstrates distinct group_keys, without touching any file outside the enum's home.

## 6. FR-010 INFO log

Every `--split=<mode>` invocation emits a log line at split-driver exit:

```
INFO waybill::generate::split: split emission complete mode=directory groups=2 total_main_modules=3
```

- `mode`: `workspace` or `directory` (lowercase per `SplitMode::Display`).
- `groups`: number of sub-SBOMs emitted.
- `total_main_modules`: number of main-modules the walker discovered.

For `--split=directory` on a polyglot dir, `groups < total_main_modules` (the merge visible in the counters).

## 7. Failure modes

| Input | Behavior |
|---|---|
| `--split=nonexistent-mode` | Clap parse error; non-zero exit; stderr lists `workspace`, `directory`. |
| `--split=""` (empty value) | Clap parse error. |
| `--split=DIRECTORY` (uppercase) | Clap parse error (`rename_all = "lowercase"` normalizes only rendering, not accepted casing). |
| `--split directory` (space-separated) | Clap parse error via `require_equals = true`. Use `--split=directory`. |
| `--split=directory` without `--output-dir` | Existing m215 error: `--split requires --output-dir`. |
| `--split=directory --output out.json` | Existing m215 error: `--split` conflicts with `--output`. |
| `--split=directory` on a scan with zero main-modules | Fallback: WARN log + single SBOM in `--output-dir`; no `split-manifest.json`. |

## References

- Spec: `specs/219-split-modes/spec.md`
- Plan: `specs/219-split-modes/plan.md`
- Payload contract: `specs/219-split-modes/contracts/manifest-additive-members.md`
- Grouping strategy contract: `specs/219-split-modes/contracts/grouping-strategy.md`
- Filename contract: `specs/219-split-modes/contracts/multi-member-filename.md`
- CLI flag contract: `specs/219-split-modes/contracts/split-mode-flag.md`
