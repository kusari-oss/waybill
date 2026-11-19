# Wire contract

Every case is asserted in CycloneDX, SPDX 2.3 and SPDX 3 through the parity extractors for C42, C62, C122, C235 and C236. Trees are installed prefixes made of synthetic `waybill-fixture-*` libraries:
- the library file, its symlinks, and `lib/cmake/<pkg>/<pkg>-config-version.cmake`, so spec 1531 makes a record;
- a shipped consumer config file that searches for it.

| Case | Tree | Expected |
|---|---|---|
| I-1 | a shipped library; consumer config searches for it under `if(NOT TARGET waybill_fixture_lib)` | record required, no C122; C236 `["NOT TARGET waybill_fixture_lib"]`; the search's file is in its evidence (FR-001, FR-002, FR-006) |
| I-2 | the same, the search in an `else()` branch | required, C236 `["else: …"]` |
| I-3 | the same, the search unconditional | required, no C236 (as today) |
| I-4 | a shipped library searched only from `share/waybill-fixture/tests/a/CMakeLists.txt` | required (FR-001) |
| I-5 | a record with no library file (version file only) searched only from a tests directory | Test, C62 `source-test-directory`, no C122 (FR-004) |
| I-6 | a record with no library file, searched under `if(NOT GPU_TARGETS)` | Optional, C122 `cmake-conditional`, C236 (as today) |
| I-7 | a record with no library file, searched once from a tests directory and once under `if(WITH_X)` | Optional, C236 `["WITH_X"]` (R4: mixed is Optional) |
| I-8 | a conditional search for a library the tree does not have | the search component is Optional with C236 (FR-003, as today) |
| I-9 | a record that ships only a static archive named by an exported-targets file, searched under `if(NOT TARGET …)` | required (R2: static archives count) |
| I-10 | a shipped library linked only under a condition (C235) and searched under a condition | Optional through the link half, with C235 (FR-005) |
| I-11 | a shipped library under `share/waybill-fixture/tests/lib/`, in an installed tree | Test through spec 1533 (`test-directory`), as today |
| I-ctl | an installed prefix whose record is searched unconditionally and linked | byte-identical to a golden from `main`'s binary (FR-007) |
| I-rep | I-1 to I-11 in one tree, scanned 20 times | identical |
