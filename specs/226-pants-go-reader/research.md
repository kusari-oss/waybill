# Phase 0: Research — Pants Go reader

**Feature**: 226-pants-go-reader
**Date**: 2026-08-03

Five research items derived from `plan.md` §"Critical Phase 0
items" + Constitution Check gate outputs. Each resolves an
ambiguity before Phase 1 design begins.

---

## R1 — Pants Go backend BUILD-file target grammar

**Decision**: Recognize four built-in target types from
`pants.backend.experimental.go.target_types`:

| Target function | Signature (kwargs subset waybill consumes) | Attribution role |
|-----------------|--------------------------------------------|------------------|
| `go_mod(name="X", **_)` | `name` (str, defaults to `"mod"`) | Owns the whole 3rdparty tree at the BUILD file's dir |
| `go_third_party_package(name="X", import_path="example.com/foo", **_)` | `name` (str) + `import_path` (str) | Owns exactly one third-party module |
| `go_binary(name="X", main="./cmd/foo", **_)` | `name` (str) + `main` (str, path relative to BUILD file's dir) | Owns the first-party main-module component |
| `go_package(name="X", **_)` | `name` (str, defaults to dir name) | Owns any first-party module component whose package dir is inside the BUILD file's dir |

**Empirical anchor**: verified against the Pants project's own
Go example at `github.com/pantsbuild/example-golang` (search
term: `go_mod(` — the built-in target types have not changed
shape since Pants 2.16, and the four listed above are documented
at `https://www.pantsbuild.org/reference/targets/go_binary`
and siblings.

**Rationale**: These four target types are the entire built-in
`pants.backend.experimental.go` API surface waybill needs for
attribution. `go_source` (file-level) and `go_test` (test-file
level) are not primary attribution roots — a `go_package` in
the same dir is the canonical owner. Plugin-registered custom
target types are silently ignored per spec Out-of-Scope.

**Alternatives considered**:
- **Include `go_source` / `go_test`**: rejected — file-level
  targets don't map cleanly to `pkg:golang/*` PURLs which are
  module-level. Attribution would be misleading ("owned by
  cmd/foo/main.go" isn't more useful than "owned by cmd/foo:pkg").
- **Include `pants.backend.experimental.golang.gogo_link`**:
  hypothetical linker target — not a real Pants target type.

---

## R2 — Regex-scoped BUILD-file DSL extraction (reuse m225 pattern)

**Decision**: Reuse the hybrid anchoring-regex + char-scan
approach from m225's
`waybill-cli/src/scan_fs/package_db/pants_shell/build_dsl.rs`:

- One anchoring regex per target-type call at line-start:
  `^\s*(go_mod|go_third_party_package|go_binary|go_package)\s*\(`
- Char-by-char `find_matching_close_paren` (with string-literal
  awareness) to identify the target's call body.
- Per-kwarg regexes for `name=`, `import_path=`, `main=`.
- Same fail-open contract: bad-shape target → typed
  `TargetParseError` variant → WARN + skip.

**Rationale**: The m225 extractor infrastructure is proven
(11 unit tests + 5 integration tests green) and handles
multi-line kwargs, arbitrary kwarg ordering, trailing commas,
and comment-lines inside target bodies. Duplicating the pattern
in pants_go's own `build_dsl.rs` is cheaper than a premature
refactor to `pants_common/` (YAGNI — deferred per plan.md
Follow-ups).

**Edge cases handled** (inherited from m225):
- Single vs double quote string literals
- Whitespace / newlines between kwargs
- Trailing commas
- Additional (ignored) kwargs (`dependencies=`, `tags=`, etc.)
- Concat / variable-reference in string fields → `NonStringLiteralSource` err

**Alternatives considered**:
- **Extract a shared `pants_common/build_walker/` module now**:
  rejected — YAGNI, only 2 consumers today. Refactor trigger is
  a third consumer (m227 Pants Docker or Kotlin).
- **Shell out to `pants peek`**: rejected — spec Assumptions
  explicitly precludes shell-out to `pants`.

---

## R3 — `go_mod` ownership root inference

**Decision**: A `pkg:golang/*` component belongs to a
`go_mod`-declared target iff its `source_path` (populated by
the existing Go reader — see
`waybill-cli/src/scan_fs/package_db/golang/legacy.rs:745,797,1080`)
falls under the directory containing the `go_mod`-declaring
`BUILD` file.

**Empirical evidence**: existing Go reader sets
`PackageDbEntry.source_path` to:
- The `go.sum` file's absolute path for third-party module
  entries (line 745, 797)
- The `go.mod` file's absolute path for main-module entries
  (line 1080)

**Inference algorithm**:

```text
For each parsed go_mod target at build_file = <dir>/BUILD:
    ownership_index.go_mod_roots.insert(<dir>, <address>)

For each pkg:golang/* component:
    for (root, address) in ownership_index.go_mod_roots:
        if component.source_path starts_with root:
            attach waybill:pants-target = address
            break  # go_mod ownership is unique per go.sum
```

**Multi-`go_mod` case**: multiple `go_mod` targets can coexist in
one repo (multi-module Go monorepo). The `starts_with root`
match uses the deepest matching root (longest prefix wins) so
`3rdparty/go/BUILD:go_mod` doesn't accidentally claim a
component owned by `services/api/3rdparty/go/BUILD:go_mod`.

**Rationale**:
- **Zero-fabrication invariant preserved**: we only annotate
  existing components; if there's no matching `pkg:golang/*`
  entry, nothing happens.
- **Robust across Pants layout variations**: works for the
  canonical `3rdparty/go/BUILD` + `3rdparty/go/go.mod` layout
  AND for less-common layouts like `services/foo/3rdparty/go/BUILD`.

**Alternatives considered**:
- **Match by import path prefix**: rejected — go_mod doesn't
  declare an import path; it's an implicit-container concept.
- **Require the go_mod BUILD file to be exactly at
  `3rdparty/go/BUILD`**: rejected — Pants supports custom
  layouts.

---

## R4 — Main-module attribution for `go_binary(main=...)` targets

**Decision**: waybill's existing Go reader tags the main-module
component with `waybill:component-role = "main-module"` in
`extra_annotations` (see
`waybill-cli/src/scan_fs/package_db/golang/legacy.rs:992`).
The pants_go enrichment pass identifies this component by that
annotation + the component's `source_path` (points at the
repo's `go.mod`).

**Attribution algorithm**:

```text
For each parsed go_binary(main="./cmd/foo") target at
build_file = <dir>/BUILD:
    resolved_main = normalize(<dir> + "/" + main)
    ownership_index.main_targets.push((<address>, resolved_main))

For each pkg:golang/* component with
extra_annotations["waybill:component-role"] == "main-module":
    for (address, resolved_main) in main_targets:
        if the component's source_path's parent equals or contains resolved_main:
            attach waybill:pants-target = address

For each parsed go_package target at build_file = <dir>/BUILD:
    ownership_index.package_targets.push((<address>, <dir>))

For each pkg:golang/* component with
extra_annotations["waybill:component-role"] == "main-module":
    for (address, package_dir) in package_targets:
        if source_path's parent starts_with package_dir:
            attach waybill:pants-target = address
```

**Rationale**:
- **Zero-fabrication invariant preserved** (same as R3).
- **Multi-address merge**: a main-module component may end up
  with multiple owning addresses (e.g., one from `go_binary`,
  one from `go_package`). The annotation merges them
  comma-separated + lexically sorted (SC-006 dedup contract
  from m225).

**Alternatives considered**:
- **Parse go.mod for the module path + match against `main=`
  path syntactically**: rejected — the module path is
  workspace-relative; `main=` is BUILD-file-relative. Path
  matching against `source_path.parent()` is more robust.
- **Only attach the `go_binary` address, not `go_package`
  addresses**: rejected per US3 — spec requires both.

---

## R5 — C145 broadening vs new C146 catalog row

**Decision**: **Broaden C145** with a doc-only description
update. No new C146 row.

**Rationale**: The C145 extractor macro (see
`waybill-cli/src/parity/extractors/cdx.rs:867`,
`spdx2.rs:622`, `spdx3.rs:682`) matches on annotation-key
`"waybill:pants-target"` only — it is ecosystem-agnostic and
tier-agnostic. The row's description currently ties the
semantic to "file-tier components emitted by the milestone-225
`pants_shell` reader", but this is prose documentation, not
functional constraint. Broadening the description to include
`pkg:golang/*` scope (emitted by m226 enrichment) is a
one-paragraph edit to `docs/reference/sbom-format-mapping.md`
row C145.

**Doc-update shape** (previewed at plan time; final wording
lives in `contracts/c145-broadening.md`):

> The C145 description gets a new second paragraph:
> "**Also emitted by milestone 226 (feature `226-pants-go-reader`)**
> on `pkg:golang/*` components enriched by the pants_go
> enrichment pass. For those components, the value is the
> Pants target address(es) whose `go_binary` / `go_package` /
> `go_third_party_package` / `go_mod` declaration(s) own the
> Go module — either as an implicit `go_mod`-root owner or an
> explicit `import_path=`/`main=` match."

**Machine-verified invariant**: `parity::extractors::tests::every_catalog_row_has_an_extractor`
already passes for C145 → remains green after the doc update
because the row_id + extractor triple are unchanged.

**Alternatives considered**:
- **Add a new C146 `waybill:pants-target-go` row + separate
  extractor triple**: rejected — the wire signal is identical
  (same annotation key, same semantic, same rejected native
  alternatives). Splitting into per-ecosystem rows would violate
  the existing catalog's grouping conventions (one row per
  annotation key, not per emission origin).
- **Leave C145 description as-is** and rely on operators
  reading the m226 spec: rejected — the docs/reference/ catalog
  is the canonical documentation surface; operators expect the
  row to describe every emission origin.

---

## Summary — resolved unknowns

- R1: 4 built-in Pants Go target types (`go_mod`,
  `go_third_party_package`, `go_binary`, `go_package`) locked;
  file-level target types deferred.
- R2: Regex-scoped extractor pattern reused from m225 verbatim
  (per-file build_dsl.rs; refactor to shared module deferred).
- R3: `go_mod` ownership by longest-prefix `source_path` match
  against the go_mod-declaring BUILD file's directory.
- R4: Main-module attribution via
  `waybill:component-role="main-module"` annotation match +
  `main=` / `go_package` dir matching.
- R5: **Broaden C145** (doc-only). Zero new C-rows. Zero new
  extractor entries.

Zero remaining `[NEEDS CLARIFICATION]` markers. Ready for Phase 1.
