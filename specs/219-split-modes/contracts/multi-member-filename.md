# Contract: `<dir-slug>.multi.<format-ext>` filename convention

**Feature**: 219-split-modes | **Related**: FR-006 (locked per Q2 clarification)

## Surface

When `emit_split` writes a sub-SBOM for a `GroupedProjection` whose `members.len() >= 2`, the emitted filename MUST match:

```
<dir-slug>.multi.<format-ext>
```

Examples:
- `services-api.multi.cdx.json` (CycloneDX)
- `services-api.multi.spdx.json` (SPDX 2.3)
- `services-api.multi.spdx3.json` (SPDX 3.0.1)
- `root.multi.cdx.json` (scan-root-level multi-member group; empty source_dir → `"root"` sentinel)

When `emit_split` writes a sub-SBOM for a `GroupedProjection` whose `members.len() == 1`, the emitted filename MUST match m215's existing convention verbatim (via the existing `filename_for` helper):

```
<slug>.<ecosystem>.<format-ext>
```

Examples (unchanged from m215):
- `libsafe.cargo.cdx.json`
- `frontend.npm.cdx.json`
- `worker.golang.spdx.json`

## `<dir-slug>` derivation

Deterministic function of `SubprojectRoot::source_dir`:

1. Take the canonicalized `source_dir` as `String` (via `to_string_lossy().to_string()`).
2. Substitute path separators: `/` → `-`, `\` → `-` (Windows).
3. Strip leading `-` (from absolute-path leading `/`).
4. Character-safety pass per m215's `subject_slug` at `split.rs:405-431`:
   - Strip: `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`, whitespace (space, tab, newline, carriage return).
   - Strip non-ASCII (defensive; PURL-name slugs are ASCII by spec).
5. Truncate to 100 bytes.
6. Lowercase.
7. If the result is empty (source_dir was empty → became empty after strip): substitute the literal `"root"`.

**Reference implementation** (proposed for `waybill-cli/src/generate/split.rs`):

```rust
pub(crate) fn dir_slug(source_dir: &str) -> String {
    let mut s = source_dir.replace('/', "-").replace('\\', "-");
    s = s.trim_start_matches('-').to_string();
    s.retain(|c| {
        !matches!(
            c,
            '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' ' | '\t' | '\n' | '\r'
        )
    });
    s.retain(|c| c.is_ascii());
    if s.len() > 100 { s.truncate(100); }
    s = s.to_ascii_lowercase();
    if s.is_empty() { "root".to_string() } else { s }
}
```

## Collision-safety

Under `--split=directory`, two distinct groups CANNOT share the same `<dir-slug>` because `group_key` IS the source_dir path — distinct paths → distinct group_keys → distinct slugs. Neighbor collision is impossible by construction.

**Edge case**: two distinct source_dirs that slugify to the same string after char-substitution (e.g., `services/api` and `services-api` both slugify to `services-api`). This is theoretically possible but requires an operator to have a top-level directory literally named `services-api` alongside a subdirectory `services/api` — a pathological polyglot layout.

**Fallback for slugify-collisions**: reuse m215's `build_collision_map` machinery. If two `<dir-slug>` outputs collide, both get a `-<sha8-hex>` suffix on the slug portion (the `.multi.<format-ext>` tail stays intact). Filename becomes:

```
<dir-slug>-<sha8-hex>.multi.<format-ext>
```

E.g., `services-api-a1b2c3d4.multi.cdx.json`.

## Reserved-name guards

- **Windows-reserved basenames** (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`): if `<dir-slug>` matches (case-insensitive), prepend `wb-` per m215's reserved-Windows-basename guard at `filename_for:461-466`. Filename becomes `wb-<dir-slug>.multi.<format-ext>`.
- **Manifest-name clash** with `split-manifest.json` itself: impossible for `.multi` filenames (the `.multi.` marker guarantees the two-token suffix; `split-manifest` has no `.multi.` in it).

## Filename determinism guarantees

For a given `(GroupedProjection, format_id)` tuple:
- The output filename IS deterministic.
- Same source_dir → same `<dir-slug>` → same filename (across scan runs, hosts, wall-clock times).
- Format-ext derives from `format_ext(format_id)` (m215's existing helper at `split.rs:373`) — identical semantics.

## Interaction with `SplitEntry.subproject_id` and `SplitEntry.root_purl`

Per R6 + E7: the manifest entry for a multi-member group carries:
- `subproject_id = <dir-slug>.multi` — matches the filename's slug+marker portion.
- `root_purl = pkg:generic/<dir-slug>@0.0.0-unknown` — synthetic; consumers who need actual member PURLs read `members[]`.

This gives operators + consumers a stable, self-describing correspondence: manifest.subproject_id + `.<format-ext>` = filename.

## Backward compat guarantees

- Single-member groups: filename convention IS m215's `<slug>.<ecosystem>.<format-ext>` — zero delta. SC-005 byte-identity gate passes.
- Multi-member groups: NEW convention. No m215 fixture produces multi-member groups (m215 always emits per-main-module), so no golden regen for existing fixtures.
