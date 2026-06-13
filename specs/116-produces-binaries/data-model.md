# Data Model — Automatic binary-name binding via produces-binaries

**Feature**: 116-produces-binaries
**Date**: 2026-06-13

This feature introduces ONE new property shape on the source-tier SBOM AND extends ONE existing envelope struct (`SourceDocumentBinding` from milestone 111). There are no persisted entities (matches every milestone since 002) — all state is in-process per scan, then serialized into the emitted SBOM. The binder's runtime index (`binary_name_to_purl`) is built once per `--bind-to-source` invocation and discarded at scan end.

## Entity 1 — `mikebom:produces-binaries` property (source-tier SBOM)

**Location**: A property on a source-tier SBOM component. The property name is `mikebom:produces-binaries`; the property value is a JSON-encoded string of an array of strings.

**Affected components**: ONLY main-module components (per spec clarification Q3). Transitive-dependency components MUST NOT carry the property.

**Affected ecosystems**: Cargo, npm, pip, gem, maven (US1+US2); Golang (US3 deferral). Other ecosystems mikebom supports (rpm, dpkg, alpine, vcpkg, conan, nuget, swift, west, idf_component, opkg, yocto-bb, …) do NOT carry the property — they're not "main module" ecosystems in the milestone-064 sense; their components represent installed/cached packages, not the operator's project.

### Property name

```text
mikebom:produces-binaries
```

| Format | Encoded as |
|---|---|
| CDX 1.6 | `properties[]` entry with `name = "mikebom:produces-binaries"`, `value = <stringified JSON array>` |
| SPDX 2.3 | `annotations[]` entry wrapped in the existing `MikebomAnnotationCommentV1` envelope, key `mikebom:produces-binaries`, value = JSON array |
| SPDX 3.0.1 | `Annotation` entry wrapped in the existing `MikebomAnnotationCommentV1` envelope, key/value identical to SPDX 2.3 |

The CDX `value` field is a string by CDX 1.6 spec; we encode the array as a JSON string. The SPDX paths wrap the array in the same `MikebomAnnotationCommentV1` envelope already used by milestones 071/072/111.

### Property value shape

```json
["bar", "baz", "baz-cli"]
```

| Field | Type | Source | Notes |
|---|---|---|---|
| (array) | JSON array of strings | per-ecosystem extractor | Sorted lex, deduped, lowercase ASCII, platform-suffix-stripped. Always non-empty when the property is present (absence = no binaries; emit no property at all per FR-001). |

### Per-ecosystem extraction sources

