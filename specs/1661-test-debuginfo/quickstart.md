# Quickstart: spec 1661

```sh
# On macOS, .cargo/config.toml packs debug info, so rebuilds stop leaving *.rcgu.o files behind.
ls target/debug/deps | wc -l        # stays flat across rebuilds
cargo nextest run -p waybill --test backtrace_resolves_source   # file:line positive control
# A one-time cleanup of a directory that grew before this change:
cargo clean
```
