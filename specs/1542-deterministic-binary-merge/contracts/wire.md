# Contract: repeatable documents

Synthetic `waybill-fixture-*` trees, scanned in the three formats. "Repeatable" means at least 20 scans give one masked document per format, and one graph-completeness reason.

| # | Tree | Expected |
|---|---|---|
| D-1 | two symlink chains ending in different bytes: `lib/a/libwaybill-fixture-g.so` → `.so.3` → `.so.3.0.0` and `lib/b/libwaybill-fixture-g.so` → `.so.2` → `.so.2.4.2`, each naming one bundled tree | repeatable; two `libwaybill-fixture-g.so` file components, one per hash; every bundled tree nested under its own file; nothing top-level except file components; no orphan |
| D-2 | two regular files with one name and the **same** bytes, in different directories, naming one bundled tree | repeatable; **one** file component carrying both paths; the bundled tree nested under it |
| D-3 | two regular files with one name and different bytes (no symlinks) | as D-1: two file components |
| D-4 | D-1 plus an npm app, so the root fallback is closed | repeatable; the binaries are unreachable and coded `unowned-binary`, the same in every scan; I-A holds in every scan |
| D-5 | one bundled tree named by two different binaries | the three formats report the same components for it, and agree on C45, C104 and C105 |
| D-6 | a `deb` package database with one package at two `?arch=` values | merged as today (milestone 1064); the survivor is the same in every scan |

## Invariants on every case

- **I-A (spec 1541):** completeness count = I7 population; no reachable component carries C45; the formats agree.
- **I-D:** no identification is top-level when its file's component exists under another hash, which would be a detached identification.
