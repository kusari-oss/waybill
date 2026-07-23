# Split-mode grouping strategies (milestone 219)

Consumer + contributor guide to `waybill sbom scan --split[=<mode>]`.
Two modes today; extensible to more per FR-007.

## 1. Mode table

| Mode | Behavior | Use when |
|--|--|--|
| `workspace` (default) | One sub-SBOM per detected main-module (m215 semantics). | You want per-package artifacts; downstream consumers key on subproject identity. |
| `directory` | One sub-SBOM per canonicalized source directory. All main-modules whose dirs match merge into ONE SBOM. | Polyglot repos where npm + go + cargo coexist in one dir; consumers organizing by directory (Backstage, IDE plugins). |

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
