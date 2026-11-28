# branch (7a99bd9d), corpus run 46113124978

## C200: `branch/summary.tsv`, `branch/c200.tsv`

- The same 27 files carry C200 (9 targets × 3 formats).
- Every path is now relative: 0 absolute, 0 above the root, 0 `/home/runner` occurrences anywhere in the regenerated corpus.
- **Every difference explained:** after removing the host prefix (`/home/runner/.cache/waybill/corpus/<id>/<pin>/{repo,prepared-no-generated-cabal}/`), `main`'s 1,176 C200 paths are byte-identical to the branch's, same keys and same order.
- **Files that differ:** exactly the 27 that carry C200. With C200 masked out, 26 are identical to `main`; see `normalized.txt` for the 27th.
- **The 27th file**, `hpack-hs-temporal-sdk/spdx-3.json`, also gains the #1738 stray `Organization "Microsoft"` element. The committed golden is `main`'s file with only its C200 line replaced, so it differs from `main` by that one line.

## Split fixture: `split-branch/` against `split-main/`

| subproject | main | branch |
|---|---|---|
| waybill-fixture-root | `package.yaml` | `` |
| waybill-fixture-a | `local-packages/a/package.yaml` | `local-packages/a` |
| waybill-fixture-c | `local-packages/c/package.yaml` | `local-packages/c` |
| waybill-fixture-d | `local-packages/d/package.yaml` | `local-packages/d` |
| waybill-fixture-d2 | `local-packages/d/waybill-fixture-d2.cabal` | `local-packages/d` |

- Under `--split=directory`, `d` and `d2` now form one sub-SBOM, `local-packages-d.multi`, so the fixture gives 4 sub-SBOMs where `main` gave 5.
- The two effects account for all 19 differing lines in `source_dirs.txt`.
- The non-Haskell `split_*` and `project_discovery_*` suites pass unchanged (329 tests, T011).
