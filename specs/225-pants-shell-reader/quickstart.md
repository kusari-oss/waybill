# Quickstart — Scanning a Pants shell backend with waybill

**Feature**: 225-pants-shell-reader
**Audience**: platform teams running waybill against Pants
monorepos that ship shell scripts (deployment scripts, wrappers,
CI helpers, shunit2 tests); compliance stakeholders auditing
shell-tool inventory (`shellcheck` / `shfmt` / `shunit2`) from
`pants.toml`.

---

## Prerequisites

- waybill built with feature 225 landed (`waybill --version` at
  the milestone-225-descended commit).
- A Pants repo with at least one `BUILD` file declaring
  `shell_source` / `shell_sources` / `shunit2_test` /
  `shunit2_tests` targets. Optionally a `pants.toml` pinning
  `[shellcheck]` / `[shfmt]` / `[shunit2]` versions.

---

## 1. Basic scan (BUILD-file walker on)

```bash
waybill sbom scan \
    --path ~/src/my-pants-repo \
    --format cyclonedx-json \
    --output my-repo.cdx.json
```

waybill discovers every `BUILD` file under the scan root, extracts
shell targets via a regex-scoped DSL parser, resolves each target's
`source=` / `sources=[glob...]` expression against the BUILD file's
own directory, and emits one `pkg:generic/*` component per resolved
`.sh` file. Grep for `pkg:generic/` in the output combined with a
target-address annotation filter to verify shell-target coverage:

```bash
jq '.components[] |
    select(.purl | startswith("pkg:generic/") and endswith(".sh") | not | not) |
    select(.properties[]? | select(.name == "waybill:pants-target"))' \
    my-repo.cdx.json | jq '{purl, target: .properties[] | select(.name=="waybill:pants-target") | .value}'
```

## 2. Verify the FR-010 diagnostic

waybill logs one summary line per scan reporting what it found:

```bash
RUST_LOG=info waybill sbom scan --path ~/src/my-pants-repo --format cyclonedx-json --output out.cdx.json 2>&1 | grep 'pants-shell reader complete'
```

Expected output:

```text
INFO waybill::scan_fs::package_db::pants_shell: pants-shell reader complete
  build_files_discovered=12
  build_files_parsed_ok=12
  build_files_skipped_corrupt=0
  shell_targets_found=25
  script_components_emitted=47
  tool_components_emitted=2
```

If `build_files_discovered=0`: your Pants repo has no `BUILD`
files under the scan root. This might be intentional (Pants
supports single-flat-file layouts on small repos, though rare in
practice).

If `build_files_skipped_corrupt >= 1`: waybill found a `BUILD`
file it couldn't parse. Grep for the specific file path in the
WARN stream.

