Stub-`nix` fixtures for `tests/closure_declared_cpe.rs` (milestone 1377 US2). Never evaluated by a real nix.

`zlib` and `openssl` are real names on purpose: the curated CPE table is keyed on the component name, so a synthetic name cannot exercise it. Their version, `9.9.9`, is synthetic. Every declared CPE uses the vendor `waybill-fixture-vendor`.

| member | row | expected |
| --- | --- | --- |
| root `waybill-fixture-cpeapp 1.0` | declared, same drv | subject carries it |
| `zlib 9.9.9` | declared, same drv, differs from curated | declared replaces curated |
| `waybill-fixture-shell 5.3p9` | declared on a different drv, same name+version | reached by name+version |
| `waybill-fixture-plain 2.0` | `cpe: null` | received, no CPE |
| `openssl 9.9.9` | `cpe: null` | keeps the curated CPE |
| `waybill-fixture-badcpe 1.0` | empty vendor | no CPE, counted as rejected |
| `waybill-fixture-skew 3.0` | declared version `2.9` | emitted exactly as declared |
