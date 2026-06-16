# Contract: `mikebom:kmp-source-set` annotation (C68)

**Feature**: 122-kotlin-swift-readers
**Date**: 2026-06-15
**Consumed by**: consumers reading emitted SBOMs; the milestone-115 parity-catalog framework; downstream KMP-target filtering tools
**Spec mapping**: FR-006, FR-008; Constitution Principle V audit (research Decision 8)

This contract defines the value shape + emission gating + Principle V audit conclusion for the ONE new `mikebom:*` annotation key this feature introduces. The annotation integrates via the existing `extra_annotations: BTreeMap<String, serde_json::Value>` channel on `PackageDbEntry` and `ResolvedComponent`; the existing emitter at `generate/cyclonedx/builder.rs:965-973` (per-component properties) automatically renders JSON-encoded values as `properties[]` entries.

## Annotation shape

**Key**: `mikebom:kmp-source-set`
**Scope**: per-component
**Storage shape** (in-process): `serde_json::Value::Array(Vec<String>)` under the same key in `extra_annotations`. Mirrors the C64 / C67 BTreeMap-array convention from milestones 116 + 119.

**CDX 1.6 carrier**: `components[].properties[]` entry with `name = "mikebom:kmp-source-set"`, `value = "<JSON-encoded array string>"`.
**SPDX 2.3 carrier**: `Package.annotations[]` entry wrapped in `MikebomAnnotationCommentV1` envelope, `field = "mikebom:kmp-source-set"`, `value = <JSON array>`.
**SPDX 3.0.1 carrier**: `Annotation` graph element targeting the component, same envelope shape as SPDX 2.3.

## Value shape (JSON array of strings)

```json
["commonMain", "iosMain", "jvmMain"]
```

Where each string is a valid Kotlin Multiplatform source-set name. The array is:

1. **Lex-sorted** (`BTreeSet` invariant — determinism for cross-scan byte-identity).
2. **Deduped** (`BTreeSet` invariant — each source-set name appears at most once).
3. **Non-empty** (the emission gate below guarantees this — empty arrays are never emitted).

### Known source-set names

The canonical Kotlin Multiplatform source-set vocabulary includes:

- `commonMain`, `commonTest` — cross-target shared code
- `jvmMain`, `jvmTest` — JVM target
- `androidMain`, `androidTest`, `androidDebug`, `androidRelease` — Android target
- `iosMain`, `iosTest`, `iosX64Main`, `iosArm64Main`, `iosSimulatorArm64Main` — iOS variants
- `macosMain`, `macosX64Main`, `macosArm64Main` — macOS variants
- `watchosMain`, `tvosMain` — watchOS/tvOS
- `linuxX64Main`, `linuxArm64Main` — Linux native
- `mingwX64Main` — Windows native
- `jsMain`, `wasmJsMain` — JS / WASM
- `nativeMain` — generic native (intermediate source-set)

The reader does NOT validate names against this list — operators may declare custom source-sets via Kotlin DSL's `sourceSets.create("myCustomSet") { ... }`. The annotation reflects whatever source-set name the operator declared.

## CDX wire-shape worked example

A KMP `shared/build.gradle.kts` declaring `kotlinx-serialization` in BOTH `commonMain` and `jvmMain`:

```kotlin
kotlin {
    sourceSets {
        commonMain { dependencies { implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2") } }
        jvmMain { dependencies { implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2") } }
    }
}
```

emits ONE component (canonical PURL is the same), carrying:

```json
{
  "purl": "pkg:maven/org.jetbrains.kotlinx/kotlinx-serialization-json@1.6.2",
  "properties": [
    { "name": "mikebom:source-files", "value": "<path>/shared/build.gradle.kts" },
    { "name": "mikebom:kmp-source-set", "value": "[\"commonMain\",\"jvmMain\"]" }
  ]
}
```

Consumers calling `JSON.parse(prop.value)` reconstitute the source-set list and can filter by `.includes("commonMain")` etc.

## SPDX 2.3 wire-shape worked example

Same component:

```json
{
  "SPDXID": "SPDXRef-Package-...",
  "name": "kotlinx-serialization-json",
  "versionInfo": "1.6.2",
  "annotations": [
    {
      "annotator": "Tool: mikebom-<version>",
      "annotationDate": "2026-06-15T...",
      "annotationType": "OTHER",
      "comment": "{\"v\":1,\"field\":\"mikebom:kmp-source-set\",\"value\":[\"commonMain\",\"jvmMain\"]}"
    }
  ]
}
```

The `MikebomAnnotationCommentV1` envelope wraps the field + value; the JSON-array value is preserved verbatim.

## SPDX 3.0.1 wire-shape worked example

Same component (`@graph` excerpt):

```json
{
  "type": "Annotation",
  "spdxId": "<doc-iri>/anno-<hash>",
  "creationInfo": "_:creationInfo",
  "subject": "<doc-iri>/pkg-<hash>",
  "annotationType": "other",
  "statement": "{\"v\":1,\"field\":\"mikebom:kmp-source-set\",\"value\":[\"commonMain\",\"jvmMain\"]}"
}
```

