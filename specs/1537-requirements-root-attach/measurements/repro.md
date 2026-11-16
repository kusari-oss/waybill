# T001: repro arms at `main`

**Script:** `measurements/arms.sh <waybill> <dir>`. It builds each arm as a temporary git repository with synthetic `waybill-fixture-*` names, scans it offline in all three formats, and prints the results.

**Binary:** a release build of `283e4c19`. `waybill-cli` and `waybill-common` are byte-identical between `283e4c19` and `1d25955c`, the feature's base (`git diff --stat` empty), so this is `main`'s scanner.

## Root's direct targets (PURLs without qualifiers)

| Arm | Tree | CycloneDX | SPDX 2.3 | SPDX 3 | Verdict | Orphan reasons |
|---|---|---|---|---|---|---|
| c1 (C-1) | `requirements.txt`: `a`, `b` | a, b | a, b | a, b | complete | none |
| c2 (C-2) | c1 plus a submodule `dep` | **dep** | **dep** | dep, a, b | **complete** | none |
| c3 (C-3) | `pyproject.toml` (`app` → `c`) plus `requirements.txt`: `a`, `b`; root = `app` | **c** | **c** | **c** | partial | **none** for `a`, `b` |
| c4 (C-4) | `requirements.txt`: `a`; `tools/requirements.txt`: `t` | a, t | a, t | a, t | complete | none |
| r12 | `pyproject.toml` (`app` → `c`) plus `vcpkg.json` (port) | c | c | c | partial | port = `foreign-build-system` |
| c11 | `requirements.txt`: `a`; `vendor/x/requirements.txt`: `v` | a | *(none)* | a | complete | none |
| c12 | `requirements.txt`: `a`; `requirements-constraints.txt`: `k` | a, k | a, k | a, k | complete | none |

**What each arm shows:**
- **c2** reproduces #1566. Format disagreement plus a false `complete` (research R1).
- **c3** is the same defect with no submodule. The verdict is correct here.
- **c11:** `vendor/x/requirements.txt` is **not read**; only `a` is a component. The SPDX 2.3 gap is not this feature. With a single component, SPDX 2.3 makes that component the document's subject (`DESCRIBES` `waybill-fixture-a`), while CycloneDX and SPDX 3 use a placeholder root that depends on it. That is open issue #1507, root *selection*. Fixtures for this feature therefore keep at least two root-level packages, so I-1 measures root edges and not #1507.
- **c12:** a file named `requirements-constraints.txt` **is read as a declaration** (research R14).
- **r12:** the vcpkg port is orphaned with a reason (`foreign-build-system`), not silently. Filed as #1584.

## Follow-ups filed

- **#1583:** the pip reader does not follow `-r` includes (research R8).
- **#1584:** a root `vcpkg.json` is orphaned (`foreign-build-system`) when the root is another ecosystem's main module (research R12).
