# Interface contracts — milestone 671

Two contract files describe the operator-facing CLI surfaces introduced by m671:

- [file_inventory_mode.md](./file_inventory_mode.md) — extension of `--file-inventory=<value>` enum with the new `source-tree` value
- [source_shape_restriction.md](./source_shape_restriction.md) — new companion flag `--file-inventory-source-shapes=<comma-list>`

Both flags are CLI-only. No API, no SDK, no wire-format changes beyond the emitted C156 annotation (see `../data-model.md`).

## Error posture

- Both flags fail loudly at CLI parse time on invalid values (FR-009).
- The companion flag is only meaningful under `--file-inventory=source-tree`; using it under other modes fails at parse time with a diagnostic naming the accepted combo.
- All errors surface via `clap`'s standard usage-error path (`exit(2)`, stderr diagnostic, no partial scan).

## Emission contract

C156 `waybill:file-inventory-source-shapes-active` doc-scope annotation is emitted iff and only if the new mode is active. See `../data-model.md` §"New annotation (parity catalog)" for the value shape.
