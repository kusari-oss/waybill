# Wire contract: installed library versions

## Component identity

`pkg:generic/<name>@<version>`
- **`<name>`**: the module or package name, lowercased and encoded with `encode_purl_segment`.
- **`<version>`**: the resolved version, encoded the same way. It has no qualifiers.

| Input | PURL |
|---|---|
| `lib/pkgconfig/waybill-fixture-a.pc`, `Version: 1.2.3` | `pkg:generic/waybill-fixture-a@1.2.3` |
| `lib/cmake/WaybillFixtureB/WaybillFixtureBConfigVersion.cmake`, `set(PACKAGE_VERSION "4.5.6")` | `pkg:generic/waybillfixtureb@4.5.6` |
| `lib/x86_64-linux-gnu/pkgconfig/zlib.pc`, `Version: 1.3.1` (not OS-owned) | `pkg:generic/zlib@1.3.1`. This merges with the embedded-version scanner's `zlib@1.3.1`. |

**Tier and source mechanism.** The tier is `deployed`. C55 `waybill:source-mechanism` is `pkg-config` or `cmake-package-version`, both new values in C55's documented set.

## Edges

A `Requires` or `Requires.private` entry that resolves under the data model's rule becomes a `DependsOn` edge, carried as the target's exact PURL. It is the same edge kind as every other package dependency, in all three formats:
- **CycloneDX:** `dependencies[].dependsOn`.
- **SPDX 2.3:** `DEPENDS_ON`.
- **SPDX 3:** a `dependsOn` relationship.

An unresolved entry produces no edge.

## New rows

**C224** and **C225**: confirmed free at implementation (T004). Main ends at C222, and the only open PR touching the catalog, #1561, claims C223.

### C224 `waybill:pkg-config-private-requires`, component scope

**Value.** A JSON array of exact PURL strings, sorted and deduplicated. It holds the targets of this component's edges that came only from `Requires.private`, not also from `Requires`. It is emitted only when non-empty.

**Where it appears.**
- **CycloneDX:** a component `properties[]` entry.
- **SPDX 2.3:** a package annotation in the waybill annotation envelope.
- **SPDX 3:** an `Annotation` on the package element.

**Parity.** `SymmetricEqual` across the three formats.

### C225 `waybill:installed-library-coverage`, document scope

**Value.** A JSON object with the keys of the data model's coverage record, in a fixed key order. It is emitted only when the walker matched at least one `*.pc`, `*ConfigVersion.cmake` or `*-config-version.cmake` file. Every skip reason is present, including zero counts.

**Where it appears.**
- **CycloneDX:** a `metadata.properties[]` entry.
- **SPDX 2.3:** a document annotation.
- **SPDX 3:** an `Annotation` on the `SpdxDocument`.

**Parity.** `SymmetricEqual`.

Example: the US1 fixture with one dpkg-owned file.

```json
{"pkg-config":{"read":3,"emitted":2},"cmake-version":{"read":2,"emitted":1},"skipped":{"computed-version":1,"malformed":0,"no-version":0,"owned-by-os-package":1,"unrecognised-location":0,"unresolved-version":1},"unresolved-requires":0,"version-disagreements":0}
```

## Existing rows touched

- **C55 `waybill:source-mechanism`**: two values are added to the documented set, `pkg-config` and `cmake-package-version`.
- **C18 source files and CycloneDX `evidence.occurrences`**: these now include the matched shared-library paths through `extra_source_paths`. The shape is unchanged.

## Invariants

- **No matching file.** A scan of a tree with no matching file produces output byte-identical to the base commit.
- **OS-owned files.** A file in the OS-only claim set, or in a Homebrew keg, never produces a component or an edge.
- **Repeated scans.** Repeated scans of the same tree are byte-identical.
