# Contract: git submodule reader (US6)

**Maps to**: FR-008, FR-008a, FR-009 | **Source-mechanism**: `git-submodule` | **New module**: `mikebom-cli/src/scan_fs/package_db/git_submodule.rs`

## Trigger

A `.gitmodules` file at the scan root. (Nested `.gitmodules` files inside
submodules are NOT recursively chased — per edge case "nested submodules".)

## Parsing

`.gitmodules` is git's standard config format:

```
[submodule "name"]
    path = third_party/foo
    url = https://github.com/org/foo.git
```

Parser: a tiny INI-style state machine (no `git2` crate; reusing the
config-file pattern from existing readers).

## HEAD revision resolution (no `git` subprocess per Assumptions)

For each submodule at `<scan-root>/<path>`:

1. Read `<scan-root>/.git/modules/<submodule-name>/HEAD`.
   - If it contains a 40-char hex SHA: that's the revision.
   - If it contains `ref: refs/heads/<branch>`: read `<scan-root>/.git/modules/<submodule-name>/refs/heads/<branch>`.
2. Fallback: read `<scan-root>/.git/modules/<submodule-name>/packed-refs` for the matching ref.
3. If all paths fail (uninitialized submodule), `head_revision = None` → emit with `version: "unknown"` + `mikebom:resolver-step: "uninitialized-submodule"` (FR-009).

## URL sanitization (FR-016)

The `.gitmodules` `url` field is passed through `sanitize_userinfo` before
any PURL or annotation construction. A `tracing::warn!` is emitted naming
`.gitmodules` and the redacted URL when sanitization changes the URL.

## PURL derivation

| URL form | PURL |
|---|---|
| `https://github.com/<org>/<repo>.git` | `pkg:github/<org>/<repo>@<head-revision>` |
| `https://gitlab.com/<org>/<repo>.git` | `pkg:gitlab/<org>/<repo>@<head-revision>` (if `pkg:gitlab/` is established; otherwise `pkg:git+https://`) |
| Any other `https://` URL | `pkg:git+https://<sanitized-url>@<head-revision>` |
| `git@github.com:<org>/<repo>.git` | `pkg:github/<org>/<repo>@<head-revision>` (after URL normalization) |
| `git@other-host:...` | `pkg:git+ssh://<sanitized-normalized-url>@<head-revision>` |

## `find_package` correlation (FR-008a) → `mikebom:build-reference`

Performed in two phases:

**Phase 1 — global accumulation** (happens during the existing cmake walk):

The cmake reader pass collects every `find_package(<target> ...)` call into a
`Set<String>` of normalized target names (case-folded). Stored on the
`ScanContext` so all readers can see it. This phase **does NOT emit
components from `find_package` calls** — preserving milestone 102's FR-007
(`find_package_does_not_emit_components` test stays green).

**Phase 2 — per-submodule classification**:

For each submodule, compute `last_path_segment = path.file_name().to_lowercase()`.
If `find_package_targets.contains(last_path_segment)` → `mikebom:build-reference: "declared-and-used"`.
Else → `mikebom:build-reference: "declared-only"`.

Target-alias resolution: when a `CMakeLists.txt` contains `add_library(Foo::Foo ALIAS bar)` and elsewhere `find_package(Foo)`, the alias-target name (`Foo`) is added to the set during Phase 1. Dynamic aliases (set inside CMake macros/functions) are not chased.

## Annotations emitted

| Annotation | Value |
|---|---|
| `mikebom:source-mechanism` | `"git-submodule"` |
| `mikebom:source-files` | absolute path of `.gitmodules` |
| `mikebom:download-url` | sanitized URL |
| `mikebom:build-reference` | `"declared-and-used"` or `"declared-only"` (FR-008a) |
| `mikebom:resolver-step` | `"uninitialized-submodule"` (FR-009, only when applicable) |

## Test cases (US6 acceptance scenarios + edge cases)

| Scenario | Fixture | Assertion |
|---|---|---|
| US6-1 (populated submodules) | `golden_inputs/git_submodule/populated/` | 3 components with correct commit SHAs |
| US6-2 (uninitialized) | `golden_inputs/git_submodule/uninitialized/` | component present with `unknown` + resolver-step annotation |
| US6-3 (gRPC integration) | `tests/transitive_parity_cpp_phase2.rs::grpc_submodule_count` | ≥16 components |
| Edge — name mismatch | `golden_inputs/git_submodule/name_mismatch/` | `declared-only` annotation |
| Edge — target alias | `golden_inputs/git_submodule/target_alias/` | `declared-and-used` via alias resolution |
| Edge — credentials in URL | `golden_inputs/git_submodule/with_creds/` | sanitized URL in PURL + warn log |
