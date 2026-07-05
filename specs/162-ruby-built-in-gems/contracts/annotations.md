# Annotation Wire Contracts: Milestone 162

**Date**: 2026-07-04
**Feature**: [spec.md](../spec.md) | **Plan**: [plan.md](../plan.md) | **Data Model**: [data-model.md](../data-model.md)

Per-format wire shapes for the 2 new per-component annotations (C113 + C114) attached to synthetic Ruby built-in gem components. Both use string values (single-source case) or JSON-array values (multi-source case for C114 only), matching the milestone-159 (C106/C107) multi-alias precedent.

## C113 — `mikebom:synthetic-built-in` (per-component)

### CycloneDX 1.6

```json
{
  "type": "library",
  "name": "bundler",
  "purl": "pkg:gem/bundler",
  "properties": [
    {"name": "mikebom:synthetic-built-in", "value": "ruby"}
  ]
}
```

Note the versionless PURL (`pkg:gem/bundler` — no `@<version>` segment) per FR-003 + Q1.

### SPDX 2.3

```json
{
  "SPDXID": "SPDXRef-Package-bundler",
  "name": "bundler",
  "annotations": [
    {
      "annotationDate": "2026-07-04T00:00:00Z",
      "annotationType": "OTHER",
      "annotator": "Tool: mikebom",
      "comment": "mikebom:synthetic-built-in=ruby"
    }
  ]
}
```

Note: `versionInfo` field is either omitted OR set to `NOASSERTION` (SPDX-native equivalent of "version unknown"). Consumer's SPDX parser must handle either shape.

### SPDX 3.0.1

```json
{
  "type": "Annotation",
  "spdxId": "...LicenseRef-mikebom-synthetic-built-in-<sha>",
  "creationInfo": "_:CreationInfo-mikebom-scan",
  "annotationType": "other",
  "statement": "mikebom:synthetic-built-in=ruby",
  "subject": "spdx:Package/bundler"
}
```

**Value vocabulary** (closed 1-value in scope for milestone 162):

| Value | Meaning |
|-------|---------|
| `ruby` | Component is a synthetic entry for a Ruby toolchain-provided built-in gem (per Q2 union of Ruby 3.2/3.3/3.4). |

**Emission conditions**: MUST appear iff the component is a synthetic Ruby built-in gem emitted by `append_synthetic_built_in_gems()` per data-model.md E4. MUST NOT appear on non-synthetic gem components (real GEM/specs entries).

## C114 — `mikebom:built-in-requirement` (per-component, conditional)

### CycloneDX 1.6 (single source)

```json
{
  "properties": [
    {"name": "mikebom:synthetic-built-in", "value": "ruby"},
    {"name": "mikebom:built-in-requirement", "value": ">= 1.2.0"}
  ]
}
```

### CycloneDX 1.6 (multiple sources with different requirements)

```json
{
  "properties": [
    {"name": "mikebom:synthetic-built-in", "value": "ruby"},
    {"name": "mikebom:built-in-requirement", "value": "[\">= 1.2.0\", \">= 2.0.0\"]"}
  ]
}
```

Value is a JSON-string-encoded array (matches milestone-159 C107 multi-alias shape). Consumers doing constraint-parsing pipe the value through `jq fromjson` to get a real array.

### SPDX 2.3

```json
{
  "annotations": [
    {"comment": "mikebom:built-in-requirement=>= 1.2.0", ...}
  ]
}
```

Multi-source case: `comment = "mikebom:built-in-requirement=[\">= 1.2.0\", \">= 2.0.0\"]"`.

### SPDX 3.0.1

```json
{
  "type": "Annotation",
  "statement": "mikebom:built-in-requirement=>= 1.2.0",
  "subject": "spdx:Package/bundler",
  ...
}
```

**Value grammar**:

```text
value          ::= single_req | multi_req
single_req     ::= <requirement-string-as-declared-in-Gemfile.lock>
multi_req      ::= JSON-string-encoded array of sorted single_req values
requirement    ::= <as observed in Gemfile.lock's indent-6 dep line;
                    e.g., ">= 1.2.0", "~> 1.0", "= 2.0.0", "!= 1.5">
```

**Emission conditions**: MUST appear iff (a) the component is a synthetic Ruby built-in gem AND (b) at least one source spec's dep-declaration had a non-empty requirement clause. When no source specified a requirement (rare — bare `bundler` in the deps), the annotation is entirely absent.

## Parity catalog integration

Both rows use the milestone-127 macro pattern at `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`:

```rust
// cdx.rs
cdx_anno!(c113_cdx, "mikebom:synthetic-built-in",     component);
cdx_anno!(c114_cdx, "mikebom:built-in-requirement",   component);

// spdx2.rs
spdx23_anno!(c113_spdx23, "mikebom:synthetic-built-in",     component);
spdx23_anno!(c114_spdx23, "mikebom:built-in-requirement",   component);

// spdx3.rs
spdx3_anno!(c113_spdx3, "mikebom:synthetic-built-in",     component);
spdx3_anno!(c114_spdx3, "mikebom:built-in-requirement",   component);
```

Registration in `mikebom-cli/src/parity/extractors/mod.rs` (adjacent to the C110/C111/C112 block):

```rust
ParityExtractor { row_id: "C113", label: "mikebom:synthetic-built-in",     cdx: c113_cdx, spdx23: c113_spdx23, spdx3: c113_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
ParityExtractor { row_id: "C114", label: "mikebom:built-in-requirement",   cdx: c114_cdx, spdx23: c114_spdx23, spdx3: c114_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
```

## Consumer jq recipes

```bash
# List all synthetic built-in gems (CDX)
jq '.components[]
    | select((.properties // [])[] | .name == "mikebom:synthetic-built-in")
    | {name, purl,
       kind: (.properties[] | select(.name == "mikebom:synthetic-built-in") | .value),
       req:  (.properties[] | select(.name == "mikebom:built-in-requirement") | .value // null)}' \
    sbom.cdx.json

# Detect real (non-synthetic) gems only
jq '.components[]
    | select(.purl // "" | startswith("pkg:gem/"))
    | select((.properties // []) | all(.name != "mikebom:synthetic-built-in"))
    | .purl' \
    sbom.cdx.json

# Extract requirement constraints (handling both single-string + multi-source array shapes)
jq -r '.components[]
       | select((.properties // [])[] | .name == "mikebom:synthetic-built-in")
       | .properties[]
       | select(.name == "mikebom:built-in-requirement")
       | (.value | try fromjson catch [.])' \
    sbom.cdx.json
```

## Byte-identity guarantee (SC-003)

For SBOMs whose scanned root does NOT contain a `Gemfile.lock` (i.e., no Ruby gem components emitted):

- C113/C114 MUST NOT appear on any component.

This is the guard that keeps SC-003 achievable: 10 non-`gem` milestone-090 fixtures × 3 formats = 30 goldens remain byte-identical to pre-162.

For SBOMs from a `Gemfile.lock` that references NO built-in gem names (i.e., every dep-target is a real GEM/specs entry):

- C113/C114 MUST NOT appear on any component (allowlist misses trigger no emission).

This is the guard that keeps the milestone-090 `gem` fixture byte-identical if its Gemfile.lock has no built-in refs. Verified during Phase 5.
