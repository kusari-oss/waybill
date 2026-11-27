# Quickstart

```sh
cargo build --release -p waybill
specs/1665-lock-package-source/measurements/measure.sh target/release/waybill branch <src> <work>
# Expect: no_source_file 0 everywhere; the reachability and orphan-reason columns identical to main.
cargo nextest run -p waybill --test lock_package_source
```
