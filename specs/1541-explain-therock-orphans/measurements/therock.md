# TheRock, measured (T013, T014; SC-002 to SC-005, SC-007, FR-009)

Every document is a CI artifact from `kusari-sandbox/waybill-fixture-therock-sdk`, read locally with `probe_i7_on_document`. The probe uses the corpus harness's own I7 functions: nested components are included, and containment counts as reachability. With sibling SPDX documents present, it also compares C45, C104 and C105 across the formats through the parity extractors. TheRock was never scanned locally.

## Built SDK (`measure-full`, base `main` `b503bd6a`)

**Final run: 44193745935, branch `2aecdf6e`.** Medians of three interleaved runs: base 46.0 s, branch 45.3 s, **0.983×**.

| | `main` | branch |
|---|---|---|
| completeness reason (last scan) | 645 not reachable | none (0) |
| components (top-level / nested) | 2,105 / 644 | 2,105 / 644 |
| I7 population | 1 | 1 |
| I7 unexplained (last scan) | 1 (`pkg:generic/gotcha`) | 1 (`pkg:generic/gotcha`) |
| C45, C104, C105 across formats | agree | agree |

**The one SDK component is order-dependent (#1600).** Each side is scanned four times per run (warm-up first), and every scan logs its completeness orphan count:

| run | branch commit | branch scans | `main` scans |
|---|---|---|---|
| 44189121691 | `2dd9c4f4` | 0, 0, 1, 0 | 645 ×4 |
| 44193745935 | `2aecdf6e` | 1, 1, 1, 0 | 645 ×4 |

**What the flip is.** Two `libgotcha.so` files share a name and content hash, and merge into one file component. Which file's identification keeps a container varies with the parallel walker's order (inference, recorded on #1600).
- **When the count is 1,** the top-level `gotcha` is unreachable in every view and carries `unowned-binary`. Completeness = I7 = 1, and the formats agree on C45 (run 44189121691's last scan).
- **When the count is 0,** completeness and the classifier count it reached through a container, while CycloneDX renders it top-level and unreached. That is the only I-A failure on the SDK, about half the scans.

**Decision (maintainer, at implementation):** ship spec 1541 with this recorded; #1600 follows as its own change.

**Everything else on the SDK holds in every scan:**
- the 644 nested components are reachable through containment;
- the completeness reason no longer counts them (645 → 0 or 1);
- no other component is unexplained.

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

**Final run: 44193741030, branch `2aecdf6e`, TheRock `b1cae390`.**

| | `main` | branch |
|---|---|---|
| completeness reason | 846 not reachable | **261** |
| components | 4,348 | 2,123 |
| test-input binaries skipped (C223) | absent | 988 |
| I7 unreachable (incl. file tier) | 2,522 | 1,528 |
| I7 explained | 1,681 | 1,526 |
| **I7 unexplained** | **841** | **2** |
| completeness count = I7 non-file population | — | **yes** (261 = 1,528 − 1,267) |
| C45, C104, C105 across formats | agree | agree (C45: 4 values) |
| median scan, interleaved (3 runs) | 120.6 s | 122.8 s (**1.018×**) |

**The 1,526 explained components at the branch:**
- 1,267 file tier;
- 215 `unowned-binary`;
- 23 `test-data-binary`;
- 12 `dead-lockfile-entry`;
- 5 C151;
- 4 `foreign-build-system`.

**Every component unexplained at `main`, by population (T012):**

| population at `main` | count | at the branch | rule |
|---|---|---|---|
| binaries under #1524's test-input directories | 550 | **skipped**, absent from the document | FR-011 (clarification Q3) |
| binaries under other test-data directories | 22 | `test-data-binary` | FR-004 |
| prebuilt binaries elsewhere | 215 | `unowned-binary` | FR-004 |
| deb `glibc` from kpack test-asset executables (ELF `.note.package`) | 1 | `test-data-binary` | FR-004 (binary-derived predicate) |
| pypi in two uv locks | 35 | reached, or `dead-lockfile-entry` | FR-005: versionless project, lock-local names, forks, optional extras |
| cargo older versions in `mirage/Cargo.lock` | 8 | `dead-lockfile-entry` | FR-002 |
| npm from LLVM tooling and a dashboard lockfile | 4 | `dead-lockfile-entry` | FR-002 |
| vcpkg `zlib`, `zstd` | 2 | `foreign-build-system` | FR-002 |
| conan `gtest`, `benchmark` (`conanfile.py`) | 2 | `foreign-build-system` | FR-007 (`conanfile.py` is a manifest) |
| `pkg:generic/tensile`, a uv workspace root | 1 | **unexplained**: **#1601** | declared defect |
| `pkg:generic/wkmi@1.0.0`, a vendored SDK linked by path | 1 | **unexplained**: **#1602** | declared defect |

**SC-002** holds: every exception is a filed waybill defect. **SC-007** holds at 1.018×, against a bound of 1.5×.
