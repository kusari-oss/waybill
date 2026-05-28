# Contract: `west.yml` reader (US3)

**Maps to**: FR-005 | **Source-mechanism**: `zephyr-west` | **New module**: `mikebom-cli/src/scan_fs/package_db/west.rs`

## Trigger

A file named `west.yml` appears at the scan root or in any `<scan-root>/**/.west/`. The reader picks up:

- `<scan-root>/west.yml`
- `<scan-root>/.west/config` references (informational only — for the actual manifest path we use `west.yml` directly)

## Parsing

`serde_yaml::from_str::<WestManifest>` (the crate is already a direct dep — research R2). See `data-model.md` for the struct shape.

### Resolved URL derivation per project

```
remote_url_base = project.remote || defaults.remote || ERROR (warn-and-skip the project)
repo_segment    = project.repo_path || project.name
full_url        = remote_url_base.trim_end('/') + '/' + repo_segment + '.git'
sanitized_url   = sanitize_userinfo(full_url)   ← FR-016
```

### PURL derivation

```
if sanitized_url matches https://github.com/<org>/<repo>(.git)?:
    PURL = pkg:github/<org>/<repo>@<revision>
else:
    PURL = pkg:git+https://<sanitized_url>@<revision>
```

## Annotations emitted

| Annotation | Value |
|---|---|
| `mikebom:source-mechanism` | `"zephyr-west"` |
| `mikebom:source-files` | absolute path of `west.yml` |
| `mikebom:download-url` | the sanitized `full_url` |
| `mikebom:groups` | (optional) comma-joined `project.groups` list when non-empty |

## CLI extension

`--exclude-group <name>` (per US3 scenario 3): repeatable flag. Projects in any
listed group are skipped. Default = emit all groups.

Argument validation: the flag is a `Vec<String>` via clap derive; group names
are compared case-sensitively against `project.groups[]`.

## Test cases (US3 acceptance scenarios mapped)

| US3 Scenario | Fixture | Assertion |
|---|---|---|
| 1 (basic project + revision) | `golden_inputs/west/basic/west.yml` | `pkg:github/zephyrproject-rtos/hal_stm32@a1b2c3d4...` |
| 2 (multi-remote routing) | `golden_inputs/west/multi_remote/` | PURL hosts differ per project's resolved remote |
| 3 (groups + `--exclude-group`) | `golden_inputs/west/groups/` | scan with `--exclude-group babblesim` drops those projects |
| 4 (Zephyr v4.4.0 real corpus) | integration test in `tests/transitive_parity_cpp_phase2.rs` | ≥79 components emerge |

## Edge cases handled

- **`import:` directives**: Captured into `WestManifest.imports` but **not chased** in this milestone. A `tracing::info!` event records the deferred import for operator awareness.
- **`path:` override**: Used for `mikebom:source-files` lookup when the local checkout exists; otherwise omitted. Does not affect PURL or version (we use `revision:`).
- **Missing `revision:`**: project is warn-and-skipped (FR-013 — the file remains parseable; we just don't emit a malformed component).
