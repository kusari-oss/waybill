# main (6084b343)

## C200 (T001): `main/summary.tsv`, `main/c200.tsv`

- 27 files (9 targets × 3 formats) carry C200.
- Every one of the paths (2 to 104 per file) is absolute, in every format.
- 0 paths are above the root.
- No golden carries C244.

## Split (T002): `split-main/`

| subproject | source_dir (workspace and directory) |
|---|---|
| waybill-fixture-root | `package.yaml` |
| waybill-fixture-a | `local-packages/a/package.yaml` |
| waybill-fixture-c | `local-packages/c/package.yaml` |
| waybill-fixture-d | `local-packages/d/package.yaml` |
| waybill-fixture-d2 | `local-packages/d/waybill-fixture-d2.cabal` |

- `--split=directory` gives 5 sub-SBOMs, the same as `workspace`. `d` and `d2` share a directory but aren't grouped, because `group_key` is `source_dir` (SC-003, measured).
- Not changed by this spec: `split-manifest.json`'s own `scan_root` is the absolute scan path. It's the operator's manifest, not an SBOM.
