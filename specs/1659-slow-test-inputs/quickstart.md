# Quickstart: spec 1659

```sh
# the combined self-scan guards
cargo nextest run -p waybill --test self_scan_guards
# the OpenVEX-reference conformance check, now carried by us4
cargo nextest run -p waybill --test closure_runtime_scope -E 'test(us4_drop_scope_excluded)'
# i1454 without the moat validator
cargo nextest run -p waybill --test sbom_edit_filter -E 'test(i1454)'
```
