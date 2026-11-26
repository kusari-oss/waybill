# Quickstart: spec 1658

## Install nextest locally

```sh
brew install cargo-nextest        # or: cargo install cargo-nextest --locked
cargo nextest --version           # 0.9.152 or later
```

## Run the gate

```sh
./scripts/gate.sh \
  --expect '>>> all pre-PR checks passed\.' \
  --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed( \([0-9]+ slow\))?(, [0-9]+ skipped)?$' \
  prepr -- ./scripts/pre-pr.sh
```

**Then check:**
- `GATE_RESULT=pass`;
- the nextest `Summary` line;
- the doctest `test result: ok.` lines.

## Run one test or one binary

```sh
cargo nextest run -p waybill --test sbom_edit_filter
cargo nextest run -E 'test(i1454)'
```

## Reproduce a nextest-only failure

```sh
cargo nextest run -p waybill --test <bin> --no-fail-fast --retries 0 -j <N>
cargo test -p waybill --test <bin>          # control: same test under the old runner
```
