# TheRock, measured (T013, T014; SC-002 to SC-005, SC-007, FR-009)

Every document is a CI artifact from `kusari-sandbox/waybill-fixture-therock-sdk`, read locally with `probe_i7_on_document`. The probe uses the corpus harness's own I7 functions: nested components are included, and containment counts as reachability. With sibling SPDX documents present, it also compares C45, C104 and C105 across the formats through the parity extractors. TheRock was never scanned locally.

## Built SDK (`measure-full` 44189121691; branch `2dd9c4f4`, base `main` `b503bd6a`)

| | `main` | branch |
|---|---|---|
| completeness reason | 645 not reachable | **1** not reachable |
| components (top-level / nested) | 2,105 / 644 | 2,105 / 644 |
| I7 population | 1 | 1 |
| I7 unexplained | **1** (`pkg:generic/gotcha`) | **0** (`gotcha` carries `unowned-binary`) |
| completeness count = I7 population | no (645 ≠ 1) | **yes** (1 = 1) |
| median scan, interleaved (3 runs) | 39.3 s | 34.8 s (**0.884×**) |

**Formats (FR-009):**
- C104 and C105 agree in all three formats.
- C45 agrees in CycloneDX and SPDX 3. **SPDX 2.3 does not.** It keys packages by PURL, and folds `gotcha`'s three components into one package reached by `CONTAINS`, which has nothing to carry the marker.
- The top-level copy exists because two binaries merged by name and hash lose one binary's nested identification. That is a binary-reader defect present on `main`, filed as **#1600**.

*The `2dd9c4f4` build predates `adf230fc`, which changes only the uv reader. The SDK has no `uv.lock`.*

## Source tree (`measure-source`)

**First run: 44184375014, branch `064e264f`.**

| | `main` | branch |
|---|---|---|
| completeness reason | 846 not reachable | 277 |
| components | 4,348 top-level | 3,389 top-level |
| test-input binaries skipped (C223) | absent | 988 |
| I7 unexplained | **841** | **14** |
| median scan (interleaved, 3 runs) | 129.1 s | 135.6 s (**1.051×**) |

**At the branch the 1,530 explained components are:**
- 1,267 file tier;
- 215 `unowned-binary`;
- 22 `test-data-binary`;
- 19 `dead-lockfile-entry`;
- 5 C151;
- 2 `foreign-build-system`.

**The 14 left were traced (T012):**
- **9 pypi** come from bare dependency names resolving across the whole scan and from an extra requested only through the project's own extra. Fixed in `adf230fc`.
- **conan (2), deb (1), `tensile` and `wkmi`:** traced in the final run below.

**Final run, on `adf230fc`:** pending.
