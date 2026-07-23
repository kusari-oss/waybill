# Quickstart: Split-mode grouping strategies

**Feature**: 219-split-modes | **Date**: 2026-07-23

## For operators

### Choose a mode

Two accepted values (v1):

| Mode | Behavior | Use when |
|--|--|--|
| `workspace` (default) | One sub-SBOM per detected main-module (m215 semantics). | You want per-package artifacts; downstream consumers key on subproject identity. |
| `directory` | One sub-SBOM per canonicalized source directory. All main-modules whose dirs match are merged into one SBOM. | You want per-directory artifacts; polyglot repos where npm + go + cargo coexist in one dir. |

### Scan a polyglot monorepo with `--split=directory`

```sh
waybill sbom scan \
  --path ~/Projects/monorepo \
  --split=directory \
  --output-dir ./sboms/ \
  --format cyclonedx-json
```

For a fixture with `services/api/{Cargo.toml, package.json}` and `services/worker/{go.mod}`:

```sh
ls ./sboms/
# services-api.multi.cdx.json    ← merged: pkg:cargo/api + pkg:npm/api
# worker.golang.cdx.json         ← single-member: m215 filename verbatim
# split-manifest.json
```

### Preserve m215 default behavior

```sh
waybill sbom scan --path ~/Projects/monorepo --split --output-dir ./sboms/
# OR (explicit)
waybill sbom scan --path ~/Projects/monorepo --split=workspace --output-dir ./sboms/
```

Both invocations produce output byte-identical to alpha.67 `--split`. Same three sub-SBOMs on the fixture above:
```
api.cargo.cdx.json
api.npm.cdx.json
worker.golang.cdx.json
```

### Inspect the multi-member manifest entry

```sh
jq '.entries[] | select(.subproject_id == "services-api.multi")' ./sboms/split-manifest.json
```

Output:
```json
{
  "subproject_id": "services-api.multi",
  "root_purl": "pkg:generic/services-api@0.0.0-unknown",
  "source_dir": "services/api",
  "component_count": 123,
  "shared_deps_count": 5,
  "files": {"cyclonedx-json": "services-api.multi.cdx.json"},
  "members": [
    {"purl": "pkg:cargo/api@0.1.0", "source_dir": "services/api"},
    {"purl": "pkg:npm/api@0.1.0", "source_dir": "services/api"}
  ]
}
```

### Verify no unexpected merges

```sh
# Count files vs count of main-modules per group:
jq '.entries[] | {subproject_id, members_count: (.members // [] | length)}' ./sboms/split-manifest.json
```

Groups with `members_count == 0` are single-member (m215 shape); groups with `members_count >= 2` are m219 merged groups.

## For contributors

### Iterate on the grouping logic

```sh
# Run the SplitMode + group_key unit tests standalone (fast).
cargo +stable test -p waybill --lib generate::split::tests

# Run the m219 integration tests (uses the two_dir_polyglot + single_dir_polyglot fixtures).
cargo +stable test -p waybill --test split_modes

# SC-005 byte-identity gate: existing m215 split tests MUST pass unchanged.
cargo +stable test -p waybill --test split_transitive_edges  # or whatever the m215 suite is called
```

### Add a new grouping variant (extensibility contract)

Say you want to add `--split=ecosystem` (one sub-SBOM per PURL ecosystem type):

1. Add the variant to the enum:

   ```rust
   pub enum SplitMode {
       Workspace,
       Directory,
       Ecosystem,  // NEW
   }
   ```

2. Add the `group_key` match arm:

   ```rust
   pub fn group_key(&self, root: &SubprojectRoot) -> String {
       match self {
           SplitMode::Workspace => root.subproject_id(),
           SplitMode::Directory => { /* ... */ }
           SplitMode::Ecosystem => root.ecosystem.clone(),  // NEW
       }
   }
   ```

3. Add a row to `docs/reference/split-modes.md` mode table.

4. Add a test scenario to `waybill-cli/tests/split_modes.rs`.

No other files need changes. CLI-flag definition, split-manifest schema, filename computation, `emit_split` orchestration — all unchanged. If any of those surfaces required editing, the extensibility contract is broken (SC-009 gate).

### Pre-PR gate

Per Constitution mandatory verification:

```sh
./scripts/pre-pr.sh
```

Both clippy `-D warnings` and full-workspace test MUST pass. Read `feedback_prepr_gate_bails_on_first_failure.md` memory before treating any failure as a flake.

### SC-005 byte-identity verification

Manual (recommended after each significant refactor):

```sh
# Build alpha.67 release binary elsewhere (git worktree at v0.1.0-alpha.67).
alpha67_out=/tmp/alpha67-split-fixture-out
mkdir -p "$alpha67_out"
/path/to/alpha67/waybill sbom scan --path <fixture> --split --output-dir "$alpha67_out"

# Run m219 branch binary with bare --split against the same fixture.
m219_out=/tmp/m219-split-fixture-out
mkdir -p "$m219_out"
./target/release/waybill sbom scan --path <fixture> --split --output-dir "$m219_out"

# Diff should be empty (byte-identical).
diff -r "$alpha67_out" "$m219_out"
```

Any diff = SC-005 violation. Investigate immediately.

## For SBOM consumers

Read `docs/reference/split-modes.md` for:
- The full mode table with when-to-choose guidance.
- Worked examples per mode + consumer-side JSON extraction snippets.
- Decision tree for consumers ingesting merged-vs-single-member groups.
- Extensibility contract for future contributors.

## Verification checklist

- [ ] `waybill sbom scan --help` shows `--split [<SPLIT>]` with `[possible values: workspace, directory]`.
- [ ] Bare `--split` on any m215 fixture → byte-identical output to alpha.67.
- [ ] `--split=workspace` explicit → byte-identical to bare `--split`.
- [ ] `--split=directory` on polyglot two-dir fixture → 2 sub-SBOMs (one per dir).
- [ ] `--split=directory` on polyglot two-dir fixture → merged sub-SBOM's `components[]` contains both members' main-modules.
- [ ] `--split=nonexistent-mode` → CLI parse error listing accepted values; exit non-zero.
- [ ] Multi-member `SplitEntry` in manifest carries `members: [...]` sorted lex by `purl`.
- [ ] Single-member `SplitEntry` in manifest has NO `members` field (jq returns `null`).
- [ ] FR-010 INFO log line contains `mode=directory` when `--split=directory` passed.
- [ ] `docs/reference/split-modes.md` exists + linked from README + covers 6 required sections.
