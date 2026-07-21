# npm fixtures — `waybill:produces-binaries` (milestone 116 PR-B)

| Sub-fixture | Manifest shape | Expected `waybill:produces-binaries` |
|---|---|---|
| `string-form/` | `{"name":"fixture-baz","version":"1.0.0","bin":"./bin/cli.js"}` | `["fixture-baz"]` |
| `object-form/` | `{"name":"fixture-baz","version":"1.0.0","bin":{"baz":"./cli.js","baz-init":"./init.js"}}` | `["baz", "baz-init"]` |
| `library-only/` | `{"name":"fixture-libonly","version":"1.0.0"}` (no `bin`) | property OMITTED |
