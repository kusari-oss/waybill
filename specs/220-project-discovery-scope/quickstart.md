# Quickstart: `--project-discovery=<mode>`

**Feature**: 220-project-discovery-scope | **Date**: 2026-07-24

## For operators

### Three modes, one flag

| Mode | Behavior | Use when |
|--|--|--|
| `all` (default) | Every reader discovers main-modules wherever they find qualifying manifests. m215 behavior. | Whole-repo scans; you want everything. |
| `root-only` | Only root-level main-modules + their ecosystem-native workspace-declared members. | Polyglot monorepo scans where you want to scope to one project without pulling in nested unrelated stuff. |
| `strict` | Only the root-level manifest itself. No workspace members. | Niche audit / compliance snapshots keyed on a single manifest file. |

### Scan a polyglot repo, ignoring nested projects

```sh
waybill sbom scan \
  --path ~/Projects/monorepo \
  --project-discovery=root-only \
  --format cyclonedx-json \
  --output monorepo-root.cdx.json
```

For a fixture with root `Cargo.toml` + nested `services/api/{package.json}` + `services/worker/{go.mod}`:

- **`--project-discovery=all`** (default): SBOM contains cargo main-module + npm main-module + go main-module + all three ecosystems' transitive deps.
- **`--project-discovery=root-only`**: SBOM contains ONLY the cargo main-module + its cargo transitive deps. No `pkg:npm/*` or `pkg:golang/*` components at all.

### Scan a Cargo workspace, honoring workspace members

```sh
waybill sbom scan \
  --path ~/Projects/my-cargo-workspace \
  --project-discovery=root-only \
  --format cyclonedx-json \
  --output workspace.cdx.json
```

For a workspace with root `Cargo.toml` (`[workspace] members = ["crates/api", "crates/worker"]`) + an unrelated `bench/Gemfile`:

- `services/api` + `services/worker` crates land in the SBOM as `waybill:workspace-member`-tagged components (because they ARE declared members).
- `bench/Gemfile`'s components DO NOT land (it's an independent nested project, not a workspace member).

### Compose with `--split[=<mode>]`

```sh
waybill sbom scan --path <root> --project-discovery=root-only --split=directory --output-dir out/
```

Scope filter runs first (drops nested projects), then split-directory grouping runs — typically producing ONE sub-SBOM (the root's directory group).

### Verify the FR-011 doc-scope annotation

```sh
jq '.metadata.properties[]? | select(.name == "waybill:project-discovery-mode")' monorepo-root.cdx.json
# → {"name":"waybill:project-discovery-mode","value":"root-only"}
```

Silent on default-mode scans (per SC-005 byte-identity contract).

### Inspect the FR-012 INFO log

```sh
RUST_LOG=info waybill sbom scan --path <root> --project-discovery=root-only ... 2>&1 | grep "project-discovery"
# INFO waybill::scan_fs: scan: project-discovery=root-only root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=2
```

- `nested_projects_ignored` is the operator-visible signal of "how much did the scope cap actually change." Non-zero = the mode had a real effect.

## For contributors

### Iterate on the filter

```sh
# Unit tests for ProjectDiscoveryMode + is_root_in_scope + apply_scope_filter.
cargo +stable test -p waybill --bin waybill -- generate::project_discovery

# Integration tests for the flag + end-to-end filter behavior.
cargo +stable test -p waybill --test project_discovery_scope
```

### Add a new mode variant (extensibility contract)

Say you want to add `--project-discovery=explicit=<paths>` (operator supplies a list of root manifests):

1. Add variant to enum:
   ```rust
   pub enum ProjectDiscoveryMode {
       All,
       RootOnly,
       Strict,
       ExplicitPaths(Vec<PathBuf>),  // NEW
   }
   ```
2. Extend the `is_root_in_scope` match arm to check against the explicit path list.
3. Add a row to `docs/reference/project-discovery.md` mode table.
4. Add an integration test scenario in `waybill-cli/tests/project_discovery_scope.rs`.

Zero changes required to CLI flag parsing (clap re-derives), the filter pass, the doc-scope annotation, or the FR-012 INFO log — the enum's method surface abstracts the mode.

### Pre-PR gate

```sh
./scripts/pre-pr.sh
```

Both clippy `-D warnings` and full-workspace test MUST pass. Read `feedback_prepr_gate_bails_on_first_failure.md` memory before treating any failure as a flake.

### SC-005 byte-identity verification

Optional manual check after each significant refactor:

```sh
# Build alpha.68 release binary elsewhere (git worktree at v0.1.0-alpha.68).
alpha68_out=/tmp/alpha68-baseline.cdx.json
/path/to/alpha68/waybill sbom scan --path <fixture> --output "$alpha68_out"

# Run m220 branch binary with default mode against the same fixture.
m220_out=/tmp/m220-default.cdx.json
./target/release/waybill sbom scan --path <fixture> --output "$m220_out"

# Diff should be empty (byte-identical).
diff "$alpha68_out" "$m220_out"
```

Any diff = SC-005 violation. Investigate immediately.

## For SBOM consumers

Read `docs/reference/project-discovery.md` (post-merge) for:
- The full mode table with when-to-choose guidance.
- Per-ecosystem workspace-member detection matrix.
- Worked examples per mode + consumer-side JSON extraction snippets.
- Decision tree for consumers ingesting scoped-vs-full SBOMs.
- Extensibility contract for future modes.

## Verification checklist

- [ ] `waybill sbom scan --help` shows `--project-discovery=<PROJECT_DISCOVERY>` with `[possible values: all, root-only, strict]`.
- [ ] Default mode (or `--project-discovery=all`) produces byte-identical output to alpha.68 on every existing m215+ fixture.
- [ ] `--project-discovery=root-only` on a polyglot-nested fixture drops nested-project components entirely.
- [ ] `--project-discovery=root-only` on a Cargo workspace fixture preserves workspace-member components.
- [ ] `--project-discovery=strict` on the same Cargo workspace drops workspace members.
- [ ] `--project-discovery=nonexistent-mode` → CLI parse error listing accepted values; exit non-zero.
- [ ] C140 doc-scope annotation present iff mode ≠ All.
- [ ] FR-012 INFO log substring `project-discovery=root-only` present when `--project-discovery=root-only` passed.
- [ ] `docs/reference/project-discovery.md` exists + linked from README + `docs/index.md` + covers all 6 required sections.
