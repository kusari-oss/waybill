# T024, T025: corpora (SC-004)

## Public corpus

Run 44943170410 of `public-corpus.yml`, dispatched `--ref main -f branch=1640-strip-url-credentials -f regen_goldens=true`. Jobs:
- `Run public corpus (1640-strip-url-credentials)`: success;
- `Report corpus failure`: skipped (its condition is `failure() && github.event_name == 'schedule'`).

The run's summary and its log ("Resolve the scanned commit", `SCANNED_SHA`) both name the scanned commit as 47aef52ae7a5ac6ca482ceb233f2ed213e70189f. That is this branch's HEAD at dispatch; its source tree is the gate's.

The `corpus-goldens-regen` artifact holds 67 files for 21 targets. Each was compared with `cmp` against its committed golden: **67 identical, 0 changed, 0 missing.**

As a control, 3 of the 67 files carry `vcs_url=`, `download_url=` or `repository_url=` qualifiers, and C55 appears in 18. So URL-bearing fields are present, and they come out unchanged. No committed golden has a URL with userinfo (measured, spec Background), so FR-006's byte-identity holds.

## Quality corpus

Runs of `quality-corpus.yml`, both dispatched from `main`'s workflow definition:
- **branch:** 44943172766, `-f branch=1640-strip-url-credentials`, artifact `quality-corpus-47aef52a…`;
- **main:** 44943174923, `-f branch=main`, artifact `quality-corpus-86bce894…`.

Each has two jobs: `Measure SBOM quality corpus` succeeded and `Report quality-corpus failure` was skipped.

The two `run-*.json` reports were flattened and compared key by key, leaving out timing, SHA and RSS keys:
- **223 keys over 18 targets: 0 differ.**
- `sbom_bytes` is identical on every target, for example cpp-mongo 1,113,103, python-pytorch 605,529 and ruby-rails 1,254,955.
