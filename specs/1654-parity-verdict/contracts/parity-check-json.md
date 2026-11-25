# Contract: `waybill sbom parity-check --format json` after spec 1654

Each element of `rows[]` gains one key. Nothing else changes.

```json
{
  "row_id": "C247",
  "label": "`waybill:boot-library-dependencies`",
  "section": "C",
  "directional": "symmetric_equal",
  "cdx":    {"kind": "present", "value_count": 4},
  "spdx23": {"kind": "present", "value_count": 4},
  "spdx3":  {"kind": "absent"},
  "universal_parity": true,
  "verdict": "fail"
}
```

- **`verdict`:** one of `pass`, `fail` or `not-checked` (data-model.md).
- **`universal_parity`:** unchanged. It still means the catalog expects the row in every format.
- **`summary`:** unchanged. `parity_gaps` equals the number of rows with `verdict: "fail"`.
- **The human-readable table (`--format table`):** unchanged.
