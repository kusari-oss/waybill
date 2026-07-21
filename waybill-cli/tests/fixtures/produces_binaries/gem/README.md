# gem fixtures — `waybill:produces-binaries` (milestone 116 PR-B)

| Sub-fixture | Gemspec shape | Expected |
|---|---|---|
| `with-executables/` | `s.executables = ["baz", "baz-server"]` | `["baz", "baz-server"]` |
| `library-only/` | No `executables` declaration | property OMITTED |
