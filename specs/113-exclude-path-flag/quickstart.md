# Quickstart — User-Supplied Directory Exclusion

**Feature**: 113-exclude-path-flag
**Audience**: Operators running `mikebom scan` who hit fixture-as-component false positives.

## The problem

Your repository has a `tests/fixtures/` (or `examples/sample-projects/`, or `services/*/testdata/`) subtree containing throwaway sample projects in one or more ecosystems — Cargo crates, Maven submodules, pip packages, npm packages — used purely to drive integration tests. When you scan, mikebom dutifully walks into those subdirectories, finds their manifests, and emits each fixture as if it were a real component. Sometimes the fixture manifests also declare synthetic `requires` lines pointing at your parent project, which then surface as spurious dependency edges in the SBOM.

The Go ecosystem has a documented convention (`testdata/` and `_`-prefixed directories are reserved) that mikebom honors unconditionally. Other ecosystems don't — so mikebom can't auto-skip those subtrees without surprising users who legitimately track real components there.

## The fix

Pass `--exclude-path <PATH_OR_PATTERN>` to your scan command. Repeat the flag for multiple subtrees. mikebom skips matched subtrees at every ecosystem walker's descent decision, so the fixture components and their synthetic edges disappear from the emitted SBOM.

## Single fixture directory at repo root

```text
$ mikebom scan --exclude-path tests/fixtures /path/to/repo > sbom.cdx.json
```

`tests/fixtures` is a literal path (no `*`, `?`, or `[`) and is anchored at the scan root. The subtree at `<repo>/tests/fixtures/**` is skipped; everything else scans normally.

## Multiple fixture directories

```text
$ mikebom scan \
    --exclude-path tests/fixtures \
    --exclude-path examples/sample-projects \
    /path/to/repo > sbom.cdx.json
```

Repeated `--exclude-path` flags accumulate. A directory matched by either entry is skipped.

## Fixture directories scattered across a monorepo

```text
$ mikebom scan --exclude-path '**/testdata' /path/to/monorepo > sbom.cdx.json
```

`**/testdata` is a glob pattern (contains `*`). It matches any directory named `testdata` at any depth — `services/a/testdata`, `services/b/testdata`, `apps/web/internal/testdata`, etc. Shell-quote the pattern so your shell doesn't expand `*` itself.

## Persistent exclusion list via env var

```text
$ export MIKEBOM_EXCLUDE_PATH='tests/fixtures:**/testdata'   # Unix
$ set MIKEBOM_EXCLUDE_PATH=tests\fixtures;**\testdata        # Windows
$ mikebom scan /path/to/repo > sbom.cdx.json
```

Use your operating system's path-list separator (`:` on Unix, `;` on Windows — same convention as `$PATH`). CLI-supplied entries and env-var entries combine by union.

## Inspect the transparency annotation

When any exclusion is in effect, the emitted SBOM carries an envelope-level annotation listing the active entries. Consumers can see at a glance that this isn't an exhaustive component list:

```text
$ jq '.metadata.properties[] | select(.name == "mikebom:exclude-path")' sbom.cdx.json
{
  "name": "mikebom:exclude-path",
  "value": "tests/fixtures,**/testdata"
}
```

(SPDX 2.3 and SPDX 3 outputs carry the equivalent annotation on the document.)

## Validate the SBOM is byte-identical without the flag

If you remove all `--exclude-path` flags and unset `MIKEBOM_EXCLUDE_PATH`, the emitted SBOM is byte-for-byte identical to one produced by a pre-feature build of mikebom against the same inputs (FR-003 / SC-002). No silent behavioral change.

## What this flag is NOT

- **Not auto-detection**: mikebom doesn't guess which directories are fixtures. You name them.
- **Not negation**: there's no `!keep-this-inside-excluded-tree` syntax in v1. Structure your entries to name only the dirs you want skipped.
- **Not for files**: each entry matches directories; an operator who wants to suppress one manifest names its containing directory instead.
- **Not for built-in skips**: `vendor/`, `node_modules/`, `target/`, `dist/`, `build/`, `__pycache__/`, and the Go-specific `testdata/`/`_*` skips are unconditional and remain in place regardless of `--exclude-path`. You never need to re-state them.

## What changes in your scan output

| Without `--exclude-path` | With `--exclude-path <pattern>` |
|---|---|
| Every manifest under the matched subtree emits a component | Zero components from the matched subtree |
| Synthetic `requires` from fixture manifests emit as real dependency edges | Zero dependency edges referencing matched-subtree components |
| No `mikebom:exclude-path` annotation in SBOM envelope | One annotation listing all active entries |
| No log output | One `info`-level summary line at scan end; per-match `debug`-level lines if `RUST_LOG=debug` |

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `error: --exclude-path entry <X>: <globset::Error>` | Pattern has unmatched brackets or invalid glob syntax. Quote the value and check `*`/`?`/`[` placements. |
| `error: --exclude-path entry was empty` | An empty string was passed (e.g. `--exclude-path ""`). Remove the empty entry. |
| Fixture component still appears in SBOM | Pattern didn't match. Run with `RUST_LOG=debug` and look for `exclude-path: matched <dir> against entry <X>` lines. If none, your pattern doesn't match the actual path; try `--exclude-path '**/<dir-name>'`. |
| Real component disappeared | Your exclusion is too broad. Use a more specific path or pattern. |
| Different result on Windows vs Linux | Should not happen — mikebom normalizes path separators at both ends. File a bug with both invocations and both SBOMs. |

## Acceptance validation (for implementers)

Run the polyglot fixture suite:

```text
$ cargo +stable test --test exclude_path_integration -p mikebom
```

Expected: all per-ecosystem suppression tests pass, the polyglot union test passes, the byte-identity test passes against the committed golden.