If `script_components_emitted=0` but `shell_targets_found > 0`:
every declared target's `source=` / `sources=[...]` resolved to
zero on-disk files. Common causes: BUILD files referencing scripts
by relative path incorrectly, or `.gitignore` excluding the scripts
from disk (waybill does NOT filter on `.gitignore`, but if the
scripts were never checked in, they're missing).

## 3. Inventory pinned shell tooling

`pants.toml` sections `[shellcheck]` / `[shfmt]` / `[shunit2]`
with `version = "..."` pins get one design-tier `pkg:generic/*`
component each:

```toml
# pants.toml
[shellcheck]
version = "v0.9.0"

[shfmt]
version = "v3.7.0"
```

Verify:

```bash
jq '.components[] |
    select(.purl | test("pkg:generic/(shellcheck|shfmt|shunit2)@"))' \
    my-repo.cdx.json
```

Expected:

```json
{
  "purl": "pkg:generic/shellcheck@v0.9.0",
  "name": "shellcheck",
  "version": "v0.9.0",
  "properties": [
    {"name": "waybill:sbom-tier", "value": "design"},
    {"name": "waybill:source-file", "value": "pants.toml"}
  ]
}
```

**Note on version format**: waybill preserves the operator's exact
version string, including any leading `v` prefix. If your
`pants.toml` says `version = "v0.9.0"`, the emitted PURL is
`pkg:generic/shellcheck@v0.9.0`. If it says `version = "0.9.0"`
without the `v`, the PURL is `pkg:generic/shellcheck@0.9.0`.
waybill does not normalize these — the pin format is operator-owned.

## 4. Multi-target ownership + dedup

If a single script file is owned by MORE THAN ONE target (rare —
happens when a `shell_source` explicit + a `shell_sources` glob
both match), waybill emits ONE component with all owning target
addresses in the `waybill:pants-target` annotation, lexically
sorted, comma-separated:

```bash
jq '.components[] |
    select(.properties[]? |
           select(.name == "waybill:pants-target") |
           .value | contains(","))' \
    my-repo.cdx.json
```

Example emitted value:
`"waybill:pants-target": "scripts:glob,scripts:single"`.

## 5. shunit2 test scope tagging

`shunit2_test` / `shunit2_tests`-owned components tag as
`waybill:lifecycle-scope=development` so downstream security tools
can filter them out of production dependency inventories:

```bash
jq '.components[] |
    select(.properties[]? |
           select(.name == "waybill:lifecycle-scope" and .value == "development")) |
    .purl' my-repo.cdx.json
```

Runtime `shell_source` / `shell_sources`-owned components do NOT
carry a lifecycle-scope property (Runtime is the default and stays
absent — this matches m179's convention).

## 6. What waybill does NOT ingest (v1 scope)

- **`shell_command` targets**: Pants's arbitrary-command wrapper.
  These describe actions, not artifacts. Deferred.
- **Plugin-registered custom shell target types**: only the four
  built-in types are recognized.
- **Nested `pants.toml` files**: only the scan-root `pants.toml`
  is consulted for `[shellcheck]` / `[shfmt]` / `[shunit2]` pins.
- **Pants's embedded shunit2 bundle**: only operator-pinned
  `[shunit2] version = "..."` triggers a `pkg:generic/shunit2@`
  component. If your BUILD files use `shunit2_test(...)` but
  `pants.toml` has no `[shunit2]` section, waybill still emits
  the per-test script components (from BUILD-file walk); it just
  doesn't emit a `pkg:generic/shunit2` entry.

## 7. What this feature does NOT change

- Repos with zero Pants BUILD files AND no `pants.toml`: SBOM
  output is byte-identical to pre-feature-225 goldens per FR-011 /
  SC-003. Grep for `pants-shell reader complete` in the log —
  absent means the reader correctly returned early.
- The m133 file-tier walker (which discovers orphan files with no
  source-tier provenance) is unchanged. Its dedupe index sees
  pants-shell-emitted script paths automatically, so no double-
  emission occurs.
- The `pants` (m223 Python) and `pants_jvm` (m224 JVM) readers are
  unchanged. All three activate independently on the same scan.

## 8. Troubleshooting

**Case A**: `build_files_discovered=0` but I know my repo has
BUILD files.

```bash
find ~/src/my-pants-repo -name 'BUILD' -not -path '*/node_modules/*' | head
```

If the output is empty, your Pants repo may use `BUILD.pants` or
`BUILD.plzconfig` naming (rare — Pants 2.x defaults to `BUILD`
with no extension). File an issue if this trips you; waybill v1
recognizes only the literal `BUILD` filename.

**Case B**: `script_components_emitted=0` but I know
`shell_source` targets exist.

Grep for WARN diagnostics naming the target or file:

```bash
RUST_LOG=warn waybill sbom scan --path ~/src/my-pants-repo ... 2>&1 | grep pants-shell
```

Common causes:
- Target's `source=` value references a variable or uses
  concatenation (`source=DEPLOY_SCRIPT`) — waybill emits WARN +
  skips per FR-009.
- File referenced by `source=` doesn't exist on disk (typo,
  gitignore).
- Target uses a name waybill doesn't recognize (v1: only 4
  built-in types).

**Case C**: `waybill:pants-target` annotation contains an
unexpected extra target address.

If a component's annotation lists TWO addresses when you expected
one, some other `shell_sources(sources=["**/*.sh"])` or
`shunit2_tests(sources=["*_test.sh"])` glob in the repo is
recursively matching the file. SC-006 dedup is working correctly
— waybill is preserving the fact that both targets claim
ownership. If this is unwanted, tighten the glob patterns in the
BUILD file to be non-overlapping.

**Case D**: waybill missed a `.sh` file entirely.

The pants-shell reader only ingests scripts declared by a BUILD-file
target. Standalone `.sh` files not referenced by any target are
picked up (if at all) by the m133 file-tier walker in orphan-file
mode. Not a bug — the reader's design intentionally couples to
Pants target ownership.
