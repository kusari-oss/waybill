# Data model: the same build gives the same answer, scan after scan

Everything is in process for one scan. Nothing new reaches the wire.

## Dedup group key (`resolve/deduplicator.rs`)

`(ecosystem, name, version, parent_purl, pants namespace, content hash)`

`content hash` is the PURL's `file-sha256` qualifier, or `None`. Only components that name one file's bytes carry it: a binary's file-level component.

**Survivor:** highest confidence; among equals, the reader's order (precedence, unchanged). Merged `source_file_paths` and `source_connection_ids` are sorted.

**Output order:** sorted by `(purl, parent_purl, content hash, first source path)`. Before this feature it was sorted by PURL alone, leaving ties among same-PURL components in `HashMap` order.

## Containment edges (`generate/graph_completeness/mod.rs`)

`add_containment_edges` takes `dependency_edges = edges.clone()` before its loop. Rule R2 reads `dependency_edges`; rules R1 and R3 write `edges`. Reachability is then a function of the component set and relationships, not their order.

## Masked document (tests and the audit)

An emitted document with these fields replaced by fixed values:
- CycloneDX: `serialNumber` and `metadata.timestamp`;
- SPDX 2.3: `documentNamespace` and `creationInfo.created`;
- SPDX 3: the creation-info `created` field and document IRIs;
- anything else the corpus golden comparison already masks.
