# Contract: vcpkg classic mode reader extension (US5)

**Maps to**: FR-007 | **Source-mechanism**: `vcpkg-classic` | **Module extended**: `mikebom-cli/src/scan_fs/package_db/vcpkg.rs`

## Trigger

A file matching `**/vcpkg/installed/<triplet>/vcpkg/info/<name>_<version>_<triplet>.list` appears anywhere under the scan root.

The `<triplet>` is e.g. `x64-linux`, `x64-osx`, `arm64-linux`, `x64-windows`, etc.

## Parsing

The filename is the manifest. Pattern:

```
<name>_<version>_<triplet>.list
^^^^^^ ^^^^^^^^^ ^^^^^^^^^
   |       |        |
   |       |        +-- triplet (vcpkg target triple)
   |       +----------- semver or string version
   +------------------- port name
```

Regex: `^([^_]+)_(.+?)_([^_]+)\.list$` (port name has no underscores; version may contain dots; triplet pattern is well-defined).

The `.list` file contents (paths of files installed by the port) are read for the `mikebom:source-files` annotation but not for component identity.

## PURL derivation

```
pkg:vcpkg/<name>@<version>
```

(Same PURL shape as `vcpkg-manifest` for byte-identity comparison during dedup.)

## Annotations emitted

| Annotation | Value |
|---|---|
| `mikebom:source-mechanism` | `"vcpkg-classic"` |
| `mikebom:source-files` | absolute path of the `.list` file |
| `mikebom:target-arch` | the triplet (e.g. `"x64-linux"`) |

## Dedup precedence (FR-015)

`VcpkgClassic` is in the manifest-mode tier alongside `VcpkgManifest`. Per the
PURL specificity tie-break (Stage 2 of the dedup model), both produce
`pkg:vcpkg/...` so the third-stage lexicographic tie-break runs. Since
`VcpkgClassic` < `VcpkgManifest` lexicographically, **`VcpkgManifest` wins**
when both detect the same library. This matches the spec's US5 scenario 2
("manifest-mode declaration wins").

## Triplet deduplication

When the same port appears under multiple triplets (e.g., `zlib_1.3.1_x64-linux.list` AND `zlib_1.3.1_x64-osx.list`), the dedup pipeline emits one component (same canonical PURL) with `mikebom:target-arch` containing a comma-joined list (per edge case "triplet variants").

## Test cases (US5 acceptance scenarios mapped)

| US5 Scenario | Fixture | Assertion |
|---|---|---|
| 1 (single port) | `golden_inputs/vcpkg_classic/single_triplet/` | `pkg:vcpkg/zlib@1.3.1` with `vcpkg-classic` |
| 2 (classic + manifest collision) | `golden_inputs/dedup_collision/vcpkg_both/` | one component, `vcpkg-manifest` wins, `vcpkg-classic` recorded in `mikebom:also-detected-via` |
