# Contract: CLI surface

## `--root-cpe <CPE>` (repeatable)

`waybill sbom scan` accepts this flag.

- **Order.** The first value is the subject's primary CPE, and later values are its secondary CPEs (FR-001).
- **Validation.** Each value is checked with `Cpe::parse`. A value that fails is a usage error raised before scanning: exit non-zero and write no document (FR-006). The message names the flag, the value and the parse error.
- **Conflicts with** `--split` (research R8).
- **Effect.** The subject's CPE list becomes exactly these values (FR-002).

## `--cpe-map <PATH>` (single)

- **The file.** A local TOML file in the schema of `data-model.md` § CpeTable. It is never fetched (FR-015).
- **Errors.** A read, parse or validation failure is a usage error raised before scanning. The message names the file, the row index and key, and the reason.
- **Matching.** A row matches a component when the component's PURL type, namespace and name equal the key's. Each pair builds `cpe:2.3:a:<vendor>:<product>:<component version>:*:*:*:*:*:*:*` with the existing escaping. A component with an empty version gets nothing.
- **Logging.** At the end of the scan, one INFO line reports the row count, the rows that matched, the components changed, and the rows that matched nothing.

## `--supplement-cdx` (changed)

- **Collisions.** For a component the scanner also found, valid supplement CPEs now replace the scanner's (FR-003). The `waybill:assertion-conflict` record for `cpe` is kept, and its justification becomes developer-wins.
- **`waybill:declared-cpe`** is no longer written for CPE conflicts.
- **Malformed CPEs** are still dropped with a WARN (#1403).
- **The supplement's `metadata.component.cpe`** is ignored, as all of `metadata.component` is (m119 FR-014). A WARN now says so and points at `--root-cpe`.

## With no new flags and no supplement

The output is byte-identical to the branch base (FR-016).
