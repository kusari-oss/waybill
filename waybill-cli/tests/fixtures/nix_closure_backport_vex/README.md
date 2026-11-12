# nix_closure_backport_vex

`closure.json` is `nix derivation show -r` output (format version 4) for a
synthetic closure, served by the stub `nix` in `tests/common/nix_stub.rs`.
It is never evaluated by a real `nix`. Used by `tests/nix_closure_backport_vex.rs`
(#1436).

Every package is a `waybill-fixture-*` name and every CVE id is synthetic
(`CVE-2099-*`). The derivation keys carry 32-character Nix base32 store
hashes, so the `nix_drv` qualifier is built from real-shaped hashes rather
than the fallback for a key without one.

| derivation | patches | what it exercises |
|---|---|---|
| `waybill-fixture-zip 6.0` × 2 | both `CVE-2099-0001` | two variants, both resolve: both hashes in the qualifier, one statement |
| `waybill-fixture-ssh 1.0` | `CVE-2099-0002`, `CVE-2099-0003` | one statement per CVE; the declaration case withholds `-0002` |
| `waybill-fixture-split 2.0` × 2 | one variant `CVE-2099-0004` + `fix-build`, the other `fix-build` only | a CVE only some variants resolve: no statement, C220 = 1 |
| `waybill-fixture-plain 1.0` | `gcc-15` (no CVE) | a patched component without a CVE keeps its PURL |
| `waybill-fixture-zip 6.1` | none | another build of the name, unpatched: never named by a statement |
| `waybill-fixture-app 1.0` | none | the evaluated package, consuming every other member |

Edit by hand: the file is small and its shape is the table above. Output
hashes are the derivation hash with its first character rotated, so each
derivation's outputs are distinct.
