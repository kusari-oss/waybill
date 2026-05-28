# Quickstart: C/C++ Ecosystem Expansion (milestone 105)

Once milestone 105 ships, mikebom can audit six previously-invisible C/C++
project shapes. This page shows the minimum-viable invocation for each.

## CPM.cmake-based projects

```bash
mikebom sbom scan --path ./my-cpm-project --format cyclonedx-json --output sbom.cdx.json
```

Expected output: one component per `cpmaddpackage(...)` call site. Sample:

```json
{
  "bom-ref": "pkg:github/fmtlib/fmt@12.1.0",
  "purl": "pkg:github/fmtlib/fmt@12.1.0",
  "version": "12.1.0",
  "name": "fmt",
  "properties": [
    {"name": "mikebom:source-mechanism", "value": "cpm-cmake"},
    {"name": "mikebom:source-files", "value": "/abs/path/Dependencies.cmake"}
  ]
}
```

## `conanfile.py`-based projects

```bash
mikebom sbom scan --path ./my-conan-project --format cyclonedx-json --output sbom.cdx.json
```

Both `conanfile.txt` and `conanfile.py` are read. Components tagged with
`mikebom:source-mechanism: "conan-recipe"`; build/tool requirements get
`mikebom:lifecycle-scope: "build"`; runtime requirements get
`mikebom:lifecycle-scope: "runtime"`.

## Zephyr applications

```bash
mikebom sbom scan --path ./my-zephyr-app --format cyclonedx-json --output sbom.cdx.json
```

Optionally exclude project groups (e.g., test simulators):

```bash
mikebom sbom scan --path ./my-zephyr-app \
  --exclude-group babblesim --exclude-group optional \
  --format cyclonedx-json --output sbom.cdx.json
```

Each west-managed project emerges as a component with the manifest-pinned
revision. The Zephyr v4.4.0 main repo (a 577 MB tree) produces ≥79 C/C++
components — compared to zero before milestone 105.

## esp-idf applications

```bash
mikebom sbom scan --path ./my-esp-idf-app --format cyclonedx-json --output sbom.cdx.json
```

All `idf_component.yml` files under the tree are unioned. Each unique
registry component produces `pkg:idf/<namespace>/<name>@<version>` with a
fallback `mikebom:download-url` to the source repo for consumers that don't
yet recognize the `pkg:idf/` ecosystem.

## vcpkg classic mode

```bash
mikebom sbom scan --path ./my-project-with-vcpkg-installed-tree \
  --format cyclonedx-json --output sbom.cdx.json
```

The reader walks `vcpkg/installed/<triplet>/vcpkg/info/*.list` and emits one
component per installed port. When a project also has `vcpkg.json` (manifest
mode), the manifest declaration wins and the classic install record is
recorded in `mikebom:also-detected-via`.

## Large C++ projects with git submodules (gRPC pattern)

```bash
# Make sure submodules are populated first.
cd ./my-project && git submodule update --init --recursive
mikebom sbom scan --path . --format cyclonedx-json --output sbom.cdx.json
```

Each submodule emerges as a component pinned to its checked-out HEAD commit.
Uninitialized submodules emit with `version: "unknown"` and a
`mikebom:resolver-step` annotation explaining why.

Each submodule also carries a `mikebom:build-reference` annotation:

- `"declared-and-used"` — at least one `find_package(<name> ...)` call in the
  project's `CMakeLists.txt` references the submodule's path basename.
- `"declared-only"` — the submodule is present but not referenced by any
  scanned `find_package` call.

Downstream vuln-scanner integrations can filter out un-referenced submodules:

```bash
jq '.components[] | select(.properties[]? | (.name == "mikebom:build-reference" and .value == "declared-and-used"))' sbom.cdx.json
```

## Cross-reader corroboration (`mikebom:also-detected-via`)

When the same library is independently identified by two or more readers
(e.g., gRPC's `abseil-cpp` matched by both `git-submodule` and `conan-recipe`),
mikebom emits **one** component (the winner per the precedence table in
`data-model.md`) with a `mikebom:also-detected-via` annotation listing the
losing source-mechanisms.

In CDX 1.6, the same signal also appears natively in
`evidence.identity[0].methods[]` — each detection record has its own method
entry with a `mikebom-source-mechanism` sub-field. The first method is the
winner; subsequent methods are the losers.

## Credential safety (FR-016)

If your `.gitmodules` or `west.yml` happens to contain credentials
(`https://user:token@github.com/...` or `ssh://deploy-key@host:...`), mikebom
strips them before emission and emits a `tracing::warn!` event:

```
WARN mikebom::identifiers::sanitize: stripped credentials from URL
  manifest_file=/abs/path/.gitmodules
  url_redacted=https://***@github.com/org/private-repo.git
```

The redacted URL appears in the SBOM and the warning is operator-actionable.
There is no opt-out — credentials NEVER appear in mikebom output.

## Verification commands

After running a scan, you can spot-check the new annotations:

```bash
# What source-mechanisms fired?
jq '.components[] | .properties // [] | .[] | select(.name == "mikebom:source-mechanism") | .value' sbom.cdx.json | sort | uniq -c

# Which submodules were declared but not referenced?
jq '.components[] | select(.properties[]? | (.name == "mikebom:build-reference" and .value == "declared-only")) | .purl' sbom.cdx.json

# Which components were detected by multiple readers?
jq '.components[] | select(.properties[]? | .name == "mikebom:also-detected-via") | {purl, also: (.properties[] | select(.name == "mikebom:also-detected-via") | .value)}' sbom.cdx.json
```
