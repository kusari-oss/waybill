# Contract: the boot-library table and its probe

## File

`waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv`, embedded with `include_str!`.

```
# observed: 2026-11-12
# sources: https://downloads.haskell.org/ghc/<v>/docs/users_guide/<v>-notes.html#included-libraries
#          https://raw.githubusercontent.com/commercialhaskell/stackage-content/<sha>/stack/global-hints.yaml
#          https://github.com/ghc/ghc/tree/ghc-<v>-release (overrides only)
# regenerate: python3 -I -B scripts/ghc-boot-libraries.py > <this file>
ghc	library	version	source	platform
9.10.3	base	4.20.2.0	release-notes	all
9.10.3	ghc-internal	9.1003.0	stack-global-hints	all
9.10.3	Win32	2.14.1.0	release-notes	windows
9.12.3	process	1.6.26.1	ghc-release-tag	all
```

The example rows show the shape only; the probe produces the real values.

- Tab-separated, LF line endings.
- Sorted by `(ghc as a version, library as bytes)`.
- `#` lines are comments, and the header row is literal.

## Probe

`scripts/ghc-boot-libraries.py`. Run with `python3 -I -B`, stdlib only (`urllib`, `html.parser`, `re`).

1. Read the release list from Stack's `global-hints.yaml`, pinned to a stackage-content commit recorded in the header. Keep every `ghc-X.Y.Z` with X.Y ≥ 9.0.
2. For each release, fetch the release notes and parse the "Included libraries" table. A 404, or a page with no table, means none.
3. For each library: take the release notes' version if present (`release-notes`), else Stack's (`stack-global-hints`).
4. Apply `OVERRIDES`. This is a literal list in the probe, each entry citing the tag and the `.cabal` it was read from (`ghc-release-tag`). The probe **fails** if the notes and Stack disagree on an entry that has no override, so a new disagreement can't land silently.
5. Mark `Win32` `windows`, and every other library `all`.

Its output is committed. A diff is reviewed like a golden. The probe keeps no copies of the downloaded data in the repository; those live in a temporary directory.

## Verification (FR-004)

`ghc_boot_libraries_match_every_available_package_db`, an integration test:
- It globs `/nix/store/*-ghc-*/lib/ghc-*/lib/package.conf.d` and `$WAYBILL_GHC_PACKAGE_DB_PATHS`.
- For each database, every `.conf` `name`/`version` must equal the table, every `all` row for that release must be present, and no `windows` row may appear unless the store path is a Windows GHC.
- It prints each database checked, with counts.
- With zero databases it prints `no GHC package database found` and fails iff `WAYBILL_REQUIRE_GHC_PACKAGE_DB=1`.
