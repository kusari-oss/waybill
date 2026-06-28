# Contract — `mikebom:peer-edge-targets` annotation

Phase 1 output. Defines the wire-shape contract for the new annotation across CDX 1.6, SPDX 2.3, and SPDX 3 outputs.

## Annotation contract

For every `PackageDbEntry` produced by the npm `package_lock.rs` reader where at least one `peerDependencies` declaration resolved to a real installed package AND that name is NOT also declared in a regular section (`dependencies` / `devDependencies` / `optionalDependencies`), the entry's `extra_annotations` map MUST contain:

```
Key:   "mikebom:peer-edge-targets"
Value: serde_json::Value::Array([
           serde_json::Value::String("pkg:npm/<peer1-name>@<peer1-version>"),
           serde_json::Value::String("pkg:npm/<peer2-name>@<peer2-version>"),
           ...
       ])
```

Where the array is sorted alphabetically (lex-ascending on the PURL string).

## Wire shape per format

### CDX 1.6

Emitted as a `properties[]` entry on the source component (the entry that owns the peer-driven edges):

```json
{
  "purl": "pkg:npm/@react-native-async-storage/async-storage@1.24.0",
  "dependsOn": [...],  // INCLUDES the peer edges (e.g., pkg:npm/react-native@0.85.3) per FR-001
  "properties": [
    {
      "name": "mikebom:peer-edge-targets",
      "value": "[\"pkg:npm/react-native@0.85.3\"]"
    }
  ]
}
```

The CDX builder at `cyclonedx/builder.rs:1086-1098` stringifies non-`Value::String` annotation values via `serde_json::to_string(other)` — so the array gets JSON-string-encoded for CDX's `xs:string`-typed `properties[].value` slot. The wire bytes match the milestone-145 pattern for `mikebom:source-files-nested-url` (compound JSON value stringified onto a CDX `xs:string` property).

### SPDX 2.3

Emitted as an `annotations[]` entry on the source component's package, with the value carried natively (not stringified) inside the envelope:

```json
{
  "name": "@react-native-async-storage/async-storage",
  ...
  "annotations": [
    {
      "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:peer-edge-targets\",\"value\":[\"pkg:npm/react-native@0.85.3\"]}",
      ...
    }
  ]
}
```

The SPDX 2.3 emitter at `spdx/annotations.rs:371` passes the `serde_json::Value::Array` through the envelope verbatim (post-milestone-145, native array shape).

### SPDX 3

Same envelope shape on the `Annotation` element with the value carried natively:

```json
{
  "type": "Annotation",
  "subject": "https://.../pkg-async-storage-...",
  "statement": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:peer-edge-targets\",\"value\":[\"pkg:npm/react-native@0.85.3\"]}",
  ...
}
```

## Cross-format invariance

A new parity-catalog row MUST be added (next available C-number, likely C97):

```rust
ParityExtractor {
    row_id: "C97",  // verify next available number
    label: "mikebom:peer-edge-targets",
    cdx: c97_cdx,
    spdx23: c97_spdx23,
    spdx3: c97_spdx3,
    directional: Directionality::SymmetricEqual,
    order_sensitive: false,
},
```

`order_sensitive: false` is safe because the array IS always sorted alphabetically before stamping (per `BTreeSet` collection order), so cross-format byte-equality is guaranteed.

## Emission decision tree

```
For each PackageDbEntry produced by parse_package_lock:
  peer_edge_targets = empty BTreeSet<String>

  For each name in entry.peerDependencies (if present):
    If name in depends_set (already added via a regular section):
      Skip (FR-003 — regular wins)
    Else if resolve_dep_via_node_modules_walk(...) returns Some(version):
      Add "<name> <version>" to depends_set         (the edge emission)
      Add "pkg:npm/<name>@<version>" to peer_edge_targets   (the annotation entry)
    Else:
      Skip — unmet peer per FR-002 (no edge, no annotation)

  If peer_edge_targets is non-empty:
    Stamp extra_annotations["mikebom:peer-edge-targets"] = Value::Array(sorted)
  Else:
    Omit the key entirely (FR-005)
```

## Negative-space contract (what MUST NOT happen)

- The annotation MUST NOT appear when the set would be empty (FR-005). Specifically:
  - Components with no `peerDependencies` declarations.
  - Components whose ALL peers are unmet (not in `packages` map).
  - Components whose ALL peers are also declared in regular sections (FR-003 precedence).
- The annotation MUST NOT contain "<name> <version>" form (that's the internal `depends_set` value shape). It MUST contain canonical PURL strings.
- The annotation MUST NOT include peer targets that resolved to edges via the regular path (only the peer-only targets — those EXCLUSIVELY declared via `peerDependencies` AND not in regular sections).
- The annotation MUST NOT contain duplicate PURL strings (BTreeSet guarantees uniqueness).

## Test surface (covers spec SC-002 + SC-003 + SC-004 + SC-005)

| Test | Asserts |
|---|---|
| `peer_dependencies_emit_edges_and_annotation_md147` | FR-001 + FR-004 + SC-003 (replaces pre-147 skip test) |
| `peer_already_in_regular_deps_takes_precedence_md147` | FR-003 + SC-004 |
| `unmet_peer_emits_no_edge_md147` | FR-002 + SC-005 |
| `peer_annotation_omitted_when_set_empty_md147` | FR-005 |
| `peer_edge_targets_array_is_sorted_alphabetically_md147` | research §A sort precedent |
| New parity-catalog row C97 cross-format byte-equality | SC-002 |
