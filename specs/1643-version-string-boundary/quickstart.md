# Quickstart: spec 1643

## Reproduce the defect (R1, local, real libraries)

```sh
cargo build --release -p waybill
specs/1643-version-string-boundary/measurements/probe.sh target/release/waybill \
  /nix/store/<hash>-zlib-1.3.2/lib/libz.1.3.2.dylib \
  /nix/store/<hash>-ncurses-6.6/lib/libncursesw.6.dylib \
  "$TMPDIR/probe"
```

On `main`: zlib is versioned only for byte `00`; ncurses is found only for byte `00`. After this spec: `00`, `2d` and `ff` are versioned; `78` (`x`) is unchanged.

## Run the tests

```sh
cargo test -p waybill --test version_string_boundary
cargo test -p waybill version_strings
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## Census (CI only)

1. Commit the log-only census (events only, output unchanged) and push it.
2. Dispatch in the TheRock SDK fixture repository:
   ```sh
   gh workflow run census-versions.yml -R kusari-sandbox/waybill-fixture-therock-sdk \
     --ref main -f waybill_ref=<full sha>
   ```
   Verify every job's `refs.txt` names that SHA.
3. Dispatch `public-corpus.yml` with `regen_goldens=true` at the same SHA. Every golden must be byte-identical.
4. Read each artifact's `versions-<target>.json` and write the per-pattern counts and the judged new matches into `measurements/census.md`.

## Measure (CI only)

```sh
gh workflow run measure-full.yml -R kusari-sandbox/waybill-fixture-therock-sdk \
  --ref main -f waybill_ref=<branch full sha> -f base_ref=<main full sha>
```

Report the ratio of median scan times and trace every component difference.
