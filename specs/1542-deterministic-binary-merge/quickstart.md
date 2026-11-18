# Quickstart

**Reproduce the old variation.** The trace's probe is `measurements/probe_repro.rs`. Copy it into `waybill-cli/tests/` and run:

```sh
cargo test -p waybill --test probe_repro -- --ignored --nocapture
```

It prints each scan's completeness reason and coded components. At `main` `37de8e08`, one orphan appeared in 4 of 12 scans.

**Repeatability tests:**

```sh
cargo test -p waybill --test scan_determinism
```

**Corpus audit**, in CI only: dispatch `public-corpus.yml` with `-f repeat=2` at the branch and at `main`.

**TheRock**, in CI only: `measure-full` keeps every scan's documents. Probe each, and compare them masked.
