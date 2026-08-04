# Quickstart — Scanning a Pants Go monorepo with waybill

**Feature**: 226-pants-go-reader
**Audience**: platform teams running waybill against Pants Go
monorepos; compliance stakeholders auditing Go dependency
provenance + toolchain versions from `pants.toml`.

---

## Prerequisites

- waybill built with feature 226 landed (`waybill --version`
  at the milestone-226-descended commit).
- A Pants Go repo with at least one `BUILD` file declaring
  `go_mod` / `go_binary` / `go_package` / `go_third_party_package`.
  Optionally `pants.toml` with `[golang] expected_version` set.

---

## 1. Basic scan (enrichment ON by default)

```bash
waybill sbom scan \
    --path ~/src/my-pants-go-repo \
    --format cyclonedx-json \
    --output my-repo.cdx.json
```

waybill runs the existing Go reader as before (emitting
`pkg:golang/*` components from `go.sum` entries), then runs the
pants_go enrichment pass which walks BUILD files, builds an
ownership index, and injects `waybill:pants-target` annotations
on matching components. Query for the annotation to see the
attribution:

```bash
jq '.components[] |
    select(.purl | startswith("pkg:golang/")) |
    {purl, target: (.properties[]? | select(.name == "waybill:pants-target") | .value)} |
    select(.target != null)' \
    my-repo.cdx.json | head -20
```

## 2. Verify the FR-010 diagnostic

waybill logs one summary line per scan reporting what the
enrichment found:

```bash
RUST_LOG=info waybill sbom scan --path ~/src/my-pants-go-repo --format cyclonedx-json --output out.cdx.json 2>&1 | grep 'pants-go enrichment complete'
```

Expected output:

```text
INFO waybill::scan_fs::package_db::pants_go: pants-go enrichment complete
  build_files_discovered=5
  build_files_parsed_ok=5
  build_files_skipped_corrupt=0
  go_targets_found=12
  components_annotated=87
  toolchain_component_emitted=1
```

If `build_files_discovered=0`: no `BUILD` files under scan root
— pants_go is a no-op.

If `go_targets_found=0`: BUILD files exist but none declare Go
targets — pants_go is effectively a no-op.

If `components_annotated=0` but `go_targets_found > 0`: waybill
found Go targets but none matched any `pkg:golang/*` component.
Common causes: the Go reader didn't run (repo has no `go.sum`),
or the target's `import_path=` names a dep not in `go.sum`
(check for INFO diagnostics naming the missing import paths).

## 3. Inventory the pinned Go toolchain

`pants.toml`:

```toml
[golang]
expected_version = "1.21"
```

Verify emission:

```bash
jq '.components[] | select(.purl == "pkg:generic/go@1.21")' my-repo.cdx.json
```

Expected:

```json
{
  "purl": "pkg:generic/go@1.21",
  "name": "go",
  "version": "1.21",
  "properties": [
    {"name": "waybill:sbom-tier", "value": "design"},
    {"name": "waybill:source-file", "value": "pants.toml"}
  ]
}
```

**Note on version format**: waybill preserves the operator's
exact `expected_version` string verbatim. If it's `"1.21"`, the
PURL is `pkg:generic/go@1.21`. If `"1.21.5"` or `"go1.21"`,
those flow through unchanged. waybill does NOT normalize.

## 4. Multi-owner attribution + dedup

If a Go module is owned by BOTH an implicit `go_mod` root AND
an explicit `go_third_party_package`, waybill emits ONE
component with all owning target addresses in the
`waybill:pants-target` annotation, lexically sorted,
comma-separated:

```bash
jq '.components[] |
    select(.purl | startswith("pkg:golang/")) |
    select(.properties[]? | select(.name == "waybill:pants-target") | .value | contains(","))' \
    my-repo.cdx.json
```

Example emitted value:
`"waybill:pants-target": "3rdparty/go:cobra,3rdparty/go:mod"`.

## 5. First-party vs third-party discrimination

Third-party components (from `go.sum`) carry addresses starting
with `3rdparty/` (or wherever your `go_mod` target lives).
First-party components (the main module, per
`waybill:component-role=main-module`) carry addresses from
`go_binary` / `go_package` targets:

