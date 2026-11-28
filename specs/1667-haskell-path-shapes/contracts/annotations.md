# Contracts

## C200 `waybill:hpack-read-outcome` (document scope)

- The keys, their order and their element shapes are unchanged.
- Every path, in `manifests-seen`, `manifests-read`, `manifests-unread[].file`, `manifests-partial[].file`, `generated-cabal-suppressed`, `manifests-dropped-by-scope`, `includes-resolved[].{manifest,file}` and `includes-unresolved[].{manifest,file}`, is relative to the canonical scan root, with `/` separators. A path above the root starts with `..`. No path is absolute.
- The value is parity-equal across CycloneDX, SPDX 2.3 and SPDX 3 (the existing C200 row and extractors, unchanged).

## `--split` `split-manifest.json`

- `source_dir` is a directory relative to the scan root. It is empty for a root-level subproject.
- `--split=directory` groups subprojects with equal `source_dir`.
- Non-Haskell values, file names and groupings are unchanged.
