# #1412 — closure members with a nixpkgs license row but no license

Outcome: **no defect.** Every gap is accounted for, and no member with a usable
row is emitted unlicensed.

nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`, `aarch64-darwin`, Determinate
Nix 3.20.0 (nix 2.34.6), the five milestone-1377 wrapper flakes, waybill built
from `be8924f7`, 2026-11-10.

## Method

- `prepare.sh` — per flake: `nix derivation show -r`, `waybill --offline debug
  closure-dump` (the population, `in_coverage_population`), and `waybill sbom
  scan --nix-closure --no-deep-hash` (what is emitted, and C184).
- `account.py` — evaluates the production expression **lifted verbatim** from
  `eval/licenses.rs` at run time (`LICENSE_PROJECTION`, `licenses_expr`,
  `ROOTS`) over production's names (every derivation's `pname()`), joins rows
  to members by `.drv` basename across all rows as `closure/licenses.rs::index`
  does, and reads each member's emitted license from the document.
- `null-licenses.sh` — confirms the null rows are packages that declare no
  `meta.license`.

The flakes were evaluated as `path:` copies outside the repository; their
`flake.lock` hashes were unchanged afterwards.

## Result

| target | derivations with a row (the #1404 probe's count) | outside C184's population | in population, row declares no license | in population, usable row | emitted licensed with the row's text | C184 `received` | `withheld-ambiguous` |
|---|---|---|---|---|---|---|---|
| curl | 84 | 1 | 1 | 82 | 82 | 82 | 0 |
| gh | 92 | 1 | 2 | 89 | 89 | 89 | 0 |
| httpie | 558 | 1 | 3 | 554 | 554 | 554 | 0 |
| openssh | 81 | 1 | 2 | 78 | 78 | 78 | 0 |
| ripgrep | 96 | 1 | 2 | 93 | 93 | 93 | 0 |

The "84 with a row" in the issue is the #1404 probe's population
(`specs/1404-nix-compound-licenses/measurements/classify.py`), which joins over
**every** closure derivation. C184 counts over `emit::coverage_drv_keys`. The
two differ by exactly one derivation on every target:

- `expand-response-params` (`2lk64f86…-expand-response-params.drv`). It
  declares no `pname`, so it is a milestone-1282 member with role `fragment`,
  which the coverage population excludes. Its row (`MIT`) does join by store
  path, and the component **is emitted with `MIT`**. It is licensed; it is just
  not counted.

The null rows are packages that declare no `meta.license` at all
(`null-licenses.sh`: `hasLicense = false`):

- `apple-sdk` 14.4 (all five), `getopt` 1.1.6 (gh, httpie, ripgrep),
  `libunwind` 14.4 (httpie), `libredirect` 0 (openssh).

So `rows = received + null rows + 1` on every target, FR-015 withheld nothing,
`declared_license::resolve` rejected nothing, and no member was merged into
another reader's component (the flakes contain no other manifests).

## Reproduce

```sh
cargo build --release -p waybill
export WAYBILL=$PWD/target/release/waybill WORK=$(mktemp -d)
bash specs/1412-closure-license-gap/prepare.sh
cd $WORK/out/curl && DUMP=dump.json SBOM=sbom.cdx.json SRC=<repo> ROWS_CACHE=rows.json \
  python3 -I -B <repo>/specs/1412-closure-license-gap/account.py
```
