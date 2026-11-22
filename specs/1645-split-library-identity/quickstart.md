# Quickstart: spec 1645

## See the split on `main`, and the fix

```sh
d=$(mktemp -d); mkdir -p $d/lib/pkgconfig
# a minimal ELF shared library whose .rodata holds "\0ncurses 6.6.20251230\0"
# (the tests build it with tests/common/installed_lib.rs::elf_shared_lib)
cat > $d/lib/pkgconfig/ncursesw.pc <<'EOF'
prefix=/usr
libdir=${prefix}/lib
Name: ncursesw
Description: synthetic
Version: 6.6.20251230
Libs: -L${libdir} -lncursesw
EOF
waybill --offline sbom scan --path $d --format cyclonedx-json --output $d/out.cdx.json
jq -r '[.components[]? | recurse(.components[]?)] | .[] | select(.name|test("ncurses")) | .purl' $d/out.cdx.json
```

- `main`: `pkg:generic/ncurses@6.6` and `pkg:generic/ncursesw@6.6.20251230`.
- Spec 1645: `pkg:generic/ncurses@6.6.20251230` only.

## Census

```sh
RUST_LOG=waybill::version_census=debug waybill --offline sbom scan --path $d … 2>&1 | grep -E 'shared_file=|curated_version_change=|cpe_form_change='
```

## Tests

```sh
cargo test -p waybill --test library_identity
cargo test -p waybill --bins scan_fs::binary::version_strings::tests::
cargo test -p waybill --bins generate::cpe::tests::
```

The module's unit tests are in the bin target. A filter over `--lib` runs 0 tests.

## Gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```
