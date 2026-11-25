# Quickstart

```sh
cargo test -p waybill --test sbom_edit_filter
bash specs/1657-test-time-first-slice/measurements/edit_identity.sh   # byte-identity, before vs after binary
bash specs/1657-test-time-first-slice/measurements/list_counts.sh     # 145 harness tests in exactly one binary
```