| Ecosystem | Source(s) | Reference |
|---|---|---|
| Cargo | `Cargo.toml` `[[bin]]` entries (explicit) + `src/main.rs` (default-binary, name = package name) + `src/bin/*.rs` (implicit, name = file stem) | Cargo book § "Configuring a target" |
| npm | `package.json` `bin` field (string form: name = package's `name` field; object form: name = each key of the object) | npm docs § "package.json — bin" |
| pip | `pyproject.toml` `[project.scripts]` keys + `[project.gui-scripts]` keys. Fallback: `setup.cfg` `[options.entry_points] console_scripts` keys + `gui_scripts` keys. | PEP 621 + setuptools docs |
| gem | gemspec `executables = [...]` array entries | RubyGems Specification reference |
| maven | POM shade-plugin `<finalName>` + jar-plugin `<finalName>` (strip `.jar` extension) | Maven Shade Plugin + Maven JAR Plugin docs |
| Go (US3) | Every directory containing a `*.go` file with `package main` declaration; binary name = directory basename | `go help build` |

### Invariants

1. **Lowercase ASCII**: every entry is `entry == entry.to_lowercase()` after the extractor stamps the value. Non-ASCII or mixed-case entries are normalized at extraction time.
2. **Extensionless**: no entry ends in `.exe`, `.jar`, `.dll`, `.so`, `.dylib`. Suffix translation is the binder's job (per Decision 7).
3. **Sorted + deduped**: `entries == LC_ALL=C sort -u(entries)`.
4. **Non-empty**: the property is OMITTED entirely when no entries are extractable (FR-001). An empty array is not a valid value.
5. **Union-merge with operator pre-seeded values** (FR-012): if the component already has a `mikebom:produces-binaries` property (e.g., from a hand-edited SBOM consumed as scan-input), the new value is `sort(dedupe(existing ∪ discovered))`. Operator values are preserved.
6. **Main-module only**: transitive-dep components never carry this property.

## Entity 2 — `SourceDocumentBinding.alias_source` (extension of milestone-111 envelope)

**Location**: A new optional field on the existing `SourceDocumentBinding` struct at `mikebom-cli/src/binding/mod.rs:187-217`. The field is serialized into the binding envelope that lives on image-tier components in the emitted SBOM.

### Field definition

```rust
// Inside SourceDocumentBinding:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub alias_source: Option<AliasSource>,
```

### Enum definition (new)

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AliasSource {
    OperatorSupplied,
    AutomaticFromProducesBinaries,
}
```

Serialized as `"operator-supplied"` or `"automatic-from-produces-binaries"`.

### Invariants

1. **Paired presence**: `alias_source` is `Some(_)` iff `alias_from` is `Some(_)` (which milestone 111 invariant-pairs with `alias_to`). The three fields are present together or absent together.
2. **Backwards compatibility**: pre-feature SBOMs deserialize cleanly via `#[serde(default)]`. The absence of `alias_source` on a binding that DOES have `alias_from`/`alias_to` SHOULD be interpreted as implicitly `OperatorSupplied` (only possible source pre-feature); however, the code never WRITES this implicit case — every NEW alias-bearing envelope post-feature MUST set `alias_source` explicitly.
3. **Operator precedence** (FR-004 + spec clarification Q3): when both an operator alias and an automatic alias would resolve to the same source-side PURL, the binder sets `alias_source = OperatorSupplied`. The automatic alias is suppressed (not also applied alongside).

## Entity 3 — `SourceSbomContext.binary_name_to_purl` (in-process binder index)

**Location**: A new field on the existing `SourceSbomContext` struct at `mikebom-cli/src/binding/verify.rs:460-474`. Lives only for the duration of a single `--bind-to-source` invocation; never serialized; never persisted.

### Field definition

```rust
// Inside SourceSbomContext:
/// Maps lowercase extensionless binary names → source-tier PURL(s) declared
/// via `mikebom:produces-binaries` on the source SBOM's components. Multi-
/// valued because FR-013's name-collision case MUST be reported as `weak`
/// with `multiple-source-candidates-for-binary-name` reason; the binder
/// needs all candidates to populate the audit trail.
binary_name_to_purl: HashMap<String, Vec<Purl>>,
```

### Population (`SourceSbomContext::load()` at verify.rs:478)

1. Parse the source SBOM (existing milestone-072 logic).
2. For each component, look for a `mikebom:produces-binaries` property.
3. If present: parse the value as a JSON array of strings; for each entry, insert into `binary_name_to_purl` mapping `entry → [component.purl]` (push, don't replace — multiple components may share a name).
4. If absent: skip (the property is opt-in per FR-001).

### Lookup (`SourceSbomContext::binding_for_purl()` at verify.rs:520)

1. Existing exact-PURL match runs first (unchanged behavior).
2. If the exact match returns a hit, return that result (no auto-alias logic runs — preserves backwards compat).
3. If the exact match returns `Unknown { source-not-found-in-bind-target }` AND the incoming PURL is shaped `pkg:generic/<name>`:
   - Compute the normalized lookup name: `name.to_lowercase()`, strip trailing `.exe` or `.jar`.
   - Look up `binary_name_to_purl[lookup_name]`.
   - If absent → return the original `Unknown` (no auto-alias possible).
   - If present with one candidate → return a binding result aliasing to that PURL, with `alias_source = AutomaticFromProducesBinaries`.
   - If present with multiple candidates → return `Weak { reason: "multiple-source-candidates-for-binary-name", alias_to: <first candidate>, alias_source: AutomaticFromProducesBinaries }` AND the audit trail (a future enrichment of the envelope OR a separate `alias_candidates` field — TBD in PR-A's review cycle) carries all candidates.

### Invariants

1. **One-time build cost**: populated once during `load()`, O(source-tier-component-count). Never re-built within a scan.
2. **Read-only after load**: never mutated during `binding_for_purl()` lookups.
3. **Survives feature absence**: when no source-tier component carries the property, the map is empty; the lookup branch in `binding_for_purl()` short-circuits to `None` on first lookup; SC-005 backwards-compat is preserved.

## Lifecycle

```text
┌─────────────────────────────────────────────────────────────────────┐
│ Source-tier scan (mikebom sbom scan --path .)                       │
│                                                                     │
│   1. Per-ecosystem main-module extractor reads manifest             │
│   2. Extracts binary names (Cargo Toml [[bin]], npm bin, ...)       │
│   3. Normalizes: lowercase, strip .exe/.jar, sort, dedupe           │
│   4. Union-merges with any pre-existing extra_annotations entry     │
│   5. Stamps mikebom:produces-binaries on main-module component's    │
│      extra_annotations BTreeMap                                     │
│   6. CDX/SPDX serializer renders it into the emitted SBOM           │
└─────────────────────────────────────────────────────────────────────┘
                          │
                          ▼ source-tier SBOM file
┌─────────────────────────────────────────────────────────────────────┐
│ Cross-tier scan (mikebom sbom scan --image <ref>                    │
│                  --bind-to-source <source-sbom-path>)               │
│                                                                     │
│   1. SourceSbomContext::load() reads source SBOM                    │
│      - Existing milestone-072 logic: component PURL set             │
│      - NEW: scan for mikebom:produces-binaries properties           │
│      - NEW: populate binary_name_to_purl index                      │
│   2. For each image-tier component, binding_for_purl() runs:        │
│      a. Existing exact-PURL match (unchanged)                       │
│      b. NEW: if Unknown AND PURL is pkg:generic/<name>:             │
│           - normalize name (case, suffix)                           │
│           - look up in binary_name_to_purl                          │
│           - on hit: produce binding with alias_source field         │
│   3. Operator --pkg-alias takes precedence over auto-alias (FR-004) │
│      - milestone-111 logic already runs first; auto-alias only      │
│        applies when operator alias didn't fire                      │
│   4. Binding envelope (with alias_source if applicable) attaches    │
│      to image-tier component via existing milestone-111 plumbing    │
└─────────────────────────────────────────────────────────────────────┘
                          │
                          ▼ image-tier SBOM file (verify-binding consumes)
```

## Validation rules summary

| Rule | Source | Where enforced |
|---|---|---|
| Lowercase + extensionless source-side names | Decision 2 / FR-001 | Per-ecosystem extractors before stamping |
| Sorted + deduped property value | Decision 2 / FR-012 | Per-ecosystem extractors via shared `normalize_produces_binaries()` helper |
| Property emitted only on main-module | Spec clarification Q3 / FR-001 | Per-ecosystem extractors guard with `is_main_module` check |
| `alias_source` paired with `alias_from`/`alias_to` | Decision 4 | `SourceDocumentBinding` constructor + serde invariant tests |
| `OperatorSupplied` precedence over `AutomaticFromProducesBinaries` | FR-004 | `attach_bindings_to_components()` at scan_cmd.rs:2317 ordering |
| Library-only crates → no property | FR-005 / FR-008 | Per-ecosystem extractors return empty list → property omitted |
| Backwards-compat: missing property → exact-PURL match unchanged | FR-014 / SC-005 | `binary_name_to_purl` empty → lookup short-circuits |
| Multi-candidate collision → `weak` with `multiple-source-candidates-for-binary-name` | FR-013 | `binding_for_purl()` auto-alias branch |

No state transitions (no lifecycle FSM); the index is build-once-read-many within a scan. The per-ecosystem extractor and the cross-tier binder run in different mikebom invocations.
