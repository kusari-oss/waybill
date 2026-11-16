# Corpus goldens (T013, SC-003)

`public-corpus.yml` with `regen_goldens=true`: run 43802092723.
- **Scanned commit:** `e66b3add9a879dadef4e983bee03718862d1a7d4`, the branch head. The workflow definition came from `1e5c53d1`, `main`.
- **Comparison:** all 66 regenerated files were byte-compared (`cmp`) with the committed goldens. **0 differ.**

As predicted (research R2, plan step 6): no public-corpus target is a CMake project, and no repository fixture contains `file(DOWNLOAD)`.
