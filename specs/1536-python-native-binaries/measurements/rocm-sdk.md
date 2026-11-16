# ROCm SDK fixture: unchanged (T020, SC-003)

`measure.yml` in `kusari-sandbox/waybill-fixture-therock-sdk`, run 43890493631, scans the pinned ROCm SDK tree at two refs:
- base `main` = `1ae157d5`;
- branch `1536-python-native-binaries` = `c690b8d8`.

The tree has no `.dist-info`, so nothing in it is `RECORD`-owned.

- Raw CycloneDX: 504,557 bytes each. `diff` of the key-sorted documents shows exactly two differing lines per side, `serialNumber` and `metadata.timestamp`.
- With those two fields, `metadata.tools` and annotators removed and keys sorted, the documents are **byte-identical** (`cmp`).
- 314 components with a PURL on each side.