```bash
# Third-party inventory:
jq '.components[] |
    select(.purl | startswith("pkg:golang/")) |
    select(.properties[]? | select(.name == "waybill:pants-target") | .value | startswith("3rdparty/"))' \
    my-repo.cdx.json | head

# First-party (main-module) inventory:
jq '.components[] |
    select(.purl | startswith("pkg:golang/")) |
    select(.properties[]? | select(.name == "waybill:component-role") | .value == "main-module") |
    {purl, target: (.properties[]? | select(.name == "waybill:pants-target") | .value)}' \
    my-repo.cdx.json
```

## 6. What waybill does NOT do (v1 scope)

- **Emit `pkg:golang/*` components for `go_third_party_package(import_path=X)`
  when X is missing from `go.sum`** — waybill declines to
  fabricate coordinates without ground-truth source (FR-012 /
  Principle IX). The missing import path is named in an INFO
  log.
- **Handle `go_source` / `go_test` file-level targets** — Pants
  prefers `go_package`; deferred.
- **Emit `min_dot_version` from `pants.toml` `[golang]`** —
  distinct semantic (version-guard lower bound vs pinned
  toolchain); only `expected_version` is emitted.
- **Nested `pants.toml` files under scan root** — only the
  scan-root `pants.toml` is consulted.
- **Plugin-registered custom Go target types** — only the 4
  built-in types are recognized.

## 7. What this feature does NOT change

- Repos with zero Pants BUILD files declaring Go targets AND
  no `pants.toml` `[golang]` section: SBOM output is
  byte-identical to pre-feature-226 goldens per FR-011 /
  SC-003. Grep for `pants-go enrichment complete` in the log —
  absent means the enrichment correctly returned early.
- The existing Go reader (m053+m055+m160+m161) is unchanged.
  Every `pkg:golang/*` component that would have been emitted
  before m226 is still emitted, with the same PURL, sha1
  hashes, and dep-graph edges. m226 only ADDS annotations.
- The `pants` (m223 Python), `pants_jvm` (m224 JVM), and
  `pants_shell` (m225 shell) readers are unchanged. All four
  Pants-family readers may activate independently on the same
  scan.
- The m191 reconciler is unchanged.

## 8. Troubleshooting

**Case A**: `build_files_discovered=0` but I know my repo has
BUILD files.

```bash
find ~/src/my-pants-go-repo -name 'BUILD' -not -path '*/node_modules/*' | head
```

If output is empty, the repo may use `BUILD.pants` naming (rare
— Pants 2.x defaults to `BUILD` with no extension). File an
issue if this trips you; waybill v1 recognizes only the literal
`BUILD` filename.

**Case B**: `components_annotated=0` but I see `pkg:golang/*`
components in the SBOM AND my BUILD files declare Go targets.

Two likely causes:
1. The `go_mod`-declaring BUILD file's directory doesn't match
   the go.sum's `source_path`. Verify: `jq '.components[] |
   select(.purl | startswith("pkg:golang/")) | .properties[]?
   | select(.name == "waybill:source-files") | .value'` — the
   paths should include `go.sum` under the same directory tree
   as your `go_mod`-declaring BUILD file.
2. Regex extractor failed to parse the `go_mod` declaration.
   Grep for `pants-go reader.*parse error` in the WARN log
   stream.

**Case C**: `waybill:pants-target` annotation contains an
unexpected extra address.

If a component's annotation lists TWO addresses when you
expected one, some other Pants target (a `go_package` in a
parent directory tree, or a redundant `go_third_party_package`)
is also claiming ownership. This is CORRECT behavior per SC-004
— multiple ownership is preserved via the merged annotation.
If unwanted, tighten your BUILD file declarations to be
non-overlapping.

**Case D**: A `pkg:golang/*` component I expected has no
`waybill:pants-target` annotation.

waybill only annotates components that match a declared Pants
target. If your repo has go.sum entries not covered by any
`go_mod` root (rare — usually indicates a broken Pants
configuration), those components remain un-annotated. Verify by
tracing which `go_mod` BUILD files should cover the missing
component's `source_path`.

**Case E**: waybill flagged an INFO diagnostic
`"import_path X named by go_third_party_package has no matching
go.sum entry"`.

Your BUILD file references an `import_path=X` that isn't in
any `go.sum`. Either regenerate the lockfile
(`pants generate-lockfiles`) or remove the stale
`go_third_party_package` declaration.