Same envelope shape as SPDX 2.3.

## Emission gating

- **Present** iff the component was discovered from a `build.gradle.kts` `kotlin { sourceSets { <name> { dependencies { ... } } } }` block AND at least one source-set declared the dep.
- **Absent** when:
  - The component was discovered from a top-level `dependencies { ... }` block (Android default; no KMP).
  - The component was discovered via the existing milestone-106 `gradle.lockfile` reader (no source-set provenance).
  - The component was discovered via any non-Kotlin reader (cargo, npm, pip, Swift, etc.).
- **Cardinality**: at most ONE `mikebom:kmp-source-set` property per component. Multiple source-sets declaring the same canonical PURL accumulate into the array per the `KmpSourceSetTracker` aggregation in data-model.md.

## Determinism guarantees

- The array is lex-sorted via `BTreeSet`. Two scans of the same project tree produce byte-identical `mikebom:kmp-source-set` values (modulo the existing per-scan random `serialNumber` + timestamp fields).
- The PURL-keyed deduplication ensures one component → one source-set array, regardless of how many `kotlin { sourceSets { ... } }` blocks declared the dep.
- The JSON-encoded string preserves Unicode + escape semantics via `serde_json::to_string` (the existing emitter's serialization path).

## C68 parity-catalog row

**Add to `docs/reference/sbom-format-mapping.md`** (new row after C67):

> | C68 | `mikebom:kmp-source-set` | per-component `properties[]` entry — JSON-encoded string carrying an array of Kotlin Multiplatform source-set names (lex-sorted, deduped). Emitted ONLY on components discovered from a `kotlin { sourceSets { ... } }` block in `build.gradle.kts`. Absent on non-KMP components. | Annotation `mikebom:kmp-source-set` on Package, `MikebomAnnotationCommentV1` envelope, value = JSON array. | Annotation `mikebom:kmp-source-set` on Package (graph-element Annotation targeting the component, same envelope shape as SPDX 2.3). | **Native-field audit per Constitution Principle V (v1.4.0)**: NO native field expressing "this dependency was declared in a specific Kotlin Multiplatform source-set" exists in CDX 1.6, SPDX 2.3, or SPDX 3.0.1. CDX 1.6's `evidence.identity[].methods[]` carries identification methods, not source-set provenance. SPDX 2.3's `Package.primaryPackagePurpose` is a category taxonomy, not a build-target provenance marker. SPDX 3.0.1's evidence-profile model would express this via a future `kotlinMultiplatformSourceSet` profile that doesn't exist in 3.0.1 stable. **Per Constitution Principle X (Transparency)**: consumers filtering an SBOM to a single target (Android-only, iOS-only) MUST know which source-set declared each dep; the annotation provides this signal in machine-parseable form. Pattern parallels C64 `mikebom:produces-binaries` (milestone 116) and C67 `mikebom:assertion-conflict` (milestone 119) in storage shape (JSON-encoded array as `properties[]` value). Driven by milestone 122 (FR-006). |

## Extractor registrations

The three extractors (`c68_cdx`, `c68_spdx23`, `c68_spdx3`) register in `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` via the existing macros:

```rust
// cdx.rs (after C67):
cdx_anno!(c68_cdx, "mikebom:kmp-source-set", component);

// spdx2.rs (after C67):
spdx23_anno!(c68_spdx23, "mikebom:kmp-source-set", component);

// spdx3.rs (after C67):
spdx3_anno!(c68_spdx3, "mikebom:kmp-source-set", component);
```

The parity-catalog table row registers in `mod.rs` (after C67's row):

```rust
ParityExtractor {
    row_id: "C68",
    label: "mikebom:kmp-source-set",
    cdx: c68_cdx,
    spdx23: c68_spdx23,
    spdx3: c68_spdx3,
    directional: Directionality::SymmetricEqual,
    order_sensitive: false,
},
```

## Consumer compatibility

- **Existing C5 `mikebom:source-tier` extractor** is unaffected — supplement-introduced components (milestone 119) still carry `declared`; KMP-declared components carry the SCANNER tier (`source` or `analyzed`) and the new `kmp-source-set` is ADDITIVE annotation.
- **Existing C40 `mikebom:component-role` extractor** is unaffected — KMP-declared components carry no role annotation by default; workspace-root + main-module roles emit independently per FR-007 / the milestone-106 convention.
- **Existing milestone-115 catalog-coverage gate** continues to pass — C68 has the same three extractors as every other C-row.
- **Existing milestone-119 supplement merge** sees `mikebom:kmp-source-set` as a normal annotation. Supplement-declared values (if an operator uses `--supplement-cdx` to override the source-set list) wins per the developer-metadata-override partition (or stays scanner-side as bytes-derived if classified there — operator's choice via the supplement file shape).
