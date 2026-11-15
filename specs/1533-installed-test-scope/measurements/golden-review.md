# Corpus goldens (T021, SC-005)

`public-corpus.yml` was dispatched with `regen_goldens=true` against `1533-installed-test-scope`: run 43772334404.
- **Scanned commit:** `0c06a3550be8e6f732f16ab8049a2cf23671d300`, as the run summary's "Scanned commit" line reports. The workflow definition came from `1cb56c56`, `main`.
- **Targets regenerated:** all 21.
- **Comparison:** every file in the `corpus-goldens-regen` artifact was byte-compared (`cmp`) with the committed golden at the same path. 66 compared, **0 differing**. The run summary agrees: "every regenerated golden is byte-identical to the committed one".

This matches research R5's prediction:
- no source-tree target is recognised as an installed tree, and none ships an installed CTest file or a test package's exported targets;
- `image-postgres16`, the one image, is unchanged. Its files under test directories are dpkg-owned, so they are excluded by FR-013 or have no component.
