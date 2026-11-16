# Measurements

**The three `*.cmake` files** are `ROCm/rocm-libraries` at `9811f2ea5235`, the commit TheRock pins in the fixture's `share/therock/therock_manifest.json`, fetched through the GitHub contents API on 2026-11-16:
- `projects/rocsparse/cmake/ClientMatrices.cmake`;
- `projects/rocsparse/cmake/rocsparse_clientmatrices.cmake`;
- `projects/hipsparse/cmake/ClientMatrices.cmake`.

**What was checked** against the fixture listing's `clients/matrices/`, which holds 24 distinct stems:
- rocSPARSE's two files each declare 24 matrices with 24 MD5s, and cover all 24 stems;
- hipSPARSE's declares 19 matrices with 19 MD5s, all among the 24.

The files are kept for reference only, not as test fixtures; tests use synthetic `waybill-fixture-*` names.
