# Quickstart

```sh
cargo build --release -p waybill
# Corpus measurement (FR-001 / FR-009): sources at their pins under <src>/<target>
specs/1664-nix-built-members/measurements/rejected.sh target/release/waybill <src> <work>
# Expect on the branch: hpack-hs-temporal-sdk rejected 0 (was 1); every other row unchanged.

cargo nextest run -p waybill --test nix_built_membership
cargo nextest run -p waybill nix_refs
```
