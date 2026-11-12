# Quickstart: verifying 966

## Hermetic (normal lanes)

```sh
cargo test -p waybill --test nix_closure_boot_versions
cargo test -p waybill --bins haskell_packages::boot_table
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

`nix_closure_boot_versions.rs` covers these cases with a stub `nix` and fixture closures (FR-013):
- a compiler that is known and in the table: the design-tier `base` is versioned, C218 is present, and C170, C171 and C151 are gone;
- a compiler that is known but absent from the table: unchanged, with C219 `absent-from-table`;
- two compilers that ship one library at the same version: one component;
- two compilers that ship different versions: two components, each with edges only from its own compiler's members;
- `Win32` on a non-Windows system: never versioned and never in the boot set;
- `os-string` on 9.6.6: not `compiler-supplied`;
- no closure: byte-identical to main.

## Table check (FR-004)

```sh
cargo test -p waybill --test ghc_boot_libraries_db -- --nocapture
WAYBILL_REQUIRE_GHC_PACKAGE_DB=1 cargo test -p waybill --test ghc_boot_libraries_db
```

Locally this expects ghc-9.6.6 (37) and ghc-9.10.3 (42) to pass. For the mutation check, change one version in the TSV and confirm the test fails, then restore the file with `cp` from a copy taken first, never `git checkout`.

## Regenerating the table

```sh
python3 -I -B scripts/ghc-boot-libraries.py > waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv
git diff --stat
```

## Corpus (Linux, CI)

1. Dispatch `public-corpus.yml` with `regen_goldens=true` on the branch, then review the golden diff with a normalized, sorted comparison.
   - **Expected:** versionless boot `pkg:hackage` components gain versions (SC-001 counts).
   - **Expected:** C218 is present on them, and C219 is present only on closure targets.
   - **Expected:** on openai, `os-string` is resolved.
   - **Not expected:** any change to a golden without `--nix-closure` (SC-004).
2. The run sets `WAYBILL_REQUIRE_GHC_PACKAGE_DB=1`, so the table check runs against the Linux GHCs the IFD targets realise.

## SC-002 (close-out, once)

For each regenerated closure golden:

```sh
osv-scanner scan source --sbom waybill-cli/tests/fixtures/public_corpus/<t>/cdx.json --format json \
  | jq -r '.results[].packages[] | select(.package.name=="base") | .vulnerabilities[].id'
```

Expect `HSEC-2023-0007` for each of the five targets.
